# E2 patcher InstallList simulation implementation

Implementation only; controlled CLI, backend, native and live acceptance remain open.
No tests, harnesses, process fixtures or game-tree runtime checks were run for this
slice. Production build/Clippy ownership stays with the coordinating agent.

The shared engine's virtual Patcher path now reads selected INI content directly
from its virtual file map. A source may be an existing file or archive-backed scratch
content created by an earlier Extract. The same virtual execution serves standalone
validation, conflict analysis and real-install preflight. It never starts a child,
rewrites configuration, creates logs, publishes a game/mod file or writes recovery
state. Pure virtual Patcher validation does not require process trust or a backend
binary. Actual installation checks both before taking its recovery snapshot; other
external actions retain their existing trust requirements.

`src/engine/patcher_simulation.rs` parses at most 8 MiB per INI and admits at most
100,000 INI keys/inferred writes. Reads, parsing and effect enumeration check cancellation.
A key index avoids quadratic repeated-key lookup. UTF-8 and
BOM-marked UTF-16 are decoded strictly; arbitrary legacy codepages are not guessed.
Sections and keys are case-insensitive, values trimmed, whole semicolon comment
lines ignored. Repeated keys replace their prior value without changing first-key
order. Inline semicolons are retained, matching the pinned reference parser.
Only `InstallList` keys beginning `install_folder` and their same-named sections
are visited. `File*`/`Replace*` values infer writes; empty values, missing sections,
`!` entries and unknown operations have no inferred effect.

Default configuration searches changes.ini at the source base and under tslpatchdata.
When namespaces.ini exists, its ordered Namespaces entries select the requested index
(or zero when no argument is supplied), then IniName and DataPath identify the actual
INI. An out-of-range index, unsafe path or missing selected INI fails explicitly;
it never silently falls back to an unrelated configuration. A named INI uses the
same tslpatchdata root as production named-INI staging. This deliberately improves
on C# `TslPatcherVfsSimulator`'s unconditional changes.ini selection. Actual supported
backend namespace behavior still needs differential evidence.

INI source values and destination folders must be safe relative paths and remain
within the configured roots, including symlink checks. The target starts at the
instruction's resolved destination (default game root). Existing virtual source
content propagates to the inferred target. Missing source payloads become an
explicit `Content::Unknown` marker, with a validation warning and event count.
Presence-dependent Copy/Move/Rename/Delete can continue in the VFS. Operations that
need bytes, including Extract, CleanList or another INI parse, fail on that marker
instead of treating it as a genuine empty file. A real publication of Unknown is
also forbidden. Real install rescans disk after preflight before setting install
mode, so virtual placeholders cannot become installation inputs.

Binary 2DA/GFF/TLK/script/container effects remain unmodeled. Conflict reports retain
the external-effects flag. Presence simulation, even with real source bytes, does
not verify a patcher's output. E1's process/log checks and the ordered failure/recovery
contract remain intact. Unknown payloads differ deliberately from the C# simulator's
empty-string placeholder: they cannot provide fabricated empty-byte evidence.

Reference inspected: pinned C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`,
`src/ModSync.Core/TSLPatcher/TslPatcherVfsSimulator.cs` and HoloPatcher
`Reader/NamespaceReader.cs`, `PatcherNamespace.cs`, CLI selection, and UI Core's
namespace-to-INI path mapping. The E2 controlled matrix in
`engine-archive-patcher-remaining-plan.md` remains required; F05/F07/F09 stay partial.
