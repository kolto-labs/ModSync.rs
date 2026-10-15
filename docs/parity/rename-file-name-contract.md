# Rename Destination as a valid file name

This contract covers a `Rename` step whose Destination is one name that is not a valid
or safe Windows file name, such as `z..txt`, `..z.txt`, a name with a control
character, `./z.txt` or `z.txt/`. Rust's `rename_file_name` accepted these (only
`relative`'s rules applied), while a Copy or Move Destination with the same part is
refused (`invalid_destination`). It also covers the drafter, which drafted such a name
(and a device name such as `con.tga`) as a bare Rename Destination that the engine
then refused, or for `..`, installed. The reference is pinned C# `d31ccbd3`; C# paths
are under `src/ModSync.Core/` there. Rust lines before this slice are at `c2c9002`.
Evidence: [rename-file-name-proof](rename-file-name-proof/README.md). F05-RENAME and
F03 stay partial.

## Why this gap

[drafted-rename-contract.md](drafted-rename-contract.md) left it as a follow-up
("Rename names with `..`") and the tracker listed it under F05-RENAME. A guide could
rename a mod file to a name that cannot exist on Windows, so the same guide installed
on Linux and failed, or did something else, on Windows; the rest of Rust's path rules
refuse a name on every host for that reason (`paths::relative`, "Windows names are
rejected on every host"). The drafter, which keeps a bare new name since the
drafted-rename slice, drafted the same names, so a reviewed draft could rename to
`new..tga` or fail only after review with `Reserved filename`.

## Triage

**C# at `d31ccbd3`.** Validation never checks a Rename Destination as a path:
`ComponentValidation.ParseDestinationWithAction` flags only the exact Destination
`<<gameDirectory>>/Override` for Rename (`ComponentValidation.cs:313-326`); the
`PathValidator.IsValidPath` check (`FileSystemUtils/PathValidator.cs:30-96`: control
characters at `:66-69` and `:145`, device names at `:78-81`, a part ending in `.` or a
space or containing `..` at `:83-86`) is in the `default` arm (`:331-365`) for Copy,
Move, CleanList and DelDuplicate only. The install runs `RenameFile` after
`SetRealPaths`, which leaves the Destination alone (`ModComponent.cs:1625-1629`).
`RenameFile` (`Instruction.cs:935-1016`) joins the raw Destination to the source's
folder (`:976-979`) and calls `RenameFileAsync` (`:1005`), which joins it again and
calls `File.Move` (`Services/FileSystem/RealFileSystemProvider.cs:66-78`); an
`IOException` fails the step (`Instruction.cs:1007-1014`). The dry run's
`VirtualFileSystemProvider.RenameFileAsync`
(`Services/FileSystem/VirtualFileSystemProvider.cs:764-825`) records the joined path
and reports nothing.

Run live on Linux (contained, see the proof): every report passes every case. The
install renames `mods/a/x.txt` to the name as written for `z..txt`, `..z.txt`,
`z<TAB>.txt`, `z<U+0001>.txt`, `z.txt.`, `z.txt `, `con.txt` and `z[1].txt`, and to
`z.txt` for `./z.txt`; for `z.txt/` `File.Move` fails, so the install stops at `A`
with nothing written.

On Windows (reasoned from the Win32 rules, not run): `z..txt` and `..z.txt` are valid
names there, so C# renames to them; a control character fails `File.Move`; a trailing
dot or space is stripped, so `z.txt.` becomes `z.txt`; `con.txt` names the console
device on Windows 10 and earlier, so `File.Move` fails there; `./z.txt` gives `z.txt`;
`z.txt/` fails. So the same guide gives different files, or a failure, depending on
the host.

**Rust at `c2c9002`.** `rename_file_name` (`src/engine.rs:218-235`) refused a
placeholder, then took the Destination through `relative` (`src/paths.rs:97-105`,
`checked_relative` at `:129-148`: `..`, `:` and `<>|"`, NUL, a trailing dot or space,
wildcards including `[` and `]`, device names via `is_reserved_name` at
`:150-175`) and required one part. `relative` drops a `.` part and an empty part,
and lets through `..` inside a longer name and control characters other than NUL, so `z..txt`, `..z.txt`,
`z<TAB>.txt` and `z<U+0001>.txt` renamed to that name and `./z.txt` and `z.txt/` to
`z.txt`, with every report passing. `invalid_destination` (`src/engine.rs:206-216`)
refused these parts only for Copy, Move, CleanList and DelDuplicate. `rename_file_name`
has two callers: the archive/destination stage (`validate_component_archives`,
`:1146-1148`, every applicable step of the component and its immediate options) and
the engine's Rename arm (`:2559-2563`, the dry run, plain `validate` and the install).
A name `relative` already refused (`con.txt`, `z.txt.`) failed at both, so the
ComponentValidation and DryRun stages, plain `validate` and `install` before any
write. The drafter kept a captured new name as a bare Destination when `is_file_name`
(`src/draft.rs:725-728`: an extension and no `/`, `\`, `:`, `<`, `>`) held, so it
drafted `con.tga`, `new..tga` and `..new.tga` (`from_match`, `:576-586`; `sanitize`,
`:857`).

**User-visible difference** (`A` renames `mods/a/x.txt`; `B` copies afterwards):

```
Destination      C# reports  C# install (Linux)   Rust before          Rust after
z..txt, ..z.txt  pass        renames to it        renames to it        refused, no writes
z<TAB>.txt       pass        renames to it        renames to it        refused, no writes
./z.txt          pass        renames to z.txt     renames to z.txt     refused, no writes
z.txt/           pass        fails at A           renames to z.txt     refused, no writes
con.txt, z.txt.  pass        renames to it        refused, no writes   refused, no writes
z.txt, z .a.txt  pass        renames              renames              renames
```

## Behaviour after this slice

- **The rule.** `rename_file_name` (`src/engine.rs:218-247`) now also requires the
  Destination to be exactly the one name `relative` returns (no separator, `.` part or
  empty part), else `Rename Destination must be one file name: <Destination>`, and
  refuses a name with `..` inside it or a control character (`invalid_destination`,
  the Copy/Move rule), with `Rename Destination is not a valid file name (`..` inside
  the name or a control character): <Destination>`. The `relative` refusals (`Unsafe
  path`, `Reserved filename`, `Wildcards are not allowed here`) are unchanged, and all
  of them fail at the same points as before: the ComponentValidation (archive) stage
  (`:1158-1160`) and the Rename arm (`:2571-2575`), so every report, plain `validate`
  and `install` refuse before any write, with the step's usual prefix (`A / step 1
  (Rename): ...`).
- **The drafter.** `is_file_name` (`src/draft.rs:725-730`) now keeps a drafted new name
  as a bare Destination only when `rename_file_name` accepts it (and it has an
  extension), so the drafter never drafts a name the engine refuses. Any other name
  (`con.tga`, `new..tga`, `..new.tga`) keeps the `<<kotorDirectory>>\Override`
  Destination, which is what the pinned C# drafter drafts for every Rename, and which
  the engine refuses after review as a placeholder path, as for a name without an
  extension. `sanitize` (`:859`) uses the same test.
- **Unchanged.** A plain file name, with any number of single dots and inner spaces,
  renames as before. A step whose conditions exclude it is not run and passes. A
  placeholder or a folder part is refused as before.

Callers checked. `rename_file_name` has the two engine callers above and now
`draft::is_file_name`, whose callers are `from_match` and `sanitize`, both reached
only from `draft::generate` (Markdown ingestion, `convert --parse-directions`, the
desktop's draft button). `invalid_destination` is unchanged for its other callers
(`destination_gate`, the archive stage's Copy/Move/CleanList/DelDuplicate arm,
DelDuplicate and CleanList execution). Rename does not go through
`resolve_instruction`, and the conflict report, FOMOD gate and checkpoint runs use the
same engine execution.

## Proof

[probe.py](rename-file-name-proof/probe.py) runs 14 authored cases (six newly refused
names, four names already refused, four controls) through the full report with a dry
run, the basic dry-run report, `install` and, for Rust, plain `validate`, compared with
the live C# CLI with a HoloPatcher stub (84 checks), and 4 drafter cases (prose with
`con.tga`, `new..tga` and `..new.tga`), checking the draft, the review, the install of
the reviewed draft and the pinned C# drafter's output (16 checks).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 37 match / 63 known difference / 0 fail |
| this slice, recorded reference | 37 match / 63 known difference / 0 fail |
| `c2c9002` (pre-fix), recorded reference | 26 match / 24 known difference / 50 fail |

On `c2c9002` the six newly refused names passed every report and plain `validate` and
installed (36 fails), the unselected option's `z..txt` passed the reports (4), and the
drafter drafted the bare names (`draft` and `csharp` of all four drafter cases, and
`install` of the two `..` cases, which renamed `old.tga`: 10). The four names already
refused and the other controls gave the same results on both builds. The C# oracle
recorded live with the pre-fix binary and again with the fixed binary was identical.

## Known differences

- **Rust refuses names C# renames to (fail closed, new).** `z..txt`, `..z.txt` and the
  control-character names: C#'s reports pass and its Linux install renames to the name;
  Rust refuses everywhere before any write. `./z.txt` likewise (C# renames to `z.txt`).
  Windows itself accepts `z..txt`; the `..` rule is C#'s own
  `PathValidator.HasInvalidWindowsFileNameParts` (`PathValidator.cs:166-193`), which
  C# applies to Copy and Move Destinations, not a Win32 rule.
  `z..txt` and `..z.txt` are valid on Windows too, so here Rust is stricter than C# on
  every host; it applies the rule C# itself applies to a Copy or Move Destination.
  Six checks each (36).
- **`z.txt/` (reports only).** C#'s install fails it too, and Rust's plain `validate`
  and `install` refuse it, so those match; C#'s reports pass it, Rust's fail. Four
  checks.
- **Names `relative` already refused (unchanged).** A trailing dot or space, a device
  name (`con.txt`) and `[` or `]` (`z[1].txt`): C# renames to the name on Linux, Rust
  refuses, as before. Six checks each (24). `[` and `]` are valid Windows names; Rust
  refuses them in every non-wildcard instruction path (`relative`), and a Rename
  Destination is no exception.
- **An unselected option's step (reports only).** The archive stage checks the Rename
  names of every immediate option, selected or not (as it already did for a folder
  part), so a bad name in an option nothing runs fails Rust's reports; C# never checks
  it, and plain `validate` and `install` pass on both. Four checks.
- **Drafted Rename after "to" (one Source).** For "Rename the file old.tga to
  con.tga." both drafters give the Override Destination; the pinned C# drafter adds
  `con.tga` as a second Source, Rust keeps one Source (the drafted-rename rule). One
  check. `tools/check_drafts.py` now lists this shape (same placeholder Destination, the
  second Source left out) under `intentional_rename_differences` too.
- **Windows.** Not run; the C# Windows behaviour above is reasoned from the Win32 name
  rules. Rust's rule is the same on every host.
- **Messages.** Rust's messages are its own; C# logs the `File.Move` failure for
  `z.txt/`. Not compared.
