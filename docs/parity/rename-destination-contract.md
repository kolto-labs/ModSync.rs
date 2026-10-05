# Rename Destination as a file name

This contract covers a `Rename` step whose Destination is a placeholder path, such as
`Rename <<modDirectory>>/a/x.txt -> <<kotorDirectory>>/Override/z.txt`. C# treats a
Rename Destination as the new file name in the source's folder, so its install fails
such a step; Rust resolved the placeholder and moved the file there, so it installed
a guide C# refuses and wrote a file C# never writes (here a mod file moved into the
game folder). The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Rust lines before this slice are at `044fca5`. Evidence:
[rename-destination-proof](rename-destination-proof/README.md). F05-RENAME stays
partial.

## Why this gap

[delete-directory-contract.md](delete-directory-contract.md) recorded it for a later
slice ("`Rename` with a placeholder-path Destination ... succeeds in Rust; C# fails
with `UnknownError`"). It changes the install decision and the files written, and
matching C# is not less safe: Rust now refuses where it moved files across roots.
The other Rename findings recorded there do not change a CLI decision: a case-only
rename (`a.txt -> A.txt`) fails C# validation and Rust alike, and the survivor of two
matches renamed onto one name depends on enumeration order in C#.

## Triage

**C# at `d31ccbd3`.** The install runs `RenameFile` after `SetRealPaths`
(`ModComponent.cs:1625-1629`), which resolves the Sources but not the Destination.
`RenameFile` (`Instruction.cs:935-1016`) joins the raw Destination to the source's
folder (`Path.Combine(Path.GetDirectoryName(sourcePath), Destination)`, `:976-979`)
and calls `RenameFileAsync(sourcePath, Destination, Overwrite)` (`:1005`), which
joins them the same way and calls `File.Move`
(`Services/FileSystem/RealFileSystemProvider.cs:66-78`). A placeholder Destination
names a folder literally called `<<kotorDirectory>>` below the source's folder, which
does not exist, so the step fails and the C# CLI install stops (`Install of 'A' failed
with exit code UnknownError`, observed live); steps already run keep their writes.
The dry run's `VirtualFileSystemProvider.RenameFileAsync`
(`Services/FileSystem/VirtualFileSystemProvider.cs:764-825`) records the joined path
as a virtual file and reports nothing, and `ComponentValidation` flags only the exact
Destination `<<gameDirectory>>/Override` (`ComponentValidation.cs:313-326`; Rust's
basic port is `src/validation/basic.rs:603-607`), so every C# report passes. The
documentation says the Destination is a "New filename (not a path, just the
filename with extension)" (`ModSync - Official Documentation.md:256-257`, at the
repository root).

**Rust at `044fca5`.** The engine's Rename arm (`src/engine.rs:2543-2552`) took a
Destination starting with `<<` as a path (`self.path`, `Roots::resolve`) and every
other Destination as one file name (`relative`, one part, else `Rename needs one
filename or a placeholder path`); the target was that path itself when it was
absolute, else the name in the source's folder (`:2576-2582`). The archive/destination
stage (`validate_component_archives`, `:1127-1143`) checked the same two shapes. So a
placeholder Destination passed every report, plain `validate` and the install
preflight, and the install moved the file to the resolved path.

**User-visible difference** (`A` renames `mods/a/x.txt`; `B` copies afterwards):

```
Destination                          C# reports  C# install         Rust before        Rust after
<<kotorDirectory>>/Override/z.txt    pass        fails at A         moves to game      refused, no writes
<<modDirectory>>/a/z.txt             pass        fails at A         renames in mods    refused, no writes
z.txt                                pass        renames            renames            renames
```

## Behaviour after this slice

- **The rule.** `rename_file_name` (`src/engine.rs:218-235`) takes a Rename
  Destination only as one file name in the source's folder, as C# joins it. A
  Destination starting with `<<` fails with `Rename Destination must be a file name,
  not a placeholder path: <Destination>`; a name with a folder part fails with `Rename
  Destination must be one file name: <Destination>`, as it did before under the old
  message; `..` and other unsafe names still fail `relative` (`Unsafe path`). Since
  [rename-file-name-contract.md](rename-file-name-contract.md) a name that is not
  one valid Windows file name (`..` inside it, a control character, `./z.txt`,
  `z.txt/`) is refused too.
- **Where.** The engine's Rename arm uses it (`src/engine.rs:2560-2564`, target
  `:2587-2589`), so a placeholder Destination fails where the step runs: the dry run
  (the `DryRun` stage of the full and basic reports and the desktop Validate step),
  plain `validate` and the install preflight, before any write, with the step's usual
  prefix (`A / step 1 (Rename): ...`). The archive/destination stage
  (`src/engine.rs:1146-1160`) still only resolves a placeholder Destination as a path,
  because it also checks the steps of unselected options, which C# never runs; a
  Destination with a folder part still fails there, now through `rename_file_name`.
- **Unchanged.** A plain file name renames as before. A step whose conditions
  exclude it, or that belongs to an option nothing runs, is not run and passes, as in
  C#.

Callers checked. `rename_file_name` has the two callers above (since
[rename-file-name-contract.md](rename-file-name-contract.md), also `draft::is_file_name`). The Rename arm is the
only code that turned a Rename Destination into a target; Rename does not go through
`resolve_instruction` or `qualify_action` (`src/engine/source_resolution.rs`), and the
conflict report, FOMOD gate and checkpoint runs use the same engine execution. The
draft generator (`src/draft.rs:576-578`, `:769-799`) could still write a Rename with a
`<<kotorDirectory>>\Override` Destination; that Destination was refused before too
(it resolved to a folder) and is now refused as a placeholder path. Since
[drafted-rename-contract.md](drafted-rename-contract.md) the drafter writes the bare
new name instead. The desktop uses the same engine and report.

## Proof

[probe.py](rename-destination-proof/probe.py) runs 9 cases (three placeholder
Destinations, a placeholder Rename after a Copy, three controls and two unchanged
differences) through the full report with a dry run, the basic dry-run report,
`install` and, for Rust, plain `validate`, and compares them with the live C# CLI
with a HoloPatcher stub (54 checks).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 25 match / 29 known difference / 0 fail |
| this slice, recorded reference | 25 match / 29 known difference / 0 fail |
| `044fca5` (pre-fix), recorded reference | 18 match / 12 known difference / 24 fail |

On `044fca5` all six checks of the four placeholder cases failed (24): Rust passed
every report and plain `validate` and installed the renamed file. The controls and
the two unchanged differences gave the same results on both builds.

## Known differences

- **Reports refuse a placeholder Rename Destination (fail closed).** C#'s dry run
  passes it and only its install fails, so its reports pass and its CLI install, which
  validates first, starts installing. Rust's dry run fails it (`DryRun` stage, one
  error), so the full and basic reports fail too. Four checks per placeholder case
  (16).
- **No partial install.** When the failing Rename follows other steps, C# keeps their
  writes (`placeholder_after_copy`: `Override/x.txt` from `A`); Rust refuses before any
  write. One check.
- **A Destination with a folder part (deliberate, unchanged).** C# renames
  `sub/z.txt` into the source's subfolder when it exists, and `../z.txt` out of the
  source's folder (no root check); Rust refuses both everywhere, as it did before. Six
  checks each (12). The documentation calls the Destination a file name.
- **A bare placeholder (unchanged decision).** `Rename <<kotorDirectory>>/r.txt ->
  <<kotorDirectory>>`: C# renames to a file named `<<kotorDirectory>>`; Rust refused it
  as a folder target and now refuses it as a placeholder path. Counted in
  [root-destination-contract.md](root-destination-contract.md).
- **Messages.** Rust's messages are its own; C# logs the `File.Move` failure and
  `Install of 'A' failed with exit code UnknownError`. Not compared.
- **Other formats.** Only TOML is probed; the rule is in the shared engine, so every
  guide format behaves the same.
- **Platform.** The reference ran on Linux under `unshare -Urnm` with a tmpfs over
  `/Override`. `<` and `>` are invalid in a Windows file name, so C#'s install should
  fail there too; not run.
- **Backslash Destinations on Linux.** With `\` separators (the usual guide spelling
  and what both drafters emit), C# on Linux treats the whole Destination as one file
  name: `<<kotorDirectory>>\Override\z.txt` or `sub\z.txt` renames the source to a
  file literally named that in its own folder, and the install succeeds. Rust refuses
  both (fail-closed); the intended target is never reached in C# either. Checked live
  in review, not in the probe.
- **Drafted Renames.** At `b5df010` `src/draft.rs` `destination()` gave every drafted
  Rename a placeholder Destination, so drafted Renames stayed refused (as they were
  before this slice, and the review gate refuses unreviewed drafts). Closed on
  2026-10-04: the drafter now keeps the bare new name, as newer C#
  (`NaturalLanguageInstructionParser.cs:1398-1424` in ModOrchestration) does; see
  [drafted-rename-contract.md](drafted-rename-contract.md).
