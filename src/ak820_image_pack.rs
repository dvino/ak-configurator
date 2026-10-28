// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline image model adapted from dvino/ak820 at
//! bef20f89d3da01d770767919bc7c8fc3c3a1a808, util/pack_ak820.py
//! (firmware repository GPL-3.0-or-later), plus ak820_tool.py and the
//! tools/installer image policy. Preserves source constants and instruction bytes.
//!
//! Packing provides no transport, file IO, ownership assertion or installation
//! authorization. The native installer separately checks local recovery ownership,
//! release admission, a fresh source capture and the complete transaction.
//! A pinned OEM digest identifies a revision, never its physical owner. Public
//! beta2 compressed defaults follow the pinned release packer contract.
use crate::ak820_settings_backup::{sha256, BackupResult};
use crate::ak820_settings_model::validate_nvm;
use crate::app::ak820_beta1_profiles::ProfileId;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(crate) const SOURCE_REVISION: &str = "bef20f89d3da01d770767919bc7c8fc3c3a1a808";
pub(crate) const APP_START: usize = 0x2800;
pub(crate) const APP_END: usize = 0xefc0;
const CODE_START: usize = 0x3027;
const CODE_END: usize = 0xc700;
const LOW_START: usize = 0x28a0;
const LOW_END: usize = 0x3000;
const NVM_END: usize = 0xef00;
const MANIFEST: usize = 0xef80;
const PROTECTED_SHA: &str = "9c4d481c14fe26b58f0f3eee81e84f45f118e06d755fd051da8d1cea5d78eb33";
const FACTORY_SHA: &str = "431db5e715957be1b158e122a2ee95034dcb248c091b3e423e50c6a139983595";
const PATCHED_SHA: &str = "92ddc3c33a23a216e360ee137c08309336c8a95250bd5523f44cfbafab9d8a72";
type Code = BTreeMap<usize, u8>;
pub(crate) type Symbols = BTreeMap<String, usize>;
/// SDCC CODE symbols from the exact, independently pinned release map.
pub(crate) fn read_symbols(text: &str) -> BackupResult<Symbols> {
    let mut symbols = Symbols::new();
    for line in text.lines().filter(|line| line.starts_with("C:")) {
        let mut words = line[2..].split_whitespace();
        let (Some(address), Some(name)) = (words.next(), words.next()) else {
            continue;
        };
        if address.len() != 8 || !address.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let value = usize::from_str_radix(address, 16).map_err(|_| "Invalid map address")?;
        // SDCC's printed map truncates long function names; two unrelated
        // unused functions can therefore share the displayed name. Only the
        // exact symbols consumed by this packer must be unique.
        let required = matches!(
            name,
            "_keymaps"
                | "_encoder_map"
                | "_ak820_default_classes"
                | "_ak820_default_offsets"
                | "_ak820_default_payload"
                | "__sdcc_gsinit_startup"
                | "_USB_DeviceInterrupt"
                | "_ak820_timer_irq"
                | "_ak820_led_irq"
                | "__mcs51_genXINIT"
                | "s_XINIT"
                | "s_XISEG"
                | "l_XINIT"
                | "s_XSEG"
                | "l_XSEG"
                | "l_PSEG"
                | "l_XISEG"
                | "s_GSINIT3"
                | "l_GSINIT3"
        );
        if symbols
            .insert(name.to_owned(), value)
            .is_some_and(|old| required && old != value)
        {
            return Err("Conflicting linked symbols".into());
        }
    }
    need(!symbols.is_empty(), "Missing linked symbols")?;
    Ok(symbols)
}

/// The two compiled default layers, in SDCC little-endian uint16_t order.
/// Mirrors pack_ak820.factory_keymaps, including canonical compressed records.
pub(crate) fn factory_keymaps(data: &Code, symbols: &Symbols) -> BackupResult<Vec<u8>> {
    let names = [
        "_ak820_default_classes",
        "_ak820_default_offsets",
        "_ak820_default_payload",
    ];
    let present = names.map(|name| symbols.contains_key(name));
    if let Some(&address) = symbols.get("_keymaps") {
        need(
            !present.iter().any(|v| *v),
            "Ambiguous compiled default keymaps",
        )?;
        return bytes(data, address, 512);
    }
    need(
        present.iter().all(|v| *v),
        "Missing compressed default keymap tables",
    )?;
    let start = names.map(|name| symbols[name]);
    let classes = bytes(data, start[0], 64)?;
    let offsets = bytes(data, start[1], 32)?;
    let mut cursor = 0usize;
    let mut output = Vec::with_capacity(512);
    for row in 0..32 {
        need(
            offsets[row] as usize == cursor,
            "Non-contiguous default keymap payload",
        )?;
        for col in 0..8 {
            let kind = (classes[row * 2 + col / 4] >> ((col % 4) * 2)) & 3;
            let value = if kind < 2 {
                kind as u16
            } else {
                let raw = bytes(
                    data,
                    start[2]
                        .checked_add(cursor)
                        .ok_or("Default table overflow")?,
                    kind as usize - 1,
                )?;
                let value = if kind == 2 {
                    raw[0] as u16
                } else {
                    u16::from_le_bytes([raw[0], raw[1]])
                };
                need(
                    value >= if kind == 2 { 2 } else { 256 },
                    "Non-canonical default keymap encoding",
                )?;
                cursor += kind as usize - 1;
                value
            };
            output.extend_from_slice(&value.to_le_bytes());
        }
    }
    need(cursor <= 255, "Default keymap payload exceeds byte offsets")?;
    let ranges = [(start[0], 64), (start[1], 32), (start[2], cursor)];
    for (index, &(a, length)) in ranges.iter().enumerate() {
        let end = a.checked_add(length).ok_or("Default table overflow")?;
        for &(b, count) in &ranges[index + 1..] {
            let other_end = b.checked_add(count).ok_or("Default table overflow")?;
            need(
                length == 0 || count == 0 || a.max(b) >= end.min(other_end),
                "Overlapping default keymap tables",
            )?;
        }
    }
    Ok(output)
}

/// Explicit factory settings for the selected compiled image. These are public
/// defaults, never a migration of the user's current settings or OEM macros.
pub(crate) fn default_nvm(ihx: &str, symbols: &Symbols) -> BackupResult<Vec<u8>> {
    let data = read_ihex(ihx)?;
    let keys = factory_keymaps(&data, symbols)?;
    let encoders = bytes(&data, symbol(symbols, "_encoder_map")?, 8)?;
    let mut nvm = vec![0; 0x2800];
    nvm[..2].copy_from_slice(&[0xe6, 0xfe]);
    nvm[3] = 1;
    nvm[24..32].copy_from_slice(&[5, 0, 255, 80, 96, 255, 0, 0]);
    nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
    for key in nvm[0x40..0x658].chunks_exact_mut(2) {
        key.copy_from_slice(&[0, 1]);
    }
    for (target, value) in nvm[0x40..0x240]
        .chunks_exact_mut(2)
        .zip(keys.chunks_exact(2))
    {
        target.copy_from_slice(&[value[1], value[0]]);
    }
    for (index, source) in [1usize, 0, 3, 2].into_iter().enumerate() {
        let value = &encoders[source * 2..source * 2 + 2];
        nvm[0x640 + index * 2..0x642 + index * 2].copy_from_slice(&[value[1], value[0]]);
    }
    nvm[0x7f1..0x7f3].copy_from_slice(&16u16.to_le_bytes());
    validate_nvm(&nvm, 0x082003)?;
    Ok(nvm)
}
fn need(ok: bool, message: &str) -> BackupResult<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn unhex(text: &str) -> BackupResult<Vec<u8>> {
    need(
        text.is_ascii() && text.len() % 2 == 0,
        "Malformed hexadecimal data",
    )?;
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let digit = |v: u8| {
                (v as char)
                    .to_digit(16)
                    .ok_or_else(|| "Malformed hexadecimal digit".to_string())
            };
            Ok((digit(p[0])? * 16 + digit(p[1])?) as u8)
        })
        .collect()
}
/// Exact duplicate bytes are allowed; conflicting overlaps fail. Only data,
/// EOF, segment-address and linear-address records are supported by the source.
/// Python bytes.fromhex ASCII whitespace is accepted between complete bytes.
pub(crate) fn read_ihex(text: &str) -> BackupResult<Code> {
    let (mut data, mut base, mut eof) = (Code::new(), 0usize, false);
    // Path.read_text performs universal newline decoding, then Python's
    // splitlines also recognizes these Unicode/ASCII record separators.
    let normalized = text.replace("\r\n", "\n");
    for line in normalized.split_terminator([
        '\n', '\r', '\u{000b}', '\u{000c}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{0085}',
        '\u{2028}', '\u{2029}',
    ]) {
        need(
            !eof && line.starts_with(':'),
            "Malformed HEX or record after EOF",
        )?;
        let mut r = Vec::new();
        // bytes.fromhex permits ASCII whitespace between complete byte pairs,
        // never between the two nibbles. Non-ASCII whitespace remains invalid.
        for word in line[1..].split(|c: char| c.is_ascii_whitespace()) {
            r.extend(unhex(word)?);
        }
        need(
            r.len() >= 5
                && r.len() == r[0] as usize + 5
                && r.iter().fold(0u8, |s, v| s.wrapping_add(*v)) == 0,
            "HEX checksum/length mismatch",
        )?;
        let n = r[0] as usize;
        let addr = u16::from_be_bytes([r[1], r[2]]) as usize;
        match r[3] {
            0 => {
                for (i, &value) in r[4..4 + n].iter().enumerate() {
                    let a = base + addr + i;
                    need(
                        (CODE_START..CODE_END).contains(&a) || (LOW_START..LOW_END).contains(&a),
                        "Code outside application window",
                    )?;
                    need(
                        data.get(&a).is_none_or(|&v| v == value),
                        "Conflicting HEX records",
                    )?;
                    data.insert(a, value);
                }
            }
            1 if n == 0 && addr == 0 => eof = true,
            kind @ (2 | 4) if n == 2 && addr == 0 => {
                base =
                    (u16::from_be_bytes([r[4], r[5]]) as usize) << if kind == 2 { 4 } else { 16 };
            }
            _ => return Err("Unexpected HEX record".into()),
        }
    }
    need(
        eof && data.contains_key(&CODE_START),
        "Incomplete linked application",
    )?;
    Ok(data)
}
fn bytes(data: &Code, start: usize, len: usize) -> BackupResult<Vec<u8>> {
    let end = start.checked_add(len).ok_or("Linked data range overflow")?;
    (start..end)
        .map(|a| {
            data.get(&a)
                .copied()
                .ok_or_else(|| "Missing compiled constant data".into())
        })
        .collect()
}
fn symbol(symbols: &Symbols, name: &str) -> BackupResult<usize> {
    symbols
        .get(name)
        .copied()
        .ok_or_else(|| format!("Missing linked symbol {name}"))
}
fn ljmp(address: usize) -> BackupResult<Vec<u8>> {
    let address = u16::try_from(address).map_err(|_| "Jump target outside CODE address space")?;
    Ok(vec![2, (address >> 8) as u8, address as u8])
}
fn validate_startup(data: &Code, s: &Symbols) -> BackupResult<()> {
    let source = symbol(s, "s_XINIT")?;
    let destination = symbol(s, "s_XISEG")?;
    let length = symbol(s, "l_XINIT")?;
    let xseg = symbol(s, "s_XSEG")?;
    let xlen = symbol(s, "l_XSEG")?;
    need(
        symbol(s, "l_PSEG")? == 0
            && length == symbol(s, "l_XISEG")?
            && matches!(xseg, 0x400 | 0x440)
            && xseg.checked_add(xlen) == Some(destination)
            && destination
                .checked_add(length)
                .is_some_and(|end| end <= 0x1000)
            && source <= 0xffff,
        "Unsupported startup XRAM/PSEG layout",
    )?;
    let mut expected = vec![
        0x79,
        length as u8,
        0xe9,
        0x44,
        (length >> 8) as u8,
        0x60,
        0x25,
        0x7a,
        ((length + 255) >> 8) as u8,
        0x7b,
        source as u8,
        0x7c,
        (source >> 8) as u8,
        0x7d,
        destination as u8,
        0x7e,
        (destination >> 8) as u8,
    ];
    expected.extend(unhex(
        "8b828c83e493ffa3ab82ac838d828e83eff0a3ad82ae83d9e7dae5",
    )?);
    let entry = symbol(s, "__mcs51_genXINIT")?;
    need(
        entry == symbol(s, "s_GSINIT3")?
            && symbol(s, "l_GSINIT3")? == expected.len()
            && bytes(data, entry, expected.len())? == expected,
        "Unsupported XINIT: expected linked AK820 single-DPTR CRT",
    )
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NativeVariant {
    Factory,
    Patched,
}
/// Immutable revision checks from validate_native; not ownership proof.
pub(crate) fn validate_native_revision(rom: &[u8]) -> BackupResult<NativeVariant> {
    need(rom.len() == 65536, "Incorrect ROM image size")?;
    let mut protected = rom[..0x27c0].to_vec();
    protected.extend_from_slice(&rom[APP_END..]);
    need(
        sha256(&protected) == PROTECTED_SHA,
        "OEM protected loader/tail differs",
    )?;
    let mut factory = rom[APP_START..0xa000].to_vec();
    factory.extend_from_slice(&rom[0xd200..APP_END]);
    let mut patched = rom[APP_START..0xa000].to_vec();
    patched.extend_from_slice(&rom[0xd380..APP_END]);
    let variant = if sha256(&factory) == FACTORY_SHA {
        NativeVariant::Factory
    } else if sha256(&patched) == PATCHED_SHA {
        NativeVariant::Patched
    } else {
        return Err("Unsupported complete OEM application variant".into());
    };
    need(
        rom[0x27c0..0x27c4] == [0xaa, 0x55, 0x33, 0xcc],
        "OEM boot marker missing",
    )?;
    need(rom[0xa040] == 0, "Select native profile 0 before migration")?;
    Ok(variant)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryMode {
    Permanent,
    Trial,
}
/// An offline candidate has no ownership or hardware qualification capability.
#[derive(Debug)]
pub(crate) struct OfflineCandidate {
    payload: Vec<u8>,
    pub(crate) mode: EntryMode,
}
impl OfflineCandidate {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.payload
    }
}
/// Explicit NVM input is required; no implicit OEM settings migration/defaulting.
/// Unknown catalog identities and unqualified profiles fail closed. Trial CODE
/// differs from permanent CODE, so a permanent-only catalog rejects trial output.
pub(crate) fn pack_candidate(
    ihx: &str,
    symbols: &Symbols,
    stock: &[u8],
    nvm: &[u8],
    mode: EntryMode,
    profile: ProfileId,
) -> BackupResult<OfflineCandidate> {
    pack_candidate_for_image(
        ihx,
        symbols,
        stock,
        nvm,
        mode,
        crate::ak820::ImageContract::Beta3(profile),
    )
}
pub(crate) fn pack_candidate_for_image(
    ihx: &str,
    symbols: &Symbols,
    stock: &[u8],
    nvm: &[u8],
    mode: EntryMode,
    image: crate::ak820::ImageContract,
) -> BackupResult<OfflineCandidate> {
    image.profile().ok_or("Image has no profile")?;
    let exact = image.matrix_profile().ok_or("Image lacks a pinned profile matrix")?
        .settings_identity
        .as_ref()
        .ok_or("Profile lacks qualified packed identity")?;
    need(
        sha256(ihx.as_bytes()) == exact.ihx_sha256,
        "Linked input differs from catalog",
    )?;
    validate_native_revision(stock)?;
    validate_nvm(nvm, exact.nvm_schema)?;
    let data = read_ihex(ihx)?;
    let payload = pack_core(&data, symbols, stock, nvm, mode, exact.nvm_schema)?;
    validate_catalog_payload_for_image(&payload, image)?;
    Ok(OfflineCandidate { payload, mode })
}
/// Structural verification is separate from catalog approval. A self-consistent
/// manifest and checksum never suffice to qualify a custom image.
pub(crate) fn validate_catalog_payload(
    payload: &[u8],
    profile: ProfileId,
) -> BackupResult<EntryMode> {
    validate_catalog_payload_for_image(payload, crate::ak820::ImageContract::Beta3(profile))
}
pub(crate) fn validate_catalog_payload_for_image(
    payload: &[u8],
    image: crate::ak820::ImageContract,
) -> BackupResult<EntryMode> {
    let exact = image.matrix_profile().ok_or("Image lacks a pinned profile matrix")?
        .settings_identity
        .as_ref()
        .ok_or("Profile lacks qualified packed identity")?;
    let mode = verify_payload(payload, exact.nvm_schema)?;
    need(
        payload[MANIFEST - APP_START + 10..MANIFEST - APP_START + 12] == [0, 0],
        "Catalog manifest reserved bytes differ",
    )?;
    need(
        mode == EntryMode::Permanent
            && sha256(&payload[..CODE_END - APP_START]) == exact.code_sha256,
        "Payload differs from catalog permanent packed CODE policy",
    )?;
    Ok(mode)
}
fn verify_payload(payload: &[u8], schema: u32) -> BackupResult<EntryMode> {
    need(
        payload.len() == APP_END - APP_START,
        "Wrong update image length; full ROM forbidden",
    )?;
    let m = &payload[MANIFEST - APP_START..];
    need(
        &m[..8] == b"AKV1PKG\0"
            && m[8] == 1
            && m[9] <= 1
            && u32::from_le_bytes(m[12..16].try_into().unwrap()) == schema,
        "Unknown application manifest",
    )?;
    need(
        Sha256::digest(&payload[..CODE_END - APP_START])[..] == m[16..48],
        "Application checksum mismatch",
    )?;
    let nvm = &payload[CODE_END - APP_START..NVM_END - APP_START];
    need(
        Sha256::digest(nvm)[..16] == m[48..64],
        "Initial configuration checksum mismatch",
    )?;
    need(
        payload[NVM_END - APP_START..MANIFEST - APP_START]
            .iter()
            .all(|&v| v == 0),
        "Journal must be empty",
    )?;
    validate_nvm(nvm, schema)?;
    Ok(if m[9] == 1 {
        EntryMode::Trial
    } else {
        EntryMode::Permanent
    })
}
/// Private deterministic core. Production callers must check OEM revision and
/// catalog before returning bytes. Synthetic tests alone may enter this core.
fn pack_core(
    data: &Code,
    s: &Symbols,
    stock: &[u8],
    nvm: &[u8],
    mode: EntryMode,
    schema: u32,
) -> BackupResult<Vec<u8>> {
    need(
        stock.len() == 65536 && nvm.len() == NVM_END - CODE_END,
        "Incorrect settings/reference size",
    )?;
    for name in [
        "__sdcc_gsinit_startup",
        "_USB_DeviceInterrupt",
        "_ak820_timer_irq",
        "_ak820_led_irq",
        "_encoder_map",
    ] {
        need(
            data.contains_key(&symbol(s, name)?),
            "Missing reachable linked symbol",
        )?;
    }
    if let Some(&address) = s.get("_keymaps") {
        need(
            data.contains_key(&address),
            "Missing reachable linked keymaps",
        )?;
        need(
            ![
                "_ak820_default_classes",
                "_ak820_default_offsets",
                "_ak820_default_payload",
            ]
            .iter()
            .any(|name| s.contains_key(*name)),
            "Ambiguous compiled default keymaps",
        )?;
    } else {
        factory_keymaps(data, s)?;
    }
    need(
        bytes(data, CODE_START, 3)? == ljmp(symbol(s, "__sdcc_gsinit_startup")?)?,
        "Unexpected SDCC CRT entry",
    )?;
    validate_startup(data, s)?;
    let mut full = stock.to_vec();
    full[APP_START..APP_END].fill(0);
    full[0x2830..0x2887].copy_from_slice(&stock[0xe650..0xe6a7]);
    let mut entry = unhex("75a80075e80075d00075812053a2fa75a15575a1aa53b1fde4f5a1")?;
    match mode {
        EntryMode::Trial => {
            entry.extend(unhex("7fc07e27122830")?);
            entry.extend(ljmp(0x2888)?);
        }
        EntryMode::Permanent => entry.extend(ljmp(CODE_START)?),
    }
    let mut check = unhex("9027c07f40e4937006a3dff9")?;
    check.extend(ljmp(CODE_START)?);
    check.extend([0x80, 0xfe]);
    need(
        APP_START + entry.len() <= 0x2830 && 0x2888 + check.len() <= LOW_START,
        "Startup helpers overlap",
    )?;
    full[APP_START..APP_START + entry.len()].copy_from_slice(&entry);
    full[0x2888..0x2888 + check.len()].copy_from_slice(&check);
    for ordinal in 0..13 {
        full[0x3000 + ordinal * 3..0x3003 + ordinal * 3].copy_from_slice(&[0x22, 0, 0]);
    }
    for (ordinal, name) in [
        (1, "_ak820_timer_irq"),
        (7, "_USB_DeviceInterrupt"),
        (10, "_ak820_led_irq"),
    ] {
        full[0x3000 + ordinal * 3..0x3003 + ordinal * 3].copy_from_slice(&ljmp(symbol(s, name)?)?);
    }
    for (&address, &value) in data {
        need(
            (CODE_START..CODE_END).contains(&address) || (LOW_START..LOW_END).contains(&address),
            "Code outside application window",
        )?;
        full[address] = value;
    }
    full[CODE_END..NVM_END].copy_from_slice(nvm);
    let code_hash = Sha256::digest(&full[APP_START..CODE_END]);
    let config_hash = Sha256::digest(nvm);
    let mut manifest = b"AKV1PKG\0".to_vec();
    manifest.extend([1, u8::from(mode == EntryMode::Trial), 0, 0]);
    manifest.extend(schema.to_le_bytes());
    manifest.extend_from_slice(&code_hash);
    manifest.extend_from_slice(&config_hash[..16]);
    full[MANIFEST..APP_END].copy_from_slice(&manifest);
    need(
        full[..APP_START] == stock[..APP_START] && full[APP_END..] == stock[APP_END..],
        "Protected ROM changed",
    )?;
    Ok(full[APP_START..APP_END].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_map_allows_unused_truncated_names_but_rejects_ambiguous_inputs() {
        let text = "C: 00003027 _encoder_map\nC: 00004000 _truncated_unused_function_name\nC: 00004004 _truncated_unused_function_name\n";
        assert_eq!(read_symbols(text).unwrap()["_encoder_map"], 0x3027);
        assert!(read_symbols(&format!("{text}C: 00005000 _encoder_map\n")).is_err());
        assert!(read_symbols(
            "C: 00003027 _ak820_default_classes\nC: 00004000 _ak820_default_classes\n"
        )
        .is_err());
    }
    fn record(address: u16, kind: u8, raw: &[u8]) -> String {
        let mut r = vec![raw.len() as u8, (address >> 8) as u8, address as u8, kind];
        r.extend(raw);
        r.push(0u8.wrapping_sub(r.iter().fold(0u8, |a, v| a.wrapping_add(*v))));
        format!(
            ":{}",
            r.iter().map(|v| format!("{v:02X}")).collect::<String>()
        )
    }
    fn hex(records: &[String]) -> String {
        records.join("\n")
    }
    fn fixture() -> (Code, Symbols, Vec<u8>, Vec<u8>) {
        let mut s: Symbols = [
            ("__sdcc_gsinit_startup", 0x3100),
            ("_USB_DeviceInterrupt", 0x3200),
            ("_ak820_timer_irq", 0x3210),
            ("_ak820_led_irq", 0x3220),
            ("_keymaps", 0x3300),
            ("_encoder_map", 0x3500),
            ("__mcs51_genXINIT", 0x3100),
            ("s_GSINIT3", 0x3100),
            ("s_XINIT", 0x3600),
            ("l_XINIT", 3),
            ("s_XISEG", 0x410),
            ("l_XISEG", 3),
            ("s_XSEG", 0x400),
            ("l_XSEG", 0x10),
            ("l_PSEG", 0),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        let startup = unhex("7903e9440060257a017b007c367d107e048b828c83e493ffa3ab82ac838d828e83eff0a3ad82ae83d9e7dae5").unwrap();
        s.insert("l_GSINIT3".into(), startup.len());
        let mut data: Code = [(0x3027, 2), (0x3028, 0x31), (0x3029, 0)]
            .into_iter()
            .collect();
        for (i, v) in startup.into_iter().enumerate() {
            data.insert(0x3100 + i, v);
        }
        for a in [0x3200, 0x3210, 0x3220, 0x3300, 0x3500] {
            data.insert(a, 0x22);
        }
        let stock = (0..65536).map(|i| ((i * 17 + 3) % 256) as u8).collect();
        let mut nvm = vec![0; 0x2800];
        nvm[..2].copy_from_slice(&[0xe6, 0xfe]);
        nvm[37..40].copy_from_slice(&[2, 0x20, 8]);
        for p in nvm[0x40..0x658].chunks_exact_mut(2) {
            p.copy_from_slice(&[0, 1]);
        }
        nvm[0x7f1..0x7f3].copy_from_slice(&16u16.to_le_bytes());
        (data, s, stock, nvm)
    }
    #[test]
    fn synthetic_python_core_parity() {
        // Oracle: pinned pack_ak820.pack with this public synthetic fixture;
        // validate_native replaced ONLY in reference test process because this
        // fixture is intentionally not OEM. Startup/NVM checks remain enabled.
        // Hash equality covers every output byte, including zero gaps/manifest.
        let (data, s, stock, nvm) = fixture();
        assert!(validate_native_revision(&stock).is_err());
        for (mode, digest) in [
            (
                EntryMode::Permanent,
                "8db1589e4c8311087ff2ceea2e0e39600d47ed420ba7b066174211c1a918508a",
            ),
            (
                EntryMode::Trial,
                "c2a3cebf0006ca8bb6ff81e8a0eb0889ca9e010ba9b2493ce87a94a625b8a088",
            ),
        ] {
            let payload = pack_core(&data, &s, &stock, &nvm, mode, 0x082002).unwrap();
            assert_eq!(sha256(&payload), digest);
            assert_eq!(verify_payload(&payload, 0x082002).unwrap(), mode);
            assert!(validate_catalog_payload(&payload, ProfileId::Main).is_err());
            assert_eq!(
                &payload[0x2830 - APP_START..0x2887 - APP_START],
                &stock[0xe650..0xe6a7]
            );
        }
    }
    #[test]
    fn startup_accepts_only_beta2_xram_starts() {
        // Synthetic startup only; no OEM bytes or packed release proof involved.
        for start in [0x400, 0x440] {
            let (mut data, mut symbols, _, _) = fixture();
            symbols.insert("s_XSEG".into(), start);
            symbols.insert("l_XSEG".into(), 0xa00 - start);
            symbols.insert("s_XISEG".into(), 0xa00);
            // Relocated startup operands: destination low/high bytes.
            data.insert(0x3100 + 14, 0x00);
            data.insert(0x3100 + 16, 0x0a);
            assert!(validate_startup(&data, &symbols).is_ok());
        }
        for start in [0x3c0, 0x420, 0x1000] {
            let (data, mut symbols, _, _) = fixture();
            symbols.insert("s_XSEG".into(), start);
            symbols.insert("l_XSEG".into(), 0xa00usize.saturating_sub(start));
            symbols.insert("s_XISEG".into(), 0xa00);
            assert!(validate_startup(&data, &symbols).is_err());
        }
    }
    #[test]
    fn hex_record_separators_match_python_splitlines() {
        let data = record(CODE_START as u16, 0, &[2, 0x31, 0]);
        let eof = record(0, 1, &[]);
        let expected = read_ihex(&format!("{data}\n{eof}\n")).unwrap();
        for separator in [
            "\r", "\r\n", "\u{000b}", "\u{000c}", "\u{001c}", "\u{001d}", "\u{001e}", "\u{0085}",
            "\u{2028}", "\u{2029}",
        ] {
            assert_eq!(
                read_ihex(&format!("{data}{separator}{eof}{separator}")).unwrap(),
                expected
            );
        }
        // A separator inside a record is a new malformed record, not byte whitespace.
        for separator in ["\u{000b}", "\u{000c}", "\r"] {
            assert!(
                read_ihex(&format!("{}{separator}{}\n{eof}\n", &data[..3], &data[3..])).is_err()
            );
        }
    }
    #[test]
    fn hex_negative_cases_and_identical_overlap() {
        let valid = record(0x3027, 0, &[2, 0x31, 0]);
        let eof = record(0, 1, &[]);
        assert!(read_ihex(&hex(&[valid.clone(), valid.clone(), eof.clone()])).is_ok());
        for records in [
            vec![valid.clone()],
            vec![eof.clone()],
            vec![valid.clone(), eof.clone(), valid.clone()],
            vec![valid.clone(), record(0x3028, 0, &[9]), eof.clone()],
            vec![record(0x3026, 0, &[0]), valid.clone(), eof.clone()],
            vec![record(0xc6ff, 0, &[0, 0]), valid.clone(), eof.clone()],
            vec![record(0x2fff, 0, &[0, 0]), valid.clone(), eof.clone()],
            vec![record(0, 4, &[0, 1]), valid.clone(), eof.clone()],
            vec![valid.clone(), record(1, 1, &[])],
            vec![valid.clone(), record(0, 3, &[0, 0, 0, 0]), eof.clone()],
            vec![valid.clone(), String::new(), eof.clone()],
            vec![":0330270002310060".into(), eof.clone()],
            vec![":ZZ".into(), eof.clone()],
        ] {
            assert!(read_ihex(&hex(&records)).is_err(), "{records:?}");
        }
        for kind in [2, 4] {
            assert!(read_ihex(&hex(&[
                record(0, kind, &[0, 0]),
                valid.clone(),
                eof.clone()
            ]))
            .is_ok());
        }
    }
    #[test]
    fn faults_stop_before_output() {
        let (mut d, mut s, stock, nvm) = fixture();
        s.insert("l_PSEG".into(), 1);
        assert!(pack_core(&d, &s, &stock, &nvm, EntryMode::Permanent, 0x082002).is_err());
        s.insert("l_PSEG".into(), 0);
        d.insert(0x3100, 0);
        assert!(pack_core(&d, &s, &stock, &nvm, EntryMode::Permanent, 0x082002).is_err());
        let (d, s, stock, nvm) = fixture();
        let good = pack_core(&d, &s, &stock, &nvm, EntryMode::Permanent, 0x082002).unwrap();
        for offset in [
            0,
            0x9f00,
            0xc700 - APP_START,
            0xef00 - APP_START,
            0xef80 - APP_START,
            0xefb0 - APP_START,
        ] {
            let mut bad = good.clone();
            bad[offset] ^= 1;
            assert!(verify_payload(&bad, 0x082002).is_err());
        }
        assert!(verify_payload(&stock, 0x082002).is_err());
        assert!(
            pack_candidate("", &s, &stock, &nvm, EntryMode::Permanent, ProfileId::Game).is_err()
        );
    }
}
