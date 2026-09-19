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
    behaviour => "behaviour-symbolic.svg",
    bug => "bug-symbolic.svg",
    calendar => "calendar-symbolic.svg",
    close => "close-symbolic.svg",
    code => "code-symbolic.svg",
    crop => "crop-symbolic.svg",
    database => "database-symbolic.svg",
    expiry => "expiry-symbolic.svg",
    file => "file-symbolic.svg",
    hash => "hash-symbolic.svg",
    history => "history-symbolic.svg",
    image => "image-symbolic.svg",
    info => "info-symbolic.svg",
    link => "link-symbolic.svg",
    lock => "lock-symbolic.svg",
    mask => "mask-symbolic.svg",
    paste => "paste-symbolic.svg",
    paused => "paused-symbolic.svg",
    person => "person-symbolic.svg",
    search => "search-symbolic.svg",
    shield => "shield-symbolic.svg",
    text => "text-symbolic.svg",
    warning => "warning-symbolic.svg",
}

macro_rules! named {
    ($($name:ident => $icon:literal,)*) => {
        $(
            pub fn $name() -> widget::icon::Handle {
                widget::icon::from_name($icon).handle()
            }
        )*
    };
}

named! {
    details => "dialog-information-symbolic",
    more => "view-more-symbolic",
    pin => "pin-symbolic",
    recent => "document-open-recent-symbolic",
    settings => "preferences-system-symbolic",
    trash => "edit-delete-symbolic",
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
