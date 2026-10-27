// SPDX-License-Identifier: GPL-2.0-or-later
//! Isolated offline QS record model against the provisional 2026-10-27
//! compact-controls firmware reference. This is not image admission, a profile
//! capability, or a transport/write path. Existing typed storage stays separate.
#![allow(dead_code)]

type CodecResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordVersion {
    V1,
    V2,
}
impl TryFrom<u8> for RecordVersion {
    type Error = String;
    fn try_from(value: u8) -> CodecResult<Self> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            _ => Err("Unknown QS record version".into()),
        }
    }
}
// QSID, offset, width, minimum, maximum, default. First six are v1.
const FIELDS: [(u16, usize, usize, u16, u16, u16); 9] = [
    (1, 5, 1, 0, 15, 0),
    (2, 6, 2, 1, 1000, 40),
    (4, 14, 2, 1, 1000, 175),
    (7, 8, 2, 1, 1000, 200),
    (18, 10, 2, 0, 100, 0),
    (19, 12, 2, 0, 100, 80),
    (3, 16, 1, 0, 29, 1),
    (6, 18, 2, 0, 30000, 0),
    (22, 17, 1, 0, 1, 0),
];
fn field_count(version: RecordVersion) -> usize {
    match version {
        RecordVersion::V1 => 6,
        RecordVersion::V2 => 9,
    }
}
fn crc(raw: &[u8]) -> u16 {
    let mut value = 0xffffu16;
    for &byte in raw {
        value ^= (byte as u16) << 8;
        for _ in 0..8 {
            value = if value & 0x8000 != 0 {
                (value << 1) ^ 0x1021
            } else {
                value << 1
            };
        }
    }
    value
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingsRecord {
    version: RecordVersion,
    values: [u16; 9],
}
impl SettingsRecord {
    /// Explicit version also defines defaults for completely erased records.
    pub(crate) fn defaults(version: RecordVersion) -> Self {
        Self {
            version,
            values: FIELDS.map(|f| f.5),
        }
    }
    pub(crate) fn version(&self) -> RecordVersion {
        self.version
    }
    pub(crate) fn get(&self, id: u16) -> CodecResult<u16> {
        FIELDS[..field_count(self.version)]
            .iter()
            .position(|f| f.0 == id)
            .map(|i| self.values[i])
            .ok_or_else(|| "Unknown QSID for record version".into())
    }
    pub(crate) fn set(&mut self, id: u16, value: u16) -> CodecResult<()> {
        let i = FIELDS[..field_count(self.version)]
            .iter()
            .position(|f| f.0 == id)
            .ok_or("Unknown QSID for record version")?;
        if !(FIELDS[i].3..=FIELDS[i].4).contains(&value) || (id == 3 && value & !0x1d != 0) {
            return Err("QS value out of range".into());
        }
        self.values[i] = value;
        Ok(())
    }
    pub(crate) fn encode(&self) -> [u8; 24] {
        let mut raw = [0; 24];
        raw[..5].copy_from_slice(match self.version {
            RecordVersion::V1 => b"QS\x01\x0b\x3f",
            RecordVersion::V2 => b"QS\x02\x0f\x7f",
        });
        for (i, &(_, at, width, _, _, _)) in FIELDS[..field_count(self.version)].iter().enumerate()
        {
            raw[at..at + width].copy_from_slice(&self.values[i].to_le_bytes()[..width]);
        }
        let checksum = crc(&raw[..22]);
        raw[22..].copy_from_slice(&checksum.to_le_bytes());
        raw
    }
    /// Decode and normalize to an explicitly chosen target. A valid v1 record
    /// can gain v2 defaults; no v2 record may be silently narrowed to v1.
    pub(crate) fn decode(raw: &[u8], target: RecordVersion) -> CodecResult<Self> {
        if raw.len() != 24 {
            return Err("QS record requires 24 bytes".into());
        }
        if raw.iter().all(|&v| v == 0) || raw.iter().all(|&v| v == 255) {
            return Ok(Self::defaults(target));
        }
        let source = RecordVersion::try_from(raw[2])?;
        let mut result = Self::defaults(source);
        for &(id, at, width, _, _, _) in &FIELDS[..field_count(source)] {
            let value = if width == 1 {
                raw[at] as u16
            } else {
                u16::from_le_bytes([raw[at], raw[at + 1]])
            };
            result.set(id, value)?;
        }
        if raw != result.encode() {
            return Err("Invalid QS header, reserved bytes or CRC".into());
        }
        result.normalize(target)
    }
    pub(crate) fn normalize(&self, target: RecordVersion) -> CodecResult<Self> {
        if self.version == RecordVersion::V2 && target == RecordVersion::V1 {
            return Err("QS record downgrade is forbidden".into());
        }
        let mut result = self.clone();
        result.version = target;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RecordVersion::{V1, V2};
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
    // Generated directly from the provisional firmware Python reference's
    // settings_record(version=v), and each field set to its maximum, on
    // 2026-10-27. Synthetic records confer no image/device qualification.
    #[test]
    fn python_golden_defaults_and_maxima() {
        for (version, default, maximum) in [
            (
                V1,
                "5153010b3f002800c80000005000af000000000000007bc9",
                "5153010b3f0fe803e80364006400e803000000000000035e",
            ),
            (
                V2,
                "5153020f7f002800c80000005000af000100000000008e61",
                "5153020f7f0fe803e80364006400e8031d0130750000d152",
            ),
        ] {
            let mut model = SettingsRecord::defaults(version);
            assert_eq!(model.encode().as_slice(), hex(default));
            for &(id, _, _, _, high, _) in &FIELDS[..field_count(version)] {
                model.set(id, high).unwrap();
            }
            assert_eq!(model.encode().as_slice(), hex(maximum));
            assert_eq!(
                SettingsRecord::decode(&hex(maximum), version).unwrap(),
                model
            );
        }
    }
    #[test]
    fn explicit_forward_migration_and_erased_target() {
        let mut old = SettingsRecord::defaults(V1);
        old.set(4, 999).unwrap();
        let upgraded = SettingsRecord::decode(&old.encode(), V2).unwrap();
        assert_eq!(upgraded.version(), V2);
        assert_eq!(upgraded.get(4).unwrap(), 999);
        for (id, expected) in [(3, 1), (6, 0), (22, 0)] {
            assert_eq!(upgraded.get(id).unwrap(), expected);
            assert!(old.get(id).is_err());
        }
        assert!(upgraded.normalize(V1).is_err());
        assert!(SettingsRecord::decode(&upgraded.encode(), V1).is_err());
        for version in [V1, V2] {
            for blank in [0, 255] {
                assert_eq!(
                    SettingsRecord::decode(&[blank; 24], version).unwrap(),
                    SettingsRecord::defaults(version)
                );
            }
        }
        for value in [0, 3, 255] {
            assert!(RecordVersion::try_from(value).is_err());
        }
    }
    #[test]
    fn corrupt_header_crc_reserved_and_unknown_versions_rejected() {
        for version in [V1, V2] {
            let baseline = SettingsRecord::defaults(version).encode();
            for i in 0..24 {
                let mut bad = baseline;
                bad[i] ^= 1;
                assert!(SettingsRecord::decode(&bad, V2).is_err(), "byte {i}");
            }
            let reserved: &[usize] = if version == V1 {
                &[16, 17, 18, 19, 20, 21]
            } else {
                &[20, 21]
            };
            for i in [0, 1, 2, 3, 4].iter().chain(reserved) {
                let mut bad = baseline;
                bad[*i] ^= 0x80;
                let checksum = crc(&bad[..22]);
                bad[22..].copy_from_slice(&checksum.to_le_bytes());
                assert!(SettingsRecord::decode(&bad, V2).is_err());
            }
        }
        for size in [0, 23, 25] {
            assert!(SettingsRecord::decode(&vec![0; size], V2).is_err());
        }
    }
    #[test]
    fn all_field_bounds_and_option_mask() {
        let mut model = SettingsRecord::defaults(V2);
        for &(id, _, _, low, high, _) in &FIELDS {
            model.set(id, low).unwrap();
            model.set(id, high).unwrap();
            if low > 0 {
                assert!(model.set(id, low - 1).is_err());
            }
            assert!(model.set(id, high + 1).is_err());
        }
        for value in 0..=31 {
            assert_eq!(model.set(3, value).is_ok(), value & !0x1d == 0);
        }
        for id in [0, 5, 65535] {
            assert!(model.set(id, 0).is_err());
        }
        let mut old = SettingsRecord::defaults(V1);
        for id in [3, 6, 22] {
            assert!(old.set(id, 0).is_err());
        }
        // Out-of-range values are rejected even when the CRC is correct.
        let mut bad = SettingsRecord::defaults(V2).encode();
        bad[16] = 2;
        let checksum = crc(&bad[..22]);
        bad[22..].copy_from_slice(&checksum.to_le_bytes());
        assert!(SettingsRecord::decode(&bad, V2).is_err());
    }
}
