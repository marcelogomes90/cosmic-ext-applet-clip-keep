use std::sync::LazyLock;

use cosmic::Element;
use cosmic::iced::advanced::text::{Ellipsize, EllipsizeHeightLimit};
use cosmic::iced::{Alignment, Length};
use cosmic::widget;

use super::ClipKeep;
use super::Preview;
use super::message::Message;
use crate::clip::model::{CaptureState, EntryKind, EntryMeta, PREVIEW_CHARS};
use crate::clip::search;
use crate::clip::settings::{MAX_ENTRIES_CEILING, Settings};
use crate::fl;

pub const VISIBLE_ROWS: usize = 50;

const PAD: u16 = 16;
const PAD_ROW_H: u16 = 12;
const GAP: u16 = 8;
const GAP_TIGHT: u16 = 4;

const PREVIEW_HEIGHT: f32 = 180.0;

const PREVIEW_ICON: &[u8] = include_bytes!("../../../resources/icons/preview-symbolic.svg");
const THUMBNAIL_HEIGHT: u16 = 32;

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
const SURFACE_MAX_HEIGHT: f32 = 660.0;

static SURFACE_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("clip-keep-popup"));

pub fn popup(app: &ClipKeep) -> Element<'_, Message> {
    let body: Element<'_, Message> = if app.showing_settings() {
        settings_page(app)
    } else {
        history_page(app)
    };

    let surface = widget::container(body)
        .width(Length::Fixed(SURFACE_WIDTH))
        .style(|theme: &cosmic::Theme| {
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
        });

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

        for (index, entry) in rows.into_iter().enumerate() {
            let item = row(app, index, entry);
            if entry.pinned.is_some() {
                pinned.push(item);
            } else {
                regular.push(item);
            }
        }

        let mut sections = Vec::new();
        if !pinned.is_empty() {
            sections.push(list_section(fl!("section-pinned"), pinned));
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
            sections.push(list_section(fl!("section-recent"), regular));
        }

        let list = widget::container(widget::column::with_children(sections))
            .padding([0, 0, PAD, 0])
            .width(Length::Fill);

        let mut list = widget::container(scroll(list)).width(Length::Fill);
        if app.preview().is_some() {
            list = list.height(Length::Fill);
        }

        children.push(list.into());
    }

    if let Some(panel) = preview_panel(app) {
        children.push(divider());
        children.push(panel);
    }

    widget::column::with_children(children).into()
}

fn list_section(title: String, rows: Vec<Element<'_, Message>>) -> Element<'_, Message> {
    let heading = widget::container(widget::text::heading(title))
        .padding([GAP, PAD, GAP_TIGHT, PAD + PAD_ROW_H])
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

fn row<'a>(app: &'a ClipKeep, index: usize, entry: &'a EntryMeta) -> Element<'a, Message> {
    let highlighted = index == app.highlight();

    let button = widget::button::custom(content(app, entry, highlighted))
        .class(quiet())
        .padding([GAP, PAD_ROW_H])
        .width(Length::Fill)
        .on_press(Message::Confirm(entry.id));

    let mut controls: Vec<Element<'a, Message>> = vec![button.into()];

    if worth_previewing(entry) {
        let showing = app.preview().is_some_and(|(id, _)| id == entry.id);
        controls.push(preview_action(showing, Message::TogglePreview(entry.id)));
    }

    controls.push(action_toggle(
        "pin-symbolic",
        entry.pinned.is_some(),
        Message::TogglePin(entry.id),
    ));
    controls.push(action("window-close-symbolic", Message::Delete(entry.id)));

    widget::container(
        widget::row::with_children(controls)
            .spacing(GAP_TIGHT)
            .align_y(Alignment::Center),
    )
    .padding([0, PAD])
    .width(Length::Fill)
    .into()
}

fn content<'a>(app: &'a ClipKeep, entry: &'a EntryMeta, highlighted: bool) -> Element<'a, Message> {
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

    accented(widget::text::body(label_for(entry)), highlighted)
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

fn accented(
    text: widget::Text<'_, cosmic::Theme>,
    highlighted: bool,
) -> widget::Text<'_, cosmic::Theme> {
    if highlighted {
        text.class(cosmic::theme::Text::Accent)
    } else {
        text
    }
}

fn preview_action(showing: bool, message: Message) -> Element<'static, Message> {
    widget::button::icon(widget::icon::from_svg_bytes(PREVIEW_ICON).symbolic(true))
        .class(cosmic::theme::Button::Icon)
        .on_press_maybe((!showing).then_some(message))
        .into()
}

fn action(glyph: &str, message: Message) -> Element<'_, Message> {
    action_toggle(glyph, false, message)
}

fn action_toggle(glyph: &str, selected: bool, message: Message) -> Element<'_, Message> {
    widget::button::icon(widget::icon::from_name(glyph).size(14).symbolic(true))
        .class(if selected {
            accent_icon()
        } else {
            cosmic::theme::Button::Icon
        })
        .on_press(message)
        .into()
}

fn accent_icon() -> cosmic::theme::Button {
    use cosmic::widget::button::{Catalog, Style};

    const BASE: cosmic::theme::Button = cosmic::theme::Button::Icon;

    fn tint(theme: &cosmic::Theme, mut style: Style) -> Style {
        let accent = theme.cosmic().accent_text_color().into();
        style.icon_color = Some(accent);
        style.text_color = Some(accent);
        style
    }

    cosmic::theme::Button::Custom {
        active: Box::new(|focused, theme| {
            tint(theme, Catalog::active(theme, focused, true, &BASE))
        }),
        disabled: Box::new(|theme| Catalog::disabled(theme, &BASE)),
        hovered: Box::new(|focused, theme| {
            tint(theme, Catalog::hovered(theme, focused, true, &BASE))
        }),
        pressed: Box::new(|focused, theme| {
            tint(theme, Catalog::pressed(theme, focused, true, &BASE))
        }),
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

fn worth_previewing(entry: &EntryMeta) -> bool {
    entry.kind != EntryKind::Image && entry.preview.chars().count() >= PREVIEW_CHARS
}

fn divider<'a>() -> Element<'a, Message> {
    widget::divider::horizontal::default().into()
}

fn preview_panel(app: &ClipKeep) -> Option<Element<'_, Message>> {
    let (id, content) = app.preview()?;

    let label = app
        .snapshot()
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .map(label_for)
        .unwrap_or_default();

    let header = widget::row::with_children(vec![
        widget::text::caption(label)
            .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
            .width(Length::Fill)
            .into(),
        action("go-up-symbolic", Message::TogglePreview(id)),
    ])
    .spacing(GAP_TIGHT)
    .align_y(Alignment::Center);

    let body: Element<'_, Message> = match content {
        Preview::Text(text) => scroll(widget::text::body(text.as_str()))
            .height(Length::Fixed(PREVIEW_HEIGHT))
            .into(),
        Preview::Unavailable => widget::container(widget::text::body(fl!("no-results")))
            .center_x(Length::Fill)
            .into(),
    };

    Some(
        widget::container(
            widget::column::with_children(vec![header.into(), body]).spacing(GAP_TIGHT),
        )
        .padding([GAP, PAD, PAD, PAD])
        .into(),
    )
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
