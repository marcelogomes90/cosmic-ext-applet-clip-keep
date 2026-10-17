use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use cosmic::iced::Subscription;
use futures::{Stream, StreamExt};
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;

use super::message::{GlobalAction, Message};

const SOCKET_NAME: &str = "cosmic-ext-applet-clip-keep.sock";

struct Source;

impl Hash for Source {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "clip-keep-global-actions".hash(state);
    }
}

pub fn subscription() -> Subscription<Message> {
    Subscription::run_with(Source, |_| stream())
}

fn stream() -> impl Stream<Item = Message> + Send + 'static {
    let Some(listener) = bind_listener() else {
        return futures::stream::empty().boxed();
    };

    futures::stream::unfold(listener, |listener| async move {
        let (mut socket, _) = listener.accept().await.ok()?;
        let mut command = String::new();
        socket.read_to_string(&mut command).await.ok()?;
        let action = parse_action(command.trim())?;
        Some((Message::Global(action), listener))
    })
    .boxed()
}

fn bind_listener() -> Option<UnixListener> {
    let path = socket_path()?;

    if path.exists() {
        match std::os::unix::net::UnixStream::connect(&path) {
            Ok(_) => return None,
            Err(_) => std::fs::remove_file(&path).ok()?,
        }
    }

    UnixListener::bind(path).ok()
}

pub fn send(action: GlobalAction) {
    let Some(path) = socket_path() else {
        tracing::warn!("cannot send global clipboard action without XDG_RUNTIME_DIR");
        return;
    };

    let command = action_name(action);
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(mut socket) => {
            use std::io::Write;
            if let Err(error) = socket.write_all(command.as_bytes()) {
                tracing::warn!(%error, "could not send global clipboard action");
            }
        }
        Err(error) => tracing::warn!(%error, "Clip Keep is not running"),
    }
}

pub fn action_from_args(mut arguments: impl Iterator<Item = String>) -> Option<GlobalAction> {
    arguments.find_map(|argument| parse_action(argument.trim()))
}

fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|directory| PathBuf::from(directory).join(SOCKET_NAME))
}

fn action_name(action: GlobalAction) -> &'static str {
    match action {
        GlobalAction::TogglePopup => "toggle",
        GlobalAction::TogglePrivate => "private",
        GlobalAction::Clear => "clear",
        GlobalAction::Previous => "previous",
        GlobalAction::Next => "next",
    }
}

fn parse_action(command: &str) -> Option<GlobalAction> {
    match command {
        "toggle" | "--toggle" => Some(GlobalAction::TogglePopup),
        "private" | "--private" => Some(GlobalAction::TogglePrivate),
        "clear" | "--clear" => Some(GlobalAction::Clear),
        "previous" | "--previous" => Some(GlobalAction::Previous),
        "next" | "--next" => Some(GlobalAction::Next),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_round_trip() {
        for action in [
            GlobalAction::TogglePopup,
            GlobalAction::TogglePrivate,
            GlobalAction::Clear,
            GlobalAction::Previous,
            GlobalAction::Next,
        ] {
            assert!(parse_action(action_name(action)).is_some());
        }
    }

    #[test]
    fn arguments_accept_long_options() {
        assert!(matches!(
            action_from_args(["--private".to_owned()].into_iter()),
            Some(GlobalAction::TogglePrivate)
        ));
    }
}
    }
}
