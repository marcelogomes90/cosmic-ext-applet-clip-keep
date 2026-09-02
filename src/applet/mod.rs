pub mod keys;
pub mod message;
pub mod subscription;
pub mod thumbs;
pub mod view;

use cosmic::Element;
use cosmic::app::{Core, Task};
use cosmic::iced::platform_specific::shell::commands::popup::destroy_popup;
use cosmic::iced::{Subscription, window};

use self::message::Message;
use self::thumbs::Thumbs;
use crate::APP_ID;
use crate::clip::model::{CaptureState, EntryId, EntryKind, Snapshot};
use crate::clip::settings::Settings;
use crate::clip::{ClipCommand, ClipHandle};
use crate::config::SettingsStore;

const PANEL_ICON: &str = "edit-paste-symbolic";

const PAUSED_ICON: &str = "changes-prevent-symbolic";
const UNAVAILABLE_ICON: &str = "dialog-warning-symbolic";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PopupState {
    #[default]
    Closed,
    Open {
        id: window::Id,
        closing: bool,
    },
}

pub struct ClipKeep {
    core: Core,
    clip: ClipHandle,
    snapshot: std::sync::Arc<Snapshot>,
    popup: PopupState,
    settings: Settings,
    store: SettingsStore,
    query: String,
    highlight: usize,
    showing_settings: bool,
    thumbs: Thumbs,
    preview: Option<EntryId>,
    preview_content: Option<Preview>,
}

pub(crate) enum Preview {
    Text(String),
    Unavailable,
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
                settings,
                store,
                query: String::new(),
                highlight: 0,
                showing_settings: false,
                thumbs: Thumbs::default(),
                preview: None,
                preview_content: None,
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
            if self.popup == PopupState::Closed {
                Subscription::none()
            } else {
                keys::subscription()
            },
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button(self.panel_icon())
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let PopupState::Open { id: popup, .. } = self.popup else {
            return cosmic::widget::text::body("").into();
        };

        if popup != id {
            return cosmic::widget::text::body("").into();
        }

        view::popup(self)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Relayout => Task::none(),
            Message::TogglePopup => self.toggle_popup(),
            Message::SurfaceClosed(id) => {
                if matches!(self.popup, PopupState::Open { id: open, .. } if open == id) {
                    self.popup = PopupState::Closed;
                    self.reset_view();
                }
                Task::none()
            }
            Message::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                self.clamp_highlight();

                if self
                    .preview
                    .is_some_and(|id| !self.snapshot.entries.iter().any(|e| e.id == id))
                {
                    self.preview = None;
                    self.preview_content = None;
                }

                Task::batch([
                    self.load_thumbnails(),
                    cosmic::task::message(Message::Relayout),
                ])
            }
            Message::SettingsChanged(settings) => {
                self.settings = settings.sanitised();
                self.clip
                    .send(ClipCommand::Settings(Box::new(self.settings.clone())));
                Task::none()
            }
            Message::Search(query) => {
                self.query = query;
                self.highlight = 0;
                self.showing_settings = false;

                self.preview = None;
                self.preview_content = None;
                Task::none()
            }
            Message::Confirm(id) => self.confirm(id),
            Message::TogglePin(id) => self.toggle_pin(id),
            Message::Delete(id) => self.delete(id),
            Message::Clear => {
                self.preview = None;
                self.preview_content = None;
                self.thumbs.clear();
                self.clip.send(ClipCommand::Clear {
                    include_pinned: false,
                });
                Task::none()
            }
            Message::TogglePreview(id) => self.toggle_preview(id),
            Message::ThumbnailLoaded(id, thumbnail) => {
                let handle = thumbnail
                    .map(|thumbnail| cosmic::widget::image::Handle::from_bytes(thumbnail.png));
                self.thumbs.insert(id, handle);
                Task::none()
            }
            Message::PreviewLoaded(id, flavor) => {
                if self.preview == Some(id) {
                    self.preview_content = Some(Self::build_preview(flavor));
                }
                Task::none()
            }
            Message::Key(action) => self.on_key(action),
            Message::ShowSettings(showing) => {
                self.showing_settings = showing;
                Task::none()
            }
            Message::Setting(settings) => self.apply_settings(*settings),
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

    fn panel_icon(&self) -> &'static str {
        match self.snapshot.capture {
            CaptureState::Paused => PAUSED_ICON,
            CaptureState::Unavailable { .. } => UNAVAILABLE_ICON,
            CaptureState::Starting | CaptureState::Active(_) => PANEL_ICON,
        }
    }

    fn apply_settings(&mut self, settings: Settings) -> Task<Message> {
        self.settings = settings.sanitised();
        self.store.save(&self.settings);
        self.clip
            .send(ClipCommand::Settings(Box::new(self.settings.clone())));
        Task::none()
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn highlight(&self) -> usize {
        self.highlight
    }

    pub(crate) fn showing_settings(&self) -> bool {
        self.showing_settings
    }

    fn toggle_popup(&mut self) -> Task<Message> {
        if self.popup != PopupState::Closed {
            return self.close_popup();
        }

        let id = window::Id::unique();
        self.popup = PopupState::Open { id, closing: false };
        self.reset_view();

        let parent = self
            .core
            .main_window_id()
            .expect("an applet always has a main window");

        cosmic::surface::surface_task(cosmic::surface::action::app_popup::<Self>(
            |_| cosmic::surface::action::LiveSettings::default(),
            move |app| {
                app.core
                    .applet
                    .get_popup_settings(parent, id, None, None, None)
            },
            None,
        ))
        .chain(cosmic::widget::text_input::focus(view::SEARCH_ID.clone()))
    }

    fn close_popup(&mut self) -> Task<Message> {
        match self.popup {
            PopupState::Open { id, closing: false } => {
                self.popup = PopupState::Open { id, closing: true };
                destroy_popup(id)
            }
            PopupState::Closed | PopupState::Open { closing: true, .. } => Task::none(),
        }
    }

    fn reset_view(&mut self) {
        self.query.clear();
        self.highlight = self.newest();
        self.showing_settings = false;
        self.preview = None;
        self.preview_content = None;
    }

    pub(crate) fn thumbs(&self) -> &Thumbs {
        &self.thumbs
    }

    pub(crate) fn preview(&self) -> Option<(EntryId, &Preview)> {
        self.preview.zip(self.preview_content.as_ref())
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
            .map(|id| {
                self.thumbs.mark_pending(id);
                let clip = self.clip.clone();
                cosmic::task::future(async move {
                    Message::ThumbnailLoaded(id, clip.thumbnail(id).await.map(Box::new))
                })
            })
            .collect();

        Task::batch(tasks)
    }

    fn confirm(&mut self, id: EntryId) -> Task<Message> {
        self.clip.send(ClipCommand::Use(id));
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
        if self.preview == Some(id) {
            self.preview = None;
            self.preview_content = None;
        }
        self.clip.send(ClipCommand::Delete(id));
        Task::none()
    }

    fn toggle_preview(&mut self, id: EntryId) -> Task<Message> {
        if self.preview == Some(id) {
            self.preview = None;
            self.preview_content = None;
            return Task::none();
        }

        self.preview = Some(id);
        self.preview_content = None;

        let clip = self.clip.clone();
        cosmic::task::future(async move {
            Message::PreviewLoaded(id, clip.load(id, None).await.map(Box::new))
        })
    }

    fn build_preview(flavor: Option<Box<crate::clip::model::Flavor>>) -> Preview {
        flavor.map_or(Preview::Unavailable, |flavor| {
            Preview::Text(String::from_utf8_lossy(&flavor.body).into_owned())
        })
    }

    fn on_key(&mut self, action: keys::Action) -> Task<Message> {
        let visible: Vec<EntryId> = view::visible(self).iter().map(|entry| entry.id).collect();
        let last = visible.len().saturating_sub(1);

        match action {
            keys::Action::Up => {
                self.highlight = if self.highlight == 0 {
                    last
                } else {
                    self.highlight - 1
                };
                Task::none()
            }
            keys::Action::Down => {
                self.highlight = if self.highlight >= last {
                    0
                } else {
                    self.highlight + 1
                };
                Task::none()
            }
            keys::Action::Dismiss => {
                if self.preview.is_some() {
                    self.preview = None;
                    self.preview_content = None;
                    Task::none()
                } else if self.showing_settings {
                    self.showing_settings = false;
                    Task::none()
                } else {
                    self.close_popup()
                }
            }
            keys::Action::Confirm => self.act_on(self.highlight, &visible, Self::confirm),
            keys::Action::Pick(index) => self.act_on(index, &visible, Self::confirm),
            keys::Action::PinHighlighted => self.act_on(self.highlight, &visible, Self::toggle_pin),
            keys::Action::DeleteHighlighted => self.act_on(self.highlight, &visible, Self::delete),
            keys::Action::PreviewHighlighted => {
                self.act_on(self.highlight, &visible, Self::toggle_preview)
            }
        }
    }

    fn act_on(
        &mut self,
        index: usize,
        visible: &[EntryId],
        action: fn(&mut Self, EntryId) -> Task<Message>,
    ) -> Task<Message> {
        match visible.get(index) {
            Some(id) => action(self, *id),
            None => Task::none(),
        }
    }

    fn clamp_highlight(&mut self) {
        let visible = view::visible(self).len();
        self.highlight = self.highlight.min(visible.saturating_sub(1));
    }

    fn newest(&self) -> usize {
        view::visible(self)
            .iter()
            .position(|entry| entry.pinned.is_none())
            .unwrap_or(0)
    }
}
