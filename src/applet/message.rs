use std::sync::Arc;

use cosmic::iced::window;

use crate::clip::model::{EntryId, Snapshot, Thumbnail};

use crate::clip::settings::Settings;

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    SurfaceClosed(window::Id),
    Snapshot(Arc<Snapshot>),
    SettingsChanged(Box<Settings>),
    Search(String),
    Surface(cosmic::surface::Action),
    DelayedSurface(window::Id, cosmic::surface::Action),
    Scrolled(f32),
    Hover(EntryId),
    Unhover(EntryId),
    Confirm(EntryId),
    TogglePin(EntryId),
    Delete(EntryId),
    Clear,
    ThumbnailLoaded(EntryId, Option<Box<Thumbnail>>),
    ShowSettings(bool),
    Setting(Box<Settings>),
    Relayout,
}
