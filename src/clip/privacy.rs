use super::mime::PASSWORD_HINT_SECRET;
use super::model::{Capture, EntryKind};
use super::settings::Settings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    PrivateMode,
    PasswordHint,
    Empty,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PrivateMode => f.write_str("private mode is on"),
            Self::PasswordHint => f.write_str("the source marked it as a secret"),
            Self::Empty => f.write_str("it is empty"),
        }
    }
}

pub struct Filter {
    private_mode: bool,
    respect_password_hint: bool,
}

impl Filter {
    pub fn new(settings: &Settings) -> Self {
        Self {
            private_mode: settings.private_mode,
            respect_password_hint: settings.respect_password_hint,
        }
    }

    pub fn accepts_offers(&self) -> bool {
        !self.private_mode
    }

    pub fn accepts(&self, capture: &Capture, password_hint_is_secret: bool) -> Result<(), Refusal> {
        if self.private_mode {
            return Err(Refusal::PrivateMode);
        }

        if self.respect_password_hint && password_hint_is_secret {
            return Err(Refusal::PasswordHint);
        }

        let Some(primary) = capture.primary() else {
            return Err(Refusal::Empty);
        };

        if capture.kind == EntryKind::Text
            && String::from_utf8_lossy(&primary.body).trim().is_empty()
        {
            return Err(Refusal::Empty);
        }

        if primary.body.is_empty() {
            return Err(Refusal::Empty);
        }

        Ok(())
    }
}

pub fn hint_is_secret(body: &[u8]) -> bool {
    String::from_utf8_lossy(body)
        .trim()
        .eq_ignore_ascii_case(PASSWORD_HINT_SECRET)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::model::Flavor;

    fn capture(text: &str) -> Capture {
        Capture {
            kind: EntryKind::Text,
            flavors: vec![Flavor::new("text/plain", text.as_bytes().to_vec())],
            source_app: None,
        }
    }

    #[test]
    fn ordinary_text_is_accepted() {
        let filter = Filter::new(&Settings::default());
        assert_eq!(filter.accepts(&capture("hello"), false), Ok(()));
    }

    #[test]
    fn private_mode_refuses_everything_and_stops_offers_being_read() {
        let filter = Filter::new(&Settings {
            private_mode: true,
            ..Settings::default()
        });

        assert!(!filter.accepts_offers());
        assert_eq!(
            filter.accepts(&capture("hello"), false),
            Err(Refusal::PrivateMode)
        );
    }

    #[test]
    fn the_password_hint_is_honoured_and_can_be_turned_off() {
        let honoured = Filter::new(&Settings::default());
        assert_eq!(
            honoured.accepts(&capture("hunter2"), true),
            Err(Refusal::PasswordHint)
        );

        let ignored = Filter::new(&Settings {
            respect_password_hint: false,
            ..Settings::default()
        });
        assert_eq!(ignored.accepts(&capture("hunter2"), true), Ok(()));
    }

    #[test]
    fn whitespace_is_not_worth_recording() {
        let filter = Filter::new(&Settings::default());
        assert_eq!(
            filter.accepts(&capture("   \n\t "), false),
            Err(Refusal::Empty)
        );
    }

    #[test]
    fn the_secret_hint_is_read_leniently() {
        assert!(hint_is_secret(b"secret"));
        assert!(hint_is_secret(b" Secret\n"));
        assert!(!hint_is_secret(b"public"));
        assert!(!hint_is_secret(b""));
    }
}
