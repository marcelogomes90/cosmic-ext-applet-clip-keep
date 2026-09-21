use super::model::EntryKind;

pub const TEXT: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];

pub const RICH_ORDER: &[&str] = &[
    "text/html",
    "text/rtf",
    "application/rtf",
    "text/richtext",
    "text/markdown",
    "text/csv",
];

pub const MAX_EXTRA_FLAVORS: usize = 4;

pub const IMAGE: &[&str] = &["image/png", "image/jpeg", "image/bmp", "image/gif"];

pub const FILES: &[&str] = &["text/uri-list"];

pub const PASSWORD_HINT: &str = "x-kde-passwordManagerHint";

pub const PASSWORD_HINT_SECRET: &str = "secret";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub kind: EntryKind,
    pub mimes: Vec<String>,
    pub password_hint: bool,
}

pub fn choose(offered: &[String], capture_images: bool) -> Option<Wanted> {
    let password_hint = offered.iter().any(|mime| mime == PASSWORD_HINT);

    if let Some(image) = first_of(offered, IMAGE) {
        return capture_images.then(|| Wanted {
            kind: EntryKind::Image,
            mimes: vec![image],
            password_hint,
        });
    }

    if let Some(files) = first_of(offered, FILES) {
        return Some(Wanted {
            kind: EntryKind::Files,
            mimes: vec![files],
            password_hint,
        });
    }

    if let Some(text) = first_of(offered, TEXT) {
        let mut mimes = vec![text];
        let extras = extras(offered, &mimes[0]);
        mimes.extend(extras);
        return Some(Wanted {
            kind: EntryKind::Text,
            mimes,
            password_hint,
        });
    }

    None
}

pub fn text_aliases(offered: &[String]) -> Vec<String> {
    let Some(primary) = offered.first() else {
        return Vec::new();
    };

    if !is_plain_text(primary) {
        return Vec::new();
    }

    let mut aliases = Vec::new();

    for alias in TEXT.iter().copied() {
        let known = offered.iter().any(|mime| mime.eq_ignore_ascii_case(alias));
        if !known {
            push_once(&mut aliases, alias);
        }
    }

    aliases
}

fn extras(offered: &[String], primary: &str) -> Vec<String> {
    let wanted: Vec<&str> = offered
        .iter()
        .map(String::as_str)
        .filter(|mime| is_extra_text(mime, primary))
        .collect();

    let mut chosen = Vec::new();

    for preferred in RICH_ORDER.iter().copied() {
        if let Some(mime) = wanted
            .iter()
            .copied()
            .find(|mime| mime.eq_ignore_ascii_case(preferred))
        {
            push_once(&mut chosen, mime);
        }
    }

    for mime in wanted {
        push_once(&mut chosen, mime);
    }

    chosen.truncate(MAX_EXTRA_FLAVORS);
    chosen
}

fn is_extra_text(mime: &str, primary: &str) -> bool {
    if mime.eq_ignore_ascii_case(primary) || is_plain_text(mime) {
        return false;
    }

    let base = mime.split(';').next().unwrap_or(mime).trim();
    if base.eq_ignore_ascii_case("application/rtf") {
        return true;
    }

    text_subtype(base).is_some_and(|subtype| !is_private_subtype(subtype))
}

fn is_private_subtype(subtype: &str) -> bool {
    subtype.starts_with('_')
        || subtype
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("x-moz-"))
}

fn is_plain_text(mime: &str) -> bool {
    if TEXT.iter().any(|known| mime.eq_ignore_ascii_case(known)) {
        return true;
    }

    let base = mime.split(';').next().unwrap_or(mime).trim();
    base.eq_ignore_ascii_case("text/plain")
}

fn text_subtype(mime: &str) -> Option<&str> {
    let (prefix, subtype) = mime.split_at_checked("text/".len())?;
    prefix.eq_ignore_ascii_case("text/").then_some(subtype)
}

fn push_once(chosen: &mut Vec<String>, mime: &str) {
    if !chosen.iter().any(|kept| kept.eq_ignore_ascii_case(mime)) {
        chosen.push(mime.to_owned());
    }
}

fn first_of(offered: &[String], candidates: &[&str]) -> Option<String> {
    candidates.iter().find_map(|candidate| {
        offered
            .iter()
            .find(|mime| mime.eq_ignore_ascii_case(candidate))
            .cloned()
    })
}

pub fn size_limit(kind: EntryKind) -> usize {
    match kind {
        EntryKind::Text | EntryKind::Files => 5 * 1024 * 1024,
        EntryKind::Image => 20 * 1024 * 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(mimes: &[&str]) -> Vec<String> {
        mimes.iter().map(|mime| (*mime).to_owned()).collect()
    }

    #[test]
    fn plain_text_picks_the_best_encoding_and_keeps_the_markup() {
        let wanted = choose(
            &offered(&[
                "TEXT",
                "text/html",
                "text/plain",
                "text/plain;charset=utf-8",
            ]),
            true,
        )
        .unwrap();

        assert_eq!(wanted.kind, EntryKind::Text);
        assert_eq!(wanted.mimes, ["text/plain;charset=utf-8", "text/html"]);
        assert!(!wanted.password_hint);
    }

    #[test]
    fn every_other_text_flavour_is_kept_beside_the_plain_one() {
        let wanted = choose(
            &offered(&[
                "text/plain;charset=utf-8",
                "text/markdown",
                "text/html",
                "application/rtf",
            ]),
            true,
        )
        .unwrap();

        assert_eq!(
            wanted.mimes,
            [
                "text/plain;charset=utf-8",
                "text/html",
                "application/rtf",
                "text/markdown"
            ],
            "the well-known rich flavours lead, the rest follows as offered"
        );
    }

    #[test]
    fn another_spelling_of_plain_text_is_not_worth_a_second_copy() {
        let wanted = choose(
            &offered(&[
                "text/plain;charset=utf-8",
                "text/plain;charset=iso-8859-1",
                "STRING",
                "TEXT",
            ]),
            true,
        )
        .unwrap();

        assert_eq!(wanted.mimes, ["text/plain;charset=utf-8"]);
    }

    #[test]
    fn an_applications_private_target_is_left_alone() {
        let wanted = choose(
            &offered(&[
                "text/plain",
                "text/_moz_htmlcontext",
                "application/x-openoffice-embed-source-xml",
                "x-special/nautilus-clipboard",
            ]),
            true,
        )
        .unwrap();

        assert_eq!(wanted.mimes, ["text/plain"]);
    }

    #[test]
    fn a_browsers_own_bookkeeping_targets_are_refused() {
        let wanted = choose(
            &offered(&[
                "text/plain;charset=utf-8",
                "text/html",
                "text/x-moz-url-priv",
                "text/_moz_htmlinfo",
            ]),
            true,
        )
        .unwrap();

        assert_eq!(
            wanted.mimes,
            ["text/plain;charset=utf-8", "text/html"],
            "the page url firefox tracks for itself does not belong in the history"
        );
    }

    #[test]
    fn an_app_that_offers_the_world_does_not_fill_the_history_with_it() {
        let wanted = choose(
            &offered(&[
                "text/plain",
                "text/html",
                "text/rtf",
                "text/richtext",
                "text/markdown",
                "text/csv",
                "text/x-vcard",
            ]),
            true,
        )
        .unwrap();

        assert_eq!(wanted.mimes.len(), MAX_EXTRA_FLAVORS + 1);
        assert_eq!(wanted.mimes[0], "text/plain");
    }

    #[test]
    fn the_flavours_a_destination_may_ask_for_are_filled_in() {
        let aliases = text_aliases(&offered(&["text/plain;charset=utf-8", "text/html"]));

        assert_eq!(aliases, ["UTF8_STRING", "text/plain", "STRING", "TEXT"]);
    }

    #[test]
    fn nothing_is_invented_for_an_entry_that_is_not_plain_text() {
        assert!(text_aliases(&offered(&["image/png"])).is_empty());
        assert!(text_aliases(&offered(&["text/uri-list"])).is_empty());
        assert!(text_aliases(&[]).is_empty());
    }

    #[test]
    fn an_alias_already_on_offer_is_not_offered_twice() {
        let aliases = text_aliases(&offered(&["STRING", "text/plain", "TEXT"]));

        assert!(!aliases.iter().any(|alias| alias == "STRING"));
        assert!(!aliases.iter().any(|alias| alias == "text/plain"));
        assert_eq!(
            aliases,
            ["text/plain;charset=utf-8", "UTF8_STRING"],
            "the two spellings of the utf-8 charset count as one"
        );
    }

    #[test]
    fn an_image_wins_over_the_url_offered_beside_it() {
        let wanted = choose(&offered(&["text/plain", "image/png"]), true).unwrap();

        assert_eq!(wanted.kind, EntryKind::Image);
        assert_eq!(wanted.mimes, ["image/png"]);
    }

    #[test]
    fn an_image_is_skipped_entirely_when_images_are_off() {
        assert_eq!(choose(&offered(&["text/plain", "image/png"]), false), None);
    }

    #[test]
    fn files_win_over_the_paths_offered_as_text() {
        let wanted = choose(&offered(&["text/plain", "text/uri-list"]), true).unwrap();

        assert_eq!(wanted.kind, EntryKind::Files);
        assert_eq!(wanted.mimes, ["text/uri-list"]);
    }

    #[test]
    fn the_password_hint_is_noticed_without_changing_what_is_wanted() {
        let wanted = choose(&offered(&["text/plain", PASSWORD_HINT]), true).unwrap();

        assert!(wanted.password_hint);
        assert_eq!(wanted.mimes, ["text/plain"]);
    }

    #[test]
    fn unrecognised_offers_are_ignored() {
        assert_eq!(choose(&offered(&["application/x-qt-internal"]), true), None);
        assert_eq!(choose(&[], true), None);
    }

    #[test]
    fn atom_names_match_regardless_of_case() {
        let wanted = choose(&offered(&["utf8_string"]), true).unwrap();

        assert_eq!(
            wanted.mimes,
            ["utf8_string"],
            "the offered spelling is kept"
        );
    }

    #[test]
    fn images_are_allowed_more_room_than_text() {
        assert!(size_limit(EntryKind::Image) > size_limit(EntryKind::Text));
    }
}
