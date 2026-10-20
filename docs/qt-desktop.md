# Qt desktop

Qt Quick frontend for the Rust ModSync engine. It is the default GUI toolkit
(cargo feature `ui-qt`) and lives in the main package under `src/ui_qt`. The egui
frontend remains available as the small-binary alternative (`ui-egui`). The Qt
frontend covers the easy-mode authoring, preview and disk-action slice, the
install wizard (guide, folders, selection, validation, conflict analysis, Ready to
Install, the "Install selected mods?" snapshot modal, install progress, stop and
restore), the desktop Settings dialog (general flags, telemetry consent and
privacy, sharing and peer settings, profiles, patcher and Nexus key), the
sharing edit gate, and the Dark, Light, K1 and K2 themes. Downloads, managed
deployments, merge, provider updates, checkpoint history, maintenance, the
standalone patcher, the guide editor, FOMOD choices and structured logs are still
reachable only in egui builds.

Use a matching Qt 6.10 or newer SDK, development tools and runtime (current local SDK 6.11.2),
including Core, Gui, Network, Qml, Quick and Quick Controls. CXX-Qt is pinned to
0.10.0. On Linux use an available LLD, mold or gold linker as required by CXX-Qt.
Set QMAKE when multiple SDKs are installed.

From the repository root:

```sh
cargo build --locked --bin modsync -j2
cargo clippy --locked --lib --bin modsync -j2 -- -D warnings
cargo fmt --all -- --check
```

The same sources build the egui frontend with
`cargo build --locked --no-default-features --features ui-egui --bin modsync`.
Run `modsync gui`; `--easy-session <dir>` opens an authoring session directly.

Use Session to open an existing
authoring session or create one from a Markdown/JSON/YAML/TOML guide, and choose
existing game/mod roots. A new session needs an absolute, unused directory whose
parent exists, outside both roots. Guide input is read through the bounded shared
reader, preserving authored selections; an existing session is never replaced.
Sessions use the same exclusive lease and
durable revisions as the CLI and current GUI; close the other owner first.
Preview uses the Rust virtual filesystem and does not apply disk actions. Archive
import and recording actions update the saved authoring session. Undo/redo seeks
saved recipe revisions, not installed disk history.

The separate Disk actions dialog applies the next recorded action in guide order,
restores disk checkpoints, and recovers pending actions through the shared engine.
It captures recipe/action identities and configured folders when asking to proceed;
changed requests are rejected. Application and restore progress can be cancelled.
The confirmation/cancel path was checked in the Linux software-rendered GUI
without changing game/mod roots. These controls have production compilation
and strict Clippy checks; successful disk execution/recovery is not yet proven here.
Do not treat UI checks as fresh-install or patcher parity evidence.

New action forms offer immediate mode, off at application startup. When enabled,
Save records the instruction and applies it through Session's shared disk engine
with checkpoints, progress and cancellation. Captured folders and revision must
still match. The candidate must be the next ordered action, with no pending
recovery or unapplied disk redo. Editing an existing action only edits the recipe.
A disk failure preserves the saved recipe and reports checkpoint recovery guidance.
The owned Linux GUI showed the mode/folders notice and rejected an out-of-order
new action without changing any saved recipe hashes or creating a disk journal.
At 1000x660 the form scrolls while Save/Cancel remain available. Successful
immediate execution, failure recovery and other platforms remain unproven.

The real K1 full.md was opened through the owned Linux Qt window as a separate
186-mod authoring session. Repeating creation refused the existing directory
while preserving the open session. The existing K1 recipe also contains all 186
guide mods and remains intact;
its required startup loose-file patch now has recorded Extract and Copy steps.
Those instructions already belong to the existing complete-guide recipe. Neither
session is a completed manual installation. Production build, strict Rust Clippy
and formatting passed; other guide formats and platforms remain unchecked here.

Easy-mode guide creation now includes explicitly named setup mods from the
introduction, preserving main-list GUIDs and the reference importer's behavior.
Session also offers an undoable repair for older sessions, requiring the same
original main-list identities, order and introduction. The actual TSL session
now contains 148 components: 3C-FD Patcher, Water Restoration, and Stutter Fix
and Force Cage Update precede the original 145 entries. Their selections remain
off until game-version requirements are reviewed. Existing components, document
metadata and immutable revisions were preserved; GUI undo/redo restored both
saved states. No supplied executable was run and no game baseline is established.

Guide notes exposes the saved introduction, widescreen warning and after-build
content as read-only Markdown with headings, emphasis and selectable text. Custom
guide admonition markers remain visible as literal text. Section changes create
a fresh document view and reset scrolling without replacing the authoring backend.
The actual TSL introduction and after-build content, empty-section message and
1000x660 viewport were observed. Named widescreen-tail mods now have authoring
rows through the shared authoring reader and missing-only repair. The actual K1
session contains 197 named entries and TSL 154, matching every original Name field
in exact source order. Existing components, metadata and immutable revisions were
preserved; GUI Undo/Redo restored both states. New tail selections remain off and
their section requirements are visible. Session offers an undoable repair of known
guide masters through the shared name/heading/author resolver. K1 robe icons now
requires Cloaked Jedi Robes; the two unmatched labels remain unchanged. A repeated
repair preserved every saved hash, and GUI Undo/Redo restored both revisions.
Requirements appear in mod notes. Row coverage is not
authored executable instructions, clean-install evidence or patcher parity.

Shared Basic-based controls and embedded Noto Sans text fonts support consistent
desktop presentation. Shared Quick file dialogs preserve application styling.
Explorer details, Dolphin icons and Finder columns browse the same predicted
inventory. Rich explorer interactions, complete workflow porting, accessibility and per-platform validation
remain pending. See `../../docs/plans/desktop-toolkit-migration.md`.

No test or fixture scaffolding is included; repository parity restrictions apply.

On hosts with constrained shared temporary storage, set TMPDIR to an existing
private directory on the active fast disk for both Cargo and the app. Archive
prediction requires scratch space even when it leaves game roots unchanged.

Development QML reload is available in debug builds. Set
`MODSYNC_QML_DEV_SOURCE` to the absolute path of the real `src/ui_qt/qml/EasyBuilder.qml` before
launching `modsync gui`. The bounded regular-file source is watched, including
atomic editor saves. Rust remains an engine-owned QML singleton. Reload waits
while the backend is busy and replaces the window only after a candidate loads;
failed creation keeps the prior window. Mod, folder, presentation and sorting
are carried over. Dialog forms, selections, navigation history and action-point
position reset, and immediate mode returns to off. Release builds ignore the
development source setting and load embedded QML.

Production Qt build, strict Rust Clippy and formatting passed for the initial
implementation. An actual atomic title/subtitle edit appeared in the same Linux
process while retaining the K1 session and selected mod; all saved JSON hashes
were unchanged. The native redraw correction passed production compilation. A subsequent actual
heading edit repainted completely without manual resizing, retained the selected
mod and session, and allowed GUI Undo/Redo to restore exact saved hashes. Busy
deferral, failed-candidate retention and other platforms have source implementation
only, without separate runtime receipts. No game/mod files were changed.

Explorer selections expose Copy, Move, Extract and Inspect patcher actions in a
selection toolbar and context menu. Double-click or Enter on a recognized archive
opens the existing extraction recording dialog; extraction starts with the mod
source root as its destination. Folders retain normal navigation. Patcher
inspection preserves the selected build point and document revision before
opening the existing installer preview. These paths use the same recording,
preview and immediate-action confirmation behavior as other authored actions.
The changed interface loaded and hot-reloaded in the actual Qt app on isolated
Xvfb :198 with an empty runtime error log. Selection dialogs, installed-file
results and cross-platform behavior have not yet been verified for this slice.
For Xvfb launches always clear WAYLAND_DISPLAY and set QT_QPA_PLATFORM=xcb;
DISPLAY alone does not override an inherited Wayland environment.

Copy, Move, Extract and Patcher forms also offer a destination browser using
the current predicted directory inventory. Mod-source/game shortcuts and Up
keep selection within the configured roots. OK fills the action destination;
Cancel leaves it unchanged, and neither choice saves a recipe or writes files.
This follows the original C# instruction editor's destination Browse capability.
The chooser passed QML source parsing (only the expected unresolved custom
module warning); live chooser validation awaits completion of the active K1
prediction, because hot reload correctly defers while the backend is busy.

Rebuilt production Qt acceptance on isolated Xvfb :199 verified the actual K1
authoring session at revision 78. Destination browsing displayed both roots:
Cancel preserved the prior game destination, while OK filled the chosen source
root. Canceling the enclosing action left revision 78 and zero disk actions.
No recipe was saved. Runtime stderr remained empty. Explorer selection and
patcher argument acceptance still require completion of its pinned-input preview.

Qt authoring now exposes Execute without claiming opaque executable changes are
predictable. Recorded executable actions require explicit execution trust and a
configured headless runner before immediate apply or the next disk action. The
runner settings form accepts existing private request storage outside configured
roots, Xvfb, and an optional dedicated Wine prefix with matching Wine/wineserver.
Settings are runtime-only and reset when reopening the session; saving them
does not launch installers. The core owns isolated displays and prefix cleanup.
Explorer-selected absolute patcher paths normalize through the core root-safe
variable-path conversion before inspection. This prevents the observed absolute
path rejection without weakening path boundaries. These source changes await
the next production build and actual GUI acceptance.
Guide rows now support arrow, Home, End and Page navigation with a visible focus
perimeter. The selection toolbar reserves its height, fixing the observed row
shift on first selection that interrupted double-click activation. These changes
also await the next actual runtime acceptance.

Explorer **New folder** records a virtual `CreateDirectory` action with an empty source and options. Its destination includes the new folder name beneath an existing parent. Recording starts in preview mode; actual creation uses the ordinary disk confirmation and checkpoint flow. Existing folders are idempotent.

Long operations show elapsed time and the current recovery phase. Hover or click the footer status for the complete message; Activity keeps a bounded recent status history and selectable full current errors. Stop still requests cancellation through the backend and does not bypass checkpoint cleanup.
