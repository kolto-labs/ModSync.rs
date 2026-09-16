# Compatibility and proof boundaries

Reference: the local ModSync C# source at `d31ccbd3bfcddb4705917eafa6483715c166de3b`. This is a Rust implementation of the instruction workflow, not a source-to-source conversion of the entire application.

| Area | Implemented behavior | Limits |
|---|---|---|
| Desktop | Native egui wizard, file/folder pickers, selection, options, search, validation, install, recovery, logs, report export | No visual or accessibility equivalence claim to Avalonia |
| CLI | List, convert (including stdin/stdout), validate, install, restore, download; JSON progress | No MCP server or protocol registration |
| Formats | Existing `thisMod` TOML; JSON arrays; YAML document streams; XML; format aliases/autodetection; fenced structured Markdown; default prose fields and YAML/TOML component comments | Prose export, legacy bullet metadata, custom regex profiles and complete parser error-recovery parity remain open |
| Metadata | Preserves additional component fields; TOML/YAML retain build metadata | Preserving a field does not implement its feature; XML typing of arbitrary metadata remains unverified |
| Selection | File selections, select-all when all components are unselected; explicit CLI components/options | No automatic dependency selection |
| Ordering | Stable dependency sort, restrictions, InstallBefore/After, instruction conditions | Missing references and cycles fail |
| Options | Choose resolves selected option GUIDs in declared option order | No automatic choice inference |
| File operations | Copy, Move, Rename, Delete, Extract, DelDuplicate, CleanList | Copy/Move flatten matched files into the target, as the C# instruction engine does |
| Archives | Streaming ZIP, 7z, RAR, TAR, GZip and 7z SFX; integrity checks; isolated extraction root | No encrypted/multipart archives; SFX support is limited to embedded 7z payloads; empty directories are not materialized |
| CleanList | C# comma-separated rows, mandatory prefix, substring and token matching | Source matching and final file sets still need full-build differential acceptance |
| Texture cleanup | Removes requested extension when another compatible extension shares its basename | This is not content-hash deduplication |
| Paths | Mod/game placeholders, legacy aliases, case-insensitive matching, wildcard sources | Rejects symlinks, device names, traversal, case collisions, overlapping roots, and special files |
| Validation | In-memory file index tracks changes across instruction boundaries; generated drafts require explicit review | Extracted content uses private temporary files outside game/mod trees; external program effects cannot be simulated |
| Patching | OdyPatcher subprocess, namespace index or named INI, exit-status checking, output capture | Requires separately installed OdyPatcher; no embedded patcher, HoloPatcher, Wine, or patch-byte equivalence claim |
| Recovery | Snapshot both roots, stop on first failure, block retry until verified restore | Full copies; restart whole guide after restore; not per-component resume or a power-loss atomic filesystem transaction |
| Downloads | HTTPS, streamed transfer and integrity checks, optional SHA-256, no-clobber publish | No host scraping, account management, CAPTCHA interaction, MEGA decryption, automatic resource acquisition, or update checks |
| Profiles | Save and reload guide selections | No separate profile manager |
| FOMOD, managed deployment | Not implemented | Do not use guides requiring these features |
| Widescreen, Aspyr, game-specific notices | Retained as metadata | No dynamic wizard stages or automatic game/version detection |
| Editor, telemetry | Not implemented | Edit instruction files externally |

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
