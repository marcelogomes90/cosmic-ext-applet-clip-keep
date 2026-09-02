use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES_CEILING: u32 = 100;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Settings {
    pub max_entries: u32,
    pub max_age_days: Option<u32>,
    pub capture_images: bool,
    pub private_mode: bool,
    pub respect_password_hint: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_entries: 100,
            max_age_days: Some(30),
            capture_images: true,
            private_mode: false,
            respect_password_hint: true,
        }
    }
}

impl Settings {
    pub fn sanitised(mut self) -> Self {
        self.max_entries = self.max_entries.clamp(1, MAX_ENTRIES_CEILING);
        self.max_age_days = self.max_age_days.filter(|days| *days > 0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_survive_sanitising_unchanged() {
        assert_eq!(Settings::default().sanitised(), Settings::default());
    }

    #[test]
    fn out_of_range_values_are_pulled_back() {
        let settings = Settings {
            max_entries: 0,
            max_age_days: Some(0),
            ..Settings::default()
        }
        .sanitised();

        assert_eq!(settings.max_entries, 1);
        assert_eq!(settings.max_age_days, None);
    }
}
