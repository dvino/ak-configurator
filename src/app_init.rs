use super::*;

// Bootstrap effects are resolved once by `new`; both real and inert callers
// then use the same application-state initializer.
struct AppBootstrap {
    app_settings: AppSettings,
    device_manager: DeviceManager,
    typing_trainer: TypingTrainerState,
    text_expander_rules_signature: Vec<(String, Option<std::time::SystemTime>)>,
    last_single_instance_signal: String,
    layer_names: Vec<String>,
    update_check: UpdateCheckState,
}

impl EntropyApp {
    pub(crate) fn assign_offline_tap_dance(
        &mut self,
        layer: usize,
        key_index: usize,
        slot: usize,
    ) -> bool {
        use crate::app::ak820_beta1_profiles as beta1;
        let Some(profile) = self.ak820_demo_profile else {
            return false;
        };
        let matrix = beta1::matrix_profile(profile);
        let count = matrix.capabilities.dynamic_entries.tap_dance as usize;
        let Ok(slot_code) = u16::try_from(slot).map(|n| 0x5700 | n) else {
            return false;
        };
        if slot >= count
            || !beta1::supports_keycode(profile, slot_code, beta1::KeycodeSource::Matrix)
        {
            return false;
        }
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        if layer >= layout.layers.len() || key_index >= layout.keys.len() {
            return false;
        }
        let old_binding = layout.get_key_binding(layer, key_index);
        self.undo_stack.push(UndoAction::Key {
            layer,
            key_idx: key_index,
            old_binding,
        });
        if let Some(layout) = self.layout.as_mut() {
            layout.set_key_binding(
                layer,
                key_index,
                crate::keyboard::KeyBinding::Vial(slot_code),
            );
        }
        self.refresh_layer_picker_content_flags();
        true
    }

    /// Offline preview: no discovery, HID handle, typing hook or update worker.
    pub(crate) fn new_ak820_demo(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = AppSettings {
            onboarding_tour_seen_version: ONBOARDING_TOUR_VERSION,
            ..load_app_settings()
        };
        cc.egui_ctx.set_zoom_factor(settings.ui_scale);
        let mut app = Self::from_bootstrap(AppBootstrap {
            typing_trainer: TypingTrainerState::from_settings(settings.typing_trainer),
            app_settings: settings,
            device_manager: DeviceManager::empty(),
            text_expander_rules_signature: Vec::new(),
            last_single_instance_signal: String::new(),
            layer_names: Vec::new(),
            update_check: UpdateCheckState::Idle,
        });
        let definition = serde_json::from_str(include_str!("../assets/ak820/layout.json"))
            .expect("bundled AK820 layout");
        let mut layout =
            KeyboardLayout::from_vial_json(&definition).expect("bundled AK820 layout contract");
        let demo: serde_json::Value =
            serde_json::from_str(include_str!("../assets/ak820/demo-keymap.json"))
                .expect("bundled demo keymap");
        layout.layers = vec![vec![1u16.into(); layout.keys.len()]; 6];
        for (index, key) in layout.keys.iter().enumerate() {
            let slot = key.row as usize * 8 + key.col as usize;
            layout.layers[0][index] =
                (demo["base_layer"][slot].as_u64().expect("demo keycode") as u16).into();
        }
        layout.encoder_layers = vec![vec![1; layout.encoders.len()]; 6];
        for (index, encoder) in layout.encoders.iter().enumerate() {
            layout.encoder_layers[0][index] = if encoder.direction == 0 { 0xAA } else { 0xA9 };
        }
        app.ak820_demo = true;
        app.current_device_name = "AK820 · demo".to_owned();
        app.device_manager.replace_devices(vec![Device {
            name: app.current_device_name.clone(),
            vendor_id: crate::ak820::VID,
            product_id: crate::ak820::PID,
            manufacturer: "AJAZZ".to_owned(),
            serial_number: String::new(),
            bus_type: "Usb".to_owned(),
            path: String::new(),
            instance_token: String::new(),
            firmware: FirmwareProtocol::Vial,
        }]);
        app.selected_device = Some(0);
        app.layer_count = 6;
        app.layer_names = (0..6).map(|i| i.to_string()).collect();
        app.keycode_picker.ak820_profile = true;
        app.keycode_picker.layer_count = 6;
        app.keycode_picker.layer_names = app.layer_names.clone();
        app.keycode_picker.supports_mouse_keys = false;
        app.keycode_picker.supports_auto_shift = false;
        app.keycode_picker.supports_tap_dance = false;
        app.keycode_picker.supports_layer_lock = false;
        app.keycode_picker.supports_persistent_default_layer = false;
        app.keycode_picker.macro_count = 16;
        app.keycode_picker.macro_texts = vec![vec![]; 16];
        app.keycode_picker.macro_actions = vec![vec![]; 16];
        app.combo_entries = vec![ComboEntry::default(); 8];
        app.combo_synced_entries = app.combo_entries.clone();
        app.combo_term = Some(40);
        app.encoder_visibility = vec![true];
        app.ak820_demo_seed = Some(layout.clone());
        app.layout = Some(layout);
        app.status_msg = "Демонстрационный режим. Настройки устройства не считывались.".to_owned();
        app.configure_ak820_offline_profile(crate::app::ak820_beta1_profiles::ProfileId::Main);
        app
    }

    pub(crate) fn rename_ak820_offline_layer(&mut self, layer: usize, name: &str) -> bool {
        if !self.ak820_demo || layer >= self.layer_count {
            return false;
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 12 {
            return false;
        }
        self.layer_names.resize(self.layer_count, String::new());
        self.layer_names[layer] = name.to_owned();
        self.keycode_picker.layer_names = self.layer_names.clone();
        if let Some(layout) = self.layout.as_mut() {
            layout.layer_names = self.layer_names.clone();
        }
        true
    }

    pub(crate) fn configure_ak820_offline_profile(
        &mut self,
        profile: crate::app::ak820_beta1_profiles::ProfileId,
    ) {
        if !self.ak820_demo {
            return;
        }

        use crate::app::ak820_beta1_profiles as beta1;
        use crate::keycode_picker::Ak820Beta1KeycodeSource as PickerSource;

        let image = beta1::profile(profile);
        let limits = &image.fixed_limits;
        let matrix = beta1::matrix_profile(profile);
        let source_pairs = [
            (beta1::KeycodeSource::Matrix, PickerSource::Matrix),
            (beta1::KeycodeSource::Encoder, PickerSource::Encoder),
            (beta1::KeycodeSource::Macro, PickerSource::Macro),
            (beta1::KeycodeSource::MacroTap, PickerSource::MacroTap),
            (beta1::KeycodeSource::MacroDown, PickerSource::MacroDown),
            (beta1::KeycodeSource::MacroUp, PickerSource::MacroUp),
        ];
        let allowed: std::collections::BTreeMap<PickerSource, std::collections::BTreeSet<u16>> =
            source_pairs
                .into_iter()
                .map(|(contract_source, picker_source)| {
                    let codes = (u16::MIN..=u16::MAX)
                        .filter(|code| beta1::supports_keycode(profile, *code, contract_source))
                        .collect();
                    (picker_source, codes)
                })
                .collect();

        if self.ak820_demo_profile == Some(profile) {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.ak820_rgb_scene_stop();
        if let (Some(current_profile), Some(layout)) =
            (self.ak820_demo_profile, self.layout.as_ref())
        {
            let snapshot = Ak820OfflineProfileProject {
                profile: current_profile,
                layout: layout.clone(),
                macro_texts: self.keycode_picker.macro_texts.clone(),
                macro_actions: self.keycode_picker.macro_actions.clone(),
                macro_names: self.keycode_picker.macro_names.clone(),
                macro_descriptions: self.keycode_picker.macro_descriptions.clone(),
                combo_entries: self.combo_entries.clone(),
                combo_names: self.combo_names.clone(),
                combo_colors: self.combo_colors.clone(),
                combo_term: self.combo_term,
                layer_names: self.layer_names.clone(),
                rgb_settings: self.rgb_settings.clone(),
                extensions: self.ak820_offline_extensions.clone(),
            };
            if let Some(existing) = self
                .ak820_demo_projects
                .iter_mut()
                .find(|project| project.profile == current_profile)
            {
                *existing = snapshot;
            } else {
                self.ak820_demo_projects.push(snapshot);
            }
        }

        if let Some(project) = self
            .ak820_demo_projects
            .iter()
            .find(|project| project.profile == profile)
            .cloned()
        {
            self.layout = Some(project.layout);
            self.keycode_picker.macro_texts = project.macro_texts;
            self.keycode_picker.macro_actions = project.macro_actions;
            self.keycode_picker.macro_names = project.macro_names;
            self.keycode_picker.macro_descriptions = project.macro_descriptions;
            self.combo_entries = project.combo_entries;
            self.combo_names = project.combo_names;
            self.combo_colors = project.combo_colors;
            self.combo_term = project.combo_term;
            self.layer_names = project.layer_names;
            self.rgb_settings = project.rgb_settings;
            self.ak820_offline_extensions = project.extensions;
        } else {
            self.layout = self.ak820_demo_seed.clone();
            self.keycode_picker.macro_texts = vec![Vec::new(); limits.macros as usize];
            self.keycode_picker.macro_actions = vec![Vec::new(); limits.macros as usize];
            self.keycode_picker.macro_names = vec![String::new(); limits.macros as usize];
            self.keycode_picker.macro_descriptions = vec![String::new(); limits.macros as usize];
            self.combo_entries = vec![ComboEntry::default(); limits.combos as usize];
            self.combo_names = vec![String::new(); limits.combos as usize];
            self.combo_colors = vec![0; limits.combos as usize];
            self.combo_term = Some(40);
            self.layer_names = (0..limits.layers).map(|index| index.to_string()).collect();
            self.ak820_offline_extensions =
                empty_schema03_offline_extensions(image.settings_schema);
            self.rgb_settings = RgbSettingsState {
                supported: limits.rgb_leds > 0,
                kind: if limits.rgb_leds > 0 {
                    RgbSupportKind::VialRgb
                } else {
                    RgbSupportKind::None
                },
                effect: 0,
                brightness: 0,
                speed: 0,
                hue: 0,
                saturation: 0,
                max_brightness: limits.rgb_maximum_brightness,
                supported_effects: beta1::rgb_modes(profile)
                    .iter()
                    .map(|(_, wire)| *wire)
                    .chain(std::iter::once(0))
                    .collect(),
                last_enabled_effect: 2,
            };
        }

        if let Some(layout) = self.layout.as_mut() {
            let layers = limits.layers as usize;
            layout.layers.resize_with(layers, Vec::new);
            let matrix_codes = allowed
                .get(&PickerSource::Matrix)
                .expect("matrix allowlist");
            for layer in &mut layout.layers {
                layer.resize(layout.keys.len(), 0u16.into());
                for binding in layer {
                    let code = binding.vial_keycode();
                    if !matrix_codes.contains(&code) {
                        *binding = 0u16.into();
                    }
                }
            }
            layout.encoder_layers.resize_with(layers, Vec::new);
            let encoder_codes = allowed
                .get(&PickerSource::Encoder)
                .expect("encoder allowlist");
            for layer in &mut layout.encoder_layers {
                layer.resize(layout.encoders.len(), 0);
                for code in layer {
                    if !encoder_codes.contains(code) {
                        *code = 0;
                    }
                }
            }
        }

        self.layer_count = limits.layers as usize;
        self.keycode_picker.layer_count = self.layer_count;
        self.keycode_picker.layer_names = self.layer_names.clone();
        self.keycode_picker.ak820_profile = true;
        self.keycode_picker.open = false;
        self.keycode_picker.result = None;
        self.keycode_picker
            .set_ak820_beta1_source(PickerSource::Matrix);
        let enabled = |name: &str| matrix.capabilities.optional_modules.get(name) == Some(&true);
        self.keycode_picker.supports_compact_leader = enabled("leader");
        self.keycode_picker.supports_compact_dynamic_macros = enabled("dynamic-macros");
        self.keycode_picker.supports_mouse_keys = enabled("mouse-keys");
        self.keycode_picker.supports_auto_shift = enabled("auto-shift");
        self.keycode_picker.supports_rgb = limits.rgb_leds > 0;
        self.keycode_picker.supports_macro = limits.macros > 0;
        self.keycode_picker.macro_count = limits.macros as usize;
        self.keycode_picker
            .macro_texts
            .resize(limits.macros as usize, Vec::new());
        self.keycode_picker
            .macro_actions
            .resize(limits.macros as usize, Vec::new());
        self.keycode_picker
            .macro_names
            .resize(limits.macros as usize, String::new());
        self.keycode_picker
            .macro_descriptions
            .resize(limits.macros as usize, String::new());

        // These schema03 editors need AK820-specific typed codecs and remain closed.
        self.keycode_picker.supports_tap_dance = false;
        self.keycode_picker.tap_dance_entries.clear();
        self.keycode_picker.tap_dance_names.clear();
        self.combo_entries
            .resize(limits.combos as usize, ComboEntry::default());
        self.combo_names
            .resize(limits.combos as usize, String::new());
        self.combo_colors.resize(limits.combos as usize, 0);
        self.combo_synced_entries = self.combo_entries.clone();
        self.keycode_picker.supports_combo = matrix.capabilities.dynamic_entries.combos > 0;
        self.keycode_picker.reset_offline_profile_context();
        self.keycode_picker.ak820_combo_caps_word = matrix
            .settings_identity
            .as_ref()
            .is_some_and(|i| i.combo_actions.iter().any(|a| a == "caps-word-toggle"));
        self.keycode_picker.ak820_beta1_keycodes = Some(allowed);
        self.keycode_picker.ak820_beta1_profile_revision = self
            .keycode_picker
            .ak820_beta1_profile_revision
            .wrapping_add(1);
        self.undo_stack.clear();
        self.layer_clipboard = None;
        self.editing_layer = None;
        self.editing_layer_text.clear();
        self.editing_layer_layout_id = None;
        self.editing_layer_device_key = None;
        self.editing_layer_focus_requested = false;
        self.selected_layer = 0;
        self.selected_key = None;
        self.selected_encoder = None;
        self.pending_handed_swap = None;
        self.pending_middle_click_assignments.clear();
        self.pending_layout_undo = false;
        self.combo_undo_stack.clear();
        self.selected_combo = 0;
        self.combo_pick_target = None;
        self.key_override_undo_stack.clear();
        self.key_override_pick_target = None;
        self.alt_repeat_undo_stack.clear();
        self.alt_repeat_pick_target = None;
        self.selected_device = Some(0);
        self.ak820_demo_profile = Some(profile);
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app_settings = load_app_settings();
        app_settings.ui_scale = clamp_ui_scale(app_settings.ui_scale);
        crate::diagnostics::set_enabled(app_settings.diagnostics_enabled);
        cc.egui_ctx.set_zoom_factor(app_settings.ui_scale);
        crate::ui_style::set_accent(app_settings.accent_color.color());
        crate::smart_input::set_text_expander_config(
            app_settings.text_expander_enabled,
            {
                let mut rules = app_settings.text_expansion_rules.clone();
                rules.extend(load_extra_text_expansion_rules(
                    &app_settings.text_expander_rule_files,
                ));
                rules
            },
            parse_text_expander_blacklist(&app_settings.text_expander_app_blacklist),
        );
        // The AK820 editor does not start the inherited global typing hooks.
        // A firmware-aware OS helper is a separate, opt-in future module.

        let text_expander_rules_signature =
            text_expander_rules_signature(&app_settings.text_expander_rule_files);
        let mut typing_trainer = TypingTrainerState::from_settings(app_settings.typing_trainer);
        typing_trainer.set_symbol_stats(load_typing_trainer_symbol_stats());
        Self::from_bootstrap(AppBootstrap {
            app_settings,
            device_manager: DeviceManager::new(),
            typing_trainer,
            text_expander_rules_signature,
            last_single_instance_signal: read_single_instance_signal(),
            layer_names: load_layer_names("default"),
            update_check: UpdateCheckState::Idle,
        })
    }

    /// The app without a window, for `--export-layout`. Reads the same
    /// settings as `new` (language, diagnostics) but starts none of the
    /// desktop machinery: no text expander, no update check, no zoom.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn new_headless() -> Self {
        let app_settings = load_app_settings();
        crate::diagnostics::set_enabled(app_settings.diagnostics_enabled);
        let mut app = Self::from_bootstrap(AppBootstrap {
            typing_trainer: TypingTrainerState::from_settings(app_settings.typing_trainer),
            app_settings,
            device_manager: DeviceManager::new(),
            text_expander_rules_signature: Vec::new(),
            last_single_instance_signal: String::new(),
            layer_names: Vec::new(),
            update_check: UpdateCheckState::Idle,
        });
        app.headless = true;
        app
    }

    #[cfg(test)]
    pub(crate) fn new_inert_for_test() -> Self {
        let app_settings = AppSettings::default();
        Self::from_bootstrap(AppBootstrap {
            typing_trainer: TypingTrainerState::from_settings(app_settings.typing_trainer),
            app_settings,
            device_manager: DeviceManager::empty_for_test(),
            text_expander_rules_signature: Vec::new(),
            last_single_instance_signal: String::new(),
            layer_names: Vec::new(),
            update_check: UpdateCheckState::Idle,
        })
    }

    fn from_bootstrap(bootstrap: AppBootstrap) -> Self {
        let AppBootstrap {
            app_settings,
            device_manager,
            typing_trainer,
            text_expander_rules_signature,
            last_single_instance_signal,
            layer_names,
            update_check,
        } = bootstrap;
        let dark_mode = app_settings.dark_mode;
        let mut keycode_picker = KeycodePicker::default();
        keycode_picker.language = app_settings.language;

        Self {
            #[cfg(not(target_arch = "wasm32"))]
            hid_device: None,
            #[cfg(not(target_arch = "wasm32"))]
            ak820_os_indicator_runtime: super::ak820_os_indicator::Runtime::default(),
            #[cfg(not(target_arch = "wasm32"))]
            ak820_rgb_runtime: super::ak820_rgb_runtime::Runtime::default(),
            #[cfg(not(target_arch = "wasm32"))]
            ak820_install_runtime: super::ak820_install::Runtime::default(),
            #[cfg(not(target_arch = "wasm32"))]
            shared_hid_output: None,
            #[cfg(not(target_arch = "wasm32"))]
            layer_write_task: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_layer_write: None,
            qmk_settings_write_queue: QmkSettingsWriteQueue::default(),
            #[cfg(not(target_arch = "wasm32"))]
            combo_write_task: None,
            settings_write_task: None,
            #[cfg(not(target_arch = "wasm32"))]
            vial_hid_task: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_layout_undo: false,
            #[cfg(not(target_arch = "wasm32"))]
            deferred_device_load: DeferredDeviceLoadState::default(),
            #[cfg(not(target_arch = "wasm32"))]
            deferred_full_layout_action: None,
            settings_write_queue: SettingsWriteQueueState::default(),
            settings_write_generation: 0,
            #[cfg(not(target_arch = "wasm32"))]
            qmk_hid_hosts: std::collections::HashMap::new(),
            application_layout_editor_active: false,
            #[cfg(not(target_arch = "wasm32"))]
            application_discovery: Default::default(),
            #[cfg(target_os = "linux")]
            gnome_integration_install_task: None,
            #[cfg(target_os = "linux")]
            gnome_integration_install_result: None,
            #[cfg(not(target_arch = "wasm32"))]
            application_layout_foreground: None,
            #[cfg(not(target_arch = "wasm32"))]
            application_layout_manual_override: None,
            #[cfg(not(target_arch = "wasm32"))]
            default_layout_sync_retry_after: None,
            #[cfg(not(target_arch = "wasm32"))]
            default_layout_device_reconciled: false,
            #[cfg(not(target_arch = "wasm32"))]
            default_layout_pending_layers: 0,
            application_picker_open: false,
            application_picker_assign_existing: false,
            application_picker_target_layout_id: None,
            application_picker_search: String::new(),
            application_picker_selected: None,
            application_picker_name: String::new(),
            application_picker_category:
                crate::application_layouts::ApplicationLayoutCategory::Other,
            application_picker_category_changed: false,
            application_picker_custom_category_id: None,
            application_categories_open: false,
            application_categories_selected_id: None,
            application_categories_name: String::new(),
            pending_device_connect: None,
            #[cfg(not(target_arch = "wasm32"))]
            headless: false,
            ak820_demo: false,
            ak820_demo_profile: None,
            ak820_demo_seed: None,
            ak820_demo_projects: Vec::new(),
            ak820_offline_import_undo: None,
            ak820_offline_extensions: None,
            firmware: FirmwareProtocol::Vial,
            #[cfg(not(target_arch = "wasm32"))]
            supported_qmk_settings: Vec::new(),
            undo_stack: Vec::new(),
            layer_clipboard: None,
            scan_frame: 0,
            last_device_scan_at: 0.0,
            last_full_device_scan_at: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            next_battery_refresh_at: None,
            hover_layer: None,
            last_layout_geometry: None,
            prev_hovered_key: None,
            prev_hovered_encoder: false,
            prev_hovered_encoder_keycode: None,
            secondary_click_handled: false,
            pending_handed_swap: None,
            pending_middle_click_assignments: Vec::new(),
            hover_layer_progress: 0.0,
            jump_back_stack: Vec::new(),
            device_manager,
            selected_device: None,
            selected_layer: 0,
            selected_key: None,
            selected_encoder: None,
            layout: None,
            layer_count: 4,
            keycode_picker,
            status_msg: String::new(),
            import_report_open: false,
            import_report_title: String::new(),
            import_report_body: String::new(),
            pending_entlayout_import_path: None,
            pending_file_dialog: None,
            connection_generation: 0,
            parent_window_handle: None,
            parent_display_handle: None,
            pending_entsettings_import_path: None,
            #[cfg(target_os = "linux")]
            ibus_registration: Default::default(),
            #[cfg(target_os = "linux")]
            pending_ibus_reload: None,
            import_progress_started_at: None,
            import_progress_title: String::new(),
            import_progress_body: String::new(),
            #[cfg(target_os = "linux")]
            linux_setup_task: None,
            dark_mode,
            last_applied_theme: None,
            app_settings,
            text_expander_rules_signature,
            text_expander_rules_last_check_at: 0.0,
            text_expander_settings_save_pending: false,
            text_expander_settings_last_edit_at: 0.0,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            tray_icon: None,
            #[cfg(target_os = "windows")]
            windows_hwnd: None,
            #[cfg(target_os = "windows")]
            windows_window_hidden_to_tray: false,
            #[cfg(target_os = "windows")]
            windows_start_hidden_to_tray_pending: false,
            #[cfg(target_os = "macos")]
            macos_ns_window: None,
            #[cfg(target_os = "macos")]
            macos_window_hidden_to_menu_bar: false,
            #[cfg(target_os = "macos")]
            macos_app_was_hidden: false,
            #[cfg(target_os = "macos")]
            macos_hidden_to_menu_bar_at: None,
            close_to_tray_prompt_open: false,
            close_to_tray_prompt_remember: false,
            force_close_requested: false,
            #[cfg(not(target_arch = "wasm32"))]
            exit_after_hid_write: false,
            main_menu_tab: MainMenuTab::Keyboard,
            combo_entries: vec![],
            combo_synced_entries: vec![],
            supports_rmk_combo_layers: false,
            combo_names: vec![],
            combo_colors: vec![],
            selected_combo: 0,
            combo_dirty: false,
            combo_edit_revision: 0,
            combo_attempted_revision: None,
            combo_names_dirty: false,
            combo_colors_dirty: false,
            combo_term: None,
            auto_shift_options: AutoShiftOptionsState::default(),
            auto_shift_timeout: None,
            auto_shift_timeout_text: String::new(),
            mouse_keys_settings: MouseKeysSettingsState::default(),
            touchpad_settings: TouchpadSettingsState::default(),
            bluetooth_settings: BluetoothSettingsState::default(),
            module_settings: ModuleSettingsState::default(),
            tap_hold_settings: TapHoldSettingsState::default(),
            pending_tap_hold_numeric_writes: std::collections::BTreeMap::new(),
            tap_hold_numeric_write_due: None,
            magic_settings: MagicSettingsState::default(),
            one_shot_settings: OneShotSettingsState::default(),
            grave_escape_settings: GraveEscapeSettingsState::default(),
            layer_led_settings: LayerLedSettingsState::default(),
            alt_repeat_entries: vec![],
            alt_repeat_names: vec![],
            alt_repeat_undo_stack: Vec::new(),
            selected_alt_repeat: 0,
            alt_repeat_visible_count: 1,
            alt_repeat_pick_target: None,
            last_single_instance_signal,
            rgb_settings: RgbSettingsState::default(),
            display_settings: DisplaySettingsState::default(),
            layout_options_value: None,
            encoder_visibility: vec![],
            combo_term_dirty: false,
            combo_visible_count: 1,
            combo_undo_stack: Vec::new(),
            combo_pick_target: None,
            key_override_entries: Vec::new(),
            key_override_names: vec![],
            key_override_dirty: false,
            key_override_visible_count: 1,
            key_override_undo_stack: Vec::new(),
            text_expander_deleted_rules: Vec::new(),
            text_expander_rule_editor: None,
            text_expander_rule_editor_focus_pending: false,
            text_expander_emoji_search: String::new(),
            text_expander_emoji_group: 0,
            text_expander_emoji_target: None,
            typing_trainer,
            typing_trainer_symbol_pool_source: None,
            typing_trainer_history_open: false,
            selected_key_override: 0,
            key_override_pick_target: None,
            matrix_tester_pressed: Vec::new(),
            matrix_tester_ever_pressed: Vec::new(),
            sticky_layout_prev_pressed: Vec::new(),
            sticky_layout_pressed_key_layers: Vec::new(),
            sticky_layout_toggled_layers: Vec::new(),
            sticky_layout_active_combos: Vec::new(),
            sticky_layout_tap_dance_states: Vec::new(),
            sticky_layout_base_layer: 0,
            sticky_layout_active_layer: 0,
            sticky_layout_last_size: None,
            sticky_layout_resize_opacity_hold_frames: 0,
            sticky_layout_viewport_events: StickyLayoutViewportEventQueue::default(),
            pending_layout_indicator_open_after_unlock: false,
            matrix_tester_last_poll: std::time::Instant::now(),
            matrix_tester_last_lock_check: std::time::Instant::now()
                - MATRIX_TESTER_LOCK_CHECK_INTERVAL,
            matrix_tester_unlock_prompted: false,
            matrix_tester_lock_checked: false,
            macro_auto_unlock_cancelled: false,
            settings_tab: SettingsTab::MatrixTester,
            layer_names,
            editing_layer: None,
            editing_layer_text: String::new(),
            editing_layer_focus_requested: false,
            editing_layer_layout_id: None,
            editing_layer_device_key: None,
            #[cfg(not(target_arch = "wasm32"))]
            editing_layer_generation: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_layer_name_writes: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            uncertain_layer_names: Vec::new(),
            current_device_name: String::new(),
            current_keyboard_id: None,
            current_encoder_visibility_id: String::new(),
            editing_layout_visibility: false,
            device_display_names: std::collections::HashMap::new(),
            device_about_info: None,
            update_check,
            #[cfg(not(target_arch = "wasm32"))]
            app_update_runtime: Default::default(),
            firmware_update_check: FirmwareUpdateCheckState::Unsupported,
            tour_state: TourState::default(),
            tour_target_rects: Vec::new(),
            unlock_open: false,
            vial_unlocked: None,
            vial_unlock_keys: vec![],
            vial_unlock_session_started: false,
            vial_unlock_polling: false,
            vial_unlock_counter: 0,
            vial_unlock_best: 50,
            vial_unlock_total: 50,
            vial_unlock_last_poll: None,
            vial_unlock_animation_nonce: 0,
            #[cfg(not(target_arch = "wasm32"))]
            connect_state: ConnectState::Idle,
            #[cfg(not(target_arch = "wasm32"))]
            retiring_connects: Vec::new(),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            test_device_scan_requests: None,
            #[cfg(all(test, not(target_arch = "wasm32")))]
            test_connect_requests: None,
            #[cfg(all(test, not(target_arch = "wasm32")))]
            test_connect_hid: None,
            #[cfg(not(target_arch = "wasm32"))]
            device_scan_state: DeviceScanState::Idle,
        }
    }

    /// Recompute which layers contain assignable keycodes.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn refresh_layer_picker_content_flags(&mut self) {
        if let Some(layout) = &self.layout {
            self.keycode_picker.layer_has_content = layout
                .layers
                .iter()
                .map(|keys| {
                    keys.iter().any(|binding| {
                        let kc = binding.vial_keycode();
                        binding.rmk_action().is_some() || (kc != 0x0000 && kc != 0x0001)
                    })
                })
                .collect();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn is_vial_locked(&self) -> bool {
        !self.ak820_demo
            && self.firmware == FirmwareProtocol::Vial
            && self.layout.is_some()
            && !self.vial_unlock_polling
            && self.vial_unlocked != Some(true)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn is_vial_locked(&self) -> bool {
        false
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn reopen_vial_hid(&mut self) {
        let Some(dev) = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
        else {
            self.hid_device = None;
            self.shared_hid_output = None;
            return;
        };
        match crate::hid::HidDevice::open_fresh_for(dev) {
            Ok(hid) => {
                self.shared_hid_output = hid.shared_output();
                self.hid_device = Some(hid);
            }
            Err(e) => {
                self.hid_device = None;
                self.shared_hid_output = None;
                self.status_msg = crate::i18n::tr_catalog_format(
                    self.app_settings.language,
                    "status_messages.hid_reopen_failed",
                    &[("error", &e.to_string())],
                );
            }
        }
    }
}

fn empty_schema03_offline_extensions(
    settings_schema: u32,
) -> Option<crate::app::ak820_offline_extensions::OfflineExtensions> {
    if settings_schema != crate::ak820_typed_storage::SCHEMA {
        return None;
    }
    let mut nvm = vec![0xA5; 0x2800];
    nvm[37..40].copy_from_slice(&[3, 0x20, 8]);
    let settings = crate::ak820_typed_storage::QmkSettings::default().encode();
    nvm[0x6A8..0x6A8 + settings.len()].copy_from_slice(&settings);
    for index in 0..8 {
        for tap_dance in [true, false] {
            let offset = crate::ak820_typed_storage::slot_offset(tap_dance, index)
                .expect("curated schema03 slot is in range");
            nvm[offset..offset + 10].fill(0);
        }
    }
    crate::app::ak820_offline_extensions::OfflineExtensions::from_nvm(&nvm).ok()
}
