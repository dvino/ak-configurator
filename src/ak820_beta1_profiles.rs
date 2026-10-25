//! Exact profile identities from the verified AK820 beta.1 capability catalog.
//!
//! The catalog describes development images. It does not grant flashing support.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub(crate) const BETA_RELEASE: &str = "v0.6.0-beta.1";
pub(crate) const SOURCE_CATALOG_SHA256: &str =
    "b5c1f29e7e3bf074e098cc489b0e8ea84e9085d530bf41313e4dde21353a5302";
pub(crate) const SOURCE_ARCHIVE_SHA256: &str =
    "ccf180581fd092a8180940b3af4445c548c6f4f1a195277f32e20c3fcefdea06";
pub(crate) const SOURCE_MATRIX_SHA256: &str =
    "6d46bf505a4d076e5eba4abaa94799ea724234b9f63ff84e1333a53f409eeabd";
pub(crate) const SOURCE_MODULE_DESCRIPTIONS_SHA256: &str =
    "ea8186c100e771aadbe829118cfd3fa95794bcc7cbad14eb5dac48f1f5dbc460";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProfileId {
    Work,
    Essential,
    Typing,
    Mouse,
    MouseTyping,
    System,
    TapDance,
    KeyOverrides,
    Settings,
}

impl ProfileId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Essential => "essential",
            Self::Typing => "typing",
            Self::Mouse => "mouse",
            Self::MouseTyping => "mouse-typing",
            Self::System => "system",
            Self::TapDance => "tap-dance",
            Self::KeyOverrides => "key-overrides",
            Self::Settings => "settings",
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CapabilityCatalog {
    pub(crate) format: String,
    pub(crate) hardware_validated: bool,
    pub(crate) flashing_authorized: bool,
    pub(crate) settings_writes_authorized: bool,
    pub(crate) images: Vec<ProfileImage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProfileImage {
    pub(crate) fixed_limits: FixedLimits,
    pub(crate) runtime_qmk_settings: Vec<serde_json::Value>,
    pub(crate) module_storage: serde_json::Value,
    pub(crate) compatible_transitions: Vec<serde_json::Value>,
    pub(crate) profile: ProfileId,
    pub(crate) model: String,
    pub(crate) status: String,
    pub(crate) release_approved: bool,
    pub(crate) hardware_validated: bool,
    pub(crate) flashing_authorized: bool,
    pub(crate) settings_writes_authorized: bool,
    pub(crate) settings_schema: u32,
    pub(crate) features: Vec<String>,
    pub(crate) build_identity: BuildIdentity,
    pub(crate) code_identity: CodeIdentity,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BuildIdentity {
    pub(crate) nvm_schema: u32,
    pub(crate) compiler_sha256: String,
    pub(crate) profile: String,
    pub(crate) configuration_sha256: String,
    pub(crate) source_manifest_sha256: String,
    pub(crate) ihx_sha256: String,
    pub(crate) map_sha256: String,
    pub(crate) mem_sha256: String,
    pub(crate) build_report_sha256: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodeIdentity {
    pub(crate) fill_byte: u8,
    pub(crate) ranges: Vec<CodeRange>,
    pub(crate) algorithm: String,
    pub(crate) sha256: String,
    pub(crate) covered_bytes: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FirmwareMatrix {
    pub(crate) settings_transitions: Vec<SettingsTransition>,
    pub(crate) format: String,
    pub(crate) default_profile: String,
    pub(crate) base_features: Vec<String>,
    pub(crate) modules: Vec<MatrixModule>,
    pub(crate) profiles: Vec<MatrixProfile>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MatrixModule {
    pub(crate) feature: String,
    pub(crate) make_switch: String,
    pub(crate) profiles: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MatrixProfile {
    pub(crate) identity: BuildIdentity,
    pub(crate) configuration: serde_json::Value,
    pub(crate) profile: ProfileId,
    pub(crate) installable: bool,
    pub(crate) release_approved: bool,
    pub(crate) optional_features: Vec<String>,
    pub(crate) capabilities: MatrixCapabilities,
    pub(crate) measurement_status: String,
    pub(crate) offline_tests_status: String,
    pub(crate) runtime_status: String,
    pub(crate) settings_status: Option<String>,
    pub(crate) settings_identity: Option<SettingsIdentity>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MatrixCapabilities {
    pub(crate) dynamic_entries: DynamicEntries,
    pub(crate) runtime_qmk_settings_ids: Vec<u16>,
    pub(crate) optional_modules: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingsIdentity {
    pub(crate) profile: ProfileId,
    pub(crate) definition_sha256: String,
    pub(crate) features: Vec<String>,
    pub(crate) supported_keycodes: Vec<u16>,
    pub(crate) combos_version: u8,
    pub(crate) extensions_version: Option<u8>,
    pub(crate) code_sha256: String,
    pub(crate) ihx_sha256: String,
    pub(crate) nvm_schema: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DynamicEntries {
    pub(crate) combos: u8,
    pub(crate) tap_dance: u8,
    pub(crate) key_overrides: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixedLimits {
    pub(crate) layers: u8,
    pub(crate) encoders: u8,
    pub(crate) macros: u8,
    pub(crate) macro_bytes: u16,
    pub(crate) combos: u8,
    pub(crate) tap_dance: u8,
    pub(crate) key_overrides: u8,
    pub(crate) rgb_leds: u8,
    pub(crate) rgb_maximum_brightness: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CodeRange {
    pub(crate) start: u32,
    pub(crate) end_exclusive: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingsTransition {
    pub(crate) source: String,
    pub(crate) target: String,
}

/// Source admission follows pinned settings_backup.image_keycode_supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeycodeSource {
    Matrix,
    Encoder,
    Macro,
    MacroTap,
    MacroDown,
    MacroUp,
}

pub(crate) fn supports_keycode(id: ProfileId, code: u16, source: KeycodeSource) -> bool {
    let row = matrix_profile(id);
    let enabled = |feature: &str| row.capabilities.optional_modules.get(feature) == Some(&true);
    if !keycode_supported(code, enabled) {
        return false;
    }
    if let Some(identity) = &row.settings_identity {
        if identity.supported_keycodes.binary_search(&code).is_err() {
            return false;
        }
    }
    if source != KeycodeSource::Matrix
        && ((0x5700..=0x5707).contains(&code)
            || (0x7c53..=0x7c58).contains(&code)
            || (0x7c13..=0x7c15).contains(&code))
    {
        return false;
    }
    let is_macro = matches!(
        source,
        KeycodeSource::Macro
            | KeycodeSource::MacroTap
            | KeycodeSource::MacroDown
            | KeycodeSource::MacroUp
    );
    !(is_macro
        && ((0x7c5d..=0x7c5f).contains(&code) && source != KeycodeSource::MacroTap
            || matches!(code, 0x7c1a | 0x7c1b | 0x7c1e)))
}

fn keycode_supported(code: u16, enabled: impl Fn(&str) -> bool) -> bool {
    let optional = match code {
        0x7011..=0x7013 => Some("nkro-control"),
        0x7c1a | 0x7c1b | 0x7c1e => Some("space-cadet"),
        0x7009..=0x700b => Some("gui-lock"),
        0x701b..=0x701d => Some("ctrl-gui-swap"),
        0x7014..=0x7016 => Some("mac-pc"),
        0x7c13..=0x7c15 => Some("auto-shift"),
        0x7c53..=0x7c57 => Some("dynamic-macros"),
        0x7c58 => Some("leader"),
        0x5700..=0x5707 => Some("tap-dance"),
        0x7c5d..=0x7c5f => Some("key-overrides"),
        _ => None,
    };
    if optional.is_some_and(&enabled) {
        return true;
    }
    let basic = |value: u16| {
        matches!(value, 0 | 1 | 4..=0xc2 | 0xe0..=0xe7)
            || (enabled("mouse-keys") && matches!(value, 0xcd..=0xd5 | 0xd9..=0xdc))
    };
    match code {
        0..=255 => basic(code),
        256..=0x3fff => basic(code & 255),
        0x4000..=0x4fff => ((code >> 8) & 15) < 6 && basic(code & 255),
        0x5000..=0x51ff => ((code - 0x5000) >> 5) < 6,
        0x5200..=0x527f | 0x52c0..=0x52df => (code & 31) < 6,
        0x52a0..=0x52bf | 0x7700..=0x770f | 0x7820..=0x782a => true,
        0x7c00 | 0x7c01 | 0x7c02 | 0x7c03 | 0x7c16 | 0x7c73 | 0x7c77 | 0x7c78 | 0x7c79 | 0x7c7a => {
            true
        }
        _ => false,
    }
}

/// Explicit transitions use exact IHX pairs; a shared schema grants nothing.
pub(crate) fn settings_transition_allowed(source: &str, target: &str) -> bool {
    let matrix = firmware_matrix();
    let known = |hash: &str| {
        matrix.profiles.iter().any(|row| {
            row.settings_identity
                .as_ref()
                .is_some_and(|identity| identity.ihx_sha256 == hash)
        })
    };
    known(source)
        && known(target)
        && (source == target
            || matrix
                .settings_transitions
                .iter()
                .any(|pair| pair.source == source && pair.target == target))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModuleDescriptionCatalog {
    pub(crate) schema: u32,
    pub(crate) features: Vec<FeatureDescription>,
    pub(crate) profiles: Vec<ProfileDescription>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FeatureDescription {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) title: LocalizedText,
    pub(crate) summary: LocalizedText,
    pub(crate) guide: LocalizedText,
    pub(crate) limits: LocalizedText,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProfileDescription {
    pub(crate) id: String,
    pub(crate) default: bool,
    pub(crate) title: LocalizedText,
    pub(crate) summary: LocalizedText,
    pub(crate) purpose: LocalizedText,
    pub(crate) optional_features: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LocalizedText {
    pub(crate) en: String,
    pub(crate) ru: String,
}

impl LocalizedText {
    pub(crate) fn for_language(&self, russian: bool) -> &str {
        if russian { &self.ru } else { &self.en }
    }
}

static CATALOG: OnceLock<CapabilityCatalog> = OnceLock::new();
static FIRMWARE_MATRIX: OnceLock<FirmwareMatrix> = OnceLock::new();
static MODULE_DESCRIPTIONS: OnceLock<ModuleDescriptionCatalog> = OnceLock::new();

pub(crate) fn catalog() -> &'static CapabilityCatalog {
    CATALOG.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta1-capability-catalog.json");
        assert_sha256(source, SOURCE_CATALOG_SHA256, "beta.1 capability catalog");
        let catalog: CapabilityCatalog = serde_json::from_str(source)
            .expect("bundled beta.1 profile catalog must be valid JSON");
        assert_eq!(catalog.format, "ak820-development-capability-catalog-1");
        assert_eq!(catalog.images.len(), 9, "beta.1 has nine curated profiles");
        assert!(!catalog.hardware_validated);
        assert!(!catalog.flashing_authorized);
        assert!(!catalog.settings_writes_authorized);
        assert!(catalog.images.iter().all(|image| {
            !image.hardware_validated
                && !image.flashing_authorized
                && !image.settings_writes_authorized
        }));
        catalog
    })
}

pub(crate) fn profile(id: ProfileId) -> &'static ProfileImage {
    catalog()
        .images
        .iter()
        .find(|image| image.profile == id)
        .expect("every curated beta.1 profile must occur in the catalog")
}

pub(crate) fn profiles() -> &'static [ProfileImage] {
    &catalog().images
}

pub(crate) fn firmware_matrix() -> &'static FirmwareMatrix {
    FIRMWARE_MATRIX.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta1-firmware-matrix.json");
        assert_sha256(source, SOURCE_MATRIX_SHA256, "beta.1 firmware matrix");
        let matrix = parse_trusted_matrix(source)
            .expect("bundled beta.1 firmware matrix must be valid and pinned");
        assert_eq!(matrix.format, "ak820-firmware-matrix-1");
        assert_eq!(matrix.default_profile, "work");
        assert_eq!(matrix.profiles.len(), 9, "beta.1 matrix has nine profiles");
        validate_contracts(&matrix).expect("bundled beta.1 contracts must agree");
        matrix
    })
}

/// The bundled bytes are the trust root. This parser rejects every byte change,
/// including duplicate/unknown fields, before deserialization can discard data.
pub(crate) fn parse_trusted_matrix(source: &str) -> Result<FirmwareMatrix, String> {
    if format!("{:x}", Sha256::digest(source.as_bytes())) != SOURCE_MATRIX_SHA256 {
        return Err("matrix differs from the pinned beta.1 source".into());
    }
    let matrix = serde_json::from_str(source).map_err(|error| format!("{error}"))?;
    validate_contracts(&matrix)?;
    Ok(matrix)
}

fn validate_contracts(matrix: &FirmwareMatrix) -> Result<(), String> {
    use std::collections::BTreeSet;
    let fail = || "beta.1 contract mismatch".to_string();
    let ids: BTreeSet<_> = matrix
        .profiles
        .iter()
        .map(|row| row.profile.as_str())
        .collect();
    let expected: BTreeSet<_> = catalog()
        .images
        .iter()
        .map(|row| row.profile.as_str())
        .collect();
    if ids != expected || ids.len() != matrix.profiles.len() {
        return Err(fail());
    }
    let mut packed = BTreeSet::new();
    for row in &matrix.profiles {
        let image = profile(row.profile);
        let schema03 = matches!(
            row.profile,
            ProfileId::TapDance | ProfileId::KeyOverrides | ProfileId::Settings
        );
        let schema = if schema03 { 0x082003 } else { 0x082002 };
        let flags = &row.capabilities.optional_modules;
        let flag = |id: &str| flags.get(id) == Some(&true);
        let features: BTreeSet<_> = row.optional_features.iter().map(String::as_str).collect();
        let enabled: BTreeSet<_> = flags
            .iter()
            .filter_map(|(id, &on)| on.then_some(id.as_str()))
            .collect();
        let all_modules: BTreeSet<_> = matrix.modules.iter().map(|m| m.feature.as_str()).collect();
        if features != enabled
            || features.len() != row.optional_features.len()
            || flags.keys().map(String::as_str).collect::<BTreeSet<_>>() != all_modules
            || row.identity.ihx_sha256 != image.build_identity.ihx_sha256
            || row.identity.configuration_sha256 != image.build_identity.configuration_sha256
            || row.identity.profile != row.profile.as_str()
            || row.identity.nvm_schema != schema
            || image.settings_schema != schema
            || image.build_identity.nvm_schema != schema
            || image.model != "ajazz/ak820"
            || row.configuration["profile"].as_str() != Some(row.profile.as_str())
            || row.configuration["nvm_schema"].as_u64() != Some(schema as u64)
            || row.configuration["optional_features"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<BTreeSet<_>>()
                })
                != Some(features.clone())
        {
            return Err(fail());
        }
        let counts = &row.capabilities.dynamic_entries;
        if counts.combos != 8
            || counts.tap_dance != if flag("tap-dance") { 8 } else { 0 }
            || counts.key_overrides != if flag("key-overrides") { 8 } else { 0 }
            || row.capabilities.runtime_qmk_settings_ids
                != if flag("qmk-settings") {
                    vec![1, 2, 7, 18, 19]
                } else {
                    vec![]
                }
        {
            return Err(fail());
        }
        let limits = &image.fixed_limits;
        if (
            limits.layers,
            limits.encoders,
            limits.macros,
            limits.macro_bytes,
            limits.combos,
            limits.tap_dance,
            limits.key_overrides,
            limits.rgb_leds,
            limits.rgb_maximum_brightness,
        ) != (6, 1, 16, 8192, 8, 8, 8, 82, 160)
            || image.code_identity.algorithm != "sha256-code-windows-zero-v1"
            || image.code_identity.fill_byte != 0
            || image.code_identity.covered_bytes != 40505
            || image
                .code_identity
                .ranges
                .iter()
                .map(|r| (r.start, r.end_exclusive))
                .collect::<Vec<_>>()
                != vec![(10400, 12288), (12327, 50944)]
            || !image.compatible_transitions.is_empty()
        {
            return Err(fail());
        }
        for (module, count) in [
            ("tap_dance", counts.tap_dance),
            ("key_overrides", counts.key_overrides),
        ] {
            let storage = &image.module_storage["modules"][module];
            if storage["enabled_entries"].as_u64() != Some(count as u64)
                || storage["stored_entries"].as_u64() != Some(if schema03 { 8 } else { 0 })
            {
                return Err(fail());
            }
        }
        if let Some(identity) = &row.settings_identity {
            if identity.profile != row.profile
                || identity.ihx_sha256 != row.identity.ihx_sha256
                || identity.nvm_schema != schema
                || identity.combos_version != 1
                || identity.extensions_version != if schema03 { Some(1) } else { None }
                || identity.definition_sha256.len() != 64
                || identity.code_sha256.len() != 64
                || identity.code_sha256 == image.code_identity.sha256
                || !packed.insert(identity.code_sha256.as_str())
                || identity
                    .features
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
                    != features
                || identity.features.len() != features.len()
                || identity.supported_keycodes
                    != (0..=u16::MAX)
                        .filter(|&code| keycode_supported(code, flag))
                        .collect::<Vec<_>>()
            {
                return Err(fail());
            }
        }
    }
    let mut pairs = BTreeSet::new();
    let known = |hash: &str| {
        matrix.profiles.iter().any(|row| {
            row.settings_identity
                .as_ref()
                .is_some_and(|id| id.ihx_sha256 == hash)
        })
    };
    for pair in &matrix.settings_transitions {
        if !known(&pair.source)
            || !known(&pair.target)
            || pair.source == pair.target
            || !pairs.insert((&pair.source, &pair.target))
        {
            return Err(fail());
        }
    }
    Ok(())
}

fn assert_sha256(source: &str, expected: &str, name: &str) {
    let actual = format!("{:x}", Sha256::digest(source.as_bytes()));
    assert_eq!(actual, expected, "bundled {name} digest mismatch");
}

pub(crate) fn matrix_profile(id: ProfileId) -> &'static MatrixProfile {
    let matrix_profile = firmware_matrix()
        .profiles
        .iter()
        .find(|profile| profile.profile == id)
        .expect("every curated profile must occur in the firmware matrix");
    let image = profile(id);
    assert_eq!(matrix_profile.release_approved, image.release_approved);
    assert_eq!(matrix_profile.installable, id == ProfileId::Work);
    if let Some(settings_identity) = &matrix_profile.settings_identity {
        assert_eq!(id, ProfileId::Work);
        assert_eq!(
            settings_identity.ihx_sha256,
            image.build_identity.ihx_sha256
        );
        assert_eq!(settings_identity.nvm_schema, image.settings_schema);
        assert_eq!(settings_identity.code_sha256.len(), 64);
    } else {
        assert_ne!(id, ProfileId::Work);
    }
    matrix_profile
}

pub(crate) fn module_descriptions() -> &'static ModuleDescriptionCatalog {
    MODULE_DESCRIPTIONS.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta1-module-descriptions.json");
        let source_sha256 = format!("{:x}", Sha256::digest(source.as_bytes()));
        assert_eq!(source_sha256, SOURCE_MODULE_DESCRIPTIONS_SHA256);
        let descriptions: ModuleDescriptionCatalog = serde_json::from_str(source)
            .expect("bundled beta.1 module descriptions must be valid JSON");
        assert_eq!(descriptions.schema, 1);
        assert_eq!(descriptions.profiles.len(), 9);
        descriptions
    })
}

pub(crate) fn feature_description(id: &str) -> Option<&'static FeatureDescription> {
    module_descriptions()
        .features
        .iter()
        .find(|feature| feature.id == id)
}

pub(crate) fn profile_description(id: ProfileId) -> &'static ProfileDescription {
    module_descriptions()
        .profiles
        .iter()
        .find(|profile| profile.id == id.as_str())
        .expect("every curated profile must have a bilingual description")
}

pub(crate) fn display_name(id: ProfileId) -> &'static str {
    match id {
        ProfileId::Work => "Work",
        ProfileId::Essential => "Essential",
        ProfileId::Typing => "Typing",
        ProfileId::Mouse => "Mouse",
        ProfileId::MouseTyping => "Mouse + Typing",
        ProfileId::System => "System",
        ProfileId::TapDance => "Tap Dance",
        ProfileId::KeyOverrides => "Key Overrides",
        ProfileId::Settings => "Settings",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const PROFILE_IDS: [&str; 9] = [
        "work",
        "essential",
        "typing",
        "mouse",
        "mouse-typing",
        "system",
        "tap-dance",
        "key-overrides",
        "settings",
    ];
    const BASE_FEATURE_IDS: [&str; 14] = [
        "six-layers",
        "sixteen-vial-macros",
        "eight-two-key-combos",
        "compact-tap-hold",
        "encoder-map",
        "media-keys",
        "nkro-reports",
        "caps-word",
        "repeat",
        "alternate-repeat",
        "grave-escape",
        "tri-layer",
        "rgb-effects",
        "vialrgb-direct-ram",
    ];
    const OPTIONAL_MODULE_IDS: [&str; 14] = [
        "nkro-control",
        "one-shot",
        "space-cadet",
        "leader",
        "mouse-keys",
        "auto-shift",
        "dynamic-macros",
        "gui-lock",
        "ctrl-gui-swap",
        "os-indicators",
        "mac-pc",
        "tap-dance",
        "key-overrides",
        "qmk-settings",
    ];

    fn exact_unique_ids<'a>(
        ids: impl IntoIterator<Item = &'a str>,
        expected: &[&str],
        label: &str,
    ) {
        let ids: Vec<_> = ids.into_iter().collect();
        let unique: BTreeSet<_> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate {label} IDs");
        assert_eq!(
            unique,
            expected.iter().copied().collect(),
            "unexpected {label} IDs"
        );
    }

    fn assert_localized_help(text: &LocalizedText, label: &str) {
        assert!(!text.en.trim().is_empty(), "missing English {label}");
        assert!(!text.ru.trim().is_empty(), "missing Russian {label}");
    }

    #[test]
    fn all_beta1_profile_sources_have_the_same_exact_unique_id_set() {
        assert_eq!(SOURCE_ARCHIVE_SHA256.len(), 64);
        exact_unique_ids(
            profiles().iter().map(|image| image.profile.as_str()),
            &PROFILE_IDS,
            "catalog profile",
        );
        exact_unique_ids(
            firmware_matrix()
                .profiles
                .iter()
                .map(|profile| profile.profile.as_str()),
            &PROFILE_IDS,
            "matrix profile",
        );
        exact_unique_ids(
            module_descriptions()
                .profiles
                .iter()
                .map(|profile| profile.id.as_str()),
            &PROFILE_IDS,
            "description profile",
        );
    }

    #[test]
    fn profile_default_approval_and_installability_are_exactly_work_only() {
        assert_eq!(firmware_matrix().default_profile, "work");
        for id in PROFILE_IDS {
            let profile_id: ProfileId = serde_json::from_str(&format!("\"{id}\""))
                .unwrap_or_else(|_| panic!("profile ID {id} must deserialize"));
            let image = profile(profile_id);
            let matrix = matrix_profile(profile_id);
            let description = profile_description(profile_id);
            let is_work = id == "work";
            assert_eq!(description.default, is_work, "{id} default flag");
            assert_eq!(image.release_approved, is_work, "{id} catalog approval");
            assert_eq!(matrix.release_approved, is_work, "{id} matrix approval");
            assert_eq!(matrix.installable, is_work, "{id} installability");
        }
    }

    #[test]
    fn optional_features_match_module_flags_and_profile_help_exactly() {
        let matrix = firmware_matrix();
        let descriptions = module_descriptions();
        exact_unique_ids(
            matrix.base_features.iter().map(String::as_str),
            &BASE_FEATURE_IDS,
            "base feature",
        );
        let module_ids: BTreeSet<_> = matrix
            .modules
            .iter()
            .map(|module| module.feature.as_str())
            .collect();
        assert_eq!(
            module_ids.len(),
            matrix.modules.len(),
            "duplicate optional module IDs"
        );
        exact_unique_ids(
            matrix.modules.iter().map(|module| module.feature.as_str()),
            &OPTIONAL_MODULE_IDS,
            "optional module",
        );

        for profile in &matrix.profiles {
            let id = profile.profile.as_str();
            let description = descriptions
                .profiles
                .iter()
                .find(|candidate| candidate.id == id)
                .unwrap_or_else(|| panic!("missing profile description for {id}"));
            let feature_set: BTreeSet<_> = profile
                .optional_features
                .iter()
                .map(String::as_str)
                .collect();
            let description_set: BTreeSet<_> = description
                .optional_features
                .iter()
                .map(String::as_str)
                .collect();
            assert_eq!(
                feature_set.len(),
                profile.optional_features.len(),
                "duplicate matrix feature for {id}"
            );
            assert_eq!(
                description_set.len(),
                description.optional_features.len(),
                "duplicate help feature for {id}"
            );
            assert_eq!(
                feature_set, description_set,
                "matrix/help features differ for {id}"
            );

            let flags = &profile.capabilities.optional_modules;
            exact_unique_ids(
                flags.keys().map(String::as_str),
                &module_ids.iter().copied().collect::<Vec<_>>(),
                "profile module flag",
            );
            let enabled: BTreeSet<_> = flags
                .iter()
                .filter_map(|(module, enabled)| enabled.then_some(module.as_str()))
                .collect();
            assert_eq!(
                enabled, feature_set,
                "enabled module flags differ from optional_features for {id}"
            );
            for module in &module_ids {
                assert_eq!(
                    flags.get(*module).copied(),
                    Some(feature_set.contains(module)),
                    "module presence flag {module} for {id}"
                );
            }
            for module in &matrix.modules {
                let declared_profiles: BTreeSet<_> =
                    module.profiles.iter().map(String::as_str).collect();
                assert_eq!(
                    declared_profiles.len(),
                    module.profiles.len(),
                    "duplicate profile in {} module membership",
                    module.feature
                );
                assert_eq!(
                    declared_profiles.contains(id),
                    feature_set.contains(module.feature.as_str()),
                    "{} module membership disagrees for {id}",
                    module.feature
                );
                assert!(
                    !module.make_switch.trim().is_empty(),
                    "{} module has no build switch",
                    module.feature
                );
            }
        }
        for module in &matrix.modules {
            let declared: BTreeSet<_> = module.profiles.iter().map(String::as_str).collect();
            let expected: BTreeSet<_> = matrix
                .profiles
                .iter()
                .filter(|profile| {
                    profile
                        .capabilities
                        .optional_modules
                        .get(&module.feature)
                        .copied()
                        .unwrap_or_else(|| panic!("missing {} module flag", module.feature))
                })
                .map(|profile| profile.profile.as_str())
                .collect();
            assert_eq!(
                declared, expected,
                "{} module profile membership is not exact",
                module.feature
            );
        }
    }

    #[test]
    fn every_optional_module_and_base_feature_has_bilingual_help_and_guides() {
        let matrix = firmware_matrix();
        let catalog = module_descriptions();
        exact_unique_ids(
            catalog.features.iter().map(|feature| feature.id.as_str()),
            &catalog
                .features
                .iter()
                .map(|feature| feature.id.as_str())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            "feature description",
        );
        let described: BTreeSet<_> = catalog
            .features
            .iter()
            .map(|feature| feature.id.as_str())
            .collect();
        let required: BTreeSet<_> = matrix
            .base_features
            .iter()
            .map(String::as_str)
            .chain(matrix.modules.iter().map(|module| module.feature.as_str()))
            .collect();
        assert_eq!(
            required, described,
            "feature help must exactly cover base features and optional modules"
        );
        for feature in &catalog.features {
            assert_eq!(
                feature.kind,
                if BASE_FEATURE_IDS.contains(&feature.id.as_str()) {
                    "base"
                } else {
                    "optional"
                },
                "incorrect help classification for {}",
                feature.id
            );
            for (kind, text) in [
                ("title", &feature.title),
                ("summary", &feature.summary),
                ("guide", &feature.guide),
                ("limits", &feature.limits),
            ] {
                assert_localized_help(text, &format!("{} {kind}", feature.id));
            }
        }
    }

    #[test]
    fn catalog_image_features_must_resolve_to_described_features() {
        let matrix = firmware_matrix();
        let described: BTreeSet<_> = module_descriptions()
            .features
            .iter()
            .map(|feature| feature.id.as_str())
            .collect();
        for image in profiles() {
            let features: BTreeSet<_> = image.features.iter().map(String::as_str).collect();
            assert_eq!(
                features.len(),
                image.features.len(),
                "duplicate feature in {} image",
                image.profile.as_str()
            );
            let matrix_profile = matrix_profile(image.profile);
            let expected: BTreeSet<_> = matrix
                .base_features
                .iter()
                .map(String::as_str)
                .chain(matrix_profile.optional_features.iter().map(String::as_str))
                .collect();
            assert_eq!(
                features,
                expected,
                "{} image feature list differs from base plus optional features",
                image.profile.as_str()
            );
            for feature in features {
                assert!(
                    described.contains(feature),
                    "{} image references undocumented feature {feature}",
                    image.profile.as_str()
                );
            }
        }
    }
    #[test]
    fn pinned_matrix_rejects_duplicate_unknown_and_changed_fields() {
        let source = include_str!("../assets/ak820/beta1-firmware-matrix.json");
        assert!(parse_trusted_matrix(source).is_ok());
        for changed in [
            source.replacen("{", "{\"unknown\":true,", 1),
            source.replacen("{", "{\"format\":\"ak820-firmware-matrix-1\",", 1),
            source.replacen("532482", "532483", 1),
        ] {
            assert!(parse_trusted_matrix(&changed).is_err());
        }
    }

    #[test]
    fn counts_qsids_and_packed_identity_remain_separate() {
        for row in &firmware_matrix().profiles {
            assert_eq!(
                row.capabilities.dynamic_entries.tap_dance,
                if row.profile == ProfileId::TapDance {
                    8
                } else {
                    0
                }
            );
            assert_eq!(
                row.capabilities.dynamic_entries.key_overrides,
                if row.profile == ProfileId::KeyOverrides {
                    8
                } else {
                    0
                }
            );
            assert_eq!(
                row.capabilities.runtime_qmk_settings_ids,
                if row.profile == ProfileId::Settings {
                    vec![1, 2, 7, 18, 19]
                } else {
                    vec![]
                }
            );
            assert_eq!(
                row.settings_identity.is_some(),
                row.profile == ProfileId::Work
            );
            let ihx = &row.identity.ihx_sha256;
            assert_eq!(
                settings_transition_allowed(ihx, ihx),
                row.profile == ProfileId::Work
            );
            assert!(!settings_transition_allowed(ihx, "unknown"));
        }
    }

    #[test]
    fn source_admission_matches_compact_processor_boundaries() {
        use KeycodeSource::*;
        for source in [Encoder, Macro, MacroTap, MacroDown, MacroUp] {
            assert!(!supports_keycode(ProfileId::TapDance, 0x5700, source));
            assert!(!supports_keycode(ProfileId::Typing, 0x7c13, source));
            assert!(!supports_keycode(ProfileId::Work, 0x7c53, source));
            assert!(!supports_keycode(ProfileId::Work, 0x7c58, source));
        }
        assert!(supports_keycode(ProfileId::TapDance, 0x5707, Matrix));
        assert!(!supports_keycode(ProfileId::TapDance, 0x5708, Matrix));
        assert!(!supports_keycode(ProfileId::Work, 0x5700, Matrix));
        assert!(supports_keycode(ProfileId::Mouse, 0xcd, Encoder));
        assert!(!supports_keycode(ProfileId::Essential, 0xcd, Matrix));
        assert!(supports_keycode(ProfileId::KeyOverrides, 0x7c5d, MacroTap));
        for source in [Macro, MacroDown, MacroUp] {
            assert!(!supports_keycode(ProfileId::KeyOverrides, 0x7c5d, source));
        }
        assert!(supports_keycode(ProfileId::Work, 0x7c1a, Encoder));
        assert!(!supports_keycode(ProfileId::Work, 0x7c1a, MacroTap));
        for id in [ProfileId::Work, ProfileId::Settings] {
            assert!(!supports_keycode(id, 0x4604, Matrix));
            assert!(!supports_keycode(id, 0x52c6, Matrix));
            assert!(!supports_keycode(id, 0xffff, Matrix));
        }
    }

    #[test]
    fn semantic_validation_rejects_bad_counts_qsids_identity_and_transitions() {
        let source = include_str!("../assets/ak820/beta1-firmware-matrix.json");
        let mutations: [fn(&mut FirmwareMatrix); 5] = [
            |m| m.profiles[0].capabilities.dynamic_entries.tap_dance = 8,
            |m| m.profiles[0].capabilities.runtime_qmk_settings_ids.push(4),
            |m| {
                m.profiles[0]
                    .settings_identity
                    .as_mut()
                    .unwrap()
                    .supported_keycodes
                    .push(65535)
            },
            |m| m.profiles[0].identity.ihx_sha256 = "0".repeat(64),
            |m| {
                m.settings_transitions.push(SettingsTransition {
                    source: "unknown".into(),
                    target: "unknown".into(),
                })
            },
        ];
        for mutate in mutations {
            let mut matrix: FirmwareMatrix = serde_json::from_str(source).unwrap();
            mutate(&mut matrix);
            assert!(validate_contracts(&matrix).is_err());
        }
    }
}
