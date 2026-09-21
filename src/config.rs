use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet, CosmicConfigEntry, Error};

use crate::clip::settings::Settings;

pub const CONFIG_VERSION: u64 = 1;

pub const SHORTCUT_REGISTERED: &str = "shortcut_registered";

macro_rules! settings_entry {
    ($($field:ident),+ $(,)?) => {
        impl CosmicConfigEntry for Settings {
            const VERSION: u64 = CONFIG_VERSION;

            fn write_entry(&self, config: &Config) -> Result<(), Error> {

                let mut first: Option<Error> = None;
                $(
                    if let Err(error) = config.set(stringify!($field), &self.$field)
                        && first.is_none()
                    {
                        first = Some(error);
                    }
                )+
                match first {
                    Some(error) => Err(error),
                    None => Ok(()),
                }
            }

            fn get_entry(config: &Config) -> Result<Self, (Vec<Error>, Self)> {
                let mut settings = Self::default();
                let mut errors = Vec::new();

                $(
                    match config.get(stringify!($field)) {
                        Ok(value) => settings.$field = value,
                        Err(error) => errors.push(error),
                    }
                )+

                let settings = settings.sanitised();
                if errors.is_empty() {
                    Ok(settings)
                } else {

                    Err((errors, settings))
                }
            }

            fn update_keys<T: AsRef<str>>(
                &mut self,
                config: &Config,
                changed_keys: &[T],
            ) -> (Vec<Error>, Vec<&'static str>) {
                let mut errors = Vec::new();
                let mut updated = Vec::new();

                for key in changed_keys {
                    match key.as_ref() {
                        $(
                            stringify!($field) => match config.get(stringify!($field)) {
                                Ok(value) => {

                                    if self.$field != value {
                                        self.$field = value;
                                        updated.push(stringify!($field));
                                    }
                                }
                                Err(error) => errors.push(error),
                            },
                        )+
                        _ => {}
                    }
                }

                if !updated.is_empty() {
                    *self = std::mem::take(self).sanitised();
                }

                (errors, updated)
            }
        }
    };
}

settings_entry!(
    max_entries,
    max_age_days,
    capture_images,
    image_size,
    private_mode,
    respect_password_hint,
    paste_on_use,
);

pub struct SettingsStore {
    config: Option<Config>,
}

impl SettingsStore {
    pub fn open(app_id: &str) -> Self {
        match Config::new(app_id, CONFIG_VERSION) {
            Ok(config) => Self {
                config: Some(config),
            },
            Err(error) => {
                tracing::warn!(%error, "settings will not persist, cosmic-config is unavailable");
                Self { config: None }
            }
        }
    }

    pub fn load(&self) -> Settings {
        let Some(config) = self.config.as_ref() else {
            return Settings::default();
        };

        match Settings::get_entry(config) {
            Ok(settings) => settings,
            Err((errors, settings)) => {
                tracing::debug!(
                    unset = errors.len(),
                    "using defaults for settings that have never been written"
                );
                settings
            }
        }
    }

    pub fn flag(&self, key: &str) -> bool {
        self.config
            .as_ref()
            .and_then(|config| config.get::<bool>(key).ok())
            .unwrap_or(false)
    }

    pub fn set_flag(&self, key: &str, value: bool) {
        let Some(config) = self.config.as_ref() else {
            return;
        };

        if let Err(error) = config.set(key, value) {
            tracing::warn!(%error, key, "could not remember a one-off decision");
        }
    }

    pub fn save(&self, settings: &Settings) {
        let Some(config) = self.config.as_ref() else {
            return;
        };

        if let Err(error) = settings.write_entry(config) {
            tracing::warn!(%error, "could not save the settings");
        }
    }
}
