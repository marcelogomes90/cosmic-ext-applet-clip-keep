use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use cosmic_settings_config::shortcuts::{self, Action, Binding, Shortcuts};

use crate::config::{
    CLEAR_SHORTCUT_REGISTERED, PRIVATE_SHORTCUT_REGISTERED, SHORTCUT_REGISTERED, SettingsStore,
};
use crate::{APP_ID, control};

const COMBINATION: &str = "Super+v";
const PRIVATE_COMBINATION: &str = "Super+Alt+v";
const CLEAR_COMBINATION: &str = "Super+Delete";

const DESCRIPTION: &str = "Clip Keep";
const PROGRAM: &str = "cosmic-ext-applet-clip-keep";
const SANDBOX: &str = "/.flatpak-info";
const KEYS: [&str; 2] = ["custom", "defaults"];

const SPECS: [(&str, &str, &str, &str); 3] = [
    (
        SHORTCUT_REGISTERED,
        COMBINATION,
        DESCRIPTION,
        control::TOGGLE,
    ),
    (
        PRIVATE_SHORTCUT_REGISTERED,
        PRIVATE_COMBINATION,
        "Clip Keep Private Mode",
        control::PRIVATE,
    ),
    (
        CLEAR_SHORTCUT_REGISTERED,
        CLEAR_COMBINATION,
        "Clip Keep Clear History",
        control::CLEAR,
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Added,
    Present,
    Taken,
    Unavailable,
}

pub fn register_once(store: &SettingsStore) {
    for (flag, combination, description, argument) in SPECS {
        if store.flag(flag) {
            continue;
        }

        match register(&command_for(argument), combination, description) {
            Outcome::Added => {
                tracing::info!(shortcut = combination, "registered the global shortcut");
            }
            Outcome::Present => {
                tracing::info!(shortcut = combination, "found an existing shortcut");
            }
            Outcome::Taken => {
                tracing::info!(
                    shortcut = combination,
                    "left the global shortcut alone; it is already in use"
                );
                continue;
            }
            Outcome::Unavailable => continue,
        }

        store.set_flag(flag, true);
    }
}

pub fn command() -> String {
    command_for(control::TOGGLE)
}

fn command_for(argument: &str) -> String {
    if std::path::Path::new(SANDBOX).exists() {
        return format!("flatpak run {APP_ID} {argument}");
    }

    let program = std::env::current_exe()
        .ok()
        .filter(|path| path.is_absolute())
        .map_or_else(|| PROGRAM.to_owned(), |path| path.display().to_string());

    format!("{program} {argument}")
}

fn register(command: &str, combination: &str, description: &str) -> Outcome {
    let Ok(context) = shortcuts::context() else {
        tracing::warn!("cosmic-config is unavailable, so the global shortcut was left alone");
        return Outcome::Unavailable;
    };

    let wanted = match Binding::from_str_partial(combination) {
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
            description: Some(description.to_owned()),
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
    fn private_and_clear_bindings_are_the_requested_combinations() {
        assert_eq!(binding(PRIVATE_COMBINATION).to_string(), "Super+Alt+v");
        assert_eq!(binding(CLEAR_COMBINATION).to_string(), "Super+Delete");
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
        assert!(command_for(control::PRIVATE).ends_with(control::PRIVATE));
        assert!(command_for(control::CLEAR).ends_with(control::CLEAR));
    }
}
