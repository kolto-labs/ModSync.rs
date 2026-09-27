# GUI and CLI parity acceptance

The target is the C# GUI's controls, layouts, text, conditional pages, interactions, and behavior, with GUI and CLI actions using one Rust implementation of each operation. Shared calls alone do not prove C# equivalence. Reference: C# commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`.

| Flow | Current shared implementation | Remaining evidence or correction |
|---|---|---|
| Base validation | `Engine::validate_report_with_options`; desktop selects full validation with dry run, CLI supplies flags | C# environment, dependency conflicts, archive validation, issue presentation; native comparison |
| Remaining-phase validation | `Engine::validate_report_remaining` enters the same report pipeline as base validation | Completed prefix survives every fresh stage engine; consumed-source regression verifies no replay or writes. Native remaining-phase rendering remains open |
| Installation | `phase::prepare` then `Engine::install` / `install_remaining`; CLI All/Base/Remaining, desktop All/Base/Retained Remaining | Equality begins after selection; CLI stable-input reconstruction differs from GUI retained choices; full-build output remains unproven |
| Selection | `selection::apply` (`src/selection.rs:1405`) owns one policy for both frontends: the single-item cascade, the bulk dependency closure with restriction warnings, the cumulative tier rule, and `Action::EnforceGame`. The CLI routes every selection flag through it (`227452d`..`384cca5`). The desktop still calls the older helpers: `ui.rs:916` runs `enforce_selectable` on every draw, and `ui.rs:1015` runs the exact-label `select_tier`. | See [Mod selection policy](#mod-selection-policy) below. CLI behaviour is probed in `mod-selection-probes/`. The GUI half waits for mod-selection Task 3, so GUI/CLI equality (probes 6 and 7 GUI side) is unverified. Exact name/GUID selection remains a Rust CLI extension. |
| Downloads | Shared download/provider modules with separate frontend orchestration | Queue, cancellation, progress, filename selection and host behavior through both entry points |
| Wizard presentation | `src/ui.rs`, currently egui | Exact C# page/control inventory and conditional routes, text, layout, keyboard/focus, accessibility, themes and native rendered comparisons |

Confirmed mismatch corrected: C# `ValidatePage.ValidateAsync` uses `ValidationPipelineOptions.WizardFull` (`FullValidation=true`, `DryRun=true`). Rust desktop previously omitted DryRun. Its worker now requests the shared full pipeline with dry run; a worker-level regression compares the entire result with the same engine entry point used by CLI and verifies no game writes.

This regression establishes shared Rust behavior only. Exact C# GUI equivalence remains unverified and incomplete. Native screenshots and driven interaction tests must accompany the control-by-control audit; passing state tests are insufficient.

Shared phase preparation and CLI phase dispatch are implemented and independently
reviewed through `3d106ef`. [Integration evidence](cli-phases-integration.md) records
the exact commands, report/error contracts, receipt verification, repeats/recovery,
normalized output archive and final-source checks (229 passing all-target tests; 199 passing
headless tests; one intentional ignored fixture in each). Whole-package final review
passed with nonblocking P3 notes; this bounded phase slice is approved. All validation
retains its existing pipeline-report failure timing.
Remaining conflicts now verify receipts in the shared engine before suffix analysis.

GUI/CLI All and Base preparation compare equal for identical post-selection guides;
stable Remaining preparation/reports also compare equal. GUI keeps successfully
installed effective Base definitions after guarded completion. CLI reconstructs
from current guide/choices: whole-payload external choices metadata can make even
suffix-only edits fail the completed-definition comparison. Reusing stable choices
is required when they affected Base. A fresh explicit Base can repeat ordinary Copy
operations; consumed Move inputs fail safely. Only explicit `--direct-markdown`
transformation is excluded from phased requests. The automated differential evidence
does not refresh the earlier native wizard captures or establish exact C# parity.

## Mod selection policy

Status on 2026-09-27, branch `selection-actions` at `384cca5`:

- The CLI half has landed (`92bd0ac`..`384cca5`) and was probed.
- The GUI half is **planned, not wired.** `src/ui.rs` is unchanged on this branch. It still calls `enforce_selectable` on every draw (`ui.rs:916`) and the exact-label `select_tier` (`ui.rs:1015`).
- Every "both frontends" rule below is a decision for mod-selection Task 3. Only the CLI side is verified.

Plan: [mod-selection-plan.md](mod-selection-plan.md). C# audit: [mod-selection-csharp-audit.md](mod-selection-csharp-audit.md). CLI evidence: [mod-selection-probes/](mod-selection-probes/README.md), with receipt sha256 `a601124f…a014`.

C# references are at `d31ccbd3`. Wizard paths are under `src/ModSync.GUI/Dialogs/WizardPages/`.

### Select by Tier is cumulative (deviation from the C# wizard)

- **Rust rule.** `SelectThroughTier` adds every base mod whose tier priority is at or above the chosen tier. The priority comes from `cli_tier_priority`, then a bare `1`–`4` fallback (`src/selection.rs:1019-1036`). The action is additive and never touches widescreen mods.
- **C# wizard.** `SelectByTier` looks the tier up in the keys `"1","2","3","4","Optional"` (`ModSelectionPage.axaml.cs:1231-1238`).
  - A normalized tier such as `2 - Recommended` misses those keys, so it takes the exact-match branch (`:1240-1247`).
  - The cumulative branch (`:1250-1256`) only runs for bare numeric tiers.
- **C# contradicts itself.**
  - The dialog prompt promises the cumulative rule: "…in that tier and higher priority tiers" (`ModSelectionPage.axaml.cs:1173`).
  - So does the button tooltip, "with cascading selection" (`ModSelectionPage.axaml:49`).
  - The C# CLI uses normalized keys and is cumulative (`src/ModSync.Core/CLI/ModBuildConverter.cs:1424-1431`).
- **Corpus.** Every tier in `mod-builds/TOMLs` at `336b36d` is a full label: 110 + 452 + 266 + 92 = 920 of 920. So every real guide takes the C# exact branch. This was checked by grepping `Tier = "…"`.
- **Evidence.**
  - CLI: `mod-selection-probes/out/p1a-tier2.stdout` gives tiers 1 and 2, including the bare `Tier = "2"` fixture mod `…0005`. `p11` and `p12` cover bare `2` and bare `Optional`.
  - Corpus: `out-corpus/summary.txt` shows k1-tier2 with 107 of 189 and k2-tier2 with 102 of 145, both equal to the oracle.
  - GUI: pending Task 3.

This replaces the earlier row that recorded the wizard's numeric-only rule as intended.

### Aspyr enforcement runs at a boundary (deviation from the C# wizard and CLI)

- **Rust rule.** `Action::EnforceGame` (`src/selection.rs:1536`, `enforce_game` `:1633`) removes selected Aspyr-exclusive mods and their dependents whenever the game is known and is not Aspyr.
  - An unknown game or no `--game-dir` only warns: `Game version unknown; left N Aspyr-exclusive mod(s) selected`.
  - The CLI runs it once, after every selection source (default-all, `--use-file-selection`, `--profile`) and every flag (`src/main.rs:486`).
  - The GUI is planned to run it on page entry and game-directory change, never per draw. It still runs per draw today (`ui.rs:916`).
- **C#.** Only the editor refuses an Aspyr-only checkbox (`src/ModSync.GUI/MainWindow.axaml.cs:5061-5075`). The wizard page has no Aspyr gating (audit, "Aspyr gating"). The C# CLI selects everything when no selectors are given (`ModBuildConverter.cs:1379-1385`) and never gates.
- **Evidence.**
  - CLI: `out/p4a-legacy-default.*` (legacy K2) and `out/p4b-k1-default.*` (K1) drop `…0008` and its dependent `…0009`.
  - `out/p4c-unknown-default.stderr` shows the unknown-game warning.
  - Legacy probe `l8` (`validate --json` on K1) now prints those two `auto-deselected` lines on stderr. Its stdout is unchanged.
  - GUI: pending Task 3.

### Bulk actions: dependency closure only (Rust policy, no C# source)

- **Rust rule.** `SelectVisible`, `SetGroup` (select), `SelectThroughTier`, `SelectCategories` and `ResetFromSelectors` add only the dependency closure (`close_dependencies`, `src/selection.rs:1265`).
  - They never deselect because of a restriction.
  - Each restriction conflict in the resulting set becomes one warning that names both GUIDs (`:1317`).
  - Validation then fails on that conflict.
- **C#.** No C# bulk path resolves anything. The wizard sets `IsSelected` directly (audit D2). The CLI resets by filter only (`ModBuildConverter.cs:1486-1493`). The editor's per-item cascade has no bulk counterpart.
- **Why.** Restriction resolution over a whole set depends on guide order: the last restricted mod silently wins. The closure-plus-warning rule is deterministic.
- **Evidence.**
  - CLI: `out/p7a-mutual.*` keeps both `…0006` and `…0007` and gives one `Restriction conflict` warning.
  - `out/p7b-default-quiet.*` shows that default-all prints no resolution output.
  - GUI: pending Task 3.

### The single-item cascade is a Rust rule (C# varies by path)

- **Rust rule.** `SetComponent`, `SetOption` and `SetWidescreen` apply the editor cascade:
  - dependencies are selected;
  - restriction targets and reverse restrictions are deselected;
  - dependents are deselected on uncheck;
  - checking an option selects its parent chain.
- **C#.** The wizard page never cascades itself.
  - The live Q9 oracle ([mod-selection-q9-oracle/](mod-selection-q9-oracle/README.md)) shows the C# wizard cascades only through editor-sidebar items that were realized before the wizard opened (Run A).
  - Going straight into the wizard gives no cascade in either direction. Run B has zero `OnCheckBoxChanged` lines, and `run-b/09` and `run-b/14` show the dependency left unticked and both restricted options ticked.
  - So there is no single C# rule to copy. The Rust rule matches the editor's `ComponentSelectionService` and the Welcome promise (`WelcomePage.axaml:37,158`).
- **Evidence.**
  - CLI: `out/p2-deselect-dep.*` shows dependents deselected on uncheck. `out/p3-nested-option.*` shows the parent chain selected from a nested option.
  - GUI: pending Task 3.

### `Outcome` auto lists are not subsets

`autoSelected` and `autoDeselected` list the nodes whose final value differs from what the action itself assigned (`src/selection.rs:1579-1592`). Nodes the action did not assign are listed if they changed. As a result:

- After `ResetFromSelectors`, a closure addition that was already selected before the reset is in `autoSelected`, but in neither `selected` nor `deselected`. See `out/p2-deselect-dep.stdout` and `out/p1b-closure.stdout`, `outcomes[0]`: Alpha Core `…0001`.
- Adapters invalidate on `selected` or `deselected` only (`Outcome::changed`). They must not treat the auto lists as change sets.

### Removing an Aspyr mod clears its options

Since `384cca5`, when enforcement removes a component it also unchecks that component's selected options. It then runs the option-uncheck rule to a fixpoint, so mods that depend on those options are deselected too (`enforce_game`, `src/selection.rs:1633-1656`).

Evidence: `out/p19-option-dep-legacy.*` removes `…0008`, its option `…0015`, and `…0016`, which depends on that option. The Aspyr control is `out/p19-option-dep-aspyr.*`. `p17` and `p18` cover an explicitly requested option under a refused mod.

### CLI surface

`validate`, `install` and `conflicts` (`InstallArgs`) and the new `selection` subcommand share these flags (`SelectionFlags`, `src/main.rs:366-385`):

| Flag | Action | Notes |
|---|---|---|
| `--select <name\|guid\|category:X\|tier:X>` | `ResetFromSelectors` | Existing flag. It now adds the dependency closure and reports additions on stderr. |
| `--option <guid>` | `SetOption{selected:true}` | Existing flag. It now selects the parent chain. |
| `--deselect <name\|guid>` | `SetComponent{selected:false}` | Dependents are deselected too. |
| `--deselect-option <guid>` | `SetOption{selected:false}` | |
| `--select-through-tier <tier>` | `SelectThroughTier` | Cumulative and additive. |
| `--add-category <name>` | `SelectCategories` | Additive and case-insensitive. |
| `--select-visible-search <text>` | `SelectVisible` | Matches shown name or description. |
| `--spoiler-free` | | Makes the search match spoiler-free text. |

- **Order.** `--select` runs first. Then `--option` and the new flags run in argv order, and `EnforceGame` runs last (`src/main.rs:411-505`). Order matters: `out/p9-argv-order-a` selects 4 mods and `-b` selects 3.
- **stderr.** Each outcome prints `Selection warning: …`, `Selection: auto-selected <name> (<guid>)` and `Selection: auto-deselected <name> (<guid>)`.
- **Conflicts.** `--direct-markdown` conflicts with every selection flag. Clap exits 2 (`out/p14-direct-md-conflict.stderr`).

`modsync selection -i GUIDE [--game-dir DIR] [--use-file-selection] [selection flags] [--json]` is read-only: it never downloads or installs. Text mode prints three lines, then one `name (guid)` line per selected node:

1. the count and detail;
2. `SELECTED n  REQUIRED n  OPTIONAL n`;
3. the filter summary.

With `--json` it prints:

```json
{
  "game": "Kotor2Aspyr",            // Debug name of game::Detected; null without --game-dir
  "counts": {"selected": 4, "total": 10, "required": 1, "optional": 3},  // base-only
  "filterSummary": "Showing all 10 mods",
  "selected": ["<guid>", "..."],     // selected components and options, guide order
  "outcomes": [                      // one per step: --select reset, each flag in argv order, then EnforceGame
    {"selected": [], "deselected": [], "autoSelected": [], "autoDeselected": [], "warnings": []}
  ],
  "downloadCandidates": {            // selection::download_candidates per Scope
    "base": [{"index": 0, "guid": "<guid>"}],
    "widescreen": [],
    "all": [{"index": 0, "guid": "<guid>"}]
  }
}
```

Default-all applies only when there is no `--select` and no `--use-file-selection`, and it is not resolved.
