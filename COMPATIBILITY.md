# Compatibility and proof boundaries

## Typed install progress and Installing page — 2026-09-27

Commits `065ffee` (shared contract), `57f198f`, `cd8f0b0` and `33de7f6` (CLI adapter and help text), `dcd1373`..`ac9cf3a`, `a24e96b` and `9df6c3b` (desktop page). Rulings, commit map and review findings are in [install-progress-integration.md](docs/parity/install-progress-integration.md). Evidence is in [install-progress-native/](docs/parity/install-progress-native/README.md).

**Library API (additive).** `Engine::install_observed(guide, emit, progress)` and `Engine::install_remaining_observed(guide, completed, emit, progress)` take a second observer, `FnMut(install_progress::Progress)`. `Engine::install` and `Engine::install_remaining` still exist and pass a no-op observer. The observer cannot return an error. A dropped receiver does not change execution. The legacy `Event { component, step, message }` callback and its serialization are unchanged.

**Progress schema.** `Progress` serializes in camelCase: `phase`, `topLevelComponentName`, `topLevelComponentGuid`, `componentIndex`, `componentTotal`, `ownerName`, `ownerGuid`, `ownerPath` (array of `OwnerFrame {name, guid, reachedFromInstruction}`), `instructionIndex`, `instructionTotal`, `operation`, `message`, `snapshotPending`. `phase` is one of `preparing`, `creatingSnapshot`, `installingComponent`, `runningInstruction`, `componentCompleted`, `recordingCompletion`, `completed`. Indices are one-based when an item is active and 0 otherwise. `componentTotal` counts only the ordered top-level components still to install; nested Choose work never changes it. `instructionTotal` is owner-local. There is no `rollingBack` phase and no error phase. `Progress` carries no schema version field.

**CLI flag.** `install --install-progress-json` writes one JSON object per line to stdout:

```json
{"type":"installProgress","progress":{...Progress...}}
```

- Only `install` accepts it. `validate` and `conflicts` reject it with `--install-progress-json is only supported by install`.
- Clap rejects it together with `--json` or `--validation-report` (exit 2, usage error). One stdout stream has one schema.
- Legacy `--json` is byte-identical to the pre-slice output. The Task 1 fix1 and Task 2a receipts both compare the same 639-byte, 10-line All stream (SHA256 `9ea418cf…`) with keys `component`, `message`, `step`.
- In typed mode, legacy Event text (`component / step: message`, which includes external-program output) goes to **stderr**, from `cd8f0b0`. Stdout stays pure NDJSON.
- Stdout write errors in typed mode are ignored on purpose (`let _ = …write_all`). A closed pipe means the consumer left, and the engine keeps its own safe cancellation and recovery contract. The install still runs to its normal end and exit code. The Task 2a `drop*` probes cover this.

**Success and failure contract (review finding #5).**

- Success: exit 0, and the last stdout line has `"phase":"completed"` with `snapshotPending:false`. `completed` is emitted only after the completion receipt is recorded and `recovery::finish` succeeds.
- Failure or cancellation after the stream started: nonzero exit, no `completed` line, and the real error text on stderr (`Error: …`). The last line is the last fact that really happened; it may be `componentCompleted` for an earlier component. No failure envelope is emitted.
- Preflight failure (lock, receipt check, scan, non-mutating preflight): zero stdout lines, nonzero exit, error on stderr, and no recovery snapshot.
- `33de7f6` updated the flag's `--help` text (`src/main.rs`) to state this contract. That is a source-only change; the `ac9cf3a` probe's `help.stdout` predates it.

**Desktop.** Base and Retained Remaining installs render one Installing page from these facts (`src/ui/installing_page.rs`). The percentage moves only on `componentCompleted`, stays at 99% or less until `completed`, and uses round-half-to-even like .NET `Math.Round`. The page shows one recovery snapshot row instead of C# checkpoint counts. Issue counts read "Not counted". Stop Install asks for confirmation and then cancels at the next safe boundary. Restore and Retry act on the roots captured when the run started. The deliberate differences from C# are listed in `docs/parity/GUI-INVENTORY.md`.

**Limits.** This does not close F10, F24 or F26, and it adds no F29 full-build proof. There is still one whole-run recovery snapshot, no per-component checkpoint and no automatic rollback. Review findings #2, #9 and #10 (`src/ui.rs` restore routing, stale Remaining dead end, Retry after a `Settings::new` failure) are fixed by `a24e96b` (#2, #10) and `9df6c3b` (#9); the scoped frontend-races re-review approved them. Frozen inline tests in `src/ui.rs` will hang on `finish_worker`; see the integration doc. No Rust tests were created, changed or run.

## Shared selection actions (CLI half) — 2026-09-27

Branch `selection-actions` at `384cca5` (`92bd0ac`..`384cca5`) adds `selection::apply` with `Action` and `Outcome` (`src/selection.rs:834-877`, `:1405`). The CLI routes every selection flag through it.

**Status:**
- CLI: landed and probed. Evidence is in [docs/parity/mod-selection-probes/](docs/parity/mod-selection-probes/README.md), with receipt sha256 `a601124f…a014`. 172 copied outputs re-hash equal to the receipt.
- GUI: planned only. `src/ui.rs` is unchanged on this branch. It still runs `enforce_selectable` per draw (`ui.rs:916`) and the old `select_tier` (`ui.rs:1015`). The GUI halves of probes 6 (download candidates) and 7 (mutual restriction) stay unverified until mod-selection Task 3.

The full rules, C# `file:line` references and CLI JSON shape are in [GUI-CLI.md, Mod selection policy](docs/parity/GUI-CLI.md#mod-selection-policy). In short:

- **Tier.** Select by Tier is cumulative over normalized tiers, with a bare `1`–`4` fallback. This is a decided deviation from the C# wizard, which selects the exact tier for every full-label tier (`ModSelectionPage.axaml.cs:1231-1247`). All 920 of 920 corpus tiers take that exact branch. C#'s own prompt (`:1173`) and tooltip (`ModSelectionPage.axaml:49`) promise the cumulative rule, and so does the C# CLI (`ModBuildConverter.cs:1424-1431`).
- **Aspyr.** `EnforceGame` runs once after every selection source whenever the game is known. This is a deviation from the C# wizard and CLI. Only the C# editor gates Aspyr-only mods (`src/ModSync.GUI/MainWindow.axaml.cs:5061-5075`).
  - Since `384cca5`, removing a mod also clears its options and the mods that depend on them (probe p19).
  - An unknown game only warns.
- **Bulk actions.** Bulk and reset actions add the dependency closure only. Restriction conflicts become warnings. This is a Rust policy with no C# source.
- **Single items.** Single-item actions use the full editor cascade. This is a Rust rule: the live Q9 oracle shows the C# wizard cascades only when editor-sidebar items were realized first ([docs/parity/mod-selection-q9-oracle/](docs/parity/mod-selection-q9-oracle/README.md)).
- **Auto lists.** `autoSelected` and `autoDeselected` can name GUIDs that are in neither `selected` nor `deselected` (probe p2, `outcomes[0]`).

**CLI behaviour changes** against `c28f4fc` (probe 8, `out-legacy/compare.txt`). 9 of 12 legacy runs are byte-identical. The other three changed:

- `validate --select "Beta Addon"` (l12) and `conflicts --select "Beta Addon"` (l7):
  - Before, they failed with `Beta Addon requires selected component …0001`.
  - Now `--select` adds the dependency closure and prints `Selection: auto-selected Alpha Core (…0001)`.
  - On the executable fixture (`out-legacy-exec`), both now succeed and print their normal stdout.
- `validate --json` on K1 (l8): stdout is unchanged. stderr gains two `Selection: auto-deselected` lines, because default-all now drops the Aspyr-only mod and its dependent on a known non-Aspyr game. The C# CLI keeps them (`ModBuildConverter.cs:1379-1385`).

**What this supersedes:**
- the wizard tier rule in the Scope paragraph below ("Wizard tier selection is additive and cumulative only for `1` through `4` and `Optional`");
- the "Tier/category selection checkpoint" statement that unknown tiers select exact matches;
- the Selection row's "explicit CLI components/options" (the CLI now resolves them).

These apply to the CLI now and to the GUI once Task 3 lands. No Rust tests were created, changed or run.
## Completion pages — 2026-09-27

Commit `12c5c68` (exact Base/Widescreen/Finished completion pages). Source and page contracts are in [completion-pages-plan.md](docs/parity/completion-pages-plan.md); the source-cited composition is in [completion-pages-csharp-audit.md](docs/parity/completion-pages-csharp-audit.md). Native review evidence is in [completion-pages-native/](docs/parity/completion-pages-native/RECEIPT.txt), reproduced from a full native matrix run at `/tmp/modsync-cpages-native`.

**Open-folder mechanism (recorded difference, follow-up only).** `Open Game Folder` and `Open Mod Folder` on the completion pages (`src/ui/completion_pages.rs:564`) call `ctx.open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())))`, which goes through the `webbrowser` crate — a browser-first API. C# uses `Process.Start(UseShellExecute=true)` (`BaseInstallCompletePage.axaml.cs:158,181`), which asks the desktop shell to open the folder directly. Native review confirmed the Rust path opens real Firefox windows on the test display instead of a file manager, and the path is not percent-encoded before being placed after `file://`. The feedback text is already honest about what happened (`"Asked the system to open the {label}: {path}"`, `src/ui/completion_pages.rs:565`), so no copy change was needed. This task does not change the mechanism; switching to a native "open in file manager" call (e.g. `xdg-open`/`open`/`explorer` dispatch, matching `Process.Start(UseShellExecute=true)` behavior) is a follow-up.

**Stat-card wrap at small window sizes (Q7 resolution, recorded with native evidence).** The Base Installation Complete stat row (`src/ui/completion_pages.rs:245-258`) wraps from 4 columns to 2×2 to a single column as available width shrinks, rather than staying at an exact 4-column layout that could clip. This is the chosen resolution to the plan's Q7 (default was "exact 4-column first, switch to 2×2 only if native capture shows clipping"); native capture at 600×400 showed the shared wizard chrome (title block, Downloads panel, footer nav, Activity dock) leaves only ~25-40px of vertical viewport for the page body on this page, matching the plan's recorded C# D12 difference ("expected to clip") — a shared-layout constraint, not specific to this module. Evidence: [completion-pages-native/RECEIPT.txt](docs/parity/completion-pages-native/RECEIPT.txt) (`probe-base2`, 600×400 section) and screenshots `base-install-complete-1280x800.png`, `base-install-complete-600x400-top.png`, `base-install-complete-600x400-stats.png` in the same directory, with sha256 values in `sha256sums.txt` there.

**Managed-deployment summary stays hidden.** C# shows a managed-deployment summary on the completion pages only when `ManagedDeploymentEnabled` is true, which is false by default. Rust has no managed install session in the engine install path — `src/deployment.rs` is used only by the CLI (`src/main.rs:452`) — so the completion pages never render one. This matches C#'s default-hidden behavior. Managed installation and its completion summary remain unimplemented in the Rust engine path; F18 remains partial.

**Copy Log line limit.** `Copy Log` copies the run-scoped installing-page log, which is capped at `LOG_LIMIT = 2000` lines (`src/ui/installing_page.rs:11,229`). Very long installs will have earlier log lines silently dropped from what gets copied. Recorded as a known limit, not fixed in this task.

**Recovery copy reuse (trivial fix, this task).** The Finished page's second "Next Steps" bullet (`src/ui/completion_pages.rs:502`) previously paraphrased the truthful recovery sentence instead of quoting it. It now reuses the `RECOVERY_TEXT` constant (`src/ui/completion_pages.rs:15`) verbatim, so the Recovery modal, the Base page's Recovery card and the Finished page all state the exact same sentence.

**Task 2b CLI surface integrated.** `completion-summary --game-dir PATH --source-dir PATH [--json]` now reads the same `recovery::CompletionSummary` as the GUI. The standalone query was approved at `cd2af69` and is included in this integration; its six-scenario CLI evidence is in `docs/parity/completion-summary-probe.log`. The earlier completion-page lane inspection predated that merge.

## Final archive diagnostic correction — 2026-09-27

Whole-package review found one P2 combined-path defect: known component archive errors lacked a severity marker, so the shared presentation displayed only an aggregate card and first-issue navigation could select the final generic failure. Scoped specification/quality review approves `099834c` and closes F1. That commit fixes the shared failed-outcome mapping with an idempotent `ERROR:` prefix, preserving component identity/detail. **Raw Report message values now include that prefix; serialized schema, stages, counters and warning behavior are unchanged.** This qualifies earlier transport-compatibility statements: schema stability does not imply unchanged diagnostic text.

Separate [focused archive-fix evidence](docs/parity/validation-archive-diagnostic-fix/README.md) records missing Extract **1/1/4**, corrupt Extract **3/0/3**, unchanged missing Copy **0/1/5**, native actionable cards, automatic/manual first-issue targeting and actual clipboard readback. Strict installs reject before snapshots/pending/game/mod content writes; expected lock files and empty state directories outside content roots are recorded, not hidden. The earlier47c3e67 native package retains its original binding. No Rust tests were created/adapted/run, and no full-parity or all-target compilation claim follows.

## Validation shared pipeline and dedicated page — 2026-09-27

The shared library now exposes pure report presentation and genuine typed stage progress without changing raw Report JSON or ordinary Event contracts. Shared full-report Conflicts checks declared restrictions/dependencies; ComponentValidation checks archive/destination availability; typed ordinary missing Copy/Move/Extract sources become component-scoped dry-run skip warnings. Missing Copy now measures0 errors/1 warning/5 passed, matching pinned native C#, while strict installation, ordinary validate, standalone file conflicts, FOMOD known-state failures and remaining receipt verification retain their guards. Shared corrections are approved through88b2dd9. Registry auto-extraction, Move remapping/fallback and broader resolver parity remain incomplete.

Desktop `src/ui/validation_page.rs` consumes these shared contracts through run/input-bound workers. Dedicated log/results, copy, first-issue, reset/Stop and theme controls are implemented through47c3e67; utilities remain reachable. The bounded final native matrix and source-bound package are now [archived](docs/parity/validation-native/README.md); the subsequent whole-package review finding is closed by the approved099834c correction above. Latest600×400 Stop/cancel proof is confirmed for47c3e67; older captures remain tied to556b165 or5548736. Exact C# GUI/clipboard/typography/accessibility parity is not established.

[Integration contracts and evidence boundaries](docs/parity/validation-page-integration.md) record truthful warning footer wording, raw JSON versus pure solution fallback, proactive Next gating and monochrome-font deviations. They also preserve the correction from historically inferred full2/0/4 to actually measured pre-fix full3/0/3 (partial2/0/1). The user has deferred all new/expanded/ported Rust tests, including obsolete inline worker fixture adaptations, until full parity. Existing tests remain; production-only build evidence is not an all-target/test compilation claim.

CLI `validate`, `conflicts`, and `install` now expose `--phase all|base|remaining`
(default `all`) through the same preparation API used by desktop workers. Remaining
conflicts verify completion roots, serialized definitions and game hashes in the
shared engine. All report compatibility is preserved. CLI Remaining requires stable
definition-equivalent guide/choices inputs; whole-payload external choices metadata
means even suffix-only edits may reject, while GUI retained history can preserve
the completed prefix. Explicit Base reruns use ordinary installer semantics, so a
Copy/Overwrite Base can repeat; consumed Move inputs reject. Explicit phased
`--direct-markdown` is unsupported; ordinary Markdown ingestion remains allowed.
Commands, output/error contracts, reviews and source-bound evidence are in
[cli-phases-integration.md](docs/parity/cli-phases-integration.md). Tasks 1–3 passed
review; whole-package final review passed with nonblocking P3 notes. This adds no
native GUI or full-build proof; the existing pre-fix native evidence retains its original scope.

Remaining-phase FOMOD configuration now uses shared choices application/merge and
prompt-status helpers that preserve completed component definitions and nested
options. The desktop editor and external choices file both feed the same remaining
operation; receipt verification is unchanged. Successful base workers retain their
exact effective definitions in the resident guide, including external choices
supplied before installation. Completion is rejected after a project reset, guide
change, or directory change; historical definitions are never reconstructed from a
subsequently edited choices file. Regressions configure an initially
unselected widescreen archive after a base Move consumes its source, then validate
and install only the remaining component. The native wizard receipt below predates
this fix and is evidence for its recorded source hashes only; the new FOMOD boundary
has automated worker/shared tests, not a newly driven native FOMOD session.

The desktop now uses the active C# embedded-host route: separate landing load,
Welcome, conditional notices, directory/selection/download/validation/review pages,
worker-driven base and remaining phases, and Finish returning to the loaded guide.
Shared full validation includes selected widescreen inputs; shared remaining
validation/install preserve the completed prefix without replaying consumed Move
sources. Failed installs block progression; input changes during a validation worker
discard its stale result. Native route evidence and deliberate safety/copy differences
are recorded in `docs/parity/wizard-native/README.md` and `GUI-INVENTORY.md`.
Exact rendering, automatic missing-mod downloads, rich markup, accessibility, and
the metadata-free JSON/TOML default-preamble distinction remain open. FOMOD input
stamps avoid per-frame payload reads; non-Unix timestamp-preserving edits remain
an invalidation limitation. This route slice does not establish full C# GUI parity.

GUI and CLI selection rules now live in the shared selection module, with their reference-specific tier behavior kept distinct. CLI filters reset selection, OR values within each group, AND category/tier groups, and normalize named/full tier labels through priority bands; numeric CLI tiers remain custom exact values. Wizard tier selection is additive and cumulative only for `1` through `4` and `Optional`; every other label is an exact case-insensitive match. Exact component names/GUIDs take precedence over selector syntax, including names such as `category:Foo`. Executable CLI regressions cover colon names, collision precedence, and parseable JSON stdout when malformed-selector warnings go to stderr. `docs/parity/GUI-INVENTORY.md` records the open exact-GUI requirements from source comparison; it does not establish native equivalence.

Remaining-phase desktop validation now returns the same structured full report as base validation. Each stage retains the verified completed-prefix context, and FOMOD inspection skips completed components. The consumed-base-source regression covers report success without replaying a destructive Move or mutating either tree or recovery snapshot. Native remaining-phase presentation and exact C# report semantics remain open.

GUI/CLI convergence and exact C# GUI acceptance are tracked in `docs/parity/GUI-CLI.md`. The desktop base validation worker now requests full validation plus dry run, matching the C# WizardFull preset through the same engine used by CLI. A worker regression compares complete reports and verifies no game writes; native visual and interaction equivalence remains open.

Dry-run aggregate counters now count individual errors (including Critical) or warnings, matching C# pipeline precedence: when a dry run contains errors, its warnings remain visible in the issues but do not increment the aggregate warning counter. Regression cases cover mixed severities, multiple warnings, and a clean run. This corrects counters only; diagnostic content equivalence remains incomplete.

The live validation oracle in `docs/parity/validation-pipeline-oracle/` compares both CLIs on one successful Copy fixture with read-only game/mod trees. Parsed JSON, exit status, and empty stderr match. Rust structured CLI reports now require `--full` for Environment/Conflicts/InstallOrder and support `--dry-run-only` to skip ComponentValidation. Desktop reports retain their full-stage default. Full-stage semantics, failing reports, warning counts, and the complete C# component archive validator remain partial; matching the successful fixture does not close those gaps.

Reference: the local ModSync C# source at `d31ccbd3bfcddb4705917eafa6483715c166de3b`. This is a Rust implementation of the instruction workflow, not a source-to-source conversion of the entire application.

| Area | Implemented behavior | Limits |
|---|---|---|
| Desktop | Native egui wizard, file/folder pickers, selection, options, search, validation, install, recovery, logs, report export | No visual or accessibility equivalence claim to Avalonia |
| CLI | List, convert (including stdin/stdout), validate, install, restore, download; JSON progress; explicit Linux `modsync://` desktop-handler registration | No MCP server, `nxm://` registration, or native Windows/macOS registration execution |
| Formats | Existing `thisMod` TOML; JSON arrays; YAML document streams; XML; format aliases/autodetection; fenced structured Markdown; default prose fields and YAML/TOML component comments | Prose export, legacy bullet metadata, custom regex profiles and complete parser error-recovery parity remain open |
| Metadata | Preserves additional component fields; TOML/YAML retain build metadata | Preserving a field does not implement its feature; XML typing of arbitrary metadata remains unverified |
| Selection | File selections, select-all when all components are unselected; explicit CLI components/options; desktop dependency/restriction and deselection cascades across components and options | Native interaction and full Unicode GUID-comparison parity remain open |
| Ordering | Stable dependency sort, restrictions, InstallBefore/After, instruction conditions | Missing references and cycles fail |
| Options | Choose resolves selected option GUIDs in declared option order | No automatic choice inference |
| File operations | Copy, Move, Rename, Delete, Extract, DelDuplicate, CleanList | Copy/Move flatten matched files into the target, as the C# instruction engine does |
| Archives | Streaming ZIP, 7z, RAR, TAR, GZip and 7z SFX; integrity checks; isolated extraction root | No encrypted/multipart archives; SFX support is limited to embedded 7z payloads; empty directories are not materialized |
| CleanList | C# comma-separated rows, mandatory prefix, substring and token matching | Source matching and final file sets still need full-build differential acceptance |
| Texture cleanup | Removes requested extension when another compatible extension shares its basename | This is not content-hash deduplication |
| Paths | Mod/game placeholders, legacy aliases, case-insensitive matching, wildcard sources | Rejects symlinks, device names, traversal, case collisions, overlapping roots, and special files |
| Validation | In-memory file index tracks changes across instruction boundaries; generated drafts require explicit review | Extracted content uses private temporary files outside game/mod trees; external program effects cannot be simulated |
| Patching | OdyPatcher subprocess, namespace index or named INI, exit-status checking, output capture | Resolves configured path, PATH, and application Resources candidates; no embedded patcher, Wine, or patch-byte equivalence claim |
| Recovery | Snapshot both roots, stop on first failure, block retry until verified restore | Full copies; restart whole guide after restore; not per-component resume or a power-loss atomic filesystem transaction |
| Downloads | HTTPS and BitTorrent magnet transfers, streamed integrity checks, SHA-256 content identity, no-clobber publish; source-ordered DeadlyStream/MEGA/Nexus/GameFront provider routing | No host scraping, account management, CAPTCHA interaction, MEGA decryption, or automatic torrent discovery |
| Profiles | Persistent profile store and CLI CRUD/capture/apply/activation; profile-backed CLI validation/install | Desktop manager, default storage discovery and full schema/platform parity remain incomplete |
| FOMOD | Archive discovery, XML/choices parsing, configured archive extraction and selected-copy installation; CLI and desktop selection flows | Full corpus, restart/recovery, and complete native interface parity remain open |
| Managed deployment | Staged-tree manifest deploy/uninstall/purge CLI with hardlink-or-copy materialization, hash-guarded restoration, preflight and rollback, profile-scoped roots, profile switch with optional purge, and patcher provenance snapshot/record | Normal engine actions are not yet redirected into per-component staging (design in `docs/parity/deployment-csharp-audit.md`); no GUI management integration |
| Merge | `merge EXISTING INCOMING OUTPUT` matches GUIDs, then normalized name/author values; it supports existing-order, field-preference and exclusion switches | No C# URL validation, domain fallback, complete per-field/resource conflict rules, full option merge, or GUI merge dialogs |
| Protocol links | `protocol-parse` parses `nxm://` and `modsync://` links into typed JSON; `protocol-fetch` resolves hosts, rejects local/private targets and every unsafe redirect hop, caps instruction downloads at 5 MiB, and atomically publishes a new destination; `protocol-open` fetches, loads and emits a `modsync://` guide from a temporary file; `protocol-resolve` resolves an `nxm://` game/mod handoff to exactly one top-level component registry entry; `nxm-download` resolves authorized Nexus mirrors and streams an archive through the shared HTTPS/integrity path; the desktop retains and downloads an `nxm://` handoff on its worker channel, then marks the matched registry file available, while a confirmed `modsync://` link loads through that same worker/status flow; Unix secondary launches forward either one bounded URL or an activation request to the primary UI through a per-user socket; `protocol-register-linux` writes and registers its desktop handler | No cancellation during a ModSync instruction fetch, Windows named-pipe handoff, Windows/macOS registration execution, or native desktop acceptance |
| Updates | `update-apply` records supplied versions; `update-check` reads the existing settings/API-key override sources, fetches Nexus versions with API headers, one 429 retry and rate-budget tracking, then atomically saves guide and result summary; the desktop can save/clear a masked API key, runs the shared check asynchronously and shows per-component update indicators; the shared client also parses file lists, MD5 matches and key validation | No desktop file/MD5/key-validation surface, no cancellation during a request, and no live-Nexus acceptance run |
| Widescreen, Aspyr, game-specific notices | Automatic KOTOR 1/KOTOR 2 Legacy/KOTOR 2 Aspyr detection, Aspyr `steamassets` content-root resolution, Aspyr-only selection gating, and phased widescreen desktop flow | No native-platform or full-build acceptance of the game-specific flows |
| Editor | Native guide editor for component metadata, dependencies/restrictions, component and instruction add/remove/reorder, executable instruction fields, and atomic multi-format save | No option editor, raw text replacement, URL/regex import, file browser, or full C# editor corpus acceptance |
| Telemetry | Default-off consent gate, nonserialized signing-secret HMAC helper, and authenticated structured-event transport with `telemetry-auth` request headers | No OTLP protobuf traces/metrics, diagnostics UI, persisted consent, or automatic UI/install event wiring |

Unknown instruction fields and actions fail rather than being ignored. Components with no executable instructions cannot be installed. External program permission is off by default.

Archive contents and copies are streamed. Set `--scratch-dir` to an existing folder outside both install trees and optionally set `--max-expanded-bytes` per archive; the desktop exposes the same settings. Extraction also checks available scratch space. A 513 MiB regression passed locally. RAR cancellation is checked between entries; other formats poll during streamed output. FOMOD archives are rejected before their content is published to the install tree because choice handling is still missing.

The full feature inventory and acceptance requirements are tracked in [docs/parity/ACCEPTANCE.md](docs/parity/ACCEPTANCE.md). Full C# parity remains incomplete.

## Validation evidence

The local compatibility probe loaded all eight existing TOML build files without parse errors:

| Guide | Components |
|---|---:|
| KOTOR1_Full | 189 |
| KOTOR1_Spoiler_Free | 109 |
| KOTOR1_Mobile_Full | 114 |
| KOTOR1_Mobile_Spoiler_Free | 63 |
| KOTOR2_Full | 145 |
| KOTOR2_Spoiler_Free | 99 |
| KOTOR2_Mobile_Full | 105 |
| KOTOR2_Mobile_Spoiler_Free | 99 |

Those counts establish parsing only. No full K1/K2 build, comparison against a manual reference install, game launch, or gameplay validation has been performed for this port.

Local Linux checks also exercised the native GUI in an isolated X11 session: load the sample guide, choose mods, validate, confirm installation, and verify the resulting TGA and removed TPC. Separate native OdyPatcher invocations through the Rust CLI installed `hello.txt` into isolated game folders using both a namespace index and a named INI, with zero patcher errors. The named-INI run preserved the original `changes.ini`. The test suite separately checks failure recovery using a controlled subprocess.

A direct HTTPS download of the RAR test fixture matched its expected SHA-256 and bytes. A second request with an incorrect hash failed without publishing the destination file.

### Streaming milestone

Local checks: 21 regular integration tests pass; the explicit 513 MiB GZip extraction and atomic-copy regression passes separately (18.23 seconds test time; 110,384 KiB maximum resident set for the measured cargo command). The temporary filesystem was tmpfs, so this measures process memory, not physical disk throughput or total system memory. New checks cover TAR/GZip CRC failures and links, expansion budgets, cleanup on cancellation, nested archive validation without install-tree writes, 7z SFX signatures crossing buffer boundaries, and C# CleanList matching. These fixtures do not establish full archive-corpus or full-build equivalence.

### Structured-format evidence

The five format tests exercise XML legacy roots/entities/collections, YAML documents and metadata, aliases/autodetection, CLI stdin/named paths, and saved C# exports. `tools/check_formats.py` also verified live C#↔Rust conversion for TOML/JSON/YAML/XML, including options and ordering constraints; see `docs/parity/format-check.json` for its exact scope. K1/K2 full guide component models survived JSON/YAML/XML round trips. These checks do not prove natural-language ingestion, complete resource migration, unknown metadata typing, or complete C# CLI parity. JSON/XML follow the C# formats' omission of root build metadata.

### Markdown import checkpoint

The shared loader imports `###` component sections, plain/bold fields and Docusaurus admonitions, linked names, categories/tiers, directions and warnings, plus explicit YAML/TOML `<!--<<ModSync>> ... -->` instructions/options. `## Mod List` limits component selection to that section, following the pinned reference. Exact master names resolve to GUIDs; unresolved master text is retained as `UnresolvedMasters` and blocks installation. Unannotated components use stable UUIDv8 identifiers derived from their section and position; C# generates random identifiers.

Two new regressions compare a captured C# Markdown conversion, execute its explicit instructions in temporary trees, and reject both instruction-free prose and unresolved dependencies before game/mod mutation. This checkpoint initially imported prose without drafting; the draft-service checkpoint below supersedes that limitation. Author-qualified master aliases, spoiler variants, special widescreen/Aspyr extraction, paste UI and full corpus field equivalence remain open.

The Rust parser loads the desktop K1 full/spoiler-free prose guides (186/180 components) and K2 full/spoiler-free guides (145/139). A live comparison with the pinned C# K1 full parser found the same 186 components; 15 of the 16 compared prose fields matched across all components. Nine `Screenshots` values differ because the reference pattern also matches that word within ordinary prose. This difference remains unresolved, not accepted as parity. The first C# attempt timed out at 120 seconds; the successful bounded retry took 198.76 seconds. See `docs/parity/markdown-check.json` for input/source hashes and mismatches. No corpus install or natural-language instruction generation was performed at that checkpoint.

### Draft-service checkpoint

`src/draft.rs` ports the reference's 56 ordered instruction patterns, sentence/conditional splitting, source/destination inference, exclusions, cleanlist handling, download recommendations and sandbox normalization. Existing authored instructions remain untouched. Desktop file-open and CLI install/validate automatically draft Markdown, while `convert --parse-directions` opts in for conversion. Draft diagnostics and the reference review warning persist in the component; the desktop displays steps and notes, and `convert --review-draft <GUID>` or the desktop review button records explicit acceptance. Gaps, conditional clauses or rejected drafts prevent acceptance. The engine still checks real paths and external-program permissions after review.

`tools/check_drafts.py` compares the pinned C# service's raw objects rather than relying on lossy C# JSON export. It assigns the input's prose fields directly because the legacy JSON loader drops some installation warnings. Across 19 focused cases and four desktop guide component sets (669 component records), instructions, option names/descriptions/selections, warnings, gaps and conditional notes matched with zero differences. The corpus inputs are parsed component JSON, so this proves the draft stage, not complete C# Markdown ingestion or installation. Source/input hashes and scope are in `docs/parity/draft-check.json`.

Some reference drafts are demonstrably incomplete interpretations: generic patterns can yield one-letter folder names, and numeric ranges can yield root placeholders. Matching them does not certify a correct install. Complete local archive-based generation, full direct-Markdown preflight rules (including 4GB/widescreen exclusions), editing unresolved drafts and full-build acceptance remain open. The Rust review gate enforces the draft service's never-auto-trust contract; C# direct-Markdown preflight currently counts flagged drafts as executable without that gate. This workflow difference must be reconciled before claiming full parity.

All 31 regular local integration tests pass with and without desktop features, alongside formatting and Clippy. Native X11 acceptance exercised automatic drafting, rejection before review, visible instructions/diagnostics, acceptance, validation, and an isolated install whose resulting bytes and source removal were verified. The preceding Markdown commit's Windows CI failure was an LF-only malformed-input test transform against a CRLF checkout; the test now normalizes line endings before mutation.

### Local archive-generation checkpoint

`convert --auto-generate-local --source-path <directory>` now uses a shared Rust service to infer instructions for fresh components with one resource link. It matches top-level ZIP/RAR/7z/EXE names using the reference's normalized-name scores and newest-file preference. Valid archives produce Extract, loose-file Move/Choose, changes.ini Patcher, namespaces.ini options, hybrid installation-method labels, and the special Remove Duplicate TGA/TPC instruction. Namespaces retain insertion order, including skipped indices. Generation stages content in scratch and leaves input archives untouched; failed generation leaves the component unchanged.

`tools/check_archive_generation.py` compares raw output from the pinned C# generator on 16 isolated ZIP fixtures. Instruction fields, ordered option details, success and installation-method labels match, with Choose identifiers normalized to names. This isolates archive inference from the C# JSON loader's URL normalization. See `docs/parity/archive-generation-check.json`. Separate Rust tests exercise exact/newest archive selection, an actual selected-folder install, validation without source/game writes, authored-data preservation, corruption, unsafe entry names, cancellation and expansion budgets.

This remains partial. Authored-option merging, insertion-first resource selection and EXE fallback are covered by the following checkpoints. URL/download-based generation, arbitrary existing-instruction deduplication, namespace encoding variants and desktop archive-generation controls remain open. Unlike C#, inference does not delete corrupt downloads. The matched reference output can omit a wrapper's extraction-directory prefix or produce an Extract-only patch-data component; matching those objects does not establish a correct patcher or full-build install. Only the isolated loose-file choice has end-to-end install proof in this checkpoint.

All 35 regular integration tests pass with and without desktop features, alongside formatting and Clippy. The 513 MiB regression was not rerun locally for this inference-only change; its latest completed native evidence is the preceding commit's three-platform CI run 35054774351.

### Authored archive options

Archive generation now reuses equivalent authored options and consolidates options with overlapping instructions. Matching follows the reference's action-specific destination/argument/overwrite checks, source wildcard comparisons and asymmetric source coverage. Consolidation preserves the first option's metadata and selection, appends missing instructions, and replaces removed option IDs in Choose instructions. Source filename fallback follows host path-separator rules, as the C# helper does; this is not a platform-independent path-matching contract.

The expanded live comparison matches all 20 fixtures, including authored identity reuse, partial-overlap consolidation, broad wildcard reuse and distinct overwrite policies. The probe assigns authored Source strings directly after C# deserialization to isolate merging from its JSON loader's separator rewriting. A new isolated install verifies that the original selected option and its GUID survive generation and install the expected texture. C# can emit repeated references to one broad option in a newly created Choose; Rust preserves that output, but this does not certify that such a guide installs correctly. All matching and generation edge cases, existing top-level instruction augmentation and full-build behavior remain unproven.

All 36 regular integration tests pass with and without desktop features, alongside formatting and Clippy. The preceding archive-generation commit passed native CI run 35055903581; that native evidence predates authored-option merging.

### Resource order and local matching

ResourceRegistry now retains insertion order instead of sorting URL keys. TOML parsing/serialization retains map order as well, and Markdown links are collected in their written order. JSON, TOML, YAML and XML round-trip tests check URL order explicitly for both components and nested options; map equality alone would miss this behavior.

Local generation now accepts multiple resource links and selects the first, following C#. The live reference comparison covers 22 fixtures, including the first-link preference and the absence of fallback when only a later link matches a downloaded archive. Those two cases invoke C#'s actual local-processing entry point with an isolated SourcePath. Full resource migration, host-specific URL normalization, cache behavior and all matching edge cases remain open. Native CI run 35056506242 passed for preceding commit `5791c7c9a43d52c0d4ea66bd081cf498c2670e78`; it predates this resource-order change.

Live C#↔Rust format checks also preserve top-level resource order in both directions across TOML/JSON/YAML/XML. The stronger check exposed C# YAML preprocessing flattening ResourceRegistry into an unreadable list; Rust now emits the same derived ModLinkFilenames compatibility map as the reference exporter. It preserves file-selection flags and retains unrelated legacy URLs. Loading discards redundant legacy entries for URLs already in the richer registry, following C# precedence, so Rust round trips do not accumulate duplicate metadata.

`resource-order-check.json` records both input and C# control URL lists: C# drops registries on options before the cross-read comparison, so nested-option resource preservation is proven only within Rust. Metadata values, legacy-resource migration and complete C# option serialization remain unproven; ordered keys are not proof of those behaviors.

All 38 regular integration tests pass with and without desktop features, alongside formatting and Clippy. New regressions explicitly compare resource order and check the YAML compatibility map's nullable selections and round-trip behavior.

### Executable inference checkpoint

An EXE that cannot be read as an archive now generates Execute with the reference's Executable Installer label. Cancellation remains an error, and the Remove Duplicate TGA/TPC special case still takes precedence. Readable EXE archives use normal archive analysis; a readable archive with no supported content does not become Execute. Inference never launches or deletes the input. The shared engine retains its external-program trust requirement, and validation does not execute the program even when trust is configured.

The expanded differential corpus has 28 cases: 27 match C# and one remains explicitly unresolved. For a ZIP renamed to `.exe`, C# forces its 7z reader, falls back to CLI listing, then emits only Extract because its subsequent root-file check returns false. Rust's signature-based reader emits Extract plus Move. `archive-generation-check.json` retains both complete outputs. The fixture annotation permits only that exact additional instruction; any other difference still fails the checker. This is recorded evidence of a parity gap, not an accepted equivalence. Real Windows installers, SFX variants and full installs remain unproven.

The preceding resource-order commit passed native CI run 35057519588; that evidence predates executable inference.

All 39 regular integration tests pass with and without desktop features, alongside formatting and Clippy. The added engine test proves rejection without process trust, validation without execution even when trust is configured, unchanged input bytes and cancellation without a generated step. These checks do not prove real installer execution or resolve the renamed-ZIP difference.

## Direct archive API augmentation checkpoint

The public `generate_from_archive` API now augments authored instructions, reusing equivalent steps and honoring existing Copy/Move folder coverage. Parent-folder coverage respects path boundaries; similarly named sibling folders remain separate. Existing Choose references and option identities are retained. The automatic local CLI path still skips components with top-level instructions, matching C#.

The corpus now contains 34 cases: 27 live CLI comparisons match, six direct-API C# outputs are checked by Rust integration tests, and the renamed-ZIP EXE difference above remains unresolved. An isolated installation test verifies repeated augmentation is idempotent and preserves an authored Copy destination in Modules, including source retention. This does not prove all merging paths or full-build parity. Native CI run 35058007498 passed for preceding commit `967f6c81cbc858cb30c3985160b7a3f2a4657758`.

Local validation for this checkpoint passed formatting, clippy with warnings denied, all-target tests and no-default-feature tests. Both test configurations passed 40 regular integration tests; the explicit 513 MiB regression was not rerun locally for this instruction-merging change.

## Legacy resource migration checkpoint

Structured guide loading migrates legacy ModLink and ModLinkFilenames sources into the ordered ResourceRegistry. Modern entries take precedence, followed by ModLink, then filename maps; URL equality is case-insensitive. Empty filename maps retain download links. Migrated resources carry normalized-URL SHA-1 ContentKey/MetadataHash values, nullable file selections, empty handler metadata and FileSize zero. Original URLs remain the download keys, including their fragments. SHA-1 here reproduces reference identity metadata; it is not a download integrity guarantee.

The live JSON-loader comparison covers 15 cases (12 URL normalization variants plus precedence, string-form links and lowercase legacy-field handling). Fourteen outputs match exactly. The remaining case matches all legacy migration fields but records a pre-existing modern Files-map difference: pinned C# drops that map, while Rust retains it. This exact gap is retained in `docs/parity/legacy-resources-check.json` and regression assertions. JSON legacy filenames follow the reference's first-character capitalization; TOML/YAML/XML retain their input filename casing. String-form ModLink follows the reference's comma/semicolon conversion rather than treating embedded newlines as separate links.

Rust round-trips migrated data through JSON/TOML/YAML/XML. An isolated flow verifies an empty legacy filename table reaches local archive inference, non-mutating validation and installation through the shared engine. Full URI behavior, modern resource deserialization, host download/cache integration and option-loader parity remain open. Native CI run 35058728745 passed for preceding commit `78774c295fc00be4be5a4ddc6b62fbf9fdba042b`.

This checkpoint passed formatting, clippy with warnings denied, and 42 regular integration tests in both all-target and no-default-feature configurations. The explicit 513 MiB test was not rerun locally for this loader change.

## Duplicate legacy URL checkpoint

Case-insensitive duplicate ModLinkFilenames URLs now use the last filename map while retaining the first URL spelling and insertion position, matching C# dictionary materialization. Existing registry entries and ModLink entries still win over that consolidated map. Three additional live-loader fixtures cover duplicate URL maps, duplicate ModLink precedence and comma/semicolon string splitting. The corpus totals 18 cases; the previously documented modern Files-map difference remains unresolved.

The 18-case comparison has 17 exact matches and the one previously recorded modern Files-map difference. Formatting, clippy and 42 regular integration tests passed in both configurations. Preceding commit `234e7cd3d3c07c0049ad0ce7bf415f985e015283` passed native CI run 35059298226.

## PrimaryUrl resource migration checkpoint

Old content-hash registry keys now migrate to their nonempty PrimaryUrl before legacy links are merged. Resource metadata is retained; the obsolete PrimaryUrl field is removed. Case-insensitive collisions retain the first URL spelling/order and the last metadata entry. Empty PrimaryUrl uses the original registry key. This also consolidates duplicate modern URL keys. Three live C# loader fixtures cover hash-key migration, collisions and the empty fallback; the corpus now has 21 cases. The existing modern Files-map difference remains open. The isolated archive-inference/install flow now also starts from an old hash-key registry.

This checkpoint passed formatting, clippy with warnings denied, and 42 regular integration tests in both configurations. The reference corpus has 20 exact matches and the existing modern Files-map difference. Preceding commit `914a14a01e87b8071aaf609677ea12f1c5990a10` passed native CI run 35059525742.

## Modern resource metadata investigation

The four-format inspection in `docs/parity/modern-resource-formats.md` broadens the known reference-loader difference: pinned C# JSON/TOML/XML drop both Files and HandlerMetadata; YAML's legacy fallback preserves file flags but regenerates identity hashes and loses size/timestamps. Rust preserves the fixture in JSON/TOML/YAML, but XML changes a nested numeric handler value into text. A regression records that exact XML gap. This is evidence of incomplete metadata fidelity, not a parity pass; earlier field-projected format checks do not certify these fields. No installer behavior changed in this investigation.

## Download filename planning checkpoint

The shared `download_plan` module and read-only `download-plan` CLI implement the reference's URL-enabled decision and filename filtering. Explicit false excludes a known file; true/null allow it. Unknown candidates and empty registries use unsatisfied Extract patterns from top-level and immediate option instructions, including unselected options. Known registry filenames and files found recursively on disk count as available, regardless of registry selection flags. Explicit filtering retains candidate duplicates; auto-discovery deduplicates case-insensitively when Extract instructions exist. Wildcard matching shares the reference helper already used by archive-option inference.

Twelve direct C# consumer cases match the Rust CLI (including the missing-source error); the C# probe restores file maps and Source strings after loading to isolate consumer behavior from known loader losses. An integration test verifies the command leaves the guide and source files unchanged. This does not implement host acquisition, cache persistence, queue integration, desktop controls or full install-readiness validation. Planning rejects source-tree symlinks consistently with Rust's install-tree policy; this safety boundary is not claimed as C# filesystem parity.

This checkpoint passed formatting, clippy with warnings denied, and 45 regular integration tests in both configurations. Preceding commit `a8d00928c41b793c12108e847a513e7a6199ce34` passed native CI run 35060165735.

## Profile persistence and CLI checkpoint

The shared profile store implements C#'s camel-case disk schema, per-profile JSON files, name sanitization using host filename rules, sorted listing with corrupt-file skipping, load/save, create, deep clone, rename, delete, capture and selection application. Capture retains an existing creation timestamp; apply leaves absent components untouched and replaces immediate option selections for present components. `profile --action ... --settings-dir ...` exposes these operations plus activation through `activeProfileName`. List/show JSON use the reference's PascalCase CLI shape. Other commands also emit JSON, unlike the reference's text messages; complete CLI output parity is not claimed.

Seven CLI operations per implementation, including duplicate-create and missing-delete failures, matched in isolated storage. Both implementations read each other's written profile files; activation preserved unrelated nested settings. `profile-check.json` records this live comparison. Local tests cover clone independence, rename conflicts, capture/apply, concurrent create serialization and corrupt settings protection. Rust refuses to replace malformed settings on activation; the reference resets malformed settings to defaults. Writes use atomic replacement and cooperating Rust writers share a lock; cross-process C# writers do not participate in that lock. Multi-file crash atomicity is unproven.

F16 is now partial, with desktop profile controls, platform-default storage, automatic active-profile install integration, directory activation through the engine, malformed GUID/date/null-schema parity and all Unicode/platform name rules still open. CLI apply exports selection flags but does not change the loader's existing all-unselected fallback on reload. No profile-controlled full installation is claimed by this checkpoint.

This checkpoint passed formatting, clippy with warnings denied, and 50 regular integration tests in both configurations. The explicit large-archive test was not rerun locally for profile persistence. Preceding commit `86675ce9650244891a4f8098f9671ac564a6423c` passed native CI run 35060722127.

## Profile-backed installation checkpoint

Validate/install now accept `--profile NAME` or `--active-profile` with an explicit settings directory. Stored paths fill missing guide/game/mod arguments; supplied arguments take precedence. Profile component and immediate option selections apply after guide loading, before explicit selection overrides. An all-unselected profile fails preflight without an installation snapshot or game-file mutation. This bypasses the exported-guide all-unselected fallback for direct profile-backed runs. These commands do not change profile/settings files or last-used timestamps; explicit profile activation remains a separate action.

Isolated CLI tests verify validation without game writes, actual selected option bytes, disabled components with missing inputs, explicit game-directory/option/component overrides, unchanged profile/settings/input files, missing defaults, and the all-unselected failure boundary. This is Rust integration evidence, not a full C# installer differential or desktop activation proof. Default settings discovery, automatic desktop profile application and remaining schema/platform cases keep F16 partial.

This checkpoint passed formatting, clippy with warnings denied, the desktop build, and 52 regular integration tests in both default and no-default-features configurations. The explicit 513 MiB archive regression was not rerun locally for this integration. Preceding commit `39a9775cb637c2aed72247ae3f70ea4e184d166c` passed native CI run 35061995790.

## Desktop profile activation checkpoint

The desktop's Install profiles panel can capture current selections/directories or activate a named profile from an explicit settings directory. Activation applies selections to the already-loaded guide, matching the reference ProfileManagerDialog flow; it does not automatically load the profile's stored guide. The shared store persists activeProfileName and last-used time. Successful activation clears validation and any pending install confirmation. Failed settings persistence leaves displayed selections and paths unchanged. Capture records the actually loaded guide path even if the guide text field has since been edited.

A desktop-state test covers capture, all-unselected activation, directory restoration, validation/confirmation reset, settings activation and malformed-settings failure. Formatting, clippy, 52 regular integration tests in both configurations and the additional desktop-state test passed. This is state-level evidence, not live native UI interaction proof. Profile listing, clone/rename/delete controls, default settings discovery and automatic startup activation remain open; F16 stays partial.

## Desktop profile management checkpoint

The Install profiles panel now lists stored profile names on explicit refresh, selects a stored profile, clones, renames, and confirms deletion. Clone/rename share the CLI store's collision and filename handling. Deletion confirmation is bound to both the settings directory and profile name. Desktop deletion clears a matching activeProfileName while preserving unrelated settings and installed files, following the reference dialog's intent. Plain CLI delete retains its existing reference behavior. Malformed settings prevent desktop deletion before profile removal. Settings are cleared before file deletion; a later deletion failure can leave an inactive profile. This is not multi-file crash atomicity.

State tests exercise list/clone/rename/delete and stale confirmation rejection. A shared-store test checks active versus inactive deletion, preservation of nested settings and malformed-settings refusal. Native widget interaction, default storage/startup behavior and the remaining schema/platform cases are still unverified or incomplete. F16 remains partial.

This checkpoint passed formatting, clippy with warnings denied, 53 regular integration tests in both configurations and two desktop-state tests. The explicit large-archive regression was not rerun locally. Commit `702afbb4ad0375911cdc394a9e08ac5de7563edb` passed native CI run 35062531335; the immediately preceding desktop commit's CI was still running when checked.

## Default profile storage checkpoint

Profile CLI commands, profile-backed validate/install, and the desktop settings-directory field now default to the platform configuration directory plus `ModSync`. Explicit settings directories still override it. The `dirs` platform helper uses the native Windows roaming-app-data folder, macOS Application Support, and Linux XDG configuration rules; see https://docs.rs/dirs/6.0.0/dirs/fn.config_dir.html. This avoids implementing native folder APIs separately.

A live Linux comparison with the pinned C# CLI created `ModSync/profiles/Default.json` beneath an existing absolute XDG_CONFIG_HOME in both implementations (`default-profile-directory-check.json`). A subprocess test proves default create, explicit override and named/active profile lookup. Local formatting, clippy, 54 integration tests in both configurations and two desktop-state tests passed. Native macOS/Windows default-folder behavior is not yet differentially verified. Rust reports unavailable config lookup instead of falling back to the working directory; unlike .NET GetFolderPath's default verification, the helper can return a base that does not yet exist. Missing/unreadable-base parity remains open, as does legacy KOTORModSync settings fallback.

Correction to earlier checkpoint notes: pinned-source caller inspection found ProfileService.ApplyProfile invoked by ProfileManagerDialog, not automatically at startup. Automatic startup reapplication is therefore not established as a C# parity requirement; the saved active name and explicit activation behavior remain supported. Broader settings restoration is still F17 work.

Native CI for `97b473bd280e138fdbdebdd0f191574d27ae481e` passed (35062691840). Run 35062899570 for `9e586aec491170a6b597bb4ae4950dfe0ed5100f` failed before all three jobs started; GitHub's annotation reports failed account payments or an insufficient spending limit. This is an infrastructure block, not a test result; further native proof remains outstanding.

## Legacy profile settings fallback checkpoint

Profile settings reads now fall back to the platform configuration directory's `KOTORModSync/settings.json` only when the requested current settings file is absent, including when storage was explicitly supplied. The profile CLI imports its two managed-deployment fields, writes subsequent activation to the current store, and leaves the legacy file unchanged. Current settings take precedence. Corrupt current or legacy JSON is rejected, retaining the documented difference from C#'s reset-to-default error behavior.

A live pinned C#/Rust CLI comparison matched resulting JSON for legacy activation and current-file precedence, with unchanged legacy bytes (`legacy-settings-check.json`). Formatting, clippy, 55 regular integration tests in both configurations and two desktop-state tests passed. General GUI settings migration, schema/casing edge cases and native UI interaction remain open; F16/F17 are not complete. This change does not migrate profile files from a legacy directory.

## Desktop directory preferences checkpoint

Desktop startup now restores C# sourcePath/destinationPath settings when corresponding launch paths were not supplied. Field names are read case-insensitively; non-string/non-null values reject the load before changing either displayed directory. The Load guide page can save both directories through atomic, locked settings writes that retain unrelated fields (including active profile and unknown nested values). The shared settings reader supports the legacy fallback while retaining its full object for directory writes. `gui --settings-dir` chooses a custom store for startup restoration and later profile/settings operations.

C# source inspection grounds the field names and nullable/empty behavior; this checkpoint has Rust store and desktop-state tests, not a live C# GUI comparison. Rust preserves unknown settings instead of serializing only the C# typed schema, and refuses corrupt JSON rather than replacing it. F17 is partial: theme, remaining settings, automatic save lifecycle, native interaction, schema edge cases and secret redaction remain outstanding.

Formatting, clippy with warnings denied, 56 regular integration tests in both configurations and three desktop-state tests passed for this checkpoint. The large-archive regression was not rerun locally; hosted native CI remains unverified under the recorded account billing limitation.

## Desktop theme preference checkpoint

The desktop Theme selector now offers the pinned C# ThemeService choices (Light, KOTOR, KOTOR 2) and persists their canonical `/Styles/*Style.axaml` paths. Startup reads the saved theme, including legacy settings fallback; absent/null uses Light. Reads are case-insensitive for the settings key. Writes atomically preserve unrelated settings and normalize that key. A save failure leaves the displayed theme unchanged. Shared string-field decoding is reused by directory preferences.

Rust uses egui light/dark visuals, with blue `#3AAAFF` and green `#18AE88` interaction/link accents drawn from the reference game themes. This is not a reproduction of the Avalonia styles, backgrounds, typography or images. C# also derives MainConfig.TargetGame from the theme; propagation into Rust's game-dependent consumers remains unimplemented. State tests verify persistence, light/dark switching and failed-save behavior; native visual/interaction proof remains open. F17/F24 stay partial.

This checkpoint passed formatting, clippy with warnings denied, 57 regular integration tests in both configurations and four desktop-state tests. The explicit large-archive test was not rerun locally; no new hosted native CI proof is claimed.

## Native profile/settings acceptance checkpoint

The native binary from `13f50111ec0eb6950ef73a21f29491fe5a6278a4` was driven with actual X11 mouse/keyboard events in an isolated Xvfb session. Screenshots and its binary hash are recorded under `docs/parity/native-profiles/`. The flow covered seeded-directory restoration, KOTOR 2 theme selection, profile capture, clone, rename, activation, stored-name listing and confirmed active-profile deletion. Disk checks verified the retained original profile, removed renamed clone, cleared active name and preserved nested settings.

The same native session ran validation and a confirmed snapshot/install for a single Copy instruction. The game directory was empty after validation; the installed file contained exactly `selected bytes`, with the source unchanged. A new process restored the saved theme and directories. This replaces the earlier blanket lack of Linux native interaction proof for these exercised paths. It does not prove all profile error states, visual equivalence, accessibility, Windows/macOS behavior or full K1/K2 guide execution. No source behavior changed in this acceptance checkpoint; the desktop binary was rebuilt, and the already-passing unit/integration suite was not repeated for documentation/screenshots.

## Tier/category selection checkpoint

The desktop Choose mods page now has exact tier/category filters and additive selection buttons backed by a shared `selection` module. Selecting a known tier includes priorities 1 through that tier; Optional includes priorities 1–4 and Optional. Unknown tiers select exact case-insensitive matches. Category selection adds matching components without clearing other selections. Both bulk operations skip WidescreenOnly components, as the pinned reference wizard does, and clear validation/install confirmation.

Source grounding: ModSelectionPage.SelectByTier and SelectByCategories. A regression covers cumulative known tiers, custom tiers, case-insensitive categories, preservation of existing selections and widescreen exclusion. Full .NET Unicode ordinal-ignore-case equivalence is not claimed. Rust still shows widescreen components in its existing single selection page, lacks the reference's separate widescreen phase and multi-category dialog, and retains its earlier text-search behavior. The new filter controls have not yet had native interaction verification. F04/F24 stay partial.

Investigation note: UseCopyForMoveActions exists in C# configuration/persistence but no execution reader was found in the pinned source. Rust already implements Move as copy then delete. No ineffective switch or alternative move behavior was added based on that setting name.

Formatting, clippy with warnings denied, 58 regular integration tests in both configurations and four desktop-state tests passed. The new selection regression is grounded in inspected reference source, not a live C# wizard differential. The explicit large-archive regression was not rerun locally.

## Spoiler-aware selection text checkpoint

The selection page now offers spoiler-free names/descriptions for components and nested options. Authored NameSpoilerFree/DescriptionSpoilerFree values take precedence; whitespace-only alternatives fall back to the reference's acronym/metadata name and simplified category description. Search now matches displayed name or simplified description, rather than the prior concatenated name/tier/category string. Normal descriptions use the reference's 200 UTF-16-unit threshold and 197-unit prefix plus ellipsis. Directions are hidden while the toggle is enabled. Component data and executable instructions are unchanged.

The acronym implementation follows the pinned converter body, including its differences from comments: JC's Weapons yields JsW, and K1 Restoration yields KR. Tests cover authored overrides, acronym limits/filler handling, hidden-text search exclusion and truncation. These are source-grounded Rust regressions, not a live C# converter differential. Unicode letter/casing behavior and truncation through an unpaired UTF-16 surrogate are not fully equivalent (Rust replaces an unpaired surrogate). The toggle is not yet persisted; links, raw draft review and activity logs can still contain original text. It is a name/description display mode, not comprehensive spoiler redaction. Native interaction and full tooltip/detail parity remain open.

Formatting, clippy with warnings denied, 60 regular integration tests in both configurations and four desktop-state tests passed for this checkpoint. No new native interaction or large-archive execution proof is claimed.

## Spoiler preference persistence and description tooltips

The selection toggle now persists the C# `spoilerFreeMode` boolean using the shared settings store and restores it at desktop startup. Missing defaults to false; case-insensitive field lookup accepts booleans and rejects null/string values. Writes preserve unrelated data and malformed JSON is not replaced. A failed toggle save leaves its previous display mode in effect. Component/option description tooltips now retain the full normal description while spoiler-mode tooltips use the authored/generated safe description, avoiding disclosure of the hidden normal description on hover.

Store and projection tests cover round trips, key casing, defaults, type rejection, corrupt-file preservation and long-description tooltip visibility. Native toggle/restart/hover proof remains outstanding. This does not redact URLs, raw draft review or activity logs and does not complete the reference's comprehensive metadata tooltip. C# settings-load failures fall back to defaults; Rust reports errors and retains files, as documented in earlier checkpoints.

Formatting, clippy with warnings denied, 62 regular integration tests in both configurations and four desktop-state tests passed. No new native interaction, hosted CI or large-archive proof is claimed for this checkpoint.

## Multiple-category chooser checkpoint

The desktop now opens a scrollable category chooser with independent checkboxes, Select categories and Cancel. Its category inventory excludes widescreen-only components and deduplicates/sorts names. Applying calls the existing shared additive selection rule with all checked categories, preserves prior choices and invalidates validation plus install confirmation. No selection changes occur while checking boxes or cancelling; Select is disabled for an empty choice. Loading a new guide clears the pending chooser.

A desktop-state regression applies two categories together, retains an unrelated selected component, excludes widescreen and verifies validation/confirmation reset. This is not native modal-dialog parity: the egui window is not an Avalonia modal window, and Unicode ordering/casing and live interaction remain unverified. The separate widescreen phase still has not been ported.

Formatting, clippy with warnings denied, 62 regular integration tests in both configurations and five desktop-state tests passed. The existing large-archive test was not rerun locally; no new native or hosted CI proof is claimed.

### Native modal input verification

The category chooser and install confirmation now use the installed egui Modal container. Background input is blocked; Escape and backdrop clicks cancel. This supersedes the earlier nonmodal-window limitation for these two dialogs. Native Linux input checks verified blocked background Deselect all and trust-checkbox clicks, category application, install cancellation without game writes or snapshots, and an explicitly confirmed two-component Copy install with exact byte checks. Evidence and source/binary hashes: `docs/parity/native-modals/receipt.json` and adjacent screenshots. Required format, lint and both test configurations passed. Windows/macOS, exhaustive focus/accessibility behavior, reference GUI comparison and the separate widescreen phase remain unverified or incomplete. F04/F24 stay partial.

### Option description visibility

Option descriptions now test the displayed text for whitespace, matching the reference ModSelectionPage option-card condition. An empty raw Description no longer suppresses authored DescriptionSpoilerFree or the generated safe fallback. The shared recursive option renderer applies the same rule to nested options. An egui rendering regression inspects emitted text shapes for both authored and fallback descriptions with an empty raw field. This is rendering-path evidence, not native accessibility or full option-card parity.

### Selection tooltip content

Component names, option checkboxes and description labels now expose a shared comprehensive tooltip following pinned ModSelectionPage.CreateModTooltip: displayed name, author, case-sensitive category descriptions, normalized tier description, full ordinary or safe description, instruction/option counts, installation method, language list and resource count. Category definitions are ported from CategoryTierDefinitions, including custom fallbacks. Numeric-only tiers retain the reference custom-tier description. A regression checks the complete multi-section output, omitted empty sections, casing, tier names and spoiler isolation. Language metadata currently requires a JSON array of strings; malformed or alternate representations do not establish schema parity. Native hover timing, positioning, accessibility and cross-platform rendering remain unverified.

### Filtered bulk selection

Select all now applies the same search/tier/category predicate as visible rows, retaining existing selections outside the filter and excluding widescreen-only components. Deselect all clears every base component regardless of filters and preserves widescreen selections, following SelectAllVisibleMods and DeselectAllMods in the reference. Clear filters resets search, tier and category without changing selection. Regression coverage combines all filters, spoiler display search, additive selection and widescreen preservation. The Rust single-page list still exposes widescreen rows; a separate widescreen phase and native button verification remain open.

### Selection and filter summaries

The selection page reports base-build selected/total counts and the reference none/all/partial selection detail, independently of active filters. A separate summary reports matching base components and active search/category/tier/spoiler filters in reference order. Both exclude widescreen-only components. Labels explicitly identify base-build scope because widescreen rows remain on the single Rust selection page. Regression coverage includes hidden selections, widescreen exclusion, no filters, spoiler mode and empty guides. Reference required/optional badges, separate widescreen flow and native summary layout remain open.

### Required and optional selection badges

The base-selection page now displays the reference required/optional counts. Required counts selected non-widescreen components with at least one declared dependency; optional counts the remaining selected base components. This follows UpdateSelectionCount exactly for supported dependency arrays, rather than inferring counts from tiers, dependency targets or dependency-list length. Explanatory tooltips state the rule. A regression distinguishes each case and excludes unselected/widescreen entries. This supersedes the earlier missing-badges note; native layout and accessibility remain unverified.

### Native selection verification

Linux native input at commit `253ba18301c0b150ca9b5542122dda63430437fe` verified search-filtered Select all, Clear filters, preserved widescreen selection, base counts, required/optional counts, safe component/option hover content and authored safe option description with an empty raw field. Screenshots, corrected fixture and binary hash are in `docs/parity/native-selection/receipt.json`. This supersedes the native-unverified notes for those exercised paths only. Tier/category dropdown interaction, accessibility, other platforms and separate widescreen flow remain open. Some emoji glyphs displayed fallback boxes; visual parity is not established.

### No-result selection state

The desktop now shows an explicit no-results message and recovery hint when no rows match. The details pane uses the shared row-filter predicate, so hidden components no longer retain visible editable options or links. Clearing filters restores details without changing selection. Native Linux screenshots and source/binary hashes are recorded in `docs/parity/native-empty-selection/receipt.json`; required Rust checks passed. This closes the exercised empty-state gap, not full grouped-card/widescreen/accessibility parity.

### Category grouping and expansion

Filtered selection rows now group by the first category (Other when absent), using ordinal UTF-16 category and raw-name ordering while retaining original guide indices for installation. Groups start collapsed and expose selected counts, select/deselect-group actions and global expand/collapse controls. Individual expansion uses egui retained state. Regression covers first-category grouping, secondary-category filtering, raw-name ordering in safe mode, empty matches and unchanged source order. Native group interaction and precise expansion-state lifecycle parity remain unverified. The existing single-page widescreen exposure also remains; group actions currently operate on all displayed members.

### Native group controls

Linux native input at commit `8ba677c` verified initial collapse, expand/collapse all, counts, filtered group deselection, restored selection and retained expansion through the exercised search/clear cycle. Evidence and binary hash: `docs/parity/native-groups/receipt.json`. This supersedes native-unverified notes for these paths only. Exact C# state lifecycle, separate widescreen handling, accessibility and other platforms remain unverified or incomplete. Installation order was not exercised in this GUI session; the shared grouping regression remains its current proof.

### Widescreen execution boundary investigation

`docs/parity/WIDESCREEN.md` records a pinned-source inconsistency: the reference base UI counts exclude widescreen, but its bulk execution receives all components and filters only by selection. The later widescreen single-component path lacks the bulk completed-state skip. Double execution is a source-grounded risk, not a live reproduction. A phased Rust port requires the isolated service oracle and recovery checks in that document. No Rust execution behavior changed in this investigation.

The widescreen double-execution risk is now reproduced at service level: bulk install produced `first` and Completed state, then single install overwrote it with `second`; both returned Success. See `docs/parity/widescreen-oracle/receipt.json`. This is not a GUI or checkpoint reproduction. Rust behavior is unchanged.

An isolated cross-phase dependency fixture now reproduces DependencyViolation with the reference wizard's widescreen-only context and Success with full component context after base installation. The initially unselected wide component was not installed by bulk execution. Evidence: `docs/parity/widescreen-dependency-oracle/`. Rust phased execution must retain completed-base dependency knowledge without replaying base instructions.

### Remaining-prefix engine execution

Engine.install_remaining now accepts a caller-verified completed prefix, retains the full selected guide for dependency/condition context, and runs only the remaining suffix through existing preflight, snapshot and stop-on-failure paths. Unknown, duplicate or non-prefix completed IDs are rejected before install writes. A two-stage fixture modifies the base source after installing it, then verifies the dependent widescreen component installs without replaying base bytes or events. This is a library primitive, not persisted resume: callers must verify guide/root/completion identity. CLI and GUI do not expose it yet. Recovery after a remaining-phase failure and durable completion receipts remain unverified/unimplemented.

### Remaining-phase recovery verification

An isolated two-stage regression now cancels the remaining suffix after its first write overwrites a completed-base file. It verifies no later step runs, retry is blocked while recovery is pending, verified restoration recovers the completed base (not the original empty game), source files return to their phase-entry bytes, and retry runs only suffix steps in order. The pending marker clears only after success. This supersedes the earlier unverified cancellation/recovery path for install_remaining; external patcher failure, process crash, durable completion identity and GUI phase wiring remain open.

### Remaining-suffix validation

Engine.validate_remaining shares completed-prefix checks with install_remaining and simulates only the suffix using full selected dependency context. A regression first executes a base Move that consumes its source: ordinary whole-guide validation fails on the consumed source, while suffix validation succeeds using the installed base output. The check verifies no suffix output, source mutation, recovery-manifest replacement or pending marker is produced. Caller-verified completion remains required; no durable resume or wizard wiring is implied.

### Saved completion evidence

Successful engine installs now atomically save completion.json with canonical roots, ordered selected component definitions and a full game-file hash manifest before clearing the pending marker. Suffix validation/install require an exact receipt match for the requested prefix and current game bytes. Missing/corrupt receipts, changed completed definitions and changed game files are rejected. Source files may change or be consumed; they are not part of completion-byte verification. Snapshots include the prior receipt and restore it (or remove it when absent), preserving the completed-base boundary after suffix failure. This supersedes the earlier caller-only completion contract. Regression coverage includes consumed sources, no replay, rollback/retry, missing/corrupt evidence and changed definitions/game bytes. Empty directories, file permissions, external effects, malicious edits to both state and game, power-loss durability and GUI phase transitions are not proven by this receipt. Hashing the entire game adds I/O proportional to its size.

### Completion summary query

`modsync completion-summary --game-dir <dir> --source-dir <dir> [--json]` reads completion.json without hashing the game and prints the components it records, the receipt's game-file count, and the game-file delta since the run started. Without `--json` it prints plain text, `N/A` when there is no delta and `No completion receipt.` when there is no receipt. With `--json` it prints the `CompletionSummary` document, or `null` when there is no receipt. Both exit 0. The delta is null while an install is pending, when snapshot.json is missing or from other roots, and when the snapshot's stored completion equals the current receipt (after a restore). A receipt written for other roots exits 1 with `Completion roots differ` on stderr. A query with no receipt creates no state directory or lock file. This is a read-only view of the receipt; it does not verify current game bytes.

### Desktop base/widescreen phases

The desktop now masks widescreen selection in the base job without changing authored choices, hides widescreen base-list rows, and transitions after base success to widescreen selection on the install page. That job uses receipt-checked suffix validation/install with full dependency context. Base navigation/profile changes are disabled after success. Zero selected widescreen mods can finish; a pending failed install requires restore before that finish action is enabled. Successful widescreen execution reaches a completion view. A desktop-state regression verifies deferred selection and phase transitions; engine receipt/no-replay/recovery regressions cover execution primitives. Native end-to-end phased interaction remains unverified. Optional warning content, automatic restart/resume navigation, guides with no selected base mods, and phase orders where base is not an execution prefix remain open; unsupported ordering fails instead of being reordered or backfilled. Start a fresh app session for another guide after completion.

### Native two-phase installation

Native Linux input at `7918409` verified base validation/confirmation, base-only writes, transition to widescreen selection, and confirmed dependent widescreen installation after the base source was removed. Final bytes, completion IDs and absence of pending recovery state were asserted. Screenshots, fixture, final completion receipt and binary hash: `docs/parity/native-phases/`. This supersedes native-unverified status for the exercised successful two-phase path; native failure/restore/skip, restart, warning content, empty-base guides, patchers and full builds remain unverified or incomplete.

### Optional widescreen notice

After base success, the desktop shows the reference default warning or authored metadata.widescreenWarningContent (also WidescreenWarningContent) before widescreen selection. Explicit blank/null content skips it; Continue dismisses it for this guide session. Text is displayed literally, matching the reference notice control. A desktop regression covers phase gating, default/authored/blank/null content and dismissal. Native notice interaction and Markdown warning extraction remain unverified/incomplete.

### Markdown widescreen guidance

Markdown now extracts the optional widescreen warning from its case-insensitive heading through the first following level-three heading and starts epilogue content at the subsequent level-two section. The desktop consumes this extracted warning. The isolated C# parser fixture in `docs/parity/markdown-warning-oracle/` confirms exact warning/epilogue text and the reference Mod List boundary excluding later widescreen components. Rust regressions cover CRLF normalization, missing first mod, absent post-widescreen section and desktop metadata consumption. This supersedes the missing warning-extraction status above; widescreen component inclusion/classification outside Mod List, native notice interaction and complete Markdown fidelity remain open.

### Markdown widescreen component classification

For prose guides, WidescreenOnly now follows component position within the optional widescreen section, matching the reference override of embedded metadata. An isolated C# fixture without a Mod List heading confirms base/base/widescreen/base classification and warning/epilogue boundaries; the Rust regression compares its saved output directly and covers CRLF, missing header and missing closing section. Evidence: `docs/parity/markdown-phase-oracle/`. This closes the positional classification gap above. The existing Mod List exclusion is unchanged. Full corpus equivalence and native Markdown-to-phased-install proof remain open; classification does not relax the installer requirement that completed base components form an execution prefix.

### Finish returns to setup

The successful completion view now has a Finish action returning to guide setup, retaining the loaded guide, directories and output log while clearing transient phase/confirmation/validation state. Loading a guide reuses the same reset. A desktop regression verifies this boundary; a new installation requires fresh validation. This supersedes the restart-required limitation above. Saved completion/recovery files are untouched; this is not restart/resume support. Native repeated-install interaction remains unverified. The reference FinishedPage contains static next steps, not authored epilogue display; epilogue documentation export is still a separate parity gap.

### Desktop validation invalidation on setup failure

Starting any desktop job now clears prior validation and install confirmation before checking directories or archive settings. Early setup failures can no longer leave the old install-ready state active. A regression covers invalid archive budgets for validate/install/restore/download and an install directory removed before launch, asserting that no worker starts. This repairs desktop state only; engine preflight already independently rejects invalid inputs.

### Wizard content export preservation

TOML and YAML export now place extracted Markdown wizard-content fields in the reference metadata object, supplying fileFormatVersion 2.0 when absent. Existing nested metadata overrides root content, including explicit blanks and PascalCase aliases. A Markdown-to-TOML/YAML regression verifies content retention and stable re-export; JSON/XML remain component-only exports. Live C# re-import of this new export path remains unverified.

### C# re-import of wizard content exports

At Rust be7052a, a fresh CLI build exported the saved Markdown warning fixture to TOML and YAML. The pinned C# deserializer imported both in an isolated process and recovered exact preamble, epilogue and widescreen-warning text plus both components. `docs/parity/content-export-oracle/` contains exports, probe, result, hashes and a runnable verifier. This supersedes the unverified cross-language re-import note above for this fixture only; full corpus and native UI proof remain open.

### YAML metadata comment recognition

YAML documents beginning with the reference case-insensitive `# Metadata` marker now load as metadata even without fileFormatVersion. Document separators must be unindented; separator-like text in block scalars remains data. Regression covers marker-only metadata, CRLF, a leading YAML directive, descriptions containing marker text, re-export and invalid non-mapping metadata. Exotic YAML directive/tag combinations and live C# differential coverage for these cases remain open.

YAML directive regression correction: directives after an explicit document end now stay with the following document. A three-document fixture first proves acceptance by serde_yaml_ng, then verifies guide components and metadata. The test failed under 795420a and passes with this correction; exhaustive YAML grammar compatibility is not claimed.

### FOMOD reference selection investigation

`docs/parity/FOMOD.md` records the full implementation/acceptance boundary. The isolated C# parser/mapper/presenter probe in `fomod-selection-oracle/` verifies five group defaults and selection validation, and reproduces an indirect required-plugin deselection conflict. Rust still refuses FOMOD installation; no FOMOD support is claimed from this evidence.

### FOMOD shared group selection

`fomod::GroupSelection` now implements the five reference group modes, default selection and exact cardinality messages. An executable regression compares defaults and selection transitions against the saved C# oracle. Required-plugin removal is rejected both directly and through exclusive siblings, with no partial mutation; this deliberately corrects the reproduced reference bypass. Conflicting required choices remain validation errors. Parser, visibility, mapping, choices files, CLI/desktop wiring and installation remain unfinished, so the engine still refuses FOMOD archives.

### FOMOD choices-file primitives

The library reads/writes the reference version/archives/selections camelCase schema (missing version defaults to 1), finds archive filenames ignoring ASCII case, and applies named plugin groups atomically through shared cardinality validation. Repeated plugin names collapse to one choice. Unsupported versions, duplicate archive/group entries, missing or ambiguous plugins, omitted required plugins and invalid cardinality are rejected without mutating selections. These are explicit stricter checks than permissive reference behavior. Unicode ordinal case equivalence, whole-session step lookup/visibility, CLI flags and configured installation remain open.

### FOMOD choices across steps

ArchiveChoices.apply_to routes named selections through unconditional StepSelection groups using ASCII-insensitive lookup. It validates the candidate set before committing any changes: late failures, missing/ambiguous names, duplicate choices and invalid untouched defaults leave all original groups unchanged. This closes whole-set atomicity for unconditional steps only. Visibility-dependent validation and session flag propagation remain required before this can configure arbitrary FOMOD packages.

### FOMOD flag-based visibility

Step selections now carry optional nested All/Any/Flag conditions, and plugins carry ordered flag assignments. Visibility follows the reference source: all selected plugins contribute even from hidden steps, later assignments win, flag names ignore ASCII case and values are case-sensitive; empty composites are true. Choices application validates resulting visible steps. Regression covers hidden flags, overwrite order, nested conditions, empty Or, value casing and hidden invalid groups becoming visible. Live reference differential proof, Unicode equivalence, file/game dependencies and package integration remain open.

### FOMOD XML selection reader

The library now reads the selection portion of ModuleConfig.xml into shared steps/groups/plugins, including Required/Recommended defaults, dependencyType defaultType, condition flags and nested flag visibility. It uses namespace-local names, handles escaped attributes/CDATA, rejects DTDs/external entities and malformed roots, and bounds nesting. Unsupported non-flag visibility conditions fail explicitly. This reader intentionally does not produce install instructions or enable FOMOD extraction: files, priorities, descriptions/images, full module metadata and conditional file mapping remain to be added to a complete module parser. Regression checks XML-driven visibility/defaults and rejection boundaries; live differential XML coverage remains open.

### FOMOD plugin details

XML selection state now preserves trimmed plugin descriptions and authored image paths. Nested description text is concatenated in document order, matching XElement.Value semantics, with entity and CDATA decoding. Image paths remain untrusted metadata and are not opened by parsing. Regression covers mixed text, missing fields and an escaping image path retained without filesystem access. Image rendering/path containment and live C# differential proof remain open.

### FOMOD module file data

parse_module_xml now retains module name, required files, plugin files and flag-conditional file patterns alongside selection state. File/folder records preserve authored source/destination and order; signed 32-bit priority parses with whitespace and defaults to zero for missing/invalid/overflow values, following the reference parser. Missing conditional dependencies become an always-true empty conjunction. Parsing does not resolve paths, sort priorities or authorize writes. File/game dependencies, info.xml, instruction mapping, archive containment and installer integration remain open.

### FOMOD selected file plan

ModuleConfig.selected_files builds a read-only ordered plan: required files, selected visible plugin files with their attached conditional files, then unconditional patterns. Supported flat flag patterns attach to the first plugin declaring all required flags, even if another selected plugin declares them too, following the reference mapper. Nested/multi-branch OR patterns and flags not supplied by one plugin are skipped, matching the reference mapper's scoped fallback. A network-disabled C# mapper probe confirms both skip cases leave only plugin files and an unconditional trailing file; source, result, and receipt are in `docs/parity/fomod-conditional-skip-oracle/`. Priorities remain preserved data: the reference mapper does not sort them. Safe path/instruction mapping and real installation remain open.

### FOMOD file-copy instruction mapping

ModuleConfig.copy_instructions maps the ordered selected file plan from an explicitly supplied extracted mod-relative directory to shared Copy instructions. Existing cross-platform path validation rejects traversal, rooted paths, wildcards and reserved names before instructions are returned. Compatible filename destinations become parent directories; requested renames and folder directives fail explicitly until implemented. A temporary-directory integration test proves dry validation creates no game files and installation copies exact bytes without changing the source. Archive extraction/configuration gates remain unchanged; this is a library mapping primitive, not end-to-end FOMOD support.

### Verified FOMOD folder mapping

An isolated C# mapper/install probe returned Success with `textures/*`, installed only top.txt and omitted nested/deep.txt. Evidence: `docs/parity/fomod-folder-oracle/`; the verifier excludes only C# .modsync session bookkeeping from asset comparison. Rust now emits the same immediate-file wildcard and a shared-engine regression verifies identical asset behavior, dry-run non-mutation and unchanged source data. This supersedes folder mapping rejection above. It is deliberately not recursive FOMOD directory semantics; empty folders, recursive expectations, renames, archive configuration and UI/CLI integration remain open.

### FOMOD info.xml metadata

The shared XML reader now parses Name, Author, Version, Website and Description with the reference parser's optional-field defaults, first direct-child lookup, case-insensitive local names and preserved authored text. A regression covers namespaces, duplicate fields, mixed text, malformed XML and DTD rejection. This is source-grounded against FomodParser.ParseInfoXml, not a live C# differential receipt. Website remains untrusted text; archive discovery and UI presentation are still pending.

### FOMOD detector segment boundary

Extraction now uses a shared terminal-segment detector matching FomodDetector.IsModuleConfigPath instead of a raw suffix check. Nested and mixed-case FOMOD paths still require configuration; an unrelated notfomod directory no longer causes false refusal. Isolated ZIP preflight coverage verifies both outcomes without writes. Detection alone does not authorize paths or configured installation.

### FOMOD archive inspection

The shared archive reader can now inspect and parse a nested ModuleConfig.xml plus sibling info.xml, returning their content root without retaining extracted files. It uses existing archive safety, size-budget and cancellation checks and temporary scratch storage. Multiple installers fail explicitly rather than choosing an enumeration-dependent first entry. ZIP regressions cover metadata, unchanged source bytes, cleanup, malformed XML, corrupt archives, unsafe paths and cancellation. This spools the whole archive and is not the C# listing-only probe; configured installation and interface integration remain open.

### FOMOD inspection CLI

`modsync fomod-inspect <archive> [--scratch-dir <directory>] [--max-expanded-bytes <bytes>]` exposes archive inspection as JSON: module path, content root, optional info metadata, steps, current visibility, group modes and plugin defaults. Non-FOMOD archives return JSON null; invalid archives exit with an error. The ZIP integration test invokes the actual CLI and checks its report. Inspection does not configure or authorize installation.

### CLI FOMOD choices validation

`fomod-inspect --choices <json>` applies the matching archive entry through the shared atomic selection logic before reporting selections and visibility. Archive, step, group and plugin matching reuse the existing case-insensitive rules. Missing archive entries, absent installers and invalid selections return errors without a success report. A subprocess regression verifies selection changes, invalid names and unchanged archive bytes. This does not persist configuration or enable installation.

### Archive-relative FOMOD copy plans

ArchiveModule copy mapping now retains the wrapper directory above fomod/ModuleConfig.xml. The inspection CLI can emit the validated shared-engine copy instructions with `--extracted-directory <relative-path>`, after optional choices application. Invalid selections and unsafe roots fail before output. A regression checks wrapper-relative source mapping. The command does not extract into that path or install; automatic execution still requires archive configuration and recovery integration.

### Configured FOMOD extraction execution

A component may now embed the version-1 choices document in its FomodChoices metadata. At each Extract instruction, the shared engine parses the actual archive configuration, requires a matching archive entry, applies and validates choices, and runs mapped copies after extraction. Wrapped paths remain relative to the actual extraction destination, which must be inside the mod directory. This uses ordinary preflight, snapshot, ordered execution and recovery; choices remain serialized with component metadata in completion receipts. A real ZIP regression proves default refusal, non-mutating validation and selected output bytes. Automatic prompting/configuration persistence UI, broader archive fixtures and failure/restore acceptance remain open.

### Configured FOMOD cancellation and restore acceptance

The configured ZIP install regression now cancels after the FOMOD Extract/copy step and before a later Delete. It verifies selected output exists at failure, the later step did not run, retry is refused while recovery is pending, restore removes extracted files and restores exact baseline/archive bytes, and an ordered retry completes with no pending marker. This covers cancellation between guide instructions, not every interruption point within archive extraction or copying.

### CLI configured FOMOD installation

Validate and install accept --fomod-choices with the existing version-1 JSON contract. Explicit CLI choices override embedded choices for the run, including option components, and are retained in completion component definitions without rewriting the input guide. A subprocess test validates without writes, installs a real configured ZIP, checks exact output bytes and verifies choices in the completion receipt. Desktop selection controls and automatic post-download prompts remain open.

### Desktop FOMOD choices-file input

The validation page now accepts an optional choices-file path using the existing file picker. Desktop validation and installation parse that file and use the same recursive guide override as CLI; load errors invalidate validation and prevent job launch. Restore does not depend on the choices file. This is file-based configuration, not the full interactive plugin-selection wizard; native visual acceptance remains pending.

### FOMOD source provenance guard

Configured copies now require every matched source to be supplied by the current archive. Stale extraction files cannot satisfy missing file directives or join folder wildcard copies. Configured extraction with Overwrite=false rejects existing archive paths rather than retaining stale bytes. Regression installs cover these three cases and verify rejection during preflight, unchanged stale source bytes and no pending snapshot or game writes. This intentionally fails closed on contaminated extraction directories instead of silently using their contents.

### Saving FOMOD selections

The inspection CLI accepts --save-choices to atomically write the current selection session, including choices applied with --choices, in the reusable version-1 schema. Capture preserves hidden-step selections and rejects invalid or ambiguous named groups by reapplying the generated document before saving. A round-trip regression verifies nondefault selection persistence and ambiguity rejection. Interactive selection editing remains outstanding.

### Choices export archive preservation

A Unix subprocess regression reproduced source-archive replacement when --save-choices named the archive through a symlinked parent directory. Export now compares canonical existing paths before writing and propagates unexpected resolution errors. The regression also reads back a successful choices export and verifies original archive bytes survive the rejected alias write. Concurrent external path replacement is outside this check's guarantee.

### Interactive console FOMOD selections

fomod-inspect --interactive prompts for numbered plugin selections, reusing group cardinality and required-plugin rules, recomputing visibility before each step and retaining edits only on success. Invalid answers retry; q or EOF cancels. Prompts use stderr so stdout remains JSON, and --save-choices persists the result for installation. A stream-driven regression covers retry, selection and cancellation. Back navigation and the desktop interactive wizard remain open.

### Console FOMOD back navigation

The interactive prompt accepts back at a group to revisit the preceding currently visible step. Revised flags are evaluated before advancing again; the regression revisits an earlier choice and hides the previously displayed following step. Cancellation still discards the whole pending session. Native console interaction and the desktop interactive wizard remain unverified.

### Interactive CLI to installation acceptance

The real CLI subprocess fixture now selects a nondefault FOMOD plugin through piped stdin, saves its choices, parses the stdout JSON, validates without install-tree writes and installs using the exported file. It verifies only alternate asset bytes are installed, prompts stay on stderr, the source archive and guide remain unchanged, and the receipt records the selected plugin. This proves the console protocol end to end, not terminal rendering or desktop interaction.

### Desktop interactive FOMOD configuration

The validation page now offers Configure FOMOD archive. A background worker inspects the selected archive, then a modal displays visible steps and plugin checkboxes through the shared selection rules. Accept captures valid choices and merges them into loaded guide metadata, preserving other archive configurations and invalidating installation readiness. Cancel discards pending edits and requests worker cancellation. A merge regression verifies package preservation. Native rendering/click acceptance, archive images, persistence to a saved guide and automatic post-download prompting remain outstanding.

### Reopening desktop FOMOD selections

The desktop editor now reapplies saved archive choices after inspection, using the selected external choices file or embedded guide configuration. Invalid saved selections stop loading instead of silently reverting to defaults. Conflicting component configurations are reported. A shared resolver regression covers nested components, case-insensitive archive names, missing entries and conflicts; native reopening acceptance remains pending.

### Desktop configured-guide persistence

The existing Save selection action now embeds the selected external FOMOD choices file into its saved guide, as installation does. Accepted editor choices were already embedded in the loaded guide and use the same save path. A desktop regression saves TOML, reloads component selections and named archive choices, verifies the in-memory guide remains unchanged, and proves invalid choices leave an existing output untouched. Native save-dialog acceptance remains pending.

### Desktop archive path entry

An isolated Linux X11 run rendered the validation page, but the system archive picker did not appear on that display. FOMOD configuration now accepts a typed archive path with optional Browse, matching the existing path-field pattern. This removes the picker as a prerequisite for opening the editor; full native editor/install acceptance is still pending.

### Native FOMOD editor/install acceptance

Linux X11 acceptance at commit 3e7cf9f opened the desktop editor from a typed archive path, selected Alternate instead of Default, accepted and reopened the session, validated without changing either install tree, and confirmed a real install. The GUI reached Installation complete; only alternate.txt reached Override with exact expected bytes, the completion receipt records Alternate, and no pending marker remains. Screenshots, fixture ZIP, receipt and binary hash are in docs/parity/native-fomod. Save-dialog/restart persistence, previews, automatic prompts, other platforms and full builds remain outside this proof.

### Desktop post-download FOMOD prompt

Successful verified direct downloads now pass their actual output path and archive limits to the desktop FOMOD editor when a guide is loaded before base installation. Before launching, Rust matches the downloaded filename against resource-registry entries and uses shared prompt state: registered `configured` and `warned` archives stay quiet, while `dismissed` or unregistered archives remain eligible for inspection. Inspection runs in the background, restores existing choices through the same resolver as manual opening, and dismisses ordinary archives without a configuration error. Failed downloads do not dispatch this event; inspection errors remain visible. An egui regression checks ordinary-archive dismissal, status suppression, and visible errors. Automatic post-download editor cancellation persists `dismissed` on matching registered resources only after inspection positively identifies a FOMOD, so a later download re-prompts as C# requires; cancellation during inspection and manual editor cancellation leave state unchanged. Native network-download-to-editor acceptance and multi-host queue integration remain pending.

### Persisted FOMOD prompt state

The Rust guide resource registry now preserves the C# `handlerMetadata.fomodPromptStatus` shape per matching archive filename. It recognizes `dismissed`, `configured`, and `warned`: dismissed archives prompt again, while configured and warned archives do not. Applying or merging valid Rust choices records `configured` when the component already has a resource entry for that archive, preserving existing metadata and its serialized key casing. The shared validation gate now also enumerates archive-like `ResourceRegistry.Files` entries independently of `Extract` instructions, safely resolves their mod-directory paths, and inspects existing archives before requiring explicit choices for detected FOMODs. A focused regression proves a registered nested FOMOD cannot evade the gate simply by omitting an Extract instruction. Registered archives that become missing after a persisted prompt state and registered unreadable archives now fail closed. Missing-archive warning classification without prompt state, configured-without-merged-instructions warnings, dependency-scope expansion, and C# post-download prompt/recovery orchestration remain open. The state by itself does not authorize execution: the Rust validation gate still requires explicit `FomodChoices`.

### Desktop FOMOD inspection lifecycle

A worker-lifecycle regression verifies that replacing an inspection requests cancellation of the old worker, disconnects its result channel, and receives the replacement worker's own error. Dropping the editor requests cancellation of the current worker. This establishes result isolation and cancellation signaling; it does not prove a bound on archive-reader cancellation latency.

### Desktop FOMOD step navigation

The editor now displays one visible step at a time with Back and Next actions, matching the C# dialog flow. Next validates the current groups and recomputes flag-dependent visibility; final acceptance still validates captured choices. Descriptions appear inline and required plugins have disabled checkboxes. A regression covers returning to revise an earlier choice, skipping a newly hidden step, invalid selection blocking advancement, and packages without optional steps. Native acceptance of the new multi-step navigation remains pending; earlier native evidence covers the previous single-panel editor.

### Native FOMOD multi-step acceptance

At d813d01, an isolated Linux X11 run exercised Next to a conditional step, Back to change its controlling selection, and Next directly to the last remaining step. Clearing the exclusive selection blocked advancement with a visible error. Accepting and reopening retained the revised choice and reset navigation to the first visible step. The game stayed empty and the mod directory retained only the original archive. Fixture, screenshots, binary hash and limitations are recorded in docs/parity/native-fomod-steps/receipt.json. This verifies native navigation; installation, process restart and other platforms were not exercised in this run.

### Shared file-conflict analysis and CLI

The shared engine now exposes analyze_conflicts, and the conflicts CLI accepts the same guide/profile/selection/FOMOD arguments as validate. Reports contain case-insensitive game-file write histories, first-observed path casing, ordered component writers with zero-based instruction indices, last writer, component counts and analyzed component count. Repeated writes by one component collapse to its latest instruction; skipped overwrite=false operations and mod-workspace staging do not count. Option writes are attributed to their top-level component and Choose instruction. External program effects are explicitly excluded. Unlike the C# analyzer, simulation errors abort rather than returning a report that silently omits failed instructions. Tests compare the real CLI report to the shared result, prove analysis leaves both trees untouched, and confirm the reported last writer against a real isolated install. Desktop presentation, C# differential execution and broader instruction/option coverage remain pending; F19 is partial.

### Desktop file-conflict report

The validation page can now run shared conflict analysis in its background worker before base installation. The expandable report lists paths and ordered writers with instruction numbers and last-writer labels, exposes external-effect limitations, and copies JSON. Input/readiness invalidation clears the report, and conflict success does not authorize installation. A desktop worker regression checks the report, read-only game state and stale-result clearing; setup-failure tests include the new action. Native rendering/click acceptance and remaining-phase analysis are still pending.

### C# conflict-service differential fixture

The pinned C# FileConflictAnalyzer was executed in a network-disabled, read-only-root sandbox with only an isolated fixture writable. Its report agrees with Rust on destination, ordered writers, latest instruction index per writer, last writer, per-component counts and analyzed count for repeated Copy instructions, overwrite=false and staging exclusions. The Rust regression consumes the captured C# result; probe source, DLL hash, result and boundaries are recorded in docs/parity/conflict-oracle. Both analyzers leave the fixture game empty. This is one service-level fixture, not full conflict parity or native desktop acceptance.

### Native desktop conflict-report acceptance

At commit 4bdd53a, an isolated Linux X11 run started conflict analysis from the validation page, displayed one conflict across two analyzed components, expanded its destination, and showed both writers in install order with the final writer labeled. Changing the component selection removed the stale report. The game tree remained empty and the mod tree retained only its two source files. Screenshots, fixture inputs, the binary hash, and proof boundaries are recorded in `docs/parity/native-conflicts/receipt.json`. Clipboard contents, other platforms, patchers, and broader instruction combinations remain unverified.

### Remaining-phase conflict analysis

Conflict analysis now has the same saved-completion prefix contract as remaining validation and installation. After a verified base phase, the desktop action analyzes only selected remaining components and keeps it available for the widescreen phase. The shared-engine regression installs a base component, enables a dependent tail component, verifies that the tail alone is analyzed without recreating a base conflict, confirms both trees remain unchanged, and rejects a non-prefix completion list. Native rendering of this post-base action remains unverified.

### FOMOD plugin type descriptors

FOMOD parsing now preserves Optional, Required, Recommended, NotUsable, and CouldBeUsable descriptors on every plugin. Required and Recommended retain the C# default-selection behavior; unknown descriptors fall back to Optional. The `fomod-inspect` JSON report exposes `pluginType`, so callers no longer have to infer the authored state from the required and selected booleans. A sandboxed execution of the pinned C# parser agrees on all six type cases, including a dependencyType default and unknown fallback; source, result, and DLL hash are in `docs/parity/fomod-plugin-types/`. DependencyType per-pattern rules, full-corpus behavior, and native UI presentation remain open.

### FOMOD file and game dependencies

The pinned C# mapper logs fileDependency and gameDependency as unmodeled, then treats each as satisfied. Rust now preserves that behavior as an always-satisfied dependency so a file or game leaf does not hide a step or discard a conditional-file pattern. Flag leaves in the same AND group remain enforced. A network-disabled C# parser/mapper probe confirms both dependency kinds evaluate true in visible and conditional contexts; source, output, and receipt are in `docs/parity/fomod-unmodeled-dependencies/`. This does not model game state or file presence, and nested unsupported dependency shapes remain rejected.

### FOMOD file rename mapping

The C# mapper cannot express a rename through its Copy action. For a file destination containing a filename, it logs the requested rename, drops that filename, and copies the source filename into the destination directory. Rust now does the same. An isolated install regression verifies that `textures/a.tga` requested as `Override/b.tga` installs `Override/a.tga`; a network-disabled C# mapper probe records the same generated source and destination in `docs/parity/fomod-rename-oracle/`. Archive extraction, full FOMOD corpus behavior, and native desktop parity remain outside this proof.

### Managed deployment manifests

The shared deployment service stages `staging/<component-guid>` trees into the game directory using hardlinks with a copy fallback, writes C#-shaped per-component JSON manifests (seven-digit `DeployedUtc`, lowercase GUID file names), backs up overwritten files, and exposes status, deploy, uninstall, newest-first purge, snapshot, record and switch through the CLI. Roots come from `--staging-dir`/`--manifest-dir` or from a profile's `profiles/<name>/{staging,deployment}` directory, as in C#. Uninstall removes only files whose current SHA-256 matches the manifest and restores a displaced file only after that removal; user-modified deployed files and their backups remain intact. Deliberate differences from C#, all fail-closed: redeploying a component that still has a manifest is refused (C# overwrites the original backup), the staged tree is preflighted and a failed deploy rolls back, symlinked targets and staged symlinks are refused, existing case variants of game paths are reused, an out-of-order uninstall hands its original backup to the later owner, corrupt manifests are skipped with a warning, and patcher provenance records a pre-existing changed file only with a backup of its prior bytes. Real CLI probes and a C# DLL differential (C# writes and Rust uninstalls; mixed C# and Rust manifests purged by Rust) are recorded in `docs/parity/deployment-csharp-audit.md`. Engine staging, GUI controls, multi-platform filesystem behavior and a C#-side purge of Rust manifests remain open.

### Concurrent direct-download queue

`download-batch` accepts a JSON array of direct HTTPS jobs and runs a bounded worker queue with retry counts and stable input-order JSON results. The shared queue marks work that has not started after cancellation as cancelled and leaves failed direct-download output unpublished because the existing downloader uses a temporary file. Deterministic executor tests cover retry, terminal failure, result order and cancellation before start. Per-byte progress/speed, live cancellation control, provider-specific handlers, GUI status/restart behavior and controlled HTTPS transport acceptance remain open.

### Interactive dependency selection checkpoint

Desktop component and option toggles now route through a shared selection resolver matching the pinned `ComponentSelectionService` behavior: selecting a component follows transitive component dependencies, resolves component/option restrictions in both directions and selects required options; selecting an option selects its parent and its component/option dependencies; deselection cascades through dependents and clears a parent when its last option is removed. Filtered, tier, category and group bulk selection use the same resolver, while base deselection cascades instead of leaving selected dependents behind. A focused Rust regression covers the reference test suite's component chain, mixed restriction, option dependency, reverse option restriction and deselection cases. The pinned C# test source contains corresponding executable cases, but a fresh local reference run could not progress past NuGet restore and was cancelled after five minutes; this checkpoint therefore has source/test alignment and Rust execution evidence, not a fresh cross-runtime result. Native pointer interaction, nested options beyond the reference's immediate option model and Unicode identifier equivalence remain open.


## Wave 2 integration follow-up — 2026-09-28 UTC

The engine's source-tree scan now excludes `.peer-*.part` directories using the
same predicate as download planning. Unaccepted peer payloads therefore do not
become wildcard install inputs. The game-tree scan is unchanged, and recovery
snapshots still cover the complete source tree and reject symlinks. A direct CLI
install copied an accepted file while leaving the staged peer payload untouched.
This is a source-discovery safeguard, not complete peer acquisition or F29 proof.
