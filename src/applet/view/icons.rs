use cosmic::widget;

macro_rules! bundled {
    ($($name:ident => $file:literal,)*) => {
        $(
            pub fn $name() -> widget::icon::Handle {
                widget::icon::from_svg_bytes(
                    include_bytes!(concat!("../../../resources/icons/", $file)).as_slice(),
                )
                .symbolic(true)
            }
        )*
    };
}

bundled! {
    app => "app-symbolic.svg",
    back => "back-symbolic.svg",
    calendar => "calendar-symbolic.svg",
    close => "close-symbolic.svg",
    crop => "crop-symbolic.svg",
    database => "database-symbolic.svg",
    file => "file-symbolic.svg",
    hash => "hash-symbolic.svg",
    history => "history-symbolic.svg",
    image => "image-symbolic.svg",
    info => "info-symbolic.svg",
    lock => "lock-symbolic.svg",
    mask => "mask-symbolic.svg",
    more => "more-symbolic.svg",
    paste => "paste-symbolic.svg",
    paused => "paused-symbolic.svg",
    pin => "pin-symbolic.svg",
    search => "search-symbolic.svg",
    settings => "settings-symbolic.svg",
    shield => "shield-symbolic.svg",
    text => "text-symbolic.svg",
    trash => "trash-symbolic.svg",
    warning => "warning-symbolic.svg",
}

pub fn clipboard() -> widget::icon::Handle {
    widget::icon::from_svg_bytes(
        include_bytes!(
            "../../../resources/io.github.marcelogomes90.cosmic-ext-applet-clip-keep-symbolic.svg"
        )
        .as_slice(),
    )
    .symbolic(true)
}

pub fn sized(handle: widget::icon::Handle, size: u16) -> widget::icon::Icon {
    widget::icon::icon(handle).size(size)
}
