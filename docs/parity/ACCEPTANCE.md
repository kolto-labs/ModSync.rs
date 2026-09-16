# Full C# feature parity — active acceptance ledger

Objective: **Full C# feature parity**. This ledger does not reduce that objective to the first Rust implementation. No domain is complete merely because it compiles, loads its metadata, or passes a narrow test. New findings expand the ledger.

Reference: ModSync `d31ccbd3bfcddb4705917eafa6483715c166de3b` in the supplied C# worktree. `reference.json` records tracked source, tests, knowledgebase, and integration paths with their Git object IDs. It is a discovery index, not an implementation checklist. Reconcile reference changes before the final audit.

Status: **partial** = some behavior exists but full acceptance is unproven; **missing** = implementation absent; **unverified** = implementation exists but end-to-end evidence is absent. None of these means done.

| ID | Required behavior | Authoritative C# source under `src/` | Rust status | Evidence required for acceptance |
|---|---|---|---|---|
| F01 | TOML/JSON/YAML/XML/Markdown import/export, metadata, instructions/options, GUIDs, round trips, autodetection and stdin | Core `Services/FileLoadingService.cs`, `ModComponentSerializationService.cs`, `Utility/Serializer.cs`, CLI `ModBuildConverter.cs` | partial | Cross-read C# and Rust exports; compare normalized component trees and error behavior for every format |
| F02 | Community Markdown ingestion, spoilers, headings, links, generated IDs, preamble/epilogue | Core `Parsing/MarkdownParser.cs`, `MarkdownUtilities.cs`, GUI paste/import flows | partial | K1/K2 full and spoiler-free guide corpus; component/metadata comparisons plus GUI paste test |
| F03 | Natural-language directions, review flags, local archive generation and direct-Markdown preflight | Core `Parsing/NaturalLanguageInstructionParser.cs`, `DraftInstructionService.cs`, `Services/AutoInstructionGenerator.cs`, `DirectMarkdownInstallPreflight.cs` | missing | Port parser cases and compare real guide outputs; ambiguous drafts must require review |
| F04 | Component/option model, defaults, game fields, selections, category/tier filters, dependency selection and ordering | Core `ModComponent.cs`, `Option.cs`, dependency/processing services; GUI `ModSelectionPage` | partial | Port selection/graph tests; equivalent CLI selection and GUI state transitions |
| F05 | All instruction actions, overwrite/missing-source behavior, conditions, path placeholders, wildcard/case handling | Core `Instruction.cs`, `ModComponent.ExecuteSingleInstructionAsync`, `FileSystemUtils` | partial | Differential instruction fixtures using both real and virtual filesystems, including edge cases |
| F06 | Supported archives, large files, ZIP/7z/RAR/TAR/GZip, nested extraction, SFX, scratch layout, corruption behavior | Core `Utility/ArchiveHelper.cs`, `Services/FileSystem/*`, `PathHelper.ArchiveExtraction.cs` | partial | Real mod corpus, >512 MiB and nested archives, integrity/path rejection and memory/disk measurements |
| F07 | CleanList mandatory/substring/token matching and texture extension priority | Core `Instruction.cs`, `ModComponent.cs`, `Data/Game.cs` | partial | Positive/negative selection cases and identical final file sets |
| F08 | Structural/environment/readiness/virtual validation, reports, stages, warnings/errors, fix navigation | Core `Services/Validation/*`; GUI `ValidatePage` and validation dialogs | partial | Equivalent structured report output and GUI reports for passing/failing guides |
| F09 | External execution, HoloPatcher/KPatcher/OdyPatcher routing, arguments, Python resources and setup | Core `PatcherEngines.cs`, `Instruction.cs`, installation services; vendor integration | partial | Native subprocess contracts and real patch output comparisons for each supported backend/platform |
| F10 | Install coordinator, cancellation, state/progress, option failure, required install order | Core `Installation/*`, installation services; GUI installing pages | partial | Interrupted and failing real installs stop/restore/replay correctly; event/state comparisons |
| F11 | Per-component Git/immutable checkpoints, content-addressed data, provenance, restore/resume, browser UI | Core `GitCheckpointService.cs`, `ImmutableCheckpoint/*`; GUI `GitCheckpointBrowserDialog` | missing | Resume after interruption at each boundary, verify hashes/order, browse/restore through desktop and CLI |
| F12 | ResourceRegistry, cache, URL normalization, filename selection, direct acquisition | Core download/cache/link services | partial | Cached/uncached and stale-resource fixtures; archive-byte/checksum comparison |
| F13 | DeadlyStream, Nexus, MEGA, GameFront; authentication/free/premium handling and update metadata | Core `Services/Download/*DownloadHandler.cs`, `NexusApiClient.cs` | missing | Host-specific fixture tests plus authorized live downloads; CAPTCHA/manual/payment blocks must remain explicit |
| F14 | Concurrent download queue, progress/speed, cancellation, retries/timeouts, downloaded-file browser | Core download manager/orchestration; GUI download dialogs | missing | Controlled HTTP server failure scenarios and desktop status/stop/restart behavior |
| F15 | FOMOD discovery/XML/dependency rules/choices/priority/mapping, configure-before-install, CLI sidecar and GUI wizard | Core `Services/Fomod/*`, CLI FOMOD hosts; GUI `FomodInstallerDialog` | missing | Reference FOMOD test corpus; identical chosen files; conditional page flows and safe failure gate |
| F16 | Persistent profile schema, CRUD/clone/rename/activate, capture/apply component and option selections | Core `Services/Profiles/*`; GUI `ProfileManagerDialog` | missing | Shared C# JSON round trips, CLI verb tests, desktop actions, active profile integration |
| F17 | Settings schema, defaults, per-key merge, redaction, paths, themes, consent, API keys | Core `Services/Settings/*`; GUI `AppSettings`/settings dialog | missing | Existing settings read/write without unrelated data loss, secret redaction, desktop persistence |
| F18 | Managed deployment staging/manifests/hardlinks, ownership, undeploy/purge/profile switching | Core `Services/Deployment/*`, `Services/Installation/Managed*`; GUI mod management | missing | File ownership and restore tests including pre-existing files and multiple profiles; real filesystem probes |
| F19 | File conflict attribution, winners and competing components, preview/UI | Core `Services/Conflicts/*`; GUI `ConflictsDialog` | missing | VFS-derived conflict report matches C# for overwrite/order/option cases and renders in desktop |
| F20 | Merge matching/heuristics, per-field preference, exclusions/order, conflict resolution | Core `ComponentMergeService.cs`, `MergeHeuristicsOptions.cs`; GUI merge dialogs | missing | Port merge tests; full TOML+Markdown pipeline retains machine instructions and order |
| F21 | Component/instruction/option editor, reordering, dependency changes, URL/regex import and file browser | Core editor/manager services; GUI editor/management/import dialogs | missing | Round-trip edited guides and exercise every editor action in the native GUI |
| F22 | Mod update checks, stored results, version comparison and badges | Core `ModUpdateCheckService.cs`, update ports; GUI update indicators | missing | Controlled API fixtures, stored results and native UI actions |
| F23 | nxm:// and modsync:// parsing, registration, handoff, safe fetching and component resolution | Core protocol/download classes; GUI protocol integration | missing | URI edge/security cases and OS registration/second-instance delivery on each platform |
| F24 | Complete install wizard, preamble, Aspyr notices, optional pages, widescreen post-base stages, summaries | GUI `InstallWizardDialog`, `Dialogs/WizardPages/*` | partial | Native UI flow coverage for each conditional stage and equivalent install selection boundaries |
| F25 | Legacy main-window controls, menus, keyboard/accessibility, rich text and spoiler variants | GUI main window, controls, services, styles and converters | partial | Control/action inventory plus native keyboard/screen-reader/visual acceptance on supported OSes |
| F26 | Full CLI verbs/options/aliases, JSON contracts, exit codes, preload args and pipeline wrappers | Core `CLI/ModBuildConverter.cs`, GUI `CLIArguments.cs`, agent runbooks | partial | Table-driven invocation comparisons including early failures, stdout/stderr and exit status |
| F27 | Logging, telemetry consent/configuration/authentication, diagnostics | Core logger/telemetry services; GUI consent; telemetry-auth integration | missing | Consent-off produces no telemetry; configured events/diagnostics and secret handling match contracts |
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
