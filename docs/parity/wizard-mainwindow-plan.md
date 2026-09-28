# Complete install wizard and legacy main-window parity plan

Status: plan only, 2026-09-27. Nothing in this document is implementation proof. Pinned C# oracle: `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Rust baseline audited at `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`. The source inventory and exact control deltas are in [wizard-mainwindow-audit.md](wizard-mainwindow-audit.md) (`docs/parity/wizard-mainwindow-audit.md:1-278`).

This slice targets F24 and F25. It must preserve the accepted install-progress, completion-pages, mod-selection and validation work rather than reopening their product rules (`docs/parity/ACCEPTANCE.md:3-29`, `docs/parity/install-progress-plan.md:1-16`, `docs/parity/completion-pages-plan.md:1-27`, `docs/parity/mod-selection-plan.md:38-116`). It cannot close F18 editor parity, F29 real full-build/game parity, or platform accessibility cells that have not been driven (`docs/parity/ACCEPTANCE.md:53-64`).

## Binding constraints

- Preserve the dirty, user-owned `AGENTS.md` and every unrelated tracked/untracked path. Implementation workers own only the files assigned below; documentation workers never rewrite source (`AGENTS.md:1-9`, `docs/parity/wizard-mainwindow-audit.md:1-8`).
- Do not create, modify, hide, compile, or run Rust tests, inline `#[cfg(test)]` modules, test fixtures presented as tests, or Rust harnesses until full C# parity. Verification uses production CLI/native binaries, disposable directories, direct input driving, filesystem manifests, accessibility-tree inspection and independent review (`AGENTS.md:3-9`, `docs/parity/install-progress-plan.md:5-16`).
- Do not build during this planning/audit slice. Later implementation builds run on the host with a task-owned `CARGO_TARGET_DIR` and `-j 2`; no task shares target directories under the saturated disk condition (user build constraint; `AGENTS.md:3-4`).
- `src/ui.rs` is a serialized integration file. The existing order is install-progress, then completion-pages, then mod-selection; this plan's first `src/ui.rs` edit starts only after all three owners have delivered and their reviews have settled. Validation-page behavior is also preserved (`docs/parity/install-progress-plan.md:189-200`, `docs/parity/completion-pages-plan.md:109-118`, `docs/parity/mod-selection-plan.md:247-258`, `docs/parity/GUI-INVENTORY.md:47-53`).
- `src/engine.rs` is serialized separately. This plan expects no engine changes. If a control needs a missing business fact or operation, the presentation task stops, records the exact API gap, and queues one additive library task after the active engine owner; the UI must not reimplement guide order, selection, download, validation, install, recovery, conflict, profile, FOMOD, or checkpoint behavior (`AGENTS.md:2-8`, `src/engine.rs:1-40`, `docs/parity/GUI-CLI.md:1-18`).
- Preserve guide order and stop-on-first-failure. A retry begins only after verified restore; no wizard or legacy-main-window action may backfill an earlier mod at the end (`AGENTS.md:5-7`, `src/ui.rs:386-445`, `src/ui.rs:2256-2289`).
- Validation remains non-mutating and never starts an external program. Presentation may show validation facts, but it cannot weaken the shared pipeline gate (`AGENTS.md:5-8`, `src/ui.rs:386-425`, `src/ui/validation_page.rs:269-455`).
- Preserve deliberate Rust safety corrections already accepted: truthful install terminal state, Continue after committed success, Stop/close confirmation, explicit install confirmation, recovery-state truth, stale-result rejection, Aspyr enforcement, and documented selection-policy deviations (`docs/parity/GUI-INVENTORY.md:1-37`, `docs/parity/mod-selection-plan.md:38-116`, `src/ui.rs:530-639`, `src/ui.rs:3138-3166`).
- Match the active `WizardHostControl`, not the older `InstallWizardDialog`. `LoadInstructionPage` remains outside the active route as a landing/main-window surface (`src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:124-184`, `docs/parity/wizard-mainwindow-audit.md:9-17`).
- A source declaration, successful build, screenshot or accessible-looking label is not keyboard/screen-reader parity. A claim requires direct input evidence and an inspected native accessibility tree on the named platform (`docs/parity/GUI-INVENTORY.md:127`, `docs/parity/GUI-INVENTORY.md:168-174`).
- Update `COMPATIBILITY.md`, `docs/parity/GUI-INVENTORY.md`, `docs/parity/GUI-CLI.md`, and `docs/parity/ACCEPTANCE.md` whenever supported behavior or a proof boundary changes (`AGENTS.md:8`, `docs/parity/ACCEPTANCE.md:59-60`).

## C# findings that govern the design

1. The active wizard has no step-navigation sidebar. It has a theme/header/progress region, current-page content, an optional compact download status bar, transient status/progress, and Back/Next/Finish footer controls (`src/ModSync.GUI/Controls/WizardHostControl.axaml:14-188`).
2. Download Center is created in code, not XAML: Fetch Downloads, Open Workspace, a status button/LED/running line, progress text, Stop Downloads and a background-download tip (`src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:219-362`).
3. Route construction is conditional and post-base widescreen pages are inserted immediately before Finished (`src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:124-217`).
4. C# Preamble is rendered Markdown, while Aspyr and Widescreen warning content are raw text (`src/ModSync.GUI/Dialogs/WizardPages/PreamblePage.axaml.cs:42-54`, `src/ModSync.GUI/Dialogs/WizardPages/AspyrNoticePage.axaml.cs:36-44`, `src/ModSync.GUI/Dialogs/WizardPages/WidescreenNoticePage.axaml.cs:42-48`).
5. The main-window menu is programmatic and contains File, Tools, Help, About and More trees; no Rust menu exists today (`src/ModSync.GUI/Services/MenuBuilderService.cs:658-1026`, `src/ui.rs:2900-2948`).
6. The C# Summary surface attaches a Markdown renderer to display text and uses explicit normal/spoiler fields through `SpoilerFreeContentConverter`; spoiler mode hides URLs behind a warning expander (`src/ModSync.GUI/Controls/SummaryTab.axaml:57-75`, `src/ModSync.GUI/Controls/SummaryTab.axaml:214-418`, `src/ModSync.GUI/Controls/SummaryTab.axaml.cs:95-189`).
7. Explicit keyboard behavior is narrow but real: directory Enter handling, editor mod-list Ctrl+Up/Ctrl+Down/Delete/Space, and embedded-log keyboard copy/select handling (`src/ModSync.GUI/Controls/DirectoryPickerControl.axaml.cs:678-707`, `src/ModSync.GUI/MainWindow.axaml.cs:2110-2167`, `src/ModSync.GUI/Controls/EmbeddedLogPanel.axaml.cs:226-251`).
8. The audited C# controls do not declare explicit `AutomationProperties.Name`, access keys or focus order. The native behavior is therefore an observation target, not a source contract to copy blindly (`docs/parity/wizard-mainwindow-audit.md:5-11`, `docs/parity/wizard-mainwindow-audit.md:219-229`).

## Decisions fixed by this plan

### 1. Use one route model and page-local render modules

`src/wizard.rs` remains the source for page identity, order, titles, subtitles and navigation gates. Page layout and page-local presentation state move out of the monolithic `wizard_content` match into `src/ui/wizard_pages.rs` plus narrowly named modules where a page already has an approved owner (`src/wizard.rs:5-89`, `src/wizard.rs:129-225`, `src/ui.rs:642-769`).

`src/ui.rs` retains only application state, worker/result binding, modal orchestration, shell integration and calls into page renderers. It does not acquire a second route enum or copy title/gate predicates (`src/ui.rs:386-445`, `src/ui.rs:2949-3105`).

### 2. Treat shared facts and presentation as different contracts

The library owns guide metadata, selection outcomes, download jobs/events, validation reports, install progress, completion receipts, recovery, conflicts, profiles, FOMOD state and file operations. GUI modules format those facts and keep ephemeral focus, expansion, scroll, copied-feedback and theme state. CLI adapters serialize the same library outputs without instantiating GUI types (`src/lib.rs:1-35`, `docs/parity/GUI-CLI.md:1-18`, `docs/parity/install-progress-plan.md:24-141`, `docs/parity/mod-selection-plan.md:116-225`).

No new shared projection module is justified for static page copy, cards, menus, Markdown layout, focus or accessible labels. Those are presentation. A missing business fact is an API gap and triggers the engine/library stop rule above (`docs/parity/install-progress-plan.md:118-141`, `docs/parity/completion-pages-plan.md:29-84`).

### 3. Preserve approved page owners

- `src/ui/installing_page.rs` remains the only Base/Remaining Installing renderer. This plan may position it in the final shell but does not change progress semantics (`docs/parity/install-progress-plan.md:155-200`, `src/ui.rs:648-650`).
- `src/ui/completion_pages.rs` from the completion-pages plan owns Base Complete, Widescreen Notice, Widescreen Complete and Finished. This plan integrates it but does not rewrite its receipt rules or truthful deviations (`docs/parity/completion-pages-plan.md:29-118`).
- `src/ui/mod_selection_page.rs` from the mod-selection plan owns base Mod Selection. This plan supplies shell/accessibility acceptance and does not rewrite selection policy (`docs/parity/mod-selection-plan.md:38-199`, `docs/parity/mod-selection-plan.md:247-258`).
- `src/ui/validation_page.rs` remains the Validation renderer and keeps its report/progress lifecycle (`docs/parity/GUI-INVENTORY.md:47-53`, `src/ui/validation_page.rs:269-455`).

### 4. Reproduce the active shell, with recorded Rust safety controls

Match the active C# header geometry, page scroll boundary, compact download status surface, transient status, persistent Download Center from Download Process onward, and Back/Next/Finish labels. Keep Rust Stop Install, Restore snapshot, Retry phase, Restart wizard, close confirmation and install-confirmation modal as documented safety additions (`src/ModSync.GUI/Controls/WizardHostControl.axaml:14-188`, `.axaml.cs:219-362`; `src/ui.rs:530-639`, `src/ui.rs:3138-3166`).

At 600×400, navigation and recovery controls remain reachable and page content remains scrollable. Exact C# clipping is not a parity target when it makes the safety controls unreachable (`docs/parity/GUI-INVENTORY.md:29-37`, `docs/parity/completion-pages-plan.md:106-108`).

### 5. Add a presentation-only CommonMark renderer

Add a small `src/ui/rich_content.rs` adapter over a reviewed CommonMark event parser. It renders the subset observed in C# authored content—paragraphs, emphasis/strong, headings, lists, inline/code blocks, links and line breaks—into egui widgets. It never parses a guide, executes HTML, fetches remote content, mutates metadata, or changes CLI output (`src/ModSync.GUI/Dialogs/WizardPages/PreamblePage.axaml.cs:42-54`, `src/ModSync.GUI/Controls/SummaryTab.axaml.cs:95-189`).

The implementation adds the parser dependency only after a license/version/API review against Rust 1.85 and eframe 0.32.3. Raw text remains available to copy; unsupported constructs render as literal text with a visible diagnostic rather than disappearing (`Cargo.toml:9-38`, `src/model.rs:175-175`).

### 6. Make the legacy main window a first-class mode, not a second installer

Build `src/ui/main_window.rs` for top chrome, menu bar, Home, mode switching, landing, tabs, activity/log, drag/drop and loading states. Menu and legacy controls call existing application methods or library APIs. They do not create alternate install/validation/download pipelines (`src/ModSync.GUI/MainWindow.axaml:50-489`, `src/ModSync.GUI/Services/MenuBuilderService.cs:658-1026`, `src/ui.rs:771-1594`, `src/ui.rs:2900-3105`).

`src/ui/summary_page.rs` owns the F25 read-only Summary cards, Markdown display and spoiler variants. `src/ui/editor.rs` or current editor code remains the implementation of F18 authoring behavior; this plan wires the tab/control container but does not claim F18 closure from a visible tab (`docs/parity/ACCEPTANCE.md:53-60`, `src/ModSync.GUI/Controls/SummaryTab.axaml:42-662`, `src/ui.rs:2667-2840`).

### 7. Accessibility names are explicit where semantics are not self-evident

Visible text is the primary name for text buttons, toggles, tabs and menu items. Icon-only theme/window/status controls receive deterministic accessible names; progress exposes value/max and a stable label; changing validation/download/install states are exposed as status updates; modal focus is trapped and restored; disabled controls retain an explanation (`src/ModSync.GUI/Controls/WizardHostControl.axaml:22-98`, `src/ModSync.GUI/MainWindow.axaml:109-183`, `docs/parity/GUI-INVENTORY.md:127`).

The implementation must use the accessibility facilities actually exposed by eframe/egui 0.32.3 with its enabled `accesskit` feature; no plan claim assumes a method name that has not been verified in the installed crate source (`Cargo.toml:11-11`, `Cargo.toml:37-37`, `Cargo.lock:1568-1670`).

## Control-to-task trace

| Audit surface | Owning task | Files | Acceptance source |
|---|---|---|---|
| Active shell/header/progress/footer/download chrome | Task 2 | `src/ui/wizard_shell.rs`, serialized `src/ui.rs` integration | `docs/parity/wizard-mainwindow-audit.md:25-39`; C# `WizardHostControl.axaml:14-188`, `.axaml.cs:219-362` |
| Welcome, Preamble, Aspyr, directory pages, Downloads Explain, Install Start | Task 3 | `src/ui/wizard_pages.rs`, `src/ui/rich_content.rs` | `docs/parity/wizard-mainwindow-audit.md:40-126` |
| Base Mod Selection | Prior approved owner; Task 2/7 only integrates/probes | `src/ui/mod_selection_page.rs`, `src/ui.rs` | `docs/parity/mod-selection-plan.md:199-330` |
| Validation | Existing owner; Task 2/7 only integrates/probes | `src/ui/validation_page.rs`, `src/ui.rs` | `docs/parity/GUI-INVENTORY.md:47-53` |
| Installing pages | Prior approved owner; Task 2/7 only integrates/probes | `src/ui/installing_page.rs`, `src/ui.rs` | `docs/parity/install-progress-plan.md:155-260` |
| Completion pages and Widescreen Notice | Prior approved owner; Task 2/7 only integrates/probes | `src/ui/completion_pages.rs`, `src/ui.rs` | `docs/parity/completion-pages-plan.md:78-150` |
| Widescreen Mod Selection | Task 3 | `src/ui/wizard_pages.rs` | `docs/parity/wizard-mainwindow-audit.md:145-152`; C# `WidescreenModSelectionPage.axaml:9-16`, `.axaml.cs:37-90` |
| Top-level main window and menus | Task 4 | `src/ui/main_window.rs`, serialized `src/ui.rs` integration | `docs/parity/wizard-mainwindow-audit.md:168-187`, `docs/parity/wizard-mainwindow-audit.md:209-218`; C# `MainWindow.axaml:50-489`, `MenuBuilderService.cs:658-1026` |
| Landing, Getting Started, Mod sidebar, Raw container, structured log | Task 5 | `src/ui/main_window.rs` plus bounded child modules | `docs/parity/wizard-mainwindow-audit.md:188-208` |
| Summary rich text and spoiler variants | Task 6 | `src/ui/summary_page.rs`, `src/ui/rich_content.rs` | `docs/parity/wizard-mainwindow-audit.md:230-239`; C# `SummaryTab.axaml:42-662` |
| Keyboard/accessibility names/focus/status | Task 7 | All final UI modules, one serialized `src/ui.rs` integration | `docs/parity/wizard-mainwindow-audit.md:219-229`; `docs/parity/GUI-INVENTORY.md:127` |

## File ownership and task sequence

Tasks that touch `src/ui.rs` are strictly serialized. New-module work may be prepared in parallel only after the prerequisite freeze, with one owner per file and no integration edits until its turn.

| Task | Owned files | Starts after | Responsibility |
|---|---|---|---|
| 0. Refresh and freeze | Read-only source plus a task-owned receipt outside source | install-progress, completion-pages and mod-selection are delivered | Re-resolve Rust HEAD, dirty state, final module APIs, line anchors and open review findings. Stop if an approved owner is still editing `src/ui.rs` or if its plan contract changed (`docs/parity/install-progress-plan.md:189-200`, `docs/parity/completion-pages-plan.md:109-118`, `docs/parity/mod-selection-plan.md:247-258`). |
| 1. Rich-content primitive | `Cargo.toml`, `Cargo.lock`, `src/ui/rich_content.rs` | Task 0 | Add presentation-only CommonMark rendering and copy/raw fallback. No `src/ui.rs`, engine, loader or CLI changes (`src/ModSync.GUI/Dialogs/WizardPages/PreamblePage.axaml.cs:42-54`, `src/ModSync.GUI/Controls/SummaryTab.axaml.cs:95-189`). |
| 2. Active wizard shell | `src/ui/wizard_shell.rs` (new), then `src/ui.rs` integration | Tasks 0 and prior UI owners | Exact active shell and Download Center presentation; preserve existing route/worker/safety controls. This is the first task in this plan allowed to edit `src/ui.rs` (`src/ModSync.GUI/Controls/WizardHostControl.axaml:14-188`, `.axaml.cs:219-362`; `src/ui.rs:2949-3085`). |
| 3. Unplanned wizard pages | `src/ui/wizard_pages.rs` (new), then its serialized `src/ui.rs` wiring | Tasks 1-2 | Welcome, Preamble, Aspyr, both directory pages, Downloads Explain, Install Start and Widescreen Selection. Delegate approved pages to their existing modules (`src/ui.rs:642-769`, `docs/parity/wizard-mainwindow-audit.md:40-167`). |
| 4. Main-window shell and menu | `src/ui/main_window.rs` (new), then its serialized `src/ui.rs` wiring | Task 3 | Top chrome, menu tree, native-window control decision, Home/mode switching, tab host, overlays and action routing. No duplicated business logic (`src/ModSync.GUI/MainWindow.axaml:50-489`, `src/ModSync.GUI/Services/MenuBuilderService.cs:658-1026`). |
| 5. Legacy child surfaces | `src/ui/getting_started.rs`, `src/ui/log_panel.rs`, `src/ui/mod_sidebar.rs`, `src/ui/raw_page.rs` (new), then serialized integration in `src/ui/main_window.rs` | Task 4 | Reproduce F25 legacy landing/onboarding/sidebar/raw/log controls. Editor-specific nested field behavior remains F18-owned (`src/ModSync.GUI/Controls/GettingStartedTab.axaml:47-568`, `ModListSidebar.axaml:14-259`, `RawTab.axaml:12-97`, `EmbeddedLogPanel.axaml:11-150`). |
| 6. Summary/spoiler surface | `src/ui/summary_page.rs` (new), reuse `src/ui/rich_content.rs`, then serialized integration in `src/ui/main_window.rs` | Tasks 1 and 4 | Exact read-only information/download/content/options/instructions cards, Markdown display, spoiler projections and URL-warning expander (`src/ModSync.GUI/Controls/SummaryTab.axaml:42-662`, `.axaml.cs:95-189`). |
| 7. Keyboard/accessibility closure | Final UI modules; one owner serially edits `src/ui.rs` last | Tasks 2-6 | Focus model, names/help, status announcements, keyboard bindings, modal trap/return and disabled explanations. Do not infer success from source declarations (`docs/parity/GUI-INVENTORY.md:127`, `docs/parity/wizard-mainwindow-audit.md:219-229`). |
| 8. Direct acceptance and documentation | `docs/parity/wizard-mainwindow-native/` (new), `docs/parity/GUI-INVENTORY.md`, `docs/parity/GUI-CLI.md`, `docs/parity/ACCEPTANCE.md`, `COMPATIBILITY.md` | Task 7 source reviews | Production CLI/native probes, reference/Rust captures, accessibility receipts, final status and proof boundaries. No source rewrites (`AGENTS.md:8`, `docs/parity/ACCEPTANCE.md:59-60`). |

No task edits `src/engine.rs`. If Task 2-6 discovers a missing shared operation, it emits an API request with the exact caller, inputs, output/events, failure/cancellation semantics and CLI consumer. A separate library owner may implement it only after current engine work is complete; the presentation task then resumes from that approved API (`AGENTS.md:2-8`, `docs/parity/GUI-CLI.md:1-18`).

## Task details

### Task 0: refresh and freeze

1. Record Rust HEAD, pinned C# HEAD, `git status --short`, and blob hashes for `src/ui.rs`, `src/wizard.rs`, `src/ui/*.rs`, the three approved plans and their integration evidence. This is necessary because the current checkout already contains unrelated untracked parity plans and a modified `AGENTS.md` (`docs/parity/wizard-mainwindow-audit.md:1-8`).
2. Confirm `src/ui/completion_pages.rs` and `src/ui/mod_selection_page.rs` exist or obtain their final owner-approved replacements before relocating any match arm. Confirm install-progress and validation public entry points by symbol, not stale line number (`docs/parity/completion-pages-plan.md:109-118`, `docs/parity/mod-selection-plan.md:247-258`, `src/ui.rs:644-650`).
3. Stop on any unresolved P0/P1 review finding in those slices. This plan integrates accepted contracts; it does not mask their open findings with a shell rewrite (`docs/parity/install-progress-plan.md:248-260`, `docs/parity/completion-pages-plan.md:141-150`, `docs/parity/mod-selection-plan.md:310-340`).

### Task 1: rich-content primitive

1. Review the selected CommonMark event parser's license, Rust 1.85 support, no-network behavior and exact API before changing manifests. Keep the dependency optional under the existing desktop feature so `--no-default-features` stays GUI-free (`Cargo.toml:9-38`).
2. Render headings, paragraphs, emphasis/strong, ordered/unordered lists, inline/code blocks, links and hard/soft breaks. Reject/escape raw HTML and never fetch images or URLs automatically; C# authored content is display data, not executable UI instructions (`src/ModSync.GUI/Dialogs/WizardPages/PreamblePage.axaml.cs:42-54`).
3. Expose presentation calls that accept `&mut egui::Ui`, raw text and a stable widget ID. Return unsupported-node diagnostics for Activity/receipts and retain byte-equal raw copy access; do not expose types from this module through the library crate (`src/ui.rs:6-12`, `src/lib.rs:1-35`).
4. Direct probe the renderer through a production desktop screen using authored Markdown with every supported construct, raw HTML, malformed links, long lines and Unicode. No Rust test or harness is created (`docs/parity/wizard-mainwindow-audit.md:230-239`).

### Task 2: active wizard shell

1. Move header/footer/download presentation behind `wizard_shell` functions while leaving application state and route transitions in `App`. Keep `Wizard` as the only route owner (`src/wizard.rs:120-225`, `src/ui.rs:386-445`).
2. Match theme selector, title/subtitle, step text, 200×6 progress, main content scroll, compact download status, transient status/progress and footer geometry from active XAML (`src/ModSync.GUI/Controls/WizardHostControl.axaml:14-188`).
3. Reproduce the code-created 320 px Download Center control set. Route Fetch/Stop/status/open through the approved download APIs/plan; until aggregate fetch exists, disable Fetch with truthful help instead of locally inventing queue state (`src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:219-362`, `docs/parity/download-orchestration-plan.md:1-8`, `src/ui.rs:470-528`).
4. Preserve Stop Install, Restore, Retry, Restart, close confirmation and install confirmation. Their visibility derives from typed run/recovery state, never log text (`src/ui.rs:581-635`, `src/ui.rs:3138-3166`, `src/ui/installing_page.rs:237-375`).
5. Keep the footer reachable at 600×400 and give content the remaining space before adding nested scrolling. Do not reproduce C# clipping that hides recovery actions (`docs/parity/GUI-INVENTORY.md:29-37`).

### Task 3: unplanned wizard pages

1. Port Welcome's hero, two comparison cards, five-step process, four key features, fresh-install warning and closing prompt. Preserve Rust's truthful manual/direct acquisition step until the approved orchestration can prove automatic missing-mod downloads (`src/ModSync.GUI/Dialogs/WizardPages/WelcomePage.axaml:14-298`, `src/ui.rs:654-703`, `docs/parity/download-orchestration-plan.md:1-8`).
2. Port Preamble hero/tip and render authored Markdown through Task 1. Port Aspyr heading/scroll/closing notice with raw content (`PreamblePage.axaml:9-23`, `PreamblePage.axaml.cs:42-54`, `AspyrNoticePage.axaml:9-17`, `AspyrNoticePage.axaml.cs:36-44`).
3. Port both directory page cards, picker text, current-path state, inline validation and game details. Enter commits/validates the typed path; Browse remains native. All validation is read-only (`ModDirectoryPage.axaml:10-107`, `GameDirectoryPage.axaml:10-115`, `DirectoryPickerControl.axaml:13-65`, `.axaml.cs:678-707`, `AGENTS.md:5-8`).
4. Port Download Process composition with a truthful selection summary and Download Center linkage. Do not assert background/automatic acquisition unless the shared download state proves it (`DownloadsExplainPage.axaml:9-40`, `src/ui.rs:724-728`).
5. Port Install Start header, ordered selection list, warning card and stale-validation message. Keep Rust's accurate snapshot and first-failure wording (`InstallStartPage.axaml:9-30`, `.axaml.cs:79-105`, `src/ui.rs:730-735`).
6. Port Widescreen Selection instruction/summary/scroll/list. Selection changes call the approved shared selection action; Remaining validation uses completed-prefix APIs and no base source is replayed (`WidescreenModSelectionPage.axaml:9-16`, `.axaml.cs:37-90`, `src/ui.rs:737-753`, `COMPATIBILITY.md:415-435`).

### Task 4: main-window shell and menu

1. Reproduce the top menu tree and item visibility from `MenuBuilderService`; URLs and labels remain pinned-oracle data. External link actions use the existing URL opener and never auto-open during menu construction (`src/ModSync.GUI/Services/MenuBuilderService.cs:658-1026`).
2. Reproduce spoiler/editor/wizard mode controls, title, themes and Home. Use native window decorations by default instead of duplicating icon-only minimize/maximize/close controls; record this as a platform-integrated difference and verify the same actions remain reachable (`src/ModSync.GUI/MainWindow.axaml:68-202`, `src/ui.rs:2900-2948`).
3. Add the main split/tab/landing/log composition and drag/drop/loading overlays. Drag/drop accepts only already-supported guide formats and passes the path into the same loader as Open File; it never auto-installs (`src/ModSync.GUI/MainWindow.axaml:204-489`, `src/model.rs:153-183`).
4. Menu operations that lack a Rust shared implementation render disabled with a reason and a parity issue reference. They do not silently no-op or claim support from preserved metadata (`AGENTS.md:7-8`, `docs/parity/ACCEPTANCE.md:59-60`).

### Task 5: legacy child surfaces

1. Build Landing/Getting Started as a guided legacy surface over the same load, directory, selection, download, validation, install and Activity state. The step indicators are projections; they do not create a second workflow state machine (`src/ModSync.GUI/Controls/LandingPageView.axaml:117-244`, `GettingStartedTab.axaml:47-568`, `src/wizard.rs:120-225`).
2. Build Mod Sidebar with search, counts, selection actions, filters, list and Global Actions. Route selection changes through the approved selection API; no direct `is_selected` writes in the renderer (`src/ModSync.GUI/Controls/ModListSidebar.axaml:14-259`, `docs/parity/mod-selection-plan.md:141-225`).
3. Build Raw tabs for TOML/Markdown/YAML/JSON only over formats the loader/exporter actually supports. Apply uses the same parse/replace/invalidation path as file loading; parse failure keeps the current guide unchanged (`src/ModSync.GUI/Controls/RawTab.axaml:12-97`, `src/model.rs:153-183`).
4. Build Structured Log with expand, count, auto-scroll, clear, copy, save, selected-copy and severity/timestamp/message rows. Raw external output remains display/log data and never becomes a business-state parser (`src/ModSync.GUI/Controls/EmbeddedLogPanel.axaml:11-150`, `src/ui.rs:3029-3067`, `docs/parity/install-progress-plan.md:118-141`).
5. Wire Editor tab container to the existing Rust editor without claiming nested F18 control parity. Any missing authoring behavior remains on F18 (`src/ModSync.GUI/Controls/EditorTab.axaml:42-607`, `src/ui.rs:2667-2840`, `docs/parity/ACCEPTANCE.md:53-54`).

### Task 6: Summary and spoiler variants

1. Port Mod Information fields/flags/category/tier cards with the shared spoiler-aware display helpers. Never mutate guide data to display a spoiler-safe variant (`src/ModSync.GUI/Controls/SummaryTab.axaml:42-191`, `src/ui.rs:1240-1306`).
2. Port Download Links normal repeater and spoiler warning expander. In spoiler mode, URLs remain collapsed until an explicit activation because host paths and filenames can contain spoilers (`src/ModSync.GUI/Controls/SummaryTab.axaml:193-311`, `src/ui.rs:1316-1331`).
3. Port Description, Directions, Download Instructions, Usage Warning, Screenshots, Known Bugs, Installation Warning, Compatibility Warning and Steam Notes. Use Task 1 Markdown rendering only for fields C# renders as Markdown and preserve raw text access (`src/ModSync.GUI/Controls/SummaryTab.axaml:314-470`, `.axaml.cs:95-189`).
4. Port read-only Options and Instructions repeaters, including selected state, spoiler-aware names/descriptions, action/source/destination/arguments and Choose targets. Editor jump is shown only when a target exists in the current editor model (`src/ModSync.GUI/Controls/SummaryTab.axaml:473-662`).

### Task 7: keyboard and accessibility closure

1. Write a source matrix for every interactive control: visible name, accessible name, role, state/value, help, enablement reason, initial focus, Tab order, Enter/Space action, Escape action and focus return. The audit tables are the starting inventory (`docs/parity/wizard-mainwindow-audit.md:25-239`).
2. Implement directory Enter, mod-list Ctrl+Up/Ctrl+Down/Delete/Space, log selected-copy, modal trap/return, menu keyboard navigation and wizard Back/Next/Finish activation where the current platform toolkit supports them (`DirectoryPickerControl.axaml.cs:678-707`, `MainWindow.axaml.cs:2110-2167`, `EmbeddedLogPanel.axaml.cs:226-251`).
3. Give icon-only/custom controls explicit names and progress/status nodes stable semantics. Announce validation/download/install terminal changes once; do not announce per-frame repaint text (`src/ModSync.GUI/Controls/WizardHostControl.axaml:22-98`, `src/ui.rs:2892-2898`).
4. Inspect, do not assume, AccessKit/AT-SPI/UIA/NSAccessibility output. A widget that looks correct but has a missing/duplicate name, wrong role/state, unreachable focus or repeated announcement fails this task (`Cargo.toml:37-37`, `docs/parity/GUI-INVENTORY.md:127`).

## Direct acceptance probes without Rust tests or harnesses

All probes use a fresh temporary root outside the repository and outside user game/mod trees. Record before/after manifests for fixture, game, mod workspace, settings, recovery state and any files opened/saved. Never launch external patchers during validation (`AGENTS.md:3-7`).

### Production CLI/static probes

1. Run the real CLI against one synthetic guide that contains base and widescreen components, all wizard metadata fields, spoiler variants, Markdown blocks, long names, categories/options and local Copy instructions. Record the parsed route inputs, selection output, validation report and ordered install candidates without modifying a user tree (`src/wizard.rs:105-169`, `src/main.rs:1-310`).
2. Byte-compare GUI-selected GUIDs, ordered component names, validation JSON, download candidates, install phase/prefix and completion summary against the corresponding real CLI/library output. A screenshot or matching count alone does not satisfy the boundary (`docs/parity/GUI-CLI.md:1-18`, `docs/parity/mod-selection-plan.md:260-309`).
3. Use `rg`/a source receipt to prove page renderers contain no direct engine, filesystem mutation, process spawn or selection-field assignment outside approved adapters. This is a static ownership gate, not runtime evidence (`AGENTS.md:2-8`, `docs/parity/mod-selection-plan.md:310-330`).
4. Parse every durable JSON/NDJSON output with an external JSON parser and bind it to the copied production executable hash. No bespoke Rust probe binary is permitted (`docs/parity/install-progress-plan.md:202-246`).

### Active wizard native matrix

1. Route variants: no optional metadata; Preamble only; K2/TSL Aspyr warning; widescreen without notice; widescreen with notice; no widescreen. Record exact page order, titles, subtitles, step counts and Back/Next/Continue/Finish gates (`src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:124-217`, `src/wizard.rs:129-225`).
2. Every page/control from the audit: click/type/select/scroll each interactive item, capture state changes, and verify all authored static clusters at 1280×800 and 600×400 (`docs/parity/wizard-mainwindow-audit.md:25-167`).
3. Preamble Markdown: headings, emphasis, lists, code, links, raw HTML, malformed input and long Unicode. Compare semantic structure to the pinned C# renderer; record intentional unsupported constructs instead of hiding them (`PreamblePage.axaml.cs:42-54`).
4. Directory pages: keyboard Enter, Browse, invalid/non-game/valid game paths, inline feedback, changing paths after validation, and no filesystem mutation from validation (`DirectoryPickerControl.axaml.cs:678-707`, `AGENTS.md:5-8`).
5. Downloads: navigation while a controlled local download is active, status bar/sidebar coherence, Stop confirmation and install gate. Do not use live hosts without separate authorization (`WizardHostControl.axaml.cs:219-362`, `docs/parity/download-orchestration-plan.md:5-16`).
6. Base/Remaining install: reuse the approved install-progress and completion-page probe matrices rather than inventing replacements. Bind this shell's screenshots/input/accessibility tree to their receipts (`docs/parity/install-progress-plan.md:202-260`, `docs/parity/completion-pages-plan.md:120-150`).

### Legacy main-window native matrix

1. Enter/leave Landing, Wizard and Editor modes; Home; theme switch; top-level tabs; log resize; drag/drop valid/invalid files; loading overlay; close while busy (`src/ModSync.GUI/MainWindow.axaml:50-489`).
2. Activate every menu item. For implemented actions, verify the real effect or opened URL; for unsupported actions, verify disabled state and explanation. No enabled no-op is acceptable (`src/ModSync.GUI/Services/MenuBuilderService.cs:658-1026`).
3. Drive Getting Started, sidebar, Raw and log controls control-by-control. Compare resulting shared state and disk manifests to the equivalent wizard/CLI action (`docs/parity/wizard-mainwindow-audit.md:188-208`).
4. Summary normal/spoiler runs: inspect every field/card, Markdown rendering, options/instructions, URL warning expander and copy/open behavior. Search captured screenshots/text for raw spoiler-only values that should be hidden (`SummaryTab.axaml:42-662`).

### Keyboard/accessibility/platform matrix

1. On Linux, drive Tab/Shift+Tab, arrows, Enter, Space, Escape and declared shortcuts through the real native window; query the AT-SPI tree with an external inspector. Capture role/name/help/state/value/focus for every interactive control and progress/status surface (`docs/parity/wizard-mainwindow-audit.md:219-229`).
2. Repeat native accessibility inspection on supported Windows and macOS using their platform accessibility APIs before claiming cross-platform parity. If a platform cannot be run, record that cell as unverified; Linux proof does not generalize (`docs/parity/ACCEPTANCE.md:59-64`, `docs/parity/GUI-INVENTORY.md:168-174`).
3. Verify modal initial focus, trap, Escape/cancel, destructive-action confirmation and focus return for category, install, stop, delete and any menu-spawned dialogs (`src/ui.rs:3107-3166`, `docs/parity/mod-selection-plan.md:310-330`).
4. Verify status announcements are neither absent nor repeated on each repaint. Record an accessibility-event trace, not only a tree snapshot (`src/ui.rs:2892-2898`, `src/ui/validation_page.rs:287-335`).

### Visual matrix

Capture pinned C# and Rust at the same geometry, theme, scale and font configuration for every route and legacy surface: 1280×800, 600×400 and one high-DPI scale; Light, K1 and K2; empty, populated, invalid, busy, terminal-success and terminal-failure states where applicable (`src/ModSync.GUI/MainWindow.axaml:15-18`, `src/ModSync.GUI/Styles/LightStyle.axaml:1-40`, `src/ModSync.GUI/Styles/KotorStyle.axaml:1-40`, `src/ModSync.GUI/Styles/Kotor2Style.axaml:1-40`).

Image similarity is supporting evidence only. A reviewer checks clipped/unreachable content, focus indication, contrast, wrapping, scroll ownership, disabled/error/success states, tooltip reachability and exact control presence against the inventory (`docs/parity/GUI-INVENTORY.md:128-129`, `docs/parity/GUI-INVENTORY.md:168-174`).

## Receipt contract

Every evidence package records:

- Rust commit SHA, pinned C# SHA, branch, dirty-state manifest, and explicit preservation of the user-owned `AGENTS.md` (`docs/parity/wizard-mainwindow-audit.md:1-8`).
- Source blob hashes for every audited/changed file and SHA-256 of each copied production executable used by a probe (`docs/parity/install-progress-plan.md:202-246`).
- Full argv/environment relevant to UI/backend selection, exit codes, stdout/stderr hashes, display server, geometry, theme, scale, fonts and owned process IDs (`docs/parity/wizard-reference/README.md:1-16`).
- Fixture guide/settings and before/after game/mod/recovery manifests; no user tree paths or credentials (`AGENTS.md:3-8`).
- Ordered interaction transcript, screenshot hashes, clipboard readback hashes, URL/open-folder outcomes, keyboard event trace and accessibility-tree/event snapshots (`docs/parity/GUI-INVENTORY.md:168-174`).
- A per-cell verdict of pass, deliberate documented difference, failed, blocked or unverified. “Source present”, “build passed”, “screenshot captured” and “tree node exists” are not promoted to a stronger verdict (`docs/parity/ACCEPTANCE.md:59-64`).
- An explicit statement that no Rust tests or harnesses were created, modified, hidden, compiled or run (user test freeze; `docs/parity/install-progress-plan.md:5-16`).

## Review gates

### Task 1 review

- Dependency is desktop-only, license/MSRV/API reviewed, raw HTML cannot execute, no network fetch occurs, links require activation, malformed content remains visible, and raw copy is byte-equal (`Cargo.toml:9-38`, `PreamblePage.axaml.cs:42-54`).
- Renderer owns no guide parsing/business state and exports no type through `src/lib.rs` (`src/lib.rs:1-35`).

### Tasks 2-3 review

- Exact active host, route and control inventory are satisfied or each difference is named with a reason (`docs/parity/wizard-mainwindow-audit.md:25-167`).
- No duplicate route enum/gates; no regression to approved selection/validation/install/completion behavior; no local parsing of logs/errors into state (`src/wizard.rs:5-225`, `src/ui.rs:386-445`).
- Small-window navigation/recovery is reachable; nested scrolls do not strand controls; stale results cannot advance (`docs/parity/GUI-INVENTORY.md:29-37`, `src/ui.rs:620-639`).

### Tasks 4-6 review

- Every MainWindow control/menu/child surface in the audit has an implementation, a disabled-with-reason placeholder backed by a ledger item, or an explicit F18 dependency; nothing enabled silently no-ops (`docs/parity/wizard-mainwindow-audit.md:168-239`, `docs/parity/ACCEPTANCE.md:53-60`).
- GUI action adapters call shared operations; no alternate install/validation/download/recovery semantics appear in presentation modules (`AGENTS.md:2-8`, `docs/parity/GUI-CLI.md:1-18`).
- Spoiler mode hides every covered authored variant and gates URLs; original guide data remains unchanged (`SummaryTab.axaml:57-75`, `SummaryTab.axaml:214-418`, `src/ui.rs:1240-1331`).
- Drag/drop and Raw Apply are fail-closed and preserve the prior loaded project on parse failure (`src/model.rs:153-183`).

### Task 7 review

- The control matrix has no unnamed icon/custom control, missing state/value, unexplained disabled action or unreachable keyboard path (`docs/parity/wizard-mainwindow-audit.md:219-229`).
- Modal focus/trap/return, status announcements and declared C# shortcuts are verified from native evidence, not inferred from widget type (`MainWindow.axaml.cs:2110-2167`, `DirectoryPickerControl.axaml.cs:678-707`, `EmbeddedLogPanel.axaml.cs:226-251`).
- Platform gaps remain unverified rather than being generalized from Linux (`docs/parity/ACCEPTANCE.md:59-64`).

### Task 8 final review

- One specification reviewer checks the whole F24/F25 diff against the audit and pinned oracle. One correctness/race reviewer checks route/input/worker/modal lifecycles. One accessibility reviewer checks the native tree/event/keyboard packages. One scope reviewer verifies F18/F29 and unsupported menu actions were not silently claimed (`docs/parity/ACCEPTANCE.md:53-64`, `docs/parity/GUI-INVENTORY.md:159-174`).
- Findings return to the owning task with an exact referent and consequence. Each correction receives a scoped re-review. A fresh reviewer then reads the combined diff and durable evidence before ledger status changes (`docs/parity/install-progress-plan.md:248-260`, `docs/parity/completion-pages-plan.md:141-150`, `docs/parity/mod-selection-plan.md:310-340`).

## Frozen-test and build risk

- Existing inline UI tests refer to private `App` state and message variants. Moving state into page modules can make them fail to compile; workers record exact drift but do not edit or hide the tests. Production shims are added only when a real production caller needs them (`docs/parity/GUI-INVENTORY.md:33-37`, `docs/parity/install-progress-plan.md:248-258`).
- Manifest changes for rich text affect dependency resolution and both feature configurations. Implementation must run the repository's four required gates only after the user lifts the Rust test freeze; until then, production checks omit tests and report that limitation explicitly (`AGENTS.md:3-7`, `Cargo.toml:9-38`).
- Builds use `host-spawn`, a unique target directory and `-j 2`; no task deletes another lane's cache or writes into a shared target directory (user build constraint).

## Completion boundary

F24 is complete only when every active-route control in the audit is implemented, integrated with approved shared behavior, and directly verified across every conditional route, keyboard/accessibility state, theme, minimum/default/high-DPI layout and supported platform; the prior install-progress/completion/mod-selection/validation evidence remains valid (`docs/parity/wizard-mainwindow-audit.md:25-167`, `docs/parity/ACCEPTANCE.md:59-60`).

F25 is complete only when every main-window/menu/legacy-child/rich/spoiler/keyboard/accessibility row has an implemented or correctly F18-owned outcome and direct native acceptance; a visible menu, tab, source file or screenshot alone does not close it (`docs/parity/wizard-mainwindow-audit.md:168-239`, `docs/parity/ACCEPTANCE.md:53-60`).

Neither domain closes F29. Real K1/K2 full-build installation and live-game verification remain separate acceptance work (`docs/parity/ACCEPTANCE.md:64-64`).

## Open questions with defaults

| # | Question | Default | Referents |
|---|---|---|---|
| Q1 | Should the Rust shell copy the C# active host exactly when a Rust safety control has no C# counterpart? | **Keep the Rust safety control and record the difference.** Stop/restore/retry/confirmation truth outranks cosmetic identity. | `docs/parity/GUI-INVENTORY.md:1-37`, `src/ui.rs:530-639` |
| Q2 | Should `LoadInstructionPage` become an active wizard page? | **No.** Keep loading in Landing/Main Window because the pinned active host explicitly removed it. | `WizardHostControl.axaml.cs:124-132`, `LoadInstructionPage.axaml:4-12` |
| Q3 | Should Rust draw custom minimize/maximize/close buttons? | **No.** Use native decorations and give platform behavior priority; record the composition difference. | `MainWindow.axaml:155-183`, `src/ui.rs:3171-3202` |
| Q4 | What happens when a pinned menu action has no shared Rust implementation? | **Show it disabled with a concise reason and ledger reference.** Never enable a no-op or add UI-only business logic. | `MenuBuilderService.cs:697-792`, `AGENTS.md:7-8` |
| Q5 | Which Markdown strategy is used? | **A reviewed desktop-only CommonMark event parser plus `src/ui/rich_content.rs`; raw HTML escaped, remote images not fetched, raw copy retained.** | `PreamblePage.axaml.cs:42-54`, `SummaryTab.axaml.cs:95-189`, `Cargo.toml:9-38` |
| Q6 | Should spoiler mode expose download URLs immediately? | **No.** Match the C# warning expander because URLs/filenames may contain spoilers. | `SummaryTab.axaml:214-306`, `src/ui.rs:1316-1331` |
| Q7 | Does wiring an Editor tab close F18? | **No.** F25 owns the main-window container and integration; nested authoring parity stays F18. | `docs/parity/ACCEPTANCE.md:53-60`, `EditorTab.axaml:42-607` |
| Q8 | Can Linux keyboard/AT-SPI evidence close all accessibility acceptance? | **No.** Keep Windows/macOS cells unverified until driven on those platforms. | `docs/parity/ACCEPTANCE.md:59-64`, `docs/parity/GUI-INVENTORY.md:168-174` |
| Q9 | Should exact C# clipping be preserved at 600×400? | **No when it makes content or safety controls unreachable.** Preserve semantic control inventory and record responsive wrapping/scrolling differences. | `docs/parity/GUI-INVENTORY.md:29-37`, `docs/parity/completion-pages-plan.md:106-108` |
| Q10 | May this plan change `src/engine.rs` to make a UI control work? | **No.** Stop and queue one additive shared-API task after the serialized engine owner; resume only after CLI and GUI can consume the same typed contract. | `AGENTS.md:2-8`, `docs/parity/GUI-CLI.md:1-18` |

## Claim blocks

claim: This plan can implement the remaining F24/F25 presentation without changing engine install semantics.
referent: `src/wizard.rs:5-225`, `src/ui.rs:386-769`, `src/ui.rs:2900-3166`, approved page plans cited in “Preserve approved page owners”.
checked: direct source and ownership-plan audit; no build or runtime probe.
verdict: holds
confidence: medium-high; discovery of a user-visible fact unavailable from any shared API would trigger the explicit library stop rule.

claim: `src/ui.rs` edits from this plan must wait until install-progress, completion-pages and mod-selection have finished in that order.
referent: `docs/parity/install-progress-plan.md:189-200`, `docs/parity/completion-pages-plan.md:109-118`, `docs/parity/mod-selection-plan.md:247-258`.
checked: existing plan ownership tables and the user's serialized-lane constraint.
verdict: holds
confidence: high; only an explicit owner handoff can change the sequence.

claim: Direct CLI/native probes can verify this slice without creating or running Rust tests or harnesses.
referent: `docs/parity/install-progress-plan.md:202-246`, `docs/parity/completion-pages-plan.md:120-139`, `docs/parity/mod-selection-plan.md:260-309`, and the probe matrices above.
checked: proof-surface design against current CLI/native entry points; probes not run in this planning turn.
verdict: holds
confidence: medium-high; a missing production observability surface must become an additive CLI/library API, not a test harness.

claim: Accessibility closure requires platform-native keyboard, tree and event evidence, not source or screenshots alone.
referent: `docs/parity/GUI-INVENTORY.md:127`, `docs/parity/GUI-INVENTORY.md:168-174`, `docs/parity/wizard-mainwindow-audit.md:219-229`.
checked: source declaration audit and existing acceptance boundaries.
verdict: holds
confidence: high; only completed native receipts can change the acceptance status.

claim: The legacy main-window plan does not duplicate F18 editor implementation or close F29 full-build parity.
referent: `docs/parity/ACCEPTANCE.md:53-64`, Task 5 and Completion Boundary above.
checked: domain/ownership trace.
verdict: holds
confidence: high; an explicit scope change in the acceptance ledger would be required to merge those domains.
