# A wildcard Source that matches nothing beside one that does

This contract covers a Copy, Move or Rename step with several Sources of which a
wildcard matches no file while another Source names files: `Move
[<<modDirectory>>/m/*.tga, <<modDirectory>>/m/*.tpc] -> Override` with only TGAs in
`m/`. C# handles the files the other Sources name; Rust refused the step. The
reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there. Rust
lines before this slice are at `f6d2ec5`. Evidence:
[wildcard-miss-proof](wildcard-miss-proof/README.md). Tracker item: F05-X2 ("the
absent-literal tolerance remain[s] open"); F05-COPY, F05-MOVE, F05-RENAME and F05-X2
stay partial.

## Why this gap

It changes a pass/fail decision on the most common shape of a loose-file step: one
wildcard per file type (`*.tga`, `*.tpc`, `*.txi`), written for a mod that ships only
some of them, or for a component that moves files another component extracted. The C#
CLI validates and installs such a step; Rust stopped the whole install before any
write, and the desktop Validate step skipped (or, in a chosen option, failed) the
component. Matching C# here is not less safe: a wildcard names no particular file, so
an empty match is not a missing download, and a step whose Sources all match nothing
still fails.

## Triage

**C# at `d31ccbd3`.** Copy and Move bind their Sources through
`TryResolveInstructionSourcesAsync` (`ModComponent.cs:975-1029`, called at
`:1597-1624`), which calls `Instruction.SetRealPaths`. That expands every Source with
`PathHelper.EnumerateFilesWithWildcards` (`Instruction.cs:312`), which skips a
literal that names no file and adds a wildcard's matches, if any
(`FileSystemUtils/PathHelper.cs:658-713`), into one list. `SetRealPaths` fails only
when that list is empty (`WildcardPatternNotFoundException` or
`FileNotFoundException`, `Instruction.cs:350-368`); otherwise every listed file
exists (`:370-376`) and the step copies or moves them. So both a wildcard and a
literal that match nothing are dropped when another Source matched, and no
auto-extraction is tried. Rename calls `SetRealPaths(skipExistenceCheck: true)`
(`ModComponent.cs:1625-1628`), which adds every literal Source back
(`Instruction.cs:313-348`); `RenameFile` then fails a missing literal
(`FileNotFoundPost`, `Instruction.cs:967-974`) and throws on an empty list
(`:950-953`), but a wildcard that matches nothing is still dropped.

**Rust at `f6d2ec5`.** `resolve_instruction` (`src/engine/source_resolution.rs:123-193`)
bound Copy and Move (and Extract) Sources with `matches_scoped(.., false, ..)`
(`:154-158`), and the Rename branch (`src/engine.rs:2810-2821`) did the same;
`matches_scoped` (`src/engine.rs:2429-2462`) returns `MissingSource` for the first
Source with no match (`:2452-2454`). So one empty wildcard failed the step: plain
`validate`, `install` and `conflicts` refused it, and the dry-run reports turned the
miss into a `DryRunSkipped` warning (a Rename, untyped, or a step in a chosen option,
`OptionalInstallFailed`, failed the report).

**User-visible difference** (`m/` holds `a.tga` only):

```
Step                                     C# CLI install    Rust before          Rust after
Move [m/*.tga, m/*.tpc] -> Override      moves a.tga       refused, no writes   moves a.tga
Copy [m/nope*.tga, m/a.tga] -> Override  copies a.tga      refused              copies a.tga
Rename [m/a.tga, m/nope*.txt] -> z.tga   renames a.tga     refused              renames a.tga
Copy [m/*.tpc, m/*.dds] -> Override      fails             refused              refused
Copy [m/a.tga, m/nope.tga] -> Override   copies a.tga      refused              refused (kept)
```

## Behaviour after this slice

- New `gathered_matches` (`src/engine.rs:2465-2496`) binds a Copy, Move or Rename
  step: it collects every Source's files in Source order (as `matches_scoped`, each
  file at its first place) and drops a wildcard Source (one with `*` or `?`) that
  matches nothing. It still returns `MissingSource`, naming the first Source that
  matched nothing, when a literal Source matched nothing or no Source matched.
- `resolve_instruction` uses it for Copy and Move
  (`src/engine/source_resolution.rs:150-160`), the Rename branch for Rename
  (`src/engine.rs:2849-2853`). A Copy or Move whose list is not empty no longer
  enters recovery, as C# tries auto-extraction only when `SetRealPaths` fails;
  every other miss is typed and recovered as before, and Rename's miss is still
  classified by `classify_miss`.
- So plain `validate`, `install`, `conflicts`, both dry-run reports, `--phase
  remaining` and the desktop Validate step handle the files that matched; the
  install writes them as C# does.
- Unchanged: Extract and Patcher (`resolve_instruction`), Delete (it already skips
  missing Sources), CleanList, Execute and Run (`matches_scoped`, per Source), the
  loose-Source check next to a component's own Extract (`check_archive_sources`,
  each Source must be in an archive, as in C# `VerifyExtractPaths`), and the FOMOD
  copy plans (`matches(.., false)` before `execute`).

Callers checked: `resolve_instruction` is called by Extract, Copy, Move and Patcher
(`src/engine.rs`, three sites); only Copy and Move take the new path. The Rename
branch is the only other caller of `gathered_matches`. Recovery
(`source_resolution.rs`, `recover`) still requires each Source to match after
auto-extraction (C# `SetRealPaths` runs again there and would drop an empty
wildcard); not probed.

## Proof

[wildcard-miss-proof](wildcard-miss-proof/README.md) runs 14 cases against the live
C# CLI with a HoloPatcher stub: the full and basic dry-run reports (decision and
counts), `install` (exit and final tree) and Rust's plain `validate` against C#'s
install decision, 84 checks.

| Binary | wildcard-miss | delete-directory |
| --- | --- | --- |
| this slice, live C# CLI | 71 match / 13 known difference / 0 fail | not rerun live |
| this slice, recorded references | 71 match / 13 known difference / 0 fail | 170 match / 10 known difference / 0 fail |
| `f6d2ec5` (pre-fix), recorded references | 31 match / 13 known difference / 40 fail | 167 match / 10 known difference / 3 fail |

On `f6d2ec5` the nine fixed cases fail (40 checks); see the proof README. The
delete-directory proof's `rename_present_and_wildcard_nomatch` (`Rename
[Override/a.tga, Override/nope*.txt]`) was a known difference and now matches its
reference (three checks; with the probe before this slice, `f6d2ec5` gave 167 /
13 / 0).

## Known differences

- **A literal Source that names no file (deliberate, fail-closed).** C# drops it when
  another Source matched and installs the rest (`literal_miss`); Rust still refuses
  the step before any write, and its reports skip the component with a
  `DryRunSkipped` warning. A named file that is missing usually means a wrong or
  incomplete download. The delete-directory proof's `copy_dir_and_file_omitted`
  (`Copy [d, d/m.txt]`, a folder Source) stays a known difference for the same
  reason.
- **Extract (deliberate, unchanged).** `Extract [p.zip, *.7z]` with no `.7z`: C#
  extracts `p.zip`; Rust's archive stage reports the step and the install is refused
  (`extract_wildcard_miss`), as recorded in
  [missing-archives-contract.md](missing-archives-contract.md).
- **A failing Rename (unchanged).** `Rename [m/a.tga, m/nope.txt]`: both fail; C#
  renames `a.tga` first and keeps it, Rust refuses before any write
  (`rename_literal_miss`).
- **Counts next to an Extract (unchanged).** Both refuse a loose wildcard missing
  from the component's own archive; the report counts differ as in
  [loose-source-contract.md](loose-source-contract.md) (`extracted_same_component`).
- **Not changed, not probed.** CleanList, Execute and Run with a wildcard that matches
  nothing beside a matching Source are still refused (C# reads or runs the matched
  file; see [delete-directory-contract.md](delete-directory-contract.md)); recovery
  after auto-extraction still needs every Source; Windows. Nothing here depends on
  the host.

## Notes for guide authors

One wildcard per file type is fine: a step installs whatever its wildcards match, as
in C#, and fails only when none of them matches. Name a file without a wildcard only
when it must be there.
