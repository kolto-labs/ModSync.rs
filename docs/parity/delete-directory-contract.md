# Delete with a directory Source

This contract covers the F05-DELETE "literal directory" case from the
[actions audit](engine-actions-archives-audit.md): a `Delete` instruction whose
Source names a directory, for example
`Delete <<kotorDirectory>>/Override/textures` or `Delete <<kotorDirectory>>/Override`.
The reference is pinned C# `d31ccbd3`, and C# paths below are under
`src/ModSync.Core/` there. Evidence:
[delete-directory-proof](delete-directory-proof/README.md). F05 stays partial.

## Triage at `2241235`

**C# behaviour.** `ModComponent.ExecuteSingleInstructionAsync` handles `Delete`
with `SetRealPaths(skipExistenceCheck: true)`, then `Instruction.DeleteFile`
(`ModComponent.cs:1586-1590`). `SetRealPaths` expands wildcards to files and
appends every literal source as written, present or not (`Instruction.cs:305-345`).
`DeleteFile` (`Instruction.cs:828-933`) then checks each path with
`_fileSystemProvider.FileExists` (`:865`). The real provider's `FileExists` is
`File.Exists` (`Services/FileSystem/RealFileSystemProvider.cs:25`), which is
false for a directory. So a directory Source is treated exactly like a missing
file:

- `Overwrite = false` or omitted (Delete's loader default): a verbose
  `File does not exist (skipping)` line (`:876`), nothing is deleted, `Success`.
- `Overwrite = true`: `FileNotFoundPost` (`:872`), which fails the component with
  `MissingSourceFiles` on install and becomes the `DryRunSkipped` warning in the
  dry run (as in [delete-strict-contract.md](delete-strict-contract.md)).

Nothing under the directory is ever deleted.

**Rust at `2241235`.** The `Delete` branch (`src/engine.rs:2264-2316`) took its
targets from `Engine::matches` (`src/engine.rs:1853-1872`). `matches` keeps every
indexed file that the Source's glob matches *or that lies under the Source path*
(`source_matches`, `src/engine.rs:1889-1891`: `candidate.starts_with(prefix)`).
That descendant rule is shared by every other caller of `matches` (Copy/Move/
Patcher source resolution, Execute/Run, Rename, CleanList, the archive preflight
and the FOMOD gate). For Delete it
meant a directory Source deleted every file beneath it, recursively. The strict
check (`Overwrite = true`) already looked only at exact file keys, so that case
failed as C# does, and install refused before any write.

**Status.** An open gap with data loss: with `Overwrite` false or omitted, Rust
emptied the directory where C# deletes nothing. `Delete <<kotorDirectory>>/Override`
removed every file in `Override`. Plain `validate` and the dry-run report passed in
both, so nothing warned the author.

## Behaviour after this slice

- **Directory Source.** `Delete` now matches only the files its literal or
  wildcard Source names (`Engine::matches_scoped(.., descendants = false)`,
  `src/engine.rs:1859`, used by the Delete branch at `src/engine.rs:2300`). A
  directory Source matches nothing beneath it, so it is a miss:
  - `Overwrite` false or omitted: nothing is deleted and the step succeeds. If
    no Source of the step matched anything, Rust's existing validation warning
    `Attempting to delete non-existent file: <path>` is recorded, as for a
    missing file.
  - `Overwrite = true`: unchanged, the typed missing-source cause
    (`DryRunSkipped` in the dry-run report, rejected by plain validate and
    install before any write).
- **Unchanged.** Wildcards still match files only in the directory they name
  (`Override/*` does not reach `Override/sub/x.tga`, before or after). Other
  actions keep the descendant rule; see below.

## Edge cases compared with the live reference

Every case is in the proof; all match after this slice.

| Case | Result (both) |
| --- | --- |
| directory, `Overwrite` omitted / false | success, nothing deleted |
| directory, `Overwrite = true` | refused (C#: `MissingSourceFiles`), `DryRunSkipped` in dry run |
| `Override` itself, omitted / true | success, nothing deleted / refused |
| `<<modDirectory>>` subdirectory | success, nothing deleted |
| trailing `/`, backslash separators, case variant (`SUB`) | success, nothing deleted |
| directory plus a present file, false / true | the file deleted, the directory kept / refused |
| empty directory, omitted / true | success / refused |
| `Override/s*` matching file `s.tga` and directory `sub` | `s.tga` deleted, `sub/` kept |
| `Override/*`, omitted / true | top-level files deleted, `sub/` kept, success (a wildcard miss is not strict) |
| read-only file inside the directory | kept |
| directory, then a later Delete of a file | later step runs |
| `<<kotorDirectory>>` itself | success, nothing deleted (matched before this slice too) |

## Known differences

- **Warning.** With `Overwrite` false or omitted and nothing else matched, Rust
  records the validation warning `Attempting to delete non-existent file` in
  plain validate and the dry-run report. C# logs only a verbose line and its dry
  run reports no issue. The decision and tree match; the probe does not compare
  ordinary warnings. This is Rust's existing missing-file behaviour.
- **Symlinks.** Not probed: Rust refuses any symlink in an install tree (a
  deliberate fail-closed rule, see [copy-move-self-contract.md](copy-move-self-contract.md)).
- **Directory Source in other actions.** Found by the same sweep, not changed
  here: `Copy <<modDirectory>>/d -> Override` copies the files under `d`
  (flattened) in Rust, while C# fails with `MissingSourceFiles`. The descendant
  rule is shared by source recovery and FOMOD scoping, so it needs its own slice.

## Other differences seen in the sweep (not changed)

A live sweep of 61 single-step fixtures (DelDuplicate, CleanList, Copy/Move,
Rename, Delete) found these further differences. They are recorded for later
slices:

- `DelDuplicate` with `Destination = <<kotorDirectory>>` fails in Rust
  (`DelDuplicate destination cannot be found`); C# deletes the duplicate in the
  game root.
- `Rename` with a case-only target (`a.txt -> A.txt`) fails in Rust
  (`Source equals destination`); C# renames it.
- `Rename` with a placeholder-path Destination (`<<kotorDirectory>>/Override/b.txt`)
  succeeds in Rust; C# fails with `UnknownError`.
- `Rename Override/*.txt -> c.txt` (two matches, Overwrite default): Rust keeps the
  last source's content, C# the first's.
- `CleanList` with a row path containing `"` or `..`: Rust refuses the step as an
  unsafe path (deliberate); C# skips or deletes it. `CleanList` with a row whose
  case differs from the file on disk deletes in Rust; C# on Linux does not find it
  (a platform accident of a case-sensitive file system).
- `Copy` with a `..` segment in Source: Rust refuses it as unsafe (deliberate).
- `DelDuplicate` with `a.tga` and `a.TGA` in one directory: Rust refuses the
  case-colliding tree (deliberate); C# deletes both.

## Notes for guide authors

- `Delete` removes files, never directories. A Source that names a folder deletes
  nothing, in C# and now in Rust. To clear a folder, use a wildcard such as
  `<<kotorDirectory>>/Override/textures/*`; it reaches only files directly in that
  folder.
- With `Overwrite = true`, a folder Source fails the mod, because no file exists
  at that path.
