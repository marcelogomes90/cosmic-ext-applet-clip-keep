use cosmic_clip_keep::clip::db::{Db, NewEntry, Stored};
use cosmic_clip_keep::clip::model::{Capture, EntryId, EntryKind, Flavor, Thumbnail};
use cosmic_clip_keep::clip::settings::Settings;
use cosmic_clip_keep::clip::{dedup, thumbnail};

const T0: i64 = 1_700_000_000_000;

fn text(body: &str) -> Capture {
    Capture {
        kind: EntryKind::Text,
        flavors: vec![Flavor::new(
            "text/plain;charset=utf-8",
            body.as_bytes().to_vec(),
        )],
        source_app: None,
    }
}

fn store(db: &mut Db, capture: &Capture, at: i64) -> Stored {
    let hash = dedup::hash(capture);
    let preview = dedup::preview(capture);

    db.store(
        &NewEntry {
            hash: &hash,
            kind: capture.kind,
            preview: &preview,
            source_app: capture.source_app.as_deref(),
            flavors: &capture.flavors,
            thumbnail: None,
        },
        at,
    )
    .expect("the store should accept a well-formed capture")
}

fn labels(db: &Db, settings: &Settings) -> Vec<String> {
    db.list(settings)
        .expect("listing should succeed")
        .into_iter()
        .map(|entry| entry.label().to_owned())
        .collect()
}

#[test]
fn a_capture_is_stored_with_every_flavor_it_offered() {
    let mut db = Db::in_memory().unwrap();
    let capture = Capture {
        kind: EntryKind::Text,
        flavors: vec![
            Flavor::new("text/plain;charset=utf-8", b"hello".to_vec()),
            Flavor::new("text/html", b"<b>hello</b>".to_vec()),
        ],
        source_app: Some("com.system76.CosmicTerm".into()),
    };

    let stored = store(&mut db, &capture, T0);
    assert!(stored.is_new());

    let entries = db.list(&Settings::default()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].preview, "hello");
    assert_eq!(entries[0].byte_size, 17);
    assert_eq!(
        entries[0].source_app.as_deref(),
        Some("com.system76.CosmicTerm")
    );

    let primary = db.load(stored.id(), None).unwrap().unwrap();
    assert_eq!(primary.mime, "text/plain;charset=utf-8");

    let flavors = db.load_all(stored.id()).unwrap();
    assert_eq!(flavors.len(), 2);
    assert_eq!(flavors[1].mime, "text/html");
}

#[test]
fn copying_the_same_thing_again_bumps_the_entry_instead_of_duplicating_it() {
    let mut db = Db::in_memory().unwrap();

    let first = store(&mut db, &text("token"), T0);
    let second = store(&mut db, &text("token"), T0 + 1000);

    assert!(first.is_new());
    assert!(!second.is_new(), "the second copy is the same content");
    assert_eq!(first.id(), second.id());

    let entries = db.list(&Settings::default()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].use_count, 2);
    assert_eq!(entries[0].last_used_at, T0 + 1000);
    assert_eq!(entries[0].created_at, T0, "the original time is kept");
}

#[test]
fn the_list_puts_the_most_recently_used_first() {
    let mut db = Db::in_memory().unwrap();

    store(&mut db, &text("oldest"), T0);
    store(&mut db, &text("middle"), T0 + 1000);
    store(&mut db, &text("newest"), T0 + 2000);
    store(&mut db, &text("oldest"), T0 + 3000);

    assert_eq!(
        labels(&db, &Settings::default()),
        ["oldest", "newest", "middle"]
    );
}

#[test]
fn pinned_entries_sit_above_the_rest() {
    let mut db = Db::in_memory().unwrap();

    let pinned = store(&mut db, &text("kept"), T0).id();
    store(&mut db, &text("ordinary"), T0 + 1000);
    db.set_pinned(pinned, true).unwrap();

    assert_eq!(labels(&db, &Settings::default()), ["kept", "ordinary"]);
}

#[test]
fn pruning_keeps_the_newest_and_never_touches_a_pin() {
    let mut db = Db::in_memory().unwrap();

    let precious = store(&mut db, &text("precious"), T0).id();
    db.set_pinned(precious, true).unwrap();

    for index in 0..10 {
        store(&mut db, &text(&format!("entry {index}")), T0 + 1000 + index);
    }

    let settings = Settings {
        max_entries: 3,
        max_age_days: None,
        ..Settings::default()
    };
    let removed = db.prune(&settings, T0 + 100_000).unwrap();

    assert_eq!(removed, 7);
    assert_eq!(
        labels(&db, &settings),
        ["precious", "entry 9", "entry 8", "entry 7"],
        "the pin plus the three newest survive"
    );
}

#[test]
fn pruning_by_age_also_spares_pins() {
    let mut db = Db::in_memory().unwrap();
    let day = 86_400_000;

    let old_pin = store(&mut db, &text("old but pinned"), T0).id();
    db.set_pinned(old_pin, true).unwrap();
    store(&mut db, &text("old and loose"), T0);
    store(&mut db, &text("recent"), T0 + 29 * day);

    let settings = Settings {
        max_age_days: Some(30),
        ..Settings::default()
    };
    db.prune(&settings, T0 + 31 * day).unwrap();

    let mut kept = labels(&db, &settings);
    kept.sort();
    assert_eq!(kept, ["old but pinned", "recent"]);
}

#[test]
fn clearing_spares_pins_unless_asked_not_to() {
    let mut db = Db::in_memory().unwrap();
    let pinned = store(&mut db, &text("kept"), T0).id();
    store(&mut db, &text("ordinary"), T0 + 1000);
    db.set_pinned(pinned, true).unwrap();

    db.clear(false).unwrap();
    assert_eq!(labels(&db, &Settings::default()), ["kept"]);

    db.clear(true).unwrap();
    assert!(labels(&db, &Settings::default()).is_empty());
}

#[test]
fn deleting_an_entry_takes_its_content_thumbnail_and_pin_with_it() {
    let mut db = Db::in_memory().unwrap();

    let png = {
        let image = image::RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]));
        let mut buffer = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut buffer),
                image::ImageFormat::Png,
            )
            .unwrap();
        buffer
    };

    let capture = Capture {
        kind: EntryKind::Image,
        flavors: vec![Flavor::new("image/png", png.clone())],
        source_app: None,
    };
    let hash = dedup::hash(&capture);
    let generated: Thumbnail = thumbnail::generate(&png).expect("a valid png has a thumbnail");

    let id = db
        .store(
            &NewEntry {
                hash: &hash,
                kind: EntryKind::Image,
                preview: "",
                source_app: None,
                flavors: &capture.flavors,
                thumbnail: Some(&generated),
            },
            T0,
        )
        .unwrap()
        .id();

    db.set_pinned(id, true).unwrap();
    assert_eq!(db.thumbnail(id).unwrap().unwrap().width, 8);
    assert_eq!(
        db.list(&Settings::default()).unwrap()[0].image_size,
        Some((8, 4))
    );

    db.delete(id).unwrap();

    assert!(db.list(&Settings::default()).unwrap().is_empty());
    assert!(db.load(id, None).unwrap().is_none());
    assert!(db.thumbnail(id).unwrap().is_none());

    assert!(!db.contains(&hash).unwrap());
}

#[test]
fn an_unknown_entry_answers_with_nothing_rather_than_failing() {
    let db = Db::in_memory().unwrap();
    let missing = EntryId(999);

    assert!(db.load(missing, None).unwrap().is_none());
    assert!(db.load_all(missing).unwrap().is_empty());
    assert!(db.thumbnail(missing).unwrap().is_none());
    assert!(db.kind(missing).unwrap().is_none());

    db.touch(missing, T0).unwrap();
    db.delete(missing).unwrap();
}

#[test]
fn the_list_is_capped_at_the_configured_size() {
    let mut db = Db::in_memory().unwrap();
    for index in 0..20 {
        store(&mut db, &text(&format!("entry {index}")), T0 + index);
    }

    let settings = Settings {
        max_entries: 5,
        ..Settings::default()
    };
    assert_eq!(db.list(&settings).unwrap().len(), 5);

    let pinned = db.list(&settings).unwrap()[0].id;
    db.set_pinned(pinned, true).unwrap();
    assert_eq!(db.list(&settings).unwrap().len(), 6);
}

#[test]
fn a_second_connection_sees_the_change_counter_move() {
    let dir = std::env::temp_dir().join(format!("clip-keep-shared-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the temporary directory should be writable");
    let path = dir.join("history.db");

    let mut writer = Db::open(&path).expect("the history should open");
    let reader = Db::open(&path).expect("a second connection should open");

    let before = reader
        .data_version()
        .expect("the counter should be readable");
    store(&mut writer, &text("copied on the other monitor"), T0);
    let after = reader
        .data_version()
        .expect("the counter should be readable");

    assert_ne!(
        before, after,
        "a write from elsewhere must move the counter"
    );
    assert_eq!(
        after,
        reader.data_version().unwrap(),
        "the counter must hold still while nothing writes"
    );

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_selection_echoed_back_within_a_moment_is_not_counted_again() {
    let mut db = Db::in_memory().expect("an in-memory history should open");
    let capture = text("pasted again");

    let stored = store(&mut db, &capture, T0);
    db.touch(stored.id(), T0 + 10).expect("a use should record");
    store(&mut db, &capture, T0 + 20);

    let entries = db.list(&Settings::default()).unwrap();
    assert_eq!(
        entries[0].use_count, 2,
        "the click and the echo another instance captures must count once"
    );

    store(&mut db, &capture, T0 + 60_000);
    let entries = db.list(&Settings::default()).unwrap();
    assert_eq!(entries[0].use_count, 3, "a later copy still counts");
}
