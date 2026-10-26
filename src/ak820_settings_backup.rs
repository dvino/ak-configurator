//! Firmware backup envelope contracts. Pure local bytes only; no HID or filesystem writes.
//!
//! This module does not grant restore permission. `validate_archive` requires a
//! trusted ROM validator/settings decoder; envelope hashes alone prove no device identity.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

pub const FORMAT: &str = "ak820-settings-backup-1";
pub const FORMAT_V2: &str = "ak820-settings-backup-2";
pub const FILES: [(&str, usize); 5] = [
    ("settings.json", 512 * 1024),
    ("before-full-rom.bin", 65536),
    ("saved-nvm.bin", 0x2800),
    ("read-log.jsonl", 8 * 1024 * 1024),
    (".gitignore", 2),
];
pub type BackupResult<T> = Result<T, String>;

// Value's ordinary deserializer silently accepts repeated object fields. Use
// this recursive visitor for every object, including macro command objects.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("strict JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Strict(Value::Number(n)))
                    .ok_or_else(|| E::custom("Invalid JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Strict, E> {
                self.visit_unit()
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut out = Vec::new();
                while let Some(Strict(v)) = a.next_element()? {
                    out.push(v)
                }
                Ok(Strict(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut out = Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if out.contains_key(&k) {
                        return Err(de::Error::custom(format!("Duplicate JSON field: {k}")));
                    }
                    let Strict(v) = a.next_value()?;
                    out.insert(k, v);
                }
                Ok(Strict(Value::Object(out)))
            }
        }
        d.deserialize_any(V)
    }
}
pub fn parse_json(raw: &[u8]) -> BackupResult<Value> {
    let mut d = serde_json::Deserializer::from_slice(raw);
    let Strict(value) = Strict::deserialize(&mut d).map_err(|e| e.to_string())?;
    d.end().map_err(|e| e.to_string())?;
    Ok(value)
}
/// Python `canonical`: sorted fields, two-space indentation, UTF-8, final LF.
/// Firmware contracts contain integer numbers; floating point serialization is
/// deliberately rejected to avoid language-specific canonical number spellings.
pub fn canonical(value: &Value) -> BackupResult<Vec<u8>> {
    fn integer_json(v: &Value) -> bool {
        match v {
            Value::Number(n) => n.is_i64() || n.is_u64(),
            Value::Array(a) => a.iter().all(integer_json),
            Value::Object(o) => o.values().all(integer_json),
            _ => true,
        }
    }
    if !integer_json(value) {
        return Err("Non-integer contract number".into());
    }
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn ensure(ok: bool, msg: &str) -> BackupResult<()> {
    if ok {
        Ok(())
    } else {
        Err(msg.into())
    }
}
fn record(bytes: &[u8]) -> Value {
    json!({"size":bytes.len(),"sha256":sha256(bytes)})
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageIdentity {
    pub ihx_sha256: String,
    pub code_sha256: String,
    pub definition_sha256: String,
    pub schema: u32,
}
#[derive(Debug)]
pub struct ValidatedArchive {
    pub manifest_sha256: String,
    pub manifest: Value,
    pub settings: Value,
    pub source: ImageIdentity,
}
/// Validate immutable file bytes. The callback MUST verify exact packed CODE,
/// manifest, protected OEM regions, journal and schema, then derive complete
/// settings (including inactive typed storage) from NVM. It is invoked before
/// settings JSON is accepted. No writable addresses are read from JSON.
pub fn validate_archive<F>(
    manifest_raw: &[u8],
    files: &BTreeMap<String, Vec<u8>>,
    images: &[ImageIdentity],
    expected_manifest: Option<&str>,
    decode_trusted_rom: F,
) -> BackupResult<ValidatedArchive>
where
    F: FnOnce(&[u8], &[u8], &ImageIdentity) -> BackupResult<Value>,
{
    ensure(manifest_raw.len() <= 65536, "Oversized manifest")?;
    let identity = sha256(manifest_raw);
    ensure(
        expected_manifest.is_none_or(|v| v == identity),
        "Backup manifest changed",
    )?;
    let manifest = parse_json(manifest_raw)?;
    let object = manifest.as_object().ok_or("Invalid backup manifest")?;
    let allowed = [
        "format",
        "created_utc",
        "device",
        "definition_sha256",
        "nvm_schema",
        "source_build_id",
        "source_ihx_sha256",
        "code_sha256",
        "counts",
        "scope",
        "files",
    ];
    ensure(
        object.len() == allowed.len() && object.keys().all(|k| allowed.contains(&k.as_str())),
        "Missing or unknown backup manifest field",
    )?;
    ensure(
        manifest["created_utc"]
            .as_str()
            .is_some_and(|v| !v.is_empty() && v.len() <= 64)
            && manifest["scope"]
                == "saved supported settings and exposed RGB values; not all current RAM",
        "Invalid backup timestamp or scope",
    )?;
    ensure(
        manifest["device"] == json!({"vid":0x320f,"pid":0x505b,"vial_uid":"0470ff9001b915d3"}),
        "Backup device differs",
    )?;
    let matches: Vec<_> = images
        .iter()
        .filter(|i| {
            manifest["source_ihx_sha256"] == i.ihx_sha256
                && manifest["code_sha256"] == i.code_sha256
                && manifest["definition_sha256"] == i.definition_sha256
        })
        .collect();
    ensure(matches.len() == 1, "Unsupported or ambiguous backup image")?;
    let source = matches[0].clone();
    ensure(
        matches!(source.schema, 0x082002 | 0x082003),
        "Unsupported NVM schema; schema01 migration is not implemented",
    )?;
    ensure(
        manifest["nvm_schema"].as_u64() == Some(source.schema as u64)
            && manifest["source_build_id"].as_u64() == Some(source.schema as u64),
        "Backup schema differs",
    )?;
    ensure(
        manifest["format"]
            == if source.schema == 0x082003 {
                FORMAT_V2
            } else {
                FORMAT
            },
        "Backup format/schema mismatch",
    )?;
    ensure(
        manifest["counts"]
            == json!({"layers":6,"physical_keys":83,"assignments":498,"encoder_assignments":12,"macros":16,"macro_capacity":8192}),
        "Backup dimensions differ",
    )?;
    let records = manifest["files"]
        .as_object()
        .ok_or("Missing backup file records")?;
    ensure(
        records.len() == FILES.len() && files.len() == FILES.len(),
        "Unexpected backup files",
    )?;
    for (name, limit) in FILES {
        let data = files
            .get(name)
            .ok_or_else(|| format!("Missing backup file: {name}"))?;
        ensure(data.len() <= limit, "Oversized backup file")?;
        ensure(
            records.get(name) == Some(&record(data)),
            "Backup hash/size mismatch",
        )?;
    }
    ensure(
        files[".gitignore"] == b"*\n",
        "Backup Git exclusion missing",
    )?;
    let rom = &files["before-full-rom.bin"];
    let nvm = &files["saved-nvm.bin"];
    ensure(
        rom.len() == 65536 && nvm.len() == 0x2800,
        "Incomplete ROM/NVM",
    )?;
    ensure(&rom[0xc700..0xef00] == nvm, "NVM differs from ROM")?;
    let settings = parse_json(&files["settings.json"])?;
    let expected = decode_trusted_rom(rom, nvm, &source)?;
    ensure(
        canonical(&settings)? == canonical(&expected)?,
        "Settings differ from validated NVM",
    )?;
    Ok(ValidatedArchive {
        manifest_sha256: identity,
        manifest,
        settings: expected,
        source,
    })
}

/// A transition is permitted only by an exact hash pair, never profile names.
/// This predicate is a prerequisite; it does not validate migration contents.
pub fn transition_allowed(
    source: &ImageIdentity,
    target: &ImageIdentity,
    pairs: &[(String, String)],
    trusted_images: &[ImageIdentity],
) -> bool {
    let trusted = |identity: &ImageIdentity| {
        let matches: Vec<_> = trusted_images
            .iter()
            .filter(|image| image.ihx_sha256 == identity.ihx_sha256)
            .collect();
        matches.len() == 1 && matches[0] == identity
    };
    if !trusted(source) || !trusted(target) {
        return false;
    }
    let known_schema = |schema| matches!(schema, 0x082001 | 0x082002 | 0x082003);
    if !known_schema(source.schema)
        || !known_schema(target.schema)
        || source.definition_sha256 != target.definition_sha256
        || (source.schema == 0x082003 && target.schema != 0x082003)
    {
        return false;
    }
    if source.ihx_sha256 == target.ihx_sha256 {
        return source == target;
    }
    pairs
        .iter()
        .any(|(s, t)| s == &source.ihx_sha256 && t == &target.ihx_sha256)
}

/// Exact firmware publication split: the sixteenth terminator must occur in
/// the final chunk, including saved zero padding (at most 27 bytes).
pub fn macro_chunks(stream: &[u8]) -> BackupResult<Vec<(usize, Vec<u8>)>> {
    ensure(
        (16..=8192).contains(&stream.len()),
        "Macro stream size unsupported",
    )?;
    let logical = macro_logical_length(stream)?;
    let final_start = stream.len().saturating_sub(28);
    ensure(
        final_start < logical,
        "Published macro padding cannot be reproduced",
    )?;
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < final_start {
        let end = (offset + 28).min(final_start);
        result.push((offset, stream[offset..end].to_vec()));
        offset = end;
    }
    result.push((offset, stream[offset..].to_vec()));
    Ok(result)
}
pub fn macro_logical_length(stream: &[u8]) -> BackupResult<usize> {
    let (mut terminated, mut operands, mut opcode, mut logical) = (0, 0, false, 0);
    for (offset, &v) in stream.iter().enumerate() {
        if terminated == 16 {
            ensure(v == 0, "Nonzero tail after macro terminators")?;
        } else if opcode {
            ensure((1..=7).contains(&v), "Unsupported macro opcode")?;
            operands = if v <= 3 { 1 } else { 2 };
            opcode = false;
        } else if operands > 0 {
            ensure(v != 0, "Zero macro operand")?;
            operands -= 1;
        } else if v == 0 {
            terminated += 1;
            logical = offset + 1;
        } else if v == 1 {
            opcode = true;
        } else {
            ensure(v < 128, "Macro text must be ASCII")?;
        }
    }
    ensure(
        terminated == 16 && operands == 0 && !opcode,
        "Incomplete macro stream",
    )?;
    Ok(logical)
}
/// Legacy schema02 RGB layout. Production callers must establish an exact
/// historical settings identity before selecting this contract.
pub(crate) fn rgb_wire(raw: &[u8], selected: bool) -> BackupResult<[u8; 6]> {
    rgb_wire_modes(raw, selected, &[(1,2),(2,6),(3,13),(4,14),(5,1),(6,31)])
}
pub(crate) fn rgb_wire_for_profile(raw: &[u8], selected: bool, profile: crate::app::ak820_beta1_profiles::ProfileId) -> BackupResult<[u8; 6]> {
    rgb_wire_modes(raw, selected, crate::app::ak820_beta1_profiles::rgb_modes(profile))
}
fn rgb_wire_modes(raw: &[u8], selected: bool, modes: &[(u8,u16)]) -> BackupResult<[u8;6]> {
    ensure(raw.len() == 8, "RGB config requires eight saved bytes")?;
    let mode = modes.iter().find(|(saved,_)| *saved == raw[0] >> 2)
        .map(|(_,wire)| *wire).ok_or("Unsupported saved RGB mode")?;
    ensure(raw[0] & 3 <= 1 && raw[3] <= 160, "Invalid saved RGB enable/brightness")?;
    let effect = if selected || raw[0] & 3 == 1 { mode } else { 0 };
    Ok([effect as u8, (effect >> 8) as u8, raw[4], raw[1], raw[2], raw[3]])
}
/// Translate by semantic wire effect, preserving enable and all other bytes.
pub(crate) fn translate_rgb(raw: &[u8], source: Option<crate::app::ak820_beta1_profiles::ProfileId>, target: crate::app::ak820_beta1_profiles::ProfileId) -> BackupResult<[u8;8]> {
    let selected = match source { Some(p) => rgb_wire_for_profile(raw,true,p)?, None => rgb_wire(raw,true)? };
    let effect = u16::from_le_bytes([selected[0],selected[1]]);
    let saved = crate::app::ak820_beta1_profiles::rgb_modes(target).iter()
        .find(|(_,wire)| *wire == effect).map(|(saved,_)| *saved)
        .ok_or("Target does not support saved RGB effect")?;
    let mut result: [u8;8] = raw.try_into().map_err(|_| "RGB config requires eight saved bytes")?;
    result[0] = (saved << 2) | (raw[0] & 3);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic envelope bytes are deliberately not a valid firmware ROM.
    // The decoder rejection proves envelope integrity is never sufficient.
    #[test]
    fn envelope_never_bypasses_trusted_decoder() {
        let image = ImageIdentity {
            ihx_sha256: "a".repeat(64),
            code_sha256: "b".repeat(64),
            definition_sha256: "c".repeat(64),
            schema: 0x082003,
        };
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert("settings.json".into(), b"{}\n".to_vec());
        files.insert("before-full-rom.bin".into(), vec![0; 65536]);
        files.insert("saved-nvm.bin".into(), vec![0; 0x2800]);
        files.insert("read-log.jsonl".into(), Vec::new());
        files.insert(".gitignore".into(), b"*\n".to_vec());
        let records: Map<String, Value> =
            files.iter().map(|(k, v)| (k.clone(), record(v))).collect();
        let mut manifest = json!({"format":FORMAT_V2,"created_utc":"2026-10-25T09:00:00+00:00","scope":"saved supported settings and exposed RGB values; not all current RAM","device":{"vid":0x320f,"pid":0x505b,"vial_uid":"0470ff9001b915d3"},"definition_sha256":image.definition_sha256,"nvm_schema":image.schema,"source_build_id":image.schema,"source_ihx_sha256":image.ihx_sha256,"code_sha256":image.code_sha256,"counts":{"layers":6,"physical_keys":83,"assignments":498,"encoder_assignments":12,"macros":16,"macro_capacity":8192},"files":records});
        let raw = canonical(&manifest).unwrap();
        for field in ["created_utc", "scope"] {
            let mut incomplete = manifest.clone();
            incomplete.as_object_mut().unwrap().remove(field);
            let error = validate_archive(
                &canonical(&incomplete).unwrap(),
                &files,
                std::slice::from_ref(&image),
                None,
                |_, _, _| panic!("incomplete manifest reached ROM decoder"),
            )
            .unwrap_err();
            assert!(error.contains("Missing or unknown"));
        }
        let err = validate_archive(
            &raw,
            &files,
            std::slice::from_ref(&image),
            None,
            |_, _, _| Err("ROM is unqualified".into()),
        )
        .unwrap_err();
        assert_eq!(err, "ROM is unqualified");
        manifest["unknown"] = json!(1);
        assert!(validate_archive(
            &canonical(&manifest).unwrap(),
            &files,
            std::slice::from_ref(&image),
            None,
            |_, _, _| panic!("unknown field reached decoder")
        )
        .is_err());
        manifest.as_object_mut().unwrap().remove("unknown");
        manifest["nvm_schema"] = json!(532483.0);
        assert!(validate_archive(
            &serde_json::to_vec(&manifest).unwrap(),
            &files,
            std::slice::from_ref(&image),
            None,
            |_, _, _| panic!("float schema reached decoder")
        )
        .is_err());
        files.get_mut(".gitignore").unwrap()[0] = b'!';
        assert!(validate_archive(
            &raw,
            &files,
            std::slice::from_ref(&image),
            None,
            |_, _, _| panic!("tampering reached decoder")
        )
        .is_err());
    }
    #[test]
    fn downgrade_is_denied_even_with_explicit_pair() {
        let source = ImageIdentity {
            ihx_sha256: "a".repeat(64),
            code_sha256: "b".repeat(64),
            definition_sha256: "c".repeat(64),
            schema: 0x082003,
        };
        let mut target = source.clone();
        target.ihx_sha256 = "d".repeat(64);
        target.schema = 0x082002;
        let trusted = vec![source.clone(), target.clone()];
        assert!(!transition_allowed(
            &source,
            &target,
            &[(source.ihx_sha256.clone(), target.ihx_sha256.clone())],
            &trusted
        ));
        target.schema = 0x082003;
        let trusted = vec![source.clone(), target.clone()];
        assert!(!transition_allowed(&source, &target, &[], &trusted));
        assert!(transition_allowed(
            &source,
            &target,
            &[(source.ihx_sha256.clone(), target.ihx_sha256.clone())],
            &trusted
        ));
    }
    #[test]
    fn inconsistent_or_untrusted_transition_identity_is_rejected() {
        let source = ImageIdentity {
            ihx_sha256: "a".repeat(64),
            code_sha256: "b".repeat(64),
            definition_sha256: "c".repeat(64),
            schema: 0x082002,
        };
        let trusted = vec![source.clone()];
        assert!(transition_allowed(&source, &source, &[], &trusted));
        assert!(!transition_allowed(&source, &source, &[], &[]));
        for field in 0..3 {
            let mut conflicting = source.clone();
            match field {
                0 => conflicting.schema = 0x082001,
                1 => conflicting.definition_sha256 = "d".repeat(64),
                _ => conflicting.code_sha256 = "e".repeat(64),
            }
            assert!(!transition_allowed(
                &source,
                &conflicting,
                &[(source.ihx_sha256.clone(), source.ihx_sha256.clone())],
                &trusted
            ));
        }
    }
    #[test]
    fn strict_nested_json() {
        assert!(parse_json(br#"{"a":{"x":1,"x":2}}"#).is_err());
        assert!(parse_json(b"NaN").is_err());
        assert!(parse_json(b"{} {}").is_err());
        assert!(parse_json(br#"{"v":true}"#).is_ok());
    }
    #[test]
    fn python_canonical_golden() {
        assert_eq!(
            canonical(&parse_json("{\"z\":\"Рус\",\"a\":[1,true]}".as_bytes()).unwrap()).unwrap(),
            "{\n  \"a\": [\n    1,\n    true\n  ],\n  \"z\": \"Рус\"\n}\n".as_bytes()
        );
        assert!(canonical(&json!(1.0)).is_err());
    }
    #[test]
    fn macro_publication_boundaries() {
        let mut stream = vec![0; 16];
        assert_eq!(macro_chunks(&stream).unwrap(), vec![(0, stream.clone())]);
        stream.resize(43, 0);
        assert!(macro_chunks(&stream).is_ok());
        stream.push(0);
        assert!(macro_chunks(&stream).is_err());
        assert!(macro_logical_length(&[0; 15]).is_err());
        let mut commands = vec![1, 5, 2, 3];
        commands.extend([0; 16]);
        assert_eq!(macro_logical_length(&commands).unwrap(), 20);
        commands[2] = 0;
        assert!(macro_logical_length(&commands).is_err());
    }
    #[test]
    fn rgb_saved_and_effective() {
        assert_eq!(
            rgb_wire(&[8, 10, 20, 160, 4, 0, 0, 0], false).unwrap(),
            [0, 0, 4, 10, 20, 160]
        );
        assert_eq!(
            rgb_wire(&[8, 10, 20, 160, 4, 0, 0, 0], true).unwrap(),
            [6, 0, 4, 10, 20, 160]
        );
        assert!(rgb_wire(&[9, 0, 0, 161, 0, 0, 0, 0], false).is_err());
    }
}
