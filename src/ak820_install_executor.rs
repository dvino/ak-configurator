// SPDX-License-Identifier: GPL-3.0-or-later
//! One-attempt installation transaction. Only the native owner implements IO.
use crate::ak820_image_pack::{APP_END, APP_START};
use crate::ak820_installer::{CheckedLocalPackage, PreparedImageBytes, SnapshotKind};
use crate::ak820_loader::{self as loader, Command, PayloadReports};
use crate::ak820_settings_backup::{sha256, BackupResult};
use crate::app::ak820_beta1_profiles::ProfileId;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub(crate) enum InstallRequest {
    /// Build locally only after a fresh source read. Missing archived OEM bytes
    /// are permitted solely when that source itself is verified OEM firmware.
    BundledFirmware {
        profile: ProfileId,
        image: crate::ak820::ImageContract,
        recovery: Option<Vec<u8>>,
        settings: InstallSettingsChoice,
    },
    Firmware {
        profile: ProfileId,
        image: crate::ak820::ImageContract,
        payload: Vec<u8>,
        recovery: Option<Vec<u8>>,
        settings: InstallSettingsChoice,
    },
    OemRollback {
        recovery: Vec<u8>,
        provenance: RecoveryProvenance,
    },
}
pub(crate) fn preserved_nvm(
    source_kind: SnapshotKind,
    source: &[u8],
    target: ProfileId,
    target_image: crate::ak820::ImageContract,
) -> BackupResult<Vec<u8>> {
    if matches!(target_image, crate::ak820::ImageContract::Beta5(_)) {
        return preserved_beta5_nvm(source_kind, source, target, target_image);
    }
    if !matches!(target_image, crate::ak820::ImageContract::Beta3(p) if p == target) {
        return Err("Selected image does not authorize saved settings preservation".into());
    }
    let source_profile = match source_kind {
        SnapshotKind::Project(crate::ak820::ImageContract::Beta3(p)) => p,
        _ => return Err("Project settings preservation requires a verified beta.3 source; select defaults otherwise".into()),
    };
    let capture = crate::ak820_backup_capture::CapturedBackup::from_rom(
        source_profile,
        source.to_vec(),
        false,
    )?;
    let archive = crate::ak820_backup_files::validate_captured(&capture)?;
    let exact = crate::app::ak820_beta1_profiles::matrix_profile(target)
        .settings_identity
        .as_ref()
        .ok_or("Missing target settings identity")?;
    let identity = crate::ak820_settings_backup::ImageIdentity {
        ihx_sha256: exact.ihx_sha256.clone(),
        code_sha256: exact.code_sha256.clone(),
        definition_sha256: exact.definition_sha256.clone(),
        schema: exact.nvm_schema,
    };
    Ok(crate::ak820_restore_plan::plan_restore(
        &archive,
        capture.nvm(),
        target,
        &identity,
        &crate::ak820_release::bundled_defaults(target)?,
    )?
    .expected_nvm()
    .to_vec())
}

/// Preview metadata for an already successful `preserved_nvm` derivation.
/// The returned NVM and payload digests bind the actual materialized bytes.
pub(crate) fn preservation_materializes_beta4_qs(
    source_kind: SnapshotKind,
    source: &[u8],
    target_image: crate::ak820::ImageContract,
) -> bool {
    matches!(
        source_kind,
        SnapshotKind::Project(crate::ak820::ImageContract::Beta4(_))
    ) && matches!(target_image, crate::ak820::ImageContract::Beta5(_))
        && source
            .get(0xc700 + 0x6a8..0xc700 + 0x6c0)
            .is_some_and(|qs| qs.iter().all(|&b| b == 0) || qs.iter().all(|&b| b == 0xff))
}
/// Validate the exact source and target contracts before preserving schema03
/// bytes. This is installer preflight, not a restore plan or write authority.
fn preserved_beta5_nvm(
    source_kind: SnapshotKind,
    source: &[u8],
    target: ProfileId,
    target_image: crate::ak820::ImageContract,
) -> BackupResult<Vec<u8>> {
    use crate::ak820::ImageContract::{Beta4, Beta5};
    use crate::ak820_settings_model::{
        exact_qs_format, settings_from_nvm_for_formats_offline, OfflineSettingsCapabilities,
    };
    use crate::app::ak820_beta1_profiles::KeycodeSource;
    let source_image = match source_kind {
        SnapshotKind::Project(image @ (Beta4(_) | Beta5(_))) => image,
        _ => return Err("Beta.5 preservation requires an exact beta.4.1 or beta.5 source".into()),
    };
    if target_image != Beta5(target) {
        return Err("Preservation target differs from selected image".into());
    }
    let capture = crate::ak820_backup_capture::CapturedBackup::from_rom(
        source_image.profile().ok_or("Missing source profile")?,
        source.to_vec(),
        false,
    )?;
    if capture.image() != source_image {
        return Err("Captured source image differs from observed image".into());
    }
    let original = capture.nvm();
    let mut candidate = original.to_vec();
    // Beta.4's erased record selects QSID22 = 0; beta.5 selects 1.
    // Materialize the exact old default only for uniformly erased storage.
    if preservation_materializes_beta4_qs(source_kind, source, target_image) {
        let default = crate::ak820_qs_versioned_offline::SettingsRecord::defaults(
            crate::ak820_qs_versioned_offline::RecordVersion::V2,
        )?
        .encode();
        candidate[0x6a8..0x6c0].copy_from_slice(&default);
    }
    let nvm = candidate.as_slice();
    let src = source_image
        .matrix_profile()
        .ok_or("Missing source contract")?;
    let dst = target_image
        .matrix_profile()
        .ok_or("Missing target contract")?;
    let source_mode = source_image
        .saved_rgb_mode(nvm[24] >> 2)
        .ok_or("Unknown source saved RGB mode")?;
    let target_mode = target_image
        .saved_rgb_mode(nvm[24] >> 2)
        .ok_or("Unsupported target saved RGB mode")?;
    if source_mode != target_mode {
        return Err("Saved RGB mode has different target semantics".into());
    }
    let rgb = [
        if nvm[24] & 3 == 1 { source_mode } else { 0 },
        0,
        nvm[28],
        nvm[25],
        nvm[26],
        nvm[27],
    ];
    let decode =
        |image: crate::ak820::ImageContract, bytes: &[u8]| -> BackupResult<serde_json::Value> {
            let row = image.matrix_profile().ok_or("Missing settings contract")?;
            let exact = row
                .settings_identity
                .as_ref()
                .ok_or("Missing exact settings identity")?;
            let format = exact_qs_format(image)?;
            settings_from_nvm_for_formats_offline(
                bytes,
                &rgb,
                row.capabilities.dynamic_entries.combos > 0,
                format,
                format,
                OfflineSettingsCapabilities {
                    combo_media: exact.combo_media,
                },
            )
        };
    let before = decode(source_image, original)?;
    let after = decode(target_image, nvm)?;
    for id in &src.capabilities.runtime_qmk_settings_ids {
        let key = id.to_string();
        if !dst.capabilities.runtime_qmk_settings_ids.contains(id)
            || before["extensions"]["qmk_settings"][key.as_str()]
                != after["extensions"]["qmk_settings"][key.as_str()]
        {
            return Err(format!("QSID {id} changes under target image"));
        }
    }
    for id in &dst.capabilities.runtime_qmk_settings_ids {
        if !src.capabilities.runtime_qmk_settings_ids.contains(id) {
            // The exact beta.5 image supplies Leader's 300 ms default for an
            // older record. Its image version does not rewrite the QS bytes.
            if *id != 23 || after["extensions"]["qmk_settings"]["23"].as_u64() != Some(300) {
                return Err(format!("Newly active QSID {id} has no qualified default"));
            }
        }
    }
    let extensions = crate::ak820_typed_storage::NonQsExtensions::decode_nvm(nvm)?;
    for (i, entry) in extensions.tap_dance.iter().enumerate() {
        if *entry != Default::default()
            && (i >= src.capabilities.dynamic_entries.tap_dance as usize
                || i >= dst.capabilities.dynamic_entries.tap_dance as usize)
        {
            return Err("Nonempty Tap Dance is not usable in both images".into());
        }
    }
    for (i, entry) in extensions.key_overrides.iter().enumerate() {
        if *entry != Default::default()
            && (i >= src.capabilities.dynamic_entries.key_overrides as usize
                || i >= dst.capabilities.dynamic_entries.key_overrides as usize)
        {
            return Err("Nonempty Key Override is not usable in both images".into());
        }
    }
    for (i, raw) in nvm[0x658..0x6a8].chunks_exact(10).enumerate() {
        if raw.iter().all(|&b| b == 0) {
            continue;
        }
        if i >= src.capabilities.dynamic_entries.combos as usize
            || i >= dst.capabilities.dynamic_entries.combos as usize
        {
            return Err("Nonempty Combo is not usable in both images".into());
        }
        let word = |at| u16::from_le_bytes([raw[at], raw[at + 1]]);
        let supports = |image: crate::ak820::ImageContract| {
            let output = word(8);
            image.valid_combo([word(0), word(2), 0, 0], output)
                || ((0xa8..=0xc2).contains(&output)
                    && image
                        .matrix_profile()
                        .and_then(|r| r.settings_identity.as_ref())
                        .is_some_and(|i| i.combo_media)
                    && image.valid_combo([word(0), word(2), 0, 0], 4))
        };
        if !supports(source_image) || !supports(target_image) {
            return Err("Combo action is unsupported by source or target".into());
        }
    }
    for (field, kind) in [
        ("keys", KeycodeSource::Matrix),
        ("encoders", KeycodeSource::Encoder),
    ] {
        for item in before[field].as_array().ok_or("Missing assignments")? {
            let code = item["keycode"].as_u64().ok_or("Invalid assignment")? as u16;
            if !source_image.supports_keycode(code, kind)
                || !target_image.supports_keycode(code, kind)
            {
                return Err(format!("Unsupported {field} keycode {code:#06x}"));
            }
        }
    }
    for commands in before["macros"]["commands"]
        .as_array()
        .ok_or("Missing macros")?
    {
        for command in commands.as_array().ok_or("Invalid macro")? {
            let kind = match command["command"].as_str().unwrap_or("") {
                "tap" | "tap16" => KeycodeSource::MacroTap,
                "down" | "down16" => KeycodeSource::MacroDown,
                "up" | "up16" => KeycodeSource::MacroUp,
                _ => continue,
            };
            let hex = command["operands_hex"]
                .as_str()
                .ok_or("Invalid macro operands")?;
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| {
                    u8::from_str_radix(&hex[i..i + 2], 16)
                        .map_err(|_| "Invalid macro operands".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut code = match bytes.as_slice() {
                [a] => *a as u16,
                [a, b] => u16::from_le_bytes([*a, *b]),
                _ => return Err("Invalid macro operand width".into()),
            };
            if bytes.len() == 2 && code > 0xff00 {
                code = (code & 255) << 8;
            }
            if !source_image.supports_keycode(code, kind)
                || !target_image.supports_keycode(code, kind)
            {
                return Err(format!("Unsupported macro keycode {code:#06x}"));
            }
        }
    }
    Ok(nvm.to_vec())
}

fn require_install_transition(
    source: SnapshotKind,
    target: crate::ak820::ImageContract,
) -> BackupResult<()> {
    use crate::ak820::ImageContract::{Beta3, Beta4, Beta5};
    match (source, target) {
        (SnapshotKind::Oem, Beta3(_) | Beta4(_))
        | (SnapshotKind::Project(Beta3(_)), Beta3(_) | Beta4(_))
        | (SnapshotKind::Project(Beta4(_)), Beta4(_) | Beta5(_))
        | (SnapshotKind::Project(Beta5(_)), Beta5(_)) => Ok(()),
        _ => Err("Firmware update blocked: no approved exact source-to-target installation transition is available".into()),
    }
}

fn require_request_admission(
    request: &InstallRequest,
    source: SnapshotKind,
    admission: Option<&crate::ak820_release::ReleaseAdmission>,
) -> BackupResult<()> {
    if matches!(request, InstallRequest::OemRollback { .. }) && source == SnapshotKind::Oem {
        return Err(
            "OEM rollback blocked: the OEM source cannot provide the required physical hold".into(),
        );
    }
    if let InstallRequest::Firmware { profile, image, .. }
    | InstallRequest::BundledFirmware { profile, image, .. } = request
    {
        let admission = admission.ok_or("Firmware release admission is required")?;
        if admission.image() != *image || image.profile() != Some(*profile) {
            return Err("Selected firmware image differs from its release authorization".into());
        }
        require_install_transition(source, *image)?;
    }
    Ok(())
}

fn firmware_requires_hold(source: SnapshotKind) -> bool {
    source != SnapshotKind::Oem
}

fn bundled_recovery(
    source_kind: SnapshotKind,
    source: &[u8],
    supplied: Option<Vec<u8>>,
) -> BackupResult<Vec<u8>> {
    if source.len() != 65536 {
        return Err("Bundled preparation requires a complete fresh ROM".into());
    }
    let recovery=match supplied {
        Some(recovery)=>recovery,
        None=>match source_kind {
            SnapshotKind::Oem=>source.to_vec(),
            SnapshotKind::Project(_)=>return Err("Updating project firmware requires an existing personal OEM reference; the current project ROM cannot replace it".into()),
        },
    };
    crate::ak820_image_pack::validate_native_revision(&recovery)?;
    if source_kind == SnapshotKind::Oem && recovery != source {
        return Err("Initial OEM reference must equal the complete fresh device capture".into());
    }
    Ok(recovery)
}

/// Locally retained first-capture provenance, never an image revision claim.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryProvenance {
    format: String,
    recovery_sha256: String,
    owner_binding: String,
}
#[derive(Clone, Debug)]
pub(crate) enum InstallSettingsChoice {
    Defaults,
    PreservedInPayload { nvm_sha256: String },
}
#[derive(Clone, Debug)]
pub(crate) struct InstallationSummary {
    pub(crate) source_is_oem: bool,
    pub(crate) action: String,
    pub(crate) source_sha256: String,
    pub(crate) payload_sha256: String,
    pub(crate) expected_sha256: String,
    pub(crate) confirmation_pin: String,
    pub(crate) physical_hold: bool,
}
pub(crate) struct PreparedInstallation {
    images: PreparedImageBytes,
    summary: InstallationSummary,
    settings: InstallSettingsOutcome,
    provenance: Option<RecoveryProvenance>,
    target: Option<ProfileId>,
    release_metadata_sha: String,
}
impl PreparedInstallation {
    pub(crate) fn recovery_provenance(&self) -> Option<&RecoveryProvenance> {
        self.provenance.as_ref()
    }
    pub(crate) fn summary(&self) -> &InstallationSummary {
        &self.summary
    }
    pub(crate) fn settings_outcome(&self) -> &InstallSettingsOutcome {
        &self.settings
    }
    pub(crate) fn confirmation_pin(&self) -> &str {
        &self.summary.confirmation_pin
    }
    pub(crate) fn prepare(
        request: InstallRequest,
        source: Vec<u8>,
        admission: Option<&crate::ak820_release::ReleaseAdmission>,
        owner_binding: Option<String>,
    ) -> BackupResult<Self> {
        let release_pin = admission
            .map(|a| format!("{}:{}", a.version(), a.metadata_sha256()))
            .unwrap_or_else(|| "personal-oem-recovery".into());
        let source_kind = crate::ak820_installer::classify_snapshot(&source)?;
        require_request_admission(&request, source_kind, admission)?;
        if let InstallRequest::BundledFirmware {
            profile,
            image,
            recovery,
            settings,
        } = request
        {
            let recovery = match recovery {
                Some(recovery) => Some(bundled_recovery(source_kind, &source, Some(recovery))?),
                None if source_kind == SnapshotKind::Oem => {
                    Some(bundled_recovery(source_kind, &source, None)?)
                }
                None => None,
            };
            let nvm = match &settings {
                InstallSettingsChoice::Defaults => {
                    crate::ak820_release::bundled_defaults_for_image(image)?
                }
                InstallSettingsChoice::PreservedInPayload { nvm_sha256 } => {
                    if matches!(image, crate::ak820::ImageContract::Beta4(_)) {
                        return Err("Beta.4.1 does not authorize preserving saved settings".into());
                    }
                    let nvm = preserved_nvm(source_kind, &source, profile, image)?;
                    if sha256(&nvm) != *nvm_sha256 {
                        return Err(
                            "Prepared settings NVM pin differs from the checked preservation plan"
                                .into(),
                        );
                    }
                    nvm
                }
            };
            let payload = if let Some(recovery) = &recovery {
                crate::ak820_release::bundled_payload_for_image(image, recovery, Some(&nvm))?
            } else {
                let admission = admission.ok_or("Firmware release admission is required")?;
                if admission.image() != image {
                    return Err(
                        "Selected firmware image differs from its release authorization".into(),
                    );
                }
                crate::ak820_release::bundled_payload_from_project(admission, &source, Some(&nvm))?
            };
            return Self::prepare(
                InstallRequest::Firmware {
                    profile,
                    image,
                    payload,
                    recovery,
                    settings,
                },
                source,
                admission,
                owner_binding,
            );
        }

        let captured_provenance = if source_kind == SnapshotKind::Oem {
            owner_binding.as_ref().map(|binding| RecoveryProvenance {
                format: "ak820-oem-provenance-1".into(),
                recovery_sha256: sha256(&source),
                owner_binding: binding.clone(),
            })
        } else {
            None
        };
        let target = match &request {
            InstallRequest::Firmware { profile, .. } => Some(*profile),
            _ => None,
        };
        let (images, action, hold, settings) = match request {
            InstallRequest::BundledFirmware { .. } => {
                return Err("Bundled request was not normalized".into());
            }
            InstallRequest::Firmware {
                profile,
                image,
                payload,
                recovery,
                settings,
            } => {
                let admission = admission.ok_or("Firmware release admission is required")?;
                if admission.image() != image {
                    return Err(
                        "Selected firmware image differs from its release authorization".into(),
                    );
                }
                admission.validate_payload(profile, &payload)?;
                let package = CheckedLocalPackage::prepare_with_optional_recovery(
                    profile,
                    &payload,
                    &source,
                    recovery.as_deref(),
                    image,
                )?;
                if source_kind == SnapshotKind::Oem
                    && recovery
                        .as_ref()
                        .is_some_and(|_| package.recovery() != source.as_slice())
                {
                    return Err(
                        "Initial OEM reference must equal the complete fresh device capture".into(),
                    );
                }
                if matches!(image, crate::ak820::ImageContract::Beta4(_))
                    && !matches!(&settings, InstallSettingsChoice::Defaults)
                {
                    return Err("Beta.4.1 installation requires explicit image defaults; saved settings are not authorized".into());
                }
                // OEM has no project hold protocol; its pinned preview requires explicit confirmation.
                let hold = firmware_requires_hold(source_kind);
                let outcome = match settings {
                    InstallSettingsChoice::Defaults => {
                        if payload[0xc700 - APP_START..0xef00 - APP_START]
                            != crate::ak820_release::bundled_defaults_for_image(image)?
                        {
                            return Err("Payload does not contain the selected defaults".into());
                        }
                        InstallSettingsOutcome::DefaultsRequested
                    }
                    InstallSettingsChoice::PreservedInPayload { nvm_sha256 } => {
                        if sha256(&payload[0xc700 - APP_START..0xef00 - APP_START]) != nvm_sha256 {
                            return Err("Prepared settings NVM pin differs".into());
                        }
                        let preserved = preserved_nvm(source_kind, &source, profile, image)?;
                        if payload[0xc700 - APP_START..0xef00 - APP_START] != preserved {
                            return Err("Payload settings do not match the checked source/target preservation plan".into());
                        }
                        if matches!(image, crate::ak820::ImageContract::Beta5(_)) {
                            let mut candidate = source.clone();
                            candidate[APP_START..APP_END].copy_from_slice(&payload);
                            let verified = crate::ak820_backup_capture::CapturedBackup::from_rom(
                                profile, candidate, false,
                            )?;
                            if verified.image() != image || verified.nvm() != preserved {
                                return Err(
                                    "Packed target failed exact image and NVM validation".into()
                                );
                            }
                        }
                        InstallSettingsOutcome::PreparedImageSettingsVerified
                    }
                };
                (
                    PreparedImageBytes::Firmware(Box::new(package)),
                    format!("firmware:{}:{}", admission.version(), profile.as_str()),
                    hold,
                    outcome,
                )
            }
            InstallRequest::OemRollback {
                recovery,
                provenance,
            } => {
                let binding=owner_binding.as_ref().ok_or("OEM rollback requires a stable nonempty per-unit serial and physical USB port; this device cannot establish ownership")?;
                if provenance.format != "ak820-oem-provenance-1"
                    || provenance.owner_binding != *binding
                    || provenance.recovery_sha256 != sha256(&recovery)
                {
                    return Err("OEM recovery has no matching first-capture provenance for this keyboard and USB port".into());
                }
                crate::ak820_image_pack::validate_native_revision(&recovery)?;
                if source.len() != 65536
                    || recovery[..APP_START] != source[..APP_START]
                    || recovery[APP_END..] != source[APP_END..]
                {
                    return Err(
                        "OEM recovery protected regions differ from the fresh device".into(),
                    );
                }
                let payload = recovery[APP_START..APP_END].to_vec();
                let mut expected = source.clone();
                expected[APP_START..APP_END].copy_from_slice(&payload);
                (
                    PreparedImageBytes::Raw {
                        source,
                        payload,
                        expected,
                    },
                    "oem-rollback".into(),
                    true,
                    InstallSettingsOutcome::OemSettings,
                )
            }
        };
        let source_sha256 = sha256(images.source());
        let payload_sha256 = sha256(images.payload());
        let expected_sha256 = sha256(images.expected());
        let confirmation_pin = sha256(format!("ak820-install-1|{release_pin}|{action}|{source_sha256}|{payload_sha256}|{expected_sha256}|{settings:?}|{hold}").as_bytes());
        Ok(Self {
            images,
            settings,
            provenance: captured_provenance,
            target,
            release_metadata_sha: admission
                .map(|a| a.metadata_sha256().to_owned())
                .unwrap_or_default(),
            summary: InstallationSummary {
                source_is_oem: source_kind == SnapshotKind::Oem,
                action,
                source_sha256,
                payload_sha256,
                expected_sha256,
                confirmation_pin,
                physical_hold: hold,
            },
        })
    }
}
/// IO is deliberately unavailable to simulation events and generic HID requests.
pub(super) trait InstallTransport {
    fn read_rom(&mut self) -> BackupResult<Vec<u8>>;
    fn fresh_hold(&mut self, cancel: &AtomicBool) -> BackupResult<()>;
    fn transition(&mut self) -> BackupResult<()>;
    fn loader_command(&mut self, command: Command, checksum: u32) -> BackupResult<()>;
    fn loader_block(&mut self, report: &[u8; 65]) -> BackupResult<()>;
    fn reconnect(&mut self) -> BackupResult<()>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InstallFirmwareOutcome {
    NotAttempted,
    Unknown,
    Verified,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InstallSettingsOutcome {
    DefaultsRequested,
    PreparedImageSettingsVerified,
    OemSettings,
    Unverified,
}
#[derive(Clone, Debug)]
pub(crate) struct InstallationResult {
    pub(crate) firmware: InstallFirmwareOutcome,
    pub(crate) settings: InstallSettingsOutcome,
    pub(crate) error: Option<String>,
}
pub(super) fn execute(
    io: &mut impl InstallTransport,
    plan: &PreparedInstallation,
    admission: Option<&crate::ak820_release::ReleaseAdmission>,
    confirmed_pin: &str,
    cancel: &AtomicBool,
    progress: &AtomicU32,
    save_safety: impl FnOnce(&[u8]) -> BackupResult<()>,
) -> InstallationResult {
    let mut outcome = InstallFirmwareOutcome::NotAttempted;
    let check = || {
        if cancel.load(Ordering::Acquire) {
            Err("Installation cancelled before erase".to_string())
        } else {
            Ok(())
        }
    };
    let result = (|| -> BackupResult<()> {
        check()?;
        if let Some(target) = plan.target {
            let admission =
                admission.ok_or("Fresh release admission is required before execution")?;
            admission.validate_payload(target, plan.images.payload())?;
            if admission.metadata_sha256() != plan.release_metadata_sha {
                return Err("Release metadata changed since preview".into());
            }
        }
        if confirmed_pin != plan.confirmation_pin() {
            return Err("Installation confirmation does not match the displayed plan".into());
        }
        progress.store(0, Ordering::Release);
        if io.read_rom()? != plan.images.source() {
            return Err("Device changed since installation preview".into());
        }
        save_safety(plan.images.source())?;
        check()?;
        progress.store(10, Ordering::Release);
        if plan.summary.physical_hold {
            io.fresh_hold(cancel)?;
        }
        check()?;
        if io.read_rom()? != plan.images.source() {
            return Err("Device changed during confirmation".into());
        }
        io.transition()?;
        io.loader_command(Command::Identity, 0)?;
        check()?;
        progress.store(20, Ordering::Release);
        // From this boundary cancellation cannot interrupt the single attempt.
        outcome = InstallFirmwareOutcome::Unknown;
        io.loader_command(Command::Erase, 0)?;
        io.loader_command(Command::Begin, 0)?;
        for (index, block) in PayloadReports::new(plan.images.payload())?.enumerate() {
            io.loader_block(&block)?;
            progress.store(25 + ((index + 1) * 60 / 799) as u32, Ordering::Release);
        }
        io.loader_command(Command::Checksum, loader::checksum(plan.images.payload()))?;
        io.loader_command(Command::Restart, 0)?;
        io.reconnect()?;
        progress.store(90, Ordering::Release);
        plan.images.verify_full_readback(&io.read_rom()?)?;
        outcome = InstallFirmwareOutcome::Verified;
        progress.store(100, Ordering::Release);
        Ok(())
    })();
    InstallationResult {
        settings: if outcome == InstallFirmwareOutcome::Verified {
            plan.settings.clone()
        } else {
            InstallSettingsOutcome::Unverified
        },
        firmware: outcome,
        error: result.err(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        calls: Vec<String>,
        fail: Option<usize>,
        rom: Vec<u8>,
        after: Vec<u8>,
    }
    impl Fake {
        fn hit(&mut self, name: &str) -> BackupResult<()> {
            self.calls.push(name.into());
            if self.fail == Some(self.calls.len()) {
                Err("injected transport fault".into())
            } else {
                Ok(())
            }
        }
    }
    impl InstallTransport for Fake {
        fn read_rom(&mut self) -> BackupResult<Vec<u8>> {
            self.hit("read-rom")?;
            Ok(if self.calls.iter().any(|s| s == "reconnect") {
                self.after.clone()
            } else {
                self.rom.clone()
            })
        }
        fn fresh_hold(&mut self, _: &AtomicBool) -> BackupResult<()> {
            self.hit("fresh-hold")
        }
        fn transition(&mut self) -> BackupResult<()> {
            self.hit("transition")
        }
        fn loader_command(&mut self, c: Command, _: u32) -> BackupResult<()> {
            self.hit(&format!("{c:?}"))
        }
        fn loader_block(&mut self, _: &[u8; 65]) -> BackupResult<()> {
            self.hit("block")
        }
        fn reconnect(&mut self) -> BackupResult<()> {
            self.hit("reconnect")
        }
    }
    #[test]
    #[ignore = "requires AK820_PRIVATE_ROM_MODEL, a qualified offline full-ROM model"]
    fn beta5_preservation_keeps_exact_bytes_and_rejects_bad_records() {
        use crate::ak820::ImageContract::{Beta4, Beta5};
        let path = std::env::var("AK820_PRIVATE_ROM_MODEL")
            .expect("set AK820_PRIVATE_ROM_MODEL for this explicit offline test");
        let model = std::fs::read(path).expect("read the offline ROM model");
        assert_eq!(
            sha256(&model),
            "abdb30dedac76e3221024cb901e92e36c2d2ebfffd8985b6685486fc0c15a260"
        );
        let profile = ProfileId::Main;
        let admission = crate::ak820_release::ReleaseAdmission::beta4_1(profile, true).unwrap();
        let defaults = crate::ak820_release::bundled_defaults_for_image(Beta4(profile)).unwrap();
        let payload =
            crate::ak820_release::bundled_payload_from_project(&admission, &model, Some(&defaults))
                .unwrap();
        let mut source = model.clone();
        source[APP_START..APP_END].copy_from_slice(&payload);
        let actual = preserved_nvm(
            SnapshotKind::Project(Beta4(profile)),
            &source,
            profile,
            Beta5(profile),
        )
        .unwrap();
        assert!(actual[..0x6a8] == defaults[..0x6a8]);
        assert!(actual[0x6c0..] == defaults[0x6c0..]);
        // Literal pinned beta.4.1 C default and official Python V2 vector.
        let source_default = [
            0x51, 0x53, 0x02, 0x0f, 0x7f, 0, 0x28, 0, 0xc8, 0, 0, 0, 0x50, 0, 0xaf, 0, 1, 0, 0, 0,
            0, 0, 0x8e, 0x61,
        ];
        assert!(actual[0x6a8..0x6c0] == source_default);
        assert!(preservation_materializes_beta4_qs(
            SnapshotKind::Project(Beta4(profile)),
            &source,
            Beta5(profile)
        ));
        let mut persisted = source.clone();
        persisted[0xc700 + 0x6a8..0xc700 + 0x6c0]
            .copy_from_slice(&model[0xc700 + 0x6a8..0xc700 + 0x6c0]);
        assert!(persisted[0xc700 + 0x6aa] == 2);
        let persisted_nvm = preserved_nvm(
            SnapshotKind::Project(Beta4(profile)),
            &persisted,
            profile,
            Beta5(profile),
        )
        .unwrap();
        assert!(persisted_nvm == persisted[0xc700..0xef00]);
        assert!(!preservation_materializes_beta4_qs(
            SnapshotKind::Project(Beta4(profile)),
            &persisted,
            Beta5(profile)
        ));
        let mut cross_compatible = false;
        for row in &crate::app::ak820_beta1_profiles::beta5_firmware_matrix().profiles {
            if row.profile != profile
                && preserved_nvm(
                    SnapshotKind::Project(Beta4(profile)),
                    &source,
                    row.profile,
                    Beta5(row.profile),
                )
                .is_ok()
            {
                cross_compatible = true;
            }
        }
        assert!(
            cross_compatible,
            "at least one cross-profile target must preserve compatible data"
        );
        let mut bad = source.clone();
        bad[0xc700 + 0x6a8] ^= 0x80;
        assert!(preserved_nvm(
            SnapshotKind::Project(Beta4(profile)),
            &bad,
            profile,
            Beta5(profile)
        )
        .is_err());
        bad = source.clone();
        bad[0xc700 + 24] = (63 << 2) | 1;
        assert!(preserved_nvm(
            SnapshotKind::Project(Beta4(profile)),
            &bad,
            profile,
            Beta5(profile)
        )
        .is_err());
        let admission5 = crate::ak820_release::ReleaseAdmission::selected(
            crate::ak820_release::InstallerRelease::Beta5,
            profile,
            true,
        )
        .unwrap();
        let payload5 =
            crate::ak820_release::bundled_payload_from_project(&admission5, &source, Some(&actual))
                .unwrap();
        let mut source5 = source.clone();
        source5[APP_START..APP_END].copy_from_slice(&payload5);
        assert!(
            preserved_nvm(
                SnapshotKind::Project(Beta5(profile)),
                &source5,
                profile,
                Beta5(profile)
            )
            .unwrap()
                == actual
        );
        assert_eq!(actual[0x6aa], 2, "the persisted QS record remains V2");
    }
    #[test]
    fn directed_project_install_transitions_cover_all_profiles() {
        use crate::ak820::ImageContract::{Alpha3, Beta2, Beta3, Beta4, Beta5};
        for source in crate::app::ak820_beta1_profiles::profiles() {
            for target in crate::app::ak820_beta1_profiles::profiles() {
                let s = source.profile;
                let t = target.profile;
                for from in [Alpha3, Beta2(s), Beta3(s), Beta4(s), Beta5(s)] {
                    for to in [Alpha3, Beta2(t), Beta3(t), Beta4(t), Beta5(t)] {
                        let allowed = matches!(
                            (from, to),
                            (Beta3(_), Beta3(_) | Beta4(_))
                                | (Beta4(_), Beta4(_) | Beta5(_))
                                | (Beta5(_), Beta5(_))
                        );
                        assert_eq!(
                            require_install_transition(SnapshotKind::Project(from), to).is_ok(),
                            allowed,
                            "{from:?} -> {to:?}"
                        );
                        assert_eq!(
                            require_install_transition(SnapshotKind::Oem, to).is_ok(),
                            matches!(to, Beta3(_) | Beta4(_))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn beta5_admits_exactly_two_hundred_directed_profile_pairs() {
        use crate::ak820::ImageContract::{Beta3, Beta4, Beta5};
        let rows = &crate::app::ak820_beta1_profiles::beta5_firmware_matrix().profiles;
        let mut allowed = 0;
        for source in rows {
            for target in rows {
                for image in [Beta4(source.profile), Beta5(source.profile)] {
                    assert!(require_install_transition(
                        SnapshotKind::Project(image),
                        Beta5(target.profile)
                    )
                    .is_ok());
                    allowed += 1;
                }
                assert!(require_install_transition(
                    SnapshotKind::Project(Beta3(source.profile)),
                    Beta5(target.profile)
                )
                .is_err());
                assert!(require_install_transition(
                    SnapshotKind::Project(Beta5(source.profile)),
                    Beta4(target.profile)
                )
                .is_err());
            }
        }
        assert_eq!(allowed, 200);
    }

    #[test]
    fn both_firmware_requests_require_exact_admission_and_directed_source() {
        use crate::ak820::ImageContract::{Beta2, Beta3, Beta4};
        for target in crate::app::ak820_beta1_profiles::profiles() {
            let t = target.profile;
            assert!(crate::ak820_release::ReleaseAdmission::bundled(t, true).is_err());
            let admission = crate::ak820_release::ReleaseAdmission::beta4_1(t, true).unwrap();
            for source in crate::app::ak820_beta1_profiles::profiles() {
                let s = source.profile;
                for bundled in [false, true] {
                    let request = |profile, image| {
                        if bundled {
                            InstallRequest::BundledFirmware {
                                profile,
                                image,
                                recovery: None,
                                settings: InstallSettingsChoice::Defaults,
                            }
                        } else {
                            InstallRequest::Firmware {
                                profile,
                                image,
                                payload: vec![],
                                recovery: None,
                                settings: InstallSettingsChoice::Defaults,
                            }
                        }
                    };
                    let good = request(t, Beta4(t));
                    for from in [
                        SnapshotKind::Oem,
                        SnapshotKind::Project(Beta3(s)),
                        SnapshotKind::Project(Beta4(s)),
                    ] {
                        assert!(require_request_admission(&good, from, Some(&admission)).is_ok());
                    }
                    assert!(require_request_admission(
                        &good,
                        SnapshotKind::Project(Beta2(s)),
                        Some(&admission)
                    )
                    .is_err());
                    assert!(require_request_admission(&good, SnapshotKind::Oem, None).is_err());
                    assert!(require_request_admission(
                        &request(t, Beta3(t)),
                        SnapshotKind::Oem,
                        Some(&admission)
                    )
                    .is_err());
                    if s != t {
                        assert!(require_request_admission(
                            &request(s, Beta4(t)),
                            SnapshotKind::Oem,
                            Some(&admission)
                        )
                        .is_err());
                        assert!(require_request_admission(
                            &request(s, Beta4(s)),
                            SnapshotKind::Oem,
                            Some(&admission)
                        )
                        .is_err());
                    }
                }
            }
            assert!(admission.validate_payload(t, &[]).is_err());
        }
    }

    fn fixture() -> PreparedInstallation {
        // Private engine-only fixture: cannot create a real native transport.
        let source = vec![0; 65536];
        let payload = vec![1; APP_END - APP_START];
        let mut expected = source.clone();
        expected[APP_START..APP_END].copy_from_slice(&payload);
        PreparedInstallation {
            images: PreparedImageBytes::Raw {
                source,
                payload,
                expected,
            },
            settings: InstallSettingsOutcome::DefaultsRequested,
            provenance: None,
            target: None,
            release_metadata_sha: String::new(),
            summary: InstallationSummary {
                source_is_oem: false,
                action: "synthetic".into(),
                source_sha256: String::new(),
                payload_sha256: String::new(),
                expected_sha256: String::new(),
                confirmation_pin: "fixture".into(),
                physical_hold: true,
            },
        }
    }
    fn fake(p: &PreparedInstallation) -> Fake {
        Fake {
            calls: vec![],
            fail: None,
            rom: p.images.source().to_vec(),
            after: p.images.expected().to_vec(),
        }
    }
    #[test]
    fn oem_exception_and_project_hold_stop_before_transition_on_failure() {
        let rollback = InstallRequest::OemRollback {
            recovery: vec![],
            provenance: RecoveryProvenance {
                format: String::new(),
                recovery_sha256: String::new(),
                owner_binding: String::new(),
            },
        };
        assert!(require_request_admission(&rollback, SnapshotKind::Oem, None).is_err());
        assert!(require_request_admission(
            &rollback,
            SnapshotKind::Project(crate::ak820::ImageContract::Beta3(ProfileId::Main)),
            None
        )
        .is_ok());

        // Synthetic plans exercise execution without claiming synthetic ROM identity.
        for oem in [true, false] {
            let mut p = fixture();
            p.summary.source_is_oem = oem;
            let source = if oem {
                SnapshotKind::Oem
            } else {
                SnapshotKind::Project(crate::ak820::ImageContract::Beta3(ProfileId::Main))
            };
            p.summary.physical_hold = firmware_requires_hold(source);
            assert_eq!(p.summary.physical_hold, !oem);
            for pin in ["wrong", "stale"] {
                let mut io = fake(&p);
                let result = execute(
                    &mut io,
                    &p,
                    None,
                    pin,
                    &AtomicBool::new(false),
                    &AtomicU32::new(0),
                    |_| panic!("unconfirmed safety save"),
                );
                assert_eq!(result.firmware, InstallFirmwareOutcome::NotAttempted);
                assert!(io.calls.is_empty());
            }
            let mut io = fake(&p);
            if !oem {
                io.fail = Some(2);
            }
            let mut saved = false;
            let result = execute(
                &mut io,
                &p,
                None,
                "fixture",
                &AtomicBool::new(false),
                &AtomicU32::new(0),
                |rom| {
                    assert_eq!(rom, p.images.source());
                    saved = true;
                    Ok(())
                },
            );
            assert!(saved);
            if oem {
                assert_eq!(result.firmware, InstallFirmwareOutcome::Verified);
                assert!(!io.calls.iter().any(|s| s == "fresh-hold"));
                assert!(io.calls.iter().any(|s| s == "Erase"));
            } else {
                assert_eq!(result.firmware, InstallFirmwareOutcome::NotAttempted);
                assert_eq!(io.calls, ["read-rom", "fresh-hold"]);
                assert!(!io.calls.iter().any(|s| s == "transition" || s == "Erase"));
            }
        }
    }
    #[test]
    fn successful_order_and_every_fault_stop_without_replay() {
        let p = fixture();
        let mut io = fake(&p);
        let progress = AtomicU32::new(0);
        let cancel = AtomicBool::new(false);
        let result = execute(&mut io, &p, None, "fixture", &cancel, &progress, |rom| {
            assert_eq!(rom, p.images.source());
            Ok(())
        });
        assert_eq!(result.firmware, InstallFirmwareOutcome::Verified);
        assert!(result.error.is_none());
        assert_eq!(
            &io.calls[..7],
            &[
                "read-rom",
                "fresh-hold",
                "read-rom",
                "transition",
                "Identity",
                "Erase",
                "Begin"
            ]
        );
        assert_eq!(
            io.calls.iter().filter(|s| s.as_str() == "block").count(),
            799
        );
        assert_eq!(
            &io.calls[806..],
            &["Checksum", "Restart", "reconnect", "read-rom"]
        );
        for fault in 1..=io.calls.len() {
            let mut bad = fake(&p);
            bad.fail = Some(fault);
            let result = execute(
                &mut bad,
                &p,
                None,
                "fixture",
                &cancel,
                &progress,
                |_| Ok(()),
            );
            assert!(result.error.is_some(), "fault {fault}");
            assert_ne!(result.firmware, InstallFirmwareOutcome::Verified);
            assert_eq!(
                bad.calls.len(),
                fault,
                "no continuation or retry at fault {fault}"
            );
        }
    }
    #[test]
    fn safety_failure_confirmation_cancel_and_changed_source_precede_transition() {
        let p = fixture();
        let progress = AtomicU32::new(0);
        for case in 0..4 {
            let mut io = fake(&p);
            if case == 3 {
                io.rom[0] ^= 1;
            }
            let result = execute(
                &mut io,
                &p,
                None,
                if case == 0 { "stale" } else { "fixture" },
                &AtomicBool::new(case == 1),
                &progress,
                |_| {
                    if case == 2 {
                        Err("save failed".into())
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.error.is_some());
            assert_eq!(result.firmware, InstallFirmwareOutcome::NotAttempted);
            assert!(!io.calls.iter().any(|s| s == "transition"));
        }
    }
    #[test]
    fn bundled_preparation_has_no_unverified_oem_reference_fallback() {
        let source = vec![0; 65536];
        let error = bundled_recovery(
            SnapshotKind::Project(crate::ak820::ImageContract::Beta3(ProfileId::Main)),
            &source,
            None,
        )
        .unwrap_err();
        assert!(error.contains("existing personal OEM reference"));
        for size in [0, 1, 65535, 65537] {
            assert!(bundled_recovery(SnapshotKind::Oem, &vec![0; size], None).is_err());
        }
        // Synthetic bytes never become an accepted native revision, whether
        // supplied explicitly or selected as a no-archive source candidate.
        assert!(bundled_recovery(SnapshotKind::Oem, &source, None).is_err());
        assert!(bundled_recovery(SnapshotKind::Oem, &source, Some(vec![0; 65536])).is_err());
        assert!(bundled_recovery(
            SnapshotKind::Project(crate::ak820::ImageContract::Beta3(ProfileId::Main)),
            &source,
            Some(vec![])
        )
        .is_err());
        assert!(preserved_nvm(
            SnapshotKind::Oem,
            &source,
            ProfileId::Main,
            crate::ak820::ImageContract::Beta3(ProfileId::Main)
        )
        .is_err());
        let request = InstallRequest::BundledFirmware {
            profile: ProfileId::Main,
            image: crate::ak820::ImageContract::Beta3(ProfileId::Main),
            recovery: None,
            settings: InstallSettingsChoice::Defaults,
        };
        assert!(PreparedInstallation::prepare(request, source, None, None).is_err());
    }
    #[test]
    fn firmware_execution_cannot_omit_current_release_admission() {
        let mut p = fixture();
        p.target = Some(ProfileId::Main);
        let mut io = fake(&p);
        let result = execute(
            &mut io,
            &p,
            None,
            "fixture",
            &AtomicBool::new(false),
            &AtomicU32::new(0),
            |_| panic!("must not save"),
        );
        assert_eq!(result.firmware, InstallFirmwareOutcome::NotAttempted);
        assert!(result.error.unwrap().contains("Fresh release admission"));
        assert!(io.calls.is_empty());
    }
    #[test]
    fn complete_readback_checks_protected_bytes_and_short_read() {
        let p = fixture();
        for offset in [0, 0x27c0, APP_START, 0xc700, APP_END, 65535, 65536] {
            let mut io = fake(&p);
            if offset == 65536 {
                io.after.pop();
            } else {
                io.after[offset] ^= 1;
            }
            let result = execute(
                &mut io,
                &p,
                None,
                "fixture",
                &AtomicBool::new(false),
                &AtomicU32::new(0),
                |_| Ok(()),
            );
            assert_eq!(result.firmware, InstallFirmwareOutcome::Unknown);
            assert!(result.error.is_some());
            assert_eq!(io.calls.last().unwrap(), "read-rom");
        }
    }
}
