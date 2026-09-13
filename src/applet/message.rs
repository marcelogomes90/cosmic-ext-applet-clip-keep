use std::sync::Arc;

use cosmic::iced::{Rectangle, window};

use crate::clip::model::{EntryId, Snapshot, Thumbnail};

use crate::clip::settings::Settings;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowAction {
    Details,
    Pin,
    Delete,
}

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
    PointerMoved(window::Id, cosmic::iced::Point),
    ArmOutsideClose(window::Id),
    CloseIfUnfocused(window::Id),
    PrepareActionHint(EntryId, RowAction),
    ShowActionHint(EntryId, RowAction, Option<Rectangle>),
    Snapshot(Arc<Snapshot>),
    SettingsChanged(Box<Settings>),
    Search(String),
    Focus(EntryId),
    Key(super::keys::Action),
    Confirm(EntryId),
    TogglePin(EntryId),
    Delete(EntryId),
    Clear,
    ThumbnailLoaded(EntryId, Option<Box<Thumbnail>>),
    ShowSettings(bool),
    ShowDetails(Option<EntryId>),
    Setting(Box<Settings>),
    Relayout,
}
