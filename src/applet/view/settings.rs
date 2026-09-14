use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget;

use super::{GAP, ICON, LIST_INSET, PAD, PAD_ROW_H, icons};
use crate::applet::ClipKeep;
use crate::applet::message::Message;
use crate::clip::settings::{MAX_ENTRIES_CEILING, Settings};
use crate::{fl, links};

pub fn page(app: &ClipKeep) -> Element<'_, Message> {
    let sections = widget::column::with_children(vec![
        section(
            icons::shield(),
            fl!("section-privacy"),
            privacy_controls(app),
        ),
        section(
            icons::history(),
            fl!("section-history"),
            history_controls(app),
        ),
        section(
            icons::settings(),
            fl!("section-behaviour"),
            behaviour_controls(app),
        ),
        section(icons::link(), fl!("section-links"), link_controls()),
    ])
    .spacing(PAD);

    widget::column::with_children(vec![
        super::page_header(fl!("settings"), Message::ShowSettings(false)),
        widget::container(super::divider()).padding([0, PAD]).into(),
        widget::container(super::scroll(widget::container(sections).padding(PAD)))
            .max_height(super::body_budget(
                super::PAGE_HEADER_RESERVE + super::DIVIDER_RESERVE,
            ))
            .width(Length::Fill)
            .into(),
    ])
    .into()
}

fn card<'a>() -> widget::ListColumn<'a, Message> {
    widget::list_column().list_item_padding([cosmic::theme::spacing().space_xxs, LIST_INSET])
}

fn section(
    handle: widget::icon::Handle,
    title: String,
    controls: Element<'_, Message>,
) -> Element<'_, Message> {
    widget::column::with_children(vec![super::heading(handle, title, None), controls])
        .spacing(PAD_ROW_H)
        .into()
}

fn edited(app: &ClipKeep, change: impl FnOnce(&mut Settings)) -> Message {
    let mut next = app.settings().clone();
    change(&mut next);
    Message::Setting(Box::new(next))
}

fn setting_row<'a>(
    handle: widget::icon::Handle,
    label: String,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    widget::row::with_children(vec![
        icons::sized(handle, ICON).into(),
        widget::text::body(label).width(Length::Fill).into(),
        control.into(),
    ])
    .spacing(GAP + 4)
    .align_y(Alignment::Center)
    .into()
}

fn toggle(
    app: &ClipKeep,
    handle: widget::icon::Handle,
    label: String,
    value: bool,
    change: fn(&mut Settings, bool),
) -> Element<'_, Message> {
    let message = edited(app, |settings| change(settings, !value));

    setting_row(
        handle,
        label,
        widget::toggler(value).on_toggle(move |_| message.clone()),
    )
}

fn privacy_controls(app: &ClipKeep) -> Element<'_, Message> {
    let settings = app.settings();

    card()
        .add(toggle(
            app,
            icons::mask(),
            fl!("setting-private-mode"),
            settings.private_mode,
            |settings, value| settings.private_mode = value,
        ))
        .add(toggle(
            app,
            icons::lock(),
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
        .add(setting_row(
            icons::database(),
            fl!("setting-max-entries"),
            entries,
        ))
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
        icons::trash(),
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
            icons::image(),
            fl!("setting-capture-images"),
            settings.capture_images,
            |settings, value| settings.capture_images = value,
        ))
        .add(toggle(
            app,
            icons::paste(),
            fl!("setting-paste-on-use"),
            settings.paste_on_use,
            |settings, value| settings.paste_on_use = value,
        ))
        .into()
}

fn link_controls<'a>() -> Element<'a, Message> {
    card()
        .add(link(icons::bug(), fl!("link-issues"), links::ISSUES))
        .add(link(
            icons::person(),
            fl!("link-developer"),
            links::DEVELOPER,
        ))
        .add(link(
            icons::code(),
            fl!("link-repository"),
            links::REPOSITORY,
        ))
        .into()
}

fn link<'a>(
    handle: widget::icon::Handle,
    label: String,
    url: &'static str,
) -> widget::list::ListButton<'a, Message> {
    widget::list::button(setting_row(
        handle,
        label,
        icons::sized(icons::link(), ICON),
    ))
    .on_press(Message::OpenLink(url))
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
