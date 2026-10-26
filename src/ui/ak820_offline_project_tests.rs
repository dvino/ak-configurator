//! Actual inert application-state regression tests; no discovery or HID seam.
use super::*;
use crate::ak820_typed_storage::{Extensions, KeyOverride, TapDance};
use crate::app::ak820_beta1_profiles::{self as beta1, ProfileId};
use crate::app::ak820_offline_extensions::OfflineExtensions;
use crate::keycode_picker::{Ak820Beta1KeycodeSource, MacroAction};

const PROFILES: [ProfileId; 10] = [
    ProfileId::Main, ProfileId::Game, ProfileId::Mmo, ProfileId::Typing, ProfileId::Code,
    ProfileId::Pointer, ProfileId::Onehand, ProfileId::Creative, ProfileId::Shortcuts, ProfileId::Multios,
];
fn app() -> (EntropyApp, egui::Context) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    (EntropyApp::new_ak820_demo(&cc), ctx)
}
fn assert_inert(app: &EntropyApp) {
    assert!(app.ak820_demo);
    assert!(app.hid_device.is_none());
    assert!(app.shared_hid_output.is_none());
    assert!(app.test_connect_hid.is_none());
    assert!(app.test_connect_requests.is_none());
    assert!(app.test_device_scan_requests.is_none());
    assert!(matches!(app.device_scan_state, DeviceScanState::Idle));
    assert!(matches!(app.connect_state, ConnectState::Idle));
    assert_eq!(app.device_manager.devices().len(), 1);
    assert!(app.device_manager.devices()[0].path.is_empty());
}
fn mark_extensions(app: &mut EntropyApp, profile: ProfileId) -> Option<Vec<u8>> {
    let old = app.ak820_offline_extensions.as_ref()?;
    let mut model = Extensions::decode_nvm(old.nvm()).unwrap();
    let matrix = beta1::matrix_profile(profile);
    if matrix.capabilities.dynamic_entries.tap_dance > 0 {
        model.tap_dance[7] = TapDance { outputs: [4, 5, 6, 7], term: 123 };
    }
    if matrix.capabilities.dynamic_entries.key_overrides > 0 {
        model.key_overrides[7] = KeyOverride { trigger: 4, replacement: 5, layers: 1, mods: 2, negative: 4, suppressed: 8, options: 0x81 };
    }
    if matrix.capabilities.runtime_qmk_settings_ids.contains(&2) {
        model.settings.set(2, 73).unwrap();
    }
    let candidate = model.apply_to_nvm(old.nvm(), profile).unwrap();
    app.ak820_offline_extensions = Some(OfflineExtensions::from_nvm(&candidate).unwrap());
    Some(candidate)
}
#[test]
fn ten_real_offline_projects_retain_independent_edits() {
    let (mut app, ctx) = app();
    assert_inert(&app);
    let mut extensions = Vec::new();
    for (i, profile) in PROFILES.into_iter().enumerate() {
        app.configure_ak820_offline_profile(profile);
        assert_eq!(app.ak820_demo_profile, Some(profile));
        assert_eq!(app.layer_count, 6);
        assert_eq!(app.layout.as_ref().unwrap().keys.len(), 83);
        assert_eq!(app.combo_entries.len(), 8);
        assert_eq!(app.keycode_picker.macro_texts.len(), 16);
        assert_eq!(
            app.ak820_offline_extensions.is_some(),
            beta1::profile(profile).settings_schema == 0x082003
        );
        let code = 4 + i as u16;
        assert!(app.assign_key_binding(&ctx, 0, 0, code.into()));
        app.layout.as_mut().unwrap().encoder_layers[0][0] = code;
        app.keycode_picker.macro_texts[0] = format!("example-{i}").into_bytes();
        app.keycode_picker.macro_actions[0] = vec![MacroAction::Tap(code)];
        app.keycode_picker.macro_names[0] = format!("M{i}");
        app.keycode_picker.macro_descriptions[0] = format!("description {i}");
        app.combo_entries[0] = ComboEntry {
            keys: [4, 5, 0, 0],
            output: code.into(),
            layer: None,
        };
        app.combo_names[0] = format!("Combo {i}");
        app.combo_colors[0] = 0x112200 + i as u32;
        assert!(app.rename_ak820_offline_layer(0, &format!("Layer {i}")));
        assert!(
            app.rgb_settings.supported,
            "RGB offline control missing for {}",
            profile.as_str()
        );
        app.rgb_settings.brightness = 80 + i as u8;
        app.rgb_settings.hue = 100 + i as u8;
        extensions.push(mark_extensions(&mut app, profile));
        assert_inert(&app);
    }
    // Reverse order also checks that visiting a cached project does not reseed it.
    for (i, profile) in PROFILES.into_iter().enumerate().rev() {
        app.configure_ak820_offline_profile(profile);
        let code = 4 + i as u16;
        assert_eq!(
            app.layout.as_ref().unwrap().layers[0][0].vial_keycode(),
            code
        );
        assert_eq!(app.layout.as_ref().unwrap().encoder_layers[0][0], code);
        assert_eq!(
            app.keycode_picker.macro_texts[0],
            format!("example-{i}").as_bytes()
        );
        assert_eq!(
            app.keycode_picker.macro_actions[0],
            vec![MacroAction::Tap(code)]
        );
        assert_eq!(app.keycode_picker.macro_names[0], format!("M{i}"));
        assert_eq!(
            app.keycode_picker.macro_descriptions[0],
            format!("description {i}")
        );
        assert_eq!(app.combo_entries[0].output.vial_keycode(), code);
        assert_eq!(app.combo_entries[0].keys, [4, 5, 0, 0]);
        assert_eq!(app.combo_names[0], format!("Combo {i}"));
        assert_eq!(app.combo_colors[0], 0x112200 + i as u32);
        assert_eq!(app.layer_names[0], format!("Layer {i}"));
        assert_eq!(app.keycode_picker.layer_names, app.layer_names);
        assert_eq!(app.rgb_settings.brightness, 80 + i as u8);
        assert_eq!(app.rgb_settings.hue, 100 + i as u8);
        assert_eq!(
            app.ak820_offline_extensions
                .as_ref()
                .map(|s| s.nvm().to_vec()),
            extensions[i]
        );
        assert_inert(&app);
    }
}
#[test]
fn switching_projects_clears_pending_picker_and_undo_before_they_can_apply() {
    let (mut app, ctx) = app();
    assert!(app.assign_key_binding(&ctx, 0, 0, 4u16.into()));
    assert!(!app.undo_stack.is_empty());
    app.pending_layout_undo = true;
    app.selected_key = Some((0, 0));
    app.keycode_picker.result = Some(5u16.into());
    app.keycode_picker.open = true;
    app.keycode_picker.constrain_ak820_combo_picker(false);
    assert!(!app.keycode_picker.picker_value_supported(0x5200));
    app.keycode_picker.vial_layer_pending = Some(0x5200);
    app.configure_ak820_offline_profile(ProfileId::Game);
    let target = app.layout.as_ref().unwrap().layers[0][0];
    assert!(app.undo_stack.is_empty());
    assert!(!app.pending_layout_undo);
    assert!(app.keycode_picker.result.is_none());
    assert!(!app.keycode_picker.open);
    assert!(app.keycode_picker.vial_layer_pending.is_none());
    assert!(
        app.keycode_picker.picker_value_supported(0x5200),
        "ordinary key policy must replace old Combo policy"
    );
    assert!(app.selected_key.is_none());
    assert!(app.selected_encoder.is_none());
    app.apply_picker_results(&ctx);
    app.maybe_start_pending_layout_undo(&ctx);
    app.undo(&ctx);
    assert_eq!(app.layout.as_ref().unwrap().layers[0][0], target);
    app.configure_ak820_offline_profile(ProfileId::Main);
    assert_eq!(app.layout.as_ref().unwrap().layers[0][0].vial_keycode(), 4);
    assert_inert(&app);
}
#[test]
fn tap_dance_assignments_follow_actual_profile_and_source_without_generic_editor() {
    let (mut app, ctx) = app();
    for profile in PROFILES {
        app.configure_ak820_offline_profile(profile);
        assert!(
            !app.keycode_picker.supports_tap_dance,
            "generic typed-incompatible editor must stay closed"
        );
        app.keycode_picker
            .set_ak820_beta1_source(Ak820Beta1KeycodeSource::Matrix);
        for code in 0x5700..=0x5707 {
            assert_eq!(
                app.keycode_picker.picker_value_supported(code),
                beta1::supports_keycode(profile, code, beta1::KeycodeSource::Matrix)
            );
        }
        assert!(!app.keycode_picker.picker_value_supported(0x5708));
        app.keycode_picker
            .set_ak820_beta1_source(Ak820Beta1KeycodeSource::Encoder);
        assert!(!app.keycode_picker.picker_value_supported(0x5700));
        app.keycode_picker
            .set_ak820_beta1_source(Ak820Beta1KeycodeSource::Matrix);
    }
    let td_profile = PROFILES.into_iter().find(|p| beta1::matrix_profile(*p).capabilities.dynamic_entries.tap_dance > 0).unwrap();
    app.configure_ak820_offline_profile(td_profile);
    app.selected_key = Some((0, 0));
    app.keycode_picker.result = Some(0x5707u16.into());
    app.apply_picker_results(&ctx);
    assert_eq!(
        app.layout.as_ref().unwrap().layers[0][0].vial_keycode(),
        0x5707
    );
    app.configure_ak820_offline_profile(ProfileId::Main);
    assert_ne!(
        app.layout.as_ref().unwrap().layers[0][0].vial_keycode(),
        0x5707
    );
    app.configure_ak820_offline_profile(td_profile);
    assert_eq!(
        app.layout.as_ref().unwrap().layers[0][0].vial_keycode(),
        0x5707
    );
    assert_inert(&app);
}

#[test]
fn tap_dance_button_seam_checks_profile_slot_and_target_bounds() {
    let (mut app, _) = app();
    assert!(!app.assign_offline_tap_dance(0, 0, 0));
    let td_profile = PROFILES.into_iter().find(|p| beta1::matrix_profile(*p).capabilities.dynamic_entries.tap_dance > 0).unwrap();
    app.configure_ak820_offline_profile(td_profile);
    let before = app.layout.as_ref().unwrap().layers.clone();
    for (layer, key, slot) in [
        (0, 0, 8),
        (6, 0, 0),
        (0, 83, 0),
        (usize::MAX, 0, 0),
        (0, 0, usize::MAX),
    ] {
        assert!(!app.assign_offline_tap_dance(layer, key, slot));
    }
    assert_eq!(app.layout.as_ref().unwrap().layers, before);
    assert!(app.assign_offline_tap_dance(5, 82, 7));
    assert_eq!(
        app.layout.as_ref().unwrap().layers[5][82].vial_keycode(),
        0x5707
    );
    app.configure_ak820_offline_profile(ProfileId::Game);
    assert!(!app.assign_offline_tap_dance(0, 0, 0));
    assert_inert(&app);
}
