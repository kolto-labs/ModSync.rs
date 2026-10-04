# Directory Sources and wildcard rules for file actions

This contract covers the F05-DELETE "literal directory" case from the
[actions audit](engine-actions-archives-audit.md): a `Delete` instruction whose
Source names a directory, for example
`Delete <<kotorDirectory>>/Override/textures` or `Delete <<kotorDirectory>>/Override`.
The follow-up (after review of `4fb71a6`) extends it to `Copy`, `Move` and
`Rename` with a directory Source, and to the wildcard rules every Source uses
(`**`, `[...]`) (F05-X2).
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
`File.Exists` (`Services/FileSystem/RealFileSystemProvider.cs:27`), which is
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

## Triage of the follow-up at `4fb71a6`

**C# wildcard rules.** `PathHelper.EnumerateFilesWithWildcards`
(`FileSystemUtils/PathHelper.cs:617-756`) returns files only: a literal path is
kept when `FileExists` holds (`:658-679`), and a pattern lists every file under
its first wildcard-free ancestor and keeps those for which
`WildcardPathMatch(file, pattern)` holds (`:684-714`). `WildcardPathMatch`
(`:760-800`) requires the same number of path levels and matches level by level.
`WildcardMatch` (`:802-820`) `Regex.Escape`s the level and turns `\*` into `.*`
and `\?` into `.`. So `*` and `**` both match any run of characters within one
level, `?` matches one character, and `[`/`]` are literal. Matching is ordinal
(case-sensitive); Rust keeps its deliberate case-insensitive keys.

**C# directory Sources in Copy, Move and Rename.** Copy and Move bind through
`TryResolveInstructionSourcesAsync` and fail with `FileNotFoundPre` when it finds
nothing (`ModComponent.cs:1597-1624`); Rename uses `SetRealPaths(skipExistenceCheck:
true)` and `RenameFile` (`:1625-1629`, `Instruction.cs:936-1016`), which returns
`FileNotFoundPost` for a path that is not a file. Both map to `MissingSourceFiles`
on install and to the `DryRunSkipped` warning in the dry run. Nothing is copied,
moved or renamed, whatever `Overwrite` is.

**Rust at `4fb71a6`.**

- `source_pattern` (`src/engine.rs:1892-1904`) passed the authored path to the
  `glob` crate unchanged, so `**` as a whole level matched any depth and `[ab]` was
  a character class. The FOMOD scoping copies built the same pattern
  (`src/fomod/resolved_choices.rs:147`, `src/fomod.rs:919`).
- Copy and Move bound with `matches` (`src/engine/source_resolution.rs:154`) and,
  on a miss, recovery's `recovery_matches` (`:278-310`, descendant test at `:298`);
  Rename bound with `matches` (`src/engine.rs:2205`). All three kept the descendant
  rule, so a directory Source expanded to every file under it, flattened into the
  destination. `Move Override/sub -> <<modDirectory>>/d` moved the whole subtree out
  of `Override`; `Rename Override/sub -> c.txt` renamed every file under it onto one
  name, overwriting as it went.
- A missing Rename Source raised the untyped `MissingSource` cause, so the dry-run
  report showed an error where C# reports `DryRunSkipped` (any missing Rename
  file, not only a directory).

**Status.** Data loss in both: `Delete Override/**` emptied every subfolder, and
`Move`/`Rename` of a directory Source moved or overwrote whole subtrees, where C#
changes nothing.

## Triage of the second follow-up at `df67229`

- **Rename miss typing.** `df67229` typed every Rename miss as
  `MissingSourceFiles`. C# `SetRealPaths(skipExistenceCheck: true)` keeps every
  literal Source (`Instruction.cs:313-345`), so `RenameFile` returns
  `FileNotFoundPost` for a missing literal, but it throws `ArgumentException` on an
  empty expansion (`Instruction.cs:950-953`), which is `UnknownError` on install and
  an Error in the dry run. `Rename Override/nope*.txt -> c.txt` therefore passed
  Rust's dry-run report with `DryRunSkipped` where C# fails it.
- **CleanList, Execute and Run with a folder Source.** They still used the folder
  rule: `CleanList [<<modDirectory>>/d]` read the first file under `d` as its list,
  and Execute/Run (`instruction_sources`) would run every file under a folder
  Source with `--allow-processes` (the probe's `execute_dir` and `run_dir` ran
  `x/run.sh` on `df67229`). C# binds both with `EnumerateFilesWithWildcards`
  and fails: CleanList with `MissingSourceFiles`, Execute with `UnknownError`
  ("Program file does not exist").
- **Missing-file classification (pre-existing).** A missing CleanList list was an
  untyped error (C#: `MissingSourceFiles`, `DryRunSkipped`), and a missing
  Execute/Run program was typed `MissingSourceFiles` (C#: `UnknownError`, an Error
  in its dry run). Both affected only the dry-run report.

## Triage of the third follow-up at `ab6b389`

`ab6b389` typed every CleanList miss as `MissingSourceFiles`, so `CleanList
[<<modDirectory>>/d/*.nope]` passed the dry-run report with `DryRunSkipped`, while C#
`CleanList` throws `ArgumentException` on the empty expansion (`UnknownError`, an
Error in its dry run). Execute/Run left every miss untyped, but C# `ExecuteProgram`
checks `FileExists` on an unexpanded wildcard and returns `FileNotFoundPost`
(`MissingSourceFiles`, `DryRunSkipped`) when every Source is a wildcard that matched
nothing. The literal-versus-wildcard test now lives in one helper.

## Behaviour after this slice

- **Delete.** `Delete` matches only the files its literal or wildcard Source names
  (`Engine::matches_scoped`, `src/engine.rs:1892`, used by the Delete branch at
  `src/engine.rs:2353`). A directory Source matches nothing beneath it, so it is a
  miss:
  - `Overwrite` false or omitted: nothing is deleted and the step succeeds (the
    dry-run report may carry a warning, see Known differences).
  - `Overwrite = true`: unchanged, the typed missing-source cause
    (`DryRunSkipped` in the dry-run report, rejected by plain validate and
    install before any write).
- **Copy, Move, Rename, CleanList, Execute, Run.** `authored_descendants`
  (`src/engine.rs:192`) is false for these actions and Delete. Copy and Move bind
  with `matches_scoped(.., false)` (`src/engine/source_resolution.rs:154-158`), and
  recovery uses the same rule (`recovery_matches`, `:284-329`, with every caller),
  so validate and install agree. Configured FOMOD copies use it too: the mapper
  writes a folder as `folder/*`, and the explicit Extract path already runs those
  copies through the Copy arm. Rename binds with it in the shared Copy/Move/Rename
  arm (`src/engine.rs:2234-2257`), CleanList in its arm (`:2465-2475`), and
  Execute/Run through `instruction_sources` (`:1943`). A directory Source is a
  missing source for all of them, whatever `Overwrite` is.
- **Miss classification.** C# `SetRealPaths(skipExistenceCheck)` keeps every
  literal Source, so its expansion is empty only when every Source is a wildcard,
  and each action classifies the two cases differently. Plain validate and install
  refuse every miss; only the dry-run report differs: `MissingSourceFiles` gives
  `DryRunSkipped`, an untyped error fails the report. One helper,
  `Engine::any_literal_source_missing` (`src/engine.rs:1954`), decides literal
  versus wildcard for all three, through `Engine::classify_miss` (`:1972`;
  `MissKind`, `:201`). A literal Source has no `*` or `?` (a folder included; `[`
  and `]` are literal).

  | Action | A literal Source missing | Only wildcard Sources, none matched |
  | --- | --- | --- |
  | Copy, Move | `MissingSourceFiles` | `MissingSourceFiles` |
  | Rename (`:2287`) | `MissingSourceFiles` | untyped (C# `ArgumentException`, `UnknownError`) |
  | CleanList (`:2499`) | `MissingSourceFiles` | untyped (C# `ArgumentException`, `UnknownError`) |
  | Execute, Run (`:2592`) | untyped (C# `UnknownError`, "Program file does not exist") | `MissingSourceFiles` (C# `FileExists(pattern)` is false: `FileNotFoundPost`) |
- **Wildcards.** `source_wildcard_glob` (`src/engine.rs:203`) turns an authored
  Source into glob text with the C# rules: runs of `*` collapse to one `*` (so
  `**` matches within one level only), `?` stays, and `[` and `]` are escaped as
  literals. `source_pattern` (`src/engine.rs:1925`) and both FOMOD scoping copies
  (`src/fomod/resolved_choices.rs:150`, `src/fomod.rs:919`) use it, so it applies
  to every action's Source. A wildcard reaches only files at its own depth:
  `Override/*` and `Override/**` match files directly in `Override`;
  `Override/*/x.tga` and `Override/**/x.tga` match `x.tga` exactly one level down.
  Literal-Source checks treat only `*` and `?` as wildcards: FOMOD literal archive
  ownership (`resolved_choices.rs:155`) and the sandbox prefix check
  (`src/paths.rs:159`, which now checks bracketed components as well, a stricter
  check).
- **Warning wording.** See Known differences.
- **Unchanged: the folder rule.** Extract, Patcher, the archive preflight, the FOMOD
  gate and `resolved_choices` keep it (FOMOD root-folder flows rely on it for
  Extract). Rust therefore still accepts work that C# refuses; see Known
  differences.
- **Unchanged: brackets in literal paths.** `paths::relative(.., false)` still
  refuses `[` and `]` in literal (non-Source) paths, a check shared by archive
  entries, downloads, checkpoints and destinations. So a Patcher Source with
  brackets is refused (`source_resolution.rs:221`), and a bracketed Move Source is
  never treated as already present at the game target (`redundant_move`,
  `:809`). An Extract Source or default destination with brackets works.
- **Not reused.** `validation/basic.rs:153` `wildcard_path` implements the same C#
  rules as a string matcher, but it is exact-case and takes no `glob::Pattern`.
  The engine needs a reusable pattern matched against case-folded keys, which
  `source_pattern` hands to several callers, so the rules are expressed as glob
  text instead.

## Edge cases compared with the live reference

Every case is in the proof; all match after this slice except the known
differences marked.

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
| `Delete Override/**` | top-level files deleted, subfolders kept |
| `Delete Override/**/y.tga` (`y.tga` two levels down) | nothing deleted |
| `Delete Override/**/x.tga`, `Override/*/x.tga` (`x.tga` one level down) | `sub/x.tga` deleted |
| `Delete Override/su*` (matches only a directory) | nothing deleted |
| `Delete Override/?.tga` | `a.tga`, `s.tga` deleted |
| `Delete Override/[ab].tga`, a file literally named `[ab].tga` present / absent | that file deleted / nothing deleted (`a.tga` kept) |
| `Copy <<modDirectory>>/d/**` with `d/e/n.txt` | only `d/m.txt` copied |
| `Copy Override/[ab].tga` | the literal file copied |
| `Extract <<modDirectory>>/[e]xample.zip`, the archive literally so named, explicit / default destination | extracted (`bracket/`, `[e]xample/`) |
| `Copy` / `Move` of `Override/sub`, omitted / false; `Move Override` | refused (`MissingSourceFiles`), `DryRunSkipped`, nothing changed |
| `Copy <<modDirectory>>/d -> Override` | refused, `DryRunSkipped` |
| `Copy <<modDirectory>>/d/*` (control) | `m.txt` copied |
| `Rename Override/sub -> c.txt`, omitted / false | refused, `DryRunSkipped` |
| `Rename` of a missing file; of a missing literal plus an unmatched wildcard | refused, `DryRunSkipped` |
| `Rename Override/nope*.txt` (only an unmatched wildcard) | refused; dry-run report fails (C#: Error, `UnknownError`) |
| `CleanList [<<modDirectory>>/d]`; a missing list file | refused, `DryRunSkipped` |
| `CleanList [<<modDirectory>>/d/*.nope]` | refused; dry-run report fails (C#: `UnknownError`) |
| `CleanList [<<modDirectory>>/d/clean.csv]` (control) | `a.tga` deleted |
| `Execute` / `Run` of folder `<<modDirectory>>/x` (with `--allow-processes`); a missing program; a missing program plus an unmatched wildcard | refused; dry-run report fails; nothing runs |
| `Execute` / `Run [<<modDirectory>>/x/*.nope]`; `Execute [<<modDirectory>>/*]` (`*` matches only folders) | refused, `DryRunSkipped`; nothing runs |
| `Execute <<modDirectory>>/x/run.sh` (control) | runs, marker written |
| `Copy [<<modDirectory>>/d, <<modDirectory>>/d/m.txt]` | known difference, see below |
| `Rename [Override/a.tga, Override/nope*.txt]` | known difference, see below |
| `Extract <<modDirectory>>/d` (plain files) | install refused in both; dry-run report: known difference |
| `Extract <<modDirectory>>/n` (holds `deep.zip`) | known difference: Rust extracts, C# refuses |
| `Extract <<modDirectory>>/[e]xample.zip` with only `example.zip` present | refused (`MissingSourceFiles`); dry-run report: known difference |

## Known differences

- **Delete warning.** With `Overwrite` false or omitted, when no Source of the step
  matched anything, Rust's `validate --dry-run` report (only; plain validate,
  install and the JSON summary without `--dry-run` show none) carries one
  `DeleteFile` warning per Source of the step: `Delete Source is a directory;
  Delete removes files only: <path>` for a folder Source,
  `Attempting to delete non-existent file: <path>` for any other. None is recorded
  when another Source of the step matched. C# logs only a verbose line and its dry
  run reports no issue. The decision and tree match; the probe does not compare
  ordinary warnings.
- **Extract with a folder Source (Rust accepts what C# refuses).** Extract keeps
  the folder rule, so `Extract <<modDirectory>>/n`, where `n` holds `deep.zip`, is
  accepted and extracts `deep.zip` in validate, the dry-run report and install.
  C# fails it with `MissingSourceFiles`. When the folder holds files that are not
  archives (`Extract <<modDirectory>>/d`), both refuse install, but Rust's dry-run
  report has an error where C# reports `DryRunSkipped`. Kept because FOMOD
  root-folder Extract flows depend on the rule; both are counted as known
  differences, and the probe fails if Rust's folder-archive acceptance changes.
- **Patcher with a folder Source.** Patcher binds one literal file or directory by
  design (`resolve_instruction`); a patcher directory is a supported Source and is
  not changed here.
- **A missing Source beside a matching one (F05-X2).** Rust requires every Source
  to match; C# drops a missing literal or wildcard when another Source matched.
  `Copy [<<modDirectory>>/d, <<modDirectory>>/d/m.txt]` and
  `Rename [Override/a.tga, Override/nope*.txt]` are refused by Rust and accepted by
  C#. `4fb71a6` accepted the Copy by accident (the folder expanded to `m.txt`).
  This is fail-closed, not data loss; the probe counts these as known differences
  and fails if Rust ever accepts them. CleanList and Execute/Run behave the same
  way when one Source matches and another misses: `Execute [run.sh, *.nope]`,
  `Execute [*.sh, *.nope]`, `CleanList [clean.csv, *.nope]` and
  `CleanList [nope.csv, *.csv]` are accepted by C# (which runs or reads the matched
  file) and refused by Rust before anything runs or is deleted. Two of them
  (`Execute [*.sh, *.nope]` and `CleanList [nope.csv, *.csv]`) show as
  `DryRunSkipped` in Rust's dry-run report where C# passes. These mixed cases were
  checked live in review of `e389200` but are not in the probe.
- **CleanList list-file choice with several resolving Sources.** Rust reads the
  first match in sorted-key order (`src/engine.rs`, `sources.first()` in the
  CleanList arm); C# reads `RealSourcePaths[0]`, which follows authored order with
  wildcard expansions first. A CleanList with more than one resolving Source can
  therefore read a different list file. Pre-existing and rare; not changed here.
- **Missing Extract archive in the full report.** For `Extract
  <<modDirectory>>/[e]xample.zip` (and any missing Extract archive, also on
  `2241235`) Rust's `validate --dry-run` report has a ComponentValidation error
  beside the `DryRunSkipped` warning; C# reports only the skip. Validate and
  install match. Counted as a known difference.
- **FOMOD S2b proof.** `scope_char_class_*` relied on `[e]` being a character
  class. It now expects the missing-source refusal, and `scope_bracket_literal_*`
  checks that an archive literally named `[e]xample.zip` is scoped and installs;
  see the [S2b README](fomod-s2b-proof/README.md). Its count is now 265.
- **Case.** C# wildcard matching is ordinal; Rust keeps its case-insensitive keys
  (a deliberate choice). The probe's case variant is a literal directory.
- **Symlinks.** Not probed: Rust refuses any symlink in an install tree (a
  deliberate fail-closed rule, see [copy-move-self-contract.md](copy-move-self-contract.md)).

## Results

| Binary | Delete-directory proof |
| --- | --- |
| third follow-up, live and recorded reference | 169 match / 11 known difference / 0 fail (180 checks) |
| `ab6b389` | 165 / 11 / 4 fail (the four all-wildcard dry runs) |
| `df67229` | 156 / 11 / 13 fail |
| second follow-up (`ab6b389`, 165 checks) | 154 / 11 / 0; `df67229`: 142 / 11 / 12 fail; `4fb71a6`: 103 / 7 / 55; `2241235`: 93 / 7 / 65 |
| first follow-up (`df67229`, 126 checks) | 121 / 5 / 0; `4fb71a6`: 88 / 1 / 37 fail |
| first slice (`4fb71a6`, 57 checks) | 57 / 0; `2241235`: 47 / 10 fail |

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
- `Rename Override/*.txt -> c.txt` (two matches onto one name, Overwrite default):
  both keep one survivor, but which one depends on enumeration order. Rust walks
  its sorted keys; C# walks the file-system listing, which is not a defined order.
  In the sweep run Rust kept `b.txt`'s content and C# `a.txt`'s.
- `CleanList` with a row path containing `"` or `..`: Rust refuses the step as an
  unsafe path (deliberate); C# skips or deletes it. `CleanList` with a row whose
  case differs from the file on disk deletes in Rust; C# on Linux does not find it
  (a platform accident of a case-sensitive file system).
- `Copy` with a `..` segment in Source: Rust refuses it as unsafe (deliberate).
- `DelDuplicate` with `a.tga` and `a.TGA` in one directory: Rust refuses the
  case-colliding tree (deliberate); C# deletes both.

## Notes for guide authors

- `Delete`, `Copy`, `Move`, `Rename`, `CleanList`, `Execute` and `Run` act on files,
  never folders. A Source that names a folder is a missing file: Delete deletes
  nothing (and with `Overwrite = true` fails the mod); the others fail the mod. Use
  a wildcard such as `<<kotorDirectory>>/Override/textures/*` instead.
- `Extract` of a folder is accepted by Rust when the folder holds archives, but C#
  refuses it. Name each archive instead.
- A wildcard reaches only files at its own depth. `*` and `**` both stay within one
  folder level: `Override/**` means the files directly in `Override`, not its
  subfolders. Write one Source per folder level you need, for example
  `Override/textures/*` and `Override/textures/hd/*`.
- `[` and `]` in a Source are ordinary characters, not a character class.
