use cosmic::iced::keyboard::key::Named;
use cosmic::iced::keyboard::{self, Key, Modifiers};
use cosmic::iced::{Event, Subscription, event};

use super::message::Message;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Confirm,
    Dismiss,
    Delete,
    TogglePin,
    ShowDetails,
    FocusSearch,
    Append(String),
    Backspace,
}

pub fn subscription() -> Subscription<Message> {
    event::listen_with(|event, status, _| {
        match event {
            Event::Keyboard(keyboard::Event::KeyReleased { key, .. }) => released(&key),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) => interpret(
                &key,
                modifiers,
                text.as_deref(),
                status == event::Status::Captured,
            ),
            _ => None,
        }
        .map(Message::Key)
    })
}

pub fn released(key: &Key) -> Option<Action> {
    match key {
        Key::Named(Named::Escape) => Some(Action::Dismiss),
        Key::Named(Named::Enter) => Some(Action::Confirm),
        _ => None,
    }
}

pub fn interpret(
    key: &Key,
    modifiers: Modifiers,
    text: Option<&str>,
    consumed: bool,
) -> Option<Action> {
    if modifiers == Modifiers::CTRL {
        let Key::Character(character) = key else {
            return None;
        };

        return match character.as_str() {
            "d" | "D" => Some(Action::Delete),
            "p" | "P" => Some(Action::TogglePin),
            "i" | "I" => Some(Action::ShowDetails),
            "f" | "F" => Some(Action::FocusSearch),
            _ => None,
        };
    }

    if !(modifiers.is_empty() || modifiers == Modifiers::SHIFT) {
        return None;
    }

    match key {
        Key::Named(Named::ArrowDown) => return Some(Action::Down),
        Key::Named(Named::ArrowUp) => return Some(Action::Up),
        _ => {}
    }

    if consumed {
        return None;
    }

    if matches!(key, Key::Named(Named::Backspace)) {
        return Some(Action::Backspace);
    }

    typed(text).map(Action::Append)
}

fn typed(text: Option<&str>) -> Option<String> {
    let text = text?;

    (!text.is_empty() && !text.chars().any(char::is_control)).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: Named) -> Key {
        Key::Named(key)
    }

    fn character(text: &str) -> Key {
        Key::Character(text.into())
    }

    #[test]
    fn the_arrows_drive_the_list() {
        let none = Modifiers::empty();

        assert_eq!(
            interpret(&named(Named::ArrowDown), none, None, false),
            Some(Action::Down)
        );
        assert_eq!(
            interpret(&named(Named::ArrowUp), none, None, false),
            Some(Action::Up)
        );
    }

    #[test]
    fn navigation_survives_a_widget_that_already_ate_the_key() {
        assert_eq!(
            interpret(&named(Named::ArrowDown), Modifiers::empty(), None, true),
            Some(Action::Down)
        );
    }

    #[test]
    fn control_drives_the_focused_entry() {
        let ctrl = Modifiers::CTRL;

        assert_eq!(
            interpret(&character("d"), ctrl, None, false),
            Some(Action::Delete)
        );
        assert_eq!(
            interpret(&character("p"), ctrl, None, false),
            Some(Action::TogglePin)
        );
        assert_eq!(
            interpret(&character("i"), ctrl, None, false),
            Some(Action::ShowDetails)
        );
        assert_eq!(
            interpret(&character("f"), ctrl, None, false),
            Some(Action::FocusSearch)
        );
    }

    #[test]
    fn what_ends_the_popup_is_taken_on_the_way_up() {
        assert_eq!(released(&named(Named::Escape)), Some(Action::Dismiss));
        assert_eq!(released(&named(Named::Enter)), Some(Action::Confirm));
        assert_eq!(released(&named(Named::ArrowDown)), None);
        assert_eq!(released(&character("a")), None);

        for modifiers in [
            Modifiers::empty(),
            Modifiers::SHIFT,
            Modifiers::CTRL,
            Modifiers::ALT,
            Modifiers::LOGO,
        ] {
            for key in [named(Named::Escape), named(Named::Enter)] {
                assert_eq!(
                    interpret(&key, modifiers, None, true),
                    None,
                    "a terminal action must not also fire on the press, or one tap would \
                     both leave the details page and use the entry behind it"
                );
            }
        }
    }

    #[test]
    fn a_modifier_has_to_match_exactly() {
        let both = Modifiers::CTRL | Modifiers::SHIFT;

        assert_eq!(interpret(&character("d"), both, None, false), None);
        assert_eq!(
            interpret(&named(Named::ArrowDown), Modifiers::ALT, None, false),
            None
        );
    }

    #[test]
    fn ordinary_typing_reaches_the_search() {
        assert_eq!(
            interpret(&character("a"), Modifiers::empty(), Some("a"), false),
            Some(Action::Append("a".to_owned()))
        );
        assert_eq!(
            interpret(&character("A"), Modifiers::SHIFT, Some("A"), false),
            Some(Action::Append("A".to_owned()))
        );
        assert_eq!(
            interpret(&named(Named::Backspace), Modifiers::empty(), None, false),
            Some(Action::Backspace)
        );
    }

    #[test]
    fn a_focused_field_keeps_the_characters_it_consumed() {
        assert_eq!(
            interpret(&character("a"), Modifiers::empty(), Some("a"), true),
            None
        );
        assert_eq!(
            interpret(&named(Named::Backspace), Modifiers::empty(), None, true),
            None
        );
    }

    #[test]
    fn control_characters_are_not_typing() {
        assert_eq!(
            interpret(&named(Named::Tab), Modifiers::empty(), Some("\t"), false),
            None
        );
        assert_eq!(
            interpret(&named(Named::F1), Modifiers::empty(), None, false),
            None
        );
    }
}
