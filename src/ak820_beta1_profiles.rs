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
    pub(crate) algorithm: String,
    pub(crate) sha256: String,
    pub(crate) covered_bytes: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FirmwareMatrix {
    pub(crate) format: String,
    pub(crate) default_profile: String,
    pub(crate) profiles: Vec<MatrixProfile>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MatrixProfile {
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
pub(crate) struct MatrixCapabilities {
    pub(crate) optional_modules: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SettingsIdentity {
    pub(crate) code_sha256: String,
    pub(crate) ihx_sha256: String,
    pub(crate) nvm_schema: u32,
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
        if russian {
            &self.ru
        } else {
            &self.en
        }
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
        let matrix: FirmwareMatrix = serde_json::from_str(source)
            .expect("bundled beta.1 firmware matrix must be valid JSON");
        assert_eq!(matrix.format, "ak820-firmware-matrix-1");
        assert_eq!(matrix.default_profile, "work");
        assert_eq!(matrix.profiles.len(), 9, "beta.1 matrix has nine profiles");
        matrix
    })
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

    #[test]
    fn pinned_catalog_describes_all_nine_profiles_without_authorizing_writes() {
        assert_eq!(SOURCE_ARCHIVE_SHA256.len(), 64);
        let mut ids = Vec::new();
        for image in profiles() {
            assert!(!ids.contains(&image.profile.as_str()));
            ids.push(image.profile.as_str());
            assert!(!image.hardware_validated);
            assert!(!image.flashing_authorized);
            assert!(!image.settings_writes_authorized);
            let matrix = matrix_profile(image.profile);
            assert_eq!(matrix.installable, image.profile == ProfileId::Work);
            assert_eq!(matrix.release_approved, image.profile == ProfileId::Work);
            assert_eq!(
                profile_description(image.profile).id,
                image.profile.as_str()
            );
            for feature in &image.features {
                assert!(
                    feature_description(feature).is_some(),
                    "missing published module help for {feature}"
                );
            }
        }
        assert_eq!(ids.len(), 9);
        assert!(!catalog().hardware_validated);
        assert!(!catalog().flashing_authorized);
        assert!(!catalog().settings_writes_authorized);
    }
}
