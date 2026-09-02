pub mod data_control;
pub mod reader;
pub mod toplevel;
pub mod writer;

use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use wayland_client::Connection;

pub use data_control::{Device, Manager, Offer, Selection, Source};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupError {
    NoCompositor(String),
    NoDataControl,
    NoSeat,
    Protocol(String),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCompositor(detail) => {
                write!(f, "could not reach the compositor: {detail}")
            }
            Self::NoDataControl => f.write_str(
                "the compositor did not offer a data-control protocol, \
                 which usually means this build is sandboxed",
            ),
            Self::NoSeat => f.write_str("the compositor reported no seat"),
            Self::Protocol(detail) => write!(f, "the Wayland handshake failed: {detail}"),
        }
    }
}

pub fn connect() -> Result<Connection, SetupError> {
    let mut attempts = Vec::new();

    for candidate in candidates() {
        match UnixStream::connect(&candidate) {
            Ok(socket) => match Connection::from_socket(socket) {
                Ok(connection) => {
                    tracing::info!(socket = %candidate.display(), "connected to the compositor");
                    return Ok(connection);
                }
                Err(error) => attempts.push(format!("{}: {error}", candidate.display())),
            },
            Err(error) => attempts.push(format!("{}: {error}", candidate.display())),
        }
    }

    Err(SetupError::NoCompositor(if attempts.is_empty() {
        "XDG_RUNTIME_DIR is not set".to_owned()
    } else {
        attempts.join("; ")
    }))
}

fn candidates() -> Vec<PathBuf> {
    let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) else {
        return Vec::new();
    };

    let mut paths = Vec::new();

    if let Some(display) = std::env::var_os("WAYLAND_DISPLAY").map(PathBuf::from) {
        paths.push(if display.is_absolute() {
            display
        } else {
            runtime_dir.join(display)
        });
    }

    for index in 0..=8 {
        let path = runtime_dir.join(format!("wayland-{index}"));
        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    paths.retain(|path| path.exists());
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_runtime_dir_yields_no_candidates() {
        temp_env(&[("XDG_RUNTIME_DIR", None)], || {
            assert!(candidates().is_empty());
        });
    }

    #[test]
    fn the_named_display_is_tried_before_the_numbered_sweep() {
        let dir = std::env::temp_dir().join(format!("clip-keep-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("wayland-0"), []).unwrap();
        std::fs::write(dir.join("wayland-3"), []).unwrap();

        temp_env(
            &[
                ("XDG_RUNTIME_DIR", Some(dir.to_str().unwrap())),
                ("WAYLAND_DISPLAY", Some("wayland-3")),
            ],
            || {
                let found = candidates();
                assert_eq!(found.first().unwrap().file_name().unwrap(), "wayland-3");
                assert!(found.iter().any(|p| p.file_name().unwrap() == "wayland-0"));
            },
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    fn temp_env(vars: &[(&str, Option<&str>)], body: impl FnOnce()) {
        use std::sync::Mutex;
        static LOCK: Mutex<()> = Mutex::new(());

        let guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let saved: Vec<_> = vars
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();

        for (key, value) in vars {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }

        body();

        for (key, value) in saved {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }

        drop(guard);
    }
}
