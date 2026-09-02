use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{PipeReader, PipeWriter};
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

use calloop::channel::{Channel, Event as ChannelEvent};
use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction, RegistrationToken};
use calloop_wayland_source::WaylandSource;
use tokio::sync::watch;
use wayland_client::globals::registry_queue_init;
use wayland_client::{Connection, QueueHandle};

use super::db::{Db, NewEntry};
use super::model::{Capture, CaptureState, EntryId, EntryKind, Flavor, Snapshot, now};
use super::privacy::{self, Filter};
use super::settings::Settings;
use super::wayland::data_control::{Device, Manager, Offer, Selection, Source, SourceData};
use super::wayland::reader::{self, Progress as ReadProgress, TRANSFER_TIMEOUT};
use super::wayland::toplevel::Toplevels;
use super::wayland::writer::{Outgoing, Progress as WriteProgress, set_nonblocking};
use super::wayland::{SetupError, data_control};
use super::{ClipCommand, dedup, mime, thumbnail};

const SECONDARY_GRACE: Duration = Duration::from_millis(300);

type TransferId = u64;

fn is_reflected_selection(
    source_active: bool,
    owned_hash: Option<[u8; 32]>,
    incoming_hash: [u8; 32],
) -> bool {
    source_active && owned_hash == Some(incoming_hash)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Content,
    PasswordHint,
}

#[derive(Debug)]
enum SlotState {
    Reading(RegistrationToken),
    Done(Vec<u8>),
    Failed,
}

#[derive(Debug)]
struct Slot {
    mime: String,
    role: Role,
    state: SlotState,
}

impl Slot {
    fn is_settled(&self) -> bool {
        !matches!(self.state, SlotState::Reading(_))
    }
}

struct Transfer {
    offer: Offer,
    kind: EntryKind,
    source_app: Option<String>,
    slots: Vec<Slot>,
    timeout: Option<RegistrationToken>,
    priming: bool,
    last_progress: Instant,
}

struct Incoming {
    reader: PipeReader,
    buffer: RefCell<Vec<u8>>,
}

impl AsFd for Incoming {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.reader.as_fd()
    }
}

struct OutgoingPipe {
    writer: PipeWriter,
    outgoing: RefCell<Outgoing>,
}

impl AsFd for OutgoingPipe {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.writer.as_fd()
    }
}

pub struct Runtime {
    db: Option<Db>,
    settings: Settings,
    filter: Filter,
    snapshot_tx: watch::Sender<Arc<Snapshot>>,
    revision: u64,
    data_version: i64,
    capture: CaptureState,
    dirty: bool,
    loop_handle: LoopHandle<'static, Runtime>,
    signal: LoopSignal,
    qh: Option<QueueHandle<Runtime>>,
    connection: Option<Connection>,
    manager: Option<Manager>,
    device: Option<Device>,
    pub(crate) toplevels: Option<Toplevels>,
    source: Option<Source>,
    owned_hash: Option<[u8; 32]>,
    primed: bool,
    transfers: HashMap<TransferId, Transfer>,
    next_transfer: TransferId,
    pub(crate) focused_app: Option<String>,
}

pub fn run(
    settings: Settings,
    snapshot_tx: watch::Sender<Arc<Snapshot>>,
    commands: Channel<ClipCommand>,
    connection: Result<Connection, SetupError>,
) {
    let mut event_loop: EventLoop<'static, Runtime> = match EventLoop::try_new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            tracing::error!(%error, "could not start the capture event loop");
            let _ = snapshot_tx.send(Arc::new(Snapshot {
                capture: CaptureState::Unavailable {
                    reason: error.to_string(),
                },
                ..Snapshot::default()
            }));
            return;
        }
    };

    let mut runtime = Runtime::new(settings, snapshot_tx, &event_loop);

    if let Err(error) = runtime.open_database() {
        tracing::error!(%error, "the history database is unavailable");
        runtime.capture = CaptureState::Unavailable {
            reason: format!("the history database could not be opened: {error}"),
        };
    }

    match connection.and_then(|connection| runtime.connect_wayland(&event_loop, connection)) {
        Ok(connection) => connection,
        Err(error) => {
            tracing::warn!(%error, "clipboard capture is off");
            runtime.capture = CaptureState::Unavailable {
                reason: error.to_string(),
            };
        }
    }

    if runtime
        .loop_handle
        .insert_source(commands, |event, (), runtime| match event {
            ChannelEvent::Msg(command) => runtime.on_command(command),
            ChannelEvent::Closed => runtime.signal.stop(),
        })
        .is_err()
    {
        tracing::error!("could not listen for commands");
        return;
    }

    runtime.publish();

    let result = event_loop.run(Duration::from_millis(500), &mut runtime, |runtime| {
        if runtime.dirty || runtime.history_changed_elsewhere() {
            runtime.publish();
        }
    });

    if let Err(error) = result {
        tracing::error!(%error, "the capture event loop stopped");
    }
}

impl Runtime {
    fn new(
        settings: Settings,
        snapshot_tx: watch::Sender<Arc<Snapshot>>,
        event_loop: &EventLoop<'static, Runtime>,
    ) -> Self {
        let settings = settings.sanitised();
        let filter = Filter::new(&settings);

        Self {
            db: None,
            settings,
            filter,
            snapshot_tx,
            revision: 0,
            data_version: 0,
            capture: CaptureState::Starting,
            dirty: false,
            loop_handle: event_loop.handle(),
            signal: event_loop.get_signal(),
            qh: None,
            connection: None,
            manager: None,
            device: None,
            toplevels: None,
            source: None,
            owned_hash: None,
            primed: false,
            transfers: HashMap::new(),
            next_transfer: 1,
            focused_app: None,
        }
    }

    fn open_database(&mut self) -> rusqlite::Result<()> {
        let path = super::db::default_path();
        tracing::info!(path = %path.display(), "opening the history");

        self.db = Some(Db::open(&path)?);
        Ok(())
    }

    fn connect_wayland(
        &mut self,
        event_loop: &EventLoop<'static, Runtime>,
        connection: Connection,
    ) -> Result<(), SetupError> {
        let (globals, queue) = registry_queue_init::<Runtime>(&connection)
            .map_err(|error| SetupError::Protocol(error.to_string()))?;
        let qh = queue.handle();

        let manager = Manager::bind(&globals, &qh)?;
        let seat = data_control::bind_seat(&globals, &qh)?;
        let device = manager.get_data_device(&seat, &qh);

        self.toplevels = Toplevels::bind(&globals, &qh, &seat);

        self.connection = Some(connection.clone());
        WaylandSource::new(connection, queue)
            .insert(event_loop.handle())
            .map_err(|error| SetupError::Protocol(error.to_string()))?;

        self.capture = if self.settings.private_mode {
            CaptureState::Paused
        } else {
            CaptureState::Active(manager.backend())
        };
        self.qh = Some(qh);
        self.manager = Some(manager);
        self.device = Some(device);
        Ok(())
    }

    fn history_changed_elsewhere(&self) -> bool {
        self.db
            .as_ref()
            .and_then(|db| db.data_version().ok())
            .is_some_and(|version| version != self.data_version)
    }

    fn publish(&mut self) {
        self.dirty = false;
        self.revision = self.revision.wrapping_add(1);

        if let Some(version) = self.db.as_ref().and_then(|db| db.data_version().ok()) {
            self.data_version = version;
        }

        let entries = match self.db.as_ref().map(|db| db.list(&self.settings)) {
            Some(Ok(entries)) => entries,
            Some(Err(error)) => {
                tracing::error!(%error, "could not read the history");
                Vec::new()
            }
            None => Vec::new(),
        };

        let snapshot = Snapshot {
            capture: self.capture.clone(),
            entries: entries.into(),
            revision: self.revision,
        };

        let _ = self.snapshot_tx.send(Arc::new(snapshot));
    }

    pub(crate) fn on_selection(&mut self, selection: Selection, connection: &Connection) {
        match selection {
            Selection::Announced | Selection::Current(None) => {}
            Selection::Current(Some(offer)) => self.start_transfer(offer, connection),
            Selection::Finished => {
                tracing::warn!("the compositor withdrew the data-control device");
                self.capture = CaptureState::Unavailable {
                    reason: "the compositor withdrew clipboard access".to_owned(),
                };
                self.device = None;
                self.dirty = true;
            }
        }
    }

    fn start_transfer(&mut self, offer: Offer, connection: &Connection) {
        let priming = !std::mem::replace(&mut self.primed, true);

        if !self.filter.accepts_offers() {
            offer.destroy();
            return;
        }

        let offered = offer.mimes();
        let Some(wanted) = mime::choose(&offered, self.settings.capture_images) else {
            tracing::debug!(?offered, "nothing worth keeping in this selection");
            offer.destroy();
            return;
        };

        let app = self.focused_app.clone();

        let id = self.next_transfer;
        self.next_transfer += 1;
        tracing::debug!(transfer = id, priming, kind = ?wanted.kind, mimes = ?wanted.mimes, "reading a selection");

        let limit = mime::size_limit(wanted.kind);
        let mut slots = Vec::with_capacity(wanted.mimes.len() + 1);

        let mut requests: Vec<(String, Role)> = wanted
            .mimes
            .iter()
            .map(|mime| (mime.clone(), Role::Content))
            .collect();

        if wanted.password_hint && self.settings.respect_password_hint {
            requests.push((mime::PASSWORD_HINT.to_owned(), Role::PasswordHint));
        }

        for (index, (mime, role)) in requests.into_iter().enumerate() {
            match self.request(&offer, id, index, &mime, limit) {
                Some(token) => slots.push(Slot {
                    mime,
                    role,
                    state: SlotState::Reading(token),
                }),
                None => slots.push(Slot {
                    mime,
                    role,
                    state: SlotState::Failed,
                }),
            }
        }

        if let Err(error) = connection.flush() {
            tracing::warn!(%error, "could not flush the receive requests");
        }

        let timeout = self
            .loop_handle
            .insert_source(
                Timer::from_duration(TRANSFER_TIMEOUT),
                move |_, (), runtime: &mut Runtime| match runtime
                    .stalled_for(id)
                    .map(|idle| TRANSFER_TIMEOUT.checked_sub(idle))
                {
                    Some(Some(left)) => TimeoutAction::ToDuration(left),
                    Some(None) => {
                        runtime.abandon(id);
                        TimeoutAction::Drop
                    }
                    None => TimeoutAction::Drop,
                },
            )
            .ok();

        self.transfers.insert(
            id,
            Transfer {
                offer,
                kind: wanted.kind,
                source_app: app,
                slots,
                timeout,
                priming,
                last_progress: Instant::now(),
            },
        );

        self.settle(id);
    }

    fn request(
        &self,
        offer: &Offer,
        transfer: TransferId,
        slot: usize,
        mime: &str,
        limit: usize,
    ) -> Option<RegistrationToken> {
        let (reader, writer) = match reader::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                tracing::warn!(%error, %mime, "could not open a pipe for a flavor");
                return None;
            }
        };

        offer.receive(mime, writer.as_fd());

        drop(writer);

        self.loop_handle
            .insert_source(
                Generic::new(
                    Incoming {
                        reader,
                        buffer: RefCell::new(Vec::new()),
                    },
                    Interest::READ,
                    Mode::Level,
                ),
                move |_, incoming, runtime: &mut Runtime| {
                    let progress = {
                        let mut buffer = incoming.buffer.borrow_mut();
                        reader::pump(&incoming.reader, &mut buffer, limit)
                    };

                    let action = match progress {
                        ReadProgress::Reading => {
                            runtime.note_progress(transfer);
                            return Ok(PostAction::Continue);
                        }
                        ReadProgress::Finished => {
                            let body = std::mem::take(&mut *incoming.buffer.borrow_mut());
                            runtime.slot_done(transfer, slot, body);
                            PostAction::Remove
                        }
                        ReadProgress::Overflowed => {
                            runtime.slot_failed(transfer, slot, "it is over the size limit");
                            PostAction::Remove
                        }
                        ReadProgress::Failed(error) => {
                            runtime.slot_failed(transfer, slot, &error.to_string());
                            PostAction::Remove
                        }
                    };
                    Ok(action)
                },
            )
            .map_err(|error| {
                tracing::warn!(%error, %mime, "could not watch a flavor's pipe");
            })
            .ok()
    }

    fn stalled_for(&self, transfer: TransferId) -> Option<Duration> {
        self.transfers
            .get(&transfer)
            .map(|transfer| transfer.last_progress.elapsed())
    }

    fn note_progress(&mut self, transfer: TransferId) {
        if let Some(transfer) = self.transfers.get_mut(&transfer) {
            transfer.last_progress = Instant::now();
        }
    }

    fn slot_done(&mut self, transfer: TransferId, slot: usize, body: Vec<u8>) {
        if let Some(entry) = self
            .transfers
            .get_mut(&transfer)
            .and_then(|transfer| transfer.slots.get_mut(slot))
        {
            entry.state = SlotState::Done(body);
        }

        if slot == 0 {
            self.hurry(transfer);
        }
        self.settle(transfer);
    }

    fn hurry(&mut self, id: TransferId) {
        let replacement = self
            .loop_handle
            .insert_source(
                Timer::from_duration(SECONDARY_GRACE),
                move |_, (), runtime: &mut Runtime| {
                    runtime.abandon(id);
                    TimeoutAction::Drop
                },
            )
            .ok();

        let previous = self
            .transfers
            .get_mut(&id)
            .and_then(|transfer| std::mem::replace(&mut transfer.timeout, replacement));

        if let Some(previous) = previous {
            self.loop_handle.remove(previous);
        }
    }

    fn slot_failed(&mut self, transfer: TransferId, slot: usize, why: &str) {
        if let Some(entry) = self
            .transfers
            .get_mut(&transfer)
            .and_then(|transfer| transfer.slots.get_mut(slot))
        {
            tracing::debug!(mime = %entry.mime, why, "dropping a flavor");
            entry.state = SlotState::Failed;
        }
        self.settle(transfer);
    }

    fn settle(&mut self, id: TransferId) {
        let Some(transfer) = self.transfers.get(&id) else {
            return;
        };
        if !transfer.slots.iter().all(Slot::is_settled) {
            return;
        }

        let Some(transfer) = self.transfers.remove(&id) else {
            return;
        };
        if let Some(timeout) = transfer.timeout {
            self.loop_handle.remove(timeout);
        }
        transfer.offer.destroy();

        self.store(transfer);
    }

    fn abandon(&mut self, id: TransferId) {
        let Some(mut transfer) = self.transfers.remove(&id) else {
            return;
        };

        let mut stalled = Vec::new();
        for slot in &mut transfer.slots {
            if let SlotState::Reading(token) = slot.state {
                self.loop_handle.remove(token);
                slot.state = SlotState::Failed;
                stalled.push(slot.mime.clone());
            }
        }

        tracing::warn!(?stalled, "gave up on a selection that stopped arriving");

        transfer.timeout = None;
        transfer.offer.destroy();
        self.store(transfer);
    }

    fn store(&mut self, transfer: Transfer) {
        let priming = transfer.priming;
        let secret = transfer
            .slots
            .iter()
            .find(|slot| slot.role == Role::PasswordHint)
            .is_some_and(|slot| match &slot.state {
                SlotState::Done(body) => privacy::hint_is_secret(body),
                SlotState::Failed | SlotState::Reading(_) => true,
            });

        let flavors: Vec<Flavor> = transfer
            .slots
            .iter()
            .filter(|slot| slot.role == Role::Content)
            .filter_map(|slot| match &slot.state {
                SlotState::Done(body) if !body.is_empty() => {
                    Some(Flavor::new(slot.mime.clone(), body.clone()))
                }
                _ => None,
            })
            .collect();

        let capture = Capture {
            kind: transfer.kind,
            flavors,
            source_app: transfer.source_app,
        };

        if let Err(refusal) = self.filter.accepts(&capture, secret) {
            tracing::debug!(%refusal, "not recording a copy");
            return;
        }

        let hash = dedup::hash(&capture);
        if is_reflected_selection(self.source.is_some(), self.owned_hash, hash) {
            tracing::debug!("ignoring the selection reflected by our own source");
            return;
        }

        let Some(db) = self.db.as_mut() else {
            return;
        };

        if priming && db.contains(&hash).unwrap_or(false) {
            tracing::debug!("the clipboard already held something we know; leaving it in place");
            return;
        }

        let preview = dedup::preview(&capture);
        let thumbnail = (capture.kind == EntryKind::Image)
            .then(|| {
                capture
                    .primary()
                    .and_then(|flavor| thumbnail::generate(&flavor.body))
            })
            .flatten();

        let entry = NewEntry {
            hash: &hash,
            kind: capture.kind,
            preview: &preview,
            source_app: capture.source_app.as_deref(),
            flavors: &capture.flavors,
            thumbnail: thumbnail.as_ref(),
        };

        match db.store(&entry, now()) {
            Ok(stored) => {
                tracing::info!(
                    id = %stored.id(),
                    kind = ?capture.kind,
                    bytes = capture.byte_size(),
                    fresh = stored.is_new(),
                    "recorded a copy"
                );
                if let Err(error) = db.prune(&self.settings, now()) {
                    tracing::warn!(%error, "could not prune the history");
                }
                self.dirty = true;
            }
            Err(error) => tracing::error!(%error, "could not record a copy"),
        }
    }

    fn offer_selection(&mut self, kind: EntryKind, flavors: Vec<Flavor>) {
        let (Some(manager), Some(device), Some(qh)) = (
            self.manager.as_ref(),
            self.device.as_ref(),
            self.qh.as_ref(),
        ) else {
            tracing::warn!("cannot set the clipboard without a data-control device");
            return;
        };

        if flavors.is_empty() {
            return;
        }

        let source = manager.create_data_source(qh);
        for flavor in &flavors {
            source.offer(&flavor.mime);
        }

        self.owned_hash = Some(dedup::hash_flavors(kind, &flavors));
        if let Some(data) = source.data()
            && let Ok(mut stored) = data.flavors.lock()
        {
            *stored = flavors
                .into_iter()
                .map(|flavor| (flavor.mime, Arc::from(flavor.body)))
                .collect();
        }

        let previous = self.source.replace(source.clone());
        device.set_selection(Some(&source));

        if let Some(previous) = previous {
            previous.destroy();
        }

        if let Some(connection) = self.connection.as_ref()
            && let Err(error) = connection.flush()
        {
            tracing::warn!(%error, "could not publish the clipboard selection immediately");
        }
    }

    pub(crate) fn on_source_send(
        &mut self,
        data: &SourceData,
        mime: &str,
        fd: std::os::fd::OwnedFd,
    ) {
        let body = data
            .flavors
            .lock()
            .ok()
            .and_then(|flavors| {
                flavors
                    .iter()
                    .find(|(offered, _)| offered == mime)
                    .map(|(_, body)| body.clone())
            })
            .unwrap_or_default();

        let writer = PipeWriter::from(fd);
        set_nonblocking(&writer);

        let pipe = OutgoingPipe {
            writer,
            outgoing: RefCell::new(Outgoing::new(body)),
        };

        match pipe.outgoing.borrow_mut().pump(&pipe.writer) {
            WriteProgress::Finished | WriteProgress::Abandoned => return,
            WriteProgress::Writing => {}
        }

        let registered = self.loop_handle.insert_source(
            Generic::new(pipe, Interest::WRITE, Mode::Level),
            |_, pipe, _: &mut Runtime| {
                let action = match pipe.outgoing.borrow_mut().pump(&pipe.writer) {
                    WriteProgress::Writing => PostAction::Continue,
                    WriteProgress::Finished | WriteProgress::Abandoned => PostAction::Remove,
                };
                Ok(action)
            },
        );

        if let Err(error) = registered {
            tracing::warn!(%error, %mime, "could not finish serving the clipboard");
        }
    }

    pub(crate) fn on_source_cancelled(&mut self, id: &wayland_client::backend::ObjectId) {
        if self.source.as_ref().is_some_and(|source| source.is(id))
            && let Some(source) = self.source.take()
        {
            source.destroy();
            self.owned_hash = None;
        }
    }

    fn on_command(&mut self, command: ClipCommand) {
        match command {
            ClipCommand::Use(id) => self.use_entry(id),
            ClipCommand::Offer { flavors } => {
                let offered: Vec<String> =
                    flavors.iter().map(|flavor| flavor.mime.clone()).collect();
                let kind =
                    mime::choose(&offered, true).map_or(EntryKind::Text, |wanted| wanted.kind);
                self.offer_selection(kind, flavors);
            }
            ClipCommand::Delete(id) => self.with_db("delete an entry", |db| db.delete(id)),
            ClipCommand::Clear { include_pinned } => {
                self.with_db("clear the history", |db| db.clear(include_pinned));
            }
            ClipCommand::SetPinned { id, pinned } => {
                self.with_db("change a pin", |db| db.set_pinned(id, pinned));
            }
            ClipCommand::Load { id, mime, reply } => {
                let flavor = self
                    .db
                    .as_ref()
                    .and_then(|db| db.load(id, mime.as_deref()).ok())
                    .flatten();
                let _ = reply.send(flavor);
            }
            ClipCommand::Thumbnail { id, reply } => {
                let thumbnail = self
                    .db
                    .as_ref()
                    .and_then(|db| db.thumbnail(id).ok())
                    .flatten();
                let _ = reply.send(thumbnail);
            }
            ClipCommand::Settings(settings) => self.apply_settings(*settings),
        }
    }

    fn use_entry(&mut self, id: EntryId) {
        let kind = self.db.as_ref().and_then(|db| db.kind(id).ok()).flatten();
        let loaded = match self.db.as_ref().map(|db| db.load_all(id)) {
            Some(Ok(flavors)) if !flavors.is_empty() => flavors,
            Some(Ok(_)) => {
                tracing::warn!(%id, "that entry has no content left");
                return;
            }
            Some(Err(error)) => {
                tracing::error!(%id, %error, "could not read an entry");
                return;
            }
            None => return,
        };

        let kind = kind.unwrap_or_else(|| {
            let offered: Vec<String> = loaded.iter().map(|flavor| flavor.mime.clone()).collect();
            mime::choose(&offered, true).map_or(EntryKind::Text, |wanted| wanted.kind)
        });
        self.offer_selection(kind, loaded);

        self.with_db("record the use of an entry", |db| db.touch(id, now()));
    }

    fn apply_settings(&mut self, settings: Settings) {
        let settings = settings.sanitised();
        if settings == self.settings {
            return;
        }

        let privacy_changed = settings.private_mode != self.settings.private_mode
            || settings.respect_password_hint != self.settings.respect_password_hint;

        self.settings = settings;
        if privacy_changed {
            self.filter = Filter::new(&self.settings);
        }

        self.capture = match (&self.capture, self.settings.private_mode) {
            (CaptureState::Active(_), true) => CaptureState::Paused,
            (CaptureState::Paused, false) => self
                .manager
                .as_ref()
                .map_or(CaptureState::Starting, |manager| {
                    CaptureState::Active(manager.backend())
                }),
            (state, _) => state.clone(),
        };

        let settings = self.settings.clone();
        self.with_db("prune the history", |db| {
            db.prune(&settings, now()).map(drop)
        });
        self.dirty = true;
    }

    fn with_db(&mut self, what: &str, action: impl FnOnce(&Db) -> rusqlite::Result<()>) {
        let Some(db) = self.db.as_ref() else {
            return;
        };

        match action(db) {
            Ok(()) => self.dirty = true,
            Err(error) => tracing::error!(%error, "could not {what}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_reflected_selection;

    #[test]
    fn only_the_active_sources_matching_selection_is_a_reflection() {
        let owned = [1; 32];
        assert!(is_reflected_selection(true, Some(owned), owned));
        assert!(!is_reflected_selection(true, Some(owned), [2; 32]));
        assert!(!is_reflected_selection(false, Some(owned), owned));
        assert!(!is_reflected_selection(true, None, owned));
    }
}
