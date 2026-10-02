# Full C# feature parity — active acceptance ledger

## Working-source reconciliation — 2026-10-02

Full parity remains incomplete. The current source implementation is tracked in
[the remaining-work tracker](../plans/2026-09-28-1130-remaining-work-tracker-plan.md),
including checkpoint/recovery, desktop acquisition, host providers, persistent
sharing, profiles/settings, protocols, diagnostics, packaging, editing/merge,
and CLI composition added after the historical checkpoints below. Historical
"not present" statements are not a current source inventory.

Source review and production build receipts remain distinct from acceptance.
Only the source boundary named in each [production receipt](production-gates-2026-10-02.md)
is covered by that receipt; subsequent changes are unverified by those builds.
The user currently requires implementation before tests/CI and forbids filesystem
test creation, editing and execution. Native, provider, full-build, platform and
live-game acceptance remains deferred. No domain is closed by this reconciliation.

## Current integration status — 2026-09-28

**Full parity is incomplete: F01–F28 remain partial and F29 is unverified.**
[PROGRESS.md](PROGRESS.md) tracks the active sequence and current blockers.
Reviewed local wave-two checkpoint `9ecf8e4` includes acquisition/P2P libraries,
profiles/settings, texture priority, loader deduplication, completion CLI/UI and
managed deployment; [integration evidence](wave2-integration/README.md) preserves
individual source/binary bindings, reviews and proof limits. Earlier dated slice
entries below remain historical; their statements of missing work apply to their
named commits, not automatically to this checkpoint.

Selected-guide `fetch-downloads` (`2889ec5`), loopback HTTPS (`6c08656`), and
Source normalization (`62b6f70`) have passed independent specification and quality
reviews with source-bound production evidence. See [current progress](PROGRESS.md)
and their linked receipts for precise scope; none closes an entire domain.
FOMOD post-download CLI policy, cancellation and scoped recording are the active
implementation. Shared desktop acquisition, persistent sharing, remaining FOMOD
semantics, host providers, Markdown, checkpoints and other ledger requirements
remain open. No Rust tests or custom Rust harnesses may be added, changed or run
under the current user freeze. Nothing here substitutes compilation for full-build,
platform, external-host or live-game proof.

## Completion pages slice checkpoint — 2026-09-27

F10, F11 and F24 remain **partial**. Source at `12c5c68` (exact Base/Widescreen/Finished completion pages); plan and source-cited composition in [completion-pages-plan.md](completion-pages-plan.md) and [completion-pages-csharp-audit.md](completion-pages-csharp-audit.md); native evidence in [completion-pages-native/](completion-pages-native/RECEIPT.txt); differences recorded in `COMPATIBILITY.md`.

- **F24 (wizard pages).** Added: `src/ui/completion_pages.rs` renders Base Installation Complete, the Widescreen Support notice (split into its own page-content arm), Widescreen Installation Complete and Finished, each from the shared read-only `recovery::CompletionSummary` and per-phase `PhaseReceipt` captured by the install worker after the engine commits — never recomputed in the UI. Stat cards, "What's Next"/Important Notes, Quick Actions and the Finished Next Steps/Support cards follow the C# measurements and copy from the audit, with recorded truthful substitutions (`RECOVERY SNAPSHOT` for `CHECKPOINTS`, `GAME FILES CHANGED` for `FILES MODIFIED`, `Recovery` for `Manage Checkpoints`, `Copy Log` for `View Logs`) because Rust has one whole-run snapshot with no rollback after success and keeps no log file. The Finished page's recovery bullet now quotes the same `RECOVERY_TEXT` constant used by the Recovery modal, instead of a paraphrase. The added (non-C#) epilogue box on Finished, the 4→2×2→1 stat-card wrap at small widths, and the hidden managed-deployment summary are recorded as deliberate, non-silent differences. Still open: the `Open Game Folder`/`Open Mod Folder` mechanism uses `ctx.open_url(file://…)` (browser-first) rather than a native shell "open folder" call — native testing showed it opening a browser instead of a file manager; fixing the mechanism is a follow-up, not done here. The Copy Log line cap (`LOG_LIMIT = 2000`) is a known limit, not a defect fixed in this slice.
- **F10 / F11 (install facts, recovery).** No change to receipt schema, `verify_completion`, `begin`, `finish` or `restore`. This slice only adds read-only presentation of existing facts; it does not close per-component checkpoints or add any rollback path after success. The plan's Task 2b (a standalone `src/main.rs` CLI query serializing `CompletionSummary`) is **not present** in this worktree — confirmed by source grep, no `completion_summary`/`CompletionSummary` hits in `src/main.rs`. The plan's probes 4, 4b, 6 and 7, which it says can only be verified through that CLI (pending→`None`, restore-binding→`None`, roots-mismatch error, no-receipt case), were not run. This is a genuine open gap, not exercised by this slice.

No Rust tests were created, changed or run in this slice.

## Install progress slice checkpoint — 2026-09-27

F10, F24 and F26 remain **partial**. F29 remains **unverified**. Source through `9df6c3b` (help text `33de7f6`); rulings, commits and review findings in [install-progress-integration.md](install-progress-integration.md); evidence in [install-progress-native/](install-progress-native/README.md).

- **F10 (install coordinator, cancellation, progress).** Added: a shared typed progress contract (`Engine::install_observed` / `install_remaining_observed`, `Progress`/`Phase`/`OwnerFrame`) with silent preflight, exact top-level suffix totals, GUID-bound owner paths, and `completed` only after the receipt and `recovery::finish`. Native proof covers desktop cancellation at a safe boundary with exact restore, failure with exact restore of the captured roots, and a late Stop that cannot undo a committed success. Still open: per-component checkpoints (Rust keeps one whole-run snapshot), automatic rollback, crash durability, nested Choose evidence beyond source reading, and the reference best-effort install flags noted below.
- **F24 (wizard pages).** Review findings #2, #9 and #10 are fixed by `a24e96b` (#2, #10) and `9df6c3b` (#9); the scoped frontend-races re-review approved them, with native evidence in `install-progress-native/gui-r2-review-fixes/`. The Installing page now renders shared facts for Base and Retained Remaining, with the deliberate C# deviations listed in `GUI-INVENTORY.md` (five C# defects not copied, "Not counted" issues, recovery snapshot row, Stop Install, round-half-to-even percent capped at 99). Still open: the 600×400 host chrome leaves about 280×50 px for the page body; DPI, keyboard, accessibility and non-Linux rendering.
- **F26 (CLI contracts).** Added: `install --install-progress-json` NDJSON with the `installProgress` envelope, conflicts with `--json`/`--validation-report`, legacy `--json` byte-identical, legacy Event text on stderr in typed mode, and a documented success/failure contract (`COMPATIBILITY.md`, `GUI-CLI.md`). Help text states the contract since `33de7f6`. Still open: a machine-readable failure record (not planned), and the full CLI verb/option comparison.
- **F29.** No full-build or game evidence. All fixtures are small disposable guides.

No Rust tests were created, changed or run. Frozen inline tests in `src/ui.rs` are expected to hang on `finish_worker` once the freeze lifts; details in the integration doc.

## Final archive diagnostic correction — 2026-09-27

Whole-package review found one P2 combined-path defect: known component archive errors lacked a severity marker, so the shared presentation displayed only an aggregate card and first-issue navigation could select the final generic failure. Scoped specification/quality review approves `099834c` and closes F1. That commit fixes the shared failed-outcome mapping with an idempotent `ERROR:` prefix, preserving component identity/detail. **Raw Report message values now include that prefix; serialized schema, stages, counters and warning behavior are unchanged.** This qualifies earlier transport-compatibility statements: schema stability does not imply unchanged diagnostic text.

Separate [focused archive-fix evidence](validation-archive-diagnostic-fix/README.md) records missing Extract **1/1/4**, corrupt Extract **3/0/3**, unchanged missing Copy **0/1/5**, native actionable cards, automatic/manual first-issue targeting and actual clipboard readback. Strict installs reject before snapshots/pending/game/mod content writes; expected lock files and empty state directories outside content roots are recorded, not hidden. The earlier47c3e67 native package retains its original binding. No Rust tests were created/adapted/run, and no full-parity or all-target compilation claim follows.

## Validation slice checkpoint — 2026-09-27

F08/F24/F25/F26 remain **partial**. Task 1 shared projection/progress and Task 1b availability/reached-option correction are independently approved through9d77f4b and88b2dd9 respectively. Renderer implementation is through47c3e67 in `src/ui/validation_page.rs`. Shared missing Copy now matches native C#0/1/5; strict install, standalone file-conflicts and completed-prefix safeguards remain intact. Pre-fix full3/0/3 replaces the earlier unsupported inferred2/0/4; partial2/0/1 remains historical measured evidence.

Latest small-window Stop/cancel interaction is confirmed at47c3e67, but **the final bounded native matrix and artifacts are archived, with the sole integrated review finding now closed by approved099834c**. Do not close page or domain acceptance from source review or old captures. [Integration checkpoint](validation-page-integration.md) records source-bound evidence, raw-versus-projected report rules, truthful warning copy, proactive Next gating, font deviations and remaining backend/UI gaps.

The latest user ordering defers every new, expanded or ported Rust test, including inline/temporary/custom Rust harnesses, until full C# feature parity. Preserve existing tests and historical evidence; obsolete worker fixture API adaptation is deferred. Earlier ledger references to future test creation do not override this ruling. Use authorized production builds and direct disposable CLI/native evidence now; production-only builds do not prove test-target compilation.

Objective: **Full C# feature parity**. This ledger does not reduce that objective to the first Rust implementation. No domain is complete merely because it compiles, loads its metadata, or passes a narrow test. New findings expand the ledger.

Reference: ModSync `d31ccbd3bfcddb4705917eafa6483715c166de3b` in the supplied C# worktree. `reference.json` records tracked source, tests, knowledgebase, and integration paths with their Git object IDs. It is a discovery index, not an implementation checklist. Reconcile reference changes before the final audit.

Status: **partial** = some behavior exists but full acceptance is unproven; **missing** = implementation absent; **unverified** = implementation exists but end-to-end evidence is absent. None of these means done.

| ID | Required behavior | Authoritative C# source under `src/` | Rust status | Evidence required for acceptance |
|---|---|---|---|---|
| F01 | TOML/JSON/YAML/XML/Markdown import/export, metadata, instructions/options, GUIDs, round trips, autodetection and stdin | Core `Services/FileLoadingService.cs`, `ModComponentSerializationService.cs`, `Utility/Serializer.cs`, CLI `ModBuildConverter.cs` | partial | Cross-read C# and Rust exports; compare normalized component trees and error behavior for every format |
| F02 | Community Markdown ingestion, spoilers, headings, links, generated IDs, preamble/epilogue | Core `Parsing/MarkdownParser.cs`, `MarkdownUtilities.cs`, GUI paste/import flows | partial | K1/K2 full and spoiler-free guide corpus; component/metadata comparisons plus GUI paste test |
| F03 | Natural-language directions, review flags, local archive generation and direct-Markdown preflight | Core `Parsing/NaturalLanguageInstructionParser.cs`, `DraftInstructionService.cs`, `Services/AutoInstructionGenerator.cs`, `DirectMarkdownInstallPreflight.cs` | partial | Port parser cases and compare real guide outputs; ambiguous drafts must require review |
| F04 | Component/option model, defaults, game fields, selections, category/tier filters, dependency selection and ordering | Core `ModComponent.cs`, `Option.cs`, dependency/processing services; GUI `ModSelectionPage` | partial | Port selection/graph tests; equivalent CLI selection and GUI state transitions |
| F05 | All instruction actions, overwrite/missing-source behavior, conditions, path placeholders, wildcard/case handling | Core `Instruction.cs`, `ModComponent.ExecuteSingleInstructionAsync`, `FileSystemUtils` | partial | Differential instruction fixtures using both real and virtual filesystems, including edge cases |
| F06 | Supported archives, large files, ZIP/7z/RAR/TAR/GZip, nested extraction, SFX, scratch layout, corruption behavior | Core `Utility/ArchiveHelper.cs`, `Services/FileSystem/*`, `PathHelper.ArchiveExtraction.cs` | partial | Real mod corpus, >512 MiB and nested archives, integrity/path rejection and memory/disk measurements |
| F07 | CleanList mandatory/substring/token matching and texture extension priority | Core `Instruction.cs`, `ModComponent.cs`, `Data/Game.cs` | partial | Positive/negative selection cases and identical final file sets |
| F08 | Structural/environment/readiness/virtual validation, reports, stages, warnings/errors, fix navigation | Core `Services/Validation/*`; GUI `ValidatePage` and validation dialogs | partial | Equivalent structured report output and GUI reports for passing/failing guides |
| F09 | External execution, HoloPatcher/KPatcher/OdyPatcher routing, arguments, Python resources and setup | Core `PatcherEngines.cs`, `Instruction.cs`, installation services; vendor integration | partial | Native subprocess contracts and real patch output comparisons for each supported backend/platform |
| F10 | Install coordinator, cancellation, state/progress, option failure, required install order | Core `Installation/*`, installation services; GUI installing pages | partial | Interrupted and failing real installs stop/restore/replay correctly; event/state comparisons |
| F11 | Per-component Git/immutable checkpoints, content-addressed data, provenance, restore/resume, browser UI | Core `GitCheckpointService.cs`, `ImmutableCheckpoint/*`; GUI `GitCheckpointBrowserDialog` | partial | Resume after interruption at each boundary, verify hashes/order, browse/restore through desktop and CLI |
| F12 | ResourceRegistry, cache, URL normalization, filename selection, direct acquisition | Core download/cache/link services | partial | Cached/uncached and stale-resource fixtures; archive-byte/checksum comparison |
| F13 | DeadlyStream, Nexus, MEGA, GameFront; authentication/free/premium handling and update metadata | Core `Services/Download/*DownloadHandler.cs`, `NexusApiClient.cs` | partial | Host-specific fixture tests plus authorized live downloads; CAPTCHA/manual/payment blocks must remain explicit |
| F14 | Concurrent download queue, progress/speed, cancellation, retries/timeouts, downloaded-file browser | Core download manager/orchestration; GUI download dialogs | partial | Controlled HTTPS failure scenarios and desktop status/stop/restart behavior |
| F15 | FOMOD discovery/XML/dependency rules/choices/priority/mapping, configure-before-install, CLI sidecar and GUI wizard | Core `Services/Fomod/*`, CLI FOMOD hosts; GUI `FomodInstallerDialog` | partial | Reference FOMOD test corpus; identical chosen files; conditional page flows and safe failure gate |
| F16 | Persistent profile schema, CRUD/clone/rename/activate, capture/apply component and option selections | Core `Services/Profiles/*`; GUI `ProfileManagerDialog` | partial | Shared C# JSON round trips, CLI verb tests, desktop actions, active profile integration |
| F17 | Settings schema, defaults, per-key merge, redaction, paths, themes, consent, API keys | Core `Services/Settings/*`; GUI `AppSettings`/settings dialog | partial | Existing settings read/write without unrelated data loss, secret redaction, desktop persistence |
| F18 | Managed deployment staging/manifests/hardlinks, ownership, undeploy/purge/profile switching | Core `Services/Deployment/*`, `Services/Installation/Managed*`; GUI mod management | partial | File ownership and restore tests including pre-existing files and multiple profiles; real filesystem probes |
| F19 | File conflict attribution, winners and competing components, preview/UI | Core `Services/Conflicts/*`; GUI `ConflictsDialog` | partial | VFS-derived conflict report matches C# for overwrite/order/option cases and renders in desktop |
| F20 | Merge matching/heuristics, per-field preference, exclusions/order, conflict resolution | Core `ComponentMergeService.cs`, `MergeHeuristicsOptions.cs`; GUI merge dialogs | partial | Port merge tests; full TOML+Markdown pipeline retains machine instructions and order |
| F21 | Component/instruction/option editor, reordering, dependency changes, URL/regex import and file browser | Core editor/manager services; GUI editor/management/import dialogs | partial | Round-trip edited guides and exercise every editor action in the native GUI |
| F22 | Mod update checks, stored results, version comparison and badges | Core `ModUpdateCheckService.cs`, update ports; GUI update indicators | partial | Controlled API fixtures, stored results and native UI actions |
| F23 | nxm:// and modsync:// parsing, registration, handoff, safe fetching and component resolution | Core protocol/download classes; GUI protocol integration | partial | URI edge/security cases and OS registration/second-instance delivery on each platform |
| F24 | Complete install wizard, preamble, Aspyr notices, optional pages, widescreen post-base stages, summaries | GUI `InstallWizardDialog`, `Dialogs/WizardPages/*` | partial | Native UI flow coverage for each conditional stage and equivalent install selection boundaries |
| F25 | Legacy main-window controls, menus, keyboard/accessibility, rich text and spoiler variants | GUI main window, controls, services, styles and converters | partial | Control/action inventory plus native keyboard/screen-reader/visual acceptance on supported OSes |
| F26 | Full CLI verbs/options/aliases, JSON contracts, exit codes, preload args and pipeline wrappers | Core `CLI/ModBuildConverter.cs`, GUI `CLIArguments.cs`, agent runbooks | partial | Table-driven invocation comparisons including early failures, stdout/stderr and exit status |
| F27 | Logging, telemetry consent/configuration/authentication, diagnostics | Core logger/telemetry services; GUI consent; telemetry-auth integration | partial | Consent-off produces no telemetry; configured events/diagnostics and secret handling match contracts |
| F28 | Packaging/resources, platform launch, patcher availability, settings migration, manual releases | project/build/workflow/resource files and release runbooks | partial | Native artifacts bound to tested commit; launch and resource probes on supported targets |
| F29 | Full K1/K2 equivalence to reference C# and manual installs | full-build runbooks, reference instruction/Markdown guides, test corpus | unverified | Isolated reference and Rust installs with identical inputs/selections; per-step hashes, final file/patch semantics, GUI/CLI equivalence and game smoke evidence |

## Current execution sequence

1. Remove the first port's archive/file-size ceiling with streamed content, then match archive/layout and CleanList semantics. This enables real full-build inputs; it is not the whole goal.
2. Complete formats, ingestion, selection and CLI contracts against C# test fixtures.
3. Implement profiles/settings, conflict analysis, managed deployment and FOMOD using shared core functions exposed to both desktop and CLI.
4. Implement acquisition/update/protocol integrations and checkpoint/resume parity.
5. Complete editor, wizard, main-window, diagnostics and packaging surfaces.
6. Execute requirement-by-requirement native/differential/full-build acceptance. Keep any unsupported, weakly tested or external-blocked requirement open.

The C# best-effort install flags must be reconciled with the user's explicit rule against skipping/backfilling failed guide steps. Do not silently reproduce unsafe install-order behavior or silently remove a required feature; record and resolve the contract before marking F10/F26 complete.

## Continuation state

- Previous turn: **progress** — created the initial port, committed it, and verified native CI on all three operating systems. This proves the initial implementation's tests only.
- Current goal is active and unbounded. No full-parity completion claim is justified.
- Current C# and Rust worktrees were clean when this ledger was created. Source reference remains the supplied C# worktree; do not substitute unrelated worktrees or protected game baselines.

### Streaming and CleanList checkpoint

- Streaming ZIP/7z/RAR extraction and copy replace memory payloads and the 512 MiB ceiling. TAR/GZip and embedded 7z SFX decoding are added. Archive verification discards decoded output without spooling it. CLI and desktop share scratch/budget options.
- CleanList now uses the C# mandatory-prefix, substring, stop-word and token-overlap rules and comma splitting; the unnecessary CSV dependency was removed.
- Local archive/path/cancellation/recovery/patcher fixtures pass. The explicit 513 MiB extraction/copy check passed in 18.23 seconds with measured command peak RSS 110,384 KiB. This is a focused regression, not full-build parity proof.
- F06/F07 remain partial: real mod corpus/differential acceptance, empty-directory behavior, other SFX formats and texture-priority edge cases remain open. Desktop controls compile; their new interactions have not yet received native GUI acceptance.

### Structured format checkpoint

- JSON exports now use the C# component-array shape. YAML uses separate metadata/component documents. XML supports current/legacy roots, repeated and singleton collections, URL-keyed resource maps, escaped names/text, CDATA and safe entity handling. All-unselected input selects top-level components, matching the reference loader.
- CLI conversion supports `--input`, `--output`, `--format` and `--stdin`, while retaining positional input/output. Shared load/save adds aliases, unknown-extension detection, UTF-8 replacement handling and atomic output creation.
- Five format integration tests include exports generated by the pinned C# source. `tools/check_formats.py` runs both implementations in both directions and compares component identity, categories, selections, ordering constraints, instructions and options. `format-check.json` records the successful run and source hashes.
- Direct C# execution exposed two required serialization details: option `Guid` must precede other fields, and singleton arrays need Newtonsoft's `json:Array` annotation to retain component dependencies. Exit status alone was insufficient: the C# loader can silently skip a failed component.
- K1 (189 components) and K2 (145 components) round-trip through JSON/YAML/XML with equal Rust component models. This is serialization evidence, not install equivalence.
- INI appeared in CLI help but is rejected by the reference serialization switch; it is not a guide-format implementation requirement. Patcher INI handling remains part of F09.
- F01/F04/F26 remain partial. Prose Markdown import/export, GUID validation/error recovery edge cases, all settings and legacy resource migrations, complete command options and native GUI acceptance remain open. JSON/XML do not carry root build metadata in the C# formats; use TOML/YAML for that metadata. Unsupported/malformed documents still fail closed in Rust.
- The Windows RAR handle fix is proven by all three native jobs in run 35050607191 at 0519d9e5f0b25e2ba3580c1bab8c36ecd16063c7. That run predates these format changes.

### Markdown import checkpoint

- The shared parser reads default component sections, bold/plain/admonition fields, name links, category/tier, explicit YAML/TOML component comments and exact named masters. Authored instructions continue through the same validated engine. Unresolved masters are retained and block installation; unannotated prose never receives executable instructions.
- Two regression tests compare the captured C# conversion and validate/install the explicit fixture in isolated trees, reject malformed metadata, preserve field boundaries and prevent prose-only or unresolved-master installation. Tier normalization now applies to every format through the shared component loader.
- The preceding structured-format commit `da7758858216d2c51a1fe46b2e098f91cb751814` passed Linux/macOS/Windows CI run 35051978262. That run predates Markdown changes.
- F02 remains partial and F03 remains missing: custom profiles, legacy metadata, full-corpus field equivalence, special content sections, GUI paste and natural-language drafting/review still require implementation and evidence.
- Live K1 prose differential: 186 components in both implementations; 15 of 16 compared text/collection fields match across all components. Nine screenshot-field differences remain (reference regex also captures the word inside prose). Language notes containing commas and bold platform subheadings inside admonitions were fixed from this comparison. `markdown-check.json` records exact inputs, source hashes, differing values and proof boundaries. Other desktop corpus files parse in Rust; their complete C# comparisons remain open.

### Draft-instruction checkpoint

- Ported the 56-pattern C# draft pipeline into shared Rust logic: clauses/conditions, commentary and gap detection, path inference/ranges/exclusions, named cleanlists, download recommendations, sanitation and review notes. File-open/install ingestion drafts Markdown automatically; conversion exposes `--parse-directions`. CLI and desktop share explicit review and the engine blocks pending reviews.
- The pinned raw C# draft service and Rust matched across 669 component records (19 focused cases and K1/K2 full/spoiler-free component inputs), including 268 drafted instructions, option fields excluding generated GUIDs, warnings, gaps and conditional notes. `draft-check.json` records input/source hashes and exact scope. This is not full ingestion or install proof.
- The oracle assigns prose fields directly after deserialization to isolate draft behavior; C# JSON loading drops some existing installation warnings, a separate F01 discrepancy. Regex fallback quirks (one-letter folder paths, numeric-range root placeholders) are preserved as reviewable artifacts and are not certified correct instructions.
- Native X11 desktop acceptance proved automatic drafts, rejection before review, visible steps/notes, explicit acceptance, successful validation and isolated file installation. 31 local regular integration tests pass in both feature configurations. Windows CI for the preceding Markdown commit failed only its LF-specific malformed-fixture mutation; the test now normalizes CRLF first.
- F03 remains partial: local-archive instruction generation, direct-Markdown 4GB/widescreen preflight, complete editing/review of unresolved drafts, all source parser edge cases and full corpus installs remain open. The reference's direct-Markdown preflight allows flagged drafts, conflicting with its never-auto-trust draft-service contract; Rust currently requires explicit review. Reconcile this before declaring workflow parity.

### Local archive-generation checkpoint

- Shared Rust inference and `convert --auto-generate-local --source-path` cover fresh components with one resource link. Loose files, folder choices, patcher configuration and namespace options, hybrid labels and the texture-deduplication special case are implemented. Existing instructions are preserved; unsupported existing-option and multiple-link cases produce explicit reports.
- Sixteen isolated archive fixtures match the pinned C# service's raw instruction/option output. `archive-generation-check.json` records source and fixture identity and exclusions. Archive-name scoring is tested separately in Rust; the reference comparison bypasses JSON resource URL canonicalization to isolate generation.
- An isolated actual install proves selection of one generated folder option and preservation of the other. Validation, corruption, unsafe names, cancellation and expansion-budget tests cover no-write/no-partial-model behavior. The reference may delete corrupt inputs; Rust preserves them. Matching generated patcher paths is not patcher-install proof.
- F03 remains partial: authored-option/instruction merging, multiple-link selection, non-archive EXE fallback, URL-based generation, desktop inference controls, full direct-Markdown preflight and full corpus installation remain open.
- The preceding draft commit `e51cf477d48596b3aea9f9f95fcf5a0e2ea4fd7c` passed Linux/macOS/Windows CI run 35054774351, including each native 513 MiB regression. That run predates this archive-generation change.

### Authored archive-option checkpoint

- Added reference-style instruction equivalence, authored-option reuse and overlap consolidation. The first option retains identity, metadata and selection; missing instructions merge and Choose references are rewritten. Existing top-level instructions still skip auto-generation, matching the C# local-processing entry point; direct augmentation of those instructions remains open.
- Twenty raw C# archive-generation fixtures match, including four authored-option cases. The probe assigns authored Source strings directly to separate inference from JSON-loader path rewriting. The reference can repeat one broad option ID in a new Choose; output equivalence is not correctness proof for that case.
- A separate actual install verifies preserved authored GUID/selection and resulting texture bytes. Native CI run 35055903581 for preceding commit `7aeb04df44b799e8f87ba751eeae04cd7aa5ad49` completed successfully; it predates option merging.
- F03 remains partial. This checkpoint removes the existing-options skip restriction; multiple links, executable fallback, desktop integration, direct-Markdown preflight and full corpus/manual-install acceptance remain open.

### Resource insertion-order checkpoint

- ResourceRegistry retains insertion order across the component model, JSON/TOML/YAML/XML conversion and Markdown link collection. Explicit order assertions include nested options. The TOML preserve-order feature is enabled; unrelated build metadata remains unchanged in its existing model.
- Removed the multiple-resource skip. Local generation uses the first link and does not search later links if the first has no archive match. Two live C# local-processing fixtures prove these behaviors; the expanded generation comparison has 22 matching fixtures.
- F01/F03/F12 remain partial: full format fidelity, resource migrations and URL normalization, cache/host behavior, remaining generator paths and full-build acceptance are still required. Previous commit `5791c7c9a43d52c0d4ea66bd081cf498c2670e78` passed native CI run 35056506242 before this change.
- The explicit cross-application order check found and fixed a YAML interoperability defect: C# flattens ResourceRegistry and recovers links through ModLinkFilenames. Rust now emits that compatibility map, preserves nullable file-selection flags and retains unrelated legacy URLs. Top-level resource order matches both directions in all four structured formats. `resource-order-check.json` also exposes C# dropping option-level registries; nested-option preservation is Rust-only evidence and F01/F12 remain partial.

### Executable-inference checkpoint

- Added Execute inference and Executable Installer labeling when EXE archive reading fails. Cancellation does not produce an Execute step; the texture-dedupe special case retains precedence. Inference and validation do not run the input, and installation still requires explicit external-program trust.
- Of 28 raw C# fixtures, 27 match. A ZIP renamed to `.exe` remains different: C# chooses its 7z reader and eventually emits only Extract; Rust sniffs ZIP content and also emits Move. The complete difference remains in `archive-generation-check.json`, with an exact fixture annotation rather than an unrestricted comparison waiver. F03 remains partial until this behavior is resolved, alongside remaining generation paths and full-build acceptance.
- Native CI run 35057519588 passed for preceding commit `9a8a740bea85f3ed7f00480aa3b9f4887934c0a2`; it predates this change. No real installer execution is claimed by the inference fixtures.

### Direct archive API augmentation checkpoint

- Public direct generation augments existing steps; automatic local generation continues to skip components with top-level instructions. Equivalent steps, authored Copy/Move coverage, existing Choose references and option identity are preserved in the covered cases.
- Six captured direct-API C# cases extend the corpus to 34. Rust integration tests compare these objects; the CLI checker separately reports 27 exact matches and the existing unresolved renamed-ZIP EXE difference.
- An isolated install verifies idempotent augmentation and the authored Modules Copy destination, source retention and absence of an unintended Override move. F03 remains partial.
- Preceding commit `967f6c81cbc858cb30c3985160b7a3f2a4657758` passed native CI run 35058007498.

### Legacy resource migration checkpoint

- F01/F12: implemented ordered legacy ModLink/ModLinkFilenames migration, modern-resource precedence, nullable filename flags, empty-map URLs and normalized-URL identity hashes. JSON filename key conversion follows the reference.
- Fifteen live C# JSON-loader cases: fourteen exact matches and one explicitly retained modern Files-map loss difference. Rust round-trips the resulting data through all four structured formats. These receipts do not certify all URI/format/option behaviors or download/cache integration.
- An isolated legacy-guide flow reaches local archive inference, validation without game writes, and installation with expected texture bytes. F01/F12 remain partial.
- Native CI run 35058728745 passed for preceding augmentation commit `78774c295fc00be4be5a4ddc6b62fbf9fdba042b`.

### Duplicate legacy URLs

- Corrected filename-map consolidation before legacy resource insertion: last map wins, first key spelling/order survives, and existing registry/ModLink precedence is unchanged. Expanded the live JSON-loader corpus from 15 to 18 cases. This closes the observed duplicate-URL mismatch; F01/F12 remain partial, including the modern Files-map difference.

### PrimaryUrl registry migration

- Added old hash-key to PrimaryUrl migration before legacy link precedence, with collision and empty-value behavior verified against three additional C# fixtures. Resource identity metadata survives migration.
- Extended the isolated inference/validation/install flow to consume an old hash-key registry. F01/F12 remain partial; modern Files loading, all URI forms, format-specific deserialization and host/cache integration remain open.

### Modern resource metadata proof boundary

- Added a four-format live loader inspection with raw outputs and input/DLL hashes in `modern-resource-formats.json`; analysis is in `modern-resource-formats.md`.
- C# map loss extends beyond the earlier JSON fixture. YAML fallback also loses original resource identity, size and timestamps. Rust has an independent XML nested-number-to-string round-trip gap.
- Added regression coverage for JSON/TOML/YAML metadata preservation and the exact remaining XML gap. F01/F12 remain partial; URL-order and component/instruction projections are insufficient evidence for resource metadata or consumer behavior.

### Download filename consumer checkpoint

- F12/F26: added shared URL-enabled and filename-selection logic plus read-only `download-plan` JSON output. Selection follows explicit file flags and the C# unsatisfied-Extract-pattern consumer, including immediate option instructions.
- Twelve live direct-consumer fixtures match. Probe metadata is restored after C# loading, so this proves consumer behavior separately from known loader loss. `download-plan-check.json` records the scope and pinned reference identity.
- Added CLI non-mutation coverage; shared wildcard extraction retains archive-generation regression coverage. F12/F26 remain partial: no host/queue/cache/desktop integration or complete readiness contract is implied.

### Profile persistence checkpoint

- Added shared profile schema/store with CRUD, capture/apply and CLI profile actions. Activation merges the active name into settings. Atomic profile replacement and cooperating-writer serialization are exercised locally.
- Live C# comparison covers seven CLI operations per implementation and cross-reading profile files in both directions; `profile-check.json` excludes runtime timestamp equality, defaults, GUI and install integration.
- F16 remains partial: automatic profile-backed install/directories, desktop CRUD/activation, platform-default storage, complete malformed-schema and Unicode/platform behavior remain open. Exported all-unselected guides still encounter the existing loader fallback on reload.

### Profile-backed validation and installation

- Connected stored profile paths and selections to CLI validation/install through the existing shared engine. Explicit arguments override defaults; named and active-profile modes are mutually exclusive and require a settings directory.
- Isolated installs prove selected option file bytes, disabled-component exclusion, explicit overrides and preservation of profile/settings/input files. All-unselected profiles stop before snapshot creation and game mutation; missing profile defaults produce explicit errors.
- F16 remains partial: these are Rust CLI integration tests, not C# full-install or desktop proof. Settings defaults, automatic desktop activation, malformed schema and all platform/Unicode cases remain outstanding.

### Desktop profile activation progress

F16/F24: the desktop now exposes capture and activation through the shared profile store, applying selections to the loaded guide and restoring game/mod directories. Activation invalidates validation and install confirmation. A state test covers successful capture/activation and malformed settings without displayed-state mutation. Native interaction proof, remaining profile manager controls and startup/default storage behavior remain unverified or missing. Neither domain is complete.

### Desktop profile management progress

F16/F24: explicit listing/selection, clone, rename and confirmed delete controls now reuse the shared store. Deleting the active profile clears its setting without changing installed files. Tests cover desktop state transitions and settings preservation/error boundaries; native interaction proof remains open. Default storage, automatic startup activation and full schema/platform equivalence remain incomplete.

### Default profile storage and startup requirement correction

F16: shared default settings-directory lookup now serves profile CLI, profile-backed install/validate and the desktop field. Live Linux C#/Rust create operations chose the same path under an isolated existing XDG_CONFIG_HOME. Native macOS/Windows folder comparison, missing/unreadable bases and legacy settings fallback remain open. Earlier automatic-startup-activation notes overstated the pinned reference: ApplyProfile's only concrete caller is the profile manager dialog. General settings restoration remains in F17; no automatic profile reapplication requirement has been established by current source evidence.

### Legacy profile settings lookup

F16/F17: profile settings now honor the reference's legacy KOTORModSync fallback and current-file precedence. A live CLI comparison matched activation's managed fields and confirmed legacy input preservation. General GUI settings restoration/migration and malformed-schema equivalence remain incomplete.

### Desktop directory preferences

F17 is now partial: sourcePath/destinationPath load at desktop startup, explicit launch paths take precedence, and the user can save directories with unrelated JSON retained. `gui --settings-dir` supports an isolated/custom settings store. Tests cover directory restoration, malformed-field rejection, casing, nested-field preservation and corrupt-file nonreplacement. This is state-level evidence only; themes, the remaining settings fields, automatic save lifecycle and live native interaction remain incomplete.

### Theme settings progress

F17/F24: the desktop now persists and restores the three canonical C# theme paths, with Light as the absent/null default. It preserves unrelated settings and retains the displayed theme on save failure. Shared-store and egui-context tests cover these behaviors. Full Avalonia appearance and the reference theme-to-TargetGame effects are not implemented or claimed; native interaction proof remains outstanding.

### Native Linux profile/settings workflow proof

F16/F17/F24 now have bounded native interaction evidence at `native-profiles/receipt.json`, tied to commit 13f50111ec0eb6950ef73a21f29491fe5a6278a4 and its binary hash. Screenshots cover profile listing, deletion confirmation, validation, installation and restart. Verified profile/settings disk state and exact installed bytes support those specific paths. This does not close any domain: full guide, platform, accessibility, visual and remaining settings requirements still apply.

### Tier/category selection progress

F04/F24: added shared additive tier/category selection rules and desktop exact filters/buttons. Reference source defines cumulative tier priorities and widescreen exclusion for bulk selection. Regression coverage exercises those rules. Native controls, multi-category dialog, reference text/spoiler filtering, Unicode comparison and the separate widescreen phase remain incomplete or unverified.

### Spoiler-aware display and search

F04/F24: authored spoiler-free text and reference fallback names/descriptions now drive component/option display and selection search. Regression tests verify hidden-description exclusion, fallbacks and truncation without mutating guide data. Persistence of the toggle, complete tooltip/detail handling, other raw text surfaces, Unicode edge cases and native interaction remain outstanding.

### Spoiler preference persistence and full description access

F17/F24: spoilerFreeMode now persists and loads at startup; normal description tooltips retain full text and spoiler tooltips exclude the hidden normal description. Store/projection regressions verify those boundaries. Native toggle/restart/hover and full reference tooltip metadata remain unverified or incomplete.

### Multiple-category chooser progress

F04/F24: a desktop checkbox chooser now applies several categories atomically at the UI state level through the shared additive selector. Existing selections survive and validation is invalidated. The chooser's category inventory excludes widescreen-only components. Native interaction, reference modal-window semantics, Unicode ordering and the separate widescreen phase remain open.

F04/F24 modal checkpoint: category selection and install confirmation now block background input using egui Modal. Native Linux screenshots and disk assertions in `native-modals/receipt.json` cover category apply, background-click cancellation, Escape and confirmed fixture installation. This supersedes the earlier nonmodal limitation for the exercised paths; it does not establish full wizard, platform or accessibility parity.

F04/F24 option-description checkpoint: the visibility guard now follows displayed text, so options with empty raw descriptions still show authored or generated spoiler-free descriptions. An egui text-shape regression exercises the actual option renderer. Full option-card tooltips, accessibility and wizard parity remain open.

F04/F24 tooltip checkpoint: shared comprehensive tooltip content now reaches component names, options and descriptions. Source-grounded regression covers all content sections and spoiler handling. Native hover/accessibility behavior remains unverified; neither domain is complete.

F04/F24 filtered bulk-selection checkpoint: row rendering and Select all share the search/tier/category predicate; bulk selection excludes widescreen and Deselect all preserves it. Clear filters is available. Shared-rule regression passes; separate widescreen UI and native interaction verification remain open.

F04/F24 summary checkpoint: base-build selection totals and active-filter summaries now follow reference counting/content rules, covered by shared-rule regression. Native layout, required/optional badges and separate widescreen flow remain open.

F04/F24 badge checkpoint: required/optional counts now follow the reference selected-base dependency-presence rule, with regression coverage and explanatory desktop tooltips. Earlier missing-badge notes are superseded for content/counting; native rendering and the broader wizard remain unverified.

F04/F24 native selection receipt: `native-selection/receipt.json` binds Linux input/screenshot checks to commit 253ba18 and binary hash. Verified filtered selection, clear filters, counts, preserved widescreen selection and safe tooltips. Missing emoji glyphs and untested platform/accessibility/dropdown paths remain explicit limitations.

F04/F24 empty-state checkpoint: no-results messaging and filtered detail visibility verified through native Linux search and clear-filter input; see `native-empty-selection/receipt.json`. Selection remains unchanged. Other platforms and accessibility remain unverified.

F04/F24 grouping checkpoint: category headers, ordered rows, per-group selection and expand/collapse controls added. Shared display-order regression passes without changing guide order. Native interaction, expansion-state lifecycle and separate widescreen flow remain open.

F04/F24 native group checkpoint: expand/collapse, filtered group selection and counts verified on Linux; see `native-groups/receipt.json`. Full expansion lifecycle, separate widescreen flow and other platforms remain open.

F10/F24 widescreen investigation: `WIDESCREEN.md` and source hashes distinguish base UI counts from actual reference execution. A possible duplicate invocation requires live reproduction before implementing phase splitting; this is new source evidence, not phase completion.

F10/F24 reference defect evidence: isolated bulk/single service calls reproduced widescreen re-execution despite Completed state. DLL-bound receipt in `widescreen-oracle/`; GUI/checkpoint/dependency/failure paths remain open.

F10/F24 dependency oracle: initial wide deselection deferred installation; wide-only context failed a completed-base dependency while full context succeeded. Service-level receipt in `widescreen-dependency-oracle/`; no phased Rust implementation or checkpoint proof claimed.

F10/F24 execution primitive: install_remaining preserves dependency context while skipping a verified completed prefix, covered by no-replay/invalid-prefix regression. Caller completion verification, phase UI, persisted identity and failure recovery proof remain open.

F10/F24 remaining-phase recovery checkpoint: cancellation after a suffix write, blocked retry, restoration of completed-base bytes and ordered suffix-only replay are covered by an executable integration regression. Durable completion identity and phase UI remain unfinished.

F08/F10/F24 suffix validation: shared prefix validation now serves dry-run and installation; consumed-base-source regression demonstrates no replay and no filesystem/recovery mutation. Durable completion identity and UI integration remain open.

F10/F11/F24 completion checkpoint: installs now save root/definition/game-hash evidence, suffix APIs verify it, and recovery restores the prior receipt. F11 advances from missing toward partial receipt support only; per-component checkpoints/provenance, crash durability, GUI transitions and full reference resume semantics remain open.

F24 phase wiring checkpoint: desktop base-only jobs transition to widescreen selection and receipt-checked remaining execution, then completion. Base inputs lock across the boundary and pending failures block skipping. Native end-to-end proof, warning page, restart/resume and empty-base handling remain open.

F10/F24 native phased-install checkpoint: two-phase success, deferred wide writes, base dependency context and no base replay verified with native input and exact-byte checks. Evidence in `native-phases/`; failure/restore/skip/restart and full builds remain open.

F24 notice checkpoint: optional widescreen guidance now precedes selection after base completion, with metadata aliases, default/blank/null semantics and one-session dismissal covered by a desktop regression. Native notice interaction and Markdown warning extraction remain open.

F02/F24 Markdown warning checkpoint: warning and post-widescreen epilogue extraction match an isolated pinned C# parser fixture (`markdown-warning-oracle/`); desktop consumes extracted metadata. Missing-heading and CRLF regressions pass. Widescreen component inclusion/classification and native notice proof remain open.

F02/F24 classification checkpoint: prose WidescreenOnly flags now match reference section positions, with a pinned C# output comparison including a later non-widescreen component. No change to Mod List inclusion boundaries or ordered-prefix installation guards; full corpus/native end-to-end proof remains open.

F24 Finish checkpoint: successful completion can return to setup without an app restart. Phase and validation state clear while guide/directories/logs persist, covered by a desktop regression. Native repeated-install proof and restart/resume remain open.

F08/F24 validation lifecycle correction: early job-setup failures now invalidate prior validation/confirmation. Regression covers all job types and removed directories without launching workers. Native failure-state proof remains open.

F01/F02/F24 metadata export correction: extracted Markdown wizard text survives TOML/YAML save/reload in the reference metadata layout, with default format-version marker and existing overrides preserved. Regression covers content and stable re-export; live cross-language re-import remains open.

F01/F02/F24 cross-language export checkpoint: pinned C# deserialization of Rust TOML/YAML exports preserves exact wizard content and component count for the saved Markdown fixture. Evidence and verifier: `content-export-oracle/`. Full corpus/native proof remains open.

F01 YAML marker checkpoint: metadata comments without format-version fields are recognized with mapping validation. Regression covers scalar separator text, CRLF and a leading directive; broader YAML grammar and live differential proof remain open.

F01 YAML regression correction: successive directive-bearing documents preserve their boundaries, verified against native parser acceptance and guide results. This repairs a regression introduced by metadata-comment support.

F15 investigation checkpoint: full FOMOD contract mapped in `FOMOD.md`; isolated reference selection probe covers five group types and reproduces required-sibling bypass. F15 remains missing in Rust, with parser/session/mapping/gate/UI/CLI/install acceptance required.

F15 implementation checkpoint: shared group-selection primitive matches the pinned five-mode oracle, protects required selections, and tests empty/conflicting groups. F15 is partial at library-rule level only; archive parsing, session visibility, configuration mapping/gates, UI/CLI and installation acceptance remain required.

F15 choices checkpoint: reference JSON schema and archive lookup implemented; explicit named group application is atomic and tested for required/cardinality/ambiguity failures. Full session routing, Unicode matching, parser/mapping and CLI/GUI installation integration remain open.

F15 named-step checkpoint: choices apply atomically across unconditional steps/groups, with regression coverage for late failures, duplicates, ambiguity and invalid untouched defaults. Conditional visibility/flags and package parser/mapping/integration remain open.

F15 visibility checkpoint: shared flag conditions and ordered propagation implemented, including hidden-step contributions and visible-only final validation. Source-derived behavior has an executable Rust regression; live differential, file/game dependencies and XML/mapping/UI/CLI remain open.

F15 XML selection checkpoint: ModuleConfig selection projection connects authored XML to shared group/flag logic, with malformed/DTD/unsupported-dependency rejection tests. Complete module/file metadata, conditional mapping, live differential and installation integration remain open.

F15 presentation metadata checkpoint: plugin descriptions and image paths reach the shared selection model, including mixed XML text and empty defaults. Rendering and safe archive-contained image resolution remain open.

F15 module-data checkpoint: XML retains required/plugin/conditional file records and priorities, with regression coverage for raw paths, order and numeric defaults. Safe mapping, priority execution, remaining dependency kinds and installation proof remain open.

F15 file-planning checkpoint: selected visible files and reference-compatible conditional attachment order are computed without writes. Unsupported mapper patterns reject rather than disappear. Live mapping differential, path containment and engine/UI/CLI integration remain open.

F15 engine mapping checkpoint: selected compatible file copies map through existing safe relative paths into shared instructions; isolated preflight/install byte test passes and unsafe/rename/folder inputs reject. Folder/rename semantics, archive gating and UI/CLI integration remain open.

F15 folder checkpoint: actual reference install establishes nonrecursive wildcard behavior; Rust mapping and isolated engine test reproduce top-level-only assets. Evidence in `fomod-folder-oracle/`. Recursive semantics and remaining end-to-end integration are not certified.

F03/F26 direct-Markdown checkpoint: `install --direct-markdown` now requires a `.md` guide and applies a shared preflight that deselects 4GB Patcher and widescreen-only components, reports generated draft count, and rejects selected components with no executable actions before creating the engine. Rust also rejects generated drafts requiring review at that same boundary, preserving the port's explicit never-auto-trust rule rather than C#'s permissive draft execution. A library regression covers special-component skips and both empty/unreviewed components; CLI regressions prove an unreviewed Markdown draft creates no game files and that reviewed actions install while 4GB/widescreen entries do not. Direct-Markdown remains partial: reviewed full-guide execution, C# differential coverage and K1/K2 full-build acceptance remain open.

F09 backend-selection checkpoint: `install --patcher-engine ody-patcher|k-patcher|holo-patcher` selects the manual `--patcher` executable; HoloPatcher is the reference-compatible default. Direct-Markdown requires explicit OdyPatcher, matching the C# CLI contract. OdyPatcher uses C#'s headless contract, `--install --game-dir <destination> --tslpatchdata <data> --cli -y`; KPatcher adds `--console` on non-Windows; HoloPatcher uses the shared install/game/data arguments. The installer removes stale patcher logs and rejects a successful subprocess whose new log reports `Error:` or `[Error]`. The existing isolated named-INI fixture asserts the Ody argv shape, temporary `tslpatchdata` replacement, removal of `namespaces.ini` from the staged copy, patch output, nonzero exit propagation, no later component execution, and restoration of the original configuration. A Unix KPatcher stub verifies the console prefix and absence of Ody flags. Packaged resource setup/assets, patch-byte differential tests, native desktop interaction acceptance, and native backend acceptance remain open.

F15 prompt-state and registry-gate checkpoint: Rust reads and writes C#'s per-resource `handlerMetadata.fomodPromptStatus` contract for case-insensitive archive filenames, preserving `dismissed`, `configured`, and `warned` policy. Applying or merging explicit Rust choices records `configured` only on a matching existing resource entry. The shared gate now independently inspects existing archive-like `ResourceRegistry.Files` inputs, so a FOMOD registered under a nested filename cannot evade validation by omitting an Extract instruction. This does not make status sufficient for installation: Rust still requires explicit `FomodChoices`. Rust now fails closed for a missing registered archive after prompt state and for an unreadable registered archive. C# missing-archive warning versus failure classification without prompt state, structured issue shaping, configured-without-merged-instructions warnings, dependency-scope expansion, download orchestration, restart/recovery behavior, and differential serialization evidence remain open. Rust desktop direct-download inspection now honors configured/warned/dismissed prompt state for matching resource entries and persists dismissal only after positive FOMOD detection, but this is not full C# multi-host post-download orchestration.

F08 structured-report discovery: C# `InstallationValidationPipeline` produces ordered `Environment`, `Conflicts`, `InstallOrder`, `ComponentValidation`, `FomodConfiguration`, and optional `DryRun` stages. Each stage has `passed`, `hasWarnings`, `summary`, and messages; the aggregate carries success, exit code, error/warning/passed counts, component count, input path, and a structured error form. Rust currently has only fail-fast engine events plus a separate conflict report. Its `--json` stream is therefore not a C# validation-pipeline report and must not be represented as one. Rust now has an initial `--validation-report` JSON path with Environment, Conflicts, InstallOrder, ComponentValidation, and FomodConfiguration stages built from shared engine/conflict facts, plus passing/failing fixtures. CLI stage selection now matches the reference: the first three stages require `--full`, while `--dry-run-only` skips component archive validation. The FOMOD stage reuses Rust's explicit `FomodChoices` requirement and archive inspection to reject detected archives without choices; it is not yet C#'s persisted prompt-status/archive-registration gate. Rust `--validation-report --dry-run` now adds the optional `DryRun` stage and `dryRun` aggregate from a fresh non-mutating engine simulation; regressions prove the result and preserve the game tree. The simulation continues after failing instructions and components, returning ordered issues without changing fail-fast install behavior. Existing Copy/Move/Rename destinations with `Overwrite=false` and missing Delete targets now produce the reference warning categories/messages and remain nonfatal no-ops during real installation. Other VFS warning classes and more than one diagnostic from a single instruction remain open. A pinned, isolated C# formatter oracle proves Rust's report and early-error JSON shapes, including null omission, explicit issue arrays, stage messages, optional issue fields and nondefault exit codes; the receipt and captured output are in `validation-json-oracle/`. A separately isolated live CLI oracle in `validation-pipeline-oracle/` proves exact passing Copy-pipeline output, exit status, empty stderr, and read-only-tree behavior for both implementations. Failure, warning, full-validation, filtering, and dry-run-only live comparisons remain open. The Rust desktop now renders shared report stage rows and counts, copies the JSON report, and scrolls to the first error before falling back to the first warning, matching the reference action. Native interaction evidence remains open.

F08 error-report checkpoint: `--validation-report` now emits the C# `ValidationPipelineJsonFormatter.SerializeError` shape when setup fails before any pipeline stage runs (guide load, selection, FOMOD-choices parse, or engine construction). The new `validation::ErrorReport` serializes `success:false`, `exitCode:1`, and the four zeroed scalar counters, and omits `inputPath`, `stages`, and `dryRun`, matching the reference's camelCase + WhenWritingNull contract. A failing pipeline report (stages ran but reported errors) now prints the report and exits nonzero with no extra stderr line, instead of the previous plain-text `anyhow` error. A CLI regression proves a missing guide produces the structured error report as the sole stdout document with empty stderr and a nonzero exit. A pinned C# formatter probe confirms the error shape with a nondefault exit code and all zero counters. Failed dry runs serialize the actual `componentName`, `componentGuid`, and one-based `instructionIndex`; missing-source failures also identify the authored source as `affectedPath`. Unknown `solution` values are omitted. Successful dry runs retain the reference's explicit empty `issues` array. Multi-diagnostic instructions, remaining VFS warnings, resolved affected paths for other failure classes, solution generation, and live end-to-end pipeline comparison remain open.

## Shared GUI/CLI phases checkpoint

Tasks 1–3 independently passed specification/quality review through `3d106ef`.
Whole-package final review passed with nonblocking P3 notes; Task 4 is complete.
`phase::prepare` now supplies both frontends; CLI `validate`, `conflicts`, and `install` accept default All, Base and
Remaining. Shared remaining conflicts verify receipt roots, definitions and game
hashes. All-mode pipeline reports retain existing failure stages and inputPath.
[Integration evidence](cli-phases-integration.md) archives raw executable outputs,
manifests/events/receipt hashes, reviews, source binding and exact limitations.
Final implementation checks passed: 229 all-target tests, 199 no-default-feature
tests, one intentionally ignored fixture in each, strict Clippy and formatting.

Differential tests compare identical post-selection All/Base and stable Remaining;
GUI retained historical Base definitions can survive suffix choices edits while
separate CLI reconstruction may reject because external metadata embeds the entire
choices payload. Explicit Base uses ordinary repeat semantics (Copy can repeat,
consumed Move rejects); Remaining cannot replay an exhausted suffix. Pending failure
requires verified restore before ordered retry. Explicit phased `--direct-markdown`
is excluded; ordinary Markdown retains receipt verification. Task 1's original P3
duplicate-test isolation cleanup is accepted as deferred nonblocking P3; Task 3
independently covers the specific duplicate guard. No feature row is promoted to
complete: native pre-fix wizard evidence is unchanged, and full C# parity, real full builds, restart behavior,
accessibility and cross-platform acceptance remain open.

### Settings verb and profile schema edge cases

F16/F17: the CLI now has `settings --action list|get|set` with the reference's directory resolution (never the legacy file), value parsing, text and `--json` output, `--reveal-secrets` and 0600 writes. Masking covers every case variant of `nexusModsApiKey`; the reference prints `NexusModsApiKey` in clear. Profile activation and listing read the managed fields case-insensitively, and a wrong type resets them, as C# does. Profile and settings files with a UTF-8 BOM now load. Capture writes C#-form GUIDs and apply matches braced or uppercase guide GUIDs. Live DLL comparisons are in `settings-check.json`; the key and behavior inventory is `profiles-settings-csharp-audit.md`. Rust still refuses to replace an unreadable settings file where C# overwrites it. Typed GUI settings load/apply for the remaining keys, theme-to-game mapping, non-en-US number parsing and native macOS/Windows folders remain open. Neither domain is complete.
