use cosmic::Element;
use cosmic::iced::advanced::text::{Ellipsize, EllipsizeHeightLimit};
use cosmic::iced::{Alignment, Length};
use cosmic::widget;

use super::{
    CONTROL_HEIGHT, GAP, GAP_TIGHT, ICON, ICON_SMALL, KEY_DELETE, KEY_INFO, KEY_PIN,
    NOTICE_RESERVE, PAD, PAD_ROW_H, SEARCH_ID, footer, icons, style,
};
use crate::applet::ClipKeep;
use crate::applet::message::Message;
use crate::clip::model::{CaptureState, EntryId, EntryKind, EntryMeta};
use crate::fl;

const MENU_LABEL_CHAR: f32 = 7.6;
const MENU_WIDTH_MIN: f32 = 120.0;
const MENU_WIDTH_MAX: f32 = 240.0;

const TILE: f32 = 36.0;
const THUMBNAIL: u16 = 28;

pub fn page(app: &ClipKeep) -> Element<'_, Message> {
    let rows = super::visible(app);
    let notice = capture_notice(app);

    let mut children: Vec<Element<'_, Message>> = vec![header(app)];
    let mut extra = 0.0;

    if let Some(notice) = notice {
        children.push(notice);
        extra += NOTICE_RESERVE;
    }

    if rows.is_empty() {
        children.push(empty_state(app));
    } else {
        let open = app.row_menu().is_some();
        let mut pinned = Vec::new();
        let mut regular = Vec::new();

        for entry in rows {
            let item = row(app, entry, open);
            if entry.pinned.is_some() {
                pinned.push(item);
            } else {
                regular.push(item);
            }
        }

        let mut sections = Vec::new();
        if !pinned.is_empty() {
            sections.push(section(icons::pin(), fl!("section-pinned"), pinned));
        }
        if !regular.is_empty() {
            if !sections.is_empty() {
                sections.push(
                    widget::container(super::divider())
                        .padding([GAP, PAD])
                        .width(Length::Fill)
                        .into(),
                );
            }
            sections.push(section(icons::history(), fl!("section-recent"), regular));
        }

        let list = widget::container(widget::column::with_children(sections))
            .padding([0, 0, PAD, 0])
            .width(Length::Fill);

        children.push(
            widget::container(super::scroll(list))
                .max_height(super::body_budget(
                    super::HEADER_RESERVE + super::FOOTER_RESERVE + extra,
                ))
                .width(Length::Fill)
                .into(),
        );
    }

    children.push(footer::bar(vec![
        (KEY_DELETE, fl!("action-delete")),
        (KEY_INFO, fl!("action-details")),
        (KEY_PIN, fl!("action-pin")),
    ]));

    widget::column::with_children(children).into()
}

fn header(app: &ClipKeep) -> Element<'_, Message> {
    let clear = widget::button::icon(icons::trash())
        .icon_size(ICON)
        .class(style::flat(false))
        .padding([0, (CONTROL_HEIGHT - ICON) / 2])
        .height(Length::Fixed(f32::from(CONTROL_HEIGHT)))
        .on_press_maybe(
            app.snapshot()
                .entries
                .iter()
                .any(|entry| entry.pinned.is_none())
                .then_some(Message::Clear),
        );

    let settings = widget::button::icon(icons::settings())
        .icon_size(ICON)
        .class(style::flat(false))
        .padding([0, (CONTROL_HEIGHT - ICON) / 2])
        .height(Length::Fixed(f32::from(CONTROL_HEIGHT)))
        .on_press(Message::ShowSettings(true));

    widget::container(
        widget::row::with_children(vec![search_field(app), clear.into(), settings.into()])
            .spacing(GAP)
            .align_y(Alignment::Center),
    )
    .padding(PAD)
    .width(Length::Fill)
    .into()
}

fn search_field(app: &ClipKeep) -> Element<'_, Message> {
    let mut field = widget::text_input(fl!("search-placeholder"), app.query())
        .id(SEARCH_ID.clone())
        .width(Length::Fill)
        .padding([0, cosmic::theme::spacing().space_xxs])
        .style(cosmic::theme::TextInput::Search)
        .leading_icon(
            widget::container(icons::sized(icons::search(), ICON))
                .padding(GAP)
                .into(),
        )
        .on_input(Message::Search);

    if !app.query().is_empty() {
        field = field.trailing_icon(
            widget::button::icon(icons::close())
                .icon_size(ICON)
                .class(cosmic::theme::Button::Icon)
                .padding(GAP)
                .on_press(Message::Search(String::new()))
                .into(),
        );
    }

    field.into()
}

fn section<'a>(
    handle: widget::icon::Handle,
    title: String,
    rows: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let count = rows.len();
    let heading = widget::container(super::heading(handle, title, Some(count)))
        .padding([GAP, PAD, GAP, PAD + PAD_ROW_H])
        .width(Length::Fill);

    let mut children: Vec<Element<'a, Message>> = vec![heading.into()];
    children.extend(rows);

    widget::column::with_children(children).into()
}

fn capture_notice(app: &ClipKeep) -> Option<Element<'_, Message>> {
    let CaptureState::Unavailable { reason } = &app.snapshot().capture else {
        return None;
    };

    let lines: Vec<Element<'_, Message>> = vec![
        widget::text::body(fl!("capture-unavailable")).into(),
        widget::text::caption(reason.clone())
            .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(2)))
            .into(),
    ];

    Some(
        widget::container(
            widget::row::with_children(vec![
                icons::sized(icons::warning(), ICON).into(),
                widget::column::with_children(lines)
                    .spacing(GAP_TIGHT)
                    .width(Length::Fill)
                    .into(),
            ])
            .spacing(GAP)
            .align_y(Alignment::Start),
        )
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

fn row<'a>(app: &'a ClipKeep, entry: &'a EntryMeta, menu_open: bool) -> Element<'a, Message> {
    let active = app.focused() == Some(entry.id);

    let button = widget::button::custom(
        widget::row::with_children(vec![
            tile(app, entry),
            widget::text::body(label(entry))
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                .width(Length::Fill)
                .into(),
        ])
        .spacing(GAP)
        .align_y(Alignment::Center),
    )
    .class(style::quiet())
    .padding([GAP_TIGHT, PAD_ROW_H])
    .width(Length::Fill)
    .on_press_maybe((!menu_open).then_some(Message::Confirm(entry.id)));

    let inner = widget::container(
        widget::row::with_children(vec![button.into(), menu_button(entry.id)])
            .spacing(GAP_TIGHT)
            .align_y(Alignment::Center),
    )
    .padding([0, GAP_TIGHT, 0, 0])
    .width(Length::Fill)
    .class(if active {
        style::active_row()
    } else {
        cosmic::theme::Container::Transparent
    });

    let row = widget::container(inner)
        .id(super::row_id(entry.id))
        .padding([1, PAD])
        .width(Length::Fill);

    let mut hover = widget::mouse_area(row);
    if !menu_open {
        hover = hover.on_move(move |_| Message::Focus(entry.id));
    }

    hover.into()
}

fn tile<'a>(app: &'a ClipKeep, entry: &'a EntryMeta) -> Element<'a, Message> {
    let content: Element<'a, Message> = match entry.kind {
        EntryKind::Image => match app.thumbs().get(entry.id) {
            Some(handle) => {
                let (width, height) = entry
                    .image_size
                    .map_or((THUMBNAIL.into(), THUMBNAIL.into()), |(w, h)| {
                        crate::clip::thumbnail::fit_within(w, h, THUMBNAIL, THUMBNAIL)
                    });

                widget::image(handle.clone())
                    .width(Length::Fixed(super::pixels(width)))
                    .height(Length::Fixed(super::pixels(height)))
                    .into()
            }
            None => icons::sized(icons::image(), ICON).into(),
        },
        EntryKind::Files => icons::sized(icons::file(), ICON).into(),
        EntryKind::Text => icons::sized(icons::text(), ICON).into(),
    };

    widget::container(content)
        .center(Length::Fixed(TILE))
        .class(style::tile())
        .into()
}

fn menu_button(id: EntryId) -> Element<'static, Message> {
    widget::container(
        widget::button::icon(icons::more())
            .icon_size(ICON_SMALL)
            .class(style::flat(false))
            .padding(GAP_TIGHT + 2)
            .on_press(Message::OpenRowMenu(id)),
    )
    .id(super::menu_button_id(id))
    .into()
}

pub(crate) fn menu_width() -> f32 {
    let labels = [
        fl!("action-details"),
        fl!("action-pin"),
        fl!("action-unpin"),
        fl!("action-delete"),
    ];

    let longest = labels
        .iter()
        .map(|label| label.chars().count())
        .max()
        .unwrap_or(0);

    label_room(longest).clamp(MENU_WIDTH_MIN, MENU_WIDTH_MAX)
}

fn label_room(characters: usize) -> f32 {
    let text = MENU_LABEL_CHAR * f32::from(u16::try_from(characters).unwrap_or(u16::MAX));
    let chrome = f32::from(ICON + (GAP + GAP_TIGHT) * 3 + GAP_TIGHT * 2);

    text + chrome
}

pub(crate) fn menu(app: &ClipKeep, id: EntryId) -> Element<'_, Message> {
    let pinned = app
        .snapshot()
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .is_some_and(|entry| entry.pinned.is_some());

    let items = vec![
        item(
            icons::info(),
            fl!("action-details"),
            Message::ShowDetails(Some(id)),
            false,
        ),
        item(
            icons::pin(),
            if pinned {
                fl!("action-unpin")
            } else {
                fl!("action-pin")
            },
            Message::TogglePin(id),
            false,
        ),
        item(
            icons::trash(),
            fl!("action-delete"),
            Message::Delete(id),
            true,
        ),
    ];

    widget::container(
        widget::column::with_children(items)
            .spacing(GAP_TIGHT)
            .width(Length::Fill),
    )
    .width(Length::Fixed(menu_width()))
    .padding(GAP_TIGHT)
    .class(cosmic::theme::Container::Dropdown)
    .into()
}

fn item(
    handle: widget::icon::Handle,
    label: String,
    message: Message,
    destructive: bool,
) -> Element<'static, Message> {
    widget::button::custom(
        widget::row::with_children(vec![
            icons::sized(handle, ICON).into(),
            widget::text::body(label).width(Length::Fill).into(),
        ])
        .spacing(GAP + GAP_TIGHT)
        .align_y(Alignment::Center),
    )
    .class(style::flat(destructive))
    .padding([GAP_TIGHT + 2, GAP + GAP_TIGHT])
    .width(Length::Fill)
    .on_press(message)
    .into()
}

fn label(entry: &EntryMeta) -> String {
    match (entry.kind, entry.image_size) {
        (EntryKind::Image, Some((width, height))) => {
            fl!("item-image", width = width, height = height)
        }
        _ => one_line(entry.label()),
    }
}

fn one_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_is_as_wide_as_its_longest_entry_needs() {
        assert!(label_room(8) > label_room(4));
        assert!(menu_width() >= MENU_WIDTH_MIN);
        assert!(menu_width() <= MENU_WIDTH_MAX);
    }

    #[test]
    fn a_wildly_long_translation_does_not_stretch_the_menu_past_the_popup() {
        assert!(
            label_room(200).clamp(MENU_WIDTH_MIN, MENU_WIDTH_MAX) < super::super::SURFACE_WIDTH
        );
    }

    #[test]
    fn a_row_shows_the_first_line_that_has_something_on_it() {
        assert_eq!(one_line("\n\n  hello \nworld"), "  hello ");
        assert_eq!(one_line("only"), "only");
        assert_eq!(one_line("   "), "");
    }
}
