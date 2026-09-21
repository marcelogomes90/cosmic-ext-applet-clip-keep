use cosmic::cosmic_theme::palette::Srgba;
use cosmic::iced::advanced::widget::text::Style as TextStyle;
use cosmic::iced::{Background, Border, Color, Shadow};
use cosmic::widget;

const ACCEL_ALPHA: f32 = 0.75;

fn component(theme: &cosmic::Theme) -> Srgba {
    theme.cosmic().bg_component_color()
}

fn divider(theme: &cosmic::Theme) -> Srgba {
    theme.cosmic().bg_component_divider()
}

fn hover(theme: &cosmic::Theme) -> Srgba {
    theme.cosmic().primary(theme.transparent).component.hover
}

fn pressed(theme: &cosmic::Theme) -> Srgba {
    theme
        .cosmic()
        .background(theme.transparent)
        .component
        .pressed
}

fn on_disabled(theme: &cosmic::Theme) -> Srgba {
    theme
        .cosmic()
        .background(theme.transparent)
        .component
        .on_disabled
}

pub fn surface(theme: &cosmic::Theme) -> widget::container::Style {
    let cosmic = theme.cosmic();
    let background = cosmic.background(theme.transparent);

    widget::container::Style {
        text_color: Some(background.on.into()),
        icon_color: Some(background.on.into()),
        background: Some(Color::from(background.base).into()),
        border: Border {
            radius: cosmic.corner_radii.radius_m.into(),
            width: 1.0,
            color: background.divider.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn quiet() -> cosmic::theme::Button {
    fn plain(_: bool, _: &cosmic::Theme) -> widget::button::Style {
        widget::button::Style::new()
    }

    cosmic::theme::Button::Custom {
        active: Box::new(plain),
        disabled: Box::new(|theme| plain(false, theme)),
        hovered: Box::new(plain),
        pressed: Box::new(plain),
    }
}

pub fn active_row<'a>() -> cosmic::theme::Container<'a> {
    cosmic::theme::Container::custom(|theme: &cosmic::Theme| widget::container::Style {
        background: Some(Color::from(hover(theme)).into()),
        border: Border {
            radius: theme.cosmic().corner_radii.radius_m.into(),
            ..Default::default()
        },
        ..Default::default()
    })
}

pub fn raised<'a>() -> cosmic::theme::Container<'a> {
    cosmic::theme::Container::custom(|theme: &cosmic::Theme| widget::container::Style {
        background: Some(Color::from(component(theme)).into()),
        border: Border {
            radius: theme.cosmic().corner_radii.radius_s.into(),
            width: 1.0,
            color: divider(theme).into(),
        },
        ..Default::default()
    })
}

pub fn tile<'a>() -> cosmic::theme::Container<'a> {
    cosmic::theme::Container::custom(|theme: &cosmic::Theme| widget::container::Style {
        background: Some(Color::from(component(theme)).into()),
        icon_color: Some(theme.cosmic().accent_text_color().into()),
        border: Border {
            radius: theme.cosmic().corner_radii.radius_s.into(),
            width: 1.0,
            color: divider(theme).into(),
        },
        ..Default::default()
    })
}

pub fn accel() -> cosmic::theme::Text {
    fn muted(theme: &cosmic::Theme) -> TextStyle {
        let mut ink = theme.cosmic().background(theme.transparent).component.on;
        ink.alpha *= ACCEL_ALPHA;

        TextStyle {
            color: Some(ink.into()),
            ..Default::default()
        }
    }

    cosmic::theme::Text::Custom(muted)
}

pub fn badge<'a>() -> cosmic::theme::Container<'a> {
    cosmic::theme::Container::custom(|theme: &cosmic::Theme| widget::container::Style {
        background: Some(Color::from(component(theme)).into()),
        border: Border {
            radius: theme.cosmic().corner_radii.radius_xl.into(),
            ..Default::default()
        },
        ..Default::default()
    })
}

pub fn flat(destructive: bool) -> cosmic::theme::Button {
    fn paint(
        theme: &cosmic::Theme,
        ink: Color,
        background: Option<Srgba>,
    ) -> widget::button::Style {
        widget::button::Style {
            background: background.map(|colour| Background::Color(colour.into())),
            border_radius: theme.cosmic().corner_radii.radius_m.into(),
            icon_color: Some(ink),
            text_color: Some(ink),
            ..widget::button::Style::new()
        }
    }

    fn ink(theme: &cosmic::Theme, destructive: bool) -> Color {
        let cosmic = theme.cosmic();

        if destructive {
            Color::from(cosmic.destructive_text_color())
        } else {
            Color::from(cosmic.on_bg_color())
        }
    }

    cosmic::theme::Button::Custom {
        active: Box::new(move |_, theme| paint(theme, ink(theme, destructive), None)),
        disabled: Box::new(move |theme| paint(theme, Color::from(on_disabled(theme)), None)),
        hovered: Box::new(move |_, theme| {
            paint(theme, ink(theme, destructive), Some(hover(theme)))
        }),
        pressed: Box::new(move |_, theme| {
            paint(theme, ink(theme, destructive), Some(pressed(theme)))
        }),
    }
}
