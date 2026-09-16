# Compatibility and proof boundaries

Reference: the local ModSync C# source at `d31ccbd3bfcddb4705917eafa6483715c166de3b`. This is a Rust implementation of the instruction workflow, not a source-to-source conversion of the entire application.

| Area | Implemented behavior | Limits |
|---|---|---|
| Desktop | Native egui wizard, file/folder pickers, selection, options, search, validation, install, recovery, logs, report export | No visual or accessibility equivalence claim to Avalonia |
| CLI | List, convert (including stdin/stdout), validate, install, restore, download; JSON progress | No MCP server or protocol registration |
| Formats | Existing `thisMod` TOML; JSON arrays; YAML document streams; XML; format aliases/autodetection; fenced structured Markdown | Prose Markdown import/export and complete parser error-recovery parity remain open |
| Metadata | Preserves additional component fields; TOML/YAML retain build metadata | Preserving a field does not implement its feature; XML typing of arbitrary metadata remains unverified |
| Selection | File selections, select-all when all components are unselected; explicit CLI components/options | No automatic dependency selection |
| Ordering | Stable dependency sort, restrictions, InstallBefore/After, instruction conditions | Missing references and cycles fail |
| Options | Choose resolves selected option GUIDs in declared option order | No automatic choice inference |
| File operations | Copy, Move, Rename, Delete, Extract, DelDuplicate, CleanList | Copy/Move flatten matched files into the target, as the C# instruction engine does |
| Archives | Streaming ZIP, 7z, RAR, TAR, GZip and 7z SFX; integrity checks; isolated extraction root | No encrypted/multipart archives; SFX support is limited to embedded 7z payloads; empty directories are not materialized |
| CleanList | C# comma-separated rows, mandatory prefix, substring and token matching | Source matching and final file sets still need full-build differential acceptance |
| Texture cleanup | Removes requested extension when another compatible extension shares its basename | This is not content-hash deduplication |
| Paths | Mod/game placeholders, legacy aliases, case-insensitive matching, wildcard sources | Rejects symlinks, device names, traversal, case collisions, overlapping roots, and special files |
| Validation | In-memory file index tracks changes across instruction boundaries | Extracted content uses private temporary files outside game/mod trees; external program effects cannot be simulated |
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
