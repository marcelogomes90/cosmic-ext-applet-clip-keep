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

const PANEL_ICON: &str = "io.github.marcelogomes90.cosmic-ext-applet-clip-keep-symbolic";

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
    hovered: Option<EntryId>,
    scroll: f32,
    showing_settings: bool,
    thumbs: Thumbs,
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
                hovered: None,
                scroll: 0.0,
                showing_settings: false,
                thumbs: Thumbs::default(),
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
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let button = self
            .core
            .applet
            .icon_button(self.panel_icon())
            .on_press(Message::TogglePopup);

        self.core.applet.autosize_window(button).into()
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
                self.showing_settings = false;
                Task::none()
            }
            Message::Surface(cosmic::surface::Action::Task(task)) => {
                let Some(parent) = self.popup_id() else {
                    return Task::none();
                };

                task()
                    .map(move |action| cosmic::Action::App(Message::DelayedSurface(parent, action)))
            }
            Message::Surface(action) => cosmic::surface::surface_task(action),
            Message::DelayedSurface(parent, action) => {
                if self.popup_id() == Some(parent) {
                    cosmic::surface::surface_task(action)
                } else {
                    Task::none()
                }
            }
            Message::Scrolled(offset) => {
                self.scroll = offset;
                Task::none()
            }
            Message::Hover(id) => {
                self.hovered = Some(id);
                Task::none()
            }
            Message::Unhover(id) => {
                if self.hovered == Some(id) {
                    self.hovered = None;
                }
                Task::none()
            }
            Message::Confirm(id) => self.confirm(id),
            Message::TogglePin(id) => self.toggle_pin(id),
            Message::Delete(id) => self.delete(id),
            Message::Clear => {
                self.thumbs.clear();
                self.clip.send(ClipCommand::Clear {
                    include_pinned: false,
                });
                Task::none()
            }
            Message::ThumbnailLoaded(id, thumbnail) => {
                let handle = thumbnail
                    .map(|thumbnail| cosmic::widget::image::Handle::from_bytes(thumbnail.png));
                self.thumbs.insert(id, handle);
                Task::none()
            }
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
        self.hovered = None;
        self.scroll = 0.0;
        self.showing_settings = false;
    }

    pub(crate) fn hovered(&self) -> Option<EntryId> {
        self.hovered
    }

    pub(crate) fn scroll(&self) -> f32 {
        self.scroll
    }

    pub(crate) fn popup_id(&self) -> Option<window::Id> {
        match self.popup {
            PopupState::Open { id, closing: false } => Some(id),
            PopupState::Open { closing: true, .. } | PopupState::Closed => None,
        }
    }

    pub(crate) fn thumbs(&self) -> &Thumbs {
        &self.thumbs
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
        self.clip.send(ClipCommand::Delete(id));
        Task::none()
    }
}
