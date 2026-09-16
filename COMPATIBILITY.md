# Compatibility and proof boundaries

Reference: the local ModSync C# source at `d31ccbd3bfcddb4705917eafa6483715c166de3b`. This is a Rust implementation of the instruction workflow, not a source-to-source conversion of the entire application.

| Area | Implemented behavior | Limits |
|---|---|---|
| Desktop | Native egui wizard, file/folder pickers, selection, options, search, validation, install, recovery, logs, report export | No visual or accessibility equivalence claim to Avalonia |
| CLI | List, convert, validate, install, restore, download; JSON progress | No MCP server or protocol registration |
| Formats | Existing `thisMod` TOML, JSON, YAML; fenced structured Markdown | XML and natural-language guide interpretation are not implemented |
| Metadata | Preserves additional component and guide fields during conversion | Preserving a field does not implement its feature |
| Selection | GUI file flags; CLI select-all or file flags, explicit components/options | No automatic dependency selection |
| Ordering | Stable dependency sort, restrictions, InstallBefore/After, instruction conditions | Missing references and cycles fail |
| Options | Choose resolves selected option GUIDs in declared option order | No automatic choice inference |
| File operations | Copy, Move, Rename, Delete, Extract, DelDuplicate, CleanList | Copy/Move flatten matched files into the target, as the C# instruction engine does |
| Archives | ZIP, 7z, RAR, integrity checks, isolated default extraction root | No encrypted/multipart archives or self-extracting executables; 512 MiB per file/archive expansion limit |
| CleanList | CSV, mandatory rows, selected mod names | Exact names only; C# fuzzy name matching is not reproduced |
| Texture cleanup | Removes requested extension when another compatible extension shares its basename | This is not content-hash deduplication |
| Paths | Mod/game placeholders, legacy aliases, case-insensitive matching, wildcard sources | Rejects symlinks, device names, traversal, case collisions, overlapping roots, and special files |
| Validation | In-memory file index tracks changes across instruction boundaries | RAR decoding uses a private temporary input file; external program effects cannot be simulated |
| Patching | OdyPatcher subprocess, namespace index or named INI, exit-status checking, output capture | Requires separately installed OdyPatcher; no embedded patcher, HoloPatcher, Wine, or patch-byte equivalence claim |
| Recovery | Snapshot both roots, stop on first failure, block retry until verified restore | Full copies; restart whole guide after restore; not per-component resume or a power-loss atomic filesystem transaction |
| Downloads | HTTPS, bounded streaming, archive integrity, optional SHA-256, no-clobber publish | No host scraping, account management, CAPTCHA interaction, MEGA decryption, automatic resource acquisition, or update checks |
| Profiles | Save and reload guide selections | No separate profile manager |
| FOMOD, managed deployment | Not implemented | Do not use guides requiring these features |
| Widescreen, Aspyr, game-specific notices | Retained as metadata | No dynamic wizard stages or automatic game/version detection |
| Editor, telemetry | Not implemented | Edit instruction files externally |

Unknown instruction fields and actions fail rather than being ignored. Components with no executable instructions cannot be installed. External program permission is off by default.

The archive expansion cap is a deliberate initial ceiling. Large texture packs above 512 MiB need streaming extraction before this port can handle them. FOMOD archives are detected and rejected before extraction because the port cannot apply their choices.

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

Local Linux checks also exercised the native GUI in an isolated X11 session: load the sample guide, choose mods, validate, confirm installation, and verify the resulting TGA and removed TPC. A separate native OdyPatcher invocation through the Rust CLI installed `hello.txt` into an isolated game folder with zero patcher errors. The test suite separately checks named-INI staging and failure recovery using a controlled subprocess.
