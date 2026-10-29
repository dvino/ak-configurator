//! Offline Vial `.vil` keymap/encoder interchange, not a settings backup.
//!
//! The bundled definition is checked against the published beta.3/beta.4.1
//! reference digest. This does not identify installed firmware or authorize a
//! write; UID equality alone does not prove keycode compatibility.
//! This module never mutates its input or authorizes a device operation. Project
//! conversion returns warnings; callers must display them before accepting it.
//! Export is partial, not a backup, and restoring it in Vial writes to a device.
//! Import targets the AK820 protocol-6 keycode space. For protocol 5 or missing/
//! unknown protocol, only codes 0x00..=0x7f and 0xe0..=0xe7 are accepted;
//! other source codes are rejected instead of being silently reinterpreted.
//! Protocol-6 symbols cover a bounded subset of upstream names and expressions;
//! unsupported or ambiguous strings are rejected. Other project sections are
//! imported only where explicitly represented and otherwise reported unchanged.
//!
//! Shape follows vial-gui aef8222a2d0429a183b2ed692d5f9efcfd383f08,
//! `protocol/keyboard_comm.py` save_layout/restore_layout.
use serde_json::{json, Value};
use crate::app::Ak820OfflineProfileProject;
use crate::keyboard::KeyBinding;
use crate::keycode_picker::MacroAction;

const MAX_JSON: usize = 2 * 1024 * 1024;
type Result<T> = std::result::Result<T, String>;

/// Matrix coordinates are in project physical-key order; encoder directions
/// are in Vial order (clockwise, counterclockwise).
#[derive(Clone, Debug)]
struct Target {
    pub uid: u64,
    pub rows: usize,
    pub columns: usize,
    pub positions: Vec<[u8; 2]>,
    pub layers: usize,
    pub encoders: usize,
    pub layout_options: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Keymap {
    pub layers: Vec<Vec<u16>>,
    pub encoders: Vec<Vec<[u16; 2]>>,
}

fn target_valid(t: &Target) -> Result<()> {
    if t.layers == 0 || t.layers > 32 || t.rows == 0 || t.rows > 256 || t.columns == 0 || t.columns > 256 || t.encoders > 32 || t.positions.is_empty() {
        return Err("Invalid target dimensions".into());
    }
    let mut seen = std::collections::HashSet::new();
    for &[r, c] in &t.positions {
        if r as usize >= t.rows || c as usize >= t.columns || !seen.insert((r, c)) {
            return Err("Invalid or duplicate physical position".into());
        }
    }
    Ok(())
}

fn keycode(v: &Value, protocol: Option<u64>) -> Result<u16> {
    if let Some(n) = v.as_u64() {
        return u16::try_from(n).map_err(|_| "Keycode out of range".into());
    }
    if let Some(s) = v.as_str() {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            return u16::from_str_radix(hex, 16).map_err(|_| "Invalid hexadecimal keycode".into());
        }
        return symbol(s, protocol);

    }
    Err("Unknown or invalid keycode".into())
}

// Derived from Keycode.serialize in pinned vial-gui, with six layers and
// sixteen macro slots. Do not use the app's keycode table: protocol values differ.
fn symbol(s: &str, protocol: Option<u64>) -> Result<u16> {
    symbol_inner(s, protocol, 0)
}

fn symbol_inner(s: &str, protocol: Option<u64>, depth: usize) -> Result<u16> {
    if depth > 8 { return Err("Vial expression nesting limit exceeded".into()); }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u16::from_str_radix(hex, 16).map_err(|_| "Invalid hexadecimal keycode".into());
    }
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        return s.parse::<u16>().map_err(|_| "Numeric expression out of range".into());
    }
    if let Some((outer, inner)) = s.split_once('(') {
        let inner = inner.strip_suffix(')').ok_or("Invalid Vial expression")?;
        if matches!(outer, "MO" | "TO" | "TG" | "DF" | "TT" | "OSL" | "TD") {
            let n: u16 = inner.parse().map_err(|_| "Invalid Vial layer or tap-dance index")?;
            if n > if outer == "TD" { 7 } else { 5 } { return Err("Vial layer or tap-dance index out of range".into()); }
            let base = match (outer, protocol) {
                ("TD", Some(5 | 6)) => 0x5700,
                ("MO", Some(5)) => 0x5100, ("TO", Some(5)) => 0x5010,
                ("TG", Some(5)) => 0x5300, ("DF", Some(5)) => 0x5200,
                ("TT", Some(5)) => 0x5800, ("OSL", Some(5)) => 0x5400,
                ("MO", Some(6)) => 0x5220, ("TO", Some(6)) => 0x5200,
                ("TG", Some(6)) => 0x5260, ("DF", Some(6)) => 0x5240,
                ("TT", Some(6)) => 0x52c0, ("OSL", Some(6)) => 0x5280,
                _ => return Err("Vial protocol 5 or 6 required for layer expression".into()),
            };
            return Ok(base | n);
        }
        let inner = symbol_inner(inner, protocol, depth + 1)?;
        let (base, tap) = match outer {
            "LCTL" => (0x0100, false), "LSFT" => (0x0200, false), "C_S" => (0x0300, false),
            "LALT" => (0x0400, false), "LCA" => (0x0500, false), "LSA" => (0x0600, false),
            "MEH" => (0x0700, false), "LGUI" => (0x0800, false), "LCG" => (0x0900, false),
            "SGUI" => (0x0a00, false), "LAG" => (0x0c00, false), "LCAG" => (0x0d00, false),
            "HYPR" => (0x0f00, false), "RCTL" => (0x1100, false), "RSFT" => (0x1200, false),
            "RALT" => (0x1400, false), "RGUI" => (0x1800, false), "RCG" => (0x1900, false),
            "LCTL_T" => (0x0100, true), "LSFT_T" => (0x0200, true), "C_S_T" => (0x0300, true),
            "LALT_T" => (0x0400, true), "LCA_T" => (0x0500, true), "LSA_T" => (0x0600, true),
            "MEH_T" => (0x0700, true), "LGUI_T" => (0x0800, true), "LCG_T" => (0x0900, true),
            "SGUI_T" => (0x0a00, true), "LAG_T" => (0x0c00, true), "LCAG_T" => (0x0d00, true),
            "ALL_T" => (0x0f00, true), "RCTL_T" => (0x1100, true), "RSFT_T" => (0x1200, true),
            "RALT_T" => (0x1400, true), "RGUI_T" => (0x1800, true), "RCG_T" => (0x1900, true),
            "RCAG_T" => (0x1d00, true),
            _ if outer.starts_with("LT") && outer.len() == 3 && matches!(outer.as_bytes()[2], b'0'..=b'5') =>
                (0x4000 | u16::from(outer.as_bytes()[2] - b'0') << 8, false),
            _ => return Err(format!("Unsupported Vial expression: {s}")),
        };
        let offset = if tap { match protocol { Some(5) => 0x6000, Some(6) => 0x2000, _ => return Err("Vial protocol 5 or 6 required for mod-tap".into()) } } else { 0 };
        if (tap || outer.starts_with("LT") && outer.len() == 3) && inner > 0xff {
            return Err("Vial tap expression would truncate an extended keycode".into());
        }
        if !tap && !outer.starts_with("LT") && inner >= 0x2000 {
            return Err("Vial modifier expression contains an extended keycode".into());
        }
        return Ok(base | offset | inner);
    }
    if let Some(n) = s.strip_prefix('M') {
        if let Ok(index) = n.parse::<u16>() {
            if index < 16 && n == index.to_string() {
                return match protocol {
                    Some(5) => Ok(0x5f12 + index), Some(6) => Ok(0x7700 + index),
                    _ => Err("Vial protocol 5 or 6 required for macro keycode".into()),
                };
            }
        }
    }
    if let Some(letter) = s.strip_prefix("KC_") {
        if letter.len() == 1 {
            let b = letter.as_bytes()[0];
            if b.is_ascii_uppercase() { return Ok(4 + u16::from(b - b'A')); }
            if (b'1'..=b'9').contains(&b) { return Ok(0x1e + u16::from(b - b'1')); }
            if b == b'0' { return Ok(0x27); }
        }
    }
    const BASIC: &[&str] = &["KC_ENTER", "KC_ESCAPE", "KC_BSPACE", "KC_TAB", "KC_SPACE", "KC_MINUS", "KC_EQUAL", "KC_LBRACKET", "KC_RBRACKET", "KC_BSLASH", "KC_NONUS_HASH", "KC_SCOLON", "KC_QUOTE", "KC_GRAVE", "KC_COMMA", "KC_DOT", "KC_SLASH", "KC_CAPSLOCK", "KC_F1", "KC_F2", "KC_F3", "KC_F4", "KC_F5", "KC_F6", "KC_F7", "KC_F8", "KC_F9", "KC_F10", "KC_F11", "KC_F12", "KC_PSCREEN", "KC_SCROLLLOCK", "KC_PAUSE", "KC_INSERT", "KC_HOME", "KC_PGUP", "KC_DELETE", "KC_END", "KC_PGDOWN", "KC_RIGHT", "KC_LEFT", "KC_DOWN", "KC_UP", "KC_NUMLOCK", "KC_KP_SLASH", "KC_KP_ASTERISK", "KC_KP_MINUS", "KC_KP_PLUS", "KC_KP_ENTER", "KC_KP_1", "KC_KP_2", "KC_KP_3", "KC_KP_4", "KC_KP_5", "KC_KP_6", "KC_KP_7", "KC_KP_8", "KC_KP_9", "KC_KP_0", "KC_KP_DOT", "KC_NONUS_BSLASH", "KC_APPLICATION"];
    if let Some(i) = BASIC.iter().position(|name| *name == s) { return Ok(0x28 + i as u16); }
    const MEDIA: &[&str] = &["KC_PWR", "KC_SLEP", "KC_WAKE", "KC_MUTE", "KC_VOLU", "KC_VOLD", "KC_MNXT", "KC_MPRV", "KC_MSTP", "KC_MPLY", "KC_MSEL", "KC_EJCT", "KC_MAIL", "KC_CALC", "KC_MYCM", "KC_WSCH", "KC_WHOM", "KC_WBAK", "KC_WFWD", "KC_WSTP", "KC_WREF", "KC_WFAV", "KC_MFFD", "KC_MRWD", "KC_BRIU", "KC_BRID"];
    if let Some(i) = MEDIA.iter().position(|name| *name == s) { return Ok(0xa5 + i as u16); }
    const MODIFIERS: &[&str] = &["KC_LCTRL", "KC_LSHIFT", "KC_LALT", "KC_LGUI", "KC_RCTRL", "KC_RSHIFT", "KC_RALT", "KC_RGUI"];
    if let Some(i) = MODIFIERS.iter().position(|name| *name == s) { return Ok(0xe0 + i as u16); }
    const MOUSE: &[&str] = &["KC_MS_U", "KC_MS_D", "KC_MS_L", "KC_MS_R", "KC_BTN1", "KC_BTN2", "KC_BTN3", "KC_BTN4", "KC_BTN5"];
    if let Some(i) = MOUSE.iter().position(|name| *name == s) {
        return match protocol { Some(5) => Ok(0xf0 + i as u16), Some(6) => Ok(0xcd + i as u16), _ => Err("Vial protocol 5 or 6 required for mouse keycode".into()) };
    }
    match s {
        "KC_NO" => Ok(0), "KC_TRNS" => Ok(1),
        "RGB_HUI" => match protocol { Some(5) => Ok(0x5cc5), Some(6) => Ok(0x7823), _ => Err("Vial protocol 5 or 6 required for RGB_HUI".into()) },
        _ => Err(format!("Unsupported or unverified Vial symbol or compound expression: {s}")),
    }
}

// The offline AK820 project uses protocol-6 keycodes. The source firmware may
// be unknown, but known incompatible protocol ranges cannot be passed through.
fn source_keycode(v: &Value, protocol: Option<u64>) -> Result<u16> {
    let code = keycode(v, protocol)?;
    if protocol != Some(6) && !(code <= 0x7f || (0xe0..=0xe7).contains(&code)) {
        return Err(format!("Vial source protocol {protocol:?}: keycode 0x{code:04x} may differ from AK820 protocol 6; conversion unsupported"));
    }
    Ok(code)
}

fn array<'a>(v: &'a Value, len: usize) -> Result<&'a Vec<Value>> {
    let a = v.as_array().ok_or("Expected array")?;
    if a.len() != len { return Err("Incorrect dimensions".into()); }
    Ok(a)
}

const ENCODING_FIELD: &str = "ak820_keycode_encoding";
const ENCODING_V6: &str = "vial-qmk-protocol-6";

// This optional private declaration describes file encoding, not device identity.
fn file_protocol(doc: &Value) -> Result<Option<u64>> {
    let actual = doc.get("vial_protocol").and_then(Value::as_u64);
    if let Some(value) = doc.get(ENCODING_FIELD) {
        if value.as_str() != Some(ENCODING_V6) { return Err("Unsupported AK820 keycode encoding declaration".into()); }
        if doc.get("vial_protocol").is_some() && actual != Some(6) { return Err("Conflicting Vial protocol and AK820 keycode encoding".into()); }
        return Ok(Some(6));
    }
    Ok(actual)
}

fn parse(raw: &[u8], t: &Target) -> Result<(Value, Keymap)> {
    target_valid(t)?;
    if raw.len() > MAX_JSON { return Err("Vial file exceeds size limit".into()); }
    let doc = crate::ak820_settings_backup::parse_json(raw).map_err(|e| e.to_string())?;
    let obj = doc.as_object().ok_or("Expected Vial object")?;
    if obj.get("version").and_then(Value::as_u64) != Some(1) { return Err("Unsupported Vial version".into()); }
    if obj.get("uid").and_then(Value::as_u64) != Some(t.uid) { return Err("Vial UID mismatch or invalid UID".into()); }
    if obj.get("layout_options").and_then(Value::as_i64) != Some(t.layout_options) { return Err("Vial layout options differ or are invalid".into()); }
    let protocol = file_protocol(&doc)?;
    let layout = array(obj.get("layout").ok_or("Missing layout")?, t.layers)?;
    let encoders = array(obj.get("encoder_layout").ok_or("Missing encoder layout")?, t.layers)?;
    let mut result = Keymap { layers: Vec::new(), encoders: Vec::new() };
    for (layer, encoder_layer) in layout.iter().zip(encoders) {
        let rows = array(layer, t.rows)?;
        let mut codes = vec![0; t.positions.len()];
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in array(row, t.columns)?.iter().enumerate() {
                if let Some(i) = t.positions.iter().position(|&[pr, pc]| pr as usize == r && pc as usize == c) {
                    codes[i] = source_keycode(value, protocol)?;
                } else if value.as_i64() != Some(-1) {
                    return Err("Unexpected populated matrix position".into());
                }
            }
        }
        result.layers.push(codes);
        let mut dirs = Vec::new();
        for encoder in array(encoder_layer, t.encoders)? {
            let pair = array(encoder, 2)?;
            dirs.push([source_keycode(&pair[0], protocol)?, source_keycode(&pair[1], protocol)?]);
        }
        result.encoders.push(dirs);
    }
    Ok((doc, result))
}

/// Preview only; callers must show the warnings and obtain an explicit decision
/// before applying these assignments to a project. No device write is authorized.
pub(crate) struct Preview {
    pub map: Keymap,
    pub warnings: Vec<String>,
}

fn import(raw: &[u8], target: &Target) -> Result<Preview> {
    let (doc, map) = parse(raw, target)?;
    let mut warnings = vec!["Import replaces every project matrix layer and encoder assignment.".into(), "Source firmware and profile are not verified; keycode meanings may differ. This is not a full backup.".into()];
    for (name, value) in doc.as_object().unwrap() {
        match name.as_str() {
            "version" | "uid" | "layout" | "encoder_layout" | "layout_options" | ENCODING_FIELD => (),
            "vial_protocol" | "via_protocol" if value.as_i64().is_some() || value.as_u64().is_some() => (),
            "macro" if value.as_array().is_some_and(|slots| slots.iter().all(|v| v.as_array().is_some_and(|a| a.is_empty()))) => (),
            "tap_dance" | "combo" | "key_override" | "alt_repeat_key" if value.as_array().is_some_and(|v| v.is_empty()) => (),
            "settings" if value.as_object().is_some_and(|v| v.is_empty()) => (),
            _ => warnings.push(format!("Not imported: {name}")),
        }
    }
    Ok(Preview { map, warnings })
}

/// A caller must display every warning and explicitly confirm before using data.
pub(crate) struct ProjectImport {
    pub project: Ak820OfflineProfileProject,
    pub warnings: Vec<String>,
}
pub(crate) struct ProjectExport {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

fn project_target(project: &Ak820OfflineProfileProject, uid: u64, layout_options: i64) -> Result<Target> {
    // Raw published beta.3 reference and pinned beta.4.1 source share this hash.
    let bytes = include_bytes!("../assets/ak820/layout.json");
    if crate::ak820_settings_backup::sha256(bytes) != "080192880c40c102022fe6b75dc865b1d36638ffd4e6585cfd429eed3478c071" {
        return Err("Bundled AK820 reference definition digest mismatch".into());
    }
    let definition: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let reference = crate::keyboard::KeyboardLayout::from_vial_json(&definition).map_err(|e| e.to_string())?;
    let layout = &project.layout;
    let mut positions: Vec<_> = layout.keys.iter().map(|k| [k.row, k.col]).collect();
    let mut expected: Vec<_> = reference.keys.iter().map(|k| [k.row, k.col]).collect();
    positions.sort_unstable();
    expected.sort_unstable();
    let mut directions: Vec<_> = layout.encoders.iter().map(|e| [e.encoder_idx, e.direction]).collect();
    let mut expected_directions: Vec<_> = reference.encoders.iter().map(|e| [e.encoder_idx, e.direction]).collect();
    directions.sort_unstable();
    expected_directions.sort_unstable();
    if uid != crate::ak820::VIAL_UID || layout_options != -1 || !reference.layout_options.is_empty()
        || (layout.rows, layout.cols, layout.layers.len()) != (16, 8, 6)
        || (reference.rows, reference.cols, expected.len()) != (16, 8, 83)
        || positions != expected || directions != expected_directions
        || directions != vec![[0, 0], [0, 1]] {
        return Err("Project or target differs from the pinned AK820 reference definition".into());
    }
    let target = Target { uid, rows: layout.rows, columns: layout.cols,
        positions: layout.keys.iter().map(|k| [k.row, k.col]).collect(),
        layers: 6, encoders: 1, layout_options };
    target_valid(&target)?;
    Ok(target)
}

fn encoder_indices(project: &Ak820OfflineProfileProject) -> [usize; 2] {
    [0, 1].map(|direction| project.layout.encoders.iter().position(|e| e.encoder_idx == 0 && e.direction == direction).unwrap())
}

/// Convert into a clone; callers can keep the original for undo. UID and layout
/// options must come from a separately established target, not the Vial file.
pub(crate) fn import_project(raw: &[u8], project: &Ak820OfflineProfileProject, uid: u64, layout_options: i64) -> Result<ProjectImport> {
    let target = project_target(project, uid, layout_options)?;
    let preview = import(raw, &target)?;
    let (doc, _) = parse(raw, &target)?;
    let mut candidate = project.clone();
    let indices = encoder_indices(project);
    candidate.layout.layers = preview.map.layers.into_iter().map(|l| l.into_iter().map(KeyBinding::Vial).collect()).collect();
    for (l, encoders) in preview.map.encoders.iter().enumerate() {
        if candidate.layout.encoder_layers.get(l).is_none_or(|layer| layer.len() != 2) { return Err("Invalid project encoder layers".into()); }
        for d in 0..2 { candidate.layout.encoder_layers[l][indices[d]] = encoders[0][d]; }
    }
    crate::ak820_offline_project::export_project(&candidate)?;
    let mut warnings = preview.warnings;
    for name in ["macro", "combo"] {
        let warning = format!("Not imported: {name}");
        if doc.get(name).is_some() && !warnings.contains(&warning) { warnings.push(warning); }
    }
    if let Some(values) = doc.get("combo").and_then(Value::as_array) {
        if values.len() == candidate.combo_entries.len() {
            let mut next = candidate.clone();
            let converted = values.iter().zip(&mut next.combo_entries).try_for_each(|(value, entry)| -> Result<()> {
                let v = array(value, 5)?;
                let protocol = file_protocol(&doc)?;
                let codes = v.iter().map(|v| source_keycode(v, protocol)).collect::<Result<Vec<_>>>()?;
                entry.keys = [codes[0], codes[1], codes[2], codes[3]];
                entry.output = KeyBinding::Vial(codes[4]);
                entry.layer = None;
                Ok(())
            });
            match converted {
                Ok(()) if crate::ak820_offline_project::export_project(&next).is_ok() => {
                    candidate = next;
                    warnings.retain(|w| w != "Not imported: combo");
                    warnings.push("Import replaces all project combo slots, including empty slots.".into());
                }
                Err(error) => warnings.push(format!("Combo unchanged: {error}")),
                _ => (),
            }
        }
    }
    if let Some(slots) = doc.get("macro").and_then(Value::as_array) {
        if slots.len() == candidate.macro_texts.len() && slots.len() == candidate.macro_actions.len() {
            let mut next = candidate.clone();
            let converted = slots.iter().enumerate().try_for_each(|(i, slot)| -> Result<()> {
                let actions = slot.as_array().ok_or("Invalid macro")?;
                let mut text = String::new();
                for action in actions {
                    let pair = array(action, 2)?;
                    if pair[0].as_str() != Some("text") { return Err("Unsupported macro action".into()); }
                    let s = pair[1].as_str().ok_or("Invalid macro text")?;
                    if !s.bytes().all(|b| (2..128).contains(&b)) { return Err("Unsupported macro text".into()); }
                    text.push_str(s);
                }
                next.macro_texts[i] = text.as_bytes().to_vec();
                next.macro_actions[i] = if text.is_empty() { vec![] } else { vec![MacroAction::Text(text)] };
                Ok(())
            });
            if converted.is_ok() && crate::ak820_offline_project::export_project(&next).is_ok() {
                candidate = next;
                warnings.retain(|w| w != "Not imported: macro");
                warnings.push("Import replaces all project macro contents and actions, including empty slots.".into());
            }
        }
    }
    Ok(ProjectImport { project: candidate, warnings })
}

fn macros_consistent(project: &Ak820OfflineProfileProject) -> bool {
    project.macro_texts.len() == project.macro_actions.len()
        && project.macro_texts.iter().zip(&project.macro_actions)
            .all(|(bytes, actions)| crate::keycode_picker::decode_macro_actions(bytes) == *actions)
}

/// Create a partial Vial file directly from the project, never from a template.
/// Absent macro/dynamic sections are ignored by the pinned Vial restore code.
pub(crate) fn export_project(project: &Ak820OfflineProfileProject, uid: u64, layout_options: i64) -> Result<ProjectExport> {
    crate::ak820_offline_project::export_project(project)?;
    let target = project_target(project, uid, layout_options)?;
    let indices = encoder_indices(project);
    let mut layout = Vec::new();
    let mut encoders = Vec::new();
    for (keys, directions) in project.layout.layers.iter().zip(&project.layout.encoder_layers) {
        if keys.len() != target.positions.len() || directions.len() != 2 { return Err("Invalid project dimensions".into()); }
        let mut rows = vec![vec![json!(-1); target.columns]; target.rows];
        for (&[r, c], binding) in target.positions.iter().zip(keys) {
            let KeyBinding::Vial(code) = binding else { return Err("Non-Vial binding".into()); };
            rows[r as usize][c as usize] = json!(code);
        }
        layout.push(rows);
        encoders.push(vec![[directions[indices[0]], directions[indices[1]]]]);
    }
    if layout.len() != target.layers { return Err("Invalid project layer count".into()); }
    // Vial ignores this private encoding declaration during restore; it is not
    // the source-keyboard protocol metadata emitted by upstream Vial.
    let doc = json!({"version": 1, "uid": uid, "layout": layout, "encoder_layout": encoders, "layout_options": layout_options, "ak820_keycode_encoding": ENCODING_V6});
    let mut warnings = vec!["Partial keymap export, not a full backup; source firmware and target compatibility are not verified. Vial restore writes to the device.".into(),
        "The AK820 keycode encoding declaration describes this file, not a verified source device or firmware; Vial and VIA source protocols are unknown and omitted.".into(),
        "Not exported: tap dance, key overrides, alternate repeat keys, settings, RGB, layer names and project metadata.".into()];
    // Published capacities do not establish the actual restore target's profile.
    warnings.push("Not exported: macros and combos; actual target capacities are unknown. Including these sections would overwrite target slots, and Vial may truncate macros.".into());
    if !macros_consistent(project) {
        warnings.push("Macro stream and action representations differ; neither is exported.".into());
    }
    let bytes = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_JSON { return Err("Vial file exceeds size limit".into()); }
    Ok(ProjectExport { bytes, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    // Synthetic example: numeric and hexadecimal encoder codes are intentional;
    // these values were not produced by upstream's keycode serializer.
    const FIXTURE: &[u8] = br#"{"version": 1, "uid": 9007199254740993, "layout": [[["KC_A", -1]]], "encoder_layout": [[["0x0004", 5]]], "layout_options": 0, "macro": [], "vial_protocol": 6, "via_protocol": 9, "tap_dance": [], "combo": [], "key_override": [], "alt_repeat_key": [], "settings": {}}"#;
    // Copied from the pinned upstream test_keyboard simulated-device serializer:
    // /tmp/vial-upstream.EcbzND/harness.py generate upstream.vil.
    const UPSTREAM: &[u8] = br#"{"version": 1, "uid": 0, "layout": [[["KC_TRNS", "0x2"], ["0x3", "KC_A"]], [["KC_B", "KC_C"], ["KC_G", "KC_E"]]], "encoder_layout": [[], []], "layout_options": -1, "macro": [], "vial_protocol": 0, "via_protocol": 9, "tap_dance": [], "combo": [], "key_override": [], "alt_repeat_key": [], "settings": {}}"#;
    fn target() -> Target { Target { uid: 9007199254740993, rows: 1, columns: 2, positions: vec![[0, 0]], layers: 1, encoders: 1, layout_options: 0 } }
    #[test]
    fn import_shape_and_upstream_fixture() {
        let preview = import(FIXTURE, &target()).unwrap();
        assert_eq!(preview.map, Keymap { layers: vec![vec![4]], encoders: vec![vec![[4, 5]]] });
        assert!(preview.warnings.iter().any(|w| w.contains("replaces every project matrix")));
        let t = Target { uid: 0, rows: 2, columns: 2, positions: vec![[0, 0], [0, 1], [1, 0], [1, 1]], layers: 2, encoders: 0, layout_options: -1 };
        assert_eq!(import(UPSTREAM, &t).unwrap().map.layers, vec![vec![1, 2, 3, 4], vec![5, 6, 10, 8]]);
    }
    #[test]
    fn upstream_serializer_protocol_fixtures() {
        // Generated with pinned Keycode.serialize after recreate_keyboard_keycodes
        // for six layers, 16 macros and eight tap dances, under each protocol.
        let v5 = json!([["KC_1",30],["KC_ENTER",40],["LSFT(KC_A)",516],["LT1(KC_A)",16644],["LCTL_T(KC_A)",24836],["RGB_HUI",23749]]);
        let v6 = json!([["KC_1",30],["KC_ENTER",40],["LSFT(KC_A)",516],["LT1(KC_A)",16644],["LCTL_T(KC_A)",8452],["RGB_HUI",30755]]);
        for (protocol, fixture) in [(5, v5), (6, v6)] {
            for pair in fixture.as_array().unwrap() {
                assert_eq!(keycode(&pair[0], Some(protocol)).unwrap() as u64, pair[1].as_u64().unwrap());
            }
        }
        // Independently evaluated by pinned Keycode.deserialize(..., reraise=True)
        // with upstream simpleeval under Python 3.11, for both protocol tables.
        for (protocol, macro0, macro15, tap, mouse) in [(5, 0x5f12, 0x5f21, 0x6104, 0xf0), (6, 0x7700, 0x770f, 0x2104, 0xcd)] {
            for (text, expected) in [("M0", macro0), ("M15", macro15), ("LCTL(LSFT(KC_A))", 0x0304), ("LT1(KC_A)", 0x4104), ("LCTL_T(KC_A)", tap), ("KC_LCTRL", 0xe0), ("KC_VOLU", 0xa9), ("KC_MS_U", mouse)] {
                assert_eq!(keycode(&json!(text), Some(protocol)).unwrap(), expected);
            }
        }
        let mut file: Value = serde_json::from_slice(FIXTURE).unwrap();
        file["layout"][0][0][0] = json!("LCTL(LSFT(KC_A))");
        file["encoder_layout"][0][0][0] = json!("M0");
        let preview = import(&serde_json::to_vec(&file).unwrap(), &target()).unwrap();
        assert_eq!(preview.map.layers[0][0], 0x0304);
        assert_eq!(preview.map.encoders[0][0][0], 0x7700);
        let combo = json!(["KC_A", "KC_B", "KC_NO", "KC_NO", "LCTL(LSFT(KC_A))"]);
        assert_eq!(combo.as_array().unwrap().iter().map(|v| keycode(v, Some(6)).unwrap()).collect::<Vec<_>>(), vec![4, 5, 0, 0, 0x0304]);
        for expression in ["LT1(MO(1))", "LCTL_T(M0)", "LCTL(MO(1))", "LT1(LSFT(KC_A))", "LCTL_T(LSFT(KC_A))"] {
            assert!(keycode(&json!(expression), Some(6)).is_err());
        }
        assert!(keycode(&json!("M0"), None).is_err());
        assert!(keycode(&json!("M16"), Some(6)).is_err());
        assert!(keycode(&json!("LCTL_T(KC_A)"), None).is_err());
        assert_eq!(keycode(&json!("MO(1)"), Some(5)).unwrap(), 0x5101);
        assert_eq!(keycode(&json!("MO(1)"), Some(6)).unwrap(), 0x5221);
        assert!(keycode(&json!("MO(6)"), Some(6)).is_err());
    }
    #[test]
    fn source_protocol_never_reinterprets_sensitive_codes() {
        for value in [json!("DF(1)"), json!(0x5201), json!("0x5201")] {
            assert!(source_keycode(&value, Some(5)).is_err());
        }
        for protocol in [None, Some(0), Some(7)] {
            for value in [json!(0x5201), json!("0x5201"), json!("DF(1)")] {
                assert!(source_keycode(&value, protocol).is_err());
            }
            assert_eq!(source_keycode(&json!("KC_A"), protocol).unwrap(), 4);
        }
        assert_eq!(source_keycode(&json!("DF(1)"), Some(6)).unwrap(), 0x5241);
        assert_eq!(source_keycode(&json!(0x5201), Some(6)).unwrap(), 0x5201);
        assert_eq!(source_keycode(&json!(0x5221), Some(6)).unwrap(), 0x5221);
        assert!(source_keycode(&json!(0x5221), None).is_err());
        assert_eq!(source_keycode(&json!("KC_LCTRL"), Some(5)).unwrap(), 0xe0);
        let mut file: Value = serde_json::from_slice(FIXTURE).unwrap();
        file["vial_protocol"] = json!(5);
        file["encoder_layout"][0][0][0] = json!("0x5201");
        assert!(import(&serde_json::to_vec(&file).unwrap(), &target()).is_err());
        file["encoder_layout"][0][0][0] = json!(4);
        file["layout"][0][0][0] = json!("DF(1)");
        assert!(import(&serde_json::to_vec(&file).unwrap(), &target()).is_err());
        file["vial_protocol"] = json!(6);
        file["layout"][0][0][0] = json!("LT1(MO(1))");
        assert!(import(&serde_json::to_vec(&file).unwrap(), &target()).is_err());
        file["layout"][0][0][0] = json!("KC_A");
        file["encoder_layout"][0][0][0] = json!("LCTL_T(M0)");
        assert!(import(&serde_json::to_vec(&file).unwrap(), &target()).is_err());
    }
    #[test]
    fn project_conversion_checks_reference_and_round_trips() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let app = crate::app::EntropyApp::new_ak820_demo(&cc);
        let mut project = app.snapshot_current_ak820_project().unwrap();
        project.macro_texts[0] = b"hello".to_vec();
        project.macro_actions[0] = vec![MacroAction::Tap(4)];
        assert!(!macros_consistent(&project));
        assert!(project_target(&project, crate::ak820::VIAL_UID, -1).is_ok());
        assert!(project_target(&project, 42, -1).is_err());
        assert!(project_target(&project, crate::ak820::VIAL_UID, 0).is_err());
        let encoded = export_project(&project, crate::ak820::VIAL_UID, -1).unwrap();
        let value: Value = serde_json::from_slice(&encoded.bytes).unwrap();
        assert!(value.get("vial_protocol").is_none());
        assert!(value.get("via_protocol").is_none());
        assert_eq!(value[ENCODING_FIELD], ENCODING_V6);
        assert_eq!(file_protocol(&value).unwrap(), Some(6));
        let mut conflict = value.clone();
        conflict["vial_protocol"] = json!(5);
        assert!(file_protocol(&conflict).is_err());
        conflict["vial_protocol"] = json!(6);
        conflict[ENCODING_FIELD] = json!("unknown");
        assert!(file_protocol(&conflict).is_err());
        assert!(encoded.warnings.iter().any(|w| w.contains("encoding declaration") && w.contains("not a verified source device")));
        assert!(value.get("macro").is_none() && value.get("combo").is_none());
        assert!(encoded.warnings.iter().any(|w| w.contains("representations differ")));
        assert!(encoded.warnings.iter().any(|w| w.contains("alternate repeat keys")));
        assert!(value.get("alt_repeat_key").is_none());
        let imported = import_project(&encoded.bytes, &project, crate::ak820::VIAL_UID, -1).unwrap();
        assert_eq!(imported.project.layout.layers, project.layout.layers);
        assert_eq!(imported.project.layout.encoder_layers, project.layout.encoder_layers);
        assert_eq!(imported.project.macro_texts, project.macro_texts);
        let mut file = value;
        file["macro"] = json!([[["text", "hello"]], [], [], [], [], [], [], [], [], [], [], [], [], [], [], []]);
        let imported = import_project(&serde_json::to_vec(&file).unwrap(), &project, crate::ak820::VIAL_UID, -1).unwrap();
        assert_eq!(imported.project.macro_texts[0], b"hello");
        assert_eq!(imported.project.macro_actions[0], vec![MacroAction::Text("hello".into())]);
        assert!(!imported.warnings.iter().any(|w| w == "Not imported: macro"));
        assert!(imported.warnings.iter().any(|w| w.contains("replaces all project macro")));
        file["macro"] = json!([[], [], [], [], [], [], [], [], [], [], [], [], [], [], [], []]);
        file["combo"] = json!([[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0]]);
        let cleared = import_project(&serde_json::to_vec(&file).unwrap(), &project, crate::ak820::VIAL_UID, -1).unwrap();
        assert!(cleared.warnings.iter().any(|w| w.contains("replaces all project macro")));
        assert!(cleared.warnings.iter().any(|w| w.contains("replaces all project combo")));
        file["combo"][0][4] = json!("LT1(MO(1))");
        let unsupported_combo = import_project(&serde_json::to_vec(&file).unwrap(), &project, crate::ak820::VIAL_UID, -1).unwrap();
        assert!(unsupported_combo.warnings.iter().any(|w| w.contains("Combo unchanged") && w.contains("truncate")));
        file["macro"][0] = json!([["tap", "KC_A"]]);
        let unsupported = import_project(&serde_json::to_vec(&file).unwrap(), &project, crate::ak820::VIAL_UID, -1).unwrap();
        assert_eq!(unsupported.project.macro_actions, project.macro_actions);
        assert!(unsupported.warnings.iter().any(|w| w == "Not imported: macro"));
        project.layout.keys[0].row = project.layout.keys[1].row;
        project.layout.keys[0].col = project.layout.keys[1].col;
        assert!(project_target(&project, crate::ak820::VIAL_UID, -1).is_err());
    }
    #[test]
    fn refuses_loss_and_invalid_geometry() {
        let t = target();
        let mut v: Value = serde_json::from_slice(FIXTURE).unwrap();
        v["macro"] = json!([[ ["text", "x"] ]]);
        let preview = import(&serde_json::to_vec(&v).unwrap(), &t).unwrap();
        assert!(preview.warnings.iter().any(|w| w.contains("macro")));
        v["layout"][0][0][1] = json!(0);
        assert!(parse(&serde_json::to_vec(&v).unwrap(), &t).is_err());
        v["uid"] = json!(1);
        assert!(parse(&serde_json::to_vec(&v).unwrap(), &t).is_err());
        assert!(import(br#"{"version":1,"version":1}"#, &t).is_err());
        assert!(keycode(&json!("NOT_A_KEY"), Some(6)).is_err());
        assert!(keycode(&json!("RGB_HUI"), None).is_err());
        assert_eq!(keycode(&json!("RGB_HUI"), Some(5)).unwrap(), 0x5cc5);
        assert_eq!(keycode(&json!("RGB_HUI"), Some(6)).unwrap(), 0x7823);
        assert_eq!(keycode(&json!("LSFT(LSFT(KC_A))"), Some(6)).unwrap(), 0x204);
        assert_eq!(keycode(&json!("0x7823"), None).unwrap(), 0x7823);
        assert!(keycode(&json!(65536), None).is_err());
    }
}
