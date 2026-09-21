use std::sync::Arc;

use cosmic::iced::{Rectangle, window};

use crate::clip::model::{EntryId, Snapshot, Thumbnail};

use crate::clip::settings::Settings;

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    ShowPopup,
    SurfaceOpened(window::Id),
    SurfaceClosed(window::Id),
    SurfaceFocused(window::Id),
    SurfaceUnfocused(window::Id),
    PointerEntered(window::Id),
    PointerLeft(window::Id),
    PointerMoved(window::Id),
    ArmOutsideClose(window::Id),
    CloseIfUnfocused(window::Id),
    OpenRowMenu(EntryId),
    PlaceRowMenu(EntryId, Option<Rectangle>),
    CloseRowMenu,
    Snapshot(Arc<Snapshot>),
    SettingsChanged(Box<Settings>),
    Search(String),
    Focus(EntryId),
    Key(super::keys::Action),
    Confirm(EntryId),
    TogglePin(EntryId),
    Delete(EntryId),
    ConfirmClear(bool),
    Clear,
    ThumbnailLoaded(EntryId, Option<Box<Thumbnail>>),
    FormatsLoaded(EntryId, Vec<String>),
    ShowSettings(bool),
    ShowDetails(Option<EntryId>),
    Setting(Box<Settings>),
    OpenLink(&'static str),
    Relayout,
}
