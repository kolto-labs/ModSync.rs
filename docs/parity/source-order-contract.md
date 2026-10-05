# The order of a step's Sources

This contract covers the order in which a Copy, Move or Extract step handles the files
its Sources name, which decides the result when two of them write the same file:
`Copy [<<modDirectory>>/b/x.txt, <<modDirectory>>/a/x.txt] -> Override`, or `Extract
[pack.zip, pack.7z]`, both extracting to `mods/pack/`. C# handles them in Source
order, so the later Source's file wins; Rust handled them in path order, so the file
whose path sorts last won. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Rust lines before this slice are at `b8b4d57`. Evidence:
[source-order-proof](source-order-proof/README.md) (Copy and Move) and
[extract-overwrite-proof](extract-overwrite-proof/README.md) (Extract). Found while
adding Destination-free collision cases to the extract-overwrite proof (tracker item
"extract-overwrite coverage", closed); F05-COPY, F05-MOVE, F05-EXTRACT and F05-X3
stay partial.

## Why this gap

It changes the files installed, with every check passing on both sides: a guide that
copies the same file name from two mods, or extracts two archives of one name in
different formats, got the other mod's file in Rust. Guides use the order on purpose
("then copy the patch over it"), so the authored order is the one to keep.

## Triage

**C# at `d31ccbd3`.** `Instruction.SetRealPaths` expands the Sources with
`PathHelper.EnumerateFilesWithWildcards` (`Instruction.cs:312`), which walks them in
authored order (a `HashSet` of the strings, enumerated in insertion order,
`FileSystemUtils/PathHelper.cs:635-637`) and appends each Source's files, then keeps
each file at its first place (`Distinct`, `Instruction.cs:379-383`).
`ExtractFileAsync` extracts `RealSourcePaths` in that order (`Instruction.cs:493-494`),
each entry written with `FileMode.Create`, so a later archive replaces an earlier
one's file. `CopyFileAsync` and `MoveFileAsync` start one task per path in that order
(`:1129`, `:1261`) behind a `SemaphoreSlim` with one slot (`:1068`, `:1197`), so they
run one at a time; run live, the later Source's file wins with `Overwrite = true` and
the first is kept with `Overwrite = false`. Within one wildcard Source, C# uses the
directory listing order.

**Rust at `b8b4d57`.** `matches_scoped` (`src/engine.rs:2414-2447`) collected every
Source's matches into a `BTreeMap` keyed by path and returned them in key order. Copy,
Move (`:2794-2876`, through `resolve_instruction`) and Extract (`:2712-2793`) handled
the files in that order, so `Copy [b/x.txt, a/x.txt]` copied `a` then `b`, and
`Extract [pack.zip, pack.7z]` extracted `pack.7z` then `pack.zip`.

**User-visible difference** (`mods/a/x.txt` is `A`, `mods/b/x.txt` is `B`):

```
Step                                         C# CLI      Rust before   Rust after
Copy [b/x.txt, a/x.txt] -> Override          x.txt = A   x.txt = B     x.txt = A
Copy [b/*, a/*] -> Override                  A           B             A
Move [b/x.txt, a/x.txt] -> Override          A           B             A
Copy [b/x.txt, a/x.txt], Overwrite = false   B           A             B
Extract [pack.zip, pack.7z] (no Destination) pack.7z's   pack.zip's    pack.7z's
```

## Behaviour after this slice

- `matches_scoped` (`src/engine.rs:2424-2463`) returns the files in Source order, each
  Source's own matches in key order, and a file two Sources name only at its first
  place, as C# lists them. Every caller gets that order: Copy, Move and Extract
  (through `resolve_instruction` and `matches`), Rename, Delete, Execute and Run
  (`instruction_sources`), CleanList, the FOMOD gate and the loose-Source gate.
- So with `Overwrite = true` the later Source's file wins a collision, and with
  `Overwrite = false` the first Source's file is kept (the later one warns and is
  skipped, as before). In one step or in two, an Extract of `pack.zip` and then
  `pack.7z` leaves `pack.7z`'s files.
- Unchanged: which files are found, missing-Source and directory rules, and the order
  of files within one wildcard. Delete's result does not depend on the order; a
  CleanList already read its first authored Source; the gates collect sets.

Callers checked: `matches` and `matches_scoped` have 13 call sites in `src/engine.rs`
and `src/engine/source_resolution.rs`; none relied on the sorted order (the gates and
`archive_members` test for emptiness or collect sets, and the CleanList choice
already took the first authored Source). The dry run and validation simulate the same
order, so `conflicts` and the reports see the file the install writes.

## Proof

[source-order-proof](source-order-proof/README.md) compares 9 Copy and Move cases with
the live C# CLI `install` (18 checks); the
[extract-overwrite proof](extract-overwrite-proof/README.md) adds five Destination-free
Extract cases (`same_stem_*`, 10 checks).

| Binary | source-order | extract-overwrite |
| --- | --- | --- |
| this slice, live C# CLI | 18 match / 0 fail | 14 match / 16 known difference / 0 fail |
| this slice, recorded references | 18 match / 0 fail | 14 match / 16 known difference / 0 fail |
| `b8b4d57` (pre-fix), recorded references | 13 match / 5 fail | 12 match / 16 known difference / 2 fail |

On `b8b4d57` the failures are the installs of `copy_ba`, `copy_wildcards_ba`,
`move_ba`, `copy_ba_b` and `copy_ba_false`, and of `same_stem_false` and
`same_stem_true` (one Extract step of `pack.zip` then `pack.7z`), each with the other
Source's file. The controls (Source order equal to path order, two Extract steps)
matched before and after.

## Known differences

- **Within one wildcard.** C# uses the directory listing order, which .NET does not
  define on Unix; Rust uses path order (case-insensitive by default). Not probed,
  but seen live in review: with a wildcard-first overlapping list
  (`Copy [*/x.txt, a/x.txt]`, `a/x.txt` and `b/x.txt` differing) C# on ext4 listed `b`
  before `a` and installed A, Rust installed B.
- **Recovered Sources.** A Source found by recovery (auto-extraction, the subfolder
  variant) is ordered as recovery returns it, as before.
- **Not probed.** Execute, Run and Rename with several Sources, and Windows. Nothing
  here depends on the host.

## Notes for guide authors

When two Sources of one step write the same file, the later one wins, as in C#. Put
the file you want to keep last, or use two steps.
