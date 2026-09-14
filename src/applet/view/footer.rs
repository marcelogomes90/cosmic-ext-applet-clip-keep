use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget;

use super::{GAP, GAP_TIGHT, PAD, style};
use crate::applet::message::Message;

const CHIP_PADDING: [u16; 2] = [2, 6];
const PAIR_GAP: u16 = 6;
const ITEM_GAP: u16 = 8;

pub fn bar<'a>(hints: Vec<(&'a str, String)>) -> Element<'a, Message> {
    let mut items: Vec<Element<'a, Message>> = Vec::with_capacity(hints.len() * 2);

    for (index, (key, label)) in hints.into_iter().enumerate() {
        if index > 0 {
            items.push(separator());
        }
        items.push(pair(key, label));
    }

    widget::column::with_children(vec![
        widget::divider::horizontal::default().into(),
        widget::container(
            widget::row::with_children(items)
                .spacing(ITEM_GAP)
                .align_y(Alignment::Center),
        )
        .center_x(Length::Fill)
        .padding([PAD, GAP])
        .into(),
    ])
    .width(Length::Fill)
    .into()
}

fn pair(key: &str, label: String) -> Element<'_, Message> {
    widget::row::with_children(vec![
        widget::container(widget::text::caption(key))
            .padding(CHIP_PADDING)
            .class(style::raised())
            .into(),
        widget::text::caption(label).into(),
    ])
    .spacing(PAIR_GAP)
    .align_y(Alignment::Center)
    .into()
}

fn separator<'a>() -> Element<'a, Message> {
    widget::container(widget::divider::vertical::default())
        .height(Length::Fixed(f32::from(GAP + GAP_TIGHT)))
        .align_y(Alignment::Center)
        .into()
}
