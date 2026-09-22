use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cosmic::iced::event::PlatformSpecific;
use cosmic::iced::event::wayland::{Event as WaylandEvent, LayerEvent, PopupEvent};
use cosmic::iced::{Event, Subscription, event, mouse, window};
use futures::Stream;

use super::message::Message;
use tokio::sync::watch;

use crate::clip::ClipHandle;
use crate::clip::model::Snapshot;
use crate::clip::settings::Settings;
use crate::config::CONFIG_VERSION;
use crate::control::{self, Listener};

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

struct Control {
    handle: ClipHandle,
    output: Option<String>,
}

impl Hash for Control {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "clip-keep-control".hash(state);
    }
}

pub fn control(handle: &ClipHandle) -> Subscription<Message> {
    Subscription::run_with(
        Control {
            handle: handle.clone(),
            output: control::panel_output(),
        },
        |control| requests(control.handle.clone(), control.output.clone()),
    )
}

fn requests(
    handle: ClipHandle,
    output: Option<String>,
) -> impl Stream<Item = Message> + Send + 'static {
    futures::stream::unfold(None, move |listening| {
        let handle = handle.clone();
        let output = output.clone();

        async move {
            let listener = match listening {
                Some(listener) => listener,
                None => Listener::open()?,
            };

            loop {
                let request = listener.next().await;
                let forced = request.forced();
                let ours = forced || claims(&handle, output.as_deref()).await;
                request.answer(ours).await;

                if ours {
                    tracing::debug!(
                        output = output.as_deref(),
                        forced,
                        "taking the global shortcut"
                    );
                    return Some((Message::ShowPopup, Some(listener)));
                }

                tracing::debug!(
                    output = output.as_deref(),
                    "another instance owns the global shortcut"
                );
            }
        }
    })
}

async fn claims(handle: &ClipHandle, output: Option<&str>) -> bool {
    let Some(output) = output else {
        return false;
    };

    handle.target_output().await.as_deref() == Some(output)
}

pub fn surfaces() -> Subscription<Message> {
    event::listen_with(|event, _, id| match event {
        Event::Window(window::Event::Opened { .. }) => Some(Message::SurfaceOpened(id)),
        Event::PlatformSpecific(PlatformSpecific::Wayland(
            WaylandEvent::Layer(LayerEvent::Focused, _, id)
            | WaylandEvent::Popup(PopupEvent::Focused, _, id),
        )) => Some(Message::SurfaceFocused(id)),
        Event::PlatformSpecific(PlatformSpecific::Wayland(
            WaylandEvent::Layer(LayerEvent::Unfocused, _, id)
            | WaylandEvent::Popup(PopupEvent::Unfocused, _, id),
        )) => Some(Message::SurfaceUnfocused(id)),
        Event::Mouse(mouse::Event::CursorEntered) => Some(Message::PointerEntered(id)),
        Event::Mouse(mouse::Event::CursorLeft) => Some(Message::PointerLeft(id)),
        Event::Mouse(mouse::Event::CursorMoved { .. }) => Some(Message::PointerMoved(id)),
        _ => None,
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
