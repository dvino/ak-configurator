// SPDX-License-Identifier: GPL-2.0-or-later
//! QS record model for explicit image-version contracts.
//!
//! Beta.4's exact image identity and profile capability are verified before the
//! production connection path uses this model to reconcile settings read-only.
//! This codec does not admit images or authorize transport writes, backup,
//! restore or installation. Existing typed non-QS storage stays separate.
#![allow(dead_code)]

type CodecResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordVersion {
    V1,
    V2,
    V3,
}
impl TryFrom<u8> for RecordVersion {
    type Error = String;
    fn try_from(value: u8) -> CodecResult<Self> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            3 => Ok(Self::V3),
            _ => Err("Unknown QS record version".into()),
        }
    }
}
/// Offline image contract version, distinct from the persisted record header.
/// The provisional v2 image reads both v1 and v2 records; v1 reads only v1.
/// Callers must supply this explicitly; no image identity is admitted here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageVersion {
    V1,
    V2,
    V3,
}
impl TryFrom<u8> for ImageVersion {
    type Error = String;
    fn try_from(value: u8) -> CodecResult<Self> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            3 => Ok(Self::V3),
            _ => Err("Unknown QS image contract version".into()),
        }
    }
}
impl ImageVersion {
    fn record_version(self) -> RecordVersion {
        match self {
            Self::V1 => RecordVersion::V1,
            Self::V2 => RecordVersion::V2,
            Self::V3 => RecordVersion::V3,
        }
    }
    fn accepts(self, record: RecordVersion) -> bool {
        match self {
            Self::V1 => record == RecordVersion::V1,
            Self::V2 => record != RecordVersion::V3,
            Self::V3 => true,
        }
    }
}
/// An image format assumption; this does not establish image identity.
/// V3 requires the marker extracted from the exact target image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImageFormat {
    version: ImageVersion,
    marker: Option<u16>,
}
impl ImageFormat {
    pub(crate) fn version(self) -> ImageVersion {
        self.version
    }
    pub(crate) fn new(version: ImageVersion, marker: Option<u16>) -> CodecResult<Self> {
        match (version, marker) {
            (ImageVersion::V3, Some(1..=1000)) | (ImageVersion::V1 | ImageVersion::V2, None) => {
                Ok(Self { version, marker })
            }
            _ => Err("Invalid marker for QS image contract".into()),
        }
    }
}
// QSID, offset, width, minimum, maximum, default. First six are v1.
const FIELDS: [(u16, usize, usize, u16, u16, u16); 10] = [
    (1, 5, 1, 0, 15, 0),
    (2, 6, 2, 1, 1000, 40),
    (4, 14, 2, 1, 1000, 175),
    (7, 8, 2, 1, 1000, 200),
    (18, 10, 2, 0, 100, 0),
    (19, 12, 2, 0, 100, 80),
    (3, 16, 1, 0, 29, 1),
    (6, 18, 2, 0, 30000, 0),
    (22, 17, 1, 0, 1, 0),
    (23, 20, 2, 1, 1000, 0),
];
fn field_count(version: RecordVersion) -> usize {
    match version {
        RecordVersion::V1 => 6,
        RecordVersion::V2 => 9,
        RecordVersion::V3 => 10,
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
    values: [u16; 10],
}
impl SettingsRecord {
    /// Explicit version also defines defaults for completely erased records.
    pub(crate) fn defaults(version: RecordVersion) -> CodecResult<Self> {
        if version == RecordVersion::V3 {
            return Err("V3 requires an explicit marker contract".into());
        }
        Ok(Self::raw_defaults(version))
    }
    fn raw_defaults(version: RecordVersion) -> Self {
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
            RecordVersion::V3 => b"QS\x03\x11\xff",
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
        if target == RecordVersion::V3 || raw.get(2) == Some(&3) {
            return Err("V3 requires an explicit marker contract".into());
        }
        if raw.len() != 24 {
            return Err("QS record requires 24 bytes".into());
        }
        if raw.iter().all(|&v| v == 0) || raw.iter().all(|&v| v == 255) {
            return Self::defaults(target);
        }
        let source = RecordVersion::try_from(raw[2])?;
        let mut result = Self::defaults(source)?;
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
    /// Strict persisted-record contract, not a source image version. Nonblank
    /// headers must match exactly; blank defaults use the supplied record
    /// version. Use decode_for_image for a v2 image retaining a v1 record.
    /// Neither API admits an image identity.
    pub(crate) fn decode_for_source(
        raw: &[u8],
        source: RecordVersion,
        target: RecordVersion,
    ) -> CodecResult<Self> {
        if source == RecordVersion::V3 || target == RecordVersion::V3 {
            return Err("V3 requires an explicit marker contract".into());
        }
        if raw.len() != 24 {
            return Err("QS record requires 24 bytes".into());
        }
        if raw.iter().all(|&v| v == 0) || raw.iter().all(|&v| v == 255) {
            return Self::defaults(source)?.normalize(target);
        }
        if RecordVersion::try_from(raw[2])? != source {
            return Err("QS record version differs from explicit source contract".into());
        }
        Self::decode(raw, source)?.normalize(target)
    }
    /// Decode within an explicit source IMAGE contract, then normalize to the
    /// target IMAGE contract. Mirrors the published beta.4 reference behavior.
    /// Image downgrade is forbidden even if storage remains v1 or erased.
    /// Production callers must first verify exact image identity and profile
    /// capability. The beta.4 connection uses this only for readback; it does
    /// not authorize writes, backup, restore or installation.
    pub(crate) fn decode_for_image(
        raw: &[u8],
        source: ImageVersion,
        target: ImageVersion,
    ) -> CodecResult<Self> {
        if source == ImageVersion::V3 || target == ImageVersion::V3 {
            return Err("V3 requires an explicit marker contract".into());
        }
        if source == ImageVersion::V2 && target == ImageVersion::V1 {
            return Err("QS image contract downgrade is forbidden".into());
        }
        if raw.len() != 24 {
            return Err("QS record requires 24 bytes".into());
        }
        let erased = raw.iter().all(|&v| v == 0) || raw.iter().all(|&v| v == 255);
        if !erased && !source.accepts(RecordVersion::try_from(raw[2])?) {
            return Err("QS record version is not accepted by source image contract".into());
        }
        Self::decode(raw, source.record_version())?.normalize(target.record_version())
    }
    pub(crate) fn normalize(&self, target: RecordVersion) -> CodecResult<Self> {
        if self.version == RecordVersion::V3 || target == RecordVersion::V3 {
            return Err("V3 requires an explicit marker contract".into());
        }
        if self.version == RecordVersion::V2 && target == RecordVersion::V1 {
            return Err("QS record downgrade is forbidden".into());
        }
        let mut result = self.clone();
        result.version = target;
        Ok(result)
    }

    /// Model firmware startup: erased storage uses the target image's C
    /// defaults; a valid older record is upgraded using its target marker.
    /// An erased record has no inferred predecessor settings.
    pub(crate) fn decode_for_formats(
        raw: &[u8],
        source: ImageFormat,
        target: ImageFormat,
    ) -> CodecResult<Self> {
        if (source.version as u8) > (target.version as u8) {
            return Err("QS image contract downgrade is forbidden".into());
        }
        if raw.len() != 24 {
            return Err("QS record requires 24 bytes".into());
        }
        if raw.iter().all(|&v| v == 0) || raw.iter().all(|&v| v == 255) {
            let mut result = Self::raw_defaults(target.version.record_version());
            if let Some(marker) = target.marker {
                result.set(22, 1)?;
                result.set(23, marker)?;
            }
            return Ok(result);
        }
        let version = RecordVersion::try_from(raw[2])?;
        if !source.version.accepts(version) {
            return Err("QS record version is not accepted by source image contract".into());
        }
        let mut result = Self::raw_defaults(version);
        for &(id, at, width, _, _, _) in &FIELDS[..field_count(version)] {
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
        let target_version = target.version.record_version();
        if (version as u8) > (target_version as u8) {
            return Err("QS record downgrade is forbidden".into());
        }
        result.version = target_version;
        if version != RecordVersion::V3 && target_version == RecordVersion::V3 {
            result.set(23, target.marker.ok_or("Missing QS target marker")?)?;
        }
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
    #[test]
    fn v3_published_vectors_and_startup_migration() {
        let parsed = RecordVersion::try_from(3).unwrap();
        assert!(SettingsRecord::defaults(parsed).is_err());
        let f1 = ImageFormat::new(ImageVersion::V1, None).unwrap();
        let f2 = ImageFormat::new(ImageVersion::V2, None).unwrap();
        let f3 = ImageFormat::new(ImageVersion::V3, Some(300)).unwrap();
        let blank = hex("51530311ff002800c80000005000af00010100002c01ba0e");
        for erased in [0, 255] {
            for source in [f1, f2, f3] {
                assert_eq!(
                    SettingsRecord::decode_for_formats(&[erased; 24], source, f3)
                        .unwrap()
                        .encode()
                        .as_slice(),
                    blank
                );
            }
        }
        let python = hex("51530311ff002800c80000005000af00010000002c01eba4");
        let max = hex("51530311ff0fe803e80364006400e8031d013075e803666d");
        for raw in [&python, &max] {
            assert_eq!(
                SettingsRecord::decode_for_formats(raw, f3, f3)
                    .unwrap()
                    .encode()
                    .as_slice(),
                raw
            );
        }
        let v1 = SettingsRecord::defaults(V1).unwrap().encode();
        let upgraded = SettingsRecord::decode_for_formats(&v1, f1, f3).unwrap();
        assert_eq!(upgraded.encode().as_slice(), python);
        let mut v2 = SettingsRecord::defaults(V2).unwrap();
        v2.set(22, 1).unwrap();
        assert_eq!(
            SettingsRecord::decode_for_formats(&v2.encode(), f2, f3)
                .unwrap()
                .encode()
                .as_slice(),
            blank
        );
        let other_marker = ImageFormat::new(ImageVersion::V3, Some(800)).unwrap();
        assert_eq!(
            SettingsRecord::decode_for_formats(&python, f3, other_marker)
                .unwrap()
                .get(23)
                .unwrap(),
            300
        );
        assert_eq!(
            SettingsRecord::decode_for_formats(&v1, f1, other_marker)
                .unwrap()
                .get(23)
                .unwrap(),
            800
        );
        assert!(SettingsRecord::decode_for_formats(&[255; 24], f3, f2).is_err());
        assert!(SettingsRecord::decode_for_formats(&python, f2, f3).is_err());
        assert!(ImageFormat::new(ImageVersion::V3, None).is_err());
        assert!(ImageFormat::new(ImageVersion::V3, Some(0)).is_err());
        assert!(ImageFormat::new(ImageVersion::V3, Some(1001)).is_err());
        assert!(ImageFormat::new(ImageVersion::V2, Some(300)).is_err());
    }
    #[test]
    fn v3_rejects_corruption_and_out_of_range_even_with_valid_crc() {
        let f3 = ImageFormat::new(ImageVersion::V3, Some(300)).unwrap();
        let raw = SettingsRecord::decode_for_formats(&[255; 24], f3, f3)
            .unwrap()
            .encode();
        for i in 0..24 {
            let mut bad = raw;
            bad[i] ^= 1;
            assert!(
                SettingsRecord::decode_for_formats(&bad, f3, f3).is_err(),
                "byte {i}"
            );
        }
        for value in [0u16, 1001, 65535] {
            let mut bad = raw;
            bad[20..22].copy_from_slice(&value.to_le_bytes());
            let checksum = crc(&bad[..22]);
            bad[22..].copy_from_slice(&checksum.to_le_bytes());
            assert!(SettingsRecord::decode_for_formats(&bad, f3, f3).is_err());
        }
        for value in [1u16, 1000] {
            let mut record = SettingsRecord::decode_for_formats(&raw, f3, f3).unwrap();
            record.set(23, value).unwrap();
            assert_eq!(
                SettingsRecord::decode_for_formats(&record.encode(), f3, f3)
                    .unwrap()
                    .get(23)
                    .unwrap(),
                value
            );
        }
        for len in [0, 23, 25] {
            assert!(SettingsRecord::decode_for_formats(&vec![0; len], f3, f3).is_err());
        }
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
            let mut model = SettingsRecord::defaults(version).unwrap();
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
        let mut old = SettingsRecord::defaults(V1).unwrap();
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
                    SettingsRecord::defaults(version).unwrap()
                );
            }
        }
        for value in [0, 4, 255] {
            assert!(RecordVersion::try_from(value).is_err());
        }
    }
    #[test]
    fn corrupt_header_crc_reserved_and_unknown_versions_rejected() {
        for version in [V1, V2] {
            let baseline = SettingsRecord::defaults(version).unwrap().encode();
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
    fn source_bound_decode_and_normalization() {
        for version in [V1, V2] {
            let mut model = SettingsRecord::defaults(version).unwrap();
            model.set(2, 913).unwrap();
            assert_eq!(
                SettingsRecord::decode_for_source(&model.encode(), version, version).unwrap(),
                model
            );
            let other = if version == V1 { V2 } else { V1 };
            assert!(SettingsRecord::decode_for_source(&model.encode(), other, V2).is_err());
            for byte in 0..24 {
                let mut bad = model.encode();
                bad[byte] ^= 1;
                assert!(SettingsRecord::decode_for_source(&bad, version, V2).is_err());
            }
        }
        let mut old = SettingsRecord::defaults(V1).unwrap();
        for (id, value) in [(1, 15), (2, 913), (4, 996), (7, 1), (18, 99), (19, 100)] {
            old.set(id, value).unwrap();
        }
        let upgraded = SettingsRecord::decode_for_source(&old.encode(), V1, V2).unwrap();
        for &(id, _, _, _, _, _) in &FIELDS[..6] {
            assert_eq!(upgraded.get(id).unwrap(), old.get(id).unwrap());
        }
        for (id, value) in [(3, 1), (6, 0), (22, 0)] {
            assert_eq!(upgraded.get(id).unwrap(), value);
        }
        assert!(SettingsRecord::decode_for_source(&upgraded.encode(), V2, V1).is_err());
        for blank in [0, 255] {
            assert!(SettingsRecord::decode_for_source(&[blank; 24], V2, V1).is_err());
            for (source, target) in [(V1, V1), (V1, V2), (V2, V2)] {
                assert_eq!(
                    SettingsRecord::decode_for_source(&[blank; 24], source, target).unwrap(),
                    SettingsRecord::defaults(target).unwrap()
                );
            }
        }
        for size in [0, 23, 25] {
            assert!(SettingsRecord::decode_for_source(&vec![0; size], V1, V2).is_err());
        }
    }
    #[test]
    fn image_contract_acceptance_migration_and_erased_defaults() {
        use ImageVersion::{V1 as I1, V2 as I2};
        let mut old = SettingsRecord::defaults(V1).unwrap();
        old.set(2, 913).unwrap();
        old.set(4, 996).unwrap();
        let mut current = SettingsRecord::defaults(V2).unwrap();
        current.set(3, 29).unwrap();
        current.set(6, 30000).unwrap();
        current.set(22, 1).unwrap();
        for source in [I1, I2] {
            for target in [I1, I2] {
                for (record, persisted) in [
                    (old.encode(), Some(V1)),
                    (current.encode(), Some(V2)),
                    ([0; 24], None),
                    ([255; 24], None),
                ] {
                    let result = SettingsRecord::decode_for_image(&record, source, target);
                    let allowed =
                        !(source == I2 && target == I1 || source == I1 && persisted == Some(V2));
                    assert_eq!(
                        result.is_ok(),
                        allowed,
                        "{source:?} -> {target:?}, persisted {persisted:?}"
                    );
                    if allowed {
                        let expected = match persisted {
                            Some(V1) => old.normalize(target.record_version()).unwrap(),
                            Some(V2) => current.clone(),
                            Some(RecordVersion::V3) => unreachable!(),
                            None => SettingsRecord::defaults(source.record_version())
                                .unwrap()
                                .normalize(target.record_version())
                                .unwrap(),
                        };
                        assert_eq!(result.unwrap(), expected);
                    }
                }
            }
        }
        // Both images validate the full stored record, including corrected-CRC
        // reserved corruption, rather than accepting only its version byte.
        for (source, model) in [(I1, old), (I2, current)] {
            for byte in 0..24 {
                let mut bad = model.encode();
                bad[byte] ^= 1;
                assert!(SettingsRecord::decode_for_image(&bad, source, I2).is_err());
            }
            let mut bad = model.encode();
            bad[21] = 1;
            let checksum = crc(&bad[..22]);
            bad[22..].copy_from_slice(&checksum.to_le_bytes());
            assert!(SettingsRecord::decode_for_image(&bad, source, I2).is_err());
            for size in [0, 23, 25] {
                assert!(SettingsRecord::decode_for_image(&vec![0; size], source, I2).is_err());
            }
        }
        for version in [0, 4, 255] {
            assert!(ImageVersion::try_from(version).is_err());
        }
    }
    #[test]
    fn all_field_bounds_and_option_mask() {
        let mut model = SettingsRecord::defaults(V2).unwrap();
        for &(id, _, _, low, high, _) in &FIELDS[..field_count(V2)] {
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
        let mut old = SettingsRecord::defaults(V1).unwrap();
        for id in [3, 6, 22] {
            assert!(old.set(id, 0).is_err());
        }
        // Out-of-range values are rejected even when the CRC is correct.
        let mut bad = SettingsRecord::defaults(V2).unwrap().encode();
        bad[16] = 2;
        let checksum = crc(&bad[..22]);
        bad[22..].copy_from_slice(&checksum.to_le_bytes());
        assert!(SettingsRecord::decode(&bad, V2).is_err());
    }
}
