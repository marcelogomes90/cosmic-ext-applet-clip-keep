use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES_CEILING: u32 = 500;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ImageSize {
    #[default]
    Small,
    Medium,
    Large,
}

impl ImageSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(clippy::struct_excessive_bools)]
pub struct Settings {
    pub max_entries: u32,
    pub max_age_days: Option<u32>,
    pub capture_images: bool,
    pub image_size: ImageSize,
    pub private_mode: bool,
    pub respect_password_hint: bool,
    pub paste_on_use: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_entries: 100,
            max_age_days: Some(30),
            capture_images: true,
            image_size: ImageSize::Small,
            private_mode: false,
            respect_password_hint: true,
            paste_on_use: false,
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
    fn a_fresh_install_shows_images_exactly_as_it_always_did() {
        assert_eq!(Settings::default().image_size, ImageSize::Small);
    }

    #[test]
    fn every_image_size_is_offered() {
        assert_eq!(
            ImageSize::ALL,
            [ImageSize::Small, ImageSize::Medium, ImageSize::Large]
        );
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
