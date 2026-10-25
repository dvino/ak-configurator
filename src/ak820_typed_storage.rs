// SPDX-License-Identifier: GPL-2.0-or-later
//! Offline schema03 codec ported from ak820 bef20f89d3da01d770767919bc7c8fc3c3a1a808
//! util/ak820_extension_settings.py and keyboards/ajazz/ak820/ak820_settings.c.
//! No transport, authorization, migration or live-write implementation.
use crate::app::ak820_beta1_profiles::{ProfileId, matrix_profile};

pub(crate) const SCHEMA: u32 = 0x082003;
const SETTINGS: usize = 0x6a8;
const FIELDS: [(u16, usize, usize, u16, u16, u16); 6] = [
    (1, 5, 1, 0, 15, 0),
    (2, 6, 2, 1, 1000, 40),
    (4, 14, 2, 1, 1000, 175),
    (7, 8, 2, 1, 1000, 200),
    (18, 10, 2, 0, 100, 0),
    (19, 12, 2, 0, 100, 80),
];
type CodecResult<T> = Result<T, String>;
fn fail<T>(message: &str) -> CodecResult<T> {
    Err(message.into())
}
fn word(raw: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([raw[at], raw[at + 1]])
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

/// All persisted QSIDs, including inactive Auto Shift timeout (4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QmkSettings {
    values: [u16; 6],
}
impl Default for QmkSettings {
    fn default() -> Self {
        Self {
            values: FIELDS.map(|f| f.5),
        }
    }
}
impl QmkSettings {
    pub(crate) fn get(&self, qsid: u16) -> CodecResult<u16> {
        FIELDS
            .iter()
            .position(|f| f.0 == qsid)
            .map(|i| self.values[i])
            .ok_or_else(|| "Unknown QSID".into())
    }
    /// Storage-level setter; profile accessibility is checked by apply_to_nvm.
    pub(crate) fn set(&mut self, qsid: u16, value: u16) -> CodecResult<()> {
        let Some(i) = FIELDS.iter().position(|f| f.0 == qsid) else {
            return fail("Unknown QSID");
        };
        if !(FIELDS[i].3..=FIELDS[i].4).contains(&value) {
            return fail("Settings value out of range");
        }
        self.values[i] = value;
        Ok(())
    }
    pub(crate) fn encode(&self) -> [u8; 24] {
        let mut raw = [0; 24];
        raw[..5].copy_from_slice(b"QS\x01\x0b\x3f");
        for (i, &(_, at, width, _, _, _)) in FIELDS.iter().enumerate() {
            raw[at..at + width].copy_from_slice(&self.values[i].to_le_bytes()[..width]);
        }
        let checksum = crc(&raw[..22]);
        raw[22..].copy_from_slice(&checksum.to_le_bytes());
        raw
    }
    pub(crate) fn decode(raw: &[u8]) -> CodecResult<Self> {
        if raw.len() != 24 {
            return fail("Settings record requires 24 bytes");
        }
        if raw.iter().all(|&v| v == 0) || raw.iter().all(|&v| v == 255) {
            return Ok(Self::default());
        }
        let mut result = Self::default();
        for &(id, at, width, _, _, _) in &FIELDS {
            result.set(
                id,
                if width == 1 {
                    raw[at] as u16
                } else {
                    word(raw, at)
                },
            )?;
        }
        if raw != result.encode() {
            return fail("Invalid settings header, reserved bytes or CRC");
        }
        Ok(result)
    }
}
fn output_valid(value: u16, modifiers: bool) -> bool {
    value == 0
        || (value < 0x2000
            && ((4..=0xa4).contains(&(value & 255))
                || (modifiers && (0xe0..=0xe7).contains(&(value & 255)))))
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TapDance {
    pub(crate) outputs: [u16; 4],
    pub(crate) term: u16,
}
impl TapDance {
    pub(crate) fn encode(self) -> CodecResult<[u8; 10]> {
        if !self.outputs.iter().all(|&v| output_valid(v, true)) || self.term > 32767 {
            return fail("Invalid Tap Dance output or term");
        }
        let mut raw = [0; 10];
        for (i, v) in self.outputs.into_iter().chain([self.term]).enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        Ok(raw)
    }
    pub(crate) fn decode(raw: &[u8]) -> CodecResult<Self> {
        if raw.len() != 10 {
            return fail("Tap Dance entry requires ten bytes");
        }
        let result = Self {
            outputs: std::array::from_fn(|i| word(raw, i * 2)),
            term: word(raw, 8),
        };
        result.encode()?;
        Ok(result)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyOverride {
    pub(crate) trigger: u16,
    pub(crate) replacement: u16,
    pub(crate) layers: u16,
    pub(crate) mods: u8,
    pub(crate) negative: u8,
    pub(crate) suppressed: u8,
    pub(crate) options: u8,
}
impl KeyOverride {
    pub(crate) fn encode(self) -> CodecResult<[u8; 10]> {
        if self != Self::default()
            && (!(4..=0xa4).contains(&self.trigger)
                || !output_valid(self.replacement, false)
                || !(1..=63).contains(&self.layers)
                || self.options & 0x40 != 0
                || self.options & 7 == 0)
        {
            return fail("Invalid Key Override entry");
        }
        let mut raw = [0; 10];
        for (i, v) in [self.trigger, self.replacement, self.layers]
            .into_iter()
            .enumerate()
        {
            raw[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        raw[6..].copy_from_slice(&[self.mods, self.negative, self.suppressed, self.options]);
        Ok(raw)
    }
    pub(crate) fn decode(raw: &[u8]) -> CodecResult<Self> {
        if raw.len() != 10 {
            return fail("Key Override entry requires ten bytes");
        }
        let result = Self {
            trigger: word(raw, 0),
            replacement: word(raw, 2),
            layers: word(raw, 4),
            mods: raw[6],
            negative: raw[7],
            suppressed: raw[8],
            options: raw[9],
        };
        result.encode()?;
        Ok(result)
    }
}
pub(crate) fn slot_offset(tap_dance: bool, index: usize) -> CodecResult<usize> {
    if index >= 8 {
        return fail("Invalid extension index");
    }
    Ok((if tap_dance { 0x6c0 } else { 0x740 })
        + if index < 6 {
            index * 10
        } else {
            64 + (index - 6) * 10
        })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Extensions {
    pub(crate) settings: QmkSettings,
    pub(crate) tap_dance: [TapDance; 8],
    pub(crate) key_overrides: [KeyOverride; 8],
}
impl Extensions {
    pub(crate) fn decode_nvm(nvm: &[u8]) -> CodecResult<Self> {
        if nvm.len() != 0x2800 || nvm[37..40] != [3, 0x20, 8] {
            return fail("Extension NVM size or schema differs");
        }
        let mut result = Self {
            settings: QmkSettings::decode(&nvm[SETTINGS..SETTINGS + 24])?,
            tap_dance: [TapDance::default(); 8],
            key_overrides: [KeyOverride::default(); 8],
        };
        for i in 0..8 {
            let at = slot_offset(true, i)?;
            result.tap_dance[i] = TapDance::decode(&nvm[at..at + 10])?;
            let at = slot_offset(false, i)?;
            result.key_overrides[i] = KeyOverride::decode(&nvm[at..at + 10])?;
        }
        Ok(result)
    }
    /// Produce an offline candidate, preserving every byte outside changed records.
    /// Exact image admission and device writes remain the caller's separate gates.
    pub(crate) fn apply_to_nvm(&self, before: &[u8], profile: ProfileId) -> CodecResult<Vec<u8>> {
        let old = Self::decode_nvm(before)?;
        let contract = matrix_profile(profile);
        if contract.identity.nvm_schema != SCHEMA {
            return fail("Profile is not schema03");
        }
        for &(id, _, _, _, _, _) in &FIELDS {
            if !contract.capabilities.runtime_qmk_settings_ids.contains(&id)
                && self.settings.get(id)? != old.settings.get(id)?
            {
                return fail("Inactive QMK setting differs");
            }
        }
        if contract.capabilities.dynamic_entries.tap_dance == 0 && self.tap_dance != old.tap_dance {
            return fail("Inactive Tap Dance differs");
        }
        if contract.capabilities.dynamic_entries.key_overrides == 0
            && self.key_overrides != old.key_overrides
        {
            return fail("Inactive Key Overrides differ");
        }
        let mut after = before.to_vec();
        if self.settings != old.settings {
            after[SETTINGS..SETTINGS + 24].copy_from_slice(&self.settings.encode());
        }
        for i in 0..8 {
            let raw = self.tap_dance[i].encode()?;
            let at = slot_offset(true, i)?;
            after[at..at + 10].copy_from_slice(&raw);
            let raw = self.key_overrides[i].encode()?;
            let at = slot_offset(false, i)?;
            after[at..at + 10].copy_from_slice(&raw);
        }
        Ok(after)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
    fn nvm(blank: u8) -> Vec<u8> {
        let mut raw = vec![0x95; 0x2800];
        raw[37..40].copy_from_slice(&[3, 0x20, 8]);
        raw[SETTINGS..SETTINGS + 24].fill(blank);
        for i in 0..8 {
            for td in [true, false] {
                let at = slot_offset(td, i).unwrap();
                raw[at..at + 10].fill(0);
            }
        }
        raw
    }
    #[test]
    fn settings_firmware_golden_and_crc_reserved_checks() {
        // default_record in ak820_settings.c; second vector produced by pinned
        // ak820_extension_settings.settings_record with all boundary values.
        let golden = hex("5153010b3f002800c80000005000af000000000000007bc9");
        assert_eq!(QmkSettings::default().encode().as_slice(), golden);
        let mut settings = QmkSettings::default();
        for (id, value) in [(1, 15), (2, 1000), (4, 1000), (7, 1), (18, 100), (19, 0)] {
            settings.set(id, value).unwrap();
        }
        assert_eq!(
            settings.encode().as_slice(),
            hex("5153010b3f0fe803010064000000e8030000000000008006")
        );
        assert_eq!(QmkSettings::decode(&settings.encode()).unwrap(), settings);
        for i in 0..24 {
            let mut bad = golden.clone();
            bad[i] ^= 1;
            assert!(QmkSettings::decode(&bad).is_err());
        }
        // Reserved/header rejection must hold even with a correct checksum.
        for i in [0, 2, 3, 4, 16, 21] {
            let mut bad = golden.clone();
            bad[i] ^= 1;
            let c = crc(&bad[..22]);
            bad[22..].copy_from_slice(&c.to_le_bytes());
            assert!(QmkSettings::decode(&bad).is_err());
        }
        for (id, value) in [
            (0, 1),
            (1, 16),
            (2, 0),
            (4, 1001),
            (7, 0),
            (18, 101),
            (19, 101),
        ] {
            assert!(settings.set(id, value).is_err());
        }
        for size in [0, 23, 25] {
            assert!(QmkSettings::decode(&vec![0; size]).is_err());
        }
    }
    #[test]
    fn table_reference_vectors_and_invalid_bounds() {
        let td = TapDance {
            outputs: [4, 0x104, 0, 0],
            term: 32767,
        };
        assert_eq!(td.encode().unwrap().as_slice(), hex("0400040100000000ff7f"));
        assert_eq!(TapDance::decode(&td.encode().unwrap()).unwrap(), td);
        let ko = KeyOverride {
            trigger: 4,
            replacement: 5,
            layers: 63,
            mods: 1,
            negative: 0,
            suppressed: 0,
            options: 1,
        };
        assert_eq!(ko.encode().unwrap().as_slice(), hex("040005003f0001000001"));
        assert_eq!(KeyOverride::decode(&ko.encode().unwrap()).unwrap(), ko);
        for value in [1, 3, 0xa5, 0xe8, 0x2004, 0xffff] {
            assert!(
                TapDance {
                    outputs: [value, 0, 0, 0],
                    term: 0
                }
                .encode()
                .is_err()
            );
        }
        assert!(TapDance { term: 32768, ..td }.encode().is_err());
        for bad in [
            KeyOverride { trigger: 0, ..ko },
            KeyOverride {
                replacement: 0xe0,
                ..ko
            },
            KeyOverride { layers: 0, ..ko },
            KeyOverride { layers: 64, ..ko },
            KeyOverride { options: 65, ..ko },
            KeyOverride { options: 128, ..ko },
        ] {
            assert!(bad.encode().is_err());
        }
        assert!(
            KeyOverride {
                replacement: 0,
                options: 0xb9,
                ..ko
            }
            .encode()
            .is_ok()
        );
        for size in [0, 9, 11] {
            assert!(TapDance::decode(&vec![0; size]).is_err());
            assert!(KeyOverride::decode(&vec![0; size]).is_err());
        }
        assert!(slot_offset(true, 8).is_err());
        for td in [true, false] {
            assert_eq!(slot_offset(td, 6).unwrap() % 64, 0);
        }
    }
    #[test]
    fn blank_noop_and_inactive_neighbors_preserved() {
        for blank in [0, 255] {
            let before = nvm(blank);
            let mut model = Extensions::decode_nvm(&before).unwrap();
            for profile in [
                ProfileId::TapDance,
                ProfileId::KeyOverrides,
                ProfileId::Settings,
            ] {
                assert_eq!(model.apply_to_nvm(&before, profile).unwrap(), before);
            }
            model.tap_dance[7] = TapDance {
                outputs: [0x1fe7, 4, 0, 0],
                term: 1,
            };
            assert!(model.apply_to_nvm(&before, ProfileId::Settings).is_err());
            let after = model.apply_to_nvm(&before, ProfileId::TapDance).unwrap();
            let at = slot_offset(true, 7).unwrap();
            assert_eq!(&after[..at], &before[..at]);
            assert_eq!(&after[at + 10..], &before[at + 10..]);
            let mut model = Extensions::decode_nvm(&before).unwrap();
            model.settings.set(4, 176).unwrap();
            assert!(model.apply_to_nvm(&before, ProfileId::Settings).is_err());
            model.settings.set(4, 175).unwrap();
            model.settings.set(2, 41).unwrap();
            assert!(model.apply_to_nvm(&before, ProfileId::TapDance).is_err());
            let after = model.apply_to_nvm(&before, ProfileId::Settings).unwrap();
            assert_eq!(&after[..SETTINGS], &before[..SETTINGS]);
            assert_eq!(&after[SETTINGS + 24..], &before[SETTINGS + 24..]);
            assert_eq!(
                Extensions::decode_nvm(&after)
                    .unwrap()
                    .settings
                    .get(2)
                    .unwrap(),
                41
            );
            assert!(model.apply_to_nvm(&before, ProfileId::Work).is_err());
        }
        assert!(Extensions::decode_nvm(&[]).is_err());
        let mut raw = nvm(0);
        raw[37] = 2;
        assert!(Extensions::decode_nvm(&raw).is_err());
    }

    #[test]
    fn nondefault_inactive_records_and_page_padding_are_preserved() {
        let mut before = nvm(0);
        let mut settings = QmkSettings::default();
        settings.set(4, 321).unwrap();
        before[SETTINGS..SETTINGS + 24].copy_from_slice(&settings.encode());
        let td = slot_offset(true, 6).unwrap();
        before[td..td + 10].copy_from_slice(
            &TapDance {
                outputs: [4, 0, 0, 0],
                term: 123,
            }
            .encode()
            .unwrap(),
        );
        let mut model = Extensions::decode_nvm(&before).unwrap();
        model.key_overrides[6] = KeyOverride {
            trigger: 4,
            replacement: 0x104,
            layers: 63,
            mods: 255,
            negative: 255,
            suppressed: 255,
            options: 0xb9,
        };
        let after = model
            .apply_to_nvm(&before, ProfileId::KeyOverrides)
            .unwrap();
        let at = slot_offset(false, 6).unwrap();
        assert_eq!(&after[..at], &before[..at]);
        assert_eq!(&after[at + 10..], &before[at + 10..]);
        assert_eq!(Extensions::decode_nvm(&after).unwrap(), model);
        let mut model = Extensions::decode_nvm(&before).unwrap();
        model.settings.set(1, 15).unwrap();
        let after = model.apply_to_nvm(&before, ProfileId::Settings).unwrap();
        assert_eq!(
            Extensions::decode_nvm(&after)
                .unwrap()
                .settings
                .get(4)
                .unwrap(),
            321
        );
        assert_eq!(&after[SETTINGS + 24..], &before[SETTINGS + 24..]);
    }
}
