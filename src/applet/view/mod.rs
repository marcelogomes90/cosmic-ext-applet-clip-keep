use std::sync::LazyLock;

use cosmic::Element;
use cosmic::cctk::sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor, Gravity,
};
use cosmic::iced::advanced::text::{Ellipsize, EllipsizeHeightLimit};
use cosmic::iced::platform_specific::runtime::wayland::popup::{SctkPopupSettings, SctkPositioner};
use cosmic::iced::{Alignment, Length, Limits, Rectangle, window};
use cosmic::widget;
use cosmic::widget::wayland::tooltip::widget::Tooltip;

use super::ClipKeep;
use super::message::Message;
use crate::clip::model::{CaptureState, EntryKind, EntryMeta, Timestamp};
use crate::clip::search;
use crate::clip::settings::{MAX_ENTRIES_CEILING, Settings};
use crate::fl;

pub const VISIBLE_ROWS: usize = 50;

const PAD: u16 = 16;
const PAD_ROW_H: u16 = 12;
const GAP: u16 = 8;
const GAP_TIGHT: u16 = 4;

const THUMBNAIL_HEIGHT: u16 = 32;

const CARD_ENABLED: bool = true;
const CARD_WIDTH: f32 = 300.0;
const CARD_LABEL_WIDTH: f32 = 96.0;
const CARD_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

static CARD_WINDOW_ID: LazyLock<window::Id> = LazyLock::new(window::Id::unique);
static CARD_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("clip-keep-card"));

pub(crate) static SEARCH_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("clip-keep-search"));

pub fn visible(app: &ClipKeep) -> Vec<&EntryMeta> {
    let entries = &app.snapshot().entries;
    search::filter(entries.iter().map(EntryMeta::label), app.query())
        .into_iter()
        .filter_map(|index| entries.get(index))
        .take(VISIBLE_ROWS)
        .collect()
}

pub const SURFACE_WIDTH: f32 = 360.0;
const SURFACE_MAX_HEIGHT: f32 = 588.0;

static SURFACE_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("clip-keep-popup"));

pub fn popup(app: &ClipKeep) -> Element<'_, Message> {
    let body: Element<'_, Message> = if app.showing_settings() {
        settings_page(app)
    } else {
        history_page(app)
    };

    let surface = widget::container(body)
        .width(Length::Fixed(SURFACE_WIDTH))
        .style(surface_style);

    widget::autosize::autosize(surface, SURFACE_ID.clone())
        .limits(
            cosmic::iced::Limits::NONE
                .min_width(1.0)
                .max_width(SURFACE_WIDTH)
                .min_height(1.0)
                .max_height(SURFACE_MAX_HEIGHT),
        )
        .into()
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
    let search = widget::text_input::search_input(fl!("search-placeholder"), app.query())
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

        children.push(
            widget::container(
                scroll(list).on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y)),
            )
            .width(Length::Fill)
            .into(),
        );
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
    let active = app.hovered() == Some(entry.id);

    let button = widget::button::custom(content(app, entry))
        .class(quiet())
        .padding([GAP, PAD_ROW_H])
        .width(Length::Fill)
        .on_press(Message::Confirm(entry.id));

    let actions = widget::row::with_children(vec![
        action_toggle(
            "pin-symbolic",
            entry.pinned.is_some(),
            Message::TogglePin(entry.id),
        ),
        action_toggle("user-trash-symbolic", false, Message::Delete(entry.id)),
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
        .padding([0, PAD])
        .width(Length::Fill);

    let hover = widget::mouse_area(row)
        .on_enter(Message::Hover(entry.id))
        .on_exit(Message::Unhover(entry.id));

    let Some(parent) = app.popup_id().filter(|_| CARD_ENABLED) else {
        return hover.into();
    };

    let card = Card::of(entry);
    let scrolled = app.scroll();

    Tooltip::new(
        hover,
        Some(move |bounds: Rectangle| SctkPopupSettings {
            parent,
            id: *CARD_WINDOW_ID,
            grab: false,
            input_zone: Some(vec![Rectangle::new(
                cosmic::iced::Point::new(-1000., -1000.),
                cosmic::iced::Size::default(),
            )]),
            positioner: SctkPositioner {
                size: None,
                size_limits: Limits::NONE.min_width(1.).min_height(1.),
                anchor_rect: Rectangle {
                    x: whole(bounds.x),
                    y: whole((bounds.y - scrolled).clamp(0.0, SURFACE_MAX_HEIGHT - bounds.height)),
                    width: whole(bounds.width),
                    height: whole(bounds.height),
                },
                anchor: Anchor::Left,
                gravity: Gravity::Left,
                constraint_adjustment: 15,
                offset: (-i32::from(GAP), 0),
                reactive: true,
            },
            parent_size: None,
            close_with_children: true,
        }),
        move || card.clone().view(),
        Message::Surface(cosmic::surface::Action::DestroyPopup(*CARD_WINDOW_ID)),
        Message::Surface,
    )
    .width(Length::Fill)
    .delay(CARD_DELAY)
    .into()
}

#[derive(Clone)]
struct Card {
    text: String,
    source_app: Option<String>,
    created_at: Timestamp,
    last_used_at: Timestamp,
    use_count: u32,
    byte_size: u64,
    image_size: Option<(u32, u32)>,
}

impl Card {
    fn of(entry: &EntryMeta) -> Self {
        Self {
            text: label_for(entry),
            source_app: entry.source_app.clone(),
            created_at: entry.created_at,
            last_used_at: entry.last_used_at,
            use_count: entry.use_count,
            byte_size: entry.byte_size,
            image_size: (entry.kind == EntryKind::Image)
                .then_some(entry.image_size)
                .flatten(),
        }
    }

    fn view(self) -> Element<'static, cosmic::Action<Message>> {
        let mut rows: Vec<Element<'static, cosmic::Action<Message>>> = Vec::new();

        if self.image_size.is_none() {
            rows.push(widget::text::body(self.text).into());
            rows.push(widget::divider::horizontal::default().into());
        }

        rows.push(
            widget::column::with_children(
                [
                    self.source_app
                        .map(|app| detail(fl!("card-source"), application(&app))),
                    self.image_size
                        .map(|(w, h)| detail(fl!("card-size"), format!("{w} × {h}"))),
                    Some(detail(fl!("card-copied"), moment(self.created_at))),
                    Some(detail(fl!("card-used"), moment(self.last_used_at))),
                    Some(detail(fl!("card-copies"), self.use_count.to_string())),
                    Some(detail(fl!("card-bytes"), bytes(self.byte_size))),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
            )
            .spacing(GAP_TIGHT)
            .into(),
        );

        widget::autosize::autosize(
            widget::container(
                widget::column::with_children(rows)
                    .spacing(GAP)
                    .width(Length::Fixed(CARD_WIDTH)),
            )
            .padding(PAD)
            .style(surface_style),
            CARD_ID.clone(),
        )
        .limits(
            Limits::NONE
                .min_width(1.0)
                .min_height(1.0)
                .max_width(CARD_WIDTH)
                .max_height(SURFACE_MAX_HEIGHT),
        )
        .into()
    }
}

fn detail(name: String, value: String) -> Element<'static, cosmic::Action<Message>> {
    widget::row::with_children(vec![
        widget::text::caption(name)
            .width(Length::Fixed(CARD_LABEL_WIDTH))
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
                .strftime(&fl!("card-moment-format"))
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

#[allow(clippy::cast_possible_truncation)]
fn whole(value: f32) -> i32 {
    value.round() as i32
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

    widget::text::body(label_for(entry))
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

fn action_toggle(glyph: &str, selected: bool, message: Message) -> Element<'_, Message> {
    widget::button::icon(widget::icon::from_name(glyph).size(14).symbolic(true))
        .class(flat_icon(selected))
        .on_press(message)
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
        .scrollbar_width(0.0)
        .scroller_width(0.0)
        .scrollbar_padding(0.0)
}

fn pixels(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
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
        section(fl!("section-privacy"), privacy_controls(app)),
        section(fl!("section-history"), history_controls(app)),
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

fn toggle(
    app: &ClipKeep,
    label: String,
    description: Option<String>,
    value: bool,
    change: fn(&mut Settings, bool),
) -> Element<'_, Message> {
    let message = edited(app, |settings| change(settings, !value));
    let mut item = widget::settings::item::builder(label);
    if let Some(description) = description {
        item = item.description(description);
    }

    item.control(widget::toggler(value).on_toggle(move |_| message.clone()))
        .into()
}

fn privacy_controls(app: &ClipKeep) -> Element<'_, Message> {
    let settings = app.settings();

    widget::list_column()
        .add(toggle(
            app,
            fl!("setting-private-mode"),
            None,
            settings.private_mode,
            |settings, value| settings.private_mode = value,
        ))
        .add(toggle(
            app,
            fl!("setting-respect-password-hint"),
            None,
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

    widget::list_column()
        .add(widget::settings::item(fl!("setting-max-entries"), entries))
        .add(
            widget::container(choice(
                app,
                fl!("setting-max-age"),
                settings.max_age_days,
                &retention_options(settings.max_age_days),
                |settings, value| settings.max_age_days = value,
            ))
            .padding([GAP, 0]),
        )
        .into()
}

fn behaviour_controls(app: &ClipKeep) -> Element<'_, Message> {
    let settings = app.settings();

    widget::list_column()
        .add(toggle(
            app,
            fl!("setting-capture-images"),
            None,
            settings.capture_images,
            |settings, value| settings.capture_images = value,
        ))
        .into()
}

fn choice<'a, T: Copy + Eq + 'static>(
    app: &'a ClipKeep,
    label: String,
    current: T,
    options: &[(T, String)],
    change: fn(&mut Settings, T),
) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = vec![widget::text::body(label).into()];

    for (value, name) in options {
        let value = *value;
        let message = edited(app, move |settings| change(settings, value));
        rows.push(
            widget::radio(
                widget::text::body(name.clone()),
                value,
                Some(current),
                move |_| message.clone(),
            )
            .into(),
        );
    }

    widget::column::with_children(rows).spacing(GAP).into()
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
}
