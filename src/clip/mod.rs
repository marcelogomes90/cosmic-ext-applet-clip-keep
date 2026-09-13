pub mod db;
pub mod dedup;
pub mod mime;
pub mod model;
pub mod privacy;
pub mod runtime;
pub mod search;
pub mod settings;
pub mod thumbnail;
pub mod wayland;

use std::sync::Arc;
use std::thread::JoinHandle;

use tokio::sync::{oneshot, watch};

use self::model::{EntryId, Flavor, Snapshot, Thumbnail};
use self::settings::Settings;

#[derive(Debug)]
pub enum ClipCommand {
    Use {
        id: EntryId,
        paste: bool,
    },
    Delete(EntryId),
    Clear {
        include_pinned: bool,
    },
    SetPinned {
        id: EntryId,
        pinned: bool,
    },
    Offer {
        flavors: Vec<Flavor>,
    },
    Thumbnail {
        id: EntryId,
        reply: oneshot::Sender<Option<Thumbnail>>,
    },
    TargetOutput {
        reply: oneshot::Sender<Option<String>>,
    },
    Settings(Box<Settings>),
}

#[derive(Clone)]
pub struct ClipHandle {
    snapshots: watch::Receiver<Arc<Snapshot>>,
    commands: calloop::channel::Sender<ClipCommand>,
}

impl std::fmt::Debug for ClipHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipHandle")
            .field("revision", &self.snapshots.borrow().revision)
            .finish_non_exhaustive()
    }
}

impl ClipHandle {
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshots.borrow())
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshots.clone()
    }

    pub fn send(&self, command: ClipCommand) {
        if let Err(error) = self.commands.send(command) {
            tracing::warn!(%error, "the capture thread is gone, dropping a command");
        }
    }

    pub async fn thumbnail(&self, id: EntryId) -> Option<Thumbnail> {
        let (reply, answer) = oneshot::channel();
        self.send(ClipCommand::Thumbnail { id, reply });
        answer.await.ok().flatten()
    }

    pub async fn target_output(&self) -> Option<String> {
        let (reply, answer) = oneshot::channel();
        self.send(ClipCommand::TargetOutput { reply });
        answer.await.ok().flatten()
    }
}

pub fn spawn(settings: Settings) -> (ClipHandle, JoinHandle<()>) {
    let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(Snapshot::default()));
    let (command_tx, command_rx) = calloop::channel::channel();
    let connection = wayland::connect();

    let join = std::thread::Builder::new()
        .name("clip-keep-capture".into())
        .spawn(move || runtime::run(settings, snapshot_tx, command_rx, connection))
        .expect("the capture thread must start");

    (
        ClipHandle {
            snapshots: snapshot_rx,
            commands: command_tx,
        },
        join,
    )
}
