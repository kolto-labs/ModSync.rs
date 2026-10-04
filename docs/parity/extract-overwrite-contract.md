# Extract with Overwrite=false onto existing files

This contract covers the `Extract` leg of F05-X3 (destination collision with
`Overwrite = false`) from the [actions audit](engine-actions-archives-audit.md):
an ordinary (non-FOMOD) `Extract` whose archive entries land on files that
already exist. The reference is pinned C# `d31ccbd3`, and C# paths below are
under `src/ModSync.Core/` there. Evidence:
[extract-overwrite-proof](extract-overwrite-proof/README.md). F05 stays partial.

## Triage at `5905259`

**C# behaviour.** The loader reads `Overwrite` for every action, and defaults it
to `true` for everything but `Delete` when the key is absent
(`Services/ModComponentSerializationService.cs:2172-2180`). Extract resolves its
archives (`ModComponent.cs:1575-1585`) and calls
`Instruction.ExtractFileAsync` (`Instruction.cs:470-552`), which never reads
`Overwrite`: it passes each archive and destination to
`IFileSystemProvider.ExtractArchiveAsync` (`Instruction.cs:504`), whose signature
has no overwrite parameter. The real provider writes every entry with
`new FileStream(path, FileMode.Create, ...)`
(`Services/FileSystem/RealFileSystemProvider.cs:248`), which truncates and
replaces an existing file. So with `Overwrite = false`:

- a file already at an entry's target (stale output of an earlier install, or a
  game file when the destination is `Override`) is replaced by the archive bytes;
- when two archives, in one step or in two steps, supply the same path, the last
  one wins.

The C# writer does not even save the flag for Extract
(`Instruction.cs:209-212`, `ShouldSerializeOverwrite` is Move/Copy/Rename only), so
a reference round trip drops it and the reloaded guide defaults to `true`.

**Rust at `5905259`.** The Extract branch published each entry with
`put(..., i.overwrite)` (`src/engine.rs:2154-2160` at that commit). `put` returns
early when the target exists and `overwrite` is false
(`src/engine.rs:1963-1965`), with no warning. So Rust kept the existing file: the
stale bytes stayed, the game file was not replaced, and the first archive won.
Validate and install both passed. Implicit source recovery already extracts with
`overwrite: true` (`src/engine/source_resolution.rs:612-622`), so only an authored
Extract step was affected.

**Status.** An open gap with a visible outcome: the same guide installs different
file contents in the mod directory and in the game.

## Behaviour after this slice

- **Extract always replaces.** Every archive entry is published with overwrite on,
  whatever `Overwrite` says, so an existing file is replaced and the last archive
  wins, as in C#. Install, validate, the dry-run report and `conflicts` all run
  the same engine, so a simulated later step sees the replaced content, and a
  `conflicts` report now lists the Extract as a writer of a game file it replaces.
- **Unchanged: configured FOMOD extraction.** A FOMOD archive with saved choices
  and `Overwrite = false` is still refused when any of its paths already exists
  (`Configured FOMOD extraction cannot retain existing files with Overwrite=false`).
  That check runs before the entries are written. It is a deliberate fail-closed
  provenance guard (COMPATIBILITY "FOMOD source provenance guard"), not a C#
  behaviour, and this slice keeps it.
- **Unchanged: Copy, Move and Rename.** They still honour `Overwrite = false`
  (Copy/Move warn and skip, Rename fails), as C# does.
- **Unchanged: directory collisions.** An entry whose target is an existing
  directory, or whose parent is an existing file, still fails, as it does in C#
  (`File.Create`/`Directory.CreateDirectory` throw, giving `InvalidArchive`).

## Known differences

- **C# dry run.** C#'s virtual provider always extracts under an archive-named
  folder, even for an explicit destination
  (`Services/FileSystem/VirtualFileSystemProvider.cs:932`), unlike its own real
  provider (`RealFileSystemProvider.cs:159-201`). A later step that reads the real
  install path then misses in the dry run, and the reference reports
  `DryRunSkipped`. That happens with any `Overwrite` value, and is a reference
  inconsistency, so the probe records the reference dry run but does not compare
  it. Rust's dry run follows the install layout.
- **Timestamps and modes.** Not compared. Both runtimes set the entry time; this
  slice does not change Rust's existing timestamp and permission handling.

## Notes for guide authors

- `Overwrite = false` on an `Extract` step does not protect existing files, in C#
  or now in Rust: the archive replaces them. To keep a file, extract into a
  separate folder and `Copy` with `Overwrite = false`, or order the steps so the
  file you want is written last.
- A configured FOMOD archive with `Overwrite = false` still fails in Rust when its
  output folder already holds any of its files. Remove that folder, or set
  `Overwrite = true`.
