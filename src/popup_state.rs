use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupKey {
    Picker,
    MacroKeyPick,
    RegularKeyPick,
    PickLayer,
    PendingKeyPick,
    TdKeyPick,
    AdvancedSlot,
}

#[derive(Debug, Default, Clone)]
pub struct PopupState {
    epochs: HashMap<PopupKey, u64>,
    open: HashSet<PopupKey>,
}

impl PopupState {
    pub fn on_open(&mut self, key: PopupKey) {
        if self.open.insert(key) {
            *self.epochs.entry(key).or_insert(0) += 1;
        }
    }

    pub fn on_close(&mut self, key: PopupKey) {
        self.open.remove(&key);
    }

    pub fn begin_frame(&mut self, key: PopupKey, is_open: bool) {
        if is_open {
            self.on_open(key);
        } else {
            self.on_close(key);
        }
    }

    pub fn id(&self, key: PopupKey) -> egui::Id {
        egui::Id::new(("popup", key, self.epochs.get(&key).copied().unwrap_or(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_id_is_stable_while_open_and_changes_only_on_reopen() {
        let mut state = PopupState::default();
        let key = PopupKey::Picker;
        let initial = state.id(key);
        state.begin_frame(key, true);
        let opened = state.id(key);
        assert_ne!(initial, opened);
        state.begin_frame(key, true);
        assert_eq!(state.id(key), opened);
        state.begin_frame(key, false);
        assert_eq!(state.id(key), opened);
        state.begin_frame(key, false);
        state.begin_frame(key, true);
        assert_ne!(state.id(key), opened);
    }

    #[test]
    fn popup_kinds_have_independent_ids_and_epochs() {
        let mut state = PopupState::default();
        let keys = [
            PopupKey::Picker,
            PopupKey::MacroKeyPick,
            PopupKey::RegularKeyPick,
            PopupKey::PickLayer,
            PopupKey::PendingKeyPick,
            PopupKey::TdKeyPick,
            PopupKey::AdvancedSlot,
        ];
        let initial: Vec<_> = keys.iter().map(|&key| state.id(key)).collect();
        assert_eq!(
            initial.iter().copied().collect::<HashSet<_>>().len(),
            keys.len()
        );
        for (index, &key) in keys.iter().enumerate() {
            state.on_open(key);
            assert_ne!(state.id(key), initial[index]);
            for (other, &other_key) in keys.iter().enumerate().skip(index + 1) {
                assert_eq!(state.id(other_key), initial[other]);
            }
        }
        assert_eq!(
            keys.iter()
                .map(|&key| state.id(key))
                .collect::<HashSet<_>>()
                .len(),
            keys.len()
        );
    }
}
