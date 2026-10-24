//! Offline contract for the reviewed wired AK820 alpha.3 firmware.
//!
//! Pinned to `ak820-firmware` alpha.3 (`8ffa3e6`, 2026-10-24),
//! `tools/installer/settings_backup.py`,
//! `qmk-8051-ak820/util/pack_ak820.py`, and the AK820 keyboard sources.

pub const VID: u16 = 0x320F;
pub const PID: u16 = 0x505B;
pub const RAW_HID_USAGE_PAGE: u16 = 0xFF60;
pub const RAW_HID_USAGE: u16 = 0x0061;
pub const REPORT_SIZE: usize = 32;
pub const VIAL_UID: u64 = 0xD315B90190FF7004;
pub const MATRIX_MASK: [u8; 16] = [
    0x3F, 0x2F, 0x3F, 0x1F, 0x1F, 0x3F, 0x1F, 0x1F, 0x1F, 0x3F, 0x3F, 0x3F, 0x07, 0x3F, 0x3C, 0x2E,
];
pub const CODE_SHA256: &str = "01f0f7a979d8cfa8217b3bba333758ae3509cf7f3f8e35fbffb1ed034b808e5d";
pub const IHX_SHA256: &str = "d505d4f420f745dbd5e98a0e3638c8f3ae6d7719c8729dad274595b71b641f18";
const MAGIC: &[u8; 8] = b"AKV1PKG\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub schema: u32,
    pub permanent: bool,
    pub code_sha256: [u8; 32],
    pub initial_nvm_sha256: [u8; 16],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestError {
    Length,
    Magic,
    Version,
    Mode,
    Reserved,
    Schema,
}

/// Parse the 64-byte manifest at ROM address EF80. The trailing NVM digest is
/// retained as provenance only: it describes installation-time NVM and changes
/// after settings edits, so recognition deliberately does not compare it live.
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, ManifestError> {
    if bytes.len() != 64 {
        return Err(ManifestError::Length);
    }
    if &bytes[..8] != MAGIC {
        return Err(ManifestError::Magic);
    }
    if bytes[8] != 1 {
        return Err(ManifestError::Version);
    }
    if bytes[9] != 0 {
        return Err(ManifestError::Mode);
    }
    if bytes[10] != 0 || bytes[11] != 0 {
        return Err(ManifestError::Reserved);
    }
    let schema = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if schema != 0x0008_2002 {
        return Err(ManifestError::Schema);
    }
    Ok(Manifest {
        schema,
        permanent: true,
        code_sha256: bytes[16..48].try_into().unwrap(),
        initial_nvm_sha256: bytes[48..64].try_into().unwrap(),
    })
}

/// Exact image recognition requires independently extracted CODE bytes to
/// match both the manifest and the reviewed alpha.3 image pin.
pub fn recognize_image(manifest: &[u8], computed_code_hash: &[u8; 32]) -> bool {
    parse_manifest(manifest)
        .map(|m| {
            m.permanent
                && m.code_sha256 == *computed_code_hash
                && hex(&m.code_sha256) == CODE_SHA256
        })
        .unwrap_or(false)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Device candidate identification only; these IDs do not establish firmware support.
pub fn is_candidate(vid: u16, pid: u16) -> bool {
    vid == VID && pid == PID
}
pub fn is_raw_hid_usage(page: u16, usage: u16) -> bool {
    page == RAW_HID_USAGE_PAGE && usage == RAW_HID_USAGE
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub layers: u8,
    pub rows: u8,
    pub columns: u8,
    pub physical_keys: u8,
    pub encoders: u8,
    pub macros: u8,
    pub macro_bytes: u16,
    pub combos: u8,
    pub tap_dances: u8,
    pub key_overrides: u8,
    pub alt_repeat: u8,
    pub via_protocol: u8,
    pub vial_protocol: u8,
    pub qmk_settings: bool,
    pub caps_word: bool,
    pub rgb_modes: &'static [u8],
    pub rgb_brightness_max: u8,
    pub rgb_leds: u8,
    pub per_led_persistent: bool,
}
pub const CAPABILITIES: Capabilities = Capabilities {
    layers: 6,
    rows: 16,
    columns: 8,
    physical_keys: 83,
    encoders: 1,
    macros: 16,
    macro_bytes: 8192,
    combos: 8,
    tap_dances: 0,
    key_overrides: 0,
    alt_repeat: 0,
    via_protocol: 9,
    vial_protocol: 6,
    qmk_settings: false,
    caps_word: true,
    rgb_modes: &[0, 1, 2, 6, 13, 14, 31],
    rgb_brightness_max: 160,
    rgb_leds: 82,
    per_led_persistent: false,
};

/// Firmware keycode semantics from the alpha.3 settings preservation allowlist.
/// This recognizes values that can safely round-trip.
/// The historical preservation helper's tap fallback comments are not used
/// here: only encoded keycodes present in the alpha.3 feature allowlist pass.
/// This does not imply every value is appropriate for a new assignment.
pub fn supports_keycode(code: u16) -> bool {
    let basic = |v: u16| v == 0 || v == 1 || (4..=0xC2).contains(&v) || (0xE0..=0xE7).contains(&v);
    if code < 256 {
        return basic(code);
    }
    if matches!(code, 0x7011 | 0x7012 | 0x7013) {
        return true;
    }
    if code < 0x4000 {
        return basic(code & 255);
    }
    if code < 0x5000 {
        return ((code >> 8) & 15) < 6 && basic(code & 255);
    }
    if code < 0x5200 {
        return ((code - 0x5000) >> 5) < 6;
    }
    if matches!(code & 0xFFE0, 0x5200 | 0x5220 | 0x5240 | 0x5260 | 0x52C0) {
        return (code & 31) < 6;
    }
    if (0x52A0..=0x52BF).contains(&code) {
        return true;
    }
    (0x7700..=0x770F).contains(&code)
        || (0x7820..=0x782A).contains(&code)
        || matches!(code, 0x7C1A | 0x7C1B | 0x7C1E)
        || matches!(
            code,
            0x7C00 | 0x7C01 | 0x7C02 | 0x7C03 | 0x7C16 | 0x7C73 | 0x7C77 | 0x7C78 | 0x7C79 | 0x7C7A
        )
}

/// Assignment policy excludes boot/reboot/debug/EEPROM actions pending an
/// explicit safe workflow, while `supports_keycode` preserves them on readback.
pub fn is_assignable_keycode(code: u16) -> bool {
    supports_keycode(code) && !(0x7C00..=0x7C03).contains(&code)
}

/// Check compact-combo semantics shared by the record parser and UI validator.
pub fn valid_combo(keys: [u16; 4], output: u16) -> bool {
    if keys == [0; 4] && output == 0 {
        return true;
    }
    let ordinary = |v: u16| (4..=0xA4).contains(&v);
    let basic_output = |v: u16| ordinary(v) || (0xE0..=0xE7).contains(&v);
    let combo_output = output < 0x2000 && basic_output(output & 0xFF);
    keys[2] == 0
        && keys[3] == 0
        && keys[0] != 0
        && keys[1] != 0
        && keys[0] != keys[1]
        && ordinary(keys[0])
        && ordinary(keys[1])
        && combo_output
}

/// FE0D04 + one index byte + four little-endian inputs + output keycode.
pub fn valid_combo_record(record: &[u8]) -> bool {
    if record.len() != 14 || record[..3] != [0xFE, 0x0D, 0x04] || record[3] >= 8 {
        return false;
    }
    let word = |i: usize| u16::from_le_bytes([record[4 + i * 2], record[5 + i * 2]]);
    valid_combo([word(0), word(1), word(2), word(3)], word(4))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> [u8; 64] {
        let mut b = [0; 64];
        b[..8].copy_from_slice(MAGIC);
        b[8] = 1;
        b[12..16].copy_from_slice(&0x082002u32.to_le_bytes());
        b[16..48].copy_from_slice(&decode(CODE_SHA256));
        b
    }
    fn decode(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks(2)
            .map(|x| u8::from_str_radix(std::str::from_utf8(x).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn image_pin_requires_manifest_and_code() {
        let m = manifest();
        let h: [u8; 32] = decode(CODE_SHA256).try_into().unwrap();
        assert!(recognize_image(&m, &h));
        let mut altered = h;
        altered[0] ^= 1;
        assert!(!recognize_image(&m, &altered));
        let mut fake = m;
        fake[16] ^= 1;
        assert!(!recognize_image(&fake, &h));
    }
    #[test]
    fn manifest_rejects_trial_schema_and_reserved() {
        let mut m = manifest();
        m[9] = 1;
        assert_eq!(parse_manifest(&m), Err(ManifestError::Mode));
        m = manifest();
        m[12] = 1;
        assert_eq!(parse_manifest(&m), Err(ManifestError::Schema));
        m = manifest();
        m[10] = 1;
        assert_eq!(parse_manifest(&m), Err(ManifestError::Reserved));
    }
    #[test]
    fn keycode_edges_and_assignment_policy() {
        for c in [
            0, 1, 4, 0xC2, 0xE7, 0x7011, 0x1C04, 0x4204, 0x50A0, 0x5205, 0x52BF, 0x770F, 0x782A,
            0x7C03, 0x7C1A,
        ] {
            assert!(supports_keycode(c), "{c:04x}");
        }
        for c in [2, 3, 0xC3, 0x4604, 0x5206, 0x7710, 0x782B, 0x7C04] {
            assert!(!supports_keycode(c), "{c:04x}");
        }
        assert!(!is_assignable_keycode(0x7C03));
        assert!(is_assignable_keycode(0x04));
    }
    #[test]
    fn combo_record_bounds_and_output_modifiers() {
        assert!(valid_combo_record(&[
            0xFE, 0x0D, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]));
        assert!(valid_combo_record(&[
            0xFE, 0x0D, 4, 0, 4, 0, 5, 0, 0, 0, 0, 0, 0xE0, 0
        ]));
        assert!(!valid_combo_record(&[
            0xFE, 0x0D, 4, 0, 4, 0, 4, 0, 0, 0, 0, 0, 5, 0
        ]));
        assert!(!valid_combo_record(&[
            0xFE, 0x0D, 4, 8, 4, 0, 5, 0, 0, 0, 0, 0, 5, 0
        ]));
    }
    #[test]
    fn usb_identity_and_rawhid_usage_are_separate() {
        assert!(is_candidate(VID, PID));
        assert!(is_raw_hid_usage(RAW_HID_USAGE_PAGE, RAW_HID_USAGE));
        assert!(!is_candidate(RAW_HID_USAGE_PAGE, RAW_HID_USAGE));
    }
    #[test]
    fn catalog_matches_contract_pins() {
        let c: serde_json::Value =
            serde_json::from_str(include_str!("../assets/ak820/catalog.json")).unwrap();
        let r = &c["devices"][0]["releases"][0];
        assert_eq!(r["code"]["sha256"], CODE_SHA256);
        assert_eq!(r["code"]["start"], 0x2800);
        assert_eq!(r["code"]["end_exclusive"], 0xC700);
        assert_eq!(r["manifest"]["offset"], 0xEF80);
        assert_eq!(c["devices"][0]["usb"]["vid"], VID);
        assert_eq!(c["devices"][0]["usb"]["pid"], PID);
    }
}
