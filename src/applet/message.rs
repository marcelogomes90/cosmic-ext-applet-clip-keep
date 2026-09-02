use std::sync::Arc;

use cosmic::iced::window;

use crate::clip::model::{EntryId, Flavor, Snapshot, Thumbnail};

use super::keys;
use crate::clip::settings::Settings;

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    SurfaceClosed(window::Id),
    Snapshot(Arc<Snapshot>),
    SettingsChanged(Box<Settings>),
    Search(String),
    Confirm(EntryId),
    TogglePin(EntryId),
    Delete(EntryId),
    Clear,
    TogglePreview(EntryId),
    ThumbnailLoaded(EntryId, Option<Box<Thumbnail>>),
    PreviewLoaded(EntryId, Option<Box<Flavor>>),
    Key(keys::Action),
    ShowSettings(bool),
    Setting(Box<Settings>),
    Relayout,
}
