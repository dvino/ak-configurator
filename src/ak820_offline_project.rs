//! Portable local example projects. This format is deliberately unrelated to
//! firmware backup archives: it contains no OEM ROM, device identity or restore
//! authorization. Canonical geometry comes from the bundled example template.
use crate::ak820_typed_storage::{Extensions, KeyOverride, QmkSettings, TapDance, SCHEMA};
use crate::app::ak820_beta1_profiles::{self as beta, KeycodeSource, ProfileId};
use crate::app::ak820_offline_extensions::OfflineExtensions;
use crate::app::{Ak820OfflineProfileProject, ComboEntry};
use crate::keyboard::KeyBinding;
use crate::keycode_picker::MacroAction;
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, String>;
const FORMAT: &str = "ak820-local-offline-example-project";
const MAX_JSON: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Project {
    format: String,
    version: u32,
    target_release: String,
    profile_id: String,
    source_catalog_sha256: String,
    source_ihx_sha256: String,
    physical_keys: Vec<[u8; 2]>,
    encoder_directions: Vec<[u8; 2]>,
    layers: Vec<Vec<u16>>,
    encoders: Vec<Vec<u16>>,
    macros: Vec<Macro>,
    combos: Vec<Combo>,
    combo_term: Option<u16>,
    layer_names: Vec<String>,
    rgb: Rgb,
    extensions: Option<Records>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Macro {
    stream: Vec<u8>,
    actions: Vec<Action>,
    name: String,
    description: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    deny_unknown_fields,
    rename_all = "camelCase"
)]
enum Action {
    Text(String),
    Tap(u16),
    Down(u16),
    Up(u16),
    Delay(u16),
    Raw(Vec<u8>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Combo {
    keys: [u16; 4],
    output: u16,
    layer: Option<u8>,
    name: String,
    color: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Rgb {
    effect: u16,
    brightness: u8,
    speed: u8,
    hue: u8,
    saturation: u8,
    last_enabled_effect: u16,
}
/// Only typed records are portable. Synthetic carrier bytes are never exported.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Records {
    schema: u32,
    settings: Vec<u8>,
    tap_dance: Vec<Vec<u8>>,
    key_overrides: Vec<Vec<u8>>,
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn vial(value: KeyBinding) -> Result<u16> {
    match value {
        KeyBinding::Vial(k) => Ok(k),
        _ => Err("Offline examples require Vial keycodes".into()),
    }
}
fn parse(raw: &[u8]) -> Result<Project> {
    require(raw.len() <= MAX_JSON, "Project exceeds size limit")?;
    let value = crate::ak820_settings_backup::parse_json(raw)?;
    if value.get("format").and_then(serde_json::Value::as_str) == Some(FORMAT)
        && value.get("version").and_then(serde_json::Value::as_u64) == Some(1)
    {
        return Err("Offline project version 1 is a beta.1 format and cannot be migrated automatically; open an existing v1 file in a compatible beta.1 application and export its contents manually".into());
    }
    let project: Project = serde_json::from_value(value).map_err(|e| e.to_string())?;
    require(
        project.format == FORMAT,
        "Unsupported offline project format",
    )?;
    require(project.version == 2, "Unsupported offline project version")?;
    require(
        project.target_release == beta::BETA_RELEASE,
        "Offline project targets a different firmware release",
    )?;
    require(
        project.source_catalog_sha256 == beta::SOURCE_CATALOG_SHA256,
        "Project firmware catalog identity differs",
    )?;
    let profile = id(&project.profile_id)?;
    require(
        project.source_ihx_sha256 == beta::profile(profile).build_identity.ihx_sha256,
        "Project image identity differs from this profile",
    )?;
    require(
        serde_json::to_vec_pretty(&project)
            .map_err(|e| e.to_string())?
            .len()
            <= MAX_JSON,
        "Project exceeds normalized size limit",
    )?;
    Ok(project)
}
fn id(raw: &str) -> Result<ProfileId> {
    beta::profiles()
        .iter()
        .find(|p| p.profile.as_str() == raw)
        .map(|p| p.profile)
        .ok_or_else(|| "Unknown offline profile".into())
}
/// Read identity before constructing the canonical bundled profile template.
pub(crate) fn profile_id(raw: &[u8]) -> Result<ProfileId> {
    id(&parse(raw)?.profile_id)
}
fn check_code(profile: ProfileId, code: u16, source: KeycodeSource) -> Result<()> {
    require(
        beta::supports_keycode(profile, code, source),
        "Keycode is unsupported for profile/source",
    )
}
fn validate_stream(profile: ProfileId, stream: &[u8]) -> Result<()> {
    let mut terminated = stream.to_vec();
    terminated.extend([0; 16]);
    require(
        crate::ak820_settings_backup::macro_logical_length(&terminated)? == stream.len() + 16,
        "Embedded macro terminator",
    )?;
    let mut at = 0;
    while at < stream.len() {
        if stream[at] != 1 {
            at += 1;
            continue;
        }
        let opcode = stream[at + 1];
        if opcode == 4 {
            at += 4;
            continue;
        }
        let width = if opcode <= 3 { 1 } else { 2 };
        let mut code = stream[at + 2] as u16;
        if width == 2 {
            code |= (stream[at + 3] as u16) << 8;
            if code >> 8 == 255 {
                code = (code & 255) << 8;
            }
        }
        let source = match opcode {
            1 | 5 => KeycodeSource::MacroTap,
            2 | 6 => KeycodeSource::MacroDown,
            _ => KeycodeSource::MacroUp,
        };
        check_code(profile, code, source)?;
        at += 2 + width;
    }
    Ok(())
}
impl Action {
    fn from_action(a: &MacroAction) -> Self {
        match a {
            MacroAction::Text(v) => Self::Text(v.clone()),
            MacroAction::Tap(v) => Self::Tap(*v),
            MacroAction::Down(v) => Self::Down(*v),
            MacroAction::Up(v) => Self::Up(*v),
            MacroAction::Delay(v) => Self::Delay(*v),
            MacroAction::Raw(v) => Self::Raw(v.clone()),
        }
    }
    fn action(&self) -> MacroAction {
        match self {
            Self::Text(v) => MacroAction::Text(v.clone()),
            Self::Tap(v) => MacroAction::Tap(*v),
            Self::Down(v) => MacroAction::Down(*v),
            Self::Up(v) => MacroAction::Up(*v),
            Self::Delay(v) => MacroAction::Delay(*v),
            Self::Raw(v) => MacroAction::Raw(v.clone()),
        }
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        let mut out = vec![];
        match self {
            Self::Text(v) => {
                require(
                    v.bytes().all(|b| (2..128).contains(&b)),
                    "Macro text must be non-control ASCII",
                )?;
                out.extend(v.bytes());
            }
            Self::Raw(v) => out.extend(v),
            Self::Delay(ms) => {
                require(*ms <= 65024, "Macro delay exceeds wire capacity")?;
                out.extend([1, 4, (*ms % 255 + 1) as u8, (*ms / 255 + 1) as u8]);
            }
            Self::Tap(k) | Self::Down(k) | Self::Up(k) => {
                let op = match self {
                    Self::Tap(_) => 1,
                    Self::Down(_) => 2,
                    _ => 3,
                };
                if *k < 256 {
                    out.extend([1, op, *k as u8]);
                } else {
                    let wire = if k & 255 == 0 { 0xff00 | (k >> 8) } else { *k };
                    out.extend([1, op + 4]);
                    out.extend(wire.to_le_bytes());
                }
            }
        }
        Ok(out)
    }
}
fn carrier() -> Vec<u8> {
    let mut nvm = vec![0; 0x2800];
    nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
    nvm[0x6a8..0x6c0].copy_from_slice(&QmkSettings::default().encode());
    nvm
}
impl Records {
    fn from_nvm(nvm: &[u8]) -> Result<Self> {
        let v = Extensions::decode_nvm(nvm)?;
        Ok(Self {
            schema: SCHEMA,
            settings: v.settings.encode().to_vec(),
            tap_dance: v
                .tap_dance
                .iter()
                .map(|v| v.encode().map(|b| b.to_vec()))
                .collect::<Result<_>>()?,
            key_overrides: v
                .key_overrides
                .iter()
                .map(|v| v.encode().map(|b| b.to_vec()))
                .collect::<Result<_>>()?,
        })
    }
    fn candidate(&self, profile: ProfileId) -> Result<Vec<u8>> {
        require(
            self.schema == SCHEMA && self.tap_dance.len() == 8 && self.key_overrides.len() == 8,
            "Invalid typed record schema/count",
        )?;
        let settings = QmkSettings::decode(&self.settings)?;
        require(
            self.settings == settings.encode(),
            "Noncanonical QMK settings record",
        )?;
        let mut v = Extensions::decode_nvm(&carrier())?;
        v.settings = settings;
        for i in 0..8 {
            v.tap_dance[i] = TapDance::decode(&self.tap_dance[i])?;
            v.key_overrides[i] = KeyOverride::decode(&self.key_overrides[i])?;
        }
        // Inactive records must equal canonical defaults, including timeout QSID4.
        v.apply_to_nvm(&carrier(), profile)
    }
}
impl Project {
    fn validate(&self, template: &Ak820OfflineProfileProject) -> Result<()> {
        let profile = id(&self.profile_id)?;
        require(profile == template.profile, "Template profile differs")?;
        require(
            self.physical_keys
                == template
                    .layout
                    .keys
                    .iter()
                    .map(|k| [k.row, k.col])
                    .collect::<Vec<_>>()
                && self.physical_keys.len() == 83,
            "Physical key order/matrix differs",
        )?;
        require(
            self.encoder_directions
                == template
                    .layout
                    .encoders
                    .iter()
                    .map(|e| [e.encoder_idx, e.direction])
                    .collect::<Vec<_>>()
                && self.encoder_directions.len() == 2,
            "Encoder direction order differs",
        )?;
        require(
            self.layers.len() == 6
                && self.encoders.len() == 6
                && self.layer_names.len() == 6
                && self.macros.len() == 16
                && self.combos.len() == 8,
            "Invalid project record counts",
        )?;
        for layer in &self.layers {
            require(layer.len() == 83, "Invalid matrix layer length")?;
            for &k in layer {
                check_code(profile, k, KeycodeSource::Matrix)?;
            }
        }
        for layer in &self.encoders {
            require(layer.len() == 2, "Invalid encoder layer length")?;
            for &k in layer {
                check_code(profile, k, KeycodeSource::Encoder)?;
            }
        }
        let mut stream_capacity = 16;
        let mut action_capacity = 16;
        for m in &self.macros {
            require(
                m.name.chars().count() <= 7 && m.description.chars().count() <= 120,
                "Macro metadata exceeds limits",
            )?;
            validate_stream(profile, &m.stream)?;
            stream_capacity += m.stream.len();
            let mut encoded = vec![];
            for a in &m.actions {
                encoded.extend(a.bytes()?);
            }
            validate_stream(profile, &encoded)?;
            action_capacity += encoded.len();
        }
        require(
            stream_capacity <= 8192 && action_capacity <= 8192,
            "Macros exceed profile capacity",
        )?;
        require(
            matches!(self.combo_term, None | Some(40)),
            "Offline Combo term is fixed",
        )?;
        for c in &self.combos {
            let [a, b, x, y] = c.keys;
            let empty = c.keys == [0; 4] && c.output == 0;
            require(
                c.layer.is_none()
                    && x == 0
                    && y == 0
                    && (empty
                        || ((4..=0xa4).contains(&a)
                            && (4..=0xa4).contains(&b)
                            && a != b
                            && c.output < 0x2000
                            && ((4..=0xa4).contains(&(c.output & 255))
                                || (0xe0..=0xe7).contains(&(c.output & 255))))),
                "Unsupported Combo record",
            )?;
        }
        let effects = beta::rgb_modes(profile)
            .iter()
            .map(|(_, wire)| *wire)
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        require(
            self.rgb.brightness <= 160
                && effects.contains(&self.rgb.effect)
                && effects
                    .iter()
                    .any(|mode| *mode != 0 && *mode == self.rgb.last_enabled_effect),
            "Invalid saved RGB values",
        )?;
        require(
            self.extensions.is_some() == (beta::profile(profile).settings_schema == SCHEMA),
            "Typed records do not match profile schema",
        )?;
        if let Some(records) = &self.extensions {
            records.candidate(profile)?;
        }
        Ok(())
    }
}
/// Export only saved example fields; caller retains any unsaved editor draft.
pub(crate) fn export_project(p: &Ak820OfflineProfileProject) -> Result<Vec<u8>> {
    require(
        !p.extensions
            .as_ref()
            .is_some_and(OfflineExtensions::has_invalid_draft),
        "Correct or discard invalid module draft before saving",
    )?;
    require(
        p.macro_texts.len() == 16
            && p.macro_actions.len() == 16
            && p.macro_names.len() == 16
            && p.macro_descriptions.len() == 16
            && p.combo_entries.len() == 8
            && p.combo_names.len() == 8
            && p.combo_colors.len() == 8,
        "Incomplete in-memory example",
    )?;
    let doc = Project {
        format: FORMAT.into(),
        version: 2,
        target_release: beta::BETA_RELEASE.into(),
        profile_id: p.profile.as_str().into(),
        source_catalog_sha256: beta::SOURCE_CATALOG_SHA256.into(),
        source_ihx_sha256: beta::profile(p.profile).build_identity.ihx_sha256.clone(),
        physical_keys: p.layout.keys.iter().map(|k| [k.row, k.col]).collect(),
        encoder_directions: p
            .layout
            .encoders
            .iter()
            .map(|e| [e.encoder_idx, e.direction])
            .collect(),
        layers: p
            .layout
            .layers
            .iter()
            .map(|l| l.iter().copied().map(vial).collect())
            .collect::<Result<_>>()?,
        encoders: p.layout.encoder_layers.clone(),
        macros: (0..16)
            .map(|i| Macro {
                stream: p.macro_texts[i].clone(),
                actions: p.macro_actions[i].iter().map(Action::from_action).collect(),
                name: p.macro_names[i].clone(),
                description: p.macro_descriptions[i].clone(),
            })
            .collect(),
        combos: (0..8)
            .map(|i| {
                Ok(Combo {
                    keys: p.combo_entries[i].keys,
                    output: vial(p.combo_entries[i].output)?,
                    layer: p.combo_entries[i].layer,
                    name: p.combo_names[i].clone(),
                    color: p.combo_colors[i],
                })
            })
            .collect::<Result<_>>()?,
        combo_term: p.combo_term,
        layer_names: p.layer_names.clone(),
        rgb: Rgb {
            effect: p.rgb_settings.effect,
            brightness: p.rgb_settings.brightness,
            speed: p.rgb_settings.speed,
            hue: p.rgb_settings.hue,
            saturation: p.rgb_settings.saturation,
            last_enabled_effect: p.rgb_settings.last_enabled_effect,
        },
        extensions: p
            .extensions
            .as_ref()
            .map(|e| Records::from_nvm(e.nvm()))
            .transpose()?,
    };
    doc.validate(p)?;
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    require(bytes.len() <= MAX_JSON, "Project exceeds size limit")?;
    Ok(bytes)
}
/// Import transactionally using the bundled template for this exact profile.
/// The returned value is an offline example and confers no live authorization.
pub(crate) fn import_project(
    raw: &[u8],
    mut template: Ak820OfflineProfileProject,
) -> Result<Ak820OfflineProfileProject> {
    let doc = parse(raw)?;
    doc.validate(&template)?;
    template.layout.layers = doc
        .layers
        .into_iter()
        .map(|l| l.into_iter().map(KeyBinding::Vial).collect())
        .collect();
    template.layout.encoder_layers = doc.encoders;
    template.macro_texts = doc.macros.iter().map(|m| m.stream.clone()).collect();
    template.macro_actions = doc
        .macros
        .iter()
        .map(|m| m.actions.iter().map(Action::action).collect())
        .collect();
    template.macro_names = doc.macros.iter().map(|m| m.name.clone()).collect();
    template.macro_descriptions = doc.macros.into_iter().map(|m| m.description).collect();
    template.combo_entries = doc
        .combos
        .iter()
        .map(|c| ComboEntry {
            keys: c.keys,
            output: c.output.into(),
            layer: c.layer,
        })
        .collect();
    template.combo_names = doc.combos.iter().map(|c| c.name.clone()).collect();
    template.combo_colors = doc.combos.iter().map(|c| c.color).collect();
    template.combo_term = doc.combo_term;
    template.layer_names = doc.layer_names;
    template.rgb_settings.effect = doc.rgb.effect;
    template.rgb_settings.brightness = doc.rgb.brightness;
    template.rgb_settings.speed = doc.rgb.speed;
    template.rgb_settings.hue = doc.rgb.hue;
    template.rgb_settings.saturation = doc.rgb.saturation;
    template.rgb_settings.last_enabled_effect = doc.rgb.last_enabled_effect;
    template.extensions = doc
        .extensions
        .map(|r| {
            r.candidate(template.profile)
                .and_then(|n| OfflineExtensions::from_nvm(&n))
        })
        .transpose()?;
    Ok(template)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture(profile: ProfileId) -> Ak820OfflineProfileProject {
        let value = serde_json::from_str(include_str!("../assets/ak820/layout.json")).unwrap();
        let mut layout = crate::keyboard::KeyboardLayout::from_vial_json(&value).unwrap();
        layout.layers = vec![vec![KeyBinding::Vial(4); 83]; 6];
        layout.encoder_layers = vec![vec![4; 2]; 6];
        Ak820OfflineProfileProject {
            profile,
            layout,
            macro_texts: vec![b"sample".to_vec(); 16],
            macro_actions: vec![vec![MacroAction::Tap(4), MacroAction::Delay(250)]; 16],
            macro_names: vec!["M".into(); 16],
            macro_descriptions: vec!["description".into(); 16],
            combo_entries: vec![ComboEntry::default(); 8],
            combo_names: vec!["Combo".into(); 8],
            combo_colors: vec![0; 8],
            combo_term: Some(40),
            layer_names: vec!["Layer".into(); 6],
            rgb_settings: crate::app::RgbSettingsState {
                effect: 2,
                last_enabled_effect: 2,
                brightness: 160,
                ..Default::default()
            },
            extensions: if beta::profile(profile).settings_schema == SCHEMA {
                Some(OfflineExtensions::from_nvm(&carrier()).unwrap())
            } else {
                None
            },
        }
    }
    #[test]
    fn all_nine_examples_roundtrip_and_strict_mutations() {
        for image in beta::profiles() {
            let p = fixture(image.profile);
            let bytes = export_project(&p).unwrap();
            assert_eq!(profile_id(&bytes).unwrap(), image.profile);
            let restored = import_project(&bytes, fixture(image.profile)).unwrap();
            assert_eq!(restored.macro_texts, p.macro_texts);
            assert_eq!(restored.macro_actions, p.macro_actions);
            assert_eq!(export_project(&restored).unwrap(), bytes);
            if let Some(other) = beta::profiles()
                .iter()
                .find(|candidate| candidate.profile != image.profile)
            {
                assert!(import_project(&bytes, fixture(other.profile)).is_err());
            }
            for mutation in 0..8 {
                let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                match mutation {
                    0 => v["unknown"] = true.into(),
                    1 => v["rgb"]["brightness"] = 161.into(),
                    2 => v["layers"][0][0] = 0xffff.into(),
                    3 => v["physicalKeys"][0] = v["physicalKeys"][1].clone(),
                    4 => v["encoders"][0] = serde_json::json!([4]),
                    5 => v["sourceCatalogSha256"] = "bad".into(),
                    6 => v["macros"][0]["stream"] = serde_json::json!([0]),
                    _ => v["combos"][0]["keys"] = serde_json::json!([4, 4, 0, 0]),
                }
                assert!(
                    import_project(&serde_json::to_vec(&v).unwrap(), fixture(image.profile))
                        .is_err()
                );
            }
        }
    }
    #[test]
    fn strict_json_and_identity() {
        assert!(profile_id(br#"{"version":1,"version":1}"#).is_err());
        assert!(profile_id(b"{} {}").is_err());
        assert!(profile_id(br#"{"format":"ak820-settings-backup"}"#).is_err());
        assert!(profile_id(&vec![b' '; MAX_JSON + 1]).is_err());
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&export_project(&fixture(ProfileId::Main)).unwrap()).unwrap();
        legacy["version"] = 1.into();
        legacy.as_object_mut().unwrap().remove("targetRelease");
        legacy.as_object_mut().unwrap().remove("sourceIhxSha256");
        assert!(parse(&serde_json::to_vec(&legacy).unwrap())
            .unwrap_err()
            .contains("cannot be migrated automatically"));
        let mut current: serde_json::Value =
            serde_json::from_slice(&export_project(&fixture(ProfileId::Main)).unwrap()).unwrap();
        current["targetRelease"] = "v0.6.0-beta.1".into();
        assert!(parse(&serde_json::to_vec(&current).unwrap()).is_err());
        current["targetRelease"] = beta::BETA_RELEASE.into();
        current["sourceIhxSha256"] = "bad".into();
        assert!(parse(&serde_json::to_vec(&current).unwrap()).is_err());
    }
    #[test]
    fn macros_validate_sources_and_wire_limits() {
        assert!(validate_stream(ProfileId::Main, b"ASCII").is_ok());
        assert!(validate_stream(ProfileId::Main, &[128]).is_err());
        assert!(validate_stream(ProfileId::Main, &[0]).is_err());
        assert!(validate_stream(ProfileId::Main, &[1, 9]).is_err());
        assert!(validate_stream(ProfileId::Main, &[1, 1, 4]).is_ok());
        assert!(validate_stream(ProfileId::Main, &[1, 5, 0, 0x52]).is_err());
        assert!(check_code(ProfileId::Main, 0x5700, KeycodeSource::Encoder).is_err());
        assert!(Action::Delay(65025).bytes().is_err());
    }
    #[test]
    fn typed_records_roundtrip_and_inactive_fields_fail_closed() {
        for profile in [ProfileId::Mmo, ProfileId::Shortcuts, ProfileId::Creative] {
            let original = carrier();
            let records = Records::from_nvm(&original).unwrap();
            let json = serde_json::to_vec(&records).unwrap();
            let decoded: Records = serde_json::from_slice(&json).unwrap();
            assert_eq!(decoded.candidate(profile).unwrap(), original);
            let mut invalid = decoded.clone();
            invalid.schema = 0x082002;
            assert!(invalid.candidate(profile).is_err());
            invalid = decoded;
            invalid.tap_dance.pop();
            assert!(invalid.candidate(profile).is_err());
        }
        let mut records = Records::from_nvm(&carrier()).unwrap();
        let mut settings = QmkSettings::default();
        settings.set(4, 123).unwrap();
        records.settings = settings.encode().to_vec();
        assert!(records.candidate(ProfileId::Main).is_err());
    }
}

#[cfg(test)]
mod size_boundary_tests {
    use super::*;

    #[test]
    fn normalized_import_and_export_share_the_same_size_boundary() {
        // Use the same full project fixture as the codec roundtrip tests.
        let mut project = super::tests::fixture(ProfileId::Main);
        let baseline = export_project(&project).unwrap();
        let text_room = MAX_JSON - baseline.len();
        project.layer_names[0] = "a".repeat(text_room + "Layer".len());
        let exact = export_project(&project).unwrap();
        assert_eq!(exact.len(), MAX_JSON);
        let reopened = import_project(&exact, super::tests::fixture(ProfileId::Main)).unwrap();
        assert_eq!(export_project(&reopened).unwrap(), exact);
        project.layer_names[0].push('a');
        assert!(export_project(&project).is_err());
        let mut doc: serde_json::Value = serde_json::from_slice(&exact).unwrap();
        doc["layerNames"][0].as_str().unwrap();
        doc["layerNames"][0] = format!("{}a", doc["layerNames"][0].as_str().unwrap()).into();
        let compact = serde_json::to_vec(&doc).unwrap();
        assert!(compact.len() < MAX_JSON);
        assert!(import_project(&compact, super::tests::fixture(ProfileId::Main)).is_err());
    }
}
