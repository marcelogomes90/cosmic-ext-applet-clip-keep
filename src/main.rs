use cosmic_ext_applet_clip_keep::config::SettingsStore;
use cosmic_ext_applet_clip_keep::{APP_ID, applet, clip, i18n, init_tracing};

fn main() -> cosmic::iced::Result {
    init_tracing();
    i18n::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting clip keep");

    let settings = SettingsStore::open(APP_ID).load();

    let (handle, _capture) = clip::spawn(settings);

    applet::run(handle)
}
