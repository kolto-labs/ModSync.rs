# CleanList list-file choice with several resolving Sources

This contract covers the F07-D "multiple matched sources" row of the
[actions audit](engine-actions-archives-audit.md): a `CleanList` step whose Source
list names more than one existing list file, for example
`CleanList [<<modDirectory>>/l/b.csv, <<modDirectory>>/l/a.csv] -> <<kotorDirectory>>/Override`.
It was recorded as a known difference in
[delete-directory-contract.md](delete-directory-contract.md). The reference is
pinned C# `d31ccbd3`; C# paths below are under `src/ModSync.Core/` there.
Evidence: [cleanlist-list-choice-proof](cleanlist-list-choice-proof/README.md).
F07 stays partial.

## Triage at `0997b0f`

**C# behaviour.** The CleanList case of `ModComponent.ExecuteSingleInstructionAsync`
calls `instruction.SetRealPaths(skipExistenceCheck: true)` and then
`ExecuteCleanListAsync(cleanlistPath: null, ...)` (`ModComponent.cs:1669-1784`).
`ExecuteCleanListAsync` reads one list only, `RealSourcePaths[0]`
(`Instruction.cs:700-708`), and ignores every other Source. `SetRealPaths`
(`Instruction.cs:305-345`) builds `RealSourcePaths` with
`PathHelper.EnumerateFilesWithWildcards` (`FileSystemUtils/PathHelper.cs:617-756`):

- it walks the authored Sources through a `HashSet<string>` (`:635`), which
  enumerates in insertion order when nothing is removed, so authored order with
  exact-duplicate strings dropped;
- an existing literal file is added at its own position (`:658-679`), a wildcard
  adds its matches at its position (`:684-714`), in `Directory.GetFiles` order
  (`Services/FileSystem/RealFileSystemProvider.cs:284-286`), which .NET does not
  sort on Unix;
- `skipExistenceCheck` then appends each literal Source that was not already
  listed (a missing file or a folder), after everything else
  (`Instruction.cs:313-345`).

So the list C# reads is the first authored Source's file. When that Source is a
wildcard with several matches, it is whichever the directory listing returns first.

**Rust at `0997b0f`.** The CleanList arm (`src/engine.rs:2493-2500`) took
`matches_scoped(&i.source, ..)`, which collects every match into a `BTreeMap` keyed
by the case-folded path (`src/engine.rs:1897-1927`), and read `sources.first()`: the
match with the smallest key, whatever order the Sources were authored in.

**Status.** A visible difference in the final tree: `CleanList [l/b.csv, l/a.csv]`
read `a.csv` and deleted the files it lists where C# reads `b.csv`. The same held
for `[l/b*.csv, l/a.csv]`, `[l/b.csv, l/*.csv]`, a game-side list after a mod-side
list (game keys sorted first in the fixture), and a second step with the same shape.
Validate and the dry-run report passed in both, so nothing told the author.

## Behaviour after this slice

- The CleanList arm still requires every Source to match (unchanged, F05-X2), then
  reads the first match of the first authored Source:
  `matches_scoped(&i.source[..1], ..)` (`src/engine.rs:2493-2506`). The first Source
  matched in the full check, so this cannot miss.
- Within one wildcard Source with several matches Rust uses the first in sorted-key
  order. C# order there is the directory listing order, which is unspecified (see
  Known differences).
- Nothing else changes: list parsing, row matching, deletion planning and the miss
  classification (`classify_miss`, `MissKind::LiteralTyped`) are as before.

## Edge cases compared with the live reference

Fixture: `mods/l/a.csv` (`Mandatory Deletions,a.tga`), `mods/l/b.csv`
(`Mandatory Deletions,b.2da`), `game/Override/z.csv` (`Mandatory Deletions,s.tga`),
and `game/Override/{a.tga, b.2da, s.tga}`. `L` is `<<modDirectory>>/l`, `OV` is
`<<kotorDirectory>>/Override`. Every step's Destination is `OV`.

| Case | Sources | Result (both after this slice) |
| --- | --- | --- |
| `literal_ab` (control) | `L/a.csv`, `L/b.csv` | `a.tga` deleted |
| `literal_ba` | `L/b.csv`, `L/a.csv` | `b.2da` deleted (Rust before: `a.tga`) |
| `literal_bab_duplicate` | `L/b.csv`, `L/a.csv`, `L/b.csv` | `b.2da` deleted (before: `a.tga`) |
| `wildcard_then_literal` | `L/b*.csv`, `L/a.csv` | `b.2da` deleted (before: `a.tga`) |
| `literal_then_wildcard` (control) | `L/a.csv`, `L/b*.csv` | `a.tga` deleted |
| `question_then_literal` | `L/b.cs?`, `L/a.csv` | `b.2da` deleted (before: `a.tga`) |
| `literal_then_overlapping_wildcard` | `L/b.csv`, `L/*.csv` | `b.2da` deleted (before: `a.tga`) |
| `backslash_then_literal` | `<<modDirectory>>\l\b.csv`, `L/a.csv` | `b.2da` deleted (before: `a.tga`) |
| `game_list_then_mod_list` (control) | `OV/z.csv`, `L/a.csv` | `s.tga` deleted |
| `mod_list_then_game_list` | `L/b.csv`, `OV/z.csv` | `b.2da` deleted (before: `s.tga`) |
| `case_variant_then_literal` | `L/B.csv`, `L/a.csv` | `b.2da` deleted (before: `a.tga`) |
| `single_control` | `L/b.csv` | `b.2da` deleted |
| `two_steps` | [`L/b.csv`, `L/a.csv`], then [`OV/z.csv`, `L/a.csv`] | `b.2da`, `s.tga` deleted (before: `a.tga`, `s.tga`) |
| `folder_then_literal` | `L`, `L/b.csv` | known difference: C# reads `b.csv`, Rust refuses |

Every dry-run decision and `DryRunSkipped` flag, and every plain-validate decision,
matched before and after; only the install trees differed.

## Known differences

- **A folder or missing Source beside a resolving one (F05-X2, unchanged).** C#
  appends a folder or missing literal Source after the matched files and reads the
  first matched list; Rust requires every Source to match and refuses before
  anything is deleted (`folder_then_literal`, all three checks; its dry-run report
  shows `DryRunSkipped` where C# passes). Fail-closed, deliberate, and already listed
  in [delete-directory-contract.md](delete-directory-contract.md).
- **Several matches of one wildcard Source first.** `CleanList [L/*.csv]` with
  `a.csv` and `b.csv`: Rust reads `a.csv` (sorted-key order). C# reads the first
  file `Directory.GetFiles` returns, an order .NET does not define on Unix (a
  platform accident, not copied). On the cloud ext4 scratch disk the live reference
  read `a.csv` for both creation orders, matching Rust, but this is not a contract,
  so the case is not in the probe.
- **Case.** C# finds `L/B.csv` through `GetCaseSensitivePath`; Rust through its
  deliberate case-insensitive keys. Both read `b.csv` here.

## Results

| Binary | CleanList list-choice proof |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 39 match / 3 known difference / 0 fail (42 checks) |
| this slice, recorded reference | 39 match / 3 known difference / 0 fail |
| `0997b0f` (pre-fix) | 30 match / 3 known difference / 9 fail (the nine install trees marked "before" above) |
