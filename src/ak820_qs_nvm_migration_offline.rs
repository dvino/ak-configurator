// SPDX-License-Identifier: GPL-2.0-or-later
//! Pure offline preparation for the provisional compact-controls QS format.
//! Layout: firmware eeconfig.h (0xFEE6), VIA schema at 37, and
//! util/ak820_extension_settings.py (QS at 0x6a8, 24 bytes).
//! Schema03 does not identify a record version or authorize an image transition.
//! No production caller, identity admission, transport or persistence is provided.
#![allow(dead_code)]

use crate::ak820_qs_versioned_offline::{RecordVersion, SettingsRecord};

const NVM_SIZE: usize = 0x2800;
const QS_START: usize = 0x6a8;
const QS_END: usize = QS_START + 24;

/// Return a separate prepared NVM payload with only its QS record normalized.
/// Source and target versions are explicit format assumptions, not trusted
/// image identities. All other bytes are opaque and preserved, not validated.
/// Completely zero/FF QS records materialize canonical defaults, following the
/// Python offline codec; this does not claim firmware saves its RAM upgrade.
pub(crate) fn prepare_qs_nvm(
    nvm: &[u8],
    source: RecordVersion,
    target: RecordVersion,
) -> Result<Vec<u8>, String> {
    if nvm.len() != NVM_SIZE {
        return Err("Incomplete or oversized NVM".into());
    }
    if nvm[..2] != [0xe6, 0xfe] || nvm[37..40] != [3, 0x20, 8] {
        return Err("NVM magic or schema03 differs".into());
    }
    let record = SettingsRecord::decode_for_source(&nvm[QS_START..QS_END], source, target)?;
    let mut prepared = nvm.to_vec();
    prepared[QS_START..QS_END].copy_from_slice(&record.encode());
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use RecordVersion::{V1, V2};

    fn carrier(record: &[u8; 24]) -> Vec<u8> {
        // Nontrivial opaque bytes include extensions, macros and reserved space.
        let mut nvm: Vec<u8> = (0..NVM_SIZE)
            .map(|i| (i.wrapping_mul(73) >> 3) as u8)
            .collect();
        nvm[..2].copy_from_slice(&[0xe6, 0xfe]);
        nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
        nvm[QS_START..QS_END].copy_from_slice(record);
        nvm
    }
    fn assert_opaque_unchanged(before: &[u8], after: &[u8]) {
        assert_eq!(after.len(), NVM_SIZE);
        assert_eq!(&before[..QS_START], &after[..QS_START]);
        assert_eq!(&before[QS_END..], &after[QS_END..]);
    }

    #[test]
    fn forward_conversion_preserves_values_opaque_bytes_and_input() {
        let mut old = SettingsRecord::defaults(V1);
        for (id, value) in [(1, 15), (2, 913), (4, 996), (7, 1), (18, 99), (19, 100)] {
            old.set(id, value).unwrap();
        }
        let input = carrier(&old.encode());
        let snapshot = input.clone();
        let output = prepare_qs_nvm(&input, V1, V2).unwrap();
        assert_eq!(input, snapshot);
        assert_opaque_unchanged(&input, &output);
        let new = SettingsRecord::decode_for_source(&output[QS_START..QS_END], V2, V2).unwrap();
        for id in [1, 2, 4, 7, 18, 19] {
            assert_eq!(new.get(id).unwrap(), old.get(id).unwrap());
        }
        for (id, value) in [(3, 1), (6, 0), (22, 0)] {
            assert_eq!(new.get(id).unwrap(), value);
        }
        assert_eq!(prepare_qs_nvm(&output, V2, V2).unwrap(), output);
        assert!(prepare_qs_nvm(&output, V2, V1).is_err());
        assert_eq!(prepare_qs_nvm(&input, V1, V1).unwrap(), input);
    }

    #[test]
    fn blank_sources_materialize_python_reference_defaults_without_downgrade() {
        // Python settings_record golden defaults from the provisional reference.
        let v2 = [
            0x51, 0x53, 2, 0x0f, 0x7f, 0, 0x28, 0, 0xc8, 0, 0, 0, 0x50, 0, 0xaf, 0, 1, 0, 0, 0, 0,
            0, 0x8e, 0x61,
        ];
        for blank in [0, 255] {
            let input = carrier(&[blank; 24]);
            let snapshot = input.clone();
            for (source, target) in [(V1, V1), (V1, V2), (V2, V2)] {
                let output = prepare_qs_nvm(&input, source, target).unwrap();
                assert_opaque_unchanged(&input, &output);
                assert_eq!(
                    &output[QS_START..QS_END],
                    SettingsRecord::defaults(target).encode()
                );
                if target == V2 {
                    assert_eq!(&output[QS_START..QS_END], v2);
                }
            }
            assert!(prepare_qs_nvm(&input, V2, V1).is_err());
            assert_eq!(input, snapshot);
        }
    }

    #[test]
    fn rejects_bad_size_magic_schema_corruption_and_source_mismatch() {
        for size in [0, QS_END, NVM_SIZE - 1, NVM_SIZE + 1] {
            assert!(prepare_qs_nvm(&vec![0; size], V1, V2).is_err());
        }
        for source in [V1, V2] {
            let input = carrier(&SettingsRecord::defaults(source).encode());
            let other = if source == V1 { V2 } else { V1 };
            assert!(prepare_qs_nvm(&input, other, V2).is_err());
            for index in [0, 1, 37, 38, 39].into_iter().chain(QS_START..QS_END) {
                let mut bad = input.clone();
                bad[index] ^= 1;
                let snapshot = bad.clone();
                assert!(prepare_qs_nvm(&bad, source, V2).is_err(), "byte {index}");
                assert_eq!(bad, snapshot);
            }
        }
    }
}
