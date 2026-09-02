use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DefaultLocalizer, LanguageLoader, Localizer};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("the fallback language is embedded in the binary");
    loader
});

pub fn init() {
    let localizer = DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations);
    let requested = i18n_embed::DesktopLanguageRequester::requested_languages();

    match localizer.select(&requested) {
        Ok(selected) => tracing::info!(?selected, ?requested, "loaded translations"),
        Err(error) => tracing::warn!(%error, "keeping English, the locale did not load"),
    }
}

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        ::i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    }};
    ($message_id:literal, $($args:expr),*) => {{
        ::i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args),*)
    }};
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    fn message_ids(catalogue: &str) -> BTreeSet<&str> {
        catalogue
            .lines()
            .filter(|line| !line.starts_with([' ', '\t', '#', '-', '*', '[', ']']))
            .filter_map(|line| line.split_once('='))
            .map(|(id, _)| id.trim())
            .filter(|id| !id.is_empty())
            .collect()
    }

    fn assert_matches_english(language: &str, catalogue: &str) {
        let english = message_ids(include_str!("../i18n/en/clip-keep.ftl"));
        let translated = message_ids(catalogue);

        assert!(
            translated == english,
            "{language} is out of step with en: missing {:?}, unknown {:?}",
            english.difference(&translated).collect::<Vec<_>>(),
            translated.difference(&english).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn every_language_translates_exactly_the_same_messages() {
        for (language, catalogue) in [
            ("de", include_str!("../i18n/de/clip-keep.ftl")),
            ("es", include_str!("../i18n/es/clip-keep.ftl")),
            ("fr", include_str!("../i18n/fr/clip-keep.ftl")),
            ("it", include_str!("../i18n/it/clip-keep.ftl")),
            ("nl", include_str!("../i18n/nl/clip-keep.ftl")),
            ("pt-BR", include_str!("../i18n/pt-BR/clip-keep.ftl")),
            ("ru", include_str!("../i18n/ru/clip-keep.ftl")),
            ("uk", include_str!("../i18n/uk/clip-keep.ftl")),
            ("zh-CN", include_str!("../i18n/zh-CN/clip-keep.ftl")),
        ] {
            assert_matches_english(language, catalogue);
        }
    }

    #[test]
    fn every_catalogue_parses_and_renders() {
        use i18n_embed::LanguageLoader as _;
        use i18n_embed::unic_langid::LanguageIdentifier;

        for language in ["de", "es", "fr", "it", "nl", "pt-BR", "ru", "uk", "zh-CN"] {
            let id: LanguageIdentifier = language.parse().expect("a well-formed language tag");
            let loader = super::fluent_language_loader!();
            loader
                .load_languages(&super::Localizations, &[id])
                .unwrap_or_else(|error| panic!("{language} failed to load: {error}"));

            let rendered = i18n_embed_fl::fl!(loader, "setting-max-age-days", days = 7);
            assert!(
                !rendered.is_empty() && !rendered.contains('{'),
                "{language} left a placeholder unresolved: {rendered}"
            );
        }
    }

    #[test]
    fn attributes_and_comments_are_not_mistaken_for_messages() {
        let ids = message_ids("# a comment = not a message\nreal = yes\n    .tooltip = no\n");

        assert_eq!(ids, BTreeSet::from(["real"]));
    }
}
