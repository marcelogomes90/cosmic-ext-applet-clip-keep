use std::sync::LazyLock;

use cosmic::Element;
use cosmic::iced::advanced::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use cosmic::iced::{Alignment, Length, Rectangle};
use cosmic::widget;

use super::ClipKeep;
use super::message::{Message, RowAction};
use crate::clip::model::{CaptureState, EntryId, EntryKind, EntryMeta, Timestamp, truncate_chars};
use crate::clip::search;
use crate::clip::settings::{MAX_ENTRIES_CEILING, Settings};
use crate::fl;

pub const VISIBLE_ROWS: usize = 50;

const PAD: u16 = 16;
const PAD_ROW_H: u16 = 12;
const LIST_INSET: u16 = 16;
const GAP: u16 = 8;
const GAP_TIGHT: u16 = 4;

const THUMBNAIL_HEIGHT: u16 = 32;
const ACTION_HINT_HEIGHT: f32 = 32.0;

const DETAILS_CHARS: usize = 400;
const DETAILS_VERTICAL_PADDING: u16 = 12;
const DETAILS_LABEL_WIDTH: f32 = 96.0;

const DETAILS_IMAGE_WIDTH: u16 = 256;
const DETAILS_IMAGE_HEIGHT: u16 = 192;

pub(crate) static SEARCH_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("clip-keep-search"));

pub(crate) static SCROLL_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("clip-keep-list"));

pub(crate) fn row_id(entry: EntryId) -> widget::Id {
    widget::Id::new(format!("clip-keep-row-{}", entry.0))
}

pub fn visible(app: &ClipKeep) -> Vec<&EntryMeta> {
    let entries = &app.snapshot().entries;
    search::filter(entries.iter().map(EntryMeta::label), app.query())
        .into_iter()
        .filter_map(|index| entries.get(index))
        .take(VISIBLE_ROWS)
        .collect()
}

const KEY_PIN: &str = "Ctrl+P";
const KEY_DELETE: &str = "Ctrl+D";
const KEY_INFO: &str = "Ctrl+I";
const KEY_SEARCH: &str = "Ctrl+F";
pub const SURFACE_WIDTH: f32 = 360.0;
const SURFACE_MAX_HEIGHT: f32 = 800.0;

static SURFACE_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("clip-keep-popup"));
static HINT_POPOVER_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("clip-keep-action-hint"));

pub fn popup(app: &ClipKeep) -> Element<'_, Message> {
    let body: Element<'_, Message> = if let Some(entry) = app.details() {
        details_page(app, entry)
    } else if app.showing_settings() {
        settings_page(app)
    } else {
        history_page(app)
    };

    let surface: Element<'_, Message> = widget::container(body)
        .width(Length::Fixed(SURFACE_WIDTH))
        .style(surface_style)
        .into();

    // Keep the popover in the widget tree even while it has no popup. Swapping the
    // root widget as the pointer moved reset hover state in the action buttons.
    let mut content = widget::popover(surface).id(HINT_POPOVER_ID.clone());

    if let Some((id, action, bounds)) = app.action_hint()
        && app.details().is_none()
        && !app.showing_settings()
    {
        let (label, shortcut) = action_hint_text(app, id, action);
        let hint = widget::container(
            widget::row::with_children(vec![
                widget::text::body(label).into(),
                widget::text::caption(shortcut).into(),
            ])
            .spacing(GAP)
            .align_y(Alignment::Center),
        )
        .padding(cosmic::theme::spacing().space_xxs)
        .class(cosmic::theme::Container::Tooltip);

        content = content
            .position(widget::popover::Position::Point(cosmic::iced::Point::new(
                bounds.x,
                (bounds.y - ACTION_HINT_HEIGHT).max(0.0),
            )))
            .popup(hint);
    }

    widget::autosize::autosize(content, SURFACE_ID.clone())
        .limits(
            cosmic::iced::Limits::NONE
                .min_width(1.0)
                .max_width(SURFACE_WIDTH)
                .min_height(1.0)
                .max_height(SURFACE_MAX_HEIGHT),
        )
        .into()
}

fn action_hint_text(app: &ClipKeep, id: EntryId, action: RowAction) -> (String, &'static str) {
    match action {
        RowAction::Details => (fl!("action-details"), KEY_INFO),
        RowAction::Pin => {
            let pinned = app
                .snapshot()
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .is_some_and(|entry| entry.pinned.is_some());
            (
                if pinned {
                    fl!("action-unpin")
                } else {
                    fl!("action-pin")
                },
                KEY_PIN,
            )
        }
        RowAction::Delete => (fl!("action-delete"), KEY_DELETE),
    }
}

fn surface_style(theme: &cosmic::Theme) -> widget::container::Style {
    let cosmic = theme.cosmic();
    let background = cosmic.background(theme.transparent);

    widget::container::Style {
        text_color: Some(background.on.into()),
        icon_color: Some(background.on.into()),
        background: Some(cosmic::iced::Color::from(background.base).into()),
        border: cosmic::iced::Border {
            radius: cosmic.corner_radii.radius_m.into(),
            width: 1.0,
            color: background.divider.into(),
        },
        shadow: cosmic::iced::Shadow::default(),
        snap: true,
    }
}

fn history_page(app: &ClipKeep) -> Element<'_, Message> {
    let rows = visible(app);
    let search = widget::text_input::search_input(
        fl!("search-placeholder", shortcut = KEY_SEARCH),
        app.query(),
    )
    .id(SEARCH_ID.clone())
    .width(Length::Fill)
    .on_input(Message::Search)
    .on_clear(Message::Search(String::new()));
    let clear = widget::button::icon(
        widget::icon::from_name("user-trash-full-symbolic")
            .size(16)
            .symbolic(true),
    )
    .on_press_maybe(
        app.snapshot()
            .entries
            .iter()
            .any(|entry| entry.pinned.is_none())
            .then_some(Message::Clear),
    );
    let settings = widget::button::icon(
        widget::icon::from_name("emblem-system-symbolic")
            .size(16)
            .symbolic(true),
    )
    .on_press(Message::ShowSettings(true));
    let controls = widget::row::with_children(vec![clear.into(), settings.into()]);
    let header = widget::row::with_children(vec![search.into(), controls.into()])
        .spacing(GAP)
        .align_y(Alignment::Center);

    let mut children: Vec<Element<'_, Message>> =
        vec![widget::container(header).padding(PAD).into()];

    if let Some(notice) = capture_notice(app) {
        children.push(notice);
    }

    if rows.is_empty() {
        children.push(empty_state(app));
    } else {
        let mut pinned = Vec::new();
        let mut regular = Vec::new();

        for entry in rows {
            let item = row(app, entry);
            if entry.pinned.is_some() {
                pinned.push(item);
            } else {
                regular.push(item);
            }
        }

        let mut sections = Vec::new();
        if !pinned.is_empty() {
            sections.push(list_section("pin-symbolic", fl!("section-pinned"), pinned));
        }
        if !regular.is_empty() {
            if !sections.is_empty() {
                sections.push(
                    widget::container(divider())
                        .padding([GAP, PAD])
                        .width(Length::Fill)
                        .into(),
                );
            }
            sections.push(list_section(
                "document-open-recent-symbolic",
                fl!("section-recent"),
                regular,
            ));
        }

        let list = widget::container(widget::column::with_children(sections))
            .padding([0, 0, PAD, 0])
            .width(Length::Fill);

        children.push(widget::container(scroll(list)).width(Length::Fill).into());
    }

    widget::column::with_children(children).into()
}

fn list_section<'a>(
    glyph: &'a str,
    title: String,
    rows: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let heading = widget::container(
        widget::row::with_children(vec![
            widget::icon::from_name(glyph)
                .size(14)
                .symbolic(true)
                .icon()
                .into(),
            widget::text::heading(title).into(),
        ])
        .spacing(GAP)
        .align_y(Alignment::Center),
    )
    .padding([GAP, PAD, GAP, PAD + PAD_ROW_H])
    .width(Length::Fill);
    let mut children: Vec<Element<'_, Message>> = vec![heading.into()];
    children.extend(rows);
    widget::column::with_children(children).into()
}

fn capture_notice(app: &ClipKeep) -> Option<Element<'_, Message>> {
    let CaptureState::Unavailable { reason } = &app.snapshot().capture else {
        return None;
    };

    let lines: Vec<Element<'_, Message>> = vec![
        widget::text::body(fl!("capture-unavailable")).into(),
        widget::text::caption(reason.clone()).into(),
    ];

    Some(
        widget::container(widget::column::with_children(lines).spacing(GAP_TIGHT))
            .padding([GAP, PAD, PAD, PAD])
            .width(Length::Fill)
            .into(),
    )
}

fn empty_state(app: &ClipKeep) -> Element<'_, Message> {
    let message = if app.query().is_empty() {
        fl!("empty-history")
    } else {
        fl!("no-results")
    };

    widget::container(widget::text::body(message))
        .center_x(Length::Fill)
        .padding([PAD, PAD, PAD * 2, PAD])
        .into()
}

fn row<'a>(app: &'a ClipKeep, entry: &'a EntryMeta) -> Element<'a, Message> {
    let active = app.focused() == Some(entry.id);

    let button = widget::button::custom(content(app, entry))
        .class(quiet())
        .padding([GAP, PAD_ROW_H])
        .width(Length::Fill)
        .on_press(Message::Confirm(entry.id));

    let actions = widget::row::with_children(vec![
        action_toggle(entry.id, RowAction::Details, false),
        action_toggle(entry.id, RowAction::Pin, entry.pinned.is_some()),
        action_toggle(entry.id, RowAction::Delete, false),
    ])
    .align_y(Alignment::Center);

    let controls: Vec<Element<'a, Message>> = vec![button.into(), actions.into()];

    let inner = widget::container(
        widget::row::with_children(controls)
            .spacing(GAP_TIGHT)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .class(if active {
        active_row()
    } else {
        cosmic::theme::Container::Transparent
    });

    let row = widget::container(inner)
        .id(row_id(entry.id))
        .padding([0, PAD])
        .width(Length::Fill);

    widget::mouse_area(row)
        .on_move(move |_| Message::Focus(entry.id))
        .into()
}

#[derive(Clone)]
struct Details {
    text: String,
    source_app: Option<String>,
    created_at: Timestamp,
    last_used_at: Timestamp,
    use_count: u32,
    byte_size: u64,
    image_size: Option<(u32, u32)>,
    image: Option<widget::image::Handle>,
}

impl Details {
    fn of(app: &ClipKeep, entry: &EntryMeta) -> Self {
        let is_image = entry.kind == EntryKind::Image;

        Self {
            text: details_text(&label_for(entry)),
            source_app: entry.source_app.clone(),
            created_at: entry.created_at,
            last_used_at: entry.last_used_at,
            use_count: entry.use_count,
            byte_size: entry.byte_size,
            image_size: is_image.then_some(entry.image_size).flatten(),
            image: is_image
                .then(|| app.thumbs().get(entry.id).cloned())
                .flatten(),
        }
    }

    fn view(self) -> Element<'static, Message> {
        let mut rows: Vec<Element<'static, Message>> = Vec::new();

        let summary = match self.image.zip(self.image_size) {
            Some((handle, size)) => Some(preview(handle, size)),
            None => self.image_size.is_none().then(|| excerpt(self.text)),
        };

        if let Some(summary) = summary {
            rows.push(summary);
            rows.push(
                widget::container(widget::divider::horizontal::default())
                    .padding([0, PAD])
                    .into(),
            );
        }

        rows.push(
            widget::container(
                widget::column::with_children(
                    [
                        self.source_app
                            .map(|app| detail(fl!("details-source"), application(&app))),
                        self.image_size
                            .map(|(w, h)| detail(fl!("details-size"), format!("{w} × {h}"))),
                        Some(detail(fl!("details-copied"), moment(self.created_at))),
                        Some(detail(fl!("details-used"), moment(self.last_used_at))),
                        Some(detail(fl!("details-copies"), self.use_count.to_string())),
                        Some(detail(fl!("details-bytes"), bytes(self.byte_size))),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>(),
                )
                .spacing(GAP_TIGHT),
            )
            .padding(PAD)
            .into(),
        );

        widget::column::with_children(rows)
            .width(Length::Fill)
            .into()
    }
}

fn excerpt(text: String) -> Element<'static, Message> {
    widget::container(
        widget::text::body(text)
            .wrapping(Wrapping::WordOrGlyph)
            .width(Length::Fill),
    )
    .padding([DETAILS_VERTICAL_PADDING, PAD])
    .into()
}

fn preview(handle: widget::image::Handle, size: (u32, u32)) -> Element<'static, Message> {
    let (width, height) = crate::clip::thumbnail::fit_within(
        size.0,
        size.1,
        DETAILS_IMAGE_WIDTH,
        DETAILS_IMAGE_HEIGHT,
    );

    widget::container(
        widget::image(handle)
            .width(Length::Fixed(pixels(width)))
            .height(Length::Fixed(pixels(height))),
    )
    .center_x(Length::Fill)
    .padding([DETAILS_VERTICAL_PADDING, PAD])
    .into()
}

fn detail(name: String, value: String) -> Element<'static, Message> {
    widget::row::with_children(vec![
        widget::text::caption(name)
            .width(Length::Fixed(DETAILS_LABEL_WIDTH))
            .into(),
        widget::text::caption(value).width(Length::Fill).into(),
    ])
    .into()
}

fn moment(at: Timestamp) -> String {
    jiff::Timestamp::from_millisecond(at)
        .map(|stamp| {
            stamp
                .to_zoned(jiff::tz::TimeZone::system())
                .strftime(&fl!("details-moment-format"))
                .to_string()
        })
        .unwrap_or_default()
}

fn application(id: &str) -> String {
    let tail = id.rsplit('.').next().unwrap_or(id);
    let mut name = String::with_capacity(tail.len() + 4);

    for (index, character) in tail.chars().enumerate() {
        if index == 0 {
            name.extend(character.to_uppercase());
            continue;
        }

        if character.is_uppercase() && !name.ends_with(' ') {
            name.push(' ');
        }
        name.push(character);
    }

    name
}

fn bytes(size: u64) -> String {
    const UNITS: [&str; 4] = ["B", "kB", "MB", "GB"];

    let mut whole = size;
    let mut remainder = 0;
    let mut unit = 0;

    while whole >= 1024 && unit + 1 < UNITS.len() {
        remainder = whole % 1024;
        whole /= 1024;
        unit += 1;
    }

    if unit == 0 {
        format!("{whole} {}", UNITS[0])
    } else {
        format!("{whole}.{} {}", remainder * 10 / 1024, UNITS[unit])
    }
}

fn content<'a>(app: &'a ClipKeep, entry: &'a EntryMeta) -> Element<'a, Message> {
    if entry.kind == EntryKind::Image
        && let Some(handle) = app.thumbs().get(entry.id)
    {
        let (width, height) = entry.image_size.map_or(
            (THUMBNAIL_HEIGHT.into(), THUMBNAIL_HEIGHT.into()),
            |(w, h)| crate::clip::thumbnail::fit(w, h, THUMBNAIL_HEIGHT),
        );

        return widget::container(
            widget::image(handle.clone())
                .width(Length::Fixed(pixels(width)))
                .height(Length::Fixed(pixels(height))),
        )
        .width(Length::Fill)
        .into();
    }

    widget::text::body(one_line(&label_for(entry)))
        .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
        .width(Length::Fill)
        .into()
}

fn quiet() -> cosmic::theme::Button {
    fn plain(_: bool, _: &cosmic::Theme) -> cosmic::widget::button::Style {
        cosmic::widget::button::Style::new()
    }

    cosmic::theme::Button::Custom {
        active: Box::new(plain),
        disabled: Box::new(|theme| plain(false, theme)),
        hovered: Box::new(plain),
        pressed: Box::new(plain),
    }
}

fn active_row<'a>() -> cosmic::theme::Container<'a> {
    cosmic::theme::Container::Custom(Box::new(|theme: &cosmic::Theme| {
        let cosmic = theme.cosmic();
        widget::container::Style {
            background: Some(
                cosmic::iced::Color::from(cosmic.primary(theme.transparent).component.hover).into(),
            ),
            border: cosmic::iced::Border {
                radius: cosmic.corner_radii.radius_m.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }))
}

pub(crate) fn action_id(id: EntryId, action: RowAction) -> widget::Id {
    let action = match action {
        RowAction::Details => "details",
        RowAction::Pin => "pin",
        RowAction::Delete => "delete",
    };
    widget::Id::new(format!("clip-keep-action-{action}-{}", id.0))
}

fn action_toggle(id: EntryId, action: RowAction, selected: bool) -> Element<'static, Message> {
    let (glyph, message) = match action {
        RowAction::Details => (
            "dialog-information-symbolic",
            Message::ShowDetails(Some(id)),
        ),
        RowAction::Pin => ("pin-symbolic", Message::TogglePin(id)),
        RowAction::Delete => ("user-trash-symbolic", Message::Delete(id)),
    };
    let button = widget::button::icon(widget::icon::from_name(glyph).size(14).symbolic(true))
        .class(flat_icon(selected))
        .on_press(message);

    widget::mouse_area(widget::container(button).id(action_id(id, action)))
        .on_enter(Message::PrepareActionHint(id, action))
        .into()
}

fn flat_icon(selected: bool) -> cosmic::theme::Button {
    fn paint(theme: &cosmic::Theme, selected: bool) -> cosmic::widget::button::Style {
        let mut style = cosmic::widget::button::Style::new();

        if selected {
            let accent = theme.cosmic().accent_text_color().into();
            style.icon_color = Some(accent);
            style.text_color = Some(accent);
        }

        style
    }

    cosmic::theme::Button::Custom {
        active: Box::new(move |_, theme| paint(theme, selected)),
        disabled: Box::new(move |theme| paint(theme, selected)),
        hovered: Box::new(move |_, theme| paint(theme, selected)),
        pressed: Box::new(move |_, theme| paint(theme, selected)),
    }
}

fn scroll<'a>(
    content: impl Into<Element<'a, Message>>,
) -> cosmic::iced::widget::Scrollable<'a, Message, cosmic::Theme, cosmic::Renderer> {
    widget::scrollable(content)
        .id(SCROLL_ID.clone())
        .scrollbar_width(0.0)
        .scroller_width(0.0)
        .scrollbar_padding(0.0)
}

pub(crate) fn scroll_into_view(row: widget::Id) -> impl widget::Operation<Option<f32>> {
    struct Reveal {
        row: widget::Id,
        viewport: Option<(Rectangle, f32)>,
        bounds: Option<Rectangle>,
    }

    impl widget::Operation<Option<f32>> for Reveal {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation<Option<f32>>)) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&widget::Id>,
            bounds: Rectangle,
            _content: Rectangle,
            translation: cosmic::iced::Vector,
            _state: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&*SCROLL_ID) {
                self.viewport = Some((bounds, translation.y));
            }
        }

        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if id == Some(&self.row) {
                self.bounds = Some(bounds);
            }
        }

        fn finish(&self) -> cosmic::iced::advanced::widget::operation::Outcome<Option<f32>> {
            use cosmic::iced::advanced::widget::operation::Outcome;

            let (Some((viewport, scrolled)), Some(row)) = (self.viewport, self.bounds) else {
                return Outcome::None;
            };

            Outcome::Some(shortfall(viewport, row, scrolled))
        }
    }

    Reveal {
        row,
        viewport: None,
        bounds: None,
    }
}

pub(crate) fn onscreen_bounds(target: widget::Id) -> impl widget::Operation<Option<Rectangle>> {
    struct Locate {
        target: widget::Id,
        scrolled: Option<f32>,
        bounds: Option<Rectangle>,
    }

    impl widget::Operation<Option<Rectangle>> for Locate {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn widget::Operation<Option<Rectangle>>),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&widget::Id>,
            _bounds: Rectangle,
            _content: Rectangle,
            translation: cosmic::iced::Vector,
            _state: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&*SCROLL_ID) {
                self.scrolled = Some(translation.y);
            }
        }

        fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
            if id == Some(&self.target) {
                self.bounds = Some(bounds);
            }
        }

        fn finish(&self) -> cosmic::iced::advanced::widget::operation::Outcome<Option<Rectangle>> {
            use cosmic::iced::advanced::widget::operation::Outcome;

            Outcome::Some(
                self.bounds
                    .zip(self.scrolled)
                    .map(|(mut bounds, scrolled)| {
                        bounds.y -= scrolled;
                        bounds
                    }),
            )
        }
    }

    Locate {
        target,
        scrolled: None,
        bounds: None,
    }
}

fn shortfall(viewport: Rectangle, row: Rectangle, scrolled: f32) -> Option<f32> {
    let top = row.y - scrolled;
    let above = viewport.y - top;
    let below = (top + row.height) - (viewport.y + viewport.height);

    if above > 0.0 {
        Some(-above)
    } else if below > 0.0 {
        Some(below)
    } else {
        None
    }
}

fn pixels(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn one_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().to_owned()
}

fn details_text(text: &str) -> String {
    if text.chars().nth(DETAILS_CHARS).is_none() {
        return text.to_owned();
    }

    let mut truncated = truncate_chars(text, DETAILS_CHARS - 3);
    truncated.push_str("...");
    truncated
}

fn label_for(entry: &EntryMeta) -> String {
    match (entry.kind, entry.image_size) {
        (EntryKind::Image, Some((width, height))) => {
            fl!("item-image", width = width, height = height)
        }
        _ => entry.label().to_owned(),
    }
}

fn divider<'a>() -> Element<'a, Message> {
    widget::divider::horizontal::default().into()
}

fn details_page<'a>(app: &'a ClipKeep, entry: &'a EntryMeta) -> Element<'a, Message> {
    let back = widget::button::icon(
        widget::icon::from_name("go-previous-symbolic")
            .size(16)
            .symbolic(true),
    )
    .on_press(Message::ShowDetails(None));

    let header = widget::row::with_children(vec![
        back.into(),
        widget::text::heading(fl!("details")).into(),
    ])
    .spacing(GAP)
    .align_y(Alignment::Center);

    widget::column::with_children(vec![
        widget::container(header).padding(PAD).into(),
        widget::container(divider()).padding([0, PAD]).into(),
        scroll(Details::of(app, entry).view()).into(),
    ])
    .into()
}

fn settings_page(app: &ClipKeep) -> Element<'_, Message> {
    let back = widget::button::icon(
        widget::icon::from_name("go-previous-symbolic")
            .size(16)
            .symbolic(true),
    )
    .on_press(Message::ShowSettings(false));

    let header = widget::row::with_children(vec![
        back.into(),
        widget::text::heading(fl!("settings")).into(),
    ])
    .spacing(GAP)
    .align_y(Alignment::Center);

    let sections = widget::column::with_children(vec![
        section(fl!("section-history"), history_controls(app)),
        section(fl!("section-privacy"), privacy_controls(app)),
        section(fl!("section-behaviour"), behaviour_controls(app)),
    ])
    .spacing(PAD);

    widget::column::with_children(vec![
        widget::container(header).padding(PAD).into(),
        widget::container(divider()).padding([0, PAD]).into(),
        scroll(widget::container(sections).padding(PAD)).into(),
    ])
    .into()
}

fn card<'a>() -> widget::ListColumn<'a, Message> {
    widget::list_column().list_item_padding([cosmic::theme::spacing().space_xxs, LIST_INSET])
}

fn section(title: String, controls: Element<'_, Message>) -> Element<'_, Message> {
    widget::column::with_children(vec![widget::text::heading(title).into(), controls])
        .spacing(PAD_ROW_H)
        .into()
}

fn edited(app: &ClipKeep, change: impl FnOnce(&mut Settings)) -> Message {
    let mut next = app.settings().clone();
    change(&mut next);
    Message::Setting(Box::new(next))
}

fn setting_row<'a>(
    label: String,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    widget::row::with_children(vec![
        widget::text::body(label).width(Length::Fill).into(),
        control.into(),
    ])
    .spacing(GAP)
    .align_y(Alignment::Center)
    .into()
}

fn toggle(
    app: &ClipKeep,
    label: String,
    value: bool,
    change: fn(&mut Settings, bool),
) -> Element<'_, Message> {
    let message = edited(app, |settings| change(settings, !value));

    setting_row(
        label,
        widget::toggler(value).on_toggle(move |_| message.clone()),
    )
}

fn privacy_controls(app: &ClipKeep) -> Element<'_, Message> {
    let settings = app.settings();

    card()
        .add(toggle(
            app,
            fl!("setting-private-mode"),
            settings.private_mode,
            |settings, value| settings.private_mode = value,
        ))
        .add(toggle(
            app,
            fl!("setting-respect-password-hint"),
            settings.respect_password_hint,
            |settings, value| settings.respect_password_hint = value,
        ))
        .into()
}

fn history_controls(app: &ClipKeep) -> Element<'_, Message> {
    let settings = app.settings();

    let base = settings.clone();
    let entries = widget::spin_button(
        settings.max_entries.to_string(),
        settings.max_entries,
        50,
        50,
        MAX_ENTRIES_CEILING,
        move |value| {
            let mut next = base.clone();
            next.max_entries = value;
            Message::Setting(Box::new(next))
        },
    );

    card()
        .add(setting_row(fl!("setting-max-entries"), entries))
        .add(retention(app))
        .into()
}

fn retention(app: &ClipKeep) -> Element<'_, Message> {
    let current = app.settings().max_age_days;
    let options = retention_options(current);
    let selected = options.iter().position(|(days, _)| *days == current);
    let values: Vec<Option<u32>> = options.iter().map(|(days, _)| *days).collect();
    let labels: Vec<String> = options.into_iter().map(|(_, label)| label).collect();
    let base = app.settings().clone();

    setting_row(
        fl!("setting-max-age"),
        widget::dropdown(labels, selected, move |index| {
            let mut next = base.clone();
            next.max_age_days = values.get(index).copied().unwrap_or(current);
            Message::Setting(Box::new(next))
        }),
    )
}

fn behaviour_controls(app: &ClipKeep) -> Element<'_, Message> {
    let settings = app.settings();

    card()
        .add(toggle(
            app,
            fl!("setting-capture-images"),
            settings.capture_images,
            |settings, value| settings.capture_images = value,
        ))
        .add(toggle(
            app,
            fl!("setting-paste-on-use"),
            settings.paste_on_use,
            |settings, value| settings.paste_on_use = value,
        ))
        .into()
}

fn retention_options(current: Option<u32>) -> Vec<(Option<u32>, String)> {
    let mut options: Vec<Option<u32>> = vec![None, Some(1), Some(7), Some(30)];
    if !options.contains(&current) {
        options.push(current);
    }
    options.sort_unstable_by_key(|days| days.unwrap_or(0));

    options
        .into_iter()
        .map(|days| {
            let label = match days {
                None => fl!("setting-max-age-never"),
                Some(days) => fl!("setting-max-age-days", days = days),
            };
            (days, label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_retention_presets_start_at_never_and_climb() {
        let options: Vec<Option<u32>> = retention_options(Some(30))
            .into_iter()
            .map(|(days, _)| days)
            .collect();

        assert_eq!(options, [None, Some(1), Some(7), Some(30)]);
    }

    #[test]
    fn a_hand_written_value_joins_the_presets_in_order() {
        let options: Vec<Option<u32>> = retention_options(Some(14))
            .into_iter()
            .map(|(days, _)| days)
            .collect();

        assert_eq!(options, [None, Some(1), Some(7), Some(14), Some(30)]);
    }

    #[test]
    fn short_details_are_left_unchanged() {
        assert_eq!(details_text("um texto curto"), "um texto curto");
    }

    #[test]
    fn long_details_are_ellipsized_at_four_hundred_characters() {
        let text = "á".repeat(DETAILS_CHARS + 20);
        let result = details_text(&text);

        assert_eq!(result.chars().count(), DETAILS_CHARS);
        assert!(result.ends_with("..."));
    }

    fn rect(y: f32, height: f32) -> Rectangle {
        Rectangle {
            x: 0.0,
            y,
            width: SURFACE_WIDTH,
            height,
        }
    }

    #[test]
    fn a_row_already_inside_the_viewport_is_left_alone() {
        assert_eq!(shortfall(rect(100.0, 400.0), rect(180.0, 40.0), 0.0), None);
    }

    #[test]
    fn a_row_above_the_viewport_scrolls_back_by_the_difference() {
        assert_eq!(
            shortfall(rect(100.0, 400.0), rect(70.0, 40.0), 0.0),
            Some(-30.0)
        );
    }

    #[test]
    fn a_row_below_the_viewport_scrolls_on_by_the_difference() {
        assert_eq!(
            shortfall(rect(100.0, 400.0), rect(480.0, 40.0), 0.0),
            Some(20.0)
        );
    }

    #[test]
    fn a_row_flush_with_the_bottom_edge_is_already_visible() {
        assert_eq!(shortfall(rect(100.0, 400.0), rect(460.0, 40.0), 0.0), None);
    }

    #[test]
    fn a_row_is_measured_where_the_scroll_has_put_it_on_screen() {
        let viewport = rect(100.0, 400.0);

        assert_eq!(shortfall(viewport, rect(1180.0, 40.0), 1000.0), None);
        assert_eq!(shortfall(viewport, rect(1070.0, 40.0), 1000.0), Some(-30.0));
        assert_eq!(shortfall(viewport, rect(1480.0, 40.0), 1000.0), Some(20.0));
    }

    #[test]
    fn walking_back_to_the_top_scrolls_all_the_way_up() {
        let viewport = rect(100.0, 400.0);

        assert_eq!(
            shortfall(viewport, rect(100.0, 40.0), 1000.0),
            Some(-1000.0),
            "the first row sits where the content starts, so it comes back by the whole offset"
        );
    }
}
