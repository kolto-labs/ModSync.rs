# Mod Selection page and shared selection actions plan

Status: planned, not started. **Revised after independent review (CHANGES_REQUIRED, items 1-7).** Rust base is `dcd1373`. Every Rust `file:line` in this plan and the audit refers to the committed blobs of `dcd1373`, not the working tree: `src/ui.rs` `d890194`, `src/selection.rs` `985ba30`, `src/main.rs` `e228ec2`, `src/game.rs` `6ef3538`, `src/download_plan.rs` `dd00a2b`. Read them with `git show dcd1373:src/ui.rs`. HEAD has since moved (`f598129`), and the working-tree `ui.rs` is shifted about +14 lines and still changing and the pinned C# oracle is `d31ccbd3bfcddb4705917eafa6483715c166de3b`. The source oracle and every file:line referent are in [mod-selection-csharp-audit.md](mod-selection-csharp-audit.md). This slice advances the P1 "selection page controls" and "selection interactions" rows of `GUI-INVENTORY.md:80-81`, plus the Selection row of `GUI-CLI.md:10`. On its own it does not close full C# GUI parity.

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md` and all unrelated worktree state.
- The user has deferred creating, porting, expanding, modifying or running Rust tests of every kind. That covers `tests/selection.rs` and the inline tests in `src/ui.rs`.
  - Preserve them byte-for-byte.
  - Do not add a temporary Rust harness under another name.
  - Record test compile drift; do not fix it.
- Selection business rules live only in `src/selection.rs`. The GUI and CLI format results and hold presentation state. Neither decides which components or options become selected.
- Keep existing public `selection::*` functions as wrappers so the frozen tests keep compiling. Frozen callers:
  - `tests/selection.rs:4-486`: `set_component_selected`, `set_option_selected`, `select_tier`, `select_categories`, `select_visible`, `deselect_base`, `matches_filters`, `apply_cli_selection`, summaries, badges and groups;
  - `src/ui.rs:3747`;
  - `src/ui.rs:4123-4158` (the `category_selection` field, `apply_category_selection`).
- `src/ui.rs` belongs to install-progress Task 2b until that lands. `dcd1373` is its GUI commit; confirm approval before starting. The completion-pages slice also plans `ui.rs` wiring. Serialize all edits to `ui.rs` (see file ownership below).

## Prerequisite before Task 1 policy is fixed: live C# cascade check (was Q9)

- **Question.** Does the pinned C# wizard cascade dependencies in practice, through the hidden editor sidebar's `ModListItem` TwoWay `IsSelected` binding (audit, "Where the page sits")?
- **Owner.** A separate agent is running this live check.
- **Result: it depends on the path.** The C# wizard cascades only through `ModListItem`s in the hidden editor sidebar that were realized before the wizard opened. The evidence is in `docs/parity/mod-selection-q9-oracle/` (README, `receipt.json`, `run-a/`, `run-b/`).
  - C# binary: pinned `d31ccbd3`, `ModSync.dll` `fbd51647…`, `ModSync.Core.dll` `89d9b236…`. These match the validation-reference build (`receipt.json`).
  - Fixture: `mod-builds/TOMLs/KOTOR1_Full.toml`, 189 base mods, on a private Xvfb display.
  - **Run A** (sidebar items realized in the editor first, then the wizard):
    - Ticking Grenades and Mines HD auto-ticks its dependency High-Poly Grenades, and unticking cascades back. See `run-a/app-trimmed.log` original lines 20284, 20291, 20298 and 20305; the sender is the ModListItem.
    - The option restriction also fires: `Option 'Standard' conflicts with component 'Revised', deselecting it` (original line 20911; `run-a/31`).
    - Deselect All cleared 24 mods but logged exactly one `OnCheckBoxChanged`, for the one realized item (line 20277). That shows the trigger is realization, not the page.
  - **Run B** (straight into the wizard): no cascade either way. `run-b/09` shows 1 of 189 with the dependency unticked, and `run-b/14` shows Standard and Revised both ticked. The full log contains zero `OnCheckBoxChanged` and zero `ComponentSelectionService` lines (header of `run-b/app-trimmed.log`, full-log sha256 `292bbea8…`).
  - Not tested: component-level Restrictions. KOTOR1_Full has none; all 21 are option-level.
  - I checked the referents myself: the lines above exist in `run-a/app-trimmed.log`. The two grep hits in the Run B log are the header comment recording `grep -c = 0`.
- **Two more C# defects found in the live run** (audit D10, D11):
  - Even in Run A, the wizard's count and checkboxes stay stale until the list is rebuilt: `run-a/15` shows 1 of 189, `run-a/16` shows 2.
  - Realizing a sidebar item whose option is still selected re-selects its parent with no click (`run-a/24`).
- **Effect on decision 1 (resolved).** C# behaviour depends on how the user reached the wizard, so no fixed C# rule exists to copy. The single-item cascade therefore stays a deliberate Rust rule. It matches C# Run A (the editor's `ComponentSelectionService` semantics) and the Welcome promise, and it does not reproduce Run B's no-cascade path or the realization-dependent stale and re-select defects. The API shape is unchanged. Task 1 is unblocked on this point.

## Decisions this plan fixes

Each decision names its criterion and its evidence.

### 1. One selection policy for both frontends: resolve, then enforce

- **Criterion:** identical selector input gives the same selected set in the GUI and the CLI.
- **Current state:**
  - The Rust GUI cascades dependencies and restrictions on every action (`ui.rs:889-906`, `1120-1212`, `1306-1314`).
  - The Rust CLI never does: `main.rs` never calls `resolve_selected` (audit, "CLI selection gaps").
  - So one guide and selection can install different sets through the two frontends.
- **C# oracle:** the wizard and CLI never cascade directly (audit D2). Live evidence refines this: the C# wizard cascades only when hidden sidebar items were realized first, and otherwise does not (prerequisite section, `docs/parity/mod-selection-q9-oracle/`). C# does promise auto-select in its Welcome copy (`WelcomePage.axaml:37,158`), and its editor cascades (`ComponentSelectionService.cs:68-157`).
- **The resolve step is deterministic and never drops a mod because of guide order.**
  - The real guides carry restriction lists: 21 in `KOTOR1_Full.toml` and 25 in `KOTOR2_Full.toml` (grep `Restrictions = ["`).
  - Running `resolve_selected` over a select-all set (`selection.rs:288-298`, fresh `visited` per component) lets the last restricted mod in guide order silently win.
  - Therefore:
    - **Single-item actions** (`SetComponent`, `SetOption`, `SetWidescreen`) apply the full editor cascade: dependencies selected, restriction targets and reverse restrictions deselected, dependents deselected on uncheck.
    - **Bulk and reset actions** (`SelectVisible`, `SetGroup`, `SelectThroughTier`, `SelectCategories`, `ResetFromSelectors`) add only the dependency closure. Each restriction conflict inside the resulting set becomes a `warnings` entry naming both GUIDs. The validation pipeline, which already reports restriction conflicts, is left to fail it.
      - **This is Rust policy with no C# source.** No C# bulk path resolves anything (audit D2; `ModBuildConverter.cs:1486-1493`). The editor's per-item cascade (`ComponentSelectionService.cs:68-157`) has no bulk counterpart.
      - Task 4 records it in `GUI-CLI.md` as a Rust-only rule, chosen for determinism.
    - **CLI default-all, `--use-file-selection` and `--profile`** are not actions. They keep today's unmodified sets (`main.rs:993-997`). No dependency/restriction resolution runs until an explicit flag does. The one exception is `EnforceGame` (decision 2a): it runs once after these sources whenever `--game-dir` detects a known game, so on a legacy K2 install a full build drops the Aspyr-only mods and their dependents instead of keeping the C# set (`ModBuildConverter.cs:1379-1385`). This is a recorded deviation.
- **Decision:** every explicit shared action ends with this shared resolve step and then the enforce step. The CLI's explicit flags call the same step.
- **Why:**
  - It keeps the Welcome promise, which Rust already reproduces at `ui.rs:626`.
  - It keeps the Rust GUI's current behaviour.
  - It removes the GUI/CLI split.
- **Deliberate divergence from the C# wizard/CLI:** the CLI prints every component that resolution adds or removes, so the extra changes are visible. Open question Q1 gives the reverse option.

### 2. One tier rule: cumulative over normalized tiers

- **Evidence:**
  - The C# wizard's tier action is **not** a no-op. It selects the exact chosen tier additively, among non-widescreen mods only (`ModSelectionPage.axaml.cs:1240-1247`). Only its cumulative branch never fires, because the priority keys never match normalized tiers (audit D1). Every tier in the real corpus is a full label: 920 of 920 (110+452+266+92).
  - Its own dialog prompt and tooltip promise the cumulative rule.
  - The C# CLI implements that rule correctly (`ModBuildConverter.cs:1424-1431`).
- **Decision:** the GUI's `Select by Tier` uses one cumulative priority, shared with the CLI:
  - `cli_tier_priority` (`selection.rs:307-320`) first;
  - then a numeric fallback: a bare `1`–`4` maps to that priority, and a bare `Optional` maps to 4;
  - otherwise an exact match.
- **Why the fallback:** without it, a bare numeric tier would regress. Today it cascades in both C# and Rust (`CategoryTierDefinitions.cs:131-159` keeps `"1"`). The real corpus has none (920 of 920 are full labels), so the impact is low. The CLI gains the same fallback.
- **Consequences:**
  - This is a **deliberate deviation from the C# wizard**. Task 4 records it in `GUI-CLI.md:10` as a deviation, not as parity:
    - Rust selects the cumulative set where C# selects the exact tier.
    - The deviation fixes C# defect D1 and matches the C# dialog copy and the C# CLI.
    - It keeps C#'s additive, base-only behaviour: nothing is deselected, and widescreen mods are untouched.
  - It replaces the "numeric/`Optional` wizard priorities" policy recorded there.
  - `select_tier` stays as a frozen wrapper, because `tests/selection.rs:72-93` encodes the old rule.
  - The new page calls the new action only.
  - Update `GUI-CLI.md` in Task 4.

### 2a. Game enforcement is one named action at one boundary in both frontends

- **Finding (review, blocking).** At `dcd1373`, `ui.rs:887` calls `game::enforce_selectable(&mut g.components, detected_game)` on **every draw** of the Mod Selection page. It cascades dependents (`game.rs:149-158`). The CLI never calls it.
- **Worst case.** Take a legacy K2 install with the same file selection and zero clicks. The GUI silently drops Aspyr-only mods and their dependents; the CLI keeps them.
- **C# oracle.** Neither the C# wizard nor the C# CLI enforces this. Only the editor refuses the checkbox (`MainWindow.axaml.cs:5061-5075`).
- **Decision.** Add `Action::EnforceGame`. It removes every selected Aspyr-exclusive component and the dependents the shared uncheck rule reaches. Every removal is reported in `Outcome.auto_deselected`.
- **Where it runs:**
  - The GUI runs it on entering Mod Selection, when the game directory changes, and as step 3 of every `apply` action (idempotent). **It never runs per draw.** The Task 3 grep gate adds: no `enforce_selectable` call inside a render function.
  - The CLI runs it once, after every selection source (default-all, `--use-file-selection`, `--profile` and explicit flags) and before phase preparation, whenever `--game-dir` detects a known game.
  - `Unknown` is a no-op with a warning in both frontends.
- **Result.** Both frontends apply the same rule at the same logical boundary, and identical inputs give identical sets.
- **Deliberate deviation from the C# wizard and CLI.**
  - Criterion: an Aspyr-exclusive mod on a legacy K2 install is known to be incompatible (C#'s own editor dialog says so, `MainWindow.axaml.cs:5070-5074`).
  - Task 4 records this in `GUI-CLI.md`.
  - This replaces the per-draw call, and it replaces the old "explicit flags only" answer to Q8.

### 3. Additive and reset are two named actions, not two policies

- The GUI's bulk actions are additive, as in C# `:1245`, `:1255` and `:1381`. CLI `--select` resets, as in C# `ModBuildConverter.cs:1486-1493`.
- Both become explicit variants of one action type, not separate modules.

### 4. Widescreen stays out of the base page

- C# excludes widescreen from filters, groups, counts and bulk actions (`:227`, `:246`, `:768`, `:817`).
- `Action::DeselectBase` must not cascade into widescreen dependents. C# leaves them alone (`:817-820`).
- The frozen `deselect_base` wrapper keeps its current cascading behaviour (`selection.rs:685-695`) and is no longer called by the page or the CLI.
- One rule for all wrappers: **wrappers keep old behaviour; only new `Action`s carry the new rules.**
- Widescreen selection remains on its own page, with plain checkboxes and summary copy per the audit.

### 5. Selection to download interface

Agreed with `audit-download-orchestration` on 2026-09-27. It verified `DownloadOrchestrationService.cs:124-126`, `DownloadsExplainPage.axaml.cs:39` and `ModBuildConverter.cs:1120/1242/2101/2639-2642/3526-3542`. I rechecked `:3514-3538`.

```rust
// src/selection.rs — additive
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope { Base, Widescreen, All }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCandidate { pub index: usize, pub guid: String } // top-level, guide order

/// is_selected && !resource_registry.is_empty() && scope matches WidescreenOnly
pub fn download_candidates(components: &[Component], scope: Scope) -> Vec<DownloadCandidate>;
```

- **Ownership.** Selection owns only which top-level components qualify. The planner owns URLs and filenames: `plan(guide: &Guide, candidates: &[selection::DownloadCandidate], options: &PlanOptions) -> AcquisitionPlan` (`download-orchestration-plan.md:163`), which borrows `&guide.components[c.index]` and keeps `download_plan::{url_enabled, select_filenames}`.
- **Scopes in use.**
  - The GUI Downloads page and CLI downloads both use `Scope::All` over the **selected** set, matching the C# GUI at `:124-126`.
  - The DownloadsExplain count does **not** use `download_candidates`. C# counts `IsSelected && !WidescreenOnly` with no registry filter (`DownloadsExplainPage.axaml.cs:39`). Rust uses `counts(components).selected`, which is built on `is_base`. `Scope::Base` stays for base-phase acquisition only.
- **Non-target.** The C# CLI downloads every component with a URL, ignoring `IsSelected`. That also makes its `--best-effort` Nexus deselect ineffective (`:3502-3536`). This is recorded as a C# defect and is not copied.
- **Dependency.** The download-orchestration plan depends on `download_candidates` landing in Task 1.

## Additive shared API (`src/selection.rs`)

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter { pub search: String, pub tier: String, pub category: String, pub spoiler_free: bool }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    SetComponent { guid: String, selected: bool },
    SetOption { guid: String, selected: bool },          // any depth; parent chain auto-selected on check
    SelectVisible(Filter),                               // additive, base only
    DeselectBase,                                        // base only, no widescreen cascade
    SetGroup { category: String, filter: Filter, selected: bool }, // group = first category or "Other"
    SelectThroughTier(String),                           // cumulative, normalized (decision 2)
    SelectCategories(Vec<String>),                       // additive, OrdinalIgnoreCase
    ResetFromSelectors(Vec<String>),                     // CLI --select semantics, unchanged
    SetWidescreen { guid: String, selected: bool },
    EnforceGame,                                         // decision 2a; the only path that runs enforce_selectable
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub selected: Vec<String>,     // GUIDs newly selected (components and options)
    pub deselected: Vec<String>,
    pub auto_selected: Vec<String>,  // nodes whose final value differs from what the action assigned (added); NOT a subset of `selected`
    pub auto_deselected: Vec<String>,// same, removed by restrictions/dependents/game; NOT a subset of `deselected`
    pub warnings: Vec<String>,       // e.g. unknown selector, unresolvable cycle
}

pub fn apply(components: &mut [Component], game: Option<game::Detected>, action: Action) -> anyhow::Result<Outcome>;
```

Each `apply` call runs these steps in order:

1. Perform the action's direct mutation.
2. Run the shared cascade: `component_checked`, `option_checked` and friends.
3. Run the `EnforceGame` rule (decision 2a). It is idempotent, so it is safe after every action.
4. Diff the before and after snapshots into the `Outcome`.

Rules for callers:

- An adapter invalidates validation, conflicts and install confirmation **exactly when** `Outcome` records a selection change (`selected` or `deselected` is non-empty). A warnings-only outcome does not invalidate. This replaces the seven hand-sequenced copies (audit R7).
- **Game enforcement.** `apply` takes `Option<game::Detected>`:
  - `None` or `Unknown` means no Aspyr enforcement, plus a warning that the Aspyr-exclusive mods were left untouched, emitted once per boundary (page entry or CLI run), not on every `Outcome`. This keeps a `selection` run without `--game-dir` from dropping them.
  - A known game enforces the rule.
  - The GUI always passes the known game, because the GameDirectory gate requires one (`ui.rs:389-395`).
- The unknown-GUID error text matches the existing CLI (`Unknown component: …`, `Unknown option: …`).

Read-only projections, which are shared facts:

- `pub fn is_base(&Component) -> bool` makes the private `base` public. It removes the five inline copies (audit R1).
- `pub fn filter_options(components) -> (Vec<String> tiers, Vec<String> categories)` returns non-widescreen values in UTF-16 ordinal order (R2).
- `category_groups` gains a base-only variant (R3). The existing function stays unchanged for the frozen test.
- `pub fn counts(components) -> Counts { selected, total, required, optional }` returns base-only counts, with REQUIRED meaning "declares dependencies" per audit D5, kept and labelled truthfully.
- `summary_detail` and `filter_summary` return only the C# strings, without the `Base build:` prefix. The existing functions remain as wrappers.
- `download_candidates` (decision 5).

## Presentation-only desktop state

This state lives in a new `src/ui/mod_selection_page.rs`, following the `installing_page` precedent:

- search text, tier and category combo values, and spoiler preference (persistence remains in `profiles`);
- the `all_expanded` flag, per-category expansion, scroll offset, hover state and card highlight;
- whether the Tier or Category dialog is open, the dialog's tier choice and tick set;
- whether Clear Filters is visible, which is derived from the filter state.

The renderer never mutates `is_selected`. It builds an `Action`, calls `selection::apply`, and forwards a non-empty `Outcome` to `App` for invalidation. The renderer reproduces the audit's composition verbatim:

- header card, toolbar, scroll list, footer badges;
- expander header with the `✓ n selected` badge;
- cards with badges, inline description and inline options;
- whole-card click, both modal dialogs, empty state, all copy, tooltips and measurements.

Beyond C#, the renderer adds these fixes:

- Group headers refresh after every action, which fixes D3 and D4.
- Options at every depth route through `SetOption`, which fixes R6.

Rust-only extras need a decision:

- The Rust-only details pane, `Check Nexus updates`, the `Update` label, directions and DraftReview tools are not in the C# page.
- The default (Q3) is to move them into the card tooltip or a secondary expander instead of a second column. `Check Nexus updates` moves to the editor surface.

## CLI adapter

`src/main.rs` keeps ownership of parsing and printing. All changes are additive.

- `--select` → `Action::ResetFromSelectors`, followed by the shared resolve (decision 1).
- `--option` → `Action::SetOption{selected:true}`, replacing the inline setter at `main.rs:999-1016`.
- New repeatable flags on `InstallArgs` are applied in argv order after the selectors:
  - `--deselect <name|guid>`
  - `--deselect-option <guid>`
  - `--select-through-tier <tier>`
  - `--add-category <name>`
  - `--select-visible-search <q>`, with `--spoiler-free` for matching displayed text.
  - Final spellings must be reviewed against Clap conflicts and `--direct-markdown` (`main.rs:293`).
- The CLI applies `Action::EnforceGame` after every selection source, using `game::detect` on `--game-dir` (decision 2a).
- A new `selection` subcommand is read-only:
  - It loads the guide and applies the same flags.
  - It prints `counts`, `filter_summary` and the resolved GUID list.
  - With `--json` it prints the `Outcome` list.
  - It never downloads or installs. This is the probe surface for GUI/CLI equality.
- Download commands that need a set take the same selection flags and call `download_candidates`. The flag wiring is owned by the download-orchestration plan; this plan provides only the type.
- Existing warnings keep their wording and stderr routing.

## File ownership and task sequence

The tasks are serialized.

| Task | Owned files | Responsibility |
|---|---|---|
| 1 | `src/selection.rs`, `src/game.rs` only if `enforce_selectable` must return removed GUIDs | `Action`, `Outcome`, `apply`, projections, `download_candidates`, cumulative tier, widescreen-safe deselect. Keep the old functions as wrappers. |
| 2 | `src/main.rs` | CLI flags, `selection` subcommand, `--option` rerouted, resolve/enforce parity |
| 3 | `src/ui/mod_selection_page.rs` (new), then `src/ui.rs` wiring **after install-progress Task 2b and the completion-pages ui.rs task are approved** | Exact renderer, dialogs, presentation state. `ui.rs` keeps only routing, invalidation from `Outcome`, and the footer message `Please select at least one mod to install.` |
| 4 | `docs/parity/mod-selection-native/` (new), `GUI-CLI.md`, `GUI-INVENTORY.md`, `ACCEPTANCE.md` | Evidence and honest status. `GUI-CLI.md:10` is revised for decision 2. |

Workers do not edit outside their ownership without first reporting the exact expansion they need. Task 2 starts from the approved Task 1. Task 3 starts only after Task 2 source review. The download-orchestration slice may consume `download_candidates` once Task 1 is approved.

## Acceptance probes without Rust tests

- **Fixtures.** Use a temporary root outside the repository. Record fixture hashes before each run.
- **Guides.** Use one synthetic guide covering the edge cases listed below, plus `mod-builds/TOMLs/KOTOR1_Full.toml` read-only for the real tier and category distribution.
- **Edge cases the synthetic guide covers:**
  - dependency chains, restrictions and mutual restriction;
  - option→component and component→option dependencies;
  - nested options with duplicate names;
  - an Aspyr-exclusive mod;
  - a widescreen mod depending on a base mod;
  - full-label and numeric tiers;
  - multi-category, uncategorized and colon-named mods;
  - `NameSpoilerFree`, and a description longer than 200 UTF-16 units.

CLI probes (real binary, copied and hashed):

1. `selection --select tier:"2 - Recommended"` resolves the cumulative tiers 1 and 2 plus their dependency closure. The `Outcome.autoSelected` lists the closure.
2. `--select X --deselect Y`, where X depends on Y, deselects X through the dependents rule, and the auto-deselected list names X.
3. `--option` on a nested option under an unselected parent selects the parent chain. The old inline behaviour left the parent unselected.
4. `--game-dir` pointing at a non-Aspyr fixture removes the Aspyr mod, and the output says so.
5. `--add-category` is additive and case-insensitive. `--select category:` resets. Both behave the same as before except for the resolution suffix.
6. `download_candidates` Base, Widescreen and All over the resolved set match the GUI Downloads list for the same state (byte-compare JSON).
7. Mutual restriction A⇄B. `--select A --select B` keeps both selected and emits one warning that names both GUIDs; validation then fails on the conflict. In the GUI, checking A and then B leaves only B selected, with A in `autoDeselected`. Default-all with no flags leaves both selected and prints no resolution output.
8. Legacy `--select`/`--option` stderr warnings and existing `install --json` output are unchanged, apart from documented resolution lines.

Native GUI matrix (1280×800 and 600×400; Light, plus K1 and K2 samples; controlled font with emoji fallback):

- initial collapsed state, then Expand All and Collapse All;
- header counts, footer SELECTED/REQUIRED/OPTIONAL, and the Clear Filters show/hide cycle;
- search, category, tier and spoiler combinations, including the filter summary and empty state;
- Select All on a filtered view (collapsed groups included); Deselect All leaves the widescreen dependent selected;
- the group button after individual toggles (label and badge refresh, D3/D4 fixed);
- whole-card click versus checkbox click, and option-card click at depth two;
- the Tier dialog (full-label cumulative) and the Category dialog (empty Select is a no-op; multi-tick);
- a dependency cascade becomes visible immediately in other groups, and the Aspyr mod is disabled;
- tooltips on the card, category badge, tier badge and option;
- after each action, CLI `selection` with equivalent flags prints the identical GUID set. Record both outputs.
- Pinned C# captures of the same fixture record layout and copy. Record every deliberate divergence (decisions 1, 2 and 4; D3; D4; R6) beside them.

Every receipt records:

- Rust HEAD and dirty-state hash, and an explicit statement that `AGENTS.md` was preserved;
- source blob hashes and the copied executable SHA-256;
- C# commit and binary hashes;
- full argv, exit codes and stdout/stderr hashes;
- fixture and output manifests;
- display geometry, theme, font and DPI, and owned process IDs;
- the interaction transcript and screenshot hashes;
- a statement that no Rust tests or harnesses were created, changed or run.

## Review gates and frozen-test risk

**Task 1 specification review:**

- every `Action` is additive or reset exactly as documented;
- the cascade is identical for every action, and enforcement runs last;
- `Outcome` diffs are exact and include options;
- no widescreen cascade from `Action::DeselectBase` (the `deselect_base` wrapper is unchanged), and widescreen is excluded from every base projection;
- the cumulative tier rule is used, and old wrappers keep their old behaviour;
- `download_candidates` matches decision 5.

**Task 1 quality review:** GUID case-insensitivity, cycle termination (the `visited` sets), no allocation in the per-frame projections beyond the current helpers, and no UI types.

**Task 2 review:** Clap conflicts, argv-order semantics, unchanged legacy warnings and JSON, the resolution report on stderr, and no local predicates in `main.rs`.

**Task 3 review:**

- exact copy and measurements against the audit tables;
- no `is_selected` writes in UI code (grep gate: `is_selected\s*=` and `&mut .*is_selected` absent from `src/ui/mod_selection_page.rs` and the Mod Selection code in `src/ui.rs`);
- invalidation driven only by `Outcome`;
- dialog focus and Escape; small-window scrolling; accessibility labels.

**Task 4 final review:** a whole-package diff from `dcd1373`, all receipts, and doc claims. Findings go back to the owning task, and a scoped re-review closes each one.

**Frozen tests that may drift:**

- Wrapper semantics are kept, so `tests/selection.rs` should still compile and its encoded rules stay true for the wrappers.
- `tests/selection.rs:52-93` encodes the numeric-only wizard tier rule. The new page no longer uses it, but the wrapper still satisfies it.
- `src/ui.rs:4123-4158` constructs `App.category_selection` and calls `apply_category_selection`. If Task 3 moves that state into the page module, that test will stop compiling. Keep a forwarding field and method only if review finds them needed for production; otherwise record the drift at that exact location for later adaptation. Do not add shims solely for uncompiled stale tests.

## Open questions and defaults

| # | Question | Default |
|---|---|---|
| Q1 | Should the CLI cascade (decision 1), or should both frontends drop the cascade to mirror the C# wizard/CLI? | Cascade in both frontends, with a printed `Outcome`. The Welcome copy promises it, and the Rust GUI already does it. |
| Q2 | Should the GUI tier action be cumulative (decision 2), or copy the C# wizard (exact tier, additive; D1)? | Cumulative, additive, base-only. Record it in `GUI-CLI.md:10` as a deviation from the C# wizard that fixes D1. |
| Q3 | What happens to the Rust-only details pane, Nexus update button, Update label, directions and DraftReview on this page? | Remove the second column. Show the Update label as a small badge next to the tier badge (a marked Rust extension). Move the Nexus check and DraftReview to the editor surface. Directions stay out of the base page, as in C#. |
| Q4 | Should Aspyr enforcement stay on the wizard page, even though C# does it only in the editor? | Keep it. It is a safety rule, and it goes through `apply`. |
| Q5 | Should option-card clicks auto-select the parent (Rust) or only toggle the option (C# D6)? | Auto-select through `SetOption`. |
| Q6 | Should the CLI offer `--all-components` to reproduce the C# CLI's whole-guide download? | No (agreed with download-orchestration). |
| Q8 | Should both frontends enforce Aspyr exclusivity (decision 2a)? This deviates from the C# wizard and CLI. | Yes, for every selection source, once per boundary, via `Action::EnforceGame`, listing dropped GUIDs. `Unknown` never enforces. Alternative: remove the GUI's per-draw call and enforce nowhere, recorded as parity with C#. |
| Q9 | Resolved (prerequisite section): C# cascade depends on sidebar realization. | Keep the single-item cascade as a Rust rule. Record it in `GUI-CLI.md` as a fixed rule where C# varies by path. |
| Q7 | Should the legacy `Select category` and `Select through tier` inline buttons remain beside the C#-exact dialogs? | Remove them. The dialogs replace them. |
