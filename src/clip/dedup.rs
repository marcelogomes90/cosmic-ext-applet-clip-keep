use sha2::{Digest, Sha256};

use super::model::{Capture, EntryKind, PREVIEW_CHARS, truncate_chars};

const INDENT_CHARS: usize = 8;

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
    let mut preview = String::new();
    let mut taken = 0usize;
    let mut blank_pending = false;

    for line in text.trim().lines() {
        let cleaned = clean_line(line);

        if cleaned.is_empty() {
            blank_pending = !preview.is_empty();
            continue;
        }

        if !preview.is_empty() {
            preview.push('\n');
            if blank_pending {
                preview.push('\n');
            }
        }
        blank_pending = false;
        taken += cleaned.chars().count() + 1;
        preview.push_str(&cleaned);

        if taken >= PREVIEW_CHARS {
            break;
        }
    }

    truncate_chars(&preview, PREVIEW_CHARS)
}

fn clean_line(line: &str) -> String {
    let indent = line
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum::<usize>()
        .min(INDENT_CHARS);

    let mut cleaned = " ".repeat(indent);
    let mut pending_space = false;

    for character in line.trim().chars() {
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }

        if character.is_control() {
            continue;
        }

        if pending_space {
            cleaned.push(' ');
            pending_space = false;
        }
        cleaned.push(character);
    }

    if cleaned.trim().is_empty() {
        cleaned.clear();
    }
    cleaned
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
    fn a_preview_keeps_the_lines_and_the_indentation() {
        let snippet = "fn main() {\n    println!(\"hi\");\n}";

        assert_eq!(preview(&text(snippet)), snippet);
    }

    #[test]
    fn runs_of_spaces_inside_a_line_collapse() {
        assert_eq!(preview(&text("one   two\t\tthree")), "one two three");
    }

    #[test]
    fn a_run_of_blank_lines_becomes_one() {
        assert_eq!(preview(&text("first\n\n\n\n\nlast")), "first\n\nlast");
    }

    #[test]
    fn deep_indentation_is_capped() {
        let preview = preview(&text("top\n\t\t\t\tdeep"));

        assert_eq!(preview, "top\n        deep");
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
