# Architecture

Clip Keep is a COSMIC panel applet with two halves: a clipboard service and a libcosmic user
interface. They communicate with plain messages and immutable snapshots, so clipboard I/O never
blocks rendering and the backend can run without the applet UI.

## Components

```text
src/
├── clip/         Wayland clipboard access, policy, SQLite storage and thumbnails
├── applet/       panel button, popup, keyboard handling, details and settings UI
├── control.rs    local socket used by the global shortcut
├── shortcut.rs   one-time Super+V registration
├── config.rs     persistent cosmic-config settings
└── main.rs       startup and wiring
```

`src/clip` deliberately has no iced or libcosmic dependency. It runs a calloop event loop on its
own thread and exposes a `ClipHandle`. The applet receives `Snapshot` values through a watch channel
and sends `ClipCommand` values back for operations such as using, pinning, deleting or clearing an
entry.

## Clipboard flow

```text
COSMIC data-control offer
        ↓
non-blocking MIME transfers
        ↓
privacy filter and content deduplication
        ↓
SQLite history and thumbnail generation
        ↓
Snapshot → applet subscription → popup
```

The backend connects directly to the session compositor rather than the panel's nested Wayland
socket. It selects only the useful MIME flavours, reads them through non-blocking pipes with size
and time limits, rejects private/password-marked content when configured, and hashes the primary
content to avoid duplicate entries.

A text selection keeps more than the plain body. The best plain-text spelling stays first, because
the hash, the preview and the search all read it, and every other text flavour the source offers
follows it, bounded in number. The other spellings of plain text are left out, since they would
only be the same bytes again; so are an application's own bookkeeping targets and any flavour whose
body a kept flavour already holds. Nothing about the primary changed, so a history recorded before
this still dedupes and previews exactly as it did.

The database stores bodies separately from the metadata rendered by the list. The UI receives a
short preview and requests only the thumbnails it needs. Pins are ordered first and are exempt from
history size, age and normal clear operations.

COSMIC starts one applet process per output. Each process has its own backend and database
connection; SQLite's data version lets them notice commits made by another instance and publish a
fresh snapshot.

## Opening and controlling the popup

The panel button toggles the popup. Super+V launches the executable with `--toggle`; that short-lived
process asks the running instances over Unix sockets, and the instance on the active window's output
opens. If no instance can claim the output, one responder is chosen as a fallback.

The normal applet popup cannot obtain a valid grab when it was opened by the global shortcut,
because that key serial belongs to the session compositor rather than the panel's nested connection.
Clip Keep therefore opens an ungrabbed XDG popup and briefly creates a transparent one-pixel layer
surface with exclusive keyboard interactivity. Once focus arrives, the helper becomes on-demand so
focus can move away on an outside click.

The helper and main popup are tracked as one set of owned surfaces. Focus loss is deferred briefly;
when focus moves to another client, the popup and helper are destroyed.

Clicking the panel button while the popup is open arrives as that focus loss first, so the popup is
already gone when the button press itself is delivered. A short guard after such a dismissal keeps
that press from reopening the popup it just closed, so the second click toggles as everywhere else.

The search field is focused only after the popup reports that it opened. Application-level keyboard
handling keeps navigation working even when an individual widget consumed the event:

- ordinary text edits the search;
- arrows move the selected row and scroll it into view;
- Enter uses the selected entry;
- Ctrl+I, Ctrl+P, Ctrl+D and Ctrl+F open details, pin, delete and focus search;
- Escape returns from details or settings to the list, then closes the popup.

Selection from the pointer and selection from the keyboard share the same `focused` entry. Scrolling
uses measured widget bounds rather than estimated row heights because headings, dividers and image
rows have different sizes.

## Details

Details are a normal page of the main popup, never another Wayland surface. The row menu and Ctrl+I
open the focused entry. The page uses metadata already present in the snapshot:
preview text, source application, timestamps, use count, byte size and optional image dimensions.
The stored flavours are not in the snapshot; they are asked for when the page opens, the way
thumbnails are, and are shown by name rather than by MIME so that the page reads like the rest of
it. An image entry shows its thumbnail instead of the preview text, bounded so that the stored
thumbnail is never scaled up. A text excerpt is capped at a height that leaves room for the
information below it and scrolls within that, so however many lines an entry holds the page itself
never grows.
Because the page stays on the main surface, opening it cannot move keyboard focus or interfere with
outside-click dismissal.

## Using an entry

Choosing an entry makes Clip Keep the clipboard owner and closes the popup. A text entry is also
advertised under the names a destination may ask for instead — `text/plain`, `UTF8_STRING`,
`STRING` and `TEXT` — each served from the body already stored rather than a copy of it, so the
destination picks the flavour it wants and an entry recorded before any of this reaches the same
places a new one does. If automatic paste is
enabled, the backend waits for the surfaces to disappear, reactivates the previously focused
toplevel and sends Ctrl+V through the virtual-keyboard protocol. Normalized IDs for known standalone
terminals use Ctrl+Shift+V, while the XTerm family uses Shift+Insert. IDEs stay on Ctrl+V because
their editor and embedded terminal share one application ID. Dismissing the popup never types
anything.

## Settings and presentation

Settings are stored through cosmic-config and propagated to every running instance. Capture policy
lives in `clip::settings::Settings`, which also carries how large image thumbnails are drawn in the
list. The smallest size is the default, so a history in use looks the same until it is changed, and
no size asks for more pixels than a stored thumbnail has. A thumbnail is rounded by the tile's own
radius less the inset around it, so it follows the theme rather than a number of its own.

Each row carries a menu holding details, pin and delete. It is a popover over the same surface,
placed from the measured bounds of the button that opened it, and libcosmic's own popover clamps it
into the surface and flips it above the row when there is no room below. Three things about it are
easy to undo by accident. The mouse area that catches a click outside the menu and the popover
itself stay mounted whether or not a menu is open, because swapping the widgets above the page
rebuilds the tree underneath and drops the overlay for a frame. While a menu is open the rows stop
reporting pointer movement, so crossing them on the way to an item does not move the selection. And
only a deliberate choice closes a menu — focus changes, pointer moves and fresh snapshots leave it
alone, while Escape closes the menu before it closes the page.

Every surface in the popup is painted from the two theme tokens libcosmic's own dropdown uses for
its panel. Icons split two ways: the chrome the desktop already has a word for — settings, delete,
the row menu — is looked up by name (`preferences-system-symbolic`, `edit-delete-symbolic`,
`view-more-symbolic`) so it matches every other applet, and everything the COSMIC theme has no glyph
for is an SVG compiled into the binary. Both render identically inside the Flatpak sandbox, because
the COSMIC icon theme is bundled in `com.system76.Cosmic.BaseApp`.

The header is fixed and only the list beneath it scrolls, so the body is capped at the surface
maximum minus a generous allowance for the chrome around it. The list carries no footer of its own:
each row menu names its shortcut beside the action, the way COSMIC's own menus do. The details page
keeps a footer, because it has no menu to hold Escape and Enter. The popup follows the panel anchor
and theme, sizes itself to its contents up to that maximum. Fluent catalogues under `i18n/` provide
every visible string. `just verify` checks formatting, clippy, backend/UI layering, tests and
metadata.
