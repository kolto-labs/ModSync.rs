# Desktop toolkit replacement

Decision: Qt 6 Quick/Quick Controls with CXX-Qt 0.10.0, retaining the Rust
engine, durable authoring sessions, virtual filesystem, checkpoint storage and
CLI. The complete egui replacement is authorized and incomplete.

The decisive requirements are a substantial desktop file explorer, switchable
Explorer/Dolphin/Finder presentations, large virtual directory inventories,
keyboard and assistive technology support, patcher configuration, rich guide
content, responsive long jobs, and Windows/macOS/Linux deployment. Visual polish
requires deliberately designed components; no toolkit makes it automatic.

| Candidate | Fit | Material drawback |
| --- | --- | --- |
| Qt Quick + CXX-Qt | Best desktop control/model foundation, custom scene graph, split panes, tree/table views, platform accessibility, Rust worker queue | Native SDK and deployment plugins, Rust/C++ bridge, LGPL obligations |
| Tauri 2 + React Aria | Strongest web component ecosystem, CSS freedom, Rust IPC, frontend hot reload | Different system webviews, WebKitGTK dependency/graphics failures, cross-engine behavior and IPC pagination |
| Electron + React Aria | Consistent Chromium, mature desktop integration | Bundled browser runtime, larger downloads/memory, Rust process integration |
| Flutter + Rust bridge | Unified renderer and broad desktop/mobile/web targets | Dart toolchain and binding lifecycle, file-explorer model/interaction work |
| Slint | Rust-oriented declarative visuals, renderer options, live preview | More explorer interaction/model work; attribution/license choice |
| Iced | Rust retained UI, custom drawing | Accessibility integration remains an open upstream issue; substantial explorer behavior to implement |

Qt Quick's native Windows/macOS control styles restrict deep customization.
Use customizable Basic/Fusion-based application controls for the switchable
explorer presentations. The shared Qt Quick file dialog is the default; native
dialogs, if added as an option, must be explicit.
Use a model backed by predicted entries, never QFileSystemModel as the authority
for virtual state. Lazy directory queries and view virtualization must avoid
recreating a complete game tree for every selection, keypress or frame.
CXX-Qt is the established integration selected here; Qt's newer official Rust
Bridge is still beta. Keep expensive operations in Rust workers, returning
results on the Qt event thread. UI cancellation, session locks and generation
identities must survive errors, view changes and shutdown.

Qt Core/Gui/Qml/Quick/Quick Controls are available under LGPLv3. Dynamically link
and retain deployment notices, library sources/version records and required
replaceability; do not introduce GPL-only add-on modules into this BSL-licensed
application. No commercial Qt license is assumed purchased. CXX-Qt is MIT/Apache.
This is an engineering packaging constraint, not a claim of completed compliance.

Migration acceptance:
- Restore all current wizard, validation/install, acquisition/sharing, profiles,
  protocols/single-instance, recovery/checkpoint, maintenance and patcher paths.
- Port authoring autosave/undo/redo and source import; keep all existing revisions
  usable and actual disk actions gated through the same Rust engine.
- Explorer needs breadcrumbs/history, tree/sidebar, details/icons/columns, sorting,
  search, modifier/range selection, inline rename, context menus, keyboard actions,
  accessible drag/drop, resizing, and explicit unknown/unobserved content states.
- Built-in OdyPatcher and supplied-installer authoring remain backend operations;
  migration cannot substitute a physical filesystem or generated demonstration.
- Verify real sessions and per-platform keyboard/screen-reader/DPI/rendering.
  Bundle a licensed common text font and icons for consistent metrics.
  UI consistency is a requirement to verify, not an automatic toolkit guarantee.
- Provide development QML reload retaining the Rust session; shipped data remains
  embedded. Production engine behavior must never depend on a dev QML directory.
- Replace default GUI only after equivalent workflows are implemented and checked;
  preserving legacy source temporarily protects work, not a completed replacement.
- Existing Rust-test/harness freeze remains; production builds and native receipts
  do not establish six-lane per-mod install parity.

Authoritative research (2026-10-09):
- https://doc.qt.io/qt-6.11/qtquickcontrols-index.html
- https://doc.qt.io/qt-6.11/qml-qtquick-treeview.html
- https://doc.qt.io/qt-6/qtquickcontrols-customize.html
- https://doc.qt.io/qt-6/qtquick-performance.html
- https://doc.qt.io/qt-6/licensing.html
- https://www.qt.io/development/qt-framework/ui-accessibility
- https://kdab.github.io/cxx-qt/book/
- https://github.com/KDAB/cxx-qt/releases/tag/v0.10.0
- https://www.qt.io/blog/qt-bridge-for-rust-0.3-cxx-qt-compatibility-and-soundness
- https://v2.tauri.app/concept/architecture/
- https://v2.tauri.app/reference/webview-versions/
- https://v2.tauri.app/start/prerequisites/
- https://react-aria.adobe.com/Tree
- https://react-aria.adobe.com/Virtualizer
- https://github.com/slint-ui/slint
- https://github.com/iced-rs/iced/issues/552

Current first migration slice: independent desktop/qt production crate using the
same parent Rust library without egui. Full migration and default cutover pending.

Cross-platform clarification: use identical application controls, explorer styles
and Rust action semantics on all desktop platforms. Do not use OS-specific native
control styles as the application default. Windows/macOS/Linux are required.
Qt mobile/WebAssembly support does not prove ModSync installer capability on
mobile/web; retain portability without promising unsupported filesystem/process
access. Flutter was also reviewed for its unified renderer and broad targets; Qt
was preferred for this desktop model/view and existing Rust-engine integration.
Sources: https://doc.qt.io/qt-6.11/supported-platforms.html and
https://docs.flutter.dev/reference/supported-platforms .

Local SDK finding: host Qt runtime libraries are 6.11.2 while installed
development tools were 6.11.1. The old QML compiler/linter crashed when loading
the newer private compiler libraries. Matching Fedora 6.11.2 development RPMs
were downloaded and extracted into a private build SDK; system packages were
not modified. Package/tool/runtime version matching belongs in build and
packaging checks on every target. Locked CXX-Qt 0.10.0 production builds and strict Rust Clippy passed with the
matching private SDK. The actual saved K1 session opened in the owned Linux Xvfb
window; read-only archive prediction and undo/redo (1 → 0 → 1) preserved saved
head hashes and left both preview roots empty. This is local software-rendering
evidence; Windows/macOS, GPU and screen-reader proof remain pending. GCC 16
emits Qt header SFINAE warnings; strict Rust Clippy is not warning-free C++ proof.

The first slice also includes archive import, durable revision seek and recorded
actions through existing Rust authoring APIs, plus breadcrumbs, sorting and
keyboard navigation. Noto Sans Regular/Bold and their verified upstream OFL
license are embedded for consistent desktop metrics. A complete model-based
explorer, the other existing application workflows and platform proof are still
required before replacing the default GUI.

Folder navigation currently uses Qt TreeView with Qt 6.10+ TreeModel fed only by
the Rust predicted directory inventory. This first snapshot-backed presentation
is not a lazy large-game model: move directory lookup/sorting and incremental
changes behind a native QAbstractItemModel boundary before claiming scalability.
The desktop crate requires Qt 6.10 or newer for this initial TreeModel.

The current explorer adds distinct Explorer details, Dolphin icon and Finder
ancestor-column presentations over the same actual predicted entries, with
human-readable sizes and shared search/sort/navigation. Modifier/range selection,
rename/context actions, drag/drop and lazy native model scaling remain pending.

All three current presentations were observed with the real first-mod archive
prediction; Finder navigation reached the recommended option’s dialog.tlk.
Saved authoring JSON hashes were unchanged and both preview roots stayed empty.
A shared /tmp tmpfs user quota interrupted prediction and a Clippy rebuild;
private NVMe TMPDIR resolved both without changing other processes or their files.
The updated layout build, strict Rust Clippy and both formatting checks passed.

Recording contract correction: the form now sends canonical PascalCase
instruction fields. The Qt bridge rejects unknown field names and unsupported
action kinds before authoring a revision, generates its own GUID/parent, and
converts selected absolute paths to the existing root tokens. All three browser
views share toggle/range selection and record-copy/move/rename/delete menus.
These menus open the saved-recipe form; they do not directly mutate game files.
Newly recorded actions advance the preview point and invalidate old predictions.

Actual recording evidence: context-menu Move created revision 3 for the real first
K1 mod, moving its recommended dialog.tlk to the main game root with overwrite.
Canonical serialized fields and root tokens were inspected in the saved revision.
The virtual source became empty and the virtual game root showed the file; real
roots stayed empty. Undo/redo restored the same durable head (cursor 2 → 1 → 2).
Ctrl-click and Shift-range selection were observed on the real extracted entries.
No supplied installer was run and no manual-install parity is claimed.

The saved session reopened in a fresh Qt process with both actual actions intact.
A recorded-action list exposes their saved definitions and selects the matching
preview point. The inspector now presents readable fields and opens editing or
confirmed removal through durable recipe transactions.

Saved-action maintenance now uses shared Rust ReplaceInstruction and
RemoveInstruction transactions. Forms capture component, index, instruction GUID
and authoring revision; the bridge rejects stale revisions or moved/replaced
actions. Edits preserve GUID, parent binding, dependencies and restrictions,
normalize root paths and skip byte-equivalent instruction changes. Removal is
confirmed in the interface and remains recoverable through recipe undo. The
inspector presents readable source/destination/options fields rather than raw JSON.

Actual maintenance receipts: saving the unchanged real Move preserved all saved
authoring JSON hashes. Editing overwrite changed that field only, preserving
GUID, parent, dependency arrays and the rest of the guide; undo restored the
original. Removing Move changed only that instruction; undo restored the entire
original guide. Prior immutable files and the abandoned edit revision remained
intact, and both real roots stayed empty. Production build, strict Rust Clippy and
Qt/root formatting passed. Stale-revision refusal is enforced in source but was
not exercised through a separate client. Full replacement and install parity
remain unproven.

Qt now exposes read-only built-in OdyPatcher inspection through the existing
Session.inspect_patcher/Engine predicted-filesystem path. The captured revision,
component and instruction boundary are checked; source edits invalidate the
result. Installer options, plain-text description and planned changes are shown,
and recording carries the selected source and namespace argument into the
durable form. The actual Character Startup Changes installer was read from a
virtual Extract prefix; description and changes.ini were shown without changing
saved-session hashes or either real root. This installer has no namespaces, so
multiple-option interaction remains unverified. Both supplied main/patch archives
were hash-verified and imported using the shared Qt dialog. The second mod now
has authored Extract + Patcher after the first mod’s Extract + Move. Its exact
patch prediction refuses the empty game root as an invalid KOTOR installation;
no fake baseline or unknown-content success substitutes for that refusal. The
required loose-file patch is imported but follow-up actions are not yet authored.
Build, strict Rust Clippy and formatting passed. Installation/parity, supplied
external patcher execution, complete warning-rich full.md presentation and full
GUI replacement remain incomplete.
