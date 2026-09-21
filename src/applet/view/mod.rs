pub mod details;
pub mod footer;
pub mod history;
pub mod icons;
pub mod settings;
pub mod style;

use std::sync::LazyLock;

use cosmic::Element;
use cosmic::iced::{Alignment, Length, Limits, Point, Rectangle, Size};
use cosmic::widget;

use super::ClipKeep;
use super::message::Message;
use crate::clip::model::{EntryId, EntryMeta};
use crate::clip::search;

pub const VISIBLE_ROWS: usize = 50;

pub(crate) const PAD: u16 = 16;
pub(crate) const PAD_ROW_H: u16 = 12;
pub(crate) const LIST_INSET: u16 = 16;
pub(crate) const GAP: u16 = 8;
pub(crate) const GAP_TIGHT: u16 = 4;

pub(crate) const CONTROL_HEIGHT: u16 = 32;

pub(crate) const ICON_SMALL: u16 = 14;
pub(crate) const ICON: u16 = 16;

pub(crate) const KEY_PIN: &str = "Ctrl+P";
pub(crate) const KEY_DELETE: &str = "Ctrl+D";
pub(crate) const KEY_INFO: &str = "Ctrl+I";
pub(crate) const KEY_ESCAPE: &str = "Esc";
pub(crate) const KEY_ENTER: &str = "Enter";

pub const SURFACE_WIDTH: f32 = 360.0;
const SURFACE_MAX_HEIGHT: f32 = 800.0;

pub(crate) const HEADER_RESERVE: f32 = 72.0;
pub(crate) const PAGE_HEADER_RESERVE: f32 = 64.0;
pub(crate) const DIVIDER_RESERVE: f32 = 1.0;
pub(crate) const FOOTER_RESERVE: f32 = 84.0;
pub(crate) const NOTICE_RESERVE: f32 = 88.0;
pub(crate) const CONFIRM_RESERVE: f32 = 144.0;

pub(crate) static SEARCH_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("clip-keep-search"));

pub(crate) static SCROLL_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("clip-keep-list"));

static SURFACE_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("clip-keep-popup"));
static PANEL_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("clip-keep-panel"));
static MENU_POPOVER_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("clip-keep-row-menu"));

pub(crate) fn row_id(entry: EntryId) -> widget::Id {
    widget::Id::new(format!("clip-keep-row-{}", entry.0))
}

pub(crate) fn menu_button_id(entry: EntryId) -> widget::Id {
    widget::Id::new(format!("clip-keep-menu-{}", entry.0))
}

pub fn visible(app: &ClipKeep) -> Vec<&EntryMeta> {
    let entries = &app.snapshot().entries;
    search::filter(entries.iter().map(EntryMeta::label), app.query())
        .into_iter()
        .filter_map(|index| entries.get(index))
        .take(VISIBLE_ROWS)
        .collect()
}

pub fn popup(app: &ClipKeep) -> Element<'_, Message> {
    let body: Element<'_, Message> = if let Some(entry) = app.details() {
        details::page(app, entry)
    } else if app.showing_settings() {
        settings::page(app)
    } else {
        history::page(app)
    };

    let surface = widget::container(body)
        .width(Length::Fixed(SURFACE_WIDTH))
        .style(style::surface);

    let menu = app
        .row_menu()
        .filter(|_| app.details().is_none() && !app.showing_settings());

    let mut catcher = widget::mouse_area(surface);
    if menu.is_some() {
        catcher = catcher.on_press(Message::CloseRowMenu);
    }

    let mut content = widget::popover(catcher).id(MENU_POPOVER_ID.clone());

    if let Some((id, Some(anchor))) = menu {
        content = content
            .position(widget::popover::Position::Point(menu_origin(
                anchor,
                history::menu_width(),
            )))
            .popup(history::menu(app, id));
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

pub fn panel(
    button: Element<'_, Message>,
    suggested: Option<Size>,
    horizontal: bool,
) -> Element<'_, Message> {
    widget::autosize::autosize(button, PANEL_ID.clone())
        .limits(panel_limits(suggested, horizontal))
        .into()
}

pub(crate) fn panel_limits(suggested: Option<Size>, horizontal: bool) -> Limits {
    let Some(bounds) = suggested else {
        return Limits::NONE;
    };

    let mut limits = Limits::NONE;

    if horizontal {
        if bounds.width > 0.0 {
            limits = limits.max_width(bounds.width);
        }
        if bounds.height > 0.0 {
            limits = limits.height(bounds.height);
        }
    } else {
        if bounds.width > 0.0 {
            limits = limits.width(bounds.width);
        }
        if bounds.height > 0.0 {
            limits = limits.max_height(bounds.height);
        }
    }

    limits
}

pub(crate) fn body_budget(reserved: f32) -> f32 {
    SURFACE_MAX_HEIGHT - reserved
}

fn menu_origin(anchor: Rectangle, width: f32) -> Point {
    let x = (anchor.x + anchor.width - width).max(f32::from(PAD));

    Point::new(x, anchor.y + anchor.height + f32::from(GAP_TIGHT))
}

pub(crate) fn page_header<'a>(title: String, back: Message) -> Element<'a, Message> {
    let back_button = widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
        .class(widget::button::ButtonClass::Link)
        .extra_small()
        .label(crate::fl!("back"))
        .padding(0)
        .spacing(4)
        .on_press(back);

    widget::container(
        cosmic::iced::widget::stack(vec![
            widget::container(back_button).into(),
            widget::container(widget::text::heading(title))
                .center(Length::Fill)
                .into(),
        ])
        .width(Length::Fill),
    )
    .padding(PAD)
    .width(Length::Fill)
    .into()
}

pub(crate) fn heading<'a>(
    handle: widget::icon::Handle,
    title: String,
    count: Option<usize>,
) -> Element<'a, Message> {
    let mut children: Vec<Element<'a, Message>> = vec![
        icons::sized(handle, ICON_SMALL).into(),
        widget::text::heading(title).into(),
    ];

    if let Some(count) = count {
        children.push(
            widget::container(widget::text::caption(count.to_string()))
                .padding([1, 8])
                .class(style::badge())
                .into(),
        );
    }

    widget::row::with_children(children)
        .spacing(GAP)
        .align_y(Alignment::Center)
        .into()
}

pub(crate) fn scroll<'a>(
    content: impl Into<Element<'a, Message>>,
) -> cosmic::iced::widget::Scrollable<'a, Message, cosmic::Theme, cosmic::Renderer> {
    widget::scrollable(content)
        .id(SCROLL_ID.clone())
        .scrollbar_width(0.0)
        .scroller_width(0.0)
        .scrollbar_padding(0.0)
}

pub(crate) fn divider<'a>() -> Element<'a, Message> {
    widget::divider::horizontal::default().into()
}

pub(crate) fn pixels(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn the_panel_button_keeps_its_own_length_however_long_the_panel_is() {
        let button = Length::Fixed(40.0);

        let along =
            panel_limits(Some(Size::new(1920.0, 40.0)), true).resolve(button, button, Size::ZERO);
        assert!(near(along.width, 40.0));

        let down =
            panel_limits(Some(Size::new(40.0, 1080.0)), false).resolve(button, button, Size::ZERO);
        assert!(near(down.height, 40.0));
    }

    #[test]
    fn the_panel_button_takes_the_thickness_the_panel_offers() {
        let button = Length::Fixed(40.0);

        let along =
            panel_limits(Some(Size::new(1920.0, 48.0)), true).resolve(button, button, Size::ZERO);
        assert!(near(along.height, 48.0));

        let down =
            panel_limits(Some(Size::new(48.0, 1080.0)), false).resolve(button, button, Size::ZERO);
        assert!(near(down.width, 48.0));
    }

    #[test]
    fn a_panel_that_suggests_nothing_constrains_nothing() {
        let button = Length::Fixed(40.0);
        let size = panel_limits(None, true).resolve(button, button, Size::ZERO);

        assert!(near(size.width, 40.0));
        assert!(near(size.height, 40.0));
    }

    fn near(left: f32, right: f32) -> bool {
        (left - right).abs() < 0.001
    }

    fn anchor(x: f32, y: f32) -> Rectangle {
        Rectangle {
            x,
            y,
            width: 28.0,
            height: 28.0,
        }
    }

    #[test]
    fn a_row_menu_hangs_below_its_button_and_ends_at_the_same_right_edge() {
        let button = anchor(SURFACE_WIDTH - 44.0, 120.0);
        let width = history::menu_width();
        let origin = menu_origin(button, width);

        assert!(near(origin.x + width, button.x + button.width));
        assert!(near(
            origin.y,
            button.y + button.height + f32::from(GAP_TIGHT)
        ));
    }

    #[test]
    fn a_row_menu_never_starts_left_of_the_surface_padding() {
        assert!(near(
            menu_origin(anchor(10.0, 40.0), history::menu_width()).x,
            f32::from(PAD)
        ));
    }

    #[test]
    fn every_page_keeps_room_for_its_header_and_footer() {
        for reserved in [
            HEADER_RESERVE + FOOTER_RESERVE,
            HEADER_RESERVE + FOOTER_RESERVE + NOTICE_RESERVE,
            HEADER_RESERVE + FOOTER_RESERVE + NOTICE_RESERVE + CONFIRM_RESERVE,
            PAGE_HEADER_RESERVE + FOOTER_RESERVE,
            PAGE_HEADER_RESERVE + DIVIDER_RESERVE,
        ] {
            assert!(body_budget(reserved) < SURFACE_MAX_HEIGHT);
            assert!(body_budget(reserved) > 0.0);
        }
    }

    #[test]
    fn the_page_header_reserve_matches_the_header_it_stands_for() {
        assert!(near(
            PAGE_HEADER_RESERVE,
            f32::from(CONTROL_HEIGHT) + f32::from(PAD) * 2.0
        ));
    }

    #[test]
    fn a_page_without_a_footer_gets_that_space_for_its_body() {
        assert!(
            body_budget(PAGE_HEADER_RESERVE + DIVIDER_RESERVE)
                > body_budget(PAGE_HEADER_RESERVE + FOOTER_RESERVE)
        );
    }
}
