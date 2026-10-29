use super::ak820_beta1_profiles as catalog;
use crate::i18n::Language;

/// Profile descriptions are read-only and live in the firmware help article.
pub(crate) fn profile_help(ui: &mut egui::Ui, language: Language) {
    let russian = language == Language::Russian;
    for image in catalog::profiles() {
        let description = catalog::profile_description(image.profile);
        ui.heading(description.title.for_language(russian));
        ui.label(description.summary.for_language(russian));
        if description.purpose.for_language(russian) != description.summary.for_language(russian) {
            ui.label(description.purpose.for_language(russian));
        }
    }
}
