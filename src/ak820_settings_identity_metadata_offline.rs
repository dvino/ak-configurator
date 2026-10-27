// SPDX-License-Identifier: GPL-2.0-or-later
//! Provisional, offline-only validation of the compact-controls metadata projection.
//! Reference: firmware tools/installer/settings_backup.py::_settings_identity and
//! tools/build_capability_catalog.py::module_contract at firmware 88d190a
//! (2026-10-27).
//! This three-field projection is NOT a complete identity, catalog, capability
//! admission, or authorization. Hashes, features, keycodes, schema contents and
//! exact image trust must be checked independently before any future integration.
#![allow(dead_code)]

use crate::ak820_settings_backup::parse_json;

const MAX_METADATA_BYTES: usize = 256;
const SCHEMA03: u32 = 0x082003;
type MetadataResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsMetadata {
    Legacy,
    CompactV2,
}

/// Accept only an explicitly supplied three-field projection, never a complete
/// identity/catalog object. Configuration version is independently supplied;
/// the reference emits either no version field or integer 2, never explicit 1.
pub(crate) fn parse_metadata(
    raw: &[u8],
    configuration_version: Option<u8>,
) -> MetadataResult<SettingsMetadata> {
    if raw.len() > MAX_METADATA_BYTES {
        return Err("Oversized settings metadata projection".into());
    }
    let value = parse_json(raw)?;
    let fields = value
        .as_object()
        .ok_or("Expected settings metadata object")?;
    if fields.keys().any(|key| {
        !matches!(
            key.as_str(),
            "compact_controls_version" | "settings_record_version" | "combo_media"
        )
    }) {
        return Err("Unknown settings metadata projection field".into());
    }
    match configuration_version {
        None if fields.is_empty() => Ok(SettingsMetadata::Legacy),
        Some(2)
            if fields.len() == 3
                && fields["compact_controls_version"].as_u64() == Some(2)
                && fields["settings_record_version"].as_u64() == Some(2)
                && fields["combo_media"].as_bool() == Some(true) =>
        {
            Ok(SettingsMetadata::CompactV2)
        }
        _ => Err("Settings metadata differs from configuration".into()),
    }
}

impl SettingsMetadata {
    /// Compare the version portion of separately supplied QMK storage metadata.
    /// Remaining storage fields and consumer enablement are outside this bounded
    /// check. Schema03 stores QS even for dormant consumers. Schema01 is the
    /// packer's historical NVM layout; recognizing its lack of QS storage does
    /// not admit it as a settings identity or configuration.
    pub(crate) fn validate_storage_versions(
        self,
        schema: u32,
        record_version: Option<u8>,
        accepted_record_versions: &[u8],
    ) -> MetadataResult<()> {
        if !matches!(schema, 0x082001 | 0x082002 | SCHEMA03) {
            return Err("Unknown NVM schema".into());
        }
        let (expected_record, expected_accepted): (Option<u8>, &[u8]) = if schema != SCHEMA03 {
            (None, &[])
        } else {
            match self {
                Self::Legacy => (Some(1), &[1]),
                Self::CompactV2 => (Some(2), &[1, 2]),
            }
        };
        if record_version != expected_record || accepted_record_versions != expected_accepted {
            return Err("QMK storage versions differ from settings metadata".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: &[u8] =
        br#"{"compact_controls_version":2,"settings_record_version":2,"combo_media":true}"#;

    #[test]
    fn synthetic_legacy_and_compact_metadata() {
        assert_eq!(
            parse_metadata(b"{}", None).unwrap(),
            SettingsMetadata::Legacy
        );
        assert_eq!(
            parse_metadata(V2, Some(2)).unwrap(),
            SettingsMetadata::CompactV2
        );
        assert!(parse_metadata(V2, None).is_err());
        assert!(parse_metadata(b"{}", Some(2)).is_err());
        for version in [0, 1, 3, 255] {
            assert!(parse_metadata(V2, Some(version)).is_err());
            assert!(parse_metadata(b"{}", Some(version)).is_err());
        }
    }

    #[test]
    fn synthetic_rejects_partial_unknown_duplicate_and_wrong_types() {
        for raw in [
            r#"{"compact_controls_version":2}"#,
            r#"{"settings_record_version":2,"combo_media":true}"#,
            r#"{"compact_controls_version":2,"settings_record_version":1,"combo_media":true}"#,
            r#"{"compact_controls_version":2,"settings_record_version":2,"combo_media":false}"#,
            r#"{"compact_controls_version":2,"settings_record_version":2,"combo_media":1}"#,
            r#"{"compact_controls_version":"2","settings_record_version":2,"combo_media":true}"#,
            r#"{"compact_controls_version":2.0,"settings_record_version":2,"combo_media":true}"#,
            r#"{"compact_controls_version":2,"settings_record_version":null,"combo_media":true}"#,
            r#"{"compact_controls_version":2,"settings_record_version":2,"combo_media":true,"extra":0}"#,
            r#"{"compact_controls_version":2,"settings_record_version":2,"combo_media":true,"combo_media":true}"#,
            r#"{"compact_controls_version":null}"#,
            "null",
            "[]",
            "{} {}",
            "{",
        ] {
            assert!(parse_metadata(raw.as_bytes(), Some(2)).is_err(), "{raw}");
            assert!(parse_metadata(raw.as_bytes(), None).is_err(), "{raw}");
        }
        assert!(parse_metadata(&[b' '; MAX_METADATA_BYTES + 1], None).is_err());
    }

    #[test]
    fn synthetic_storage_versions_match_both_layouts_and_schema_presence() {
        for (metadata, record, accepted) in [
            (SettingsMetadata::Legacy, 1, vec![1]),
            (SettingsMetadata::CompactV2, 2, vec![1, 2]),
        ] {
            metadata
                .validate_storage_versions(SCHEMA03, Some(record), &accepted)
                .unwrap();
            metadata
                .validate_storage_versions(0x082002, None, &[])
                .unwrap();
            metadata
                .validate_storage_versions(0x082001, None, &[])
                .unwrap();
            for unknown in [0, 0x082004] {
                assert!(metadata
                    .validate_storage_versions(unknown, None, &[])
                    .is_err());
            }
            for wrong in [None, Some(0), Some(3)] {
                assert!(metadata
                    .validate_storage_versions(SCHEMA03, wrong, &accepted)
                    .is_err());
            }
            for wrong in [vec![], vec![2], vec![2, 1], vec![1, 1, 2]] {
                assert!(metadata
                    .validate_storage_versions(SCHEMA03, Some(record), &wrong)
                    .is_err());
            }
            assert!(metadata
                .validate_storage_versions(0x082002, Some(record), &accepted)
                .is_err());
        }
        assert!(SettingsMetadata::Legacy
            .validate_storage_versions(SCHEMA03, Some(2), &[1, 2])
            .is_err());
        assert!(SettingsMetadata::CompactV2
            .validate_storage_versions(SCHEMA03, Some(1), &[1])
            .is_err());
    }
}
