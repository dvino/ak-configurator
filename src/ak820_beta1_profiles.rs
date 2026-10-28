//! Exact profile identities from the verified AK820 beta.3 capability catalog.
//!
//! The catalog describes development images. It does not grant flashing support.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub(crate) const BETA_RELEASE: &str = "v0.6.0-beta.3";
pub(crate) const SOURCE_CATALOG_SHA256: &str =
    "cdf03e9fa4a9104ec715e9db58215b1061bfdafb9b239f16461871d278fe241b";
pub(crate) const SOURCE_ARCHIVE_SHA256: &str =
    "bf4c8d86b59819eb9e5a4f11cbe0bc6f25b89e09a1e668e073e193ed050a3543";
pub(crate) const SOURCE_MATRIX_SHA256: &str =
    "d07cbc6bb562e84193f6de121733d13e21aceb129b653e9ff97ae954da36920c";
pub(crate) const SOURCE_MODULE_DESCRIPTIONS_SHA256: &str =
    "2947032a7f53c43d906c3ab909a7fb0beb31d83bae4e44368db422ef18afffc6";
pub(crate) const BETA4_RELEASE: &str = "v0.6.0-beta.4";
pub(crate) const BETA4_1_RELEASE: &str = "v0.6.0-beta.4.1";
pub(crate) const BETA4_ARCHIVE_SHA256: &str =
    "14ffb5a1e3be81819f11eb3c5c6a5100a5b4b6bbbfa822f7cf33e578f79d8097";
const BETA4_MATRIX_SHA256: &str =
    "48001ce5d0e3f59d9c0244c3674fe5db8b91497e5a64f40350946bb5ec53e874";
const BETA4_CAPABILITY_SHA256: &str =
    "b3c91ea8d2f0b9f1d359a6e150d6cd386b0c77944b0ababbaaa2e2937c2b6ed8";
pub(crate) const BETA4_1_MATRIX_SHA256: &str =
    "7b38153e68d168718d04c5c8c8502daad875ca63298bf0e79c20a39c4648c570";
pub(crate) const BETA4_1_CAPABILITY_SHA256: &str =
    "83c365b204d731823c22601d192af5d725252d26b7167c8047ceef07bdb6804b";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProfileId {
    Main,
    Game,
    Mmo,
    Typing,
    Code,
    Pointer,
    Onehand,
    Creative,
    Shortcuts,
    Multios,
}

impl ProfileId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Game => "game",
            Self::Mmo => "mmo",
            Self::Typing => "typing",
            Self::Code => "code",
            Self::Pointer => "pointer",
            Self::Onehand => "onehand",
            Self::Creative => "creative",
            Self::Shortcuts => "shortcuts",
            Self::Multios => "multios",
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
    pub(crate) historical_sources: Vec<HistoricalSource>,
    #[serde(default)]
    pub(crate) historical_profiles: Vec<MatrixProfile>,
    pub(crate) feature_pairs: Vec<FeaturePair>,
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
    #[serde(default)]
    pub(crate) direct_protocol: Option<u8>,
    #[serde(default)]
    pub(crate) macro_layer_tap: bool,
    #[serde(default)]
    pub(crate) combo_actions: Vec<String>,
    pub(crate) profile: ProfileId,
    pub(crate) definition_sha256: String,
    pub(crate) features: Vec<String>,
    pub(crate) supported_keycodes: Vec<u16>,
    pub(crate) combos_version: u8,
    pub(crate) extensions_version: Option<u8>,
    pub(crate) code_sha256: String,
    pub(crate) ihx_sha256: String,
    pub(crate) nvm_schema: u32,
    #[serde(default)]
    pub(crate) settings_record_version: Option<u8>,
    #[serde(default)]
    pub(crate) compact_controls_version: Option<u8>,
    #[serde(default)]
    pub(crate) combo_media: bool,
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
    supports_keycode_for_row(matrix_profile(id), code, source)
}
pub(crate) fn supports_keycode_for_row(
    row: &MatrixProfile,
    code: u16,
    source: KeycodeSource,
) -> bool {
    let enabled = |feature: &str| row.capabilities.optional_modules.get(feature) == Some(&true);
    if macro_layer_tap_keycode(code)
        && (source != KeycodeSource::Matrix
            || !row
                .settings_identity
                .as_ref()
                .is_some_and(|i| i.macro_layer_tap))
    {
        return false;
    }
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

pub(crate) fn macro_layer_tap_keycode(code: u16) -> bool {
    (0x40..=0x45).contains(&(code >> 8)) && (0xf0..=0xff).contains(&(code & 255))
}

fn keycode_supported(code: u16, enabled: impl Fn(&str) -> bool) -> bool {
    let optional = match code {
        0x7c16 => Some("grave-escape"),
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
        0x4000..=0x4fff => {
            macro_layer_tap_keycode(code) || ((code >> 8) & 15) < 6 && basic(code & 255)
        }
        0x5000..=0x51ff => ((code - 0x5000) >> 5) < 6,
        0x5200..=0x527f | 0x52c0..=0x52df => (code & 31) < 6,
        0x52a0..=0x52bf | 0x7700..=0x770f | 0x7820..=0x782a => true,
        0x7c00 | 0x7c01 | 0x7c02 | 0x7c03 | 0x7c73 | 0x7c77 | 0x7c78 | 0x7c79 | 0x7c7a => true,
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
                .is_some_and(|id| id.ihx_sha256 == hash)
        }) || previous_firmware_matrix()
            .profiles
            .iter()
            .any(|r| r.identity.ihx_sha256 == hash)
            || historical_settings_identities()
                .iter()
                .any(|id| id.ihx_sha256 == hash)
    };
    let current_target = matrix
        .profiles
        .iter()
        .any(|row| row.identity.ihx_sha256 == target);
    known(source)
        && current_target
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
static BETA4_FIRMWARE_MATRIX: OnceLock<FirmwareMatrix> = OnceLock::new();
static BETA4_1_FIRMWARE_MATRIX: OnceLock<FirmwareMatrix> = OnceLock::new();
static BETA4_1_CAPABILITY_CATALOG: OnceLock<CapabilityCatalog> = OnceLock::new();
static BETA4_CAPABILITY_CATALOG: OnceLock<CapabilityCatalog> = OnceLock::new();

pub(crate) fn catalog() -> &'static CapabilityCatalog {
    CATALOG.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta3-capability-catalog.json");
        assert_sha256(source, SOURCE_CATALOG_SHA256, "beta.3 capability catalog");
        let catalog: CapabilityCatalog = serde_json::from_str(source)
            .expect("bundled beta.3 profile catalog must be valid JSON");
        assert_eq!(catalog.format, "ak820-development-capability-catalog-1");
        assert_eq!(catalog.images.len(), 10, "beta.3 has ten curated profiles");
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
        .expect("every curated beta.3 profile must occur in the catalog")
}

pub(crate) fn profiles() -> &'static [ProfileImage] {
    &catalog().images
}

pub(crate) fn firmware_matrix() -> &'static FirmwareMatrix {
    FIRMWARE_MATRIX.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta3-firmware-matrix.json");
        assert_sha256(source, SOURCE_MATRIX_SHA256, "beta.3 firmware matrix");
        let matrix = parse_trusted_matrix(source)
            .expect("bundled beta.3 firmware matrix must be valid and pinned");
        assert_eq!(matrix.format, "ak820-firmware-matrix-1");
        assert_eq!(matrix.default_profile, "main");
        assert_eq!(matrix.profiles.len(), 10, "beta.3 matrix has ten profiles");
        validate_contracts(&matrix).expect("bundled beta.3 contracts must agree");
        matrix
    })
}

/// Exact beta.4 metadata is admitted for offline identity and read-only
/// recognition. The release catalogs explicitly deny hardware writes.
pub(crate) fn beta4_firmware_matrix() -> &'static FirmwareMatrix {
    BETA4_FIRMWARE_MATRIX.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta4-firmware-matrix.json");
        assert_sha256(source, BETA4_MATRIX_SHA256, "beta.4 firmware matrix");
        let matrix: FirmwareMatrix = serde_json::from_str(source)
            .expect("bundled beta.4 firmware matrix must be valid");
        let catalog = beta4_capability_catalog();
        assert_eq!(matrix.format, "ak820-firmware-matrix-1");
        assert_eq!(matrix.profiles.len(), 10);
        assert_eq!(matrix.historical_profiles.len(), 10);
        for row in &matrix.profiles {
            let image = catalog.images.iter().find(|image| image.profile == row.profile)
                .expect("beta.4 matrix profile must have an exact capability row");
            let identity = row.settings_identity.as_ref().expect("beta.4 settings identity");
            assert!(row.installable && row.release_approved && image.release_approved);
            assert!(!image.hardware_validated && !image.flashing_authorized && !image.settings_writes_authorized);
            assert_eq!(row.identity.ihx_sha256, image.build_identity.ihx_sha256);
            assert_eq!(identity.ihx_sha256, row.identity.ihx_sha256);
            assert_eq!(identity.nvm_schema, 0x082003);
            assert_eq!(identity.settings_record_version, Some(2));
            assert_eq!(identity.compact_controls_version, Some(2));
            assert_eq!(identity.combo_media, true);
        }
        matrix
    })
}

fn beta4_capability_catalog() -> &'static CapabilityCatalog {
    BETA4_CAPABILITY_CATALOG.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta4-capability-catalog.json");
        assert_sha256(source, BETA4_CAPABILITY_SHA256, "beta.4 capability catalog");
        let catalog: CapabilityCatalog = serde_json::from_str(source)
            .expect("bundled beta.4 capability catalog must be valid");
        assert_eq!(catalog.format, "ak820-development-capability-catalog-1");
        assert!(!catalog.hardware_validated && !catalog.flashing_authorized);
        assert!(!catalog.settings_writes_authorized);
        assert!(catalog.images.iter().all(|image| {
            !image.hardware_validated
                && !image.flashing_authorized
                && !image.settings_writes_authorized
        }));
        catalog
    })
}

pub(crate) fn beta4_settings_writes_authorized(id: ProfileId) -> bool {
    let beta4 = beta4_capability_catalog();
    let beta4_1 = beta4_1_capability_catalog();
    beta4.settings_writes_authorized
        && beta4_1.settings_writes_authorized
        && beta4.images.iter().any(|image| {
            image.profile == id && image.settings_writes_authorized
        })
        && beta4_1.images.iter().any(|image| {
            image.profile == id && image.settings_writes_authorized
        })
}

pub(crate) fn beta4_matrix_profile(id: ProfileId) -> &'static MatrixProfile {
    beta4_firmware_matrix().profiles.iter().find(|row| row.profile == id)
        .expect("every beta.4 profile has an exact matrix row")
}

pub(crate) fn beta4_1_firmware_matrix() -> &'static FirmwareMatrix {
    BETA4_1_FIRMWARE_MATRIX.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta4.1-firmware-matrix.json");
        assert_sha256(source, BETA4_1_MATRIX_SHA256, "beta.4.1 firmware matrix");
        let matrix: FirmwareMatrix = serde_json::from_str(source)
            .expect("bundled beta.4.1 firmware matrix must be valid");
        let catalog = beta4_1_capability_catalog();
        assert_eq!(matrix.format, "ak820-firmware-matrix-1");
        assert_eq!(matrix.profiles.len(), 10);
        assert_eq!(matrix.historical_profiles.len(), 10);
        assert_eq!(catalog.images.iter().map(|image| image.profile.as_str()).collect::<std::collections::BTreeSet<_>>().len(), 10);
        for row in &matrix.profiles {
            let image = catalog.images.iter().find(|image| image.profile == row.profile)
                .expect("beta.4.1 matrix profile must have a capability row");
            let exact = row.settings_identity.as_ref().expect("beta.4.1 settings identity");
            let beta4 = beta4_matrix_profile(row.profile);
            let beta4_exact = beta4.settings_identity.as_ref().expect("beta.4 settings identity");
            assert!(row.installable && row.release_approved && image.release_approved);
            assert!(!image.hardware_validated && image.flashing_authorized);
            assert!(!image.settings_writes_authorized);
            assert_eq!(row.identity.ihx_sha256, image.build_identity.ihx_sha256);
            assert_eq!(row.identity.map_sha256, image.build_identity.map_sha256);
            assert_eq!(exact.ihx_sha256, row.identity.ihx_sha256);
            assert_eq!(row.identity.ihx_sha256, beta4.identity.ihx_sha256);
            assert_eq!(row.identity.map_sha256, beta4.identity.map_sha256);
            assert_eq!(exact.code_sha256, beta4_exact.code_sha256);
            assert_eq!(exact.nvm_schema, image.settings_schema);
            assert_eq!(exact.settings_record_version, Some(2));
            let mut matrix_qsid = row.capabilities.runtime_qmk_settings_ids.clone();
            let mut capability_qsid: Vec<_> = image.runtime_qmk_settings.iter()
                .map(|value| value.as_u64().expect("beta.4.1 QSID must be an integer") as u16)
                .collect();
            matrix_qsid.sort_unstable();
            capability_qsid.sort_unstable();
            assert_eq!(matrix_qsid, capability_qsid);
        }
        matrix
    })
}

pub(crate) fn beta4_1_capability_catalog() -> &'static CapabilityCatalog {
    BETA4_1_CAPABILITY_CATALOG.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta4.1-capability-catalog.json");
        assert_sha256(source, BETA4_1_CAPABILITY_SHA256, "beta.4.1 capability catalog");
        let catalog: CapabilityCatalog = serde_json::from_str(source)
            .expect("bundled beta.4.1 capability catalog must be valid");
        let raw: serde_json::Value = serde_json::from_str(source).unwrap();
        assert_eq!(catalog.format, "ak820-development-capability-catalog-1");
        assert_eq!(raw["release_approved"], true);
        assert_eq!(catalog.hardware_validated, false);
        assert_eq!(catalog.flashing_authorized, true);
        assert_eq!(catalog.settings_writes_authorized, false);
        assert_eq!(catalog.images.len(), 10);
        catalog
    })
}

pub(crate) fn beta4_1_matrix_profile(id: ProfileId) -> &'static MatrixProfile {
    beta4_1_firmware_matrix().profiles.iter().find(|row| row.profile == id)
        .expect("every beta.4.1 profile has an exact matrix row")
}

/// The bundled bytes are the trust root. This parser rejects every byte change,
/// including duplicate/unknown fields, before deserialization can discard data.
pub(crate) fn parse_trusted_matrix(source: &str) -> Result<FirmwareMatrix, String> {
    if format!("{:x}", Sha256::digest(source.as_bytes())) != SOURCE_MATRIX_SHA256 {
        return Err("matrix differs from the pinned beta.3 source".into());
    }
    let matrix = serde_json::from_str(source).map_err(|error| format!("{error}"))?;
    validate_contracts(&matrix)?;
    Ok(matrix)
}

fn validate_contracts(matrix: &FirmwareMatrix) -> Result<(), String> {
    use std::collections::BTreeSet;
    let fail = || "beta.3 contract mismatch".to_string();
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
        return Err(format!("{} at validator line {}", fail(), line!()));
    }
    let mut packed = BTreeSet::new();
    for row in &matrix.profiles {
        let image = profile(row.profile);
        let schema03 = true;
        let schema = 0x082003;
        let flags = &row.capabilities.optional_modules;
        let flag = |id: &str| flags.get(id) == Some(&true);
        let features: BTreeSet<_> = row.optional_features.iter().map(String::as_str).collect();
        let enabled: BTreeSet<_> = flags
            .iter()
            .filter_map(|(id, &on)| on.then_some(id.as_str()))
            .collect();
        let all_modules: BTreeSet<_> = matrix.modules.iter().map(|m| m.feature.as_str()).collect();
        if !row.installable
            || !row.release_approved
            || !image.release_approved
            || row.settings_identity.is_none()
            || features != enabled
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
            return Err(format!("{} at validator line {}", fail(), line!()));
        }
        let counts = &row.capabilities.dynamic_entries;
        if counts.combos != 8
            || counts.tap_dance != if flag("tap-dance") { 8 } else { 0 }
            || counts.key_overrides != if flag("key-overrides") { 8 } else { 0 }
            || row.capabilities.runtime_qmk_settings_ids
                != if flag("qmk-settings") {
                    if flag("auto-shift") {
                        vec![1, 2, 4, 7, 18, 19]
                    } else {
                        vec![1, 2, 7, 18, 19]
                    }
                } else {
                    vec![]
                }
        {
            return Err(format!("{} at validator line {}", fail(), line!()));
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
            return Err(format!("{} at validator line {}", fail(), line!()));
        }
        for (module, count) in [
            ("tap_dance", counts.tap_dance),
            ("key_overrides", counts.key_overrides),
        ] {
            let storage = &image.module_storage["modules"][module];
            if storage["enabled_entries"].as_u64() != Some(count as u64)
                || storage["stored_entries"].as_u64() != Some(if schema03 { 8 } else { 0 })
            {
                return Err(format!("{} at validator line {}", fail(), line!()));
            }
        }
        if let Some(identity) = &row.settings_identity {
            if identity.profile != row.profile
                || identity.ihx_sha256 != row.identity.ihx_sha256
                || identity.nvm_schema != schema
                || identity.direct_protocol
                    != if flag("vialrgb-direct") {
                        Some(1)
                    } else {
                        None
                    }
                || !identity.macro_layer_tap
                || identity.combo_actions != ["caps-word-toggle"]
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
                return Err(format!("{} at validator line {}", fail(), line!()));
            }
        }
    }
    let mut pairs = BTreeSet::new();
    let historical = &historical_settings_identities()[..3];
    if matrix.historical_sources.len() != historical.len()
        || matrix
            .historical_sources
            .iter()
            .map(|id| &id.ihx_sha256)
            .collect::<BTreeSet<_>>()
            .len()
            != historical.len()
        || matrix.historical_sources.iter().any(|source| {
            !historical.iter().any(|id| {
                id.ihx_sha256 == source.ihx_sha256 && id.code_sha256 == source.code_sha256
            })
        })
    {
        return Err(format!("{} at validator line {}", fail(), line!()));
    }
    let known = |hash: &str| {
        matrix
            .profiles
            .iter()
            .any(|row| row.identity.ihx_sha256 == hash)
            || matrix
                .historical_profiles
                .iter()
                .any(|r| r.identity.ihx_sha256 == hash)
            || historical.iter().any(|id| id.ihx_sha256 == hash)
    };
    for pair in &matrix.settings_transitions {
        if !known(&pair.source)
            || !matrix
                .profiles
                .iter()
                .any(|row| row.identity.ihx_sha256 == pair.target)
            || !pairs.insert((&pair.source, &pair.target))
        {
            return Err(format!("{} at validator line {}", fail(), line!()));
        }
    }
    if pairs.len() != 330 {
        return Err(format!("{} at validator line {}", fail(), line!()));
    }
    if matrix.feature_pairs.len() != 136 {
        return Err(format!("{} at validator line {}", fail(), line!()));
    }
    let mut feature_pairs = BTreeSet::new();
    for pair in &matrix.feature_pairs {
        if pair.features.len() != 2
            || pair.features[0] == pair.features[1]
            || !feature_pairs.insert(
                pair.features
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>(),
            )
        {
            return Err(format!("{} at validator line {}", fail(), line!()));
        }
        let expected: BTreeSet<_> = matrix
            .profiles
            .iter()
            .filter(|row| {
                pair.features
                    .iter()
                    .all(|feature| row.optional_features.contains(feature))
            })
            .map(|row| row.profile.as_str())
            .collect();
        if expected != pair.profiles.iter().map(String::as_str).collect()
            || expected.len() != pair.profiles.len()
            || pair.status
                != if expected.is_empty() {
                    "no-catalog-profile"
                } else {
                    "catalog-supported"
                }
        {
            return Err(format!("{} at validator line {}", fail(), line!()));
        }
    }
    Ok(())
}

fn assert_sha256(source: &str, expected: &str, name: &str) {
    let actual = format!("{:x}", Sha256::digest(source.as_bytes()));
    assert_eq!(actual, expected, "bundled {name} digest mismatch");
}

pub(crate) fn matrix_profile(id: ProfileId) -> &'static MatrixProfile {
    firmware_matrix()
        .profiles
        .iter()
        .find(|row| row.profile == id)
        .expect("every active profile is in the firmware matrix")
}

pub(crate) fn has_feature(id: ProfileId, feature: &str) -> bool {
    matrix_profile(id)
        .capabilities
        .optional_modules
        .get(feature)
        == Some(&true)
}

/// Saved enum to VialRGB wire effect, exact to each profile's compiled enum.
pub(crate) fn rgb_modes(id: ProfileId) -> &'static [(u8, u16)] {
    if has_feature(id, "vialrgb-direct") {
        &[(1, 2), (2, 6), (3, 13), (4, 14), (5, 1), (6, 31)]
    } else {
        &[(1, 2), (2, 6), (3, 13), (4, 14), (5, 31)]
    }
}

pub(crate) fn module_descriptions() -> &'static ModuleDescriptionCatalog {
    MODULE_DESCRIPTIONS.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta3-module-descriptions.json");
        let source_sha256 = format!("{:x}", Sha256::digest(source.as_bytes()));
        assert_eq!(source_sha256, SOURCE_MODULE_DESCRIPTIONS_SHA256);
        let descriptions: ModuleDescriptionCatalog = serde_json::from_str(source)
            .expect("bundled beta.3 module descriptions must be valid JSON");
        assert_eq!(descriptions.schema, 1);
        assert_eq!(descriptions.profiles.len(), 10);
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
        ProfileId::Main => "Main",
        ProfileId::Game => "Game",
        ProfileId::Mmo => "MMO",
        ProfileId::Typing => "Typing",
        ProfileId::Code => "Code",
        ProfileId::Pointer => "Pointer",
        ProfileId::Onehand => "Onehand",
        ProfileId::Creative => "Creative",
        ProfileId::Shortcuts => "Shortcuts",
        ProfileId::Multios => "MultiOS",
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoricalSource {
    pub(crate) ihx_sha256: String,
    pub(crate) code_sha256: String,
}
#[derive(Debug, Deserialize)]
pub(crate) struct FeaturePair {
    pub(crate) features: Vec<String>,
    pub(crate) profiles: Vec<String>,
    pub(crate) status: String,
}
/// Historical identities are source-only; they do not add active profile aliases.
#[derive(Debug)]
pub(crate) struct HistoricalSettingsIdentity {
    pub(crate) ihx_sha256: &'static str,
    pub(crate) code_sha256: &'static str,
    pub(crate) nvm_schema: u32,
    pub(crate) combos: bool,
    pub(crate) profile: Option<ProfileId>,
}
pub(crate) fn historical_settings_identities() -> &'static [HistoricalSettingsIdentity] {
    static ALL: OnceLock<Vec<HistoricalSettingsIdentity>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut entries = vec![
            HistoricalSettingsIdentity {
                ihx_sha256: "f1c4347e60ca5f7ea6c557debe60272259f4ddf9acfac9897fa992210bf7ef2e",
                code_sha256: "6ff7b1653925da7dc12bdb6f7ef5fc1dc4319c5b931def50c68b9cf45c4054d7",
                nvm_schema: 0x082002,
                combos: false,
                profile: None,
            },
            HistoricalSettingsIdentity {
                ihx_sha256: "652ee9b583a285cea779d3937b8bcd84035e391d823d71d8ea1d34db2fdef880",
                code_sha256: "a7ad972f47acff20f3492ed846f5c6ca0825d43de263f26cc9328069a27cdd10",
                nvm_schema: 0x082002,
                combos: true,
                profile: None,
            },
            HistoricalSettingsIdentity {
                ihx_sha256: "d505d4f420f745dbd5e98a0e3638c8f3ae6d7719c8729dad274595b71b641f18",
                code_sha256: "01f0f7a979d8cfa8217b3bba333758ae3509cf7f3f8e35fbffb1ed034b808e5d",
                nvm_schema: 0x082002,
                combos: true,
                profile: None,
            },
        ];
        for row in &previous_firmware_matrix().profiles {
            let id = row.settings_identity.as_ref().expect("previous identity");
            entries.push(HistoricalSettingsIdentity {
                ihx_sha256: &id.ihx_sha256,
                code_sha256: &id.code_sha256,
                nvm_schema: id.nvm_schema,
                combos: true,
                profile: Some(row.profile),
            });
        }
        entries
    })
}
pub(crate) fn historical_settings_identity(
    ihx: &str,
    code: &str,
    schema: u32,
) -> Option<&'static HistoricalSettingsIdentity> {
    historical_settings_identities()
        .iter()
        .find(|id| id.ihx_sha256 == ihx && id.code_sha256 == code && id.nvm_schema == schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    const IDS: [ProfileId; 10] = [
        ProfileId::Main,
        ProfileId::Game,
        ProfileId::Mmo,
        ProfileId::Typing,
        ProfileId::Code,
        ProfileId::Pointer,
        ProfileId::Onehand,
        ProfileId::Creative,
        ProfileId::Shortcuts,
        ProfileId::Multios,
    ];
    #[test]
    fn all_ten_exact_identities_and_descriptions_agree() {
        assert_eq!(profiles().len(), 10);
        assert_eq!(firmware_matrix().default_profile, "main");
        for id in IDS {
            let image = profile(id);
            let row = matrix_profile(id);
            assert_eq!(image.build_identity.ihx_sha256, row.identity.ihx_sha256);
            assert_eq!(image.settings_schema, 0x082003);
            assert!(row.installable && row.release_approved && image.release_approved);
            assert!(
                !image.flashing_authorized
                    && !image.settings_writes_authorized
                    && !image.hardware_validated
            );
            let identity = row.settings_identity.as_ref().unwrap();
            assert_eq!(identity.ihx_sha256, row.identity.ihx_sha256);
            assert_ne!(identity.code_sha256, image.code_identity.sha256);
            let help = profile_description(id);
            assert_eq!(help.default, id == ProfileId::Main);
            assert!(!help.summary.en.is_empty() && !help.summary.ru.is_empty());
            assert_eq!(
                row.optional_features
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>(),
                help.optional_features.iter().collect()
            );
            assert!(settings_transition_allowed(
                &identity.ihx_sha256,
                &identity.ihx_sha256
            ));
        }
        for old in [
            "work",
            "essential",
            "mouse",
            "mouse-typing",
            "system",
            "tap-dance",
            "key-overrides",
            "settings",
        ] {
            assert!(serde_json::from_str::<ProfileId>(&format!("\"{old}\"")).is_err());
        }
    }
    #[test]
    fn counts_settings_direct_and_pairs_follow_exact_modules() {
        for id in IDS {
            let row = matrix_profile(id);
            assert_eq!(row.capabilities.dynamic_entries.combos, 8);
            assert_eq!(
                row.capabilities.dynamic_entries.tap_dance,
                if has_feature(id, "tap-dance") { 8 } else { 0 }
            );
            assert_eq!(
                row.capabilities.dynamic_entries.key_overrides,
                if has_feature(id, "key-overrides") {
                    8
                } else {
                    0
                }
            );
            assert_eq!(
                row.capabilities.runtime_qmk_settings_ids,
                if has_feature(id, "auto-shift") {
                    vec![1, 2, 4, 7, 18, 19]
                } else {
                    vec![1, 2, 7, 18, 19]
                }
            );
            assert_eq!(
                has_feature(id, "vialrgb-direct"),
                matches!(
                    id,
                    ProfileId::Main | ProfileId::Game | ProfileId::Mmo | ProfileId::Creative
                )
            );
            assert_eq!(
                rgb_modes(id).iter().any(|(_, wire)| *wire == 1),
                has_feature(id, "vialrgb-direct")
            );
        }
        assert!(
            has_feature(ProfileId::Shortcuts, "tap-dance")
                && has_feature(ProfileId::Shortcuts, "key-overrides")
        );
        assert_eq!(firmware_matrix().feature_pairs.len(), 136);
    }
    #[test]
    fn historical_registry_and_explicit_transition_direction_are_exact() {
        assert_eq!(historical_settings_identities().len(), 13);
        for source in historical_settings_identities() {
            assert!(
                historical_settings_identity(
                    source.ihx_sha256,
                    source.code_sha256,
                    source.nvm_schema
                )
                .is_some()
            );
            assert!(
                historical_settings_identity(
                    source.ihx_sha256,
                    source.code_sha256,
                    source.nvm_schema ^ 1
                )
                .is_none()
            );
            for id in IDS {
                let target = &matrix_profile(id).identity.ihx_sha256;
                assert!(settings_transition_allowed(source.ihx_sha256, target));
                assert!(!settings_transition_allowed(target, source.ihx_sha256));
            }
        }
        assert!(!settings_transition_allowed(
            "unknown",
            &matrix_profile(ProfileId::Main).identity.ihx_sha256
        ));
    }
    #[test]
    fn source_and_optional_keycode_semantics_are_conservative() {
        use KeycodeSource::*;
        for id in IDS {
            for (code, feature) in [
                (0x7c16, "grave-escape"),
                (0x7011, "nkro-control"),
                (0x5700, "tap-dance"),
                (0x7c5d, "key-overrides"),
                (0xcd, "mouse-keys"),
            ] {
                assert_eq!(supports_keycode(id, code, Matrix), has_feature(id, feature));
            }
            for source in [Encoder, Macro, MacroTap, MacroDown, MacroUp] {
                for code in [0x5700, 0x7c13, 0x7c53, 0x7c58] {
                    assert!(!supports_keycode(id, code, source));
                }
            }
            for source in [Macro, MacroDown, MacroUp] {
                assert!(!supports_keycode(id, 0x7c5d, source));
            }
            for code in [0x4604, 0x52c6, 0xffff] {
                assert!(!supports_keycode(id, code, Matrix));
            }
        }
        assert!(supports_keycode(ProfileId::Shortcuts, 0x7c5d, MacroTap));
        assert!(supports_keycode(ProfileId::Code, 0x7c16, Encoder));
        assert!(!supports_keycode(ProfileId::Code, 0x7c1a, MacroTap));
    }
    #[test]
    fn pinned_bytes_and_semantic_tampering_are_rejected() {
        let source = include_str!("../assets/ak820/beta3-firmware-matrix.json");
        assert!(parse_trusted_matrix(source).is_ok());
        for changed in [
            format!("{source} "),
            source.replacen("{", "{\"unknown\":true,", 1),
            source.replacen("{", "{\"format\":\"duplicate\",", 1),
        ] {
            assert!(parse_trusted_matrix(&changed).is_err());
        }
        let changes: [fn(&mut FirmwareMatrix); 6] = [
            |m| m.profiles[0].capabilities.dynamic_entries.combos = 7,
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
            |m| m.settings_transitions[0].target = "unknown".into(),
            |m| m.feature_pairs[0].profiles.push("main".into()),
        ];
        for change in changes {
            let mut matrix: FirmwareMatrix = serde_json::from_str(source).unwrap();
            change(&mut matrix);
            assert!(validate_contracts(&matrix).is_err());
        }
    }
}

/// Previous release identities remain exact and never inherit beta.3 features.
pub(crate) fn previous_firmware_matrix() -> &'static FirmwareMatrix {
    static PREVIOUS: OnceLock<FirmwareMatrix> = OnceLock::new();
    PREVIOUS.get_or_init(|| {
        let source = include_str!("../assets/ak820/beta2-firmware-matrix.json");
        assert_sha256(
            source,
            "e646cb3cb58c50c55a184538b77674db92e0d54750b1a9734d1b092a6c20a08e",
            "beta.2 source registry",
        );
        serde_json::from_str(source).expect("pinned beta.2 matrix")
    })
}
pub(crate) fn previous_matrix_profile(id: ProfileId) -> &'static MatrixProfile {
    previous_firmware_matrix()
        .profiles
        .iter()
        .find(|r| r.profile == id)
        .expect("previous exact profile")
}
