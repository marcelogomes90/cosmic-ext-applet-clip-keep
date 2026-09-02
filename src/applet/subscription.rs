use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cosmic::iced::Subscription;
use futures::Stream;

use super::message::Message;
use tokio::sync::watch;

use crate::clip::ClipHandle;
use crate::clip::model::Snapshot;
use crate::clip::settings::Settings;
use crate::config::CONFIG_VERSION;

struct Source(ClipHandle);

impl Hash for Source {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "clip-keep-snapshots".hash(state);
    }
}

pub fn snapshots(handle: &ClipHandle) -> Subscription<Message> {
    Subscription::run_with(Source(handle.clone()), |Source(handle)| {
        stream(handle.subscribe())
    })
}

fn stream(
    receiver: watch::Receiver<Arc<Snapshot>>,
) -> impl Stream<Item = Message> + Send + 'static {
    futures::stream::unfold((receiver, true), |(mut receiver, first)| async move {
        if !first && receiver.changed().await.is_err() {
            return None;
        }

        let snapshot = Arc::clone(&receiver.borrow_and_update());
        Some((Message::Snapshot(snapshot), (receiver, false)))
    })
}

pub fn settings() -> Subscription<Message> {
    cosmic::cosmic_config::config_subscription::<_, Settings>(
        "clip-keep-settings",
        crate::APP_ID.into(),
        CONFIG_VERSION,
    )
    .map(|update| Message::SettingsChanged(Box::new(update.config)))
}
