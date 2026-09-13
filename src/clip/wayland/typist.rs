use std::io::Write;
use std::os::fd::AsFd;

use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use crate::clip::runtime::Runtime;

const KEYMAP: &str = r#"xkb_keymap {
    xkb_keycodes "clip-keep" {
        minimum = 8;
        maximum = 255;
        <LCTL> = 37;
        <LFSH> = 50;
        <AB05> = 55;
        <INS> = 118;
    };
    xkb_types "clip-keep" { include "complete" };
    xkb_compat "clip-keep" { include "complete" };
    xkb_symbols "clip-keep" {
        key <LCTL> { [ Control_L ] };
        key <LFSH> { [ Shift_L ] };
        key <AB05> { [ v, V ] };
        key <INS> { [ Insert ] };
        modifier_map Control { <LCTL> };
        modifier_map Shift { <LFSH> };
    };
};
"#;

const XKB_V1: u32 = 1;

const PASTE_KEY: u32 = 47;
const INSERT_KEY: u32 = 110;

const CONTROL: u32 = 1 << 2;
const SHIFT: u32 = 1;

const PRESSED: u32 = 1;
const RELEASED: u32 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasteShortcut {
    ControlV,
    ControlShiftV,
    ShiftInsert,
}

wayland_client::delegate_noop!(Runtime: ZwpVirtualKeyboardManagerV1);
wayland_client::delegate_noop!(Runtime: ZwpVirtualKeyboardV1);

pub struct Typist {
    keyboard: ZwpVirtualKeyboardV1,
    _keymap: std::fs::File,
}

impl Typist {
    pub fn bind(
        globals: &GlobalList,
        qh: &QueueHandle<Runtime>,
        seat: &WlSeat,
        connection: &Connection,
    ) -> Option<Self> {
        let Ok(manager) = globals.bind::<ZwpVirtualKeyboardManagerV1, _, _>(qh, 1..=1, ()) else {
            tracing::info!("no virtual keyboard; pasting for you is off");
            return None;
        };

        let keyboard = manager.create_virtual_keyboard(seat, qh, ());

        let keymap = match describe(&keyboard) {
            Ok(keymap) => keymap,
            Err(error) => {
                tracing::warn!(%error, "could not hand the compositor a keymap");
                keyboard.destroy();
                return None;
            }
        };

        if let Err(error) = connection.flush() {
            tracing::warn!(%error, "could not deliver the keymap");
            keyboard.destroy();
            return None;
        }

        tracing::info!("ready to paste for you");

        Some(Self {
            keyboard,
            _keymap: keymap,
        })
    }

    pub fn paste(&self, shortcut: PasteShortcut, connection: &Connection) {
        let (key, modifiers) = match shortcut {
            PasteShortcut::ControlV => (PASTE_KEY, CONTROL),
            PasteShortcut::ControlShiftV => (PASTE_KEY, CONTROL | SHIFT),
            PasteShortcut::ShiftInsert => (INSERT_KEY, SHIFT),
        };

        let mut at = moment();
        let mut next = || {
            at = at.wrapping_add(1);
            at
        };

        self.keyboard.modifiers(modifiers, 0, 0, 0);
        self.keyboard.key(next(), key, PRESSED);
        self.keyboard.key(next(), key, RELEASED);
        self.keyboard.modifiers(0, 0, 0, 0);

        if let Err(error) = connection.flush() {
            tracing::warn!(%error, "could not deliver the paste");
        }
    }
}

impl Drop for Typist {
    fn drop(&mut self) {
        self.keyboard.destroy();
    }
}

fn describe(keyboard: &ZwpVirtualKeyboardV1) -> std::io::Result<std::fs::File> {
    use rustix::fs::{MemfdFlags, memfd_create};

    let memfd = memfd_create("clip-keep-keymap", MemfdFlags::CLOEXEC)?;
    let mut file = std::fs::File::from(memfd);

    file.write_all(KEYMAP.as_bytes())?;
    file.write_all(&[0])?;
    file.flush()?;

    let size = u32::try_from(KEYMAP.len() + 1).unwrap_or(u32::MAX);
    keyboard.keymap(XKB_V1, file.as_fd(), size);

    Ok(file)
}

fn moment() -> u32 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let millis = now.tv_sec.unsigned_abs() * 1_000 + now.tv_nsec.unsigned_abs() / 1_000_000;

    u32::try_from(millis % u64::from(u32::MAX)).unwrap_or(0)
}

const TERMINAL_NAMES: &[&str] = &[
    "alacritty",
    "aterm",
    "blackbox",
    "boxi",
    "cool-retro-term",
    "console",
    "contour",
    "cosmic-term",
    "cosmicterm",
    "ddterm",
    "deepin-terminal",
    "eterm",
    "foot",
    "footclient",
    "ghostty",
    "gnome-console",
    "gnome-terminal",
    "guake",
    "hyper",
    "kgx",
    "kitty",
    "konsole",
    "lilyterm",
    "lxterminal",
    "mate-terminal",
    "mlterm",
    "ptyxis",
    "qterminal",
    "rio",
    "roxterm",
    "rxvt",
    "sakura",
    "st",
    "tabby",
    "terminator",
    "terminology",
    "terminix",
    "tilix",
    "urxvt",
    "uxterm",
    "warp",
    "waveterm",
    "wezterm",
    "xfce4-terminal",
    "xterm",
    "yakuake",
];

const TRAILING_SEGMENTS: &[&str] = &[
    "beta",
    "canary",
    "dev",
    "devel",
    "development",
    "gui",
    "nightly",
    "server",
];
const PROCESS_SUFFIXES: &[&str] = &["-server", "_server", "-gui", "_gui"];

pub fn is_terminal(app_id: &str) -> bool {
    terminal_shortcut(app_id).is_some()
}

pub fn paste_shortcut(app_id: Option<&str>) -> PasteShortcut {
    app_id
        .and_then(terminal_shortcut)
        .unwrap_or(PasteShortcut::ControlV)
}

fn terminal_shortcut(app_id: &str) -> Option<PasteShortcut> {
    let id = app_id.trim().to_ascii_lowercase();
    let id = id.strip_suffix(".desktop").unwrap_or(&id);

    if id.is_empty() {
        return None;
    }

    if id.starts_with("dev.boxi.boxi.") {
        return Some(PasteShortcut::ControlShiftV);
    }

    let mut components = id.rsplit('.');
    let mut name = components.next().unwrap_or(id);
    while TRAILING_SEGMENTS.contains(&name) {
        name = components.next()?;
    }

    while let Some(base) = PROCESS_SUFFIXES
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
    {
        name = base;
    }

    let terminal = TERMINAL_NAMES.contains(&name)
        || name == "terminal"
        || name.ends_with("-terminal")
        || name.ends_with("_terminal");

    terminal.then_some(if matches!(name, "xterm" | "uxterm") {
        PasteShortcut::ShiftInsert
    } else {
        PasteShortcut::ControlShiftV
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_reverse_dns_ids_are_recognised() {
        for id in [
            "org.kde.konsole",
            "com.system76.CosmicTerm",
            "org.gnome.Console",
            "org.gnome.Terminal",
            "app.devsuite.Ptyxis",
            "com.mitchellh.ghostty",
            "com.raggesilver.BlackBox",
            "com.gexperts.Tilix",
            "io.elementary.terminal",
            "net.launchpad.terminator",
            "org.contourterminal.Contour",
            "org.deepin.terminal",
            "org.qterminal.qterminal",
            "org.wezfurlong.wezterm",
        ] {
            assert!(is_terminal(id), "{id} should be recognised as a terminal");
        }
    }

    #[test]
    fn common_bare_ids_are_recognised() {
        for id in [
            "alacritty",
            "boxi",
            "foot",
            "footclient",
            "Kitty",
            "st",
            "uxterm",
            "warp",
            "xfce4-terminal",
        ] {
            assert!(is_terminal(id), "{id} should be recognised as a terminal");
        }
    }

    #[test]
    fn desktop_process_and_development_variants_are_normalised() {
        assert!(is_terminal("com.system76.CosmicTerm.desktop"));
        assert!(is_terminal("gnome-terminal-server"));
        assert!(is_terminal("org.gnome.Terminal.Server"));
        assert!(is_terminal("org.wezfurlong.wezterm-gui"));
        assert!(is_terminal("org.gnome.Ptyxis.Devel"));
        assert!(is_terminal("org.gnome.Ptyxis.Nightly.desktop"));
        assert!(is_terminal("dev.boxi.Boxi.fedora"));
    }

    #[test]
    fn paste_shortcut_follows_the_terminal_family() {
        assert_eq!(paste_shortcut(None), PasteShortcut::ControlV);
        assert_eq!(paste_shortcut(Some("firefox")), PasteShortcut::ControlV);
        assert_eq!(
            paste_shortcut(Some("com.gexperts.Tilix")),
            PasteShortcut::ControlShiftV
        );
        assert_eq!(
            paste_shortcut(Some("xfce4-terminal")),
            PasteShortcut::ControlShiftV
        );
        assert_eq!(
            paste_shortcut(Some("dev.boxi.Boxi.fedora")),
            PasteShortcut::ControlShiftV
        );
        assert_eq!(paste_shortcut(Some("XTerm")), PasteShortcut::ShiftInsert);
        assert_eq!(paste_shortcut(Some("UXTerm")), PasteShortcut::ShiftInsert);
    }

    #[test]
    fn an_explicit_terminal_suffix_is_recognised() {
        assert!(is_terminal("org.example.acme-terminal"));
        assert!(is_terminal("vendor_custom_terminal"));
    }

    #[test]
    fn an_ordinary_application_is_left_alone() {
        for id in [
            "",
            "   ",
            "firefox",
            "org.mozilla.firefox",
            "code",
            "com.system76.CosmicFiles",
            "org.gnome.Builder",
            "com.jetbrains.IntelliJ-IDEA",
            "org.example.Determination",
            "org.example.LongTerm",
            "org.example.Terminalizer",
        ] {
            assert!(!is_terminal(id), "{id} should not be treated as a terminal");
        }
    }

    #[test]
    fn the_keymap_names_the_keys_the_paste_needs() {
        assert!(KEYMAP.contains("<AB05>"), "the v key must be defined");
        assert!(KEYMAP.contains("<INS>"), "the Insert key must be defined");
        assert!(KEYMAP.contains("modifier_map Control"));
        assert!(KEYMAP.contains("modifier_map Shift"));
    }
}
