// SPDX-License-Identifier: GPL-2.0-or-later
//! Offline restore planning from pinned settings_backup.py/settings_workflow.py.
//! No transport or execution API. A successful plan grants no write permission.
use crate::ak820_settings_backup::{
    BackupResult, ImageIdentity, ValidatedArchive, canonical, macro_chunks, rgb_wire,
    rgb_wire_for_profile, sha256, transition_allowed, translate_rgb,
};
use crate::ak820_settings_model::{settings_from_nvm, validate_nvm};
use crate::ak820_typed_storage::{Extensions, QmkSettings, SCHEMA, slot_offset};
use crate::app::ak820_beta1_profiles::{
    KeycodeSource, ProfileId, firmware_matrix, historical_settings_identities,
    historical_settings_identity, matrix_profile,
};
use serde_json::{Value, json};

/// Requirements for a future executor, never evidence that they were fulfilled.
/// Owned values are exposed only by shared reference, including on every setter.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RestorePrecondition {
    PinnedSource {
        archive_manifest_sha256: String,
        identity: ImageIdentity,
        nvm: Vec<u8>,
    },
    SelectedTarget {
        profile: ProfileId,
        identity: ImageIdentity,
        nvm: Vec<u8>,
    },
    RevalidateSourceBeforeFirstSetter,
    FreshPhysicalUnlockAfterPlanning {
        reject_preexisting_unlock: bool,
    },
    FreshUnlockedStatusBeforeEachSetter,
    MatchingAckAndReadbackBeforeNextSetter,
    StopOnTimeoutStorageFaultDisconnectOrMismatch,
    DiscoveredComboCount {
        expected: usize,
    },
    QueriedActiveQsid {
        expected: Vec<u16>,
    },
    FinalGetterReconciliation,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReadExpectation {
    Getter {
        command: &'static str,
        arguments: Value,
        expected: Value,
    },
    NvmRange {
        offset: usize,
        expected: Vec<u8>,
    },
    FullNvm(Vec<u8>),
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RestoreStep {
    command: &'static str,
    arguments: Value,
    expectations: Vec<ReadExpectation>,
    preconditions: Vec<RestorePrecondition>,
}
impl RestoreStep {
    pub(crate) fn preconditions(&self) -> &[RestorePrecondition] {
        &self.preconditions
    }
    pub(crate) fn command(&self) -> &'static str {
        self.command
    }
    pub(crate) fn arguments(&self) -> &Value {
        &self.arguments
    }
    pub(crate) fn expectations(&self) -> &[ReadExpectation] {
        &self.expectations
    }
}
/// Owned snapshots and steps have read-only accessors. Each setter requires a
/// matching ACK and a fresh unlocked status before it. Any failure must stop;
/// no retries, reset, rollback, command replay or physical I/O are modeled here.
#[derive(Debug)]
pub(crate) struct RestorePlan {
    source: ImageIdentity,
    target: ImageIdentity,
    before: Vec<u8>,
    expected: Vec<u8>,
    steps: Vec<RestoreStep>,
    preconditions: Vec<RestorePrecondition>,
    final_getters: Value,
    final_extensions: Option<Extensions>,
}
impl RestorePlan {
    pub(crate) fn preconditions(&self) -> &[RestorePrecondition] {
        &self.preconditions
    }
    /// Re-query all supported settings, including unchanged fields, at completion.
    pub(crate) fn final_getters(&self) -> &Value {
        &self.final_getters
    }
    pub(crate) fn final_extensions(&self) -> Option<&Extensions> {
        self.final_extensions.as_ref()
    }

    pub(crate) fn source(&self) -> &ImageIdentity {
        &self.source
    }
    pub(crate) fn target(&self) -> &ImageIdentity {
        &self.target
    }
    pub(crate) fn before_nvm(&self) -> &[u8] {
        &self.before
    }
    pub(crate) fn expected_nvm(&self) -> &[u8] {
        &self.expected
    }
    pub(crate) fn steps(&self) -> &[RestoreStep] {
        &self.steps
    }
    pub(crate) fn final_readback(&self) -> ReadExpectation {
        ReadExpectation::FullNvm(self.expected.clone())
    }
    pub(crate) fn requires_fresh_physical_unlock(&self) -> bool {
        true
    }
}
fn require(ok: bool, message: &str) -> BackupResult<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn hex_bytes(value: &Value) -> BackupResult<Vec<u8>> {
    let s = value.as_str().ok_or("Expected hexadecimal string")?;
    require(s.len() % 2 == 0 && s.is_ascii(), "Invalid hexadecimal data")?;
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Invalid hexadecimal data".into())
        })
        .collect()
}
fn identity(profile: ProfileId) -> BackupResult<ImageIdentity> {
    let row = matrix_profile(profile);
    let exact = row
        .settings_identity
        .as_ref()
        .ok_or("Image lacks qualified packed settings identity")?;
    Ok(ImageIdentity {
        ihx_sha256: exact.ihx_sha256.clone(),
        code_sha256: exact.code_sha256.clone(),
        definition_sha256: exact.definition_sha256.clone(),
        schema: exact.nvm_schema,
    })
}
fn target_contract(
    profile: ProfileId,
    id: &ImageIdentity,
) -> BackupResult<&'static crate::app::ak820_beta1_profiles::MatrixProfile> {
    let image = crate::ak820::contract_for_hash(id.schema, &id.code_sha256)
        .ok_or("Unknown exact target image")?;
    let row = image
        .matrix_profile()
        .ok_or("Target lacks profile settings contract")?;
    let exact = row
        .settings_identity
        .as_ref()
        .ok_or("Target lacks settings identity")?;
    require(
        row.profile == profile
            && exact.ihx_sha256 == id.ihx_sha256
            && exact.definition_sha256 == id.definition_sha256,
        "Target differs from exact profile identity",
    )?;
    Ok(row)
}
fn transition_pairs() -> Vec<(String, String)> {
    firmware_matrix()
        .settings_transitions
        .iter()
        .chain(
            crate::app::ak820_beta1_profiles::previous_firmware_matrix()
                .settings_transitions
                .iter(),
        )
        .map(|p| (p.source.clone(), p.target.clone()))
        .collect()
}
const DEFINITION_SHA: &str = "72b2261e1f7da5ec8ba37b8276bab12f12642e3d6514ec76ca1e4cd9b499fa6e";
fn historical_identity(id: &ImageIdentity) -> bool {
    id.definition_sha256 == DEFINITION_SHA
        && historical_settings_identity(&id.ihx_sha256, &id.code_sha256, id.schema).is_some()
}
fn trusted_identities() -> Vec<ImageIdentity> {
    let mut result: Vec<_> = firmware_matrix()
        .profiles
        .iter()
        .filter_map(|r| identity(r.profile).ok())
        .collect();
    result.extend(
        historical_settings_identities()
            .iter()
            .map(|h| ImageIdentity {
                ihx_sha256: h.ihx_sha256.into(),
                code_sha256: h.code_sha256.into(),
                definition_sha256: DEFINITION_SHA.into(),
                schema: h.nvm_schema,
            }),
    );
    result
}
fn source_rgb(raw: &[u8], selected: bool, profile: Option<ProfileId>) -> BackupResult<[u8; 6]> {
    match profile {
        Some(p) => rgb_wire_for_profile(raw, selected, p),
        None => rgb_wire(raw, selected),
    }
}
/// Production registry and transition pairs come only from the bundled exact
/// settings catalog. Linked CODE identities and profile names grant nothing.
/// The source NVM is bound again to the archive manifest and derived settings.
/// Target identity/read stability must be independently verified before execution.
pub(crate) fn plan_restore(
    archive: &ValidatedArchive,
    source_nvm: &[u8],
    target_profile: ProfileId,
    target_identity: &ImageIdentity,
    target_nvm: &[u8],
) -> BackupResult<RestorePlan> {
    let matrix = firmware_matrix();
    let trusted = trusted_identities();
    let pairs = transition_pairs();
    target_contract(target_profile, target_identity)?;
    let source_profile = matrix
        .profiles
        .iter()
        .find(|r| identity(r.profile).ok().as_ref() == Some(&archive.source))
        .map(|r| r.profile);
    require(
        source_profile.is_some() || historical_identity(&archive.source),
        "Unknown exact source image",
    )?;
    let source_combo = match source_profile {
        Some(profile) => matrix_profile(profile).capabilities.dynamic_entries.combos > 0,
        None => {
            historical_settings_identity(
                &archive.source.ihx_sha256,
                &archive.source.code_sha256,
                archive.source.schema,
            )
            .ok_or("Unknown historical Combo contract")?
            .combos
        }
    };
    plan_with_registry(
        archive,
        source_nvm,
        source_profile.or_else(|| {
            historical_settings_identity(
                &archive.source.ihx_sha256,
                &archive.source.code_sha256,
                archive.source.schema,
            )
            .and_then(|h| h.profile)
        }),
        source_combo,
        target_profile,
        target_identity,
        target_nvm,
        &trusted,
        &pairs,
    )
}

/// Reset the settings reachable through the reviewed setters to the selected
/// release's compiled defaults. Inactive modules, reserved bytes and values
/// without a setter remain unchanged. This is a derived local reset plan, not
/// an imported or supposedly captured settings archive.
pub(crate) fn plan_factory_reset(
    profile: ProfileId,
    target: &ImageIdentity,
    before: &[u8],
) -> BackupResult<RestorePlan> {
    let contract = target_contract(profile, target)?;
    validate_nvm(before, target.schema)?;
    let image = crate::ak820::contract_for_hash(target.schema, &target.code_sha256)
        .ok_or("Unknown reset target")?;
    let defaults = crate::ak820_release::defaults_for_image(image)?;
    let mut wanted = before.to_vec();
    wanted[24..29].copy_from_slice(&defaults[24..29]);
    wanted[40] = defaults[40];
    wanted[0x40..0x6a8].copy_from_slice(&defaults[0x40..0x6a8]);
    wanted[0x7f0..0x7f3].copy_from_slice(&defaults[0x7f0..0x7f3]);
    wanted[0x800..0x810].fill(0);
    let mut typed = Extensions::decode_nvm(before)?;
    let default_settings = QmkSettings::default();
    for &id in &contract.capabilities.runtime_qmk_settings_ids {
        typed.settings.set(id, default_settings.get(id)?)?;
    }
    if contract.capabilities.dynamic_entries.tap_dance > 0 {
        typed.tap_dance.fill(Default::default());
    }
    if contract.capabilities.dynamic_entries.key_overrides > 0 {
        typed.key_overrides.fill(Default::default());
    }
    wanted = typed.apply_to_nvm_for_contract(&wanted, contract)?;
    let mut settings = settings_from_nvm(
        &wanted,
        &rgb_wire_for_profile(&wanted[24..32], false, profile)?,
        true,
    )?;
    crate::ak820_settings_model::apply_image_limitations(&mut settings, target);
    let manifest = json!({"format":"ak820-derived-factory-reset-1","profile":profile.as_str(),
        "files":{"saved-nvm.bin":{"size":wanted.len(),"sha256":sha256(&wanted)}},
        "compiled_ihx_sha256":target.ihx_sha256});
    let archive = ValidatedArchive {
        manifest_sha256: sha256(&canonical(&manifest)?),
        manifest,
        settings,
        source: target.clone(),
    };
    plan_restore(&archive, &wanted, profile, target, before)
}
// Private registry injection exists for pure synthetic tests only. No caller can
// replace the production trusted registry or bypass catalog admission.
fn plan_with_registry(
    archive: &ValidatedArchive,
    source_nvm: &[u8],
    source_profile: Option<ProfileId>,
    source_combo: bool,
    target_profile: ProfileId,
    target: &ImageIdentity,
    before: &[u8],
    trusted: &[ImageIdentity],
    pairs: &[(String, String)],
) -> BackupResult<RestorePlan> {
    // Unknown identities reach this fallback only through the private synthetic registry seam.
    let target_row =
        target_contract(target_profile, target).unwrap_or_else(|_| matrix_profile(target_profile));
    require(
        transition_allowed(&archive.source, target, pairs, trusted),
        "Unknown image, unreviewed exact transition, or schema03 downgrade",
    )?;
    require(
        matches!(archive.source.schema, 0x082002 | SCHEMA)
            && matches!(target.schema, 0x082002 | SCHEMA),
        "Schema01 migration semantic gate is unavailable",
    )?;
    validate_nvm(source_nvm, archive.source.schema)?;
    validate_nvm(before, target.schema)?;
    require(
        archive.manifest["files"]["saved-nvm.bin"]["sha256"] == sha256(source_nvm)
            && archive.manifest["files"]["saved-nvm.bin"]["size"] == source_nvm.len(),
        "Source NVM differs from validated archive",
    )?;
    let target_combo = target_row.capabilities.dynamic_entries.combos > 0;
    let mut settings = settings_from_nvm(
        source_nvm,
        &source_rgb(&source_nvm[24..32], false, source_profile)?,
        source_combo,
    )?;
    crate::ak820_settings_model::apply_image_limitations(&mut settings, &archive.source);
    require(
        canonical(&settings)? == canonical(&archive.settings)?,
        "Source settings differ from validated archive",
    )?;
    let mut blockers = Vec::new();
    for (start, end) in [(2, 24), (32, 37)] {
        if source_nvm[start..end] != before[start..end] {
            blockers.push(format!(
                "Core NVM {start}..{} has no restore setter",
                end - 1
            ));
        }
    }
    let translated_rgb = translate_rgb(&source_nvm[24..32], source_profile, target_profile)?;
    let rgb = translated_rgb.as_slice();
    let selected = rgb_wire_for_profile(rgb, true, target_profile)?;
    let effective = rgb_wire_for_profile(rgb, false, target_profile)?;
    if selected[0] == 1 {
        blockers.push("Direct RGB palette has no getter".into());
    }
    if rgb[5..8] != before[29..32] || rgb[6..8] != [0, 0] {
        blockers.push("RGB flags/padding have no setter".into());
    }
    for (name, source) in [
        ("keys", KeycodeSource::Matrix),
        ("encoders", KeycodeSource::Encoder),
    ] {
        for item in settings[name].as_array().ok_or("Missing assignments")? {
            let code = item["keycode"].as_u64().ok_or("Invalid keycode")? as u16;
            if !crate::app::ak820_beta1_profiles::supports_keycode_for_row(target_row, code, source)
            {
                blockers.push(format!("Unsupported {name} keycode {code:#06x}"));
            }
        }
    }
    let stream = hex_bytes(&settings["macros"]["data_hex"])?;
    let chunks = match macro_chunks(&stream) {
        Ok(v) => v,
        Err(e) => {
            blockers.push(e);
            Vec::new()
        }
    };
    for commands in settings["macros"]["commands"]
        .as_array()
        .ok_or("Missing macro commands")?
    {
        for command in commands.as_array().ok_or("Invalid macro commands")? {
            let name = command["command"].as_str().unwrap_or("");
            let source = match name {
                "tap" | "tap16" => KeycodeSource::MacroTap,
                "down" | "down16" => KeycodeSource::MacroDown,
                "up" | "up16" => KeycodeSource::MacroUp,
                _ => continue,
            };
            let operands = hex_bytes(&command["operands_hex"])?;
            let mut code = if operands.len() == 1 {
                operands[0] as u16
            } else {
                u16::from_le_bytes([operands[0], operands[1]])
            };
            if operands.len() == 2 && code > 0xff00 {
                code = (code & 255) << 8;
            }
            if !crate::app::ak820_beta1_profiles::supports_keycode_for_row(target_row, code, source)
            {
                blockers.push(format!("Unsupported macro {name} keycode {code:#06x}"));
            }
        }
    }
    let typed = if target.schema == SCHEMA {
        let old = Extensions::decode_nvm(before)?;
        let wanted = if archive.source.schema == SCHEMA {
            Extensions::decode_nvm(source_nvm)?
        } else {
            require(
                old.settings == QmkSettings::default()
                    && old.tap_dance.iter().all(|v| *v == Default::default())
                    && old.key_overrides.iter().all(|v| *v == Default::default()),
                "Legacy backup cannot overwrite nondefault target extensions",
            )?;
            old.clone()
        };
        // Reject inaccessible fields before creating any setter plan.
        wanted.apply_to_nvm_for_contract(before, target_row)?;
        Some(wanted)
    } else {
        None
    };
    if source_combo
        && !target_row
            .settings_identity
            .as_ref()
            .is_some_and(|i| i.combo_actions.iter().any(|a| a == "caps-word-toggle"))
    {
        for entry in settings["combos"]["entries_hex"]
            .as_array()
            .ok_or("Missing Combo records")?
        {
            let data = hex_bytes(entry)?;
            if u16::from_le_bytes([data[8], data[9]]) == 0x7c73 {
                blockers.push("Target lacks Combo Caps Word action".into());
            }
        }
    }
    if source_combo && !target_combo {
        blockers.push("Target has no Combo setters".into());
    }
    if !source_combo && target_combo {
        let target_settings = settings_from_nvm(
            before,
            &rgb_wire_for_profile(&before[24..32], false, target_profile)?,
            true,
        )?;
        require(
            target_settings["combos"]["entries_hex"]
                .as_array()
                .ok_or("Missing Combos")?
                .iter()
                .all(|v| v.as_str() == Some("00000000000000000000")),
            "Legacy backup must preserve existing nonempty Combos",
        )?;
    }
    require(
        blockers.is_empty(),
        &format!(
            "Restore preflight blocked before any setter:\n{}",
            blockers.join("\n")
        ),
    )?;
    let mut plan = RestorePlan {
        source: archive.source.clone(),
        target: target.clone(),
        before: before.to_vec(),
        expected: before.to_vec(),
        steps: Vec::new(),
        preconditions: vec![
            RestorePrecondition::PinnedSource {
                archive_manifest_sha256: archive.manifest_sha256.clone(),
                identity: archive.source.clone(),
                nvm: source_nvm.to_vec(),
            },
            RestorePrecondition::SelectedTarget {
                profile: target_profile,
                identity: target.clone(),
                nvm: before.to_vec(),
            },
            RestorePrecondition::RevalidateSourceBeforeFirstSetter,
            RestorePrecondition::FreshPhysicalUnlockAfterPlanning {
                reject_preexisting_unlock: true,
            },
            RestorePrecondition::FreshUnlockedStatusBeforeEachSetter,
            RestorePrecondition::MatchingAckAndReadbackBeforeNextSetter,
            RestorePrecondition::StopOnTimeoutStorageFaultDisconnectOrMismatch,
            RestorePrecondition::QueriedActiveQsid {
                expected: target_row.capabilities.runtime_qmk_settings_ids.clone(),
            },
            RestorePrecondition::FinalGetterReconciliation,
        ],
        final_getters: Value::Null,
        final_extensions: None,
    };
    if target_combo {
        plan.preconditions
            .push(RestorePrecondition::DiscoveredComboCount { expected: 8 });
    }
    for item in settings["keys"].as_array().unwrap() {
        let layer = item["layer"].as_u64().unwrap() as usize;
        let row = item["row"].as_u64().unwrap() as usize;
        let col = item["column"].as_u64().unwrap() as usize;
        let value = item["keycode"].as_u64().unwrap() as u16;
        plan.persist(
            "set_key",
            json!([layer, row, col, value]),
            0x40 + layer * 256 + row * 16 + col * 2,
            &value.to_be_bytes(),
            Some(("get_key", json!([layer, row, col]), json!(value))),
        );
    }
    for item in settings["encoders"].as_array().unwrap() {
        let layer = item["layer"].as_u64().unwrap() as usize;
        let direction = item["direction"].as_u64().unwrap() as usize;
        let value = item["keycode"].as_u64().unwrap() as u16;
        plan.persist(
            "set_encoder",
            json!([layer, direction, value]),
            0x640 + layer * 4 + (1 - direction) * 2,
            &value.to_be_bytes(),
            Some(("get_encoder", json!([layer, direction]), json!(value))),
        );
    }
    plan.persist(
        "set_layout_options",
        json!([source_nvm[40]]),
        40,
        &source_nvm[40..41],
        Some(("get_layout_options", json!([]), json!(source_nvm[40]))),
    );
    if plan.expected[0x800..0x800 + stream.len()] != stream
        || plan.expected[0x7f1..0x7f3] != (stream.len() as u16).to_le_bytes()
    {
        // beta2 ak820_macro_write durably sets busy before the first chunk;
        // length remains unchanged until publication of terminator sixteen.
        let mut intermediate = plan.expected.clone();
        for (offset, data) in chunks {
            intermediate[0x7f0] = 1;
            intermediate[0x800 + offset..0x800 + offset + data.len()].copy_from_slice(&data);
            let mut expectations = vec![ReadExpectation::NvmRange {
                offset: 0x800 + offset,
                expected: data.clone(),
            }];
            if offset + data.len() < stream.len() {
                expectations.push(ReadExpectation::FullNvm(intermediate.clone()));
            }
            plan.steps.push(RestoreStep {
                preconditions: plan.preconditions.clone(),
                command: "upload_macro_chunk",
                arguments: json!([offset, data]),
                expectations,
            });
        }
        plan.expected[0x800..0x800 + stream.len()].copy_from_slice(&stream);
        plan.expected[0x7f0] = 0;
        plan.expected[0x7f1..0x7f3].copy_from_slice(&(stream.len() as u16).to_le_bytes());
        let mut published = stream.clone();
        published.resize(8192, 0);
        let step = plan.steps.last_mut().unwrap();
        step.expectations.push(ReadExpectation::Getter {
            command: "macros",
            arguments: json!([]),
            expected: json!(published),
        });
        step.expectations
            .push(ReadExpectation::FullNvm(plan.expected.clone()));
    }
    if plan.expected[24..32] != *rgb {
        plan.steps.push(RestoreStep {
            preconditions: plan.preconditions.clone(),
            command: "set_rgb_mode",
            arguments: json!(selected),
            expectations: vec![ReadExpectation::Getter {
                command: "rgb_mode",
                arguments: json!([]),
                expected: json!(selected),
            }],
        });
        if rgb[0] & 3 == 0 {
            plan.steps.push(RestoreStep {
                preconditions: plan.preconditions.clone(),
                command: "set_rgb_mode",
                arguments: json!(effective),
                expectations: vec![ReadExpectation::Getter {
                    command: "rgb_mode",
                    arguments: json!([]),
                    expected: json!(effective),
                }],
            });
        }
        plan.persist("save_rgb", json!([]), 24, rgb, None);
    }
    if source_combo {
        for (i, value) in settings["combos"]["entries_hex"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let data = hex_bytes(value)?;
            if data.iter().all(|&byte| byte == 0)
                && plan.expected[0x658 + i * 10..0x662 + i * 10]
                    .iter()
                    .all(|&byte| byte == 0)
            {
                continue;
            }
            let mut stored = data.clone();
            stored[4..8].copy_from_slice(&[0x20, 8, 1, 0]);
            plan.persist(
                "set_combo",
                json!([i, data]),
                0x658 + i * 10,
                &stored,
                Some(("get_combo", json!([i]), json!(data))),
            );
        }
    }
    if let Some(wanted) = typed {
        let capabilities = &target_row.capabilities;
        let mut current = Extensions::decode_nvm(&plan.expected)?;
        for &id in &capabilities.runtime_qmk_settings_ids {
            let value = wanted.settings.get(id)?;
            if current.settings.get(id)? != value {
                current.settings.set(id, value)?;
                plan.persist(
                    "set_setting",
                    json!([id, value]),
                    0x6a8,
                    &current.settings.encode(),
                    Some(("get_setting", json!([id]), json!(value))),
                );
            }
        }
        for td in [true, false] {
            if (if td {
                capabilities.dynamic_entries.tap_dance
            } else {
                capabilities.dynamic_entries.key_overrides
            }) == 0
            {
                continue;
            }
            for i in 0..8 {
                let data = if td {
                    wanted.tap_dance[i].encode()?
                } else {
                    wanted.key_overrides[i].encode()?
                };
                let kind = if td { "tap_dance" } else { "key_overrides" };
                plan.persist(
                    "set_extension",
                    json!([kind, i, data.as_slice()]),
                    slot_offset(td, i)?,
                    &data,
                    Some(("get_extension", json!([kind, i]), json!(data.as_slice()))),
                );
            }
        }
        require(
            Extensions::decode_nvm(&plan.expected)? == wanted,
            "Unexposed extension state differs",
        )?;
    }
    validate_nvm(&plan.expected, target.schema)?;
    plan.final_getters = settings_from_nvm(
        &plan.expected,
        &rgb_wire_for_profile(&plan.expected[24..32], false, target_profile)?,
        target_combo,
    )?;
    plan.final_extensions = if target.schema == SCHEMA {
        Some(Extensions::decode_nvm(&plan.expected)?)
    } else {
        None
    };
    Ok(plan)
}
impl RestorePlan {
    fn persist(
        &mut self,
        command: &'static str,
        arguments: Value,
        offset: usize,
        data: &[u8],
        getter: Option<(&'static str, Value, Value)>,
    ) {
        if self.expected[offset..offset + data.len()] == *data {
            return;
        }
        self.expected[offset..offset + data.len()].copy_from_slice(data);
        let mut expectations = Vec::new();
        if let Some((command, arguments, expected)) = getter {
            expectations.push(ReadExpectation::Getter {
                command,
                arguments,
                expected,
            });
        }
        expectations.push(ReadExpectation::FullNvm(self.expected.clone()));
        self.steps.push(RestoreStep {
            preconditions: self.preconditions.clone(),
            command,
            arguments,
            expectations,
        });
    }
}
/// Catalog-bound offline firmware-image migration; never a device operation.
/// Schema01 remains unsupported: its semantic migration gate is unavailable.
pub(crate) fn migrate_schema02_to03(
    source: &[u8],
    from: &ImageIdentity,
    target_profile: ProfileId,
    to: &ImageIdentity,
) -> BackupResult<Vec<u8>> {
    require(
        historical_identity(from),
        "Source differs from exact profile identity",
    )?;
    require(
        identity(target_profile)? == *to,
        "Target differs from exact profile identity",
    )?;
    let matrix = firmware_matrix();
    let trusted = trusted_identities();
    let pairs: Vec<_> = matrix
        .settings_transitions
        .iter()
        .map(|pair| (pair.source.clone(), pair.target.clone()))
        .collect();
    validate_nvm(source, from.schema)?;
    let rgb = translate_rgb(&source[24..32], None, target_profile)?;
    if !historical_settings_identity(&from.ihx_sha256, &from.code_sha256, from.schema)
        .ok_or("Unknown historical Combo contract")?
        .combos
    {
        require(
            source[0x658..0x6a8].iter().all(|&byte| byte == 0),
            "Legacy reserved Combo bytes are nonempty; explicit migration cannot interpret them",
        )?;
    }
    let mut candidate = migrate_with_registry(source, from, to, &trusted, &pairs)?;
    candidate[24..32].copy_from_slice(&rgb);
    Ok(candidate)
}
// Private transform permits synthetic fixtures, never caller-supplied trust.
fn migrate_with_registry(
    source: &[u8],
    from: &ImageIdentity,
    to: &ImageIdentity,
    trusted: &[ImageIdentity],
    pairs: &[(String, String)],
) -> BackupResult<Vec<u8>> {
    require(
        from.schema == 0x082002
            && to.schema == SCHEMA
            && transition_allowed(from, to, pairs, trusted),
        "Unreviewed exact schema02 -> schema03 migration",
    )?;
    validate_nvm(source, from.schema)?;
    let mut candidate = source.to_vec();
    candidate[0x6a8..0x7f0].fill(0);
    candidate[37..40].copy_from_slice(&SCHEMA.to_le_bytes()[..3]);
    candidate[0x6a8..0x6c0].copy_from_slice(&QmkSettings::default().encode());
    validate_nvm(&candidate, SCHEMA)?;
    Ok(candidate)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factory_reset_uses_public_defaults_and_preserves_inactive_and_reserved_bytes() {
        for row in crate::app::ak820_beta1_profiles::previous_firmware_matrix()
            .profiles
            .iter()
            .chain(firmware_matrix().profiles.iter())
        {
            let profile = row.profile;
            let exact = row.settings_identity.as_ref().unwrap();
            let target = ImageIdentity {
                ihx_sha256: exact.ihx_sha256.clone(),
                code_sha256: exact.code_sha256.clone(),
                definition_sha256: exact.definition_sha256.clone(),
                schema: exact.nvm_schema,
            };
            let image =
                crate::ak820::contract_for_hash(target.schema, &target.code_sha256).unwrap();
            let defaults = crate::ak820_release::defaults_for_image(image).unwrap();
            let unchanged = plan_factory_reset(profile, &target, &defaults).unwrap();
            assert!(
                unchanged.steps().is_empty(),
                "{}: {:?}",
                profile.as_str(),
                unchanged.steps()
            );
            let mut before = defaults.clone();
            before[0x40..0x42].copy_from_slice(&4u16.to_be_bytes());
            before[0x7a0] = 0x55; // unused extension padding
            let mut typed = Extensions::decode_nvm(&before).unwrap();
            typed.settings.set(7, 350).unwrap();
            // QSID4 is stored in every image; exact image capabilities determine accessibility.
            typed.settings.set(4, 250).unwrap();
            before[0x6a8..0x6c0].copy_from_slice(&typed.settings.encode());
            let plan = plan_factory_reset(profile, &target, &before).unwrap();
            assert_eq!(plan.expected_nvm()[0x7a0], 0x55);
            let after = Extensions::decode_nvm(plan.expected_nvm()).unwrap();
            assert_eq!(
                after.settings.get(7).unwrap(),
                QmkSettings::default().get(7).unwrap()
            );
            assert_eq!(
                after.settings.get(4).unwrap(),
                if row.capabilities.runtime_qmk_settings_ids.contains(&4) {
                    QmkSettings::default().get(4).unwrap()
                } else {
                    250
                }
            );
            assert!(!plan.steps().is_empty());
            assert!(plan.requires_fresh_physical_unlock());
        }
    }
    #[test]
    fn previous_and_current_restore_targets_use_reviewed_direction_and_capabilities() {
        use crate::ak820::ImageContract;
        let old = ImageContract::Beta2(ProfileId::Main);
        let new = ImageContract::Beta3(ProfileId::Main);
        let image_id = |image: ImageContract| {
            let exact = image
                .matrix_profile()
                .unwrap()
                .settings_identity
                .as_ref()
                .unwrap();
            ImageIdentity {
                ihx_sha256: exact.ihx_sha256.clone(),
                code_sha256: exact.code_sha256.clone(),
                definition_sha256: exact.definition_sha256.clone(),
                schema: exact.nvm_schema,
            }
        };
        let old_id = image_id(old);
        let new_id = image_id(new);
        let old_nvm = crate::ak820_release::defaults_for_image(old).unwrap();
        let new_nvm = crate::ak820_release::defaults_for_image(new).unwrap();
        // Public NVM-only host models; these are not accepted full-ROM archives.
        let old_archive = archive(&old_nvm, old_id.clone(), ProfileId::Main);
        assert!(
            plan_restore(&old_archive, &old_nvm, ProfileId::Main, &old_id, &old_nvm)
                .unwrap()
                .steps()
                .is_empty()
        );
        assert!(plan_restore(&old_archive, &old_nvm, ProfileId::Main, &new_id, &new_nvm).is_ok());
        let new_archive = archive(&new_nvm, new_id.clone(), ProfileId::Main);
        assert!(plan_restore(&new_archive, &new_nvm, ProfileId::Main, &old_id, &old_nvm).is_err());
        let old_game = ImageContract::Beta2(ProfileId::Game);
        let game_id = image_id(old_game);
        let game_nvm = crate::ak820_release::defaults_for_image(old_game).unwrap();
        assert!(plan_restore(&old_archive, &old_nvm, ProfileId::Game, &game_id, &game_nvm).is_ok());
        let mut unknown = old_id.clone();
        unknown.ihx_sha256 = "0".repeat(64);
        assert!(plan_factory_reset(ProfileId::Main, &unknown, &old_nvm).is_err());
        assert!(plan_restore(&old_archive, &old_nvm, ProfileId::Main, &unknown, &old_nvm).is_err());
    }
    // These identities and bytes are synthetic offline models. Explicit test
    // registries/pairs never establish production image or hardware qualification.
    fn image(tag: char, schema: u32) -> ImageIdentity {
        ImageIdentity {
            ihx_sha256: tag.to_string().repeat(64),
            code_sha256: tag.to_string().repeat(64),
            definition_sha256: "d".repeat(64),
            schema,
        }
    }
    fn nvm(schema: u32) -> Vec<u8> {
        let mut raw = vec![0; 0x2800];
        raw[..2].copy_from_slice(&[0xe6, 0xfe]);
        raw[37..40].copy_from_slice(&schema.to_le_bytes()[..3]);
        raw[24] = 5;
        raw[27] = 80;
        raw[0x7f1] = 16;
        for i in 0..8 {
            raw[0x658 + i * 10 + 4..0x658 + i * 10 + 8].copy_from_slice(&[0x20, 8, 1, 0]);
        }
        if schema == SCHEMA {
            raw[0x6a8..0x6c0].copy_from_slice(&QmkSettings::default().encode());
        }
        raw
    }
    fn archive(raw: &[u8], source: ImageIdentity, profile: ProfileId) -> ValidatedArchive {
        let mut archive = ValidatedArchive {
            manifest_sha256: "synthetic".into(),
            manifest: json!({"files":{"saved-nvm.bin":{"sha256":sha256(raw),"size":raw.len()}}}),
            settings: settings_from_nvm(
                raw,
                &(if source.schema == SCHEMA {
                    rgb_wire_for_profile(&raw[24..32], false, profile)
                } else {
                    rgb_wire(&raw[24..32], false)
                })
                .unwrap(),
                matrix_profile(profile).capabilities.dynamic_entries.combos > 0,
            )
            .unwrap(),
            source,
        };
        crate::ak820_settings_model::apply_image_limitations(
            &mut archive.settings,
            &archive.source,
        );
        archive
    }
    fn plan(raw: &[u8], target: &[u8]) -> BackupResult<RestorePlan> {
        let image = image('a', 0x082002);
        let a = archive(raw, image.clone(), ProfileId::Main);
        plan_with_registry(
            &a,
            raw,
            None,
            true,
            ProfileId::Main,
            &image,
            target,
            &[image.clone()],
            &[],
        )
    }
    #[test]
    fn exact_historical_migration_and_rgb_translation() {
        let h = &historical_settings_identities()[1];
        let from = ImageIdentity {
            ihx_sha256: h.ihx_sha256.into(),
            code_sha256: h.code_sha256.into(),
            definition_sha256: DEFINITION_SHA.into(),
            schema: h.nvm_schema,
        };
        let mut raw = nvm(0x082002);
        raw[24] = (6 << 2) | 1; // Legacy Reactive.
        let snapshot = raw.clone();
        let to = identity(ProfileId::Code).unwrap();
        let migrated = migrate_schema02_to03(&raw, &from, ProfileId::Code, &to).unwrap();
        assert_eq!(raw, snapshot);
        assert_eq!(migrated[24], (5 << 2) | 1);
        let saved_archive = archive(&raw, from.clone(), ProfileId::Main);
        let before = nvm(SCHEMA);
        let plan = plan_restore(&saved_archive, &raw, ProfileId::Code, &to, &before).unwrap();
        assert_eq!(plan.expected_nvm()[24], (5 << 2) | 1);
        let fixed = super::tests::archive(plan.expected_nvm(), to.clone(), ProfileId::Code);
        assert!(
            plan_restore(
                &fixed,
                plan.expected_nvm(),
                ProfileId::Code,
                &to,
                plan.expected_nvm()
            )
            .unwrap()
            .steps()
            .is_empty()
        );
        let mut unknown = from.clone();
        unknown.definition_sha256 = "0".repeat(64);
        assert!(migrate_schema02_to03(&raw, &unknown, ProfileId::Code, &to).is_err());
        assert!(migrate_schema02_to03(&migrated, &to, ProfileId::Code, &from).is_err());
        raw[24] = (5 << 2) | 1; // Historical Direct is unavailable on Code.
        assert!(
            migrate_schema02_to03(&raw, &from, ProfileId::Code, &to)
                .unwrap_err()
                .contains("RGB effect")
        );
        let direct = archive(&raw, from.clone(), ProfileId::Main);
        assert!(plan_restore(&direct, &raw, ProfileId::Code, &to, &before).is_err());
        assert_eq!(before, nvm(SCHEMA));
    }
    #[test]
    fn current_reactive_mode_five_is_never_direct() {
        let mut raw = nvm(SCHEMA);
        raw[24] = (5 << 2) | 1;
        let from = identity(ProfileId::Code).unwrap();
        let to = identity(ProfileId::Main).unwrap();
        let archive = archive(&raw, from, ProfileId::Code);
        let plan = plan_restore(&archive, &raw, ProfileId::Main, &to, &nvm(SCHEMA)).unwrap();
        assert_eq!(plan.expected_nvm()[24], (6 << 2) | 1);
        assert_eq!(
            plan.final_getters()["rgb"]["current_mode_hex"],
            "1f0000000050"
        );
    }
    #[test]
    fn historical_source_without_combo_never_interprets_reserved_bytes_or_clears_target() {
        let h = &historical_settings_identities()[0];
        assert!(!h.combos);
        let from = ImageIdentity {
            ihx_sha256: h.ihx_sha256.into(),
            code_sha256: h.code_sha256.into(),
            definition_sha256: DEFINITION_SHA.into(),
            schema: h.nvm_schema,
        };
        let mut raw = nvm(0x082002);
        raw[0x658..0x6a8].fill(0xa5); // Captured evidence, never a Combo record.
        let mut saved = archive(&nvm(0x082002), from.clone(), ProfileId::Main);
        saved.manifest["files"]["saved-nvm.bin"]["sha256"] = json!(sha256(&raw));
        saved.settings =
            settings_from_nvm(&raw, &rgb_wire(&raw[24..32], false).unwrap(), false).unwrap();
        assert!(saved.settings.get("combos").is_none());
        let target = identity(ProfileId::Main).unwrap();
        let mut before = nvm(SCHEMA);
        let plan = plan_restore(&saved, &raw, ProfileId::Main, &target, &before).unwrap();
        assert_eq!(plan.expected_nvm(), before);
        before[0x658..0x65c].copy_from_slice(&[4, 0, 5, 0]);
        before[0x660..0x662].copy_from_slice(&[6, 0]);
        let snapshot = before.clone();
        assert!(
            plan_restore(&saved, &raw, ProfileId::Main, &target, &before)
                .unwrap_err()
                .contains("preserve existing nonempty Combos")
        );
        assert_eq!(before, snapshot);
        assert!(
            migrate_schema02_to03(&raw, &from, ProfileId::Main, &target)
                .unwrap_err()
                .contains("reserved Combo")
        );
        raw[0x658..0x6a8].fill(0);
        assert!(migrate_schema02_to03(&raw, &from, ProfileId::Main, &target).is_ok());
    }
    #[test]
    fn read_only_restore_refuses_before_transport_and_callbacks() {
        let before = nvm(0x082002);
        let mut saved = before.clone();
        saved[0x40..0x42].copy_from_slice(&4u16.to_be_bytes());
        let restore = plan(&saved, &before).unwrap();
        assert!(!restore.steps().is_empty());
        let (hid, recorder) = crate::hid::HidDevice::test_read_only_device(Default::default());
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let progress = std::sync::atomic::AtomicU32::new(0);
        let error = hid
            .ak820_restore_backup(
                &restore,
                &cancelled,
                &progress,
                || panic!("read-only restore must not reopen a source"),
                |_| panic!("read-only restore must not create a backup"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("read-only"));
        assert!(recorder.requests().is_empty());
    }

    #[test]
    fn no_op_and_key_plan_preserve_unaddressable_neighbors() {
        let before = nvm(0x082002);
        assert!(plan(&before, &before).unwrap().steps().is_empty());
        let mut saved = before.clone();
        saved[0x40..0x42].copy_from_slice(&4u16.to_be_bytes());
        let p = plan(&saved, &before).unwrap();
        assert!(p.preconditions().contains(
            &RestorePrecondition::FreshPhysicalUnlockAfterPlanning {
                reject_preexisting_unlock: true
            }
        ));
        assert!(
            p.preconditions()
                .contains(&RestorePrecondition::DiscoveredComboCount { expected: 8 })
        );
        assert!(
            p.preconditions()
                .contains(&RestorePrecondition::QueriedActiveQsid {
                    expected: matrix_profile(ProfileId::Main)
                        .capabilities
                        .runtime_qmk_settings_ids
                        .clone()
                })
        );
        assert!(
            p.preconditions()
                .contains(&RestorePrecondition::PinnedSource {
                    archive_manifest_sha256: "synthetic".into(),
                    identity: image('a', 0x082002),
                    nvm: saved.clone()
                })
        );
        assert!(
            p.preconditions()
                .contains(&RestorePrecondition::SelectedTarget {
                    profile: ProfileId::Main,
                    identity: image('a', 0x082002),
                    nvm: before.clone()
                })
        );
        for requirement in [
            RestorePrecondition::RevalidateSourceBeforeFirstSetter,
            RestorePrecondition::FreshUnlockedStatusBeforeEachSetter,
            RestorePrecondition::MatchingAckAndReadbackBeforeNextSetter,
            RestorePrecondition::StopOnTimeoutStorageFaultDisconnectOrMismatch,
            RestorePrecondition::FinalGetterReconciliation,
        ] {
            assert!(p.preconditions().contains(&requirement));
        }
        assert!(
            p.steps()
                .iter()
                .all(|step| step.preconditions() == p.preconditions())
        );
        assert_eq!(
            p.final_getters(),
            &archive(&saved, image('a', 0x082002), ProfileId::Main).settings
        );
        assert!(p.final_extensions().is_none());
        assert_eq!(p.steps().len(), 1);
        assert_eq!(p.steps()[0].command(), "set_key");
        assert_eq!(p.expected_nvm(), saved);
        assert_eq!(p.before_nvm(), before);
        assert!(matches!(
            &p.steps()[0].expectations()[0],
            ReadExpectation::Getter {
                command: "get_key",
                ..
            }
        ));
        assert!(matches!(&p.steps()[0].expectations()[1],ReadExpectation::FullNvm(v) if v==&saved));
    }
    #[test]
    fn core_direct_palette_and_macro_padding_block_before_planning() {
        let before = nvm(0x082002);
        let mut saved = before.clone();
        saved[3] = 1;
        assert!(
            plan(&saved, &before)
                .unwrap_err()
                .contains("no restore setter")
        );
        let mut saved = before.clone();
        saved[24] = 21;
        assert!(plan(&saved, &before).unwrap_err().contains("palette"));
        let mut saved = before.clone();
        saved[0x7f1] = 44;
        assert!(plan(&saved, &before).unwrap_err().contains("padding"));
    }
    #[test]
    fn macro_chunks_publish_only_at_end_and_preserve_unused_tail() {
        let before = nvm(0x082002);
        let mut saved = before.clone();
        // First macro text plus 16 terminators; final chunk owns terminator 16.
        saved[0x800..0x81e].fill(b'x');
        saved[0x81e..0x82e].fill(0);
        saved[0x7f1] = 46;
        let p = plan(&saved, &before).unwrap();
        assert!(
            p.steps()
                .iter()
                .all(|step| step.preconditions() == p.preconditions())
        );
        assert!(
            p.preconditions()
                .contains(&RestorePrecondition::FinalGetterReconciliation)
        );
        assert_eq!(p.steps().len(), 2);
        assert_eq!(p.steps()[0].command(), "upload_macro_chunk");
        assert_eq!(p.steps()[0].expectations().len(), 2);
        assert!(matches!(
            p.steps()[1].expectations().last(),
            Some(ReadExpectation::FullNvm(_))
        ));
        assert_eq!(p.expected_nvm(), saved);
    }
    #[test]
    fn nonmatrix_processors_in_macro_and_encoder_are_rejected() {
        let before = nvm(0x082002);
        let mut saved = before.clone();
        saved[0x640..0x642].copy_from_slice(&0x5700u16.to_be_bytes());
        assert!(
            plan(&saved, &before)
                .unwrap_err()
                .contains("Unsupported encoders")
        );
        let mut saved = before.clone();
        saved[0x800..0x804].copy_from_slice(&[1, 5, 0x53, 0x7c]);
        saved[0x7f1] = 20;
        assert!(
            plan(&saved, &before)
                .unwrap_err()
                .contains("Unsupported macro")
        );
    }
    #[test]
    fn exact_transition_and_archive_binding_fail_closed() {
        let raw = nvm(0x082002);
        let a = image('a', 0x082002);
        let b = image('b', 0x082002);
        let archive = archive(&raw, a.clone(), ProfileId::Main);
        assert!(
            plan_with_registry(
                &archive,
                &raw,
                None,
                true,
                ProfileId::Main,
                &b,
                &raw,
                &[a.clone(), b.clone()],
                &[]
            )
            .is_err()
        );
        assert!(
            plan_with_registry(
                &archive,
                &raw,
                None,
                true,
                ProfileId::Main,
                &b,
                &raw,
                &[a.clone(), b.clone()],
                &[(a.ihx_sha256.clone(), b.ihx_sha256.clone())]
            )
            .is_ok()
        );
        let mut altered = raw.clone();
        altered[0x3f] = 1;
        assert!(
            plan_with_registry(
                &archive,
                &altered,
                None,
                true,
                ProfileId::Main,
                &a,
                &raw,
                &[a.clone()],
                &[]
            )
            .unwrap_err()
            .contains("archive")
        );
        assert!(
            plan_with_registry(
                &archive,
                &raw,
                None,
                true,
                ProfileId::Main,
                &a,
                &raw,
                &[a.clone(), a.clone()],
                &[]
            )
            .is_err()
        );
    }
    #[test]
    fn schema_migration_is_exact_and_preserves_combos_macro_and_core() {
        let mut raw = nvm(0x082002);
        raw[0x6a8..0x7f0].fill(0x95);
        raw[0x123] = 0xab;
        let a = image('a', 0x082002);
        let b = image('b', SCHEMA);
        let pairs = vec![(a.ihx_sha256.clone(), b.ihx_sha256.clone())];
        assert!(migrate_with_registry(&raw, &a, &b, &[a.clone(), b.clone()], &[]).is_err());
        let migrated =
            migrate_with_registry(&raw, &a, &b, &[a.clone(), b.clone()], &pairs).unwrap();
        assert_eq!(&migrated[..37], &raw[..37]);
        assert_eq!(&migrated[40..0x6a8], &raw[40..0x6a8]);
        assert_eq!(&migrated[0x7f0..], &raw[0x7f0..]);
        assert_eq!(
            Extensions::decode_nvm(&migrated).unwrap().settings,
            QmkSettings::default()
        );
        assert!(!transition_allowed(
            &b,
            &a,
            &[(b.ihx_sha256.clone(), a.ihx_sha256.clone())],
            &[a.clone(), b.clone()]
        ));
    }
    #[test]
    fn inaccessible_typed_fields_reject_and_default_blanks_remain_byte_exact() {
        let before = nvm(SCHEMA);
        let id = image('a', SCHEMA);
        let archive = archive(&before, id.clone(), ProfileId::Code);
        let p = plan_with_registry(
            &archive,
            &before,
            Some(ProfileId::Code),
            true,
            ProfileId::Code,
            &id,
            &before,
            &[id.clone()],
            &[],
        )
        .unwrap();
        assert!(p.steps().is_empty());
        assert_eq!(
            p.final_extensions(),
            Some(&Extensions::decode_nvm(&before).unwrap())
        );
        assert!(
            p.preconditions()
                .contains(&RestorePrecondition::QueriedActiveQsid {
                    expected: matrix_profile(ProfileId::Code)
                        .capabilities
                        .runtime_qmk_settings_ids
                        .clone()
                })
        );
        let mut source = before.clone();
        let mut extensions = Extensions::decode_nvm(&source).unwrap();
        extensions.settings.set(4, 321).unwrap();
        source[0x6a8..0x6c0].copy_from_slice(&extensions.settings.encode());
        let changed = super::tests::archive(&source, id.clone(), ProfileId::Code);
        assert!(
            plan_with_registry(
                &changed,
                &source,
                Some(ProfileId::Code),
                true,
                ProfileId::Code,
                &id,
                &before,
                &[id.clone()],
                &[]
            )
            .unwrap_err()
            .contains("Inactive QMK")
        );
        let mut blank = before.clone();
        blank[0x6a8..0x6c0].fill(255);
        let p = plan_with_registry(
            &archive,
            &before,
            Some(ProfileId::Code),
            true,
            ProfileId::Code,
            &id,
            &blank,
            &[id.clone()],
            &[],
        )
        .unwrap();
        assert_eq!(p.expected_nvm(), blank);
    }
}
