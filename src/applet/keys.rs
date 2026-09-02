use cosmic::iced::keyboard::key::Named;
use cosmic::iced::keyboard::{self, Key, Modifiers};
use cosmic::iced::{Event, Subscription, event};

use super::message::Message;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Confirm,
    Dismiss,
    Pick(usize),
    PinHighlighted,
    DeleteHighlighted,
    PreviewHighlighted,
}

pub fn subscription() -> Subscription<Message> {
    event::listen_with(|event, status, _| {
        let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
            return None;
        };

        if status == event::Status::Captured {
            return None;
        }

        interpret(&key, modifiers).map(Message::Key)
    })
}

pub fn interpret(key: &Key, modifiers: Modifiers) -> Option<Action> {
    if modifiers.alt() {
        return match key {
            Key::Character(character) => match character.as_str() {
                "p" | "P" => Some(Action::PinHighlighted),
                digit => digit
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=9).contains(n))
                    .map(|n| Action::Pick(n - 1)),
            },
            Key::Named(Named::Backspace | Named::Delete) => Some(Action::DeleteHighlighted),
            _ => None,
        };
    }

    if modifiers.control() && matches!(key, Key::Character(space) if space.as_str() == " ") {
        return Some(Action::PreviewHighlighted);
    }

    match key {
        Key::Named(Named::ArrowUp) => Some(Action::Up),
        Key::Named(Named::ArrowDown) => Some(Action::Down),
        Key::Named(Named::Enter) => Some(Action::Confirm),
        Key::Named(Named::Escape) => Some(Action::Dismiss),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::keyboard::key::Named;

    fn named(key: Named) -> Key {
        Key::Named(key)
    }

    fn character(text: &str) -> Key {
        Key::Character(text.into())
    }

    fn alt() -> Modifiers {
        Modifiers::ALT
    }

    #[test]
    fn the_arrows_and_enter_drive_the_list() {
        assert_eq!(
            interpret(&named(Named::ArrowDown), Modifiers::empty()),
            Some(Action::Down)
        );
        assert_eq!(
            interpret(&named(Named::ArrowUp), Modifiers::empty()),
            Some(Action::Up)
        );
        assert_eq!(
            interpret(&named(Named::Enter), Modifiers::empty()),
            Some(Action::Confirm)
        );
        assert_eq!(
            interpret(&named(Named::Escape), Modifiers::empty()),
            Some(Action::Dismiss)
        );
    }

    #[test]
    fn ordinary_typing_is_left_for_the_search_box() {
        assert_eq!(interpret(&character("a"), Modifiers::empty()), None);
        assert_eq!(interpret(&character("1"), Modifiers::empty()), None);
        assert_eq!(
            interpret(&named(Named::Backspace), Modifiers::empty()),
            None,
            "backspace on its own edits the query"
        );
    }

    #[test]
    fn alt_and_a_digit_picks_that_row() {
        assert_eq!(interpret(&character("1"), alt()), Some(Action::Pick(0)));
        assert_eq!(interpret(&character("9"), alt()), Some(Action::Pick(8)));
        assert_eq!(
            interpret(&character("0"), alt()),
            None,
            "the rows are numbered from one"
        );
    }

    #[test]
    fn alt_shortcuts_act_on_the_highlighted_row() {
        assert_eq!(
            interpret(&character("p"), alt()),
            Some(Action::PinHighlighted)
        );
        assert_eq!(
            interpret(&character("P"), alt()),
            Some(Action::PinHighlighted)
        );
        assert_eq!(
            interpret(&named(Named::Backspace), alt()),
            Some(Action::DeleteHighlighted)
        );
    }

    #[test]
    fn control_space_opens_the_preview() {
        assert_eq!(
            interpret(&character(" "), Modifiers::CTRL),
            Some(Action::PreviewHighlighted)
        );
        assert_eq!(interpret(&character(" "), Modifiers::empty()), None);
    }

    #[test]
    fn an_unmapped_combination_does_nothing() {
        assert_eq!(interpret(&character("q"), Modifiers::CTRL), None);
        assert_eq!(interpret(&named(Named::Tab), Modifiers::empty()), None);
    }
}
