# Pinned C# Mod Selection oracle and Rust comparison

2026-09-27 source audit. This was read-only: no builds, tests, native launches, product edits or commits.

- **Oracle:** `/home/brunner56/.codex/worktrees/5813/ModSync` at HEAD `d31ccbd3bfcddb4705917eafa6483715c166de3b`. C# paths below are relative to that checkout.
- **Rust:** `/home/brunner56/Workspaces/ModSync.rs` at HEAD `dcd1373`.
- **Rust line numbers** refer to the committed blobs of `dcd1373` listed below (verified with `git rev-parse dcd1373:src/<file>`). Read them with `git show dcd1373:src/ui.rs`. HEAD has since moved (`f598129`), and the working-tree `ui.rs` is shifted about +14 lines, so do not re-anchor against it.

| File | Blob |
|---|---|
| `src/ui.rs` | `d890194cf9662157875c2eb669e741a8922d31b1` |
| `src/selection.rs` | `985ba3084b02e5aeef216556e532b1177fe8cfdd` |
| `src/main.rs` | `e228ec2aedb31be8a3bc3910c582a1b371e87456` |
| `src/wizard.rs` | `1dbd29e58c4f5c9f010588c2032947b2953412a2` |
| `src/game.rs` | `6ef353828a3366e5a34123cc1006d7d68b20c19c` |
| `src/download_plan.rs` | `dd00a2bb666d4473c8887ffba028b10b9596ef0f` |

The source observations below are not native acceptance evidence.

## Where the page sits

- The active host builds `new ModSelectionPage(_allComponents, _parentWindow as MainWindow)` as step 7 (`WizardHostControl.axaml.cs:161`). It runs inside the 1280×800 main window, minimum 600×400 (`MainWindow.axaml:15-18`).
- The legacy modal constructs it without a parent window (`InstallWizardDialog.axaml.cs:107`). That path has no spoiler-free sync.
- Title is `Mod Selection` and subtitle is `Choose the mods you want to install` (`ModSelectionPage.axaml.cs:97-99`). Rust matches both (`wizard.rs:33`, `wizard.rs:54`).
- The page only mutates `ModComponent.IsSelected` / `Option.IsSelected` on the shared `_allComponents` list. The `IsSelected` setter only raises `PropertyChanged` (`ModComponent.cs:637-649`).
- In the wizard, nothing subscribes to that change to cascade dependencies. `grep "nameof(ModComponent.IsSelected)"` finds only serializer reads.
- **The page's own code never cascades.** It has no dependency auto-select, no restriction deselect, no parent auto-select from an option, and no Aspyr gating. Those rules live in the editor's `ComponentSelectionService` (`ComponentSelectionService.cs:68-157`, `159-203`, `253-347`), reached from `MainWindow.OnCheckBoxChanged` (`MainWindow.axaml.cs:4987-5080`).
- **Live check required: a latent indirect path exists.**
  - The editor sidebar's `ModListItem` binds `IsChecked="{Binding IsSelected, Mode=TwoWay}"` (`ModListItem.axaml:54`, `:246`). Its `IsCheckedChanged` handler forwards to `MainWindow.OnComponentCheckBoxChanged` (`ModListItem.axaml.cs:63`, `:217-232`; `MainWindow.axaml.cs:5132`), which cascades and can show an Aspyr dialog.
  - During the wizard, that sidebar is hidden but not removed: `MainGrid`/`ModListSidebar` use `IsVisible="{Binding MainContentVisible}"`, and the wizard is a sibling (`MainWindow.axaml:222-244`, `:211-214`).
  - If those items were realized before the wizard opened, a wizard toggle may still fire the editor cascade through the binding. Whether they are realized depends on the runtime path, for example loading in the editor first or ItemsControl realization of hidden content. Source alone cannot settle it.
  - **Live result (`docs/parity/mod-selection-q9-oracle/`): it depends on the path.** Run A realized sidebar items first and cascades dependencies on tick and untick, plus the option-level restriction Standard/Revised (`run-a/app-trimmed.log` original lines 20284-20305, 20911). Run B went straight to the wizard and never cascades (zero `OnCheckBoxChanged`, `run-b/09`, `run-b/14`). Component-level restrictions are untested; the corpus has none.
- `Option` is a sealed `ModComponent` (`Option.cs:7`). Options reuse the component display and tooltip helpers.

## Exact page composition

`ModSelectionPage.axaml:9-186` is a root `Grid RowDefinitions="Auto,Auto,*,Auto"`. It has no responsive rule.

| Region | Source | Controls, measurements, copy |
|---|---|---|
| Header card | axaml 11-57 | Border row 0, margin 0,0,0,12, padding 16,12, radius 8. Grid `Auto,*,Auto`.<br>Left stack spacing 4: `SelectionCountText` 18 Bold `0 of 0 mods selected`; `SelectionDetailsText` 12, opacity .7, `Select the mods you want to install`.<br>Right horizontal stack spacing 8, vertically centred, four buttons with padding 12,6:<br>• `✓ Select All` (tip `Select all visible mods`)<br>• `✗ Deselect All` (`Deselect all mods`)<br>• `⭐ Select by Tier` (`Select mods by tier with cascading selection`)<br>• `🏷️ Select by Category` (`Select mods by category`) |
| Filter toolbar | axaml 59-119 | Border row 1, same margin, padding and radius. Grid columns `*,12,Auto,12,Auto,12,Auto,12,Auto`, rows `Auto,8,Auto`.<br>• `SearchTextBox`: watermark `🔍 Search mods by name or description...`, tip `Type to filter mods by name or description`.<br>• `CategoryFilterComboBox`: MinWidth 160, placeholder/first item `All Categories`, tip `Filter by mod category`.<br>• `TierFilterComboBox`: MinWidth 140, `All Tiers`, tip `Filter by mod quality tier`.<br>• `SpoilerFreeToggle`: ToggleSwitch `Spoiler-Free`, tip `Hide spoiler content in mod names and descriptions`.<br>• `ExpandCollapseAllButton`: padding 8,6, `▼ Expand All` / `▲ Collapse All`, tip `Expand or collapse all category sections`.<br>• Row 2 spans all columns: `FilterSummaryText` 12, opacity .6. |
| List | axaml 121-132 | Border row 2, padding 8, radius 8. `ScrollViewer` has horizontal disabled and vertical auto. It holds `StackPanel ModListPanel` with spacing 0. |
| Footer | axaml 134-185 | Border row 3, margin 0,12,0,0, padding 16,12, radius 8. Grid `Auto,16,Auto,16,Auto,*,Auto`.<br>Three stacks with spacing 2: label 11 opacity .6 (`SELECTED`, `REQUIRED`, `OPTIONAL`) over a value 16 Bold.<br>`ClearFiltersButton` sits in column 6: padding 12,6, `Clear Filters`, `IsVisible=False`, tip `Reset all filters and search`. |

### Category expander (built in code, `ModSelectionPage.axaml.cs:285-386`)

- **Grouping.** Groups use the first category, or `Other` if there is none. Groups are ordered ordinally. Mods inside a group are ordered by `Name` ordinally (`:281-283`, `:377`). This affects display only. The install order stays in the guide.
- **Expander.** Margin 0,4, stretch alignment. Initial `IsExpanded` is the previous state for that category if known, otherwise `_allExpanded`. `_allExpanded` starts false, so every group starts collapsed (`:292-298`, `:49`).
- **Header grid** is `Auto,*,Auto,Auto` (`:301-367`):
  - the category name, 16 Bold;
  - `({n} mods)`, 13, opacity .7, left margin 8;
  - only when `selected > 0`, a badge with padding 6,2, radius 4, margin-left 8, text `✓ {n} selected` at 11 SemiBold;
  - a group button with padding 8,4, margin-left 8, FontSize 10:
    - when all are selected: `✗ Deselect All`, tip `Deselect all mods in this category`;
    - otherwise: `✓ Select All`, tip `Select all mods in this category`.
- **Content** is a StackPanel with spacing 4 of mod cards (`:372-382`).

### Empty state (`:388-417`)

- Border padding 24, centred.
- `🔍` at 32.
- `No mods match your filters` at 16.
- `Try adjusting your search or filter criteria` at 13, opacity .7.

### Mod card (`CreateModCard`, `:420-635`)

- **Border.** Padding 14,12, margin 8,4, border thickness 1, radius 8, hand cursor.
- **Hover.** Background `ModListItem.HoverBackgroundBrush`, border `ModListItem.HoverDefaultBrush` (fallback `#A8B348`, `ThemeResourceHelper.cs:63-65`). On exit the background returns to `DefaultBackgroundBrush` and the border to null (`:433-449`).
- **Grid.** Columns `Auto,12,*,Auto`. Rows are name, badges, a 4px spacer, description, plus an options row added dynamically (`:451-458`, `:621`).
- **CheckBox.** Column 0, top-aligned, RowSpan 5 (`:461-481`). The options panel spans columns 0-3 in row 4, so the checkbox column overlaps the options row.
- **Card click.**
  - Clicking anywhere except a CheckBox toggles `IsSelected`, sets the checkbox and calls `UpdateSelectionCount` (`:484-498`).
  - Clicks on an option card are handled by the option and do not reach the card (`:744-761`).
- **Name.** `GetDisplayName`, 15 SemiBold, wrapping (`:501-512`).
- **Badges row.** Grid `*,Auto`, top margin 4, spanning columns 2-3 (`:516-584`).
  - Each category badge: padding 6,2, right margin 4, radius 6, classes `mod-list-item-badge category-badge`. Text is 10 Medium with character-ellipsis trimming and no wrap. Tooltip is `GetCategoryDescription`.
  - The single tier badge sits on the right with the same metrics, class `tier-badge`, and tooltip `GetTierDescription`.
  - Light colours: category `#E8F0F8` / `#2D5A7F`; tier `#F0E8F0` / `#6B4C6B` (`LightStyle.axaml:434-448`). The Kotor styles differ (`KotorStyle.axaml:422+`).
- **Description.** `GetSimplifiedDescription`, 13, opacity .75, wrapping, row 3, spanning columns 2-3 (`:587-601`).
- **Options.** A StackPanel with top margin 8 and spacing 4, holding one option card per `component.Options` (`:604-623`). Only direct options are shown; there is no recursion.
- **Card tooltip.** `CreateModTooltip` is set on the card border (`:628-632`).

### Option card (`CreateOptionCard`, `:637-764`)

- **Border.** Margin 0,2, padding 8,4, border thickness 1, radius 3, hand cursor. Grid `Auto,*`.
- **CheckBox.** Right margin 8, vertically centred.
- **Text.** Name at 12 Medium, wrapping. Description at 11, opacity .8, wrapping.
- **Tooltip.** `CreateModTooltip(option)` on the border.
- **Background.** The hover brush while selected, otherwise transparent. Hover shows the hover brush and exit restores it.
- **Click.**
  - A click on the card toggles `opt.IsSelected` only.
  - It does not select the parent, cascade, or refresh counts.

### Tooltip text (`CreateModTooltip`, `:1033-1126`)

Parts are joined with `\n\n`:

1. `📦 {name}`
2. `👤 Author: …`
3. `🏷️ Categories:\n• {cat}: {desc}` (one line per category)
4. `⭐ Tier: {tier} - {desc}`
5. `📝 Description: …`
   - Normal mode uses the full description.
   - Spoiler-free mode uses the spoiler-free description, or a generated one.
6. `⚙️ Installation: {n} installation step(s), {n} customization option(s), uses {method}`
7. `🌐 Language support: …`
8. `🔗 {n} download source(s) available`

Rust `selection::tooltip` matches this structure (`selection.rs:591-652`).

### Display helpers

- **`GetDisplayName`** (`:955-976`).
  - Spoiler-free mode: `NameSpoilerFree`, else `SpoilerFreeContentConverter.GenerateAutoName`, which builds an acronym plus `(cat/cat, Tier t, by a)` (`SpoilerFreeContentConverter.axaml.cs:144-211`, acronym `:222-292`).
  - Otherwise: `Name ?? "Unnamed Mod"`.
  - Rust `display_name` reproduces the acronym, skip words, 8-character cap and metadata suffix (`selection.rs:429-483`).
- **`GetSimplifiedDescription`** (`:982-1008`).
  - Spoiler-free mode: `DescriptionSpoilerFree`, else `This {cats} modification enhances your gameplay.` or `This modification enhances your gameplay.` (`:1013-1028`).
  - Otherwise: the description truncated to 197 UTF-16 units plus `...` when longer than 200.
  - Rust matches, using UTF-16 units (`selection.rs:485-505`).

## Toolbar behavior and dialogs

| Control | C# behavior | Rust today |
|---|---|---|
| Search | `TextChanged` sets the text and calls `RebuildModList`. Matching is a case-insensitive substring of the **displayed** name or simplified description, so it honours spoiler-free mode (`:158-165`, `:783-793`). | `selection::matches_search` is equivalent (`selection.rs:507-515`). The hint text differs: `Search displayed name or description` (`ui.rs:919`). |
| Category filter | Items are `All Categories` followed by the distinct ordinal-sorted categories of **non-widescreen** mods (`:224-240`). Matching is `Contains(…, OrdinalIgnoreCase)` (`:771-774`). | The combo collects categories from **all** components, including widescreen (`ui.rs:962-966`). The label is `All categories` (lower case). |
| Tier filter | Items are `All Tiers` followed by the distinct ordinal-sorted tiers of non-widescreen mods (`:243-259`). Matching is `Equals(OrdinalIgnoreCase)` (`:777-780`). | Tiers include widescreen (`ui.rs:956-961`). The label is `All tiers`. |
| Spoiler-Free | Two-way bound to `MainWindow.SpoilerFreeMode` (`:66-84`, `:185-203`). Toggling rebuilds the list. The value is stored in settings (`SettingsService.cs:30`). | A checkbox labelled `Spoiler-free names and descriptions` (`ui.rs:937`). It persists through the profile store and reverts if the save fails. |
| Expand/Collapse All | Flips `_allExpanded`, sets it on the current expanders and relabels the button (`:1128-1141`). | `Expand all` / `Collapse all` with a one-frame `.open()` (`ui.rs:1093-1105`). |
| Clear Filters | Resets search, category and tier. Leaves spoiler mode alone. **Hidden** unless a filter is active (`:825-847`, `:937-949`). | Always shown, **disabled** when no filter is active, and placed in the toolbar rather than the footer (`ui.rs:922-933`). |
| ✓ Select All | Sets `IsChecked=true` on every built checkbox, which is every filtered mod including collapsed groups. Then `UpdateSelectionCount` only (`:806-813`). **No rebuild and no cascade.** | `select_visible`, then `resolve_selected` (**cascades dependencies and restrictions**), then `enforce_selectable` (Aspyr) (`ui.rs:889-906`). |
| ✗ Deselect All | Sets `IsSelected=false` on every non-widescreen mod and rebuilds (`:815-823`). Widescreen mods are untouched. | `deselect_base` calls `set_component_selected(false)`. That **cascades to selected widescreen dependents** of base mods (`selection.rs:685-695`, `:81-102`). This diverges from C#. |
| ⭐ Select by Tier | Opens a modal `Window`. See the tier dialog below. | No modal. The tier filter combo plus an inline `Select through tier` button (`ui.rs:978-996`), then `resolve_selected` and `enforce_selectable`. |
| 🏷️ Select by Category | Opens a modal. See the category dialog below. | `Select multiple categories…` opens an egui Modal (`ui.rs:1060-1075`, `2994-3024`). It is additive and then calls `resolve_selected`. It does **not** call `enforce_selectable` (`ui.rs:1809-1827`). An extra inline `Select category` button also exists (`ui.rs:1016-1036`). |
| Group button | Sets `IsSelected` on every mod in the group without cascading, then `RebuildModList` (`:357-365`). | `Select group` / `Deselect group` calls `set_component_selected` per index, which cascades (`ui.rs:1120-1147`). |
| Card checkbox or card click | Sets `IsSelected` directly and calls `UpdateSelectionCount` only (`:468-475`, `:484-498`). | Checkbox only; the card is not clickable. It mutates `c.is_selected` and then calls `set_component_selected` (cascade) and `enforce_selectable` (`ui.rs:1165-1212`). |
| Option checkbox or option-card click | Sets `opt.IsSelected` only (`:662-672`, `:744-761`). | Top-level options go through `set_option_selected`, which auto-selects the parent and cascades (`ui.rs:1306-1314`). Nested options recurse through `Self::options`, but **the return value is discarded**. Nested toggles therefore bypass the shared rule and do not invalidate validation (`ui.rs:2510-2535`, `:2532`). |

### Select by Tier dialog (`ShowSelectByTierDialog`, `:1143-1226`)

- Returns silently when no non-widescreen tiers exist.
- **Window.** Title `Select Mods by Tier`, 400×300, centred on the owner.
- **Content.** StackPanel margin 16, spacing 12:
  - the prompt `Select a tier to select all mods in that tier and higher priority tiers:` (wrapping, bottom margin 8);
  - a ComboBox of the ordinal-sorted tiers, with index 0 selected;
  - a right-aligned button row with spacing 8: `Select` and `Cancel`, padding 12,6.
- **Result.** `Select` calls `SelectByTier` and closes the dialog. The dialog opens only if an ancestor window exists (`:1221-1225`).

### Select by Category dialog (`ShowSelectByCategoryDialog`, `:1264-1373`)

- Returns silently when no categories exist.
- **Window.** Title `Select Mods by Category`, 450×500.
- **Content.**
  - the prompt `Select one or more categories:`;
  - a ScrollViewer with MaxHeight 300 holding a StackPanel with spacing 4 of CheckBoxes (margin 0,2), one per non-widescreen category, ordinal-sorted;
  - the same `Select` / `Cancel` row.
- **Result.** `Select` with no ticks does nothing and leaves the dialog open. With ticks it additively sets `IsSelected=true` on any non-widescreen mod with a matching category (OrdinalIgnoreCase), rebuilds, and closes (`:1351-1358`, `:1375-1387`).
- **Rust differences** (`ui.rs:2994-3024`):
  - The heading `Select mods by category` is inside the modal, not the window title.
  - The prompt is missing.
  - The button reads `Select categories` instead of `Select`, and it is disabled rather than a no-op when nothing is ticked.

### Absent from the C# page

Each absence below was checked in source.

| Absent item | Evidence |
|---|---|
| Option-picker dialogs | Options are inline cards (`:604-623`). |
| Dependency or conflict prompts | The page's only `Window`s are the two dialogs above (`:1157`, `:1278`). |
| Update badge | No update reference in the file. |
| Widescreen marker | Widescreen mods are filtered out at `:768`. They are chosen later on `WidescreenModSelectionPage`: plain `CheckBox Content=mod.Name` rows, no cascade, always valid, with summary copy `No widescreen-only mods are in this build.` / `{t} widescreen mod(s) available. None selected — you can skip them and continue.` / `{s} of {t} widescreen mod(s) selected for installation.` (`WidescreenModSelectionPage.axaml:10`, `.axaml.cs:62-104`). |
| Aspyr gating | Editor only (`MainWindow.axaml.cs:5061`). |
| Author or required/optional label on the card | The author appears only in the tooltip and the spoiler-free auto-name. |

## Counts, summaries and page validation

- **`UpdateSelectionCount`** (`:849-892`) counts over non-widescreen mods:
  - `{s} of {t} mods selected`;
  - details: `Select the mods you want to install` when s=0, `All mods selected for installation` when s=t, otherwise `{t-s} mods not selected`;
  - badges: SELECTED = s; REQUIRED = selected mods whose `Dependencies` is non-empty; OPTIONAL = s − REQUIRED.
- **`UpdateFilterSummary`** (`:894-935`) shows `Showing {v} of {t} mods • Filters: …` or `Showing all {t} mods`.
  - The filter parts are `Search: "q"`, `Category: c`, `Tier: t` and `Spoiler-Free Mode`.
  - Spoiler mode counts as a filter for the summary but not for Clear Filters.
- **`ValidateAsync`** (`:108-117`) blocks forward navigation with `Please select at least one mod to install.` when the non-widescreen selected count is 0.
- **Rust counterparts:**
  - The summary text is shared (`selection.rs:697-747`, `:749-762`).
  - The UI prefixes both lines with `Base build: ` and joins count and detail with ` — ` (`ui.rs:1040`, `1051`).
  - It shows `Required: n` / `Optional: n` with tooltips but has **no SELECTED badge** (`ui.rs:1043-1049`).
  - The gate is equivalent (`ui.rs:395`), but the footer copy is the generic `Complete this page before continuing.` (`ui.rs:583`).

## Rust composition today

The native captures in `docs/parity/native-selection` (commit `253ba18`) and `native-groups` (`8ba677c`) predate the wizard route. They show the old tab shell (`selected.png`: `1 Load guide / 2 Choose mods / 3 Validate and install`). They do not show the current page. From source at `dcd1373`, the current Rust Mod Selection page (`ui.rs:717`, `surface(…, 1)`) is laid out as follows:

- **Top.** `Check Nexus updates` (not in C#).
- **Toolbar.** Select all / Deselect all / search / Clear filters on one row, then the spoiler checkbox. After that come the tier combo with `Select through tier` and the category combo with `Select category`.
- **Summaries.** Two `Base build:` summary lines and the Required/Optional labels, then `Select multiple categories…`.
- **Body.** A two-column layout:
  - left: CollapsingHeaders `{cat} ({n} mods)` preceded by a `Select group` / `Deselect group` button with a `{s} / {n} selected` label;
  - rows: checkbox, a selectable name label with the tooltip, and an `Update` label when `component_update_available`;
  - right: a **details pane** for `self.selected` with heading, `{author} · {tier}`, description, directions (hidden in spoiler mode), recursive options, download links, `{n} install steps` and DraftReview tools.
- **Missing C# elements.** Card borders, category and tier badges, inline descriptions and options per card, whole-card click, header `✓ n selected` badge, footer badge row and emoji labels.

## Inline rules in the Rust GUI that belong in `selection.rs`

| # | Location | Inline rule |
|---|---|---|
| R1 | `ui.rs:349-377` (`selected_widescreen`, `selected_base`), `1061-1074`, `1083-1092`, `1225-1230`, `352`/`364` | The `WidescreenOnly` predicate is copied five times. `selection::base` is private (`selection.rs:300-306`). |
| R2 | `ui.rs:956-966` | Tier and category option lists are collected without excluding widescreen. C# excludes it at `:227` and `:246`. |
| R3 | `ui.rs:1083-1092` | `category_groups` does not filter widescreen (`selection.rs:765-787`), so the UI strips it afterwards. |
| R4 | `ui.rs:1120-1147` | The group select/deselect decision (`selected < len`) and the per-index loop. |
| R5 | `ui.rs:1165-1212` | The checkbox mutates `is_selected` before the shared call. The shared call then re-derives the state. |
| R6 | `ui.rs:2510-2535` | Nested option toggles mutate `option.is_selected` directly and discard the change (`let _ = Self::options…`, `:2532`). **This bypasses `set_option_selected` and validation invalidation.** |
| R0 | `ui.rs:887` | `enforce_selectable` runs on **every draw** of the Mod Selection page and cascades dependents (`game.rs:149-158`). The CLI never enforces. With the same file selection on a legacy K2 install and zero clicks, the GUI drops Aspyr-only mods and their dependents while the CLI keeps them. Neither C# frontend enforces this outside the editor. |
| R7 | `ui.rs:889-906`, `978-996`, `1016-1036`, `1120-1147`, `1165-1212`, `1306-1314`, `1809-1827` | Every action hand-sequences `select_*`, then `resolve_selected`, then `enforce_selectable`, then invalidation. The sequence is inconsistent: `apply_category_selection` skips `enforce_selectable`, and the checkbox and option paths skip `confirm_install = false` (`ui.rs:1174-1179`, `1253-1259`). |
| R8 | `ui.rs:1197` | The update badge is decided in the UI. The call to `updates::component_update_available` is shared, which is fine; its placement is the only divergence. |
| R9 | `ui.rs:500-518` vs `1263-1277` | The GUI reads download-link metadata keys inconsistently: `Link` in one place and `ModLink` in the other. |

## CLI selection gaps

- `install`, `validate` and `conflicts` share `InstallArgs` (`main.rs:142-146`).
- `--select` calls `apply_cli_selection`. It resets the selection, applies exact name/GUID, category and tier filters with cumulative tiers, and prints warnings (`main.rs:983-991`, `selection.rs:343-393`).
- `--option` is an **inline recursive setter** (`main.rs:999-1016`). It sets only `o.is_selected = true`: no parent auto-select, no cascade, no `set_option_selected`.
- `--use-file-selection` and `--profile` apply stored flags. Neither `resolve_selected` nor `enforce_selectable` is ever called from `main.rs` (grep).

The CLI has no equivalent of the following GUI actions:

- deselect a component or option;
- additive select (every selector resets);
- search / select-visible;
- select a group;
- the wizard's select-by-tier rule;
- dependency or restriction resolution;
- Aspyr enforcement;
- spoiler-free display;
- a printed selection summary or badges.

The download commands do not take selection at all:

- `list` prints the raw guide (`main.rs:575-578`).
- `download-plan` takes one `--component` (`main.rs:180-190`).
- `download-batch` reads a JSON job list (`main.rs:223-233`).

## C# defects (documented, not parity targets)

| ID | Defect | Evidence |
|---|---|---|
| D1 | **Select-by-Tier never cascades.** It is not a no-op: it still selects the exact chosen tier additively among non-widescreen mods (`:1240-1247`). The priority keys are `"1","2","3","4","Optional"` (`:1231-1238`), but `Tier` is normalized on set to `1 - Essential` … `4 - Optional` (`ModComponent.cs:218-228`, `CategoryTierDefinitions.cs:113-159`). A bare numeric tier survives normalization unchanged, because the regex strips it to empty and the input is returned (`CategoryTierDefinitions.cs:131-159`), so only numeric tiers cascade. Every tier in the real corpus is a full label: `mod-builds/TOMLs` has 110 `1 - Essential`, 452 `2 - Recommended`, 266 `3 - Suggested` and 92 `4 - Optional`, 920 in total (grep in `/run/media/brunner56/MyBook/Workspaces/ModSync/mod-builds`). Every real guide therefore takes the exact-match branch (`:1240-1247`). This contradicts the dialog prompt (`:1173`) and the button tooltip (axaml 49). The C# CLI uses the correct normalized keys (`ModBuildConverter.cs:1424-1431`). Rust copied the wizard bug (`selection.rs:395-413`) and records it as intended in `GUI-CLI.md:10`. | source |
| D2 | **Welcome promises auto-select, but the wizard does not deliver it.** `WelcomePage.axaml:158` says `Browse by category or tier — dependencies auto-select` and `:37` says `✓ Automatic Dependencies`. No wizard selection path cascades directly. The live run shows the indirect sidebar binding does cascade when its items were realized first (Run A) and not otherwise (Run B), so auto-select depends on the path the user took. The C# CLI's `DependencyResolverService.ResolveDependencies` orders components only and never reads `IsSelected` (`ModBuildConverter.cs:830`; grep of `DependencyResolverService.cs` shows no `IsSelected`). | source |
| D3 | **Stale group headers.** Card, checkbox and Select All paths call only `UpdateSelectionCount` (`:468-475`, `:484-498`, `:806-813`), so `✓ n selected` and the group button label go stale. | source |
| D4 | **The group button can invert.** It captures `selectedCount` at build time (`:357-365`). After individual toggles it can do the opposite of its label. | source |
| D5 | **REQUIRED is mislabelled.** It counts selected mods that *declare* dependencies, not required mods (`:854`). Rust reproduces this and says so in its tooltip. | source |
| D6 | **Option clicks don't propagate.** They don't select the parent, and an option selected under an unselected mod is silently inert. | source |
| D7 | **Spoiler-free mode leaks through the tier dialog.** Its item text shows raw tiers, which is harmless. The category dialog shows raw category names; categories are not spoiler-protected, so this is by design. | not a defect |
| D8 | **Checkbox RowSpan 5 overlaps the options row** (`:480`, `:618-620`). | layout |
| D10 | **Stale display after an indirect cascade.** Even in Run A, the wizard count and checkboxes stay stale until the list is rebuilt: `run-a/15` shows 1 of 189 after the tick, and the rebuild (`run-a/16`) shows 2. | live, `docs/parity/mod-selection-q9-oracle/` |
| D11 | **Unrequested re-select.** Realizing a sidebar item whose option is still selected re-selects its parent with no click. KOTOR Dialogue Fixes returned after Deselect All (`run-a/24`, log after the sidebar-filter marker). | live, `docs/parity/mod-selection-q9-oracle/` |
| D9 | **Legacy modal constructor has no spoiler sync.** `InstallWizardDialog.axaml.cs:107` omits the parent window. | inactive path |

## Measurement and copy checklist for the renderer

- **Window.** 1280×800, minimum 600×400. All measurements are in the tables above.
- **Copy** that the Rust renderer must use verbatim:

| Group | Strings |
|---|---|
| Header buttons | `✓ Select All` · `✗ Deselect All` · `⭐ Select by Tier` · `🏷️ Select by Category` |
| Toolbar | `🔍 Search mods by name or description...` · `All Categories` · `All Tiers` · `Spoiler-Free` · `▼ Expand All` / `▲ Collapse All` |
| Footer | `SELECTED` · `REQUIRED` · `OPTIONAL` · `Clear Filters` |
| Group header | `({n} mods)` · `✓ {n} selected` · `✓ Select All` / `✗ Deselect All` |
| Empty state | `No mods match your filters` · `Try adjusting your search or filter criteria` |
| Dialogs | `Select Mods by Tier` · `Select Mods by Category` · `Select` · `Cancel` · the two prompts quoted above |
| Validation | `Please select at least one mod to install.` |

- All tooltips are quoted above.
- **Glyph limitation.** Emoji rendered as fallback boxes in the earlier native receipt (`native-selection/receipt.json` limitations). Add a font fallback before comparing captures.
