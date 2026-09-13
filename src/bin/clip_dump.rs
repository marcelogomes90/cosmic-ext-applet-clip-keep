use std::sync::Arc;
use std::time::Duration;

use cosmic_ext_applet_clip_keep::clip::model::{CaptureState, Flavor, Snapshot};
use cosmic_ext_applet_clip_keep::clip::{ClipCommand, ClipHandle};
use cosmic_ext_applet_clip_keep::config::SettingsStore;
use cosmic_ext_applet_clip_keep::{APP_ID, clip, init_tracing};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    init_tracing();

    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let paste = arguments.iter().any(|argument| argument == "--paste");
    let offer_latest = paste
        || arguments
            .iter()
            .any(|argument| argument == "--offer-latest");
    let copy = arguments
        .iter()
        .position(|argument| argument == "--copy")
        .and_then(|at| arguments.get(at + 1))
        .cloned();

    let settings = SettingsStore::open(APP_ID).load();
    let (handle, _capture) = clip::spawn(settings);

    if arguments
        .iter()
        .any(|argument| argument == "--target-output")
    {
        match target_output(&handle).await {
            Some(output) => println!("--> the shortcut would open on {output}"),
            None => println!("--> no output could be named, so the first instance would answer"),
        }
        return;
    }

    if let Some(text) = copy {
        println!("--> putting {} bytes on the clipboard", text.len());
        handle.send(ClipCommand::Offer {
            flavors: vec![Flavor::new("text/plain;charset=utf-8", text.into_bytes())],
        });
    }

    let mut snapshots = handle.subscribe();
    let mut last_state: Option<CaptureState> = None;
    let mut offered = false;

    println!("watching the clipboard; press Ctrl+C to stop");

    loop {
        let snapshot = {
            let borrowed = snapshots.borrow_and_update();
            Arc::clone(&borrowed)
        };

        report(&snapshot, &mut last_state);

        if offer_latest && !offered {
            offered = offer_newest(&handle, &snapshot, paste);
        }

        if snapshots.changed().await.is_err() {
            println!("the capture thread stopped");
            return;
        }
    }
}

async fn target_output(handle: &ClipHandle) -> Option<String> {
    for _ in 0..20 {
        if let Some(output) = handle.target_output().await {
            return Some(output);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    None
}

fn report(snapshot: &Snapshot, last_state: &mut Option<CaptureState>) {
    if last_state.as_ref() != Some(&snapshot.capture) {
        println!("--- capture: {:?}", snapshot.capture);
        *last_state = Some(snapshot.capture.clone());
    }

    println!(
        "--- revision {} ({} entries)",
        snapshot.revision,
        snapshot.entries.len()
    );

    for entry in snapshot.entries.iter().take(20) {
        println!(
            "  {:>5}  {:?}  {:>8} B  used {:>3}  {:<20}  {}",
            entry.id.0,
            entry.kind,
            entry.byte_size,
            entry.use_count,
            entry.source_app.as_deref().unwrap_or("-"),
            entry.label(),
        );
    }
}

fn offer_newest(handle: &ClipHandle, snapshot: &Snapshot, paste: bool) -> bool {
    let Some(entry) = snapshot.entries.first() else {
        return false;
    };

    println!(
        "--> offering entry {} back to the clipboard (paste: {paste})",
        entry.id
    );
    handle.send(ClipCommand::Use {
        id: entry.id,
        paste,
    });
    true
}
