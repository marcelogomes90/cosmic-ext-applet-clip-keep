use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use cosmic_settings_config::shortcuts::{self, Action, Binding, Shortcuts};

use crate::config::{SHORTCUT_REGISTERED, SettingsStore};
use crate::{APP_ID, control};

const COMBINATION: &str = "Super+v";

const DESCRIPTION: &str = "Clip Keep";
const PROGRAM: &str = "cosmic-ext-applet-clip-keep";
const SANDBOX: &str = "/.flatpak-info";
const KEYS: [&str; 2] = ["custom", "defaults"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Added,
    Present,
    Taken,
    Unavailable,
}

pub fn register_once(store: &SettingsStore) {
    if store.flag(SHORTCUT_REGISTERED) {
        return;
    }

    match register(&command()) {
        Outcome::Added => {
            tracing::info!(shortcut = COMBINATION, "registered the global shortcut");
        }
        Outcome::Present => {
            tracing::info!("found an existing shortcut that opens Clip Keep");
        }
        Outcome::Taken => {
            tracing::info!(
                shortcut = COMBINATION,
                "left the global shortcut alone; it will be tried again on a later start"
            );
            return;
        }
        Outcome::Unavailable => return,
    }

    store.set_flag(SHORTCUT_REGISTERED, true);
}

pub fn command() -> String {
    if std::path::Path::new(SANDBOX).exists() {
        return format!("flatpak run {APP_ID} {}", control::TOGGLE);
    }

    let program = std::env::current_exe()
        .ok()
        .filter(|path| path.is_absolute())
        .map_or_else(|| PROGRAM.to_owned(), |path| path.display().to_string());

    format!("{program} {}", control::TOGGLE)
}

fn register(command: &str) -> Outcome {
    let Ok(context) = shortcuts::context() else {
        tracing::warn!("cosmic-config is unavailable, so the global shortcut was left alone");
        return Outcome::Unavailable;
    };

    let wanted = match Binding::from_str_partial(COMBINATION) {
        Ok(binding) => binding,
        Err(error) => {
            tracing::warn!(%error, "could not describe the global shortcut");
            return Outcome::Unavailable;
        }
    };

    let shortcuts = KEYS
        .iter()
        .filter_map(|key| context.get::<Shortcuts>(key).ok())
        .collect::<Vec<_>>();

    if shortcuts
        .iter()
        .any(|shortcuts| launches(shortcuts.0.values(), command))
    {
        return Outcome::Present;
    }

    if shortcuts
        .iter()
        .any(|shortcuts| occupied(shortcuts.0.keys(), &wanted))
    {
        return Outcome::Taken;
    }

    let mut custom = context.get::<Shortcuts>("custom").unwrap_or_default();
    custom.0.insert(
        Binding {
            description: Some(DESCRIPTION.to_owned()),
            ..wanted
        },
        Action::Spawn(command.to_owned()),
    );

    write(&context, custom)
}

fn write(context: &Config, custom: Shortcuts) -> Outcome {
    match context.set("custom", custom) {
        Ok(()) => Outcome::Added,
        Err(error) => {
            tracing::warn!(%error, "could not write the global shortcut");
            Outcome::Unavailable
        }
    }
}

fn occupied<'a>(bindings: impl Iterator<Item = &'a Binding>, wanted: &Binding) -> bool {
    bindings.into_iter().any(|binding| binding == wanted)
}

fn launches<'a>(actions: impl Iterator<Item = &'a Action>, command: &str) -> bool {
    actions.into_iter().any(
        |action| matches!(action, Action::Spawn(existing) if existing.trim() == command.trim()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(combination: &str) -> Binding {
        Binding::from_str_partial(combination).expect("the combination must describe a binding")
    }

    #[test]
    fn the_combination_we_ask_for_is_super_and_v() {
        let wanted = binding(COMBINATION);
        assert!(wanted.modifiers.logo);
        assert!(!wanted.modifiers.ctrl && !wanted.modifiers.alt && !wanted.modifiers.shift);
        assert_eq!(wanted.to_string(), "Super+v");
    }

    #[test]
    fn a_taken_combination_is_recognised_whatever_it_is_called() {
        let mine = binding(COMBINATION);
        let theirs = Binding {
            description: Some("Somebody else".to_owned()),
            ..binding("Super+v")
        };

        assert!(occupied([&theirs].into_iter(), &mine));
    }

    #[test]
    fn a_longer_combination_does_not_count_as_ours() {
        let mine = binding(COMBINATION);
        let other = binding("Super+Shift+v");

        assert!(!occupied([&other].into_iter(), &mine));
    }

    #[test]
    fn a_manual_shortcut_for_our_command_is_recognised() {
        let command = "flatpak run io.github.example.ClipKeep --toggle";
        let action = Action::Spawn(command.to_owned());

        assert!(launches([&action].into_iter(), command));
    }

    #[test]
    fn another_spawn_action_does_not_count_as_ours() {
        let action = Action::Spawn("something-else --toggle".to_owned());

        assert!(!launches([&action].into_iter(), "clip-keep --toggle"));
    }

    #[test]
    fn the_command_carries_the_toggle_flag() {
        assert!(command().ends_with(control::TOGGLE));
    }
}
