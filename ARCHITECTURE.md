# Architecture

Clip Keep watches the COSMIC compositor's clipboard, stores what passes through it, and offers it
back from a panel applet. This document covers *why* the seams are where they are — most of them
exist to defend against a specific failure, and two of them exist because a COSMIC applet lives in
two places at once.

## Layout

```
src/
├── clip/       the clipboard itself: Wayland, storage, policy. No iced, no libcosmic.
│   ├── wayland/  data_control.rs reader.rs writer.rs toplevel.rs   protocol and transfers
│   ├── runtime.rs                                                  the calloop actor loop
│   ├── db.rs model.rs dedup.rs                                     sqlite, wire types, hashing
│   └── mime.rs privacy.rs search.rs settings.rs thumbnail.rs       what to keep and what to drop
├── applet/     the COSMIC applet: iced views and presentation state
│   ├── view/   the popup: history page, settings page, previews
│   └── keys.rs message.rs subscription.rs thumbs.rs
├── config.rs   the settings entry, stored through cosmic_config
└── bin/        cosmic-clip-keep-dump, a headless clipboard watcher
```

## The flow

```
cosmic-comp  (Wayland, ext-data-control or wlr-data-control)
        │  selection offers, one per copy
        ▼
  clip::wayland::Device        one Manager, one Device, one seat
        │  Selection::Current(offer)
        ▼
  clip::runtime::Runtime       one calloop event loop on its own thread
        │  Transfer → slots → privacy::Filter → dedup::hash → Db::store
        ▼
  Snapshot                     immutable value, published on a tokio::sync::watch channel
        │  applet::subscription turns the watch channel into an iced Subscription
        ▼
  applet::ClipKeep             iced/libcosmic views, popup, thumbnails
```

Everything crossing the middle is a plain value. `clip` never calls into `applet`; it publishes a
`Snapshot` and lets the UI catch up whenever it can. The applet talks back only through
`ClipCommand` (`src/clip/mod.rs`), a small enum whose only replies travel over one-shot channels.

**`src/clip` is free of iced and libcosmic, and that is load-bearing.** It is what lets the whole
capture path be exercised by `cosmic-clip-keep-dump` with no display server and no iced runtime,
which is how every Wayland-facing change in this repository gets tested. The check is mechanical,
and `just layering` runs it:

```sh
grep -rn 'cosmic::\|iced::\|use cosmic\b' src/clip/    # must print nothing
```

## The two sockets

This is the central invariant of the applet, and the one most likely to be broken by an innocent
refactor.

A COSMIC applet has **two** connections to two different compositors, and they must not be
confused:

- **`WAYLAND_SOCKET`** is an open file descriptor cosmic-panel passes in the environment. It leads
  to the panel's *own nested compositor*, and libcosmic consumes it to place the panel button.
- **`$WAYLAND_DISPLAY` under `$XDG_RUNTIME_DIR`** leads to cosmic-comp, the session compositor. It
  is the only one that implements data-control, and it is the one capture must use.

`clip::wayland::connect` therefore ignores `WAYLAND_SOCKET` outright and reaches the session
compositor by path. Taking that descriptor would break the applet twice over in a way that looks
like two unrelated bugs: `wayland-client`'s `connect_to_env` claims the descriptor *and clears the
variable*, so libcosmic then falls back to `WAYLAND_DISPLAY` and draws an ordinary floating window
instead of a panel surface — while capture ends up talking to the panel's nested compositor, which
offers no data-control at all.

`X-HostWaylandDisplay=true` in the desktop entry is what keeps this deterministic. cosmic-panel
strips `WAYLAND_DISPLAY` from an applet's environment unless the entry asks for it, and without it
`candidates()` is reduced to sweeping `wayland-0` through `wayland-8` and hoping.

## Every monitor runs its own copy

cosmic-panel spawns one applet process per output. Each has its own Wayland connection, its own
SQLite connection, and its own `Snapshot`. Nothing coordinates them, so a copy recorded by the
instance on one monitor is invisible to the instance on the other until it re-reads the database.

`Runtime::history_changed_elsewhere` closes that gap with SQLite's `PRAGMA data_version`, which
moves only when a *different* connection commits. The event loop already wakes at least twice a
second, and the pragma is a single page read, so it is polled there rather than watching the file:

```rust
event_loop.run(Duration::from_millis(500), &mut runtime, |runtime| {
    if runtime.dirty || runtime.history_changed_elsewhere() {
        runtime.publish();
    }
});
```

`publish` records the version it read, so an instance never republishes on its own writes.
`a_second_connection_sees_the_change_counter_move` in `tests/history.rs` pins the mechanism.

The database itself is opened in WAL mode with a five second busy timeout, because concurrent
writers are the normal case here rather than an edge one.

## Reading a selection

The requirement that shapes this whole path: **reading a selection must never hold up the
application that owns it.** A data-control read is a pipe the source application has to fill, and a
naive blocking read makes copying a large image out of a browser freeze the browser.

`mime::choose` decides what is worth asking for before anything is read — one image format, or one
URI list, or plain text plus its HTML alternative — so an offer of thirty flavours costs at most
two pipes. Each requested type becomes a `Slot` on a `Transfer`, read through a non-blocking pipe
registered with the event loop (`clip/wayland/reader.rs`, 64 KiB per read, 4 MiB per burst before
yielding). Size ceilings come from `mime::size_limit`: 5 MiB for text and URI lists, 20 MiB for
images.

Two timers guard a transfer. `TRANSFER_TIMEOUT` (2 s) bounds the whole thing. `SECONDARY_GRACE`
(300 ms) is armed the moment the *primary* slot completes, and abandons whatever is still
outstanding — the alternative flavours are a nicety, and an application that has already handed
over the text it was asked for should not keep the entry out of the history because it is slow with
the HTML.

Three things stop an entry that should not exist:

- **`privacy::Filter`.** Private mode refuses offers before they are read at all. The
  `x-kde-passwordManagerHint` flavour is requested alongside the content and, when it reads
  `secret`, the whole capture is dropped. A slot that *failed* to read counts as secret, because
  the safe reading of an unknown answer to "is this a password" is yes.
- **`is_reflected_selection`.** Putting an entry back on the clipboard makes this process the
  selection owner, and the compositor immediately announces that selection to us. Comparing the
  incoming hash against the hash we own stops a paste from re-recording itself.
- **Priming.** The first offer after startup is whatever was already on the clipboard, typically
  from before the applet existed. If the database already knows it, it is left alone rather than
  bumped to the top of the list on every login.

`dedup::hash` is a SHA-256 over the entry kind and the primary flavour only, with text trimmed
first, so the same snippet copied from two applications that disagree about trailing whitespace or
about which alternative flavours to publish is one entry. That hash is `UNIQUE` in the schema, and
`Db::store` turns a collision into `Stored::Repeated`: `last_used_at` moves and `use_count` rises,
so the list orders by recency of *use* rather than of capture.

`source_app` comes from `clip/wayland/toplevel.rs`, which tracks the focused toplevel through
`ext-foreign-toplevel-list` and the COSMIC toplevel-info protocol. The clipboard carries no notion
of who copied; the focused window at the moment of the offer is the closest available answer.

## What is stored, and what is not

Entries live in one SQLite file at `$XDG_DATA_HOME/cosmic-clip-keep/history.db`, chmod `0600` on
creation. Content bodies live in a separate `contents` table keyed by mime, thumbnails in a third,
pins in a fourth, all cascading on delete.

The list the applet renders is `EntryMeta` — id, kind, a truncated preview string, byte size,
source, timestamps, and thumbnail dimensions. **No body ever reaches the applet as part of a
snapshot.** A full image is loaded only while it is previewed, over `ClipCommand::Thumbnail` and
`ClipCommand::Load`, and dropped when the preview closes. This is what keeps a hundred-entry
history of screenshots from being a hundred screenshots in memory. Thumbnails are generated once at
capture, capped at `thumbnail::MAX_EDGE` (256 px).

Pins are the exception to every limit. `Db::list` orders by `(not pinned, pin position, last used,
id)` and raises its own `LIMIT` by the number of pins; `Db::prune` excludes pinned rows from both
the count limit and the age limit; `Db::clear` spares them unless explicitly told not to. A pin is
a statement that this entry should outlive the policy, so no policy may quietly overrule it.

## The popup

`view::popup` builds one surface, sized by `autosize` with `max_height(SURFACE_MAX_HEIGHT)` and a
minimum of one pixel, so the popup is as tall as its content and no taller. Everything inside is `Length::Shrink`
for the same reason — with one deliberate exception.

**When a preview is open the list reclaims `Length::Fill`.** iced's flex layout lays out non-fill
children in order, each seeing what the previous ones left, so an all-`Shrink` column lets a long
history consume the entire height and lays the preview panel out with nothing. Making the list the
fill child puts it last in the layout order and the fixed-height preview gets its space back. The
popup then sits at its maximum for as long as a preview is open, which is also steadier than having
it resize under the pointer as previews are toggled.

Two things in the popup work around gaps in libcosmic rather than expressing a preference:

- **The scrollbar has no width.** libcosmic's `Scrollable` theme offers `Permanent` and `Minimal`
  and both paint a thumb; there is no transparent variant to select. `view::scroll` sets
  `scrollbar_width`, `scroller_width` and `scrollbar_padding` to zero instead, which keeps wheel and
  touch scrolling and paints nothing.
- **A pinned row's glyph is coloured by hand.** `Button::Icon` computes an accent colour for its
  selected state and then discards it — libcosmic leaves `icon_color` unset unless the button is
  disabled — so `.selected(true)` shows up only on hover. `view::accent_icon` borrows the stock
  icon appearance through the `Catalog` trait and overrides the two colour fields.

Padding lives on the children of each page rather than on a wrapper around it. That is what lets a
scrollable span the full width, so its content keeps the same margins while the scroll area itself
reaches the popup edge.

Private mode gets no notice in the list. The panel button already changes to
`changes-prevent-symbolic`, and repeating it in the list spends a row on something the user just
turned on themselves. Only `CaptureState::Unavailable`, which is a real failure with a reason worth
reading, earns a line.

Every glyph the applet names must exist in the COSMIC icon theme, which is narrower than Adwaita:
`window-pin-symbolic`, `window-unpin-symbolic` and `view-reveal-symbolic` do not, and a machine
running a fuller icon theme will hide that from you. A new glyph has to be checked against
`/usr/share/icons/Cosmic` *and* against the theme bundled in `com.system76.Cosmic.BaseApp`, which
is what the Flatpak actually sees.

`resources/icons/preview-symbolic.svg` is the one glyph the applet draws itself, embedded with
`include_bytes!` rather than installed. The theme carries no plain eye — its only one,
`image-red-eye-symbolic`, is struck through and therefore means *hidden*, the opposite of the
action. `icon::from_svg_bytes(..).symbolic(true)` sets the same flag a themed `-symbolic` name
gets, so the colour in the file is ignored and the glyph follows the panel ink like every other.

## Packaging

The Flatpak asks for seven permissions. Four are ordinary; three need explaining.

**`--filesystem=/run/user` is what makes capture work at all.** Current Flatpak — 1.18 at the time
of writing — hands a sandboxed application a Wayland socket it creates through
`wp_security_context_manager_v1`, and
cosmic-comp does not expose data-control on a security-context connection. The socket Flatpak
provides is therefore useless for a clipboard manager, and the host's real socket has to be
reachable by path. It is measurable in one command:

```sh
flatpak run --command=cosmic-clip-keep-dump io.github.marcelogomes90.cosmic-ext-applet-clip-keep
#=> clipboard capture is off — no data-control

flatpak run --filesystem=/run/user --command=cosmic-clip-keep-dump io.github.marcelogomes90.cosmic-ext-applet-clip-keep
#=> bound data-control protocol="ext_data_control_manager_v1"
```

`X_PRIVILEGED_WAYLAND_SOCKET`, which cosmic-panel offers to applets asking for
`X-HostWaylandDisplay`, is **not** a substitute: the panel creates it through the same
security-context protocol.

**The two `xdg-config/cosmic` grants are split on purpose.** `cosmic_config::Config::new` reads and
writes `$XDG_CONFIG_HOME/cosmic/<id>/v<n>` on the filesystem; the `dbus-config` feature only routes
*change notifications* through the settings daemon. So a read-only view of `~/.config/cosmic` is
needed for the theme, and `:create` on this applet's own subdirectory — and nothing else — is what
lets its settings persist.

What is deliberately **not** there:

- **`--socket=inherit-wayland-socket`.** cosmic-panel injects it on the `flatpak run` command line
  for every Flatpak applet it launches, so declaring it in the manifest is redundant and leaves the
  application with no display when it is launched any other way.
- **`--device=dri`.** libcosmic is built here without the `wgpu` feature, so rendering is software.
- **Any access to `$HOME`.** An image is recorded when the image itself is on the clipboard. Reading
  a file a copied URI points at would mean holding read access to the user's documents for the sake
  of a thumbnail, which is a poor trade for a process that already sees everything they copy.

One rule keeps the offline build working: **regenerate `cargo-sources.json` whenever `Cargo.lock`
gains a package** — `just flatpak-sources`. Adding a dependency edge to a crate that is already
vendored does not need it, because the generator enumerates the packages in the lock file rather
than the graph.

## How to test

`cosmic-clip-keep-dump` is the tool that matters. It runs the entire capture path headless, prints
the capture state and the history as it changes, and can put text on the clipboard with `--copy` or
offer the newest entry back with `--offer-latest`. Pointing it at a throwaway `XDG_DATA_HOME` gives
a clean database, which is how multi-instance behaviour gets exercised without touching a real
history.

```sh
just verify    # fmt-check, clippy -D warnings, layering, tests, metadata validation
just run-dump  # headless: watch the live clipboard
just test
```

Unit tests live at the bottom of the module they cover, in a `#[cfg(test)] mod tests`, and are named
as sentences describing the invariant (`the_named_display_is_tried_before_the_numbered_sweep`)
rather than after the function under test. Storage tests live in `tests/history.rs`, against a real
SQLite file rather than an in-memory one where the behaviour under test is cross-connection.

There are no comments in the source. What a reader needs to know that the code cannot say is here
instead, where it can be read in one sitting and cannot drift out of sight of the person changing
the thing it describes.
