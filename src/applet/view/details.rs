use cosmic::Element;
use cosmic::iced::advanced::text::Wrapping;
use cosmic::iced::{Alignment, Length};
use cosmic::widget;

use super::{GAP, GAP_TIGHT, ICON, ICON_SMALL, KEY_ENTER, KEY_ESCAPE, PAD, footer, icons, style};
use crate::applet::ClipKeep;
use crate::applet::message::Message;
use crate::clip::model::{EntryKind, EntryMeta, Timestamp, truncate_chars};
use crate::fl;

const DETAILS_CHARS: usize = 400;
const LABEL_WIDTH: f32 = 116.0;
const ROW_HEIGHT: f32 = 30.0;

const IMAGE_WIDTH: u16 = 256;
const IMAGE_HEIGHT: u16 = 192;

const FACTS_RESERVE: f32 = 340.0;

pub fn page<'a>(app: &'a ClipKeep, entry: &'a EntryMeta) -> Element<'a, Message> {
    widget::column::with_children(vec![
        super::page_header(fl!("details"), Message::ShowDetails(None)),
        widget::container(super::scroll(body(app, entry)))
            .max_height(super::body_budget(
                super::PAGE_HEADER_RESERVE + super::FOOTER_RESERVE,
            ))
            .width(Length::Fill)
            .into(),
        footer::bar(vec![
            (KEY_ESCAPE, fl!("footer-back")),
            (KEY_ENTER, fl!("footer-use")),
        ]),
    ])
    .into()
}

fn body<'a>(app: &'a ClipKeep, entry: &'a EntryMeta) -> Element<'a, Message> {
    let is_image = entry.kind == EntryKind::Image;
    let image_size = is_image.then_some(entry.image_size).flatten();
    let thumbnail = is_image
        .then(|| app.thumbs().get(entry.id).cloned())
        .flatten();

    let summary = match thumbnail.zip(image_size) {
        Some((handle, size)) => Some(preview(handle, size)),
        None => image_size
            .is_none()
            .then(|| excerpt(details_text(entry.label()))),
    };

    let facts = [
        entry
            .source_app
            .as_deref()
            .map(|id| (icons::app(), fl!("details-source"), application(id))),
        Some((
            icons::calendar(),
            fl!("details-copied"),
            moment(entry.created_at),
        )),
        Some((
            icons::history(),
            fl!("details-used"),
            moment(entry.last_used_at),
        )),
        Some((
            icons::hash(),
            fl!("details-copies"),
            entry.use_count.to_string(),
        )),
        Some((
            icons::database(),
            fl!("details-bytes"),
            bytes(entry.byte_size),
        )),
        image_size.map(|(width, height)| {
            (
                icons::crop(),
                fl!("details-size"),
                format!("{width} × {height}"),
            )
        }),
    ];

    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for (index, (handle, name, value)) in facts.into_iter().flatten().enumerate() {
        if index > 0 {
            rows.push(super::divider());
        }
        rows.push(detail(handle, name, value));
    }

    if let Some(formats) = formats(app.formats()) {
        rows.push(super::divider());
        rows.push(formats);
    }

    let mut children: Vec<Element<'a, Message>> = Vec::new();

    if let Some(summary) = summary {
        children.push(
            widget::container(summary)
                .class(style::raised())
                .width(Length::Fill)
                .padding(PAD)
                .into(),
        );
        children.push(
            widget::container(super::divider())
                .padding([PAD, 0, 0, 0])
                .into(),
        );
    }

    children.push(
        widget::container(super::heading(
            icons::info(),
            fl!("details-information"),
            None,
        ))
        .padding([PAD, 0, GAP, 0])
        .into(),
    );
    children.push(
        widget::column::with_children(rows)
            .width(Length::Fill)
            .into(),
    );

    widget::container(widget::column::with_children(children).width(Length::Fill))
        .padding([0, PAD, PAD, PAD])
        .width(Length::Fill)
        .into()
}

fn excerpt_budget() -> f32 {
    super::body_budget(super::PAGE_HEADER_RESERVE + super::FOOTER_RESERVE + FACTS_RESERVE)
}

fn excerpt(text: String) -> Element<'static, Message> {
    let lines = widget::text::body(text)
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill);

    widget::container(widget::scrollable(lines))
        .max_height(excerpt_budget())
        .width(Length::Fill)
        .into()
}

fn preview(handle: widget::image::Handle, size: (u32, u32)) -> Element<'static, Message> {
    let (width, height) =
        crate::clip::thumbnail::fit_within(size.0, size.1, IMAGE_WIDTH, IMAGE_HEIGHT);

    widget::container(
        widget::image(handle)
            .width(Length::Fixed(super::pixels(width)))
            .height(Length::Fixed(super::pixels(height))),
    )
    .center_x(Length::Fill)
    .into()
}

fn detail(handle: widget::icon::Handle, name: String, value: String) -> Element<'static, Message> {
    widget::container(fact(
        handle,
        name,
        widget::text::caption(value).width(Length::Fill),
        Alignment::Center,
    ))
    .height(Length::Fixed(ROW_HEIGHT))
    .align_y(Alignment::Center)
    .into()
}

fn fact<'a>(
    handle: widget::icon::Handle,
    name: String,
    value: impl Into<Element<'a, Message>>,
    align: Alignment,
) -> Element<'a, Message> {
    widget::row::with_children(vec![
        icons::sized(handle, ICON_SMALL).into(),
        widget::divider::vertical::default()
            .height(Length::Fixed(f32::from(ICON)))
            .into(),
        widget::text::caption(name)
            .width(Length::Fixed(LABEL_WIDTH))
            .into(),
        value.into(),
    ])
    .spacing(GAP + GAP_TIGHT)
    .align_y(align)
    .into()
}

fn formats<'a>(mimes: &[String]) -> Option<Element<'a, Message>> {
    let names = format_names(mimes);
    if names.is_empty() {
        return None;
    }

    let lines: Vec<Element<'a, Message>> = names
        .into_iter()
        .map(|name| {
            widget::text::caption(name)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill)
                .into()
        })
        .collect();

    Some(
        widget::container(fact(
            icons::code(),
            fl!("details-formats"),
            widget::column::with_children(lines)
                .spacing(GAP_TIGHT)
                .width(Length::Fill),
            Alignment::Start,
        ))
        .padding([GAP_TIGHT + 2, 0])
        .into(),
    )
}

fn format_names(mimes: &[String]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for mime in mimes {
        let name = format_name(mime);
        if !names.contains(&name) {
            names.push(name);
        }
    }

    names
}

fn format_name(mime: &str) -> String {
    let base = mime.split(';').next().unwrap_or(mime).trim();

    for (known, name) in [
        ("text/html", "HTML"),
        ("text/rtf", "RTF"),
        ("application/rtf", "RTF"),
        ("text/richtext", "RTF"),
        ("text/markdown", "Markdown"),
        ("text/csv", "CSV"),
        ("image/png", "PNG"),
        ("image/jpeg", "JPEG"),
        ("image/gif", "GIF"),
        ("image/bmp", "BMP"),
    ] {
        if base.eq_ignore_ascii_case(known) {
            return name.to_owned();
        }
    }

    if crate::clip::mime::FILES
        .iter()
        .any(|known| base.eq_ignore_ascii_case(known))
    {
        return fl!("format-files");
    }

    if crate::clip::mime::TEXT.iter().any(|known| {
        let known = known.split(';').next().unwrap_or(known);
        base.eq_ignore_ascii_case(known)
    }) {
        return fl!("format-plain");
    }

    base.to_owned()
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

fn details_text(text: &str) -> String {
    if text.chars().nth(DETAILS_CHARS).is_none() {
        return text.to_owned();
    }

    let mut truncated = truncate_chars(text, DETAILS_CHARS - 3);
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn mimes(list: &[&str]) -> Vec<String> {
        list.iter().map(|mime| (*mime).to_owned()).collect()
    }

    #[test]
    fn a_text_summary_never_pushes_the_facts_off_the_page() {
        let budget = super::super::body_budget(
            super::super::PAGE_HEADER_RESERVE + super::super::FOOTER_RESERVE,
        );

        assert!(excerpt_budget() > 0.0);
        assert!(
            excerpt_budget() + FACTS_RESERVE <= budget,
            "the excerpt and the information block have to share one page"
        );
    }

    #[test]
    fn an_entry_whose_formats_have_not_arrived_shows_no_row() {
        assert!(format_names(&[]).is_empty());
    }

    #[test]
    fn a_charset_is_not_part_of_the_name_a_reader_needs() {
        assert_eq!(
            format_name("text/plain;charset=utf-8"),
            format_name("text/plain")
        );
        assert_eq!(format_name("TEXT/HTML"), "HTML");
    }

    #[test]
    fn the_two_spellings_of_rich_text_read_as_one_format() {
        assert_eq!(
            format_names(&mimes(&[
                "text/plain;charset=utf-8",
                "text/html",
                "text/rtf",
                "text/richtext"
            ])),
            [
                format_name("text/plain"),
                "HTML".to_owned(),
                "RTF".to_owned()
            ]
        );
    }

    #[test]
    fn a_format_nobody_named_still_shows_what_it_is() {
        assert_eq!(
            format_name("application/x-invented"),
            "application/x-invented"
        );
    }

    #[test]
    fn an_application_id_reads_as_a_name() {
        assert_eq!(application("org.chromium.Chromium"), "Chromium");
        assert_eq!(application("com.system76.CosmicFiles"), "Cosmic Files");
    }

    #[test]
    fn sizes_climb_through_the_units() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1024), "1.0 kB");
        assert_eq!(bytes(1024 * 1024 * 2), "2.0 MB");
    }
}
