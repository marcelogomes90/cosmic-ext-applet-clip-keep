use super::model::EntryKind;

pub const TEXT: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];

pub const MARKUP: &[&str] = &["text/html"];

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
        mimes.extend(first_of(offered, MARKUP));
        return Some(Wanted {
            kind: EntryKind::Text,
            mimes,
            password_hint,
        });
    }

    None
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
