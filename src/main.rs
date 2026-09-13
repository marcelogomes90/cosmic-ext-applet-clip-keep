use cosmic_ext_applet_clip_keep::config::SettingsStore;
use cosmic_ext_applet_clip_keep::{APP_ID, applet, clip, control, i18n, init_tracing, shortcut};

fn main() -> cosmic::iced::Result {
    init_tracing();

    if std::env::args()
        .skip(1)
        .any(|argument| argument == control::TOGGLE)
    {
        control::toggle();
        return Ok(());
    }

    i18n::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting clip keep");

    let store = SettingsStore::open(APP_ID);
    let settings = store.load();

    shortcut::register_once(&store);

    let (handle, _capture) = clip::spawn(settings);

    applet::run(handle)
}
