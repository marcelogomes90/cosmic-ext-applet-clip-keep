use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

pub const TOGGLE: &str = "--toggle";

const DIRECTORY: &str = "clip-keep-control";
const EXTENSION: &str = "sock";

const ASK: u8 = b't';
const FORCE: u8 = b'T';
const TAKEN: u8 = b'1';
const DECLINED: u8 = b'0';

const REPLY: Duration = Duration::from_millis(250);
const REQUEST: Duration = Duration::from_millis(100);
const RETRY: Duration = Duration::from_secs(1);
const NAMES: usize = 8;

pub fn panel_output() -> Option<String> {
    std::env::var("COSMIC_PANEL_OUTPUT")
        .ok()
        .filter(|name| !name.is_empty())
}

pub fn toggle() {
    let Some(directory) = directory() else {
        tracing::warn!("XDG_RUNTIME_DIR is not set, so no running applet can be reached");
        return;
    };

    if !toggle_in(&directory) {
        tracing::warn!(
            directory = %directory.display(),
            "no running applet answered the toggle"
        );
    }
}

pub struct Listener {
    path: PathBuf,
    listener: UnixListener,
}

pub struct Request {
    stream: UnixStream,
    forced: bool,
}

impl Listener {
    pub fn open() -> Option<Self> {
        Self::open_in(&directory()?)
    }

    pub fn open_in(directory: &Path) -> Option<Self> {
        if let Err(error) = std::fs::create_dir_all(directory) {
            tracing::warn!(%error, "could not make room for the control socket");
            return None;
        }

        restrict(directory);
        sweep(directory);

        for _ in 0..NAMES {
            let path = directory.join(format!("{}.{EXTENSION}", token()));

            match UnixListener::bind(&path) {
                Ok(listener) => {
                    tracing::debug!(socket = %path.display(), "listening for the global shortcut");
                    return Some(Self { path, listener });
                }
                Err(error) if error.kind() == ErrorKind::AddrInUse => {}
                Err(error) => {
                    tracing::warn!(%error, "could not open the control socket");
                    return None;
                }
            }
        }

        None
    }

    pub async fn next(&self) -> Request {
        loop {
            match self.listener.accept().await {
                Ok((stream, _)) => {
                    if let Some(request) = read(stream).await {
                        return request;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "the control socket refused a connection, retrying");
                    tokio::time::sleep(RETRY).await;
                }
            }
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Request {
    pub fn forced(&self) -> bool {
        self.forced
    }

    pub async fn answer(mut self, taken: bool) {
        let reply = [if taken { TAKEN } else { DECLINED }];
        let _ = tokio::time::timeout(REQUEST, self.stream.write_all(&reply)).await;
    }
}

fn directory() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|runtime| PathBuf::from(runtime).join(DIRECTORY))
}

fn toggle_in(directory: &Path) -> bool {
    let mut answered = Vec::new();

    for path in sockets(directory) {
        match ask(&path, ASK) {
            Answer::Taken => return true,
            Answer::Declined => answered.push(path),
            Answer::Gone => {
                let _ = std::fs::remove_file(&path);
            }
            Answer::Unclear => {}
        }
    }

    answered
        .iter()
        .any(|path| matches!(ask(path, FORCE), Answer::Taken))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    Taken,
    Declined,
    Gone,
    Unclear,
}

fn ask(path: &Path, request: u8) -> Answer {
    let mut stream = match std::os::unix::net::UnixStream::connect(path) {
        Ok(stream) => stream,
        Err(error) if abandoned(&error) => return Answer::Gone,
        Err(error) => {
            tracing::debug!(socket = %path.display(), %error, "could not reach an applet");
            return Answer::Unclear;
        }
    };

    let _ = stream.set_read_timeout(Some(REPLY));
    let _ = stream.set_write_timeout(Some(REPLY));

    if stream.write_all(&[request]).is_err() {
        return Answer::Unclear;
    }

    let mut reply = [0u8; 1];
    match stream.read_exact(&mut reply) {
        Ok(()) if reply[0] == TAKEN => Answer::Taken,
        Ok(()) => Answer::Declined,
        Err(error) => {
            tracing::debug!(socket = %path.display(), %error, "an applet did not answer");
            Answer::Unclear
        }
    }
}

fn sweep(directory: &Path) {
    for path in sockets(directory) {
        if let Err(error) = std::os::unix::net::UnixStream::connect(&path)
            && abandoned(&error)
        {
            tracing::debug!(socket = %path.display(), "removing an abandoned socket");
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn restrict(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;

    if let Err(error) = std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
    {
        tracing::warn!(directory = %directory.display(), %error, "could not restrict the control socket");
    }
}

fn abandoned(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::NotFound | ErrorKind::ConnectionRefused
    )
}

fn sockets(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };

    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == EXTENSION)
        })
        .collect()
}

async fn read(mut stream: UnixStream) -> Option<Request> {
    let request = tokio::time::timeout(REQUEST, stream.read_u8())
        .await
        .ok()?
        .ok()?;

    match request {
        ASK => Some(Request {
            stream,
            forced: false,
        }),
        FORCE => Some(Request {
            stream,
            forced: true,
        }),
        _ => None,
    }
}

fn token() -> String {
    use std::hash::{BuildHasher, Hasher};

    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos()),
    );

    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("clip-keep-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("the scratch directory must be writable");
        path
    }

    #[test]
    fn two_instances_never_pick_the_same_socket_name() {
        assert_ne!(token(), token());
    }

    #[test]
    fn only_sockets_are_listed() {
        let directory = scratch("listing");
        std::fs::write(directory.join("a.sock"), []).unwrap();
        std::fs::write(directory.join("notes.txt"), []).unwrap();

        let found = sockets(&directory);
        assert_eq!(found.len(), 1, "only the socket should be listed");
        assert!(found[0].ends_with("a.sock"));

        std::fs::remove_dir_all(&directory).ok();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_live_listener_answers_the_toggle_and_an_abandoned_socket_is_swept() {
        let directory = scratch("control");
        std::fs::write(directory.join("stale.sock"), []).unwrap();

        let listener = Listener::open_in(&directory).expect("the listener must open");
        assert!(
            !directory.join("stale.sock").exists(),
            "opening must sweep a socket nothing listens on"
        );

        let answering = tokio::spawn(async move {
            let request = listener.next().await;
            assert!(!request.forced(), "the first pass must be a plain ask");
            request.answer(true).await;
            listener
        });

        let asked = {
            let directory = directory.clone();
            tokio::task::spawn_blocking(move || toggle_in(&directory))
                .await
                .unwrap()
        };

        let listener = answering.await.unwrap();
        assert!(asked, "the instance that took the request must be reported");

        let socket = listener.path.clone();
        drop(listener);
        assert!(
            !socket.exists(),
            "a listener removes its socket on the way out"
        );

        std::fs::remove_dir_all(&directory).ok();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_instance_that_declines_is_asked_again_without_a_choice() {
        let directory = scratch("forced");
        let listener = Listener::open_in(&directory).expect("the listener must open");

        let answering = tokio::spawn(async move {
            let first = listener.next().await;
            assert!(!first.forced());
            first.answer(false).await;

            let second = listener.next().await;
            assert!(second.forced(), "the fallback pass must be forced");
            second.answer(true).await;
        });

        let asked = {
            let directory = directory.clone();
            tokio::task::spawn_blocking(move || toggle_in(&directory))
                .await
                .unwrap()
        };

        answering.await.unwrap();
        assert!(asked, "the fallback must reach the only instance there is");

        std::fs::remove_dir_all(&directory).ok();
    }
}
