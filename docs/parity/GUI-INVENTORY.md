# C# GUI parity inventory

## Final archive diagnostic correction — 2026-09-27

Whole-package review found one P2 combined-path defect: known component archive errors lacked a severity marker, so the shared presentation displayed only an aggregate card and first-issue navigation could select the final generic failure. Scoped specification/quality review approves `099834c` and closes F1. That commit fixes the shared failed-outcome mapping with an idempotent `ERROR:` prefix, preserving component identity/detail. **Raw Report message values now include that prefix; serialized schema, stages, counters and warning behavior are unchanged.** This qualifies earlier transport-compatibility statements: schema stability does not imply unchanged diagnostic text.

Separate [focused archive-fix evidence](validation-archive-diagnostic-fix/README.md) records missing Extract **1/1/4**, corrupt Extract **3/0/3**, unchanged missing Copy **0/1/5**, native actionable cards, automatic/manual first-issue targeting and actual clipboard readback. Strict installs reject before snapshots/pending/game/mod content writes; expected lock files and empty state directories outside content roots are recorded, not hidden. The earlier47c3e67 native package retains its original binding. No Rust tests were created/adapted/run, and no full-parity or all-target compilation claim follows.

## Validation page integration checkpoint — 2026-09-27

Shared projection/progress is approved through9d77f4b and shared source-availability/FOMOD execution-scope corrections through88b2dd9. The dedicated native page is implemented in `src/ui/validation_page.rs`, routed by `src/ui.rs`, through47c3e67. It has page-local log/results, stage progress, splitter, report copy, first-issue navigation and run/input-bound lifecycle handling. Utilities remain separately reachable. This supersedes the old generic-validation-surface gap for source implementation, not native equivalence.

The latest600×400 run/Stop confirmation/cancelled Next-blocked case is parent/reviewer confirmed at47c3e67. The completed bounded core native matrix and durable package are in [validation-native/README.md](validation-native/README.md); the subsequent whole-package review finding is closed by the approved099834c correction above. Earlier556b165 and5548736 captures retain those bindings. Missing Copy is now the shared warning0/1/5, not a hard error. C# clipboard readback, exact typography/metrics, broad keyboard/accessibility and complete visual equivalence remain open. See [integration contracts, rulings and evidence boundaries](validation-page-integration.md); whole-GUI status remains open.

Status: open. This is a source audit against C# commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`. It does not establish rendered, native-widget, keyboard, screen-reader, or end-to-end behavioral equivalence. Those require driven native comparisons after implementation.

## Desktop route integration status

The active C# entry point is `MainWindow.EnterWizardMode` →
`Controls/WizardHostControl`, not the older `InstallWizardDialog`. Loading belongs
to a separate landing surface; the embedded wizard starts at Welcome. Rust now
consumes `src/wizard.rs` and renders distinct pages, conditional preamble/Aspyr
pages, dynamically inserted widescreen pages, titles/subtitles, Step N of M,
Back/Next/Continue/Finish, and a 200×6 progress indicator. The shell requests
1280×800 with a 600×400 minimum. There is no step-navigation sidebar. A 320px
download sidebar and footer status appear at Download Process and persist through
completion. Finish retains the loaded guide and completion context; Start wizard
creates a fresh route. Settings and editor remain utility surfaces.

Forward gates derive from live UI/worker state. Base validation uses the shared
full/dry-run pipeline on every selected component, including widescreen; base
installation excludes widescreen. Remaining validation/install receive the verified
completed prefix through shared APIs. Consumed base Move sources are not replayed.
Failed installs do not enable Continue. Inputs changed during validation discard
the result; changed inputs invalidate readiness before navigation. External FOMOD
choices use metadata stamps, including ctime/inode on Unix, without payload reads
each repaint. On non-Unix systems, same-size edits preserving modification/creation
timestamps can escape this stamp: this is not a content-hash guarantee.

Intentional safety differences: Rust refreshes Continue after worker completion
(the active C# host can leave it disabled), retains Activity Stop and close-request
confirmation, requires explicit install confirmation, and blocks retry when recovery
has a pending snapshot. Download workers allow forward reading navigation but
serialize other operations and disable Back/input editing. C# has no active host
wizard Cancel/Stop. Welcome follows the feature row, Problem/Solution columns,
five-step process, key features, fresh-install warning and closing prompt hierarchy.
Its download copy explicitly states manual/direct acquisition until the automatic
missing-mod queue exists, rather than promising unsupported background acquisition.

This closes the four-tab route gap, not exact GUI parity. Remaining work includes
provider download planning/Fetch Downloads, aggregate progress, rich authored markup,
exact cards/control composition, theme tokens, accessibility and supported-platform
validation. See `wizard-native/README.md` for bounded native evidence. Metadata-free
C# JSON default preamble versus TOML metadata reset remains a shared-loader gap.

## Compared sources

- Rust desktop: `src/ui.rs` (`App`, especially `eframe::App::update`) and shared operations in the `modsync` library.
- C# active shell/routing: `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.GUI/Controls/WizardHostControl.axaml` and `.axaml.cs`; `Dialogs/InstallWizardDialog` is the older host.
- C# pages: `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.GUI/Dialogs/WizardPages/*`.
- C# legacy onboarding: `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.GUI/Controls/GettingStartedTab.axaml` and `.axaml.cs`; wizard landing content is `Controls/LandingPageView.axaml` hosted by `LoadInstructionPage`.
- C# validation presentation: `WizardPages/ValidatePage.axaml` and `.axaml.cs`, `Services/WizardValidationStagePresenter.cs`, and `Services/ValidationPipelineDialogMapper.cs`.
- C# styles: `App.axaml`, `Styles/LightStyle.axaml`, `Styles/KotorStyle.axaml`, and `Styles/Kotor2Style.axaml`.
- Behavior/runbook cross-check: `AGENTS.md`, `docs/local_desktop_agent_runbook.md`, and `docs/knowledgebase/gui-validation-surfaces.md` in the C# checkout; `docs/parity/GUI-CLI.md` in this checkout.

## Original gap inventory (before route integration)

This historical audit describes the former four-tab UI and older dialog oracle.
The integration status above supersedes its route, host, geometry and cancellation
claims. Remaining control/content/theme/accessibility differences remain open until
a driven native comparison covers them. Priority means implementation order, not proof.

| Priority / group | C# contract and source pointer | Rust source today | Exact current difference | Implementation acceptance |
|---|---|---|---|---|
| P0 — wizard shell and route state | `InstallWizardDialog.InitializePages`, `NavigateToPageInternalAsync`, `UpdatePageStates`; shell in `InstallWizardDialog.axaml` | `App::update`: four selectable buttons (`Load guide`, `Choose mods`, `Validate and install`, `Edit guide`) and `page: usize` | Rust has four freely addressable tabs, no per-page objects, sidebar step list, completed/current/accessibility state, per-page title/subtitle, `Step N of M`, wizard progress, Back/Next/Finish semantics, or page validation/deactivation hooks. | Introduce the exact ordered route model below. Gate forward navigation through page validation; permit only current/completed sidebar entries; preserve Back, Cancel, Finish, install-page Continue/Stop Install labels and states. Keep route state/presentation in UI, while actions call shared library operations. |
| P0 — base sequence | `InstallWizardDialog.InitializePages` | All load/directories share Rust page 0; selection is page 1; validation/install share page 2 | Missing distinct pages and their copy/layout: `LoadInstructionPage`, `WelcomePage`, optional `PreamblePage`, `ModDirectoryPage`, `GameDirectoryPage`, optional `AspyrNoticePage`, `ModSelectionPage`, `DownloadsExplainPage`, `ValidatePage`, `InstallStartPage`, `InstallingPage`, `BaseInstallCompletePage`, `FinishedPage`. | Render this exact sequence and page-specific controls/copy. Do not collapse steps into tabs or a global pane. Confirm title/subtitle and navigation availability against each `WizardPageBase` implementation. |
| P0 — conditional routes | `InitializePages` and `AddWidescreenPages` | `widescreen_notice`, `installed_phase`, `completed_base`, `widescreen_notice_seen`, and page-2 inline sections | Preamble and Aspyr warning are not wizard pages. C# Aspyr page appears only for KOTOR2/TSL plus nonblank content. C# dynamically inserts optional `WidescreenNoticePage`, then `WidescreenModSelectionPage`, `WidescreenInstallingPage`, and `WidescreenCompletePage` before Finished after base completion. Rust shows phase content inline and does not reproduce dynamic sidebar/page numbering. | Use metadata and target-game predicates exactly. Insert/re-number widescreen pages only after base completion, omit notice when content is blank, and omit the full widescreen branch when there are no widescreen-only components. Preserve base selection separately from widescreen selection. |
| P0 — shared GUI/CLI business logic boundary | `GUI-CLI.md`; C# calls core pipeline/install services from page handlers | Rust GUI already calls library functions for validation, install, remaining-phase install, selection helpers, downloads, recovery; CLI also calls library code | Shared calls are partial evidence only. UI still owns orchestration and state preparation; parity of selections, validation stage data, cancellation/progress events, and remaining-phase inputs has not been proven for both frontends. GUI-specific cards/copy belong in presentation, but stage rules must not be reimplemented there. | For each button, document the one shared library entry point also used by CLI, with identical prepared inputs and typed results/events. Keep filtering/selection predicates, validation stages, install/remaining install, download planning, conflict analysis, and recovery in library modules. Add cross-frontend contract tests at those boundaries; do not make the CLI instantiate UI state. |
| P1 — load/onboarding | `LoadInstructionPage` + `LandingPageView`; legacy `GettingStartedTab` control map | Rust page 0 combines guide load, protocol/NXM prompts, drafting, game/mod directories, profile/key/telemetry settings | The C# wizard first shows a landing surface with load/create states and blocks Next until components exist, then a rich Welcome page, then separate directory pages with explanation cards, browse controls, inline validation feedback, and detected-game details. Legacy onboarding separately exposes `Step1ModDirectoryPicker`, `Step1KotorDirectoryPicker`, `Step2Button`, Fetch/Open/Download status/Stop/Validate actions. Rust neither matches the wizard landing/welcome/directory composition nor the required legacy onboarding surface. | Reproduce load/create loaded-state feedback, full Welcome content, separate workspace/game pages and their validation. Implement the separate legacy Getting Started surface and its named controls/actions as part of full GUI parity; do not treat combined Rust page 0 as equivalent. |
| P1 — selection page controls | `ModSelectionPage.axaml` and `.axaml.cs` | Rust page 1 already has Select all/Deselect all, search, tier/category combo filters and selection actions, spoiler-free toggle, Clear filters, selection/filter summaries, required/optional badges, multi-category popup, expand/collapse, empty state, scrollable component groups and option details | The feature set partially overlaps, but exact controls and composition differ. C# uses `✓ Select All`, `✗ Deselect All`, modal `⭐ Select by Tier`, modal `🏷️ Select by Category`, its named search/combo/toggle/expand controls, `SELECTED`/`REQUIRED`/`OPTIONAL` badges, and a single card hierarchy. Rust uses inline `Select through tier`/`Select category`, reports required/optional without the C# selected badge layout, and uses a two-column list/details design with different labels and visual states. | Reproduce named controls, exact text, combo values, modal selection behavior, all three badge counts, filter summary, empty state, group/card expansion, option descriptions/tooltips, and Clear Filters while retaining existing working behavior through the matched presentation. Reuse shared Rust matching/dependency helpers; do not duplicate predicates in `ui.rs`. |
| P1 — selection interactions | `ModSelectionPage.ApplyFilters`, select handlers, dependency resolution, dynamically built component/option cards | Rust invokes `selection::{select_visible,deselect_base,resolve_selected}` and renders nested options | Source suggests overlapping concepts, but does not prove C# additive/reset rules, required/disabled states, dependency side effects, game compatibility, option click-card behavior, tooltip/spoiler substitutions, or scroll preservation. | Drive identical fixtures through both UIs. Assert selections and visible rows after search/category/tier combinations, select-all, deselect-all, select-by dialogs, spoiler-free toggle, card clicks, options, dependencies, and incompatible games. |
| P1 — downloads explanation and background state | `DownloadsExplainPage`; wizard/main-window download services | Rust download actions and activity log exist outside a dedicated wizard page | Missing selection summary and exact “downloads continue while you proceed” page. Global busy state disables the entire Rust central UI, which conflicts with C#’s stated background-download progression. Download status/stop are not located or presented like C#. | Add the dedicated page and typed download progress/status model. Allow navigation while background downloads continue where C# does; block install at the same boundary. Use shared downloader/cancellation logic for GUI and CLI. |
| P1 — validation layout | `ValidatePage.axaml` | Rust page 2 has Run validation, a collapsible inline report with error/warning/passed counts, stage summaries/messages, Copy validation report, and Go to first issue; activity logs remain in a global bottom panel | Rust has partial result controls, but not the C# page composition: header/copy, page-local progress/status, expandable validation log, resizable splitter, independent results scroller, summary panel, or styled badges. Rust’s separate global Copy report copies activity lines, while Copy validation report serializes the typed report as JSON; neither matches the C# human-readable page report. | Match the page grid and named controls: `ValidateButton`, `ValidationProgress`, `StatusText`, `LogExpander`, `LogScrollViewer`, `LogText`, `LogGridSplitter`, `ResultsScrollViewer`, `ResultsPanel`, `SummaryText`, `SummaryDetails`, and three badges. Preserve useful Rust JSON export only as an additional explicitly labeled action if desired; it does not satisfy C# Copy report parity. |
| P1 — validation cards, log, copy, focus | `ValidatePage.ApplyPipelineResultToWizardUi`; `WizardValidationStagePresenter.ApplyStages`; `gui-validation-surfaces.md` | Rust renders colored stage summaries/messages, counts, JSON copy, and a Go to first issue action that scrolls the matching stage message and appends it to the activity log | Rust’s existing first-issue action is narrower than C#: it does not expand/page-focus a validation log, choose and highlight the first error/warning result card, or coordinate independent log/results scrolling. Rust also lacks the C# card mapping for Environment, Conflicts, Install Order, Archive/Component Validation and capped dry-run issues; aggregate cards; solution hints; automatic warning/error log expansion; queue flush/reset semantics; and the human-readable report sections/full log. | Feed typed shared validation results to a presentation mapper matching C# titles, severities, counts, ordering, detail text, and solution hints. C#-parity `Copy report` must include summary, `--- Results ---`, every displayed card, `--- Log ---`, and complete flushed log. A new run resets both scroll positions; warned/failed runs expand and focus/scroll/highlight the first issue in both representations; clean runs leave log collapsed. |
| P1 — install review and confirmation | `InstallStartPage.axaml`/`.cs` | Rust page 2 uses `confirm_install` and an inline confirmation block/modal-like state | C# has a dedicated review page with selected count/list and Important warnings; Next begins installation after page validation. Rust does not match its page, copy, scroll, or navigation contract. | Add the dedicated page with exact selected list/count and warnings. Prepare the install plan via shared logic and start it once on forward navigation. |
| P1 — installation progress | `InstallingPage.axaml`/`.cs` | Rust uses a spinner, global logs, busy disabling, and terminal `Done` | Missing overall percent/count, current mod/directions/current operation, per-mod progress, checkpoint status, elapsed/remaining/rate, warning/error totals, checkpoint count, explanatory panel, and page-specific cancellation/close behavior. Rust does not expose enough typed progress to reproduce these values. | Define shared typed progress events from the installer and consume them in both frontends (CLI can format them). Match the page fields and update cadence. During install disable Back, label Next `Continue`, label Cancel `Stop Install`, confirm stop, finish current mod as C# specifies, and intercept window close. |
| P1 — base/final pages | `BaseInstallCompletePage`, `WidescreenCompletePage`, `FinishedPage` | Rust page 2 renders terse completion labels and an inline phase summary | Missing base statistics (mods/checkpoints/time/files), “What’s Next?”, notes, managed-deployment summary, quick actions (logs/checkpoints/game folder/mod folder), widescreen completion presentation, final next steps/support card, and Finish closure semantics. | Reproduce each distinct page, copy, statistics and quick actions. Populate values from shared completion receipts rather than recomputing filesystem/business facts in the UI. |
| P2 — navigation/cancel/window behavior | `InstallWizardDialog.axaml(.cs)` | Standard egui viewport; close is intercepted only while busy | Missing 900×650 centered-owner shell, 800×600 minimum, 240px step sidebar, custom minimize/close controls, cancel confirmation when idle, install-specific stop confirmation, and Finish dialog result. | Match dimensions and navigation chrome first; then verify minimize, X, Escape/window-close, Cancel, Stop Install, Back, Next, Continue and Finish in every state. Avoid relying on egui defaults where they differ from Avalonia behavior. |
| P2 — keyboard and accessibility | Avalonia native controls, `DirectoryPickerControl.OnPathInputKeyDown`, focusable buttons/list; inspect page XAML/code-behind | egui immediate-mode controls; no explicit accessibility names, landmarks, focus order, accelerators, or validation announcements in `ui.rs` | Neither source inspection nor egui defaults establish parity. Rust lacks an explicit focus plan for new pages, issue navigation, modal focus trapping/restoration, keyboard activation, and screen-reader names/status announcements. C# itself has few explicit `AutomationProperties`, so observed native behavior is the oracle. | Build a keyboard matrix: logical Tab/Shift+Tab order, Enter/Space activation, Escape/cancel behavior, sidebar access constraints, modal trap/return, browse-field Enter, and focus after validation. Give every icon-only/custom control an accessible name; expose titles, progress and validation status through the platform accessibility tree. Verify with native accessibility inspection on each supported OS, not screenshots alone. |
| P2 — layout and scrolling | Wizard shell plus every page `.axaml`; headless smoke checks in `GuiSmokeHeadlessTests` | egui panels and automatic layouts | Rust lacks the fixed shell proportions, per-page grids/cards, nested scroll boundaries, validation splitter, list minimum sizes, 600px content caps, typography hierarchy, wrapping/trimming rules, and install/wizard page-specific scrolling. | Port dimensions/spacing from XAML as explicit acceptance values where material. Test 800×600 minimum, 900×650 default, high DPI, long translated/content strings, long mod names, empty/large lists, and splitter resizing. Compare native screenshots at controlled size/theme/font scale. |
| P2 — themes and visual states | `ThemeManager`, Light/KOTOR/KOTOR2 style files and page-local styles | Rust theme combo stores C# style-path strings and maps only broad egui visuals | The names exist, but exact brushes, fonts, border thickness, padding, minimum control sizes, corner radii, hover/pressed/disabled/focus/current/completed/error/warning/success states are not ported. Selecting a similarly named egui theme is not parity. | Create a token inventory directly from the three XAML styles, then map every wizard state. Capture reference and Rust native screenshots for all three themes and key states; use image comparison plus manual review. Keep platform focus indication and contrast intact. |

## Active route

Base route:

1. Welcome (after successful landing load)
3. Before You Begin — only when `preambleContent` is nonblank
4. Mod Workspace Directory
5. KOTOR Game Directory
6. Aspyr Version Notice — only for target `KOTOR2` or `TSL` and nonblank `aspyrExclusiveWarningContent`
7. Mod Selection
8. Download Process
9. Validation
10. Ready to Install
11. Installing Mods
12. Base Installation Complete
13. Installation Complete

After leaving Base Installation Complete, if any component is `WidescreenOnly`, insert immediately before Installation Complete:

1. Widescreen Support — only when `widescreenWarningContent` is nonblank
2. Widescreen Mod Selection
3. Installing Widescreen Mods
4. Widescreen Installation Complete

Page totals update after insertion. The active host has no step-list navigation;
only Back, Next/Continue and Finish change the route. The older dialog sidebar's
`IsCompleted`/`IsAccessible` rules do not describe the active host.

## Implementation order and proof gates

1. **Route and shell:** exact pages, predicates, navigation gating, cancellation, dimensions. Gate: deterministic route-model tests for K1, K2/TSL, preamble/no-preamble, widescreen/no-widescreen and blank/nonblank warning content.
2. **Typed shared operation contracts:** selection, validation, download, install progress, remaining-phase install, cancellation, completion receipt. Gate: GUI and CLI adapters consume the same library result/event fixtures without independent business predicates.
3. **Selection and onboarding pages:** exact controls, labels, filtering/count states and dependencies. Gate: driven native interactions plus state receipts.
4. **Validation page:** exact cards, counts, log, splitter, copy, scrolling/highlight/focus. Gate: clean/warning/error fixtures and clipboard text comparison against the C# contract.
5. **Install/review/progress/completion pages:** exact fields, cancellation and quick actions. Gate: synthetic typed progress stream, then a disposable end-to-end install in guide order.
6. **Accessibility, layout and themes:** keyboard matrix, accessibility-tree inspection, three-theme screenshots at default/minimum/high-DPI sizes. Gate: native evidence on supported platforms.

## Evidence required before claiming exact parity

- A control-and-copy checklist covering every row above, including conditional absence as well as presence.
- Driven native recordings or screenshots at controlled window size for each page and each theme; source presence and egui state tests are insufficient.
- Keyboard traversal and modal focus receipts, plus accessibility-tree labels/roles/states and progress/status announcements.
- C# versus Rust clipboard report comparison for clean, warning and error validation fixtures.
- GUI/CLI receipts showing identical prepared selection, validation report, install plan, ordered phase boundaries, cancellation outcome and completion receipt from shared library APIs.
- A real disposable install verifying progress/completion UI against resulting files. This is separate from full C# behavioral parity and must not be inferred from a build or unit tests.
