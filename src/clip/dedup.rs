use sha2::{Digest, Sha256};

use super::model::{Capture, EntryKind, PREVIEW_CHARS};

pub fn hash(capture: &Capture) -> [u8; 32] {
    hash_flavors(capture.kind, &capture.flavors)
}

pub fn hash_flavors(kind: EntryKind, flavors: &[super::model::Flavor]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([kind as u8]);

    match flavors.first() {
        Some(flavor) if kind == EntryKind::Text => {
            hasher.update(String::from_utf8_lossy(&flavor.body).trim().as_bytes());
        }
        Some(flavor) => hasher.update(&flavor.body),
        None => {}
    }

    hasher.finalize().into()
}

pub fn preview(capture: &Capture) -> String {
    let Some(flavor) = capture.primary() else {
        return String::new();
    };

    match capture.kind {
        EntryKind::Image => String::new(),
        EntryKind::Files => {
            let text = String::from_utf8_lossy(&flavor.body);
            let names: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(basename)
                .collect();
            summarise(&names.join(", "))
        }
        EntryKind::Text => summarise(&String::from_utf8_lossy(&flavor.body)),
    }
}

fn basename(uri: &str) -> &str {
    uri.rsplit('/').find(|part| !part.is_empty()).unwrap_or(uri)
}

fn summarise(text: &str) -> String {
    let mut preview = String::with_capacity(PREVIEW_CHARS);
    let mut pending_space = false;

    for character in text.trim().chars() {
        if preview.chars().count() >= PREVIEW_CHARS {
            break;
        }

        if character.is_whitespace() {
            pending_space = !preview.is_empty();
            continue;
        }

        if character.is_control() {
            continue;
        }

        if pending_space {
            preview.push(' ');
            pending_space = false;
        }
        preview.push(character);
    }

    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::model::Flavor;

    fn text(body: &str) -> Capture {
        Capture {
            kind: EntryKind::Text,
            flavors: vec![Flavor::new("text/plain", body.as_bytes().to_vec())],
            source_app: None,
        }
    }

    #[test]
    fn the_same_text_hashes_the_same_regardless_of_surrounding_whitespace() {
        assert_eq!(hash(&text("hello")), hash(&text("  hello\n")));
    }

    #[test]
    fn different_text_hashes_differently() {
        assert_ne!(hash(&text("hello")), hash(&text("hallo")));
    }

    #[test]
    fn the_same_bytes_in_different_kinds_do_not_collide() {
        let as_text = text("data");
        let as_files = Capture {
            kind: EntryKind::Files,
            ..as_text.clone()
        };

        assert_ne!(hash(&as_text), hash(&as_files));
    }

    #[test]
    fn the_extra_markup_flavor_does_not_change_the_hash() {
        let plain = text("hello");
        let mut with_markup = plain.clone();
        with_markup
            .flavors
            .push(Flavor::new("text/html", b"<b>hello</b>".to_vec()));

        assert_eq!(hash(&plain), hash(&with_markup));
    }

    #[test]
    fn a_preview_is_one_line_within_budget() {
        let preview = preview(&text("fn main() {\n    println!(\"hi\");\n}"));

        assert_eq!(preview, "fn main() { println!(\"hi\"); }");
    }

    #[test]
    fn a_long_preview_is_cut_at_the_character_budget() {
        let preview = preview(&text(&"a".repeat(PREVIEW_CHARS * 2)));

        assert_eq!(preview.chars().count(), PREVIEW_CHARS);
    }

    #[test]
    fn multibyte_text_is_never_cut_mid_character() {
        let preview = preview(&text(&"á".repeat(PREVIEW_CHARS * 2)));

        assert_eq!(preview.chars().count(), PREVIEW_CHARS);
        assert!(preview.chars().all(|character| character == 'á'));
    }

    #[test]
    fn control_characters_are_dropped() {
        assert_eq!(preview(&text("red\u{1b}[31mtext")), "red[31mtext");
    }

    #[test]
    fn a_file_list_previews_as_its_names() {
        let capture = Capture {
            kind: EntryKind::Files,
            flavors: vec![Flavor::new(
                "text/uri-list",
                b"# comment\nfile:///home/me/a.txt\nfile:///home/me/b.png\n".to_vec(),
            )],
            source_app: None,
        };

        assert_eq!(preview(&capture), "a.txt, b.png");
    }

    #[test]
    fn an_image_has_no_text_preview() {
        let capture = Capture {
            kind: EntryKind::Image,
            flavors: vec![Flavor::new("image/png", vec![0x89, b'P', b'N', b'G'])],
            source_app: None,
        };

        assert_eq!(preview(&capture), "");
    }
}
