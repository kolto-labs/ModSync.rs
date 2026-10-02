# F24/F25 wizard and legacy main-window source audit

Pinned C# oracle: `d31ccbd3bfcddb4705917eafa6483715c166de3b`. Rust source audited at `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`. This is a read-only source comparison; it is not native rendering, keyboard, accessibility-tree, build, install, full-guide, or live-game proof (`docs/parity/ACCEPTANCE.md:59-60`, `docs/parity/GUI-INVENTORY.md:168-174`).

## Scope and inventory rules

- The active install route is `MainWindow.EnterWizardMode` into `Controls/WizardHostControl`; `LoadInstructionPage` is explicitly removed from that route and loading remains in `LandingPageView` (`src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:124-184`, `src/ModSync.GUI/MainWindow.axaml:204-214`, `docs/parity/GUI-INVENTORY.md:55-60`).
- The active base route is Welcome, optional Preamble, Mod Directory, Game Directory, optional Aspyr Notice, Mod Selection, Downloads, Validation, Install Start, Installing, Base Complete, then Finished; widescreen Notice when authored, Selection, Installing, and Complete are inserted before Finished only when widescreen components exist (`src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:124-217`).
- “Control” below means every interactive control, named status/output control, repeated item host, page scroll boundary, and authored static-content cluster. Decorative `Grid`, `StackPanel`, and `Border` nodes are grouped with the content they lay out; this keeps every user-facing or state-bearing XAML element represented without pretending layout primitives have independent bindings (`src/ModSync.GUI/Dialogs/WizardPages/*.axaml`, `src/ModSync.GUI/MainWindow.axaml:50-489`).
- “Declared a11y name” records only an explicit source declaration. The audited wizard/main-window XAML contains no `AutomationProperties.Name`, `AutomationProperties.HelpText`, `AccessKey`, `HotKey`, or `KeyBinding`; button text, headers, tooltips, and native-widget inference are therefore recorded as implicit presentation, not proven accessibility names (`src/ModSync.GUI/Controls/WizardHostControl.axaml:1-190`, `src/ModSync.GUI/Dialogs/WizardPages/WelcomePage.axaml:1-300`, `src/ModSync.GUI/MainWindow.axaml:1-491`).
- Status labels are: **matched** (Rust source contains the same control contract), **partial** (same purpose but different composition/copy/state), **missing** (no Rust control), and **covered** (an approved named plan owns the gap). A plan label is not implementation proof (`docs/parity/install-progress-plan.md:1-3`, `docs/parity/completion-pages-plan.md:1-3`, `docs/parity/mod-selection-plan.md:1-5`).

## Approved-plan coverage map

| C# surface | Current Rust source | Coverage disposition | Referents |
|---|---|---|---|
| Wizard shell/header/footer/theme/progress/download chrome | Route header, theme buttons, progress, footer, activity and 320 px download side panel exist, but active C# Download Center controls and exact status presentation differ | Shell geometry is **unplanned in the three approved plans** and goes to wizard T2. The Download Center controls themselves are owned by download-orchestration T2b in `src/ui/downloads_panel.rs` (`docs/parity/download-orchestration-plan.md:344`, `:365`); the shell places that panel and does not rebuild it | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:14-188`, `.axaml.cs:219-362`; Rust: `src/ui.rs:449-639`, `src/ui.rs:2949-3085`; `docs/parity/download-orchestration-plan.md:1-8` |
| Installing and Widescreen Installing | One shared typed renderer for Base/Remaining | **Covered and implemented by install-progress**; exact accessibility/platform acceptance remains open | `docs/parity/install-progress-plan.md:155-200`, `docs/parity/GUI-INVENTORY.md:1-37`; `src/ui.rs:648-650`; `src/ui/installing_page.rs:516-770` |
| Base Complete, Widescreen Notice, Widescreen Complete, Finished | One generic completion arm; notice shares bare metadata arm | **Covered, not implemented, by completion-pages** | `docs/parity/completion-pages-plan.md:17-27`, `docs/parity/completion-pages-plan.md:84-118`; `src/ui.rs:705-712`, `src/ui.rs:757-766` |
| Base Mod Selection | Rust has filters, selection actions, grouped list, details and option projection, but labels/composition differ | **Covered by mod-selection**; the plan intentionally preserves documented safety deviations | `docs/parity/mod-selection-plan.md:38-141`, `docs/parity/mod-selection-plan.md:199-258`; `src/ui.rs:924-1371` |
| Validation | Dedicated Rust page exists | Covered by the separate validation-page package, not by the three plans named for this audit | `docs/parity/GUI-INVENTORY.md:47-53`; `src/ui/validation_page.rs:269-455` |
| Welcome, Preamble, Mod Directory, Game Directory, Aspyr Notice, Downloads Explain composition, Install Start, Widescreen Selection | Rust has route arms but reduced or different controls/copy | **Unplanned** except shared download behavior; this plan must assign exact presentation work | C#: `src/ModSync.GUI/Dialogs/WizardPages/WelcomePage.axaml:14-298`, `PreamblePage.axaml:9-23`, `ModDirectoryPage.axaml:10-107`, `GameDirectoryPage.axaml:10-115`, `AspyrNoticePage.axaml:9-17`, `DownloadsExplainPage.axaml:9-40`, `InstallStartPage.axaml:9-30`, `WidescreenModSelectionPage.axaml:9-16`; Rust: `src/ui.rs:654-735`, `src/ui.rs:737-753` |
| Legacy main window, menus, landing, Getting Started, Summary/Editor/Raw tabs, log, drag/drop/loading overlays | Rust exposes utility pages, activity log and guide editor, but not the C# legacy composition or menu tree | **Unplanned** | C#: `src/ModSync.GUI/MainWindow.axaml:50-489`, `src/ModSync.GUI/Services/MenuBuilderService.cs:658-1026`; Rust: `src/ui.rs:771-1594`, `src/ui.rs:2667-2840`, `src/ui.rs:2900-3105` |

## Active wizard shell: control-by-control diff

| C# control | Binding/action and declared keyboard/a11y contract | Rust equivalent and delta | Status / owner | Referents |
|---|---|---|---|---|
| `LightThemeButton`, `K1ThemeButton`, `TslThemeButton` | Click handlers select Light/K1/K2; tooltip supplies visible help; no explicit accelerator or automation name | Three 36×28 selected buttons with the same labels/tooltips and persisted theme paths | matched in source; native a11y unproven | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:22-65`; Rust: `src/ui.rs:2957-2976` |
| `PageTitleText`, `PageSubtitleText` | Code-behind assigns active page `Title`/`Subtitle`; static text only, no explicit live-region/name declaration | Rust reads `Page::title/subtitle` and renders the same route strings | matched text; announcement behavior unproven | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:67-80`, `.axaml.cs:694-714`; Rust: `src/wizard.rs:25-65`, `src/ui.rs:2980-2996` |
| `ProgressStepText`, `WizardProgress` | `Step N of M`; maximum/value update with route count; no explicit progress a11y metadata | Same text, 200×6 progress computed from route index/count | matched in source | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:81-98`, `.axaml.cs:714-716`; Rust: `src/ui.rs:2999-3018` |
| Outer content `ScrollViewer` + `PageContent` | One vertical host scroll containing current page; code swaps content | Rust CentralPanel adds one outer vertical `wizard_content` scroll; Validation can bypass it at large size and page renderers can add nested scrolls | partial; shell task | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:103-108`, `.axaml.cs:373-422`; Rust: `src/ui.rs:3087-3105` |
| `DownloadStatusBarButton`, icon, text | Visible from Downloads onward; click requests details; icon/text reflect download state | Footer shows status text only; details are in persistent side panel; no compact status button/icon | partial; shell task | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:115-139`, `.axaml.cs:594-642`; Rust: `src/ui.rs:449-460`, `src/ui.rs:530-541` |
| `StatusText`, `StatusProgress` | Named transient footer status and indeterminate progress | No equivalent footer status/progress pair; global Activity owns job spinner/status | missing; shell task | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:141-159`; Rust: `src/ui.rs:3029-3067` |
| `BackButton` | Click; enabled only when page permits backward navigation; content `← Back`; no explicit access key/name | Rust `Back`, gated by route and idle worker; omits arrow glyph | partial; shell task | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:161-168`, `.axaml.cs:772-785`; Rust: `src/ui.rs:572-580`; `src/wizard.rs:68-80`, `src/wizard.rs:216-225` |
| `NextButton` | Click; active-page forward gate; content `Next →`, which changes to `Continue` on install pages and back to `Next →` elsewhere | Rust `Next`/`Continue` with shared gate and install confirmation; omits the `→` glyph | partial (label differs, like `← Back`); behavior matched with explicit Rust safety modal deviation; shell task | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:169-176` (`Content="Next →"` at `:176`), `.axaml.cs:772-785` (`:780`, `:785`); Rust: `src/ui.rs:552-570` (`"Next"` at `:558`), `src/ui.rs:3154-3166` |
| `FinishButton` | Visible on last page; raises completion and host exits wizard | Rust Finish calls `finish_wizard`, returns to retained loaded project | matched behavior | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml:177-184`, `.axaml.cs:871-875`; Rust: `src/ui.rs:545-551`, `docs/parity/completion-pages-plan.md:19-27` |
| Runtime Download Center: heading, Fetch, Open Workspace, status button/LED/running/progress, Stop, tip | Buttons raise typed host events; tooltips describe actions; visible from Downloads onward | Rust side panel has status, Stop, Open workspace, direct URL/name/hash inputs, Download archive, linked Nexus action, sources and limitation text; no Fetch-all, aggregate status button/LED/count | partial; controls belong to download-orchestration T2b (`src/ui/downloads_panel.rs`), placement belongs to the shell task | C#: `src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:219-362`; Rust: `src/ui.rs:470-528`, `src/ui.rs:3078-3085` |

## Wizard pages: every authored control and binding

Static copy is grouped only where adjacent `TextBlock`s share one authored content card; every interactive, named, repeated, or status control is listed separately.

### Welcome

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Page `ScrollViewer` | Vertical Auto, horizontal Disabled | Outer Rust host scroll only | partial | C#: `src/ModSync.GUI/Dialogs/WizardPages/WelcomePage.axaml:14-15`; Rust: `src/ui.rs:3092-3105` |
| Hero title/subtitle and feature strip | Static `Welcome to ModSync`, solution tagline, three feature labels; no explicit automation properties | Same text collapsed into centered rows | matched text, different composition | C#: `WelcomePage.axaml:19-39`; Rust: `src/ui.rs:654-660` |
| Problem card | Heading, lead, five bullets | Same semantic copy; punctuation/icon/style differ | partial | C#: `WelcomePage.axaml:43-62`; Rust: `src/ui.rs:662-667` |
| Solution card | Heading, lead, five bullets | Same semantic copy with shortened lead | partial | C#: `WelcomePage.axaml:64-82`; Rust: `src/ui.rs:668-672` |
| Installation Process | Heading plus five numbered cards and details | Same five steps, but step 4 truthfully says manual/direct acquisition instead of C# automatic background downloads | deliberate partial | C#: `WelcomePage.axaml:86-204`; Rust: `src/ui.rs:675-684` |
| Key Features | Heading plus four feature cards | Same four titles/details in a 2-column group | partial layout | C#: `WelcomePage.axaml:212-259`; Rust: `src/ui.rs:685-693` |
| Fresh-install warning and closing prompt | Static warning icon/title/body and two closing lines | Same essential copy, reduced card/icon treatment; Rust adds loaded component count | partial | C#: `WelcomePage.axaml:265-293`; Rust: `src/ui.rs:694-703` |

### Preamble and Aspyr Notice

| Page/control | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Preamble hero and `ContentScrollViewer` | Static book/title; code replaces scroll content with `MarkdownRenderer.RenderToPanel`; no explicit keyboard/a11y binding | Rust emits raw metadata through a label, so rich Markdown structure/links are absent | missing rich presentation; unplanned | C#: `PreamblePage.axaml:9-23`, `PreamblePage.axaml.cs:42-54`; Rust: `src/ui.rs:705-712` |
| Preamble tip card | Static icon plus wrapped instruction | Missing | missing; unplanned | C#: `PreamblePage.axaml:17-22`; Rust: `src/ui.rs:705-712` |
| Aspyr heading, content scroll/text, closing notice | Code assigns raw authored Aspyr text; static warning and closing line; no explicit automation properties | Rust renders only raw authored metadata; heading/card/closing copy absent | partial; unplanned | C#: `AspyrNoticePage.axaml:9-17`, `AspyrNoticePage.axaml.cs:36-44`; Rust: `src/ui.rs:705-712` |

### Mod Workspace Directory

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Page scroll; hero icon/title/subtitle | Vertical page scroll and static intro | Rust uses outer scroll and one short instruction line | partial | C#: `ModDirectoryPage.axaml:10-24`; Rust: `src/ui.rs:713-716` |
| “What will be stored here?” card | Heading plus four static bullets | Missing | missing; unplanned | C#: `ModDirectoryPage.axaml:29-44`; Rust: `src/ui.rs:713-716` |
| “Recommended Setup” card | Heading plus four static bullets | Only “separate from game” survives in intro | missing composition; unplanned | C#: `ModDirectoryPage.axaml:46-59`; Rust: `src/ui.rs:713-716` |
| `SourcePathPicker` | `DirectoryPickerControl` with title, description, picker type and watermark; child has title/description/current path, editable path, Browse button and suggestions; Enter validates/commits through `OnPathInputKeyDown` | Rust `path_row` has label, single-line field and Browse; no current-path display/suggestions or explicit Enter handler | partial; unplanned | C#: `ModDirectoryPage.axaml:63-72`, `src/ModSync.GUI/Controls/DirectoryPickerControl.axaml:13-65`, `.axaml.cs:678-707`; Rust: `src/ui.rs:713-716`, `src/ui.rs:2600-2614` |
| Validation feedback: `ValidationTitle`, `ValidationMessage` | Code switches valid/invalid title/message and visibility | Rust one line says exists or choose existing | partial; unplanned | C#: `ModDirectoryPage.axaml:74-91`, `ModDirectoryPage.axaml.cs:133-152`; Rust: `src/ui.rs:716` |
| Bottom information card | Static icon and “change later” copy | Missing | missing; unplanned | C#: `ModDirectoryPage.axaml:94-106`; Rust: `src/ui.rs:713-716` |

### KOTOR Game Directory

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Page scroll; hero icon/title/subtitle | Vertical page scroll and static intro | Rust outer scroll and short instruction | partial | C#: `GameDirectoryPage.axaml:10-24`; Rust: `src/ui.rs:718-721` |
| Common Locations card | Steam/GOG/Origin labels and paths | Missing | missing; unplanned | C#: `GameDirectoryPage.axaml:29-46`; Rust: `src/ui.rs:718-721` |
| Important Notes card | Four static requirements | Only supported-PC detection is represented indirectly | missing composition; unplanned | C#: `GameDirectoryPage.axaml:48-61`; Rust: `src/ui.rs:718-721` |
| `DestinationPathPicker` | Same child controls as Source picker with KOTOR picker type; Enter path handling lives in shared control | Rust label/field/Browse only | partial; unplanned | C#: `GameDirectoryPage.axaml:65-74`, `src/ModSync.GUI/Controls/DirectoryPickerControl.axaml:13-65`, `.axaml.cs:678-707`; Rust: `src/ui.rs:718-721`, `src/ui.rs:2600-2614` |
| Validation title/message/game details | Code reports valid game, invalid directory and detected game details | Rust reports debug-form detected game only | partial; unplanned | C#: `GameDirectoryPage.axaml:76-98`, `GameDirectoryPage.axaml.cs:134-190`; Rust: `src/ui.rs:721` |
| Detection information card | Static icon and executable-detection copy | Missing | missing; unplanned | C#: `GameDirectoryPage.axaml:102-114`; Rust: `src/ui.rs:718-721` |

### Base Mod Selection

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| `SelectionCountText`, `SelectionDetailsText` | Code updates selected/total and contextual detail | Rust selection and filter summaries exist but use different copy and placement | partial; mod-selection plan | C#: `ModSelectionPage.axaml:18-27`, `.axaml.cs:852-876`; Rust: `src/ui.rs:1093-1113` |
| Select All, Deselect All, Select by Tier, Select by Category | Click handlers; content/tooltips are the only declared labels/help | Rust Select/Deselect, tier action, category action and multi-category modal exist with different labels/placement and deliberate policy differences | covered | C#: `ModSelectionPage.axaml:31-55`, `.axaml.cs:140-218`; Rust: `src/ui.rs:942-970`, `src/ui.rs:1033-1091`, `src/ui.rs:1114-1128`, `docs/parity/mod-selection-plan.md:38-116` |
| `SearchTextBox` | TextChanged filters by name/description; watermark and tooltip | Rust single-line search with different hint | covered | C#: `ModSelectionPage.axaml:61-71`, `.axaml.cs:160-167`; Rust: `src/ui.rs:971-988` |
| Category and Tier filter combos | SelectionChanged filters; placeholders/tooltips | Rust combos with same filter dimensions but reversed order and different case | covered | C#: `ModSelectionPage.axaml:72-89`, `.axaml.cs:169-185`; Rust: `src/ui.rs:1009-1070` |
| `SpoilerFreeToggle` | IsCheckedChanged updates page state; content/tooltip only | Rust persisted checkbox with longer label | covered, visual/a11y proof open | C#: `ModSelectionPage.axaml:90-98`, `.axaml.cs:187-205`; Rust: `src/ui.rs:989-1008` |
| Expand/Collapse All | Click changes label and category-expander state | Rust button changes all category groups | covered | C#: `ModSelectionPage.axaml:99-106`, `.axaml.cs:90-94`, `.axaml.cs:1132-1141`; Rust: `src/ui.rs:1147-1159` |
| Filter summary | Code displays visible/total and active filters | Rust uses shared filter summary | covered | C#: `ModSelectionPage.axaml:110-115`, `.axaml.cs:899-935`; Rust: `src/ui.rs:1104-1113` |
| `ModListScrollViewer` and dynamic category/component/option cards | Code builds category selection button/expander, component checkbox/name/metadata/details, nested option checkboxes/details | Rust two-column grouped list/detail view and option projection; exact card hierarchy differs | covered for policy, exact presentation still acceptance work | C#: `ModSelectionPage.axaml:119-128`, `.axaml.cs:323-760`; Rust: `src/ui.rs:1129-1371` |
| Selected/Required/Optional badges | Code updates all three counts | Rust shows Required/Optional, while selected is embedded in summary rather than a `SELECTED` badge | partial; mod-selection plan | C#: `ModSelectionPage.axaml:132-173`, `.axaml.cs:852-891`; Rust: `src/ui.rs:1093-1103` |
| `ClearFiltersButton` | Visible only with active filters; resets filters | Rust disabled rather than hidden when inactive | covered with presentation delta | C#: `ModSelectionPage.axaml:175-183`, `.axaml.cs:150-158`, `.axaml.cs:826-843`, `.axaml.cs:937-951`; Rust: `src/ui.rs:975-987` |

### Download Process

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Page scroll and centered heading | Static | Rust heading `Download your mod files` | partial | C#: `DownloadsExplainPage.axaml:9-16`; Rust: `src/ui.rs:724-725` |
| Intro and `SelectionSummaryText` | Code reports selected/total; static intro promises background download | Rust reports selected base count and truthful manual/direct acquisition limitation | deliberate partial; shared behavior in download plan | C#: `DownloadsExplainPage.axaml:17-25`, `.axaml.cs:36-46`; Rust: `src/ui.rs:726-727` |
| “What's happening” card: heading and four bullets | Static download/background/status/install-wait claims | Reduced to one paragraph; detailed controls live in side panel | missing exact composition; shell/page task | C#: `DownloadsExplainPage.axaml:27-33`; Rust: `src/ui.rs:470-528`, `src/ui.rs:724-728` |
| Tip card | Static continue-while-download copy | Same rule is enforced by `wizard_gate`, but card is absent | missing presentation; shell/page task | C#: `DownloadsExplainPage.axaml:35-39`; Rust: `src/ui.rs:390-425`, `src/ui.rs:724-728` |

### Validation

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Heading/description, Run Validation, progress, status | Click runs validation; progress/status change in code | Dedicated Rust page has Run/Stop, progress and status projection | covered by validation package | C#: `ValidatePage.axaml:13-47`, `.axaml.cs:89-104`, `.axaml.cs:157-167`, `.axaml.cs:293-430`; Rust: `src/ui/validation_page.rs:269-335` |
| Log expander/header/progress, first issue, copy, resize hint | Named controls; click handlers; tooltips; no explicit focus/accessibility declaration | Rust has the same actions and a collapsible log, with native proof bounded separately | covered by validation package | C#: `ValidatePage.axaml:52-98`, `.axaml.cs:157-167`, `.axaml.cs:680-682`; Rust: `src/ui/validation_page.rs:336-353` |
| Log scroll/text, splitter, result scroll/cards | Code appends log and builds result entries | Rust has independent scroll, splitter state and result cards | covered by validation package | C#: `ValidatePage.axaml:100-137`, `.axaml.cs:188-280`, `.axaml.cs:462-613`; Rust: `src/ui/validation_page.rs:358-440` |
| Summary/details and Error/Warning/Passed badges | Code updates counts and terminal copy | Rust renders corresponding summary and counts | covered by validation package | C#: `ValidatePage.axaml:139-170`, `.axaml.cs:618-663`; Rust: `src/ui/validation_page.rs:448-455` |

### Ready to Install

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| `HeaderText` and review instruction | Static heading/body | Rust count/game/workspace text, without heading | partial; unplanned | C#: `InstallStartPage.axaml:9-12`; Rust: `src/ui.rs:730-735` |
| `SelectedModsText` + `ModListScrollViewer` | Code reports selected count and adds ordered selected names | Rust reports count and ordered names through shared ordering | matched data, different composition | C#: `InstallStartPage.axaml:13-18`, `.axaml.cs:79-105`; Rust: `src/ui.rs:731-734` |
| Important heading and three bullets | Static irreversible/checkpoint/do-not-close claims | Rust uses a truthful snapshot/stop-on-first-failure sentence and confirmation modal; exact card absent | deliberate partial; unplanned page | C#: `InstallStartPage.axaml:20-29`; Rust: `src/ui.rs:734-735`, `src/ui.rs:3154-3166` |

### Installing Mods and Installing Widescreen Mods

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Base Installing: page scroll; percent/count/main progress; current mod/directions/operation/current progress; checkpoint status; elapsed/remaining/rate; warnings/errors/checkpoints; showcase; What's Happening; warning | Code-behind updates named fields from install/checkpoint/log callbacks; there are no page-local buttons or explicit automation properties | Rust shared renderer covers the corresponding facts, substitutes one recovery-snapshot state, refuses text-derived issue counts, adds safe Stop/Restore/Retry, and does not copy five measured C# defects | covered and implemented by install-progress | C#: `InstallingPage.axaml:9-251`, `.axaml.cs:183-470`; Rust: `src/ui/installing_page.rs:516-770`; `docs/parity/GUI-INVENTORY.md:1-37` |
| Widescreen Installing: page scroll, `StatusText`, `ProgressBar` | Code loops selected widescreen mods and updates status/progress; no explicit automation properties | Rust deliberately reuses the full shared Installing renderer for Remaining instead of the older reduced page | covered deliberate improvement | C#: `WidescreenInstallingPage.axaml:9-14`, `.axaml.cs:67-140`; Rust: `src/ui.rs:648-650`, `src/ui/installing_page.rs:516-770` |

### Base Installation Complete

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Page scroll; success icon/title/body | Static | Generic Rust completion sentence only | covered, not implemented | C#: `BaseInstallCompletePage.axaml:9-27`; Rust: `src/ui.rs:757-761`; `docs/parity/completion-pages-plan.md:84-108` |
| Four stat cards: Mods Installed, Checkpoints, Time Elapsed, Files Modified | Code assigns four values | Absent; approved plan replaces untruthful checkpoint/file claims with recovery snapshot and game-file delta | covered, not implemented | C#: `BaseInstallCompletePage.axaml:29-98`, `.axaml.cs:74-100`; `docs/parity/completion-pages-plan.md:29-76`, `docs/parity/completion-pages-plan.md:86-94` |
| What's Next card | Three numbered guidance groups | Absent | covered, not implemented | C#: `BaseInstallCompletePage.axaml:100-136`; `docs/parity/completion-pages-plan.md:91-94` |
| Important Notes and hidden managed summary | Four bullets plus hidden status | Absent | covered, not implemented | C#: `BaseInstallCompletePage.axaml:140-178`; `docs/parity/completion-pages-plan.md:91-108` |
| Quick Actions: View Logs, Manage Checkpoints, Open Game Folder, Open Mod Folder | Click handlers attached in code; content is only declared name | Rust has Open game/workspace and Copy log; plan defines truthful Copy Log/Recovery replacements | covered, not implemented | C#: `BaseInstallCompletePage.axaml:182-228`, `.axaml.cs:103-123`; Rust: `src/ui.rs:762-764`; `docs/parity/completion-pages-plan.md:91-108` |
| Footer instruction | Static | Missing | covered, not implemented | C#: `BaseInstallCompletePage.axaml:232-241`; `docs/parity/completion-pages-plan.md:91-94` |

### Widescreen Notice, Selection, and Complete

| Page/control | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Notice heading, bordered `ContentScrollViewer`, raw `ContentText` | Code assigns raw authored warning; no explicit automation properties | Rust currently shares bare metadata arm; completion plan splits exact notice composition | covered by completion-pages, not implemented | C#: `WidescreenNoticePage.axaml:9-15`, `.axaml.cs:42-48`; Rust: `src/ui.rs:705-712`; `docs/parity/completion-pages-plan.md:95-98` |
| Selection instruction, `SelectionSummaryText`, `ModListScrollViewer`, `ModListPanel` | Code creates one checkbox per widescreen component and updates summary; page cannot Cancel | Rust creates one checkbox per component, summary and inline Remaining validation surface; exact scroll/panel/cancel semantics differ | partial; **unplanned by approved plans** | C#: `WidescreenModSelectionPage.axaml:9-16`, `.axaml.cs:37-90`; Rust: `src/ui.rs:737-753`; `src/wizard.rs:68-80` |
| Complete page scroll, heading/body, hidden managed summary | Static completion content; page cannot Cancel | Generic completion arm; approved plan specifies installed-vs-skipped truthful copy | covered, not implemented | C#: `WidescreenCompletePage.axaml:6-11`, `.axaml.cs:21-27`; Rust: `src/ui.rs:757-765`; `docs/parity/completion-pages-plan.md:99-102` |

### Finished

| Controls | Binding/action and declared keyboard/a11y contract | Rust diff | Status | Referents |
|---|---|---|---|---|
| Page scroll; completion heading/body; hidden managed summary | Static; page cannot Cancel | Generic completion sentence | covered, not implemented | C#: `FinishedPage.axaml:6-11`, `.axaml.cs:21-29`; Rust: `src/ui.rs:757-765`; `docs/parity/completion-pages-plan.md:103-108` |
| Next Steps card | Heading plus three bullets | Missing | covered, not implemented | C#: `FinishedPage.axaml:14-23`; `docs/parity/completion-pages-plan.md:103-108` |
| Support card | Heading plus two lines | Missing | covered, not implemented | C#: `FinishedPage.axaml:25-30`; `docs/parity/completion-pages-plan.md:103-108` |
| Finish instruction | Static | Rust says Finish returns to loaded project | covered copy delta | C#: `FinishedPage.axaml:32-33`; Rust: `src/ui.rs:765`; `docs/parity/completion-pages-plan.md:19-27` |

### Inactive `LoadInstructionPage`

| Controls | Binding/action and route status | Rust diff | Status | Referents |
|---|---|---|---|---|
| `LandingPageHost` only | Hosts `LandingPageView`; page title/subtitle still exist in code, but active host explicitly excludes it | Rust exposes guide/settings as utility/landing functionality outside its wizard | F25 landing scope, not an F24 active page | C#: `LoadInstructionPage.axaml:4-12`, `.axaml.cs:39-41`, `src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:128-132`; Rust: `src/ui.rs:771-923`, `src/ui.rs:2905-2943` |

## Legacy main window: direct controls

| C# control(s) | Binding/action and declared keyboard/a11y contract | Rust equivalent and gap | Referents |
|---|---|---|---|
| `TopMenu` | ItemsSource comes from `MenuBuilderService`; menu headers are visible names; no explicit access keys/gestures | No menu bar | C#: `src/ModSync.GUI/MainWindow.axaml:63-66`, `.axaml.cs:1010-1038`; Rust: `src/ui.rs:2900-2948` |
| `SpoilerFreeModeToggle` | Two-way `SpoilerFreeMode`, hidden in editor, tooltip | Rust spoiler checkbox exists inside Mod Selection, not persistent top-level chrome | C#: `MainWindow.axaml:68-77`; Rust: `src/ui.rs:989-1008` |
| `TitleTextBlock` | Static ModSync title | Rust `ModSync.rs` plus tagline | C#: `MainWindow.axaml:79-87`; Rust: `src/ui.rs:2900-2904` |
| `EditorModeToggle`, `WizardModeToggle` | Two-way mode bindings, conditional visibility, tooltips | Rust buttons enter utility editor or wizard; no equivalent two-way toggles | C#: `MainWindow.axaml:89-107`; Rust: `src/ui.rs:2905-2943` |
| Main-window Light/K1/TSL theme buttons | Click handlers, tooltips | Theme controls appear only in Rust wizard header | C#: `MainWindow.axaml:109-153`; Rust: `src/ui.rs:2949-2977` |
| Minimize, maximize, close buttons | Click handlers; icon-only content `-`, `▢`, `X`; no explicit names/tooltips | Rust uses native viewport decorations; no app-drawn controls | C#: `MainWindow.axaml:155-183`; Rust: `src/ui.rs:3171-3202` |
| `HomeButton` | Click, content and tooltip, visible with main content | No Home button; utility navigation uses Guide/settings, Guide editor and Return to wizard | C#: `MainWindow.axaml:186-202`; Rust: `src/ui.rs:2905-2943` |
| `WizardHost` | Bound to `WizardContentVisible` | Rust conditionally renders wizard panels when `wizard` exists and utility is false | C#: `MainWindow.axaml:204-214`; Rust: `src/ui.rs:2949-3105` |
| `MainGrid` and `ModListSidebar` | Visibility binding; sidebar exposes 16 named requested events plus two-way search | Rust Mod Selection is wizard-specific; no persistent editor sidebar/action event surface | C#: `MainWindow.axaml:216-254`; Rust: `src/ui.rs:924-1371` |
| Editor `GridSplitter` | Resize sidebar/editor columns | No equivalent persistent editor split | C#: `MainWindow.axaml:255-259`; Rust: `src/ui.rs:3092-3105` |
| `TabControl`, hidden Getting Started, Summary, Editor, Raw tabs | Visibility and selection handler; child controls bind current component/editor/spoiler/services and raise requested events | Rust has utility setup, guide editor and wizard selection but no tabbed legacy composition or Summary tab | C#: `MainWindow.axaml:265-357`; Rust: `src/ui.rs:771-1594`, `src/ui.rs:2667-2840`, `src/ui.rs:2905-2943` |
| `LandingPage` | Visible from `LandingPageVisible` | Rust setup surface fills CentralPanel when no wizard | partial composition | C#: `MainWindow.axaml:362-372`; Rust: `src/ui.rs:771-923`, `src/ui.rs:3092-3105` |
| `LogPanelSplitter`, `EmbeddedLogPanel` | Resizable 160 px output panel | Rust Activity panel is resizable and contains copy/save/stop/log | partial | C#: `MainWindow.axaml:374-387`; Rust: `src/ui.rs:3029-3067` |
| Drag/drop overlay: title, filename, info, action | Window accepts drag/drop; code updates four named output controls | No drag/drop overlay | C#: `MainWindow.axaml:389-451`; Rust: `src/ui.rs:3171-3202` |
| Loading overlay: spinner/text | Code controls modal-looking non-hit-test overlay | Rust worker spinner appears in Activity, without full-window overlay | C#: `MainWindow.axaml:455-489`; Rust: `src/ui.rs:3034-3047` |

## Legacy child surfaces: control inventory

This section inventories every primary/named or repeated control reachable directly from `MainWindow`; nested editor components are identified as composite controls because their independent F21 editor parity is outside the F25 main-window shell (`docs/parity/ACCEPTANCE.md:56`, `:60`).

| Surface | Controls and binding/action contract | Rust disposition | Referents |
|---|---|---|---|
| `LandingPageView` | Header/subheader; Load Instruction button/status; Create Instructions button/status; Recent Files list items; Sponsor button. Button clicks call parent window operations; status text is refreshed in code | Setup surface can load/draft guides, but no two-card landing/recent-files/sponsor composition | C#: `src/ModSync.GUI/Controls/LandingPageView.axaml:117-244`, `.axaml.cs:23-55`; Rust: `src/ui.rs:771-923` |
| `GettingStartedTab` steps 1-2 | Step status cards; two `DirectoryPickerControl`s; Load Instruction, Import Clipboard, Settings | Rust setup covers paths/load/draft/settings primitives, but not numbered step/status composition or clipboard import button | C#: `GettingStartedTab.axaml:47-145`; Rust: `src/ui.rs:773-923` |
| `GettingStartedTab` steps 3-4 | Step status; prose selection guidance; Scrape Downloads, Open Mod Directory, download LED/status/details/Stop/progress, Validate, hidden checkbox | Rust has selection page, Download side panel and validation page, but no legacy combined controls/status | C#: `GettingStartedTab.axaml:149-318`; Rust: `src/ui.rs:470-528`, `src/ui.rs:924-1371`, `src/ui/validation_page.rs:269-455` |
| `GettingStartedTab` validation results | Summary; error counter/name; Previous/Next/Jump; type/description; Auto Fix; success panel | Rust validation has first-issue navigation and cards, not the legacy per-error navigation/auto-fix surface | C#: `GettingStartedTab.axaml:317-429`; Rust: `src/ui/validation_page.rs:336-455` |
| `GettingStartedTab` install/help | Overall progress/text; Install; hidden completion checkbox; rich-text help runs; Output, Settings, GitHub issue, Sponsor, Jump to current step | Rust has wizard install and Activity controls but lacks this legacy combined help/action surface and its inline rich-text run structure | C#: `GettingStartedTab.axaml:432-568`; Rust: `src/ui.rs:530-639`, `src/ui.rs:3029-3067` |
| `ModListSidebar` | Search box; Select/Deselect; tier combo; tier/category selectors; Refresh; stats/counts; mod list; Global Actions flyout trigger | Rust selection has search/filter/actions/list but no persistent editor sidebar or global-actions flyout | C#: `src/ModSync.GUI/Controls/ModListSidebar.axaml:14-259`, `.axaml.cs:41-192`; Rust: `src/ui.rs:924-1371` |
| `SummaryTab`: information and flags | Current-component name uses spoiler converter; editor-only GUID; author/method/language; widescreen/Aspyr badges; category/tier | Rust details show spoiler-aware name/description, author/tier and metadata selectively; no equivalent full card set | C#: `SummaryTab.axaml:42-191`; Rust: `src/ui.rs:1272-1358` |
| `SummaryTab`: download links | Download Mod; normal URL repeater with open/copy/invalid state; spoiler mode replaces it with an explicit “may contain spoilers” expander and a second repeater | Rust always shows download links in details when available; spoiler-free projection hides authored text but does not gate potentially spoiler-bearing URLs behind the C# warning expander | C#: `SummaryTab.axaml:193-311`; Rust: `src/ui.rs:1316-1331` |
| `SummaryTab`: rich content cards | Description, Directions, Download Instructions, Usage Warning and Screenshots use spoiler-aware projection; Known Bugs, Installation Warning, Compatibility Warning and Steam Notes use direct bindings | Rust displays description, optional normal-mode directions and selected metadata, but has no general Markdown-to-rich-text rendering for these cards | C#: `SummaryTab.axaml:314-470`, `SummaryTab.axaml.cs:95-189`, `SummaryTab.axaml.cs:421-474`; Rust: `src/ui.rs:1295-1358` |
| `SummaryTab`: options/instructions | Options repeater with selected state/name/description and spoiler converters; Instructions repeater with editor jump, index, action/source/destination/arguments/Choose targets | Rust option projection and instruction-count/draft JSON exist, but no equivalent read-only instruction expander or editor jump | C#: `SummaryTab.axaml:473-662`; Rust: `src/ui.rs:1301-1358` |
| `EditorTab` top/actions | Expand/Collapse all; Basic Info, Description, Dependencies, Instructions, Options expanders; Auto Generate; repeaters for instructions/options | Rust guide editor has component fields/instruction CRUD but no matching expander/control composition; nested C# editor controls remain F21 scope | C#: `EditorTab.axaml:42-607`; Rust: `src/ui.rs:2667-2840`; `docs/parity/ACCEPTANCE.md:56` |
| `RawTab` | Apply Editor Changes; generated GUID text; format tabs; TOML, Markdown, YAML, JSON multiline editors | Rust guide editor has no raw multi-format tabbed editing surface | C#: `RawTab.axaml:12-97`, `.axaml.cs:54-194`; Rust: `src/ui.rs:2667-2840` |
| `EmbeddedLogPanel` | Expand toggle; line count; Auto-scroll; Clear/Copy/Save; scroll/list; context Copy selected; row severity/timestamp/message | Rust Activity has Stop, Copy report, Save report and scroll, but no expand, line count, auto-scroll toggle, clear, selected-copy or structured row presentation | C#: `EmbeddedLogPanel.axaml:11-150`, `.axaml.cs:61-251`; Rust: `src/ui.rs:3029-3067` |
| `DirectoryPickerControl` | Title, description, current path, editable path, Browse, suggestions; Enter invokes path validation/selection | Rust `path_row` is label, field and Browse only | C#: `DirectoryPickerControl.axaml:13-65`, `.axaml.cs:678-707`; Rust: `src/ui.rs:2600-2614` |

## Menu inventory

| Menu | Every declared item | Rust disposition | Referents |
|---|---|---|---|
| File | Open File; Close (editor-only); Save (editor-only); Exit | No menu | `src/ModSync.GUI/Services/MenuBuilderService.cs:665-695`; `src/ui.rs:2900-2948` |
| Tools | Editor Mode; separator; Fix iOS case sensitivity; Fix file/folder permissions; Manage Checkpoints; Run HoloPatcher; Settings; Managed Deployment Status; Purge Managed Deployments; Show Output Log; non-Windows Fix file and folder permissions | No menu; some operations exist elsewhere, but not this action surface | `MenuBuilderService.cs:697-792`; `src/ui.rs:771-1594`, `src/ui.rs:2900-3067` |
| Help | DeadlyStream → Discord/Website; KOTOR Community Portal → Discord/Website; PCGamingWiki → KOTOR 1/KOTOR 2 | No menu | `MenuBuilderService.cs:794-846`; `src/ui.rs:2900-2948` |
| About | The ModSync Project → DeadlyStream/GitHub; HoloPatcher → DeadlyStream/GitHub | No menu | `MenuBuilderService.cs:952-992`; `src/ui.rs:2900-2948` |
| More | Other Projects → PyKotor and nested HoloPatcher/Holocron/Auto-Translate/KotorDiff, LIP Composer/reone, open-source engines; Modding Tools | No menu | `MenuBuilderService.cs:848-951`, `MenuBuilderService.cs:993-1006`; `src/ui.rs:2900-2948` |
| Per-mod context menu (`BuildContextMenuForComponent`) | All modes: Select/Deselect Mod; Uninstall Managed Deployment (only when deployed, confirmed). Editor mode: Move Up, Move Down, Move to Top, Move to Bottom, Delete Mod, Duplicate Mod, Edit Instructions, Edit Raw TOML, Test Install This Mod, Validate Mod Files | No context menu | `MenuBuilderService.cs:68-135`, `:223-354`; `src/ui.rs:2900-3105` |
| Global Actions flyout (`BuildGlobalActionsFlyout`) | Common items: Refresh List, Profiles..., Validate All Mods, Analyze File Conflicts, Check for Nexus Updates, Managed Deployment Status, Purge Managed Deployments. Flyout-only: Generate Instructions from ModLinks, Lock Install Order, Remove All Dependencies. Editor mode: Add New Mod, Mod Management Tools, Mod Statistics, Save Config, Close TOML | No Global Actions surface; conflicts and update check are reachable elsewhere | `MenuBuilderService.cs:137-187`, `:356-525` (Analyze File Conflicts `:392`, Check for Nexus Updates `:413`), `:527-583`; Rust: `src/ui.rs:1499-1501` |
| Global Actions context menu (`BuildGlobalActionsContextMenu`) | Same common items, without the three flyout-only items. Editor mode: Add New Mod, Mod Management Tools, Mod Statistics, Save Config, Close TOML | No equivalent | `MenuBuilderService.cs:189-219`, `:356-525`, `:585-656` |

## Keyboard and accessibility declaration diff

| Surface | C# source declaration | Rust source declaration | Gap / required proof | Referents |
|---|---|---|---|---|
| Wizard navigation | Buttons are native controls with visible content; no explicit access keys, focus order, default button, Escape route, or automation names | egui buttons have visible text; no explicit keyboard/focus/a11y policy in these render arms | Both require driven Tab/Shift+Tab, Enter/Space, Escape, focus return and accessibility-tree inspection; source alone cannot establish parity | C#: `WizardHostControl.axaml:161-184`; Rust: `src/ui.rs:542-639`; `docs/parity/GUI-INVENTORY.md:127` |
| Directory entry | Explicit `KeyDown`; Enter validates/commits path | No explicit Enter handler in `path_row` | Missing explicit behavior; native default is not evidence | C#: `DirectoryPickerControl.axaml:39-45`, `.axaml.cs:678-707`; Rust: `src/ui.rs:2600-2614` |
| Mod list | Ctrl+Up/Ctrl+Down reorder, Delete removes after confirmation, Space toggles selection in EditorMode | No persistent editor mod list, so no equivalent bindings | Missing F25 keyboard contract | C#: `MainWindow.axaml.cs:2110-2167`; Rust: `src/ui.rs:2900-3105` |
| Embedded log | ListBox KeyDown supports explicit keyboard behavior; context menu copies selected | Activity log is labels inside ScrollArea; no selectable row/key handler | Missing F25 log keyboard contract | C#: `EmbeddedLogPanel.axaml:65-79`, `.axaml.cs:226-251`; Rust: `src/ui.rs:3060-3066` |
| Icon-only/custom controls | Theme and window buttons rely on tooltip/content; progress/status have no explicit live semantics | Theme buttons have tooltips; progress/status announcements are not declared | Add stable accessible names, roles/states and status announcements, then inspect native trees | C#: `MainWindow.axaml:109-183`, `WizardHostControl.axaml:22-98`; Rust: `src/ui.rs:2957-3018`; `docs/parity/GUI-INVENTORY.md:127` |
| Modals | C# selection dialogs are programmatic Windows with Select/Cancel; no explicit trap/restore declarations | Rust category/install/stop use `egui::Modal`, with source-level close handling | Native trap, Escape, initial focus and return-focus remain acceptance cells | C#: `ModSelectionPage.axaml.cs:1159-1219`, `.axaml.cs:1280-1366`; Rust: `src/ui.rs:3107-3166`; `docs/parity/mod-selection-plan.md:310-330` |

## Rich text and spoiler-variant diff

| Contract | C# | Rust | Verdict | Referents |
|---|---|---|---|---|
| Preamble Markdown | `MarkdownRenderer.RenderToPanel` produces structured content | Raw metadata label | missing | C#: `PreamblePage.axaml.cs:42-54`; Rust: `src/ui.rs:705-712` |
| Summary Markdown | `SummaryTab` attaches `MarkdownRenderingService` to content text blocks and refreshes them | Plain egui labels/monospace JSON | missing | C#: `SummaryTab.axaml.cs:95-189`, `SummaryTab.axaml.cs:421-474`; Rust: `src/ui.rs:1272-1358` |
| Spoiler-aware name/description/directions/download instructions/usage warning/screenshots/options | MultiBinding uses `SpoilerFreeContentConverter`; normal and spoiler properties are separate inputs | Shared Rust display helpers cover component/option name, description and tooltip; directions are hidden in spoiler mode, but the wider C# card set is absent | partial | C#: `SummaryTab.axaml:57-75`, `SummaryTab.axaml:319-418`, `SummaryTab.axaml:483-532`; Rust: `src/ui.rs:1240-1306`, `src/ui.rs:2625-2645` |
| Spoiler-bearing download URLs | Normal repeater is replaced in spoiler mode by a warning expander before URLs appear | URLs remain visible in selected component details | missing safety/presentation parity | C#: `SummaryTab.axaml:214-306`; Rust: `src/ui.rs:1316-1331` |
| Raw Markdown editing | Dedicated Markdown tab and text box | No raw format tabs | missing | C#: `RawTab.axaml:53-94`; Rust: `src/ui.rs:2667-2840` |

## Consolidated unplanned work

1. Exact wizard shell scrolling, footer transient status, compact download status bar, Download Center controls, and source-declared accessibility/focus semantics are not owned by install-progress, completion-pages, or mod-selection (`src/ModSync.GUI/Controls/WizardHostControl.axaml:14-188`, `.axaml.cs:219-362`; `src/ui.rs:449-639`, `src/ui.rs:2949-3085`).
2. Exact Welcome, Preamble Markdown/tip, Aspyr framing, both directory pages, Download Process cards, Install Start card, and Widescreen Selection composition are not owned by those plans (`src/ModSync.GUI/Dialogs/WizardPages/WelcomePage.axaml:14-298`, `PreamblePage.axaml:9-23`, `AspyrNoticePage.axaml:9-17`, `ModDirectoryPage.axaml:10-107`, `GameDirectoryPage.axaml:10-115`, `DownloadsExplainPage.axaml:9-40`, `InstallStartPage.axaml:9-30`, `WidescreenModSelectionPage.axaml:9-16`).
3. The legacy main window's top-level controls, full menu tree, landing/onboarding surfaces, persistent mod sidebar, Summary/Editor/Raw tabs, structured log, drag/drop overlay and loading overlay have no F25 implementation plan (`src/ModSync.GUI/MainWindow.axaml:50-489`, `src/ModSync.GUI/Services/MenuBuilderService.cs:658-1026`; `src/ui.rs:2900-3105`).
4. Explicit keyboard and accessibility acceptance is unowned across both domains; existing plans leave it as a final native gate rather than an implementation unit (`docs/parity/GUI-INVENTORY.md:127`, `docs/parity/GUI-INVENTORY.md:159-174`, `docs/parity/mod-selection-plan.md:310-330`).
5. Rich Markdown content and the complete spoiler-variant matrix are not covered by the three plans (`src/ModSync.GUI/Dialogs/WizardPages/PreamblePage.axaml.cs:42-54`, `src/ModSync.GUI/Controls/SummaryTab.axaml:57-75`, `src/ModSync.GUI/Controls/SummaryTab.axaml:214-418`, `src/ModSync.GUI/Controls/SummaryTab.axaml.cs:95-189`; `src/ui.rs:705-712`, `src/ui.rs:1240-1331`).

## Claim blocks

claim: The active C# install wizard begins at Welcome, not Load Instruction, and conditionally inserts preamble, Aspyr, and post-base widescreen pages.
referent: `src/ModSync.GUI/Controls/WizardHostControl.axaml.cs:124-217`.
checked: `nl -ba src/ModSync.GUI/Controls/WizardHostControl.axaml.cs | sed -n '120,220p'` in the pinned C# checkout.
verdict: holds
confidence: high; only a different shipped entry point or later C# commit would change it.

claim: Rust has the route order, titles, subtitles, gates, and dynamic widescreen insertion, but exact page/control composition is not complete.
referent: `src/wizard.rs:5-89`, `src/wizard.rs:129-225`, `src/ui.rs:642-769`; C# page XAML listed above.
checked: line-numbered source comparison; no build or runtime probe.
verdict: holds
confidence: high for source shape, medium for rendered behavior until native comparison.

claim: Installing is covered by install-progress; Base/Widescreen/Finished completion surfaces by completion-pages; base Mod Selection by mod-selection; the remaining F24/F25 surfaces are not owned by those approved plans.
referent: `docs/parity/install-progress-plan.md:155-200`, `docs/parity/completion-pages-plan.md:84-118`, `docs/parity/mod-selection-plan.md:199-258`, plus the coverage table above.
checked: plan heading/ownership/file-scope audit against current Rust page arms.
verdict: holds
confidence: high; a newer plan that explicitly claims one of the unplanned surfaces would change it.

claim: Neither C# source nor Rust source establishes keyboard or screen-reader parity for F24/F25.
referent: `docs/parity/GUI-INVENTORY.md:127`, `docs/parity/GUI-INVENTORY.md:168-174`; declaration matrix above.
checked: `rg -n 'AutomationProperties|AccessKey|HotKey|KeyBinding'` across the audited C# wizard/main-window XAML and code, plus direct Rust UI source inspection.
verdict: holds
confidence: high for missing declarations and proof; native accessibility inspection could establish runtime behavior without changing the source finding.

claim: C# has richer Markdown and spoiler-specific presentation than the current Rust UI.
referent: `src/ModSync.GUI/Dialogs/WizardPages/PreamblePage.axaml.cs:42-54`, `src/ModSync.GUI/Controls/SummaryTab.axaml:57-75`, `src/ModSync.GUI/Controls/SummaryTab.axaml:214-418`, `src/ModSync.GUI/Controls/SummaryTab.axaml.cs:95-189`; `src/ui.rs:705-712`, `src/ui.rs:1240-1331`.
checked: direct source comparison.
verdict: holds
confidence: high; implementing a shared rich-content renderer and gated spoiler URL surface would change it.
