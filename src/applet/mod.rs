pub mod keys;
pub mod message;
pub mod subscription;
pub mod thumbs;
pub mod view;

use cosmic::Element;
use cosmic::app::{Core, Task};
use cosmic::iced::platform_specific::{
    runtime::wayland::layer_surface::SctkLayerSurfaceSettings,
    shell::commands::{
        layer_surface::{KeyboardInteractivity, Layer, set_keyboard_interactivity},
        popup::destroy_popup,
    },
};
use cosmic::iced::{Subscription, window};

use self::message::Message;
use self::thumbs::Thumbs;
use crate::APP_ID;
use crate::clip::model::{CaptureState, EntryId, EntryKind, EntryMeta, Snapshot};
use crate::clip::settings::Settings;
use crate::clip::{ClipCommand, ClipHandle};
use crate::config::SettingsStore;

const REOPEN_GUARD: std::time::Duration = std::time::Duration::from_millis(400);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PopupState {
    #[default]
    Closed,
    Open(window::Id),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ClearState {
    #[default]
    Idle,
    Asking,
}

pub struct ClipKeep {
    core: Core,
    clip: ClipHandle,
    snapshot: std::sync::Arc<Snapshot>,
    popup: PopupState,
    keyboard_surface: Option<window::Id>,
    focused_surface: Option<window::Id>,
    pointer_inside_popup: bool,
    outside_close_armed: bool,
    dismissed_at: Option<std::time::Instant>,
    settings: Settings,
    store: SettingsStore,
    query: String,
    focused: Option<EntryId>,
    restore: Option<usize>,
    menu: Option<(EntryId, Option<cosmic::iced::Rectangle>)>,
    showing_settings: bool,
    details: Option<EntryId>,
    clearing: ClearState,
    thumbs: Thumbs,
    formats: Option<(EntryId, Vec<String>)>,
}

pub fn run(clip: ClipHandle) -> cosmic::iced::Result {
    cosmic::applet::run::<ClipKeep>(clip)
}

impl cosmic::Application for ClipKeep {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ClipHandle;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, clip: Self::Flags) -> (Self, Task<Message>) {
        let store = SettingsStore::open(APP_ID);
        let settings = store.load();
        let snapshot = clip.snapshot();

        (
            Self {
                core,
                clip,
                snapshot,
                popup: PopupState::Closed,
                keyboard_surface: None,
                focused_surface: None,
                pointer_inside_popup: false,
                outside_close_armed: false,
                dismissed_at: None,
                settings,
                store,
                query: String::new(),
                focused: None,
                restore: None,
                menu: None,
                showing_settings: false,
                details: None,
                clearing: ClearState::default(),
                thumbs: Thumbs::default(),
                formats: None,
            },
            Task::none(),
        )
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::SurfaceClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            subscription::snapshots(&self.clip),
            subscription::settings(),
            subscription::control(&self.clip),
            subscription::surfaces(),
            if self.popup == PopupState::Closed {
                Subscription::none()
            } else {
                keys::subscription()
            },
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let button = self
            .core
            .applet
            .icon_button_from_handle(self.panel_icon())
            .on_press(Message::TogglePopup);

        view::panel(
            button.into(),
            self.core.applet.suggested_bounds,
            self.core.applet.is_horizontal(),
        )
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let PopupState::Open(popup) = self.popup else {
            return cosmic::widget::text::body("").into();
        };

        if popup != id {
            return cosmic::widget::text::body("").into();
        }

        view::popup(self)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if self.menu.is_some() && closes_row_menu(&message) {
            self.menu = None;
        }

        match message {
            Message::Relayout | Message::CloseRowMenu => Task::none(),
            Message::TogglePopup => self.toggle_popup(),
            Message::ShowPopup => self.open_popup(),
            Message::SurfaceOpened(id) => self.opened(id),
            Message::SurfaceClosed(id) => self.closed(id),
            Message::SurfaceFocused(id) => self.surface_focused(id),
            Message::SurfaceUnfocused(id) => self.surface_unfocused(id),
            Message::PointerEntered(id) | Message::PointerMoved(id) => {
                if self.popup_id() == Some(id) {
                    self.pointer_inside_popup = true;
                }
                Task::none()
            }
            Message::PointerLeft(id) => {
                if self.popup_id() == Some(id) {
                    self.pointer_inside_popup = false;
                }
                Task::none()
            }
            Message::ArmOutsideClose(id) => {
                if self.popup_id() == Some(id) {
                    self.outside_close_armed = true;
                }
                Task::none()
            }
            Message::CloseIfUnfocused(id) => self.close_if_unfocused(id),
            Message::OpenRowMenu(id) => self.open_row_menu(id),
            Message::PlaceRowMenu(id, bounds) => {
                if matches!(self.menu, Some((current, _)) if current == id) {
                    self.menu = Some((id, bounds));
                }
                Task::none()
            }
            Message::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                self.settle_focus();

                Task::batch([
                    self.load_thumbnails(),
                    cosmic::task::message(Message::Relayout),
                ])
            }
            Message::SettingsChanged(settings) => self.settings_changed(*settings),
            Message::Search(query) => self.search(query),
            Message::Focus(id) => {
                self.focused = Some(id);
                Task::none()
            }
            Message::Key(action) => {
                if self.menu.take().is_some() && action == keys::Action::Dismiss {
                    return Task::none();
                }
                self.act(action)
            }
            Message::Confirm(id) => self.confirm(id),
            Message::TogglePin(id) => self.toggle_pin(id),
            Message::Delete(id) => self.delete(id),
            Message::ConfirmClear(asking) => {
                self.ask_clear(asking);
                Task::none()
            }
            Message::Clear => self.clear(),
            Message::ThumbnailLoaded(id, thumbnail) => {
                let handle = thumbnail
                    .map(|thumbnail| cosmic::widget::image::Handle::from_bytes(thumbnail.png));
                self.thumbs.insert(id, handle);
                Task::none()
            }
            Message::FormatsLoaded(id, mimes) => {
                if self.details == Some(id) {
                    self.formats = Some((id, mimes));
                }
                Task::none()
            }
            Message::ShowSettings(showing) => self.show_settings(showing),
            Message::ShowDetails(id) => self.show_details(id),
            Message::Setting(settings) => self.apply_settings(*settings),
            Message::OpenLink(url) => cosmic::task::future(async move {
                crate::links::open(url).await;
                Message::Relayout
            }),
        }
    }
}

impl ClipKeep {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub(crate) fn settings(&self) -> &Settings {
        &self.settings
    }

    fn panel_icon(&self) -> cosmic::widget::icon::Handle {
        match self.snapshot.capture {
            CaptureState::Paused => view::icons::paused(),
            CaptureState::Unavailable { .. } => view::icons::warning(),
            CaptureState::Starting | CaptureState::Active(_) => view::icons::clipboard(),
        }
    }

    fn apply_settings(&mut self, settings: Settings) -> Task<Message> {
        let settings = settings.sanitised();
        self.store.save(&settings);
        self.settings_changed(settings)
    }

    fn settings_changed(&mut self, settings: Settings) -> Task<Message> {
        self.settings = settings.sanitised();
        self.clip
            .send(ClipCommand::Settings(Box::new(self.settings.clone())));
        Task::none()
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn clearing(&self) -> bool {
        self.clearing == ClearState::Asking
    }

    fn ask_clear(&mut self, asking: bool) {
        self.clearing = if asking {
            ClearState::Asking
        } else {
            ClearState::Idle
        };
    }

    pub(crate) fn showing_settings(&self) -> bool {
        self.showing_settings
    }

    pub(crate) fn details(&self) -> Option<&EntryMeta> {
        let id = self.details?;
        self.snapshot.entries.iter().find(|entry| entry.id == id)
    }

    pub(crate) fn formats(&self) -> &[String] {
        match &self.formats {
            Some((id, mimes)) if Some(*id) == self.details => mimes,
            _ => &[],
        }
    }

    fn toggle_popup(&mut self) -> Task<Message> {
        if self.popup != PopupState::Closed {
            return self.close_popup();
        }

        if just_dismissed(self.dismissed_at.take(), std::time::Instant::now()) {
            return Task::none();
        }

        self.open_popup()
    }

    fn open_popup(&mut self) -> Task<Message> {
        if self.popup != PopupState::Closed {
            return Task::none();
        }

        self.reset_view();
        self.focused = self.newest();
        self.focused_surface = None;
        self.pointer_inside_popup = false;
        self.outside_close_armed = false;
        self.dismissed_at = None;

        self.create_popup()
    }

    fn create_popup(&mut self) -> Task<Message> {
        let id = window::Id::unique();
        self.popup = PopupState::Open(id);
        tracing::debug!(?id, "opening an ungrabbed popup");

        let parent = self
            .core
            .main_window_id()
            .expect("an applet always has a main window");

        cosmic::surface::surface_task(cosmic::surface::action::app_popup::<Self>(
            |_| cosmic::surface::action::LiveSettings::default(),
            move |app| {
                let mut settings = app
                    .core
                    .applet
                    .get_popup_settings(parent, id, None, None, None);
                settings.grab = false;
                settings
            },
            None,
        ))
    }

    fn opened(&mut self, id: window::Id) -> Task<Message> {
        if self.popup_id() != Some(id) {
            return Task::none();
        }

        let arm = cosmic::task::future(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            Message::ArmOutsideClose(id)
        });

        Task::batch([
            self.capture_keyboard(),
            cosmic::widget::text_input::focus(view::SEARCH_ID.clone()),
            self.reveal(),
            arm,
        ])
    }

    fn close_popup(&mut self) -> Task<Message> {
        let PopupState::Open(id) = self.popup else {
            return Task::none();
        };

        self.popup = PopupState::Closed;
        self.focused_surface = None;
        self.pointer_inside_popup = false;
        self.outside_close_armed = false;
        self.reset_view();

        Task::batch([destroy_popup(id), self.release_keyboard()])
    }

    fn closed(&mut self, id: window::Id) -> Task<Message> {
        if !matches!(self.popup, PopupState::Open(open) if open == id) {
            return Task::none();
        }

        self.popup = PopupState::Closed;
        self.focused_surface = None;
        self.pointer_inside_popup = false;
        self.outside_close_armed = false;
        self.reset_view();

        self.release_keyboard()
    }

    fn surface_focused(&mut self, id: window::Id) -> Task<Message> {
        if self.popup_id() == Some(id) {
            self.focused_surface = Some(id);
            return Task::none();
        }

        if self.keyboard_surface != Some(id) {
            return Task::none();
        }

        self.focused_surface = Some(id);
        tracing::debug!(?id, "keyboard captured; allowing focus to follow clicks");
        set_keyboard_interactivity(id, KeyboardInteractivity::OnDemand)
    }

    fn surface_unfocused(&mut self, id: window::Id) -> Task<Message> {
        let ours = self.popup_id() == Some(id) || self.keyboard_surface == Some(id);
        if !ours {
            return Task::none();
        }

        if self.focused_surface == Some(id) {
            self.focused_surface = None;
        }

        if self.outside_close_armed && !self.pointer_inside_popup {
            self.dismissed_at = Some(std::time::Instant::now());
            return self.close_popup();
        }
        let Some(popup) = self.popup_id() else {
            return Task::none();
        };

        cosmic::task::future(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Message::CloseIfUnfocused(popup)
        })
    }

    fn close_if_unfocused(&mut self, popup: window::Id) -> Task<Message> {
        if self.popup_id() != Some(popup) || self.focused_surface.is_some() {
            return Task::none();
        }

        tracing::debug!(?popup, "closing after focus moved outside the popup");
        self.dismissed_at = Some(std::time::Instant::now());
        self.close_popup()
    }

    fn capture_keyboard(&mut self) -> Task<Message> {
        if self.keyboard_surface.is_some() {
            return Task::none();
        }

        let id = window::Id::unique();
        self.keyboard_surface = Some(id);
        tracing::debug!(?id, "requesting keyboard focus through a helper layer");

        cosmic::surface::surface_task(cosmic::surface::action::app_layer_shell::<Self>(
            |_| cosmic::surface::action::LiveSettings::default(),
            move |_| SctkLayerSurfaceSettings {
                id,
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                input_zone: Some(Vec::new()),
                namespace: format!("{APP_ID}.keyboard-focus"),
                size: Some((Some(1), Some(1))),
                exclusive_zone: -1,
                ..SctkLayerSurfaceSettings::default()
            },
            None,
        ))
    }

    fn release_keyboard(&mut self) -> Task<Message> {
        let Some(id) = self.keyboard_surface.take() else {
            return Task::none();
        };

        if self.focused_surface == Some(id) {
            self.focused_surface = None;
        }
        tracing::debug!(?id, "releasing the helper keyboard layer");
        cosmic::surface::surface_task(cosmic::surface::action::destroy_layer_shell(id))
    }

    fn reset_view(&mut self) {
        self.menu = None;
        self.query.clear();
        self.focused = None;
        self.restore = None;
        self.showing_settings = false;
        self.details = None;
        self.formats = None;
        self.clearing = ClearState::Idle;
    }

    pub(crate) fn focused(&self) -> Option<EntryId> {
        self.focused
    }

    pub(crate) fn row_menu(&self) -> Option<(EntryId, Option<cosmic::iced::Rectangle>)> {
        self.menu
    }

    fn open_row_menu(&mut self, id: EntryId) -> Task<Message> {
        if matches!(self.menu, Some((current, _)) if current == id) {
            self.menu = None;
            return Task::none();
        }

        self.menu = Some((id, None));
        self.focused = Some(id);

        cosmic::iced::runtime::task::widget(view::onscreen_bounds(view::menu_button_id(id)))
            .map(move |bounds| cosmic::Action::App(Message::PlaceRowMenu(id, bounds)))
    }

    pub(crate) fn popup_id(&self) -> Option<window::Id> {
        match self.popup {
            PopupState::Open(id) => Some(id),
            PopupState::Closed => None,
        }
    }

    pub(crate) fn thumbs(&self) -> &Thumbs {
        &self.thumbs
    }

    fn act(&mut self, action: keys::Action) -> Task<Message> {
        use keys::Action;

        if let Some(id) = self.details {
            return match action {
                Action::Dismiss => {
                    self.details = None;
                    Task::none()
                }
                Action::Confirm => self.confirm(id),
                Action::FocusSearch => self.focus_search(),
                _ => Task::none(),
            };
        }

        if self.showing_settings {
            return match action {
                Action::Dismiss => {
                    self.showing_settings = false;
                    Task::none()
                }
                Action::FocusSearch => self.focus_search(),
                _ => Task::none(),
            };
        }

        if self.clearing() {
            match action {
                Action::Dismiss => {
                    self.clearing = ClearState::Idle;
                    return Task::none();
                }
                Action::Confirm => return self.clear(),
                _ => self.clearing = ClearState::Idle,
            }
        }

        match action {
            Action::Down => self.step(true),
            Action::Up => self.step(false),
            Action::Confirm => match self.focused {
                Some(id) => self.confirm(id),
                None => Task::none(),
            },
            Action::Dismiss => self.close_popup(),
            Action::Delete => match self.focused {
                Some(id) => self.delete(id),
                None => Task::none(),
            },
            Action::TogglePin => match self.focused {
                Some(id) => self.toggle_pin(id),
                None => Task::none(),
            },
            Action::ShowDetails => match self.focused {
                Some(id) => self.show_details(Some(id)),
                None => Task::none(),
            },
            Action::FocusSearch => self.focus_search(),
            Action::Append(text) => {
                let mut query = self.query.clone();
                query.push_str(&text);
                self.search(query)
            }
            Action::Backspace => {
                let mut query = self.query.clone();
                query.pop();
                self.search(query)
            }
        }
    }

    fn search(&mut self, query: String) -> Task<Message> {
        self.query = query;
        self.showing_settings = false;
        self.details = None;
        self.clearing = ClearState::Idle;
        self.focused = self.listed().first().copied();
        self.reveal()
    }

    fn focus_search(&mut self) -> Task<Message> {
        self.showing_settings = false;
        self.details = None;
        cosmic::widget::text_input::focus(view::SEARCH_ID.clone())
    }

    fn clear(&mut self) -> Task<Message> {
        self.clearing = ClearState::Idle;
        self.thumbs.clear();
        self.clip.send(ClipCommand::Clear {
            include_pinned: false,
        });
        Task::none()
    }

    fn show_settings(&mut self, showing: bool) -> Task<Message> {
        self.showing_settings = showing;
        self.details = None;
        self.clearing = ClearState::Idle;
        Task::none()
    }

    fn show_details(&mut self, id: Option<EntryId>) -> Task<Message> {
        self.showing_settings = false;
        self.clearing = ClearState::Idle;

        let shown = id.and_then(|wanted| {
            self.snapshot
                .entries
                .iter()
                .find(|entry| entry.id == wanted)
                .map(|entry| (entry.id, entry.kind))
        });
        self.details = shown.map(|(id, _)| id);
        self.formats = None;

        let Some((id, kind)) = shown else {
            return Task::none();
        };

        let tasks = vec![
            self.load_formats(id),
            if kind == EntryKind::Image {
                self.load_thumbnail(id)
            } else {
                Task::none()
            },
        ];

        Task::batch(tasks)
    }

    fn load_formats(&self, id: EntryId) -> Task<Message> {
        let clip = self.clip.clone();

        cosmic::task::future(async move { Message::FormatsLoaded(id, clip.formats(id).await) })
    }

    fn step(&mut self, down: bool) -> Task<Message> {
        let listed = self.listed();
        if listed.is_empty() {
            return Task::none();
        }

        let at = self
            .focused
            .and_then(|id| listed.iter().position(|other| *other == id));

        let next = match (at, down) {
            (Some(index), true) => (index + 1) % listed.len(),
            (Some(index), false) => (index + listed.len() - 1) % listed.len(),
            (None, true) => 0,
            (None, false) => listed.len() - 1,
        };

        self.focused = Some(listed[next]);
        self.reveal()
    }

    fn reveal(&self) -> Task<Message> {
        let Some(id) = self.focused else {
            return Task::none();
        };

        cosmic::iced::runtime::task::widget(view::scroll_into_view(view::row_id(id))).then(
            |shortfall| match shortfall {
                Some(delta) => cosmic::iced::widget::scrollable::scroll_by(
                    view::SCROLL_ID.clone(),
                    cosmic::iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: delta },
                ),
                None => Task::none(),
            },
        )
    }

    fn settle_focus(&mut self) {
        let listed = self.listed();

        if self
            .details
            .is_some_and(|id| !self.snapshot.entries.iter().any(|entry| entry.id == id))
        {
            self.details = None;
        }

        if self.focused.is_some_and(|id| listed.contains(&id)) {
            self.restore = None;
            return;
        }

        self.focused = match self.restore.take() {
            Some(at) if !listed.is_empty() => listed.get(at.min(listed.len() - 1)).copied(),
            _ => listed.first().copied(),
        };
    }

    fn listed(&self) -> Vec<EntryId> {
        view::visible(self).iter().map(|entry| entry.id).collect()
    }

    fn place(&self, id: EntryId) -> Option<usize> {
        self.listed().iter().position(|other| *other == id)
    }

    fn newest(&self) -> Option<EntryId> {
        let entries = &self.snapshot.entries;

        entries
            .iter()
            .find(|entry| entry.pinned.is_none())
            .or_else(|| entries.first())
            .map(|entry| entry.id)
    }

    fn load_thumbnails(&mut self) -> Task<Message> {
        const PER_SNAPSHOT: usize = 24;

        let wanted: Vec<EntryId> = self
            .snapshot
            .entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::Image)
            .map(|entry| entry.id)
            .filter(|id| self.thumbs.wants(*id))
            .take(PER_SNAPSHOT)
            .collect();

        if wanted.is_empty() {
            return Task::none();
        }

        let tasks: Vec<Task<Message>> = wanted
            .into_iter()
            .map(|id| self.load_thumbnail(id))
            .collect();

        Task::batch(tasks)
    }

    fn load_thumbnail(&mut self, id: EntryId) -> Task<Message> {
        if !self.thumbs.wants(id) {
            return Task::none();
        }

        self.thumbs.mark_pending(id);
        let clip = self.clip.clone();

        cosmic::task::future(async move {
            Message::ThumbnailLoaded(id, clip.thumbnail(id).await.map(Box::new))
        })
    }

    fn confirm(&mut self, id: EntryId) -> Task<Message> {
        self.clip.send(ClipCommand::Use {
            id,
            paste: self.settings.paste_on_use,
        });
        self.close_popup()
    }

    fn toggle_pin(&mut self, id: EntryId) -> Task<Message> {
        let pinned = self
            .snapshot
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .is_some_and(|entry| entry.pinned.is_some());

        self.clip.send(ClipCommand::SetPinned {
            id,
            pinned: !pinned,
        });
        Task::none()
    }

    fn delete(&mut self, id: EntryId) -> Task<Message> {
        self.restore = self.place(id);
        self.clip.send(ClipCommand::Delete(id));
        Task::none()
    }
}

fn closes_row_menu(message: &Message) -> bool {
    matches!(
        message,
        Message::CloseRowMenu
            | Message::Confirm(_)
            | Message::TogglePin(_)
            | Message::Delete(_)
            | Message::ConfirmClear(_)
            | Message::Clear
            | Message::Search(_)
            | Message::ShowSettings(_)
            | Message::ShowDetails(_)
            | Message::Setting(_)
            | Message::TogglePopup
            | Message::ShowPopup
            | Message::SurfaceClosed(_)
    )
}

fn just_dismissed(dismissed_at: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    dismissed_at.is_some_and(|at| now.duration_since(at) < REOPEN_GUARD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn the_click_that_dismissed_the_popup_does_not_reopen_it() {
        let dismissed_at = Instant::now();

        assert!(just_dismissed(
            Some(dismissed_at),
            dismissed_at + Duration::from_millis(80)
        ));
    }

    #[test]
    fn a_later_click_opens_the_popup_again() {
        let dismissed_at = Instant::now();

        assert!(!just_dismissed(
            Some(dismissed_at),
            dismissed_at + REOPEN_GUARD + Duration::from_millis(1)
        ));
    }

    #[test]
    fn a_popup_that_was_never_dismissed_never_guards() {
        assert!(!just_dismissed(None, Instant::now()));
    }

    #[test]
    fn an_open_row_menu_survives_everything_that_is_not_a_choice() {
        assert!(!closes_row_menu(&Message::OpenRowMenu(EntryId(1))));
        assert!(!closes_row_menu(&Message::PlaceRowMenu(EntryId(1), None)));
        assert!(!closes_row_menu(&Message::Focus(EntryId(1))));
        assert!(!closes_row_menu(&Message::Relayout));
        assert!(!closes_row_menu(&Message::SurfaceFocused(window::Id::NONE)));
        assert!(!closes_row_menu(&Message::SurfaceUnfocused(
            window::Id::NONE
        )));
        assert!(!closes_row_menu(&Message::ArmOutsideClose(
            window::Id::NONE
        )));
        assert!(!closes_row_menu(&Message::PointerEntered(window::Id::NONE)));
    }

    #[test]
    fn keys_are_left_to_decide_for_themselves() {
        assert!(!closes_row_menu(&Message::Key(keys::Action::Dismiss)));
    }

    #[test]
    fn choosing_an_action_or_leaving_the_page_puts_the_menu_away() {
        assert!(closes_row_menu(&Message::Delete(EntryId(1))));
        assert!(closes_row_menu(&Message::TogglePin(EntryId(1))));
        assert!(closes_row_menu(&Message::ShowDetails(Some(EntryId(1)))));
        assert!(closes_row_menu(&Message::ShowSettings(true)));
        assert!(closes_row_menu(&Message::CloseRowMenu));
        assert!(closes_row_menu(&Message::Search(String::new())));
    }
}
