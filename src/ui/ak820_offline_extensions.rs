//! In-memory editors for schema03 examples. No device or transport access.
use crate::ak820_typed_storage::{Extensions, KeyOverride, TapDance};
use crate::app::ak820_beta1_profiles::ProfileId;
use crate::i18n::Language;

#[derive(Clone, Debug)]
pub(crate) struct OfflineExtensions {
    nvm: Vec<u8>,
    draft: Extensions,
    invalid: bool,
}
impl OfflineExtensions {
    pub(crate) fn from_nvm(nvm: &[u8]) -> Result<Self, String> {
        Ok(Self {
            nvm: nvm.to_vec(),
            draft: Extensions::decode_nvm(nvm)?,
            invalid: false,
        })
    }
    pub(crate) fn has_invalid_draft(&self) -> bool {
        self.invalid
    }

    pub(crate) fn nvm(&self) -> &[u8] {
        &self.nvm
    }
    /// Validate the complete draft before replacing the in-memory candidate.
    fn apply(&mut self, profile: ProfileId) -> bool {
        match self.draft.apply_to_nvm(&self.nvm, profile) {
            Ok(candidate) => {
                let changed = candidate != self.nvm;
                self.nvm = candidate;
                self.invalid = false;
                changed
            }
            Err(_) => {
                self.invalid = true;
                false
            }
        }
    }
    fn discard(&mut self) {
        // Only validated candidates enter nvm.
        self.draft = Extensions::decode_nvm(&self.nvm).expect("validated offline NVM");
        self.invalid = false;
    }
}
fn text<'a>(language: Language, en: &'a str, ru: &'a str) -> &'a str {
    if language == Language::Russian {
        ru
    } else {
        en
    }
}
fn number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u16,
    range: std::ops::RangeInclusive<u16>,
    hex: bool,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut control = egui::DragValue::new(value).range(range).speed(1);
        if hex {
            control = control.hexadecimal(4, false, true);
        }
        ui.add(control).changed()
    })
    .inner
}
fn mask(ui: &mut egui::Ui, label: &str, value: &mut u8) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(value)
                .range(0..=255)
                .hexadecimal(2, false, true),
        )
        .changed()
    })
    .inner
}
/// Returns true only when the validated in-memory NVM changed.
pub(crate) fn show(
    ui: &mut egui::Ui,
    language: Language,
    profile: ProfileId,
    state: &mut OfflineExtensions,
) -> bool {
    if !matches!(
        profile,
        ProfileId::TapDance | ProfileId::KeyOverrides | ProfileId::Settings
    ) {
        return false;
    }
    ui.heading(text(
        language,
        "Offline module settings",
        "Настройки модулей офлайн",
    ));
    ui.label(text(language,
        "Changes affect this example project in memory. Live module reads, writes and physical behavior are not verified. Pending Tap Dance actions and the active override are volatile runtime state; they are not saved here.",
        "Изменения действуют в памяти этого примера. Чтение и запись модулей на клавиатуре и физическое поведение не проверены. Ожидающие действия Tap Dance и активная замена — временное состояние прошивки, здесь оно не сохраняется."));
    let mut edited = false;
    match profile {
        ProfileId::TapDance => {
            ui.label(text(language,
                "Assign TD(0)…TD(7) on a key separately. Outputs use hexadecimal keycodes: 0000 uses the firmware fallback; otherwise below 2000, low byte 04…A4 or E0…E7. Term: 0 uses 200 ms; 1…32767 ms is explicit. Zero hold falls back to tap; zero double tap repeats tap; zero tap-then-hold uses hold or tap. Invalid combinations keep the previous candidate.",
                "Назначьте TD(0)…TD(7) на клавишу отдельно. Выходы — шестнадцатеричные коды: 0000 использует резервный выход прошивки; остальные ниже 2000 с младшим байтом 04…A4 или E0…E7. Интервал 0 означает 200 мс; явное значение — 1…32767 мс. Нулевое удержание использует одиночное нажатие; нулевое двойное — повтор одиночного; нулевое нажатие с удержанием — удержание или одиночное. При ошибке прежний вариант сохраняется."));
            for (i, row) in state.draft.tap_dance.iter_mut().enumerate() {
                egui::CollapsingHeader::new(format!("TD({i})"))
                    .id_salt(("offline-td", i))
                    .show(ui, |ui| {
                        for (label, value) in [
                            text(language, "Single tap", "Одиночное нажатие"),
                            text(language, "Hold", "Удержание"),
                            text(language, "Double tap", "Двойное нажатие"),
                            text(language, "Tap then hold", "Нажатие и удержание"),
                        ]
                        .into_iter()
                        .zip(row.outputs.iter_mut())
                        {
                            edited |= number(ui, label, value, 0..=u16::MAX, true);
                        }
                        edited |= number(
                            ui,
                            text(language, "Term (ms)", "Интервал (мс)"),
                            &mut row.term,
                            0..=32767,
                            false,
                        );
                        if ui
                            .button(text(language, "Clear row", "Очистить строку"))
                            .clicked()
                        {
                            *row = TapDance::default();
                            edited = true;
                        }
                    });
            }
        }
        ProfileId::KeyOverrides => {
            ui.label(text(language,
                "All-zero rows are empty. Other rows require trigger 04…A4, replacement 0000 or below 2000 with low byte 04…A4, layers 01…3F. Modifier masks: 01 left Ctrl, 02 left Shift, 04 left Alt, 08 left GUI; 10/20/40/80 are their right counterparts. Layer bits 01/02/04/08/10/20 select layers 0…5. Options: 01 trigger-down, 02 required-mod-down, 04 negative-mod-up, 08 one required mod suffices, 10 no re-registration, 20 no unregister on another key, 80 enabled. At least one of 01/02/04 is required; 40 is forbidden. Hexadecimal fields are validated together.",
                "Нулевая строка пуста. Иначе: исходная клавиша 04…A4, замена 0000 или ниже 2000 с младшим байтом 04…A4, слои 01…3F. Маски модификаторов: 01 левый Ctrl, 02 левый Shift, 04 левый Alt, 08 левый GUI; 10/20/40/80 — соответствующие правые. Биты слоёв 01/02/04/08/10/20 выбирают слои 0…5. Флаги: 01 нажатие клавиши, 02 нажатие нужного модификатора, 04 отпускание запрещённого модификатора, 08 достаточно одного нужного модификатора, 10 без повторной регистрации, 20 без отмены другой клавишей, 80 включено. Нужен хотя бы один из 01/02/04; 40 запрещён. Шестнадцатеричные поля проверяются вместе."));
            for (i, row) in state.draft.key_overrides.iter_mut().enumerate() {
                egui::CollapsingHeader::new(format!(
                    "{} {i}",
                    text(language, "Override", "Замена")
                ))
                .id_salt(("offline-ko", i))
                .show(ui, |ui| {
                    edited |= number(
                        ui,
                        text(language, "Trigger", "Исходная клавиша"),
                        &mut row.trigger,
                        0..=u16::MAX,
                        true,
                    );
                    edited |= number(
                        ui,
                        text(language, "Replacement", "Замена"),
                        &mut row.replacement,
                        0..=u16::MAX,
                        true,
                    );
                    edited |= number(
                        ui,
                        text(language, "Layer mask", "Маска слоёв"),
                        &mut row.layers,
                        0..=63,
                        true,
                    );
                    edited |= mask(
                        ui,
                        text(language, "Required modifiers", "Нужные модификаторы"),
                        &mut row.mods,
                    );
                    edited |= mask(
                        ui,
                        text(language, "Negative modifiers", "Запрещённые модификаторы"),
                        &mut row.negative,
                    );
                    edited |= mask(
                        ui,
                        text(language, "Suppressed modifiers", "Подавляемые модификаторы"),
                        &mut row.suppressed,
                    );
                    edited |= mask(ui, text(language, "Options", "Флаги"), &mut row.options);
                    if ui
                        .button(text(language, "Clear row", "Очистить строку"))
                        .clicked()
                    {
                        *row = KeyOverride::default();
                        edited = true;
                    }
                });
            }
        }
        ProfileId::Settings => {
            ui.label(text(language,
                "Only active QSIDs are editable. Auto Shift timeout (QSID 4) is inactive and preserved. Grave Escape mask: 01 Alt, 02 Ctrl, 04 GUI, 08 Shift. A blank default record remains byte-exact until an effective setting changes.",
                "Доступны только активные QSID. Интервал Auto Shift (QSID 4) неактивен и сохраняется. Маска Grave Escape: 01 Alt, 02 Ctrl, 04 GUI, 08 Shift. Пустая запись значений по умолчанию сохраняется побайтно до изменения действующего значения."));
            for (id, en, ru, low, high) in [
                (1, "Grave Escape flags", "Флаги Grave Escape", 0, 15),
                (2, "Combo term (ms)", "Интервал Combo (мс)", 1, 1000),
                (7, "Tapping term (ms)", "Интервал нажатия (мс)", 1, 1000),
                (18, "Tap delay (ms)", "Задержка нажатия (мс)", 0, 100),
                (
                    19,
                    "Caps Lock delay (ms)",
                    "Задержка Caps Lock (мс)",
                    0,
                    100,
                ),
            ] {
                let mut value = state.draft.settings.get(id).expect("known QSID");
                if number(ui, text(language, en, ru), &mut value, low..=high, id == 1) {
                    if state.draft.settings.set(id, value).is_ok() {
                        edited = true;
                    }
                }
            }
        }
        _ => {}
    }
    let changed = edited && state.apply(profile);
    if state.invalid {
        ui.colored_label(egui::Color32::RED,text(language,"Invalid draft: check the ranges and masks above. The previous NVM candidate is unchanged.","Ошибка: проверьте диапазоны и маски выше. Прежний вариант NVM сохранён."));
        if ui
            .button(text(
                language,
                "Discard invalid draft",
                "Отменить ошибочные изменения",
            ))
            .clicked()
        {
            state.discard();
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(blank: u8) -> OfflineExtensions {
        let mut raw = vec![0; 0x2800];
        raw[37..40].copy_from_slice(&[3, 0x20, 8]);
        raw[0x6a8..0x6c0].fill(blank);
        OfflineExtensions::from_nvm(&raw).unwrap()
    }
    #[test]
    fn invalid_edit_retains_candidate_and_can_be_corrected_or_discarded() {
        let mut state = fixture(0);
        let before = state.nvm().to_vec();
        state.draft.tap_dance[0].outputs[0] = 1;
        assert!(!state.apply(ProfileId::TapDance));
        assert!(state.invalid);
        assert_eq!(state.nvm(), before);
        state.discard();
        assert!(!state.invalid);
        assert_eq!(state.draft.tap_dance[0], TapDance::default());
        state.draft.tap_dance[0].outputs[0] = 4;
        assert!(state.apply(ProfileId::TapDance));
        assert_eq!(
            Extensions::decode_nvm(state.nvm()).unwrap().tap_dance[0].outputs[0],
            4
        );
    }
    #[test]
    fn per_profile_gate_and_blank_noop() {
        for blank in [0, 255] {
            let mut state = fixture(blank);
            let before = state.nvm().to_vec();
            for profile in [
                ProfileId::TapDance,
                ProfileId::KeyOverrides,
                ProfileId::Settings,
            ] {
                assert!(!state.apply(profile));
                assert!(!state.invalid);
                assert_eq!(state.nvm(), before);
            }
            state.draft.settings.set(2, 41).unwrap();
            assert!(!state.apply(ProfileId::TapDance));
            assert_eq!(state.nvm(), before);
            assert!(state.apply(ProfileId::Settings));
            assert_eq!(&state.nvm()[..0x6a8], &before[..0x6a8]);
            assert_eq!(&state.nvm()[0x6c0..], &before[0x6c0..]);
            state.draft.settings.set(4, 176).unwrap();
            let current = state.nvm().to_vec();
            assert!(!state.apply(ProfileId::Settings));
            assert_eq!(state.nvm(), current);
        }
    }
    #[test]
    fn key_override_partial_draft_requires_complete_valid_row() {
        let mut state = fixture(0);
        let before = state.nvm().to_vec();
        state.draft.key_overrides[7].trigger = 4;
        assert!(!state.apply(ProfileId::KeyOverrides));
        assert_eq!(state.nvm(), before);
        state.draft.key_overrides[7].layers = 1;
        state.draft.key_overrides[7].options = 0x81;
        assert!(!state.apply(ProfileId::Settings));
        assert_eq!(state.nvm(), before);
        assert!(state.apply(ProfileId::KeyOverrides));
        assert_eq!(&state.nvm()[..0x78a], &before[..0x78a]);
        assert_eq!(&state.nvm()[0x794..], &before[0x794..]);
    }
}
