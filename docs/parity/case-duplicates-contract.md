# Case duplicates in the game folder in the full report

This contract covers the last check of C#'s Environment stage: C# fails `validate
--full` (and `--full --dry-run`, `--full --dry-run-only`) when the game folder holds
two names that differ only in case, with one Environment error; Rust refused the same
folder before any report, so its full report had no stages, `errorCount` 0 and
`componentCount` 0, and its text output was a bare `Error: Case-colliding files:
<path>`. The reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/`
there. Rust lines before this slice are at `0f41c1f`. Evidence:
[case-duplicates-proof](case-duplicates-proof/README.md). Tracker item: F08
("remaining backend classes, exact reports ..."); F08 stays partial.

## Why this gap

Both sides fail these trees, so no pass/fail decision changes; the report does. A
script or the desktop Validate step reading the JSON report saw no failed stage, no
error counted and no component, and the text output did not say what to fix in C#'s
words. On Linux a game folder with both `Override` and `override` (or `a.tga` and
`A.tga`) is a common result of copying mods by hand, so this report is one users meet.
The fix only changes how an existing refusal is reported; it refuses nothing new and
loosens nothing.

Candidates looked at and skipped (all compared live against the C# CLI):

- **A wildcard Delete in the game folder** (`Delete <<kotorDirectory>>/Override/*.txt`
  also removed a game file named `Y.TXT`; C# matches ordinally and keeps it). Wildcard
  case is a documented, deliberate Rust choice (delete-directory contract), so not
  changed here.
- **Basic report with roots** (`validate -s mods -g game` without `--dry-run`): C#
  ignores the roots and fails its FOMOD stage; Rust validates with them and passes.
  Documented Rust extension (basic-validation-without-roots).
- **Components with no instructions, a self-Dependency, a Choose of a missing option, a
  Dependency on a missing GUID, an uppercase `<<MODDIRECTORY>>`, a Source outside the
  mod folder**: C# passes or installs, Rust refuses (all fail closed and documented).
- **Profiles** (`profile create/clone/rename/activate/delete/show`, case-only names,
  `a/b`, `..`, `CON`, padded names, the active profile renamed or deleted): files and
  exit codes match C#.
- **Convert field fidelity** (TOML with every serialized field): Rust keeps fields C#
  drops (`CompatibilityWarning`, `KnownBugs`, `SteamNotes`, `WidescreenOnly`,
  `InstallationWarning`, registry `Files`, an Extract's `Overwrite = false`); no Rust
  loss found. C#'s `IsSelected` rewrite is the convert `--select` area, already
  decided.
- **Patcher namespaces**: Rust's simulation reads `namespaces.ini` and refuses an index
  it cannot resolve; C#'s simulation ignores namespaces. Rust is stricter (fail
  closed).

## Triage

**C# at `d31ccbd3`.** `validate --full` runs the Environment stage first
(`Services/Validation/InstallationValidationPipeline.cs:66-82`), which calls
`InstallationService.ValidateInstallationEnvironmentAsync`. It lists the game
folder's duplicates with `PathHelper.FindCaseInsensitiveDuplicates(MainConfig.DestinationPath.FullName)`
and logs each one as `<path> has a duplicate, please resolve before attempting an
install.` (`Services/InstallationService.cs:935-941`). That helper walks every
subfolder and groups files and folders by `StringComparer.OrdinalIgnoreCase`
(`FileSystemUtils/PathHelper.cs:971-1056`), the same mapping as Rust's path keys
(`src/unicode_case.rs:40-42`); it returns nothing on Windows (`PathHelper.cs:904-907`).
Any duplicate sets `You have duplicate files/folders in your installation directory
in a case-insensitive environment.Please resolve these before continuing. Check the
output window for the specific files to resolve.` (`InstallationService.cs:1059-1065`),
the last assignment of the stage's message, so it replaces the patcher, read-only
folder, duplicate GUID and component messages (`:1014-1057`). The stage fails with
that one message, the report counts one error and stops. The mod folder is not checked.
The basic report (no `--full`) has no Environment stage. `install` runs the full
pipeline first and returns 1 (`CLI/ModBuildConverter.cs:3576-3598`).

**Rust at `0f41c1f`.** `Engine::new` scans both roots and refuses the first name whose
case-folded key it has seen (`src/engine.rs:576-628`: `Case-colliding files: <path>`,
`Case-colliding directories: <path>`). Every command builds its engine before
anything else (`src/main.rs:3279-3281`, desktop `src/ui.rs:4115-4118`), so the full
report (`Engine::validate_report_with_options_observed`, `src/engine.rs:854`, Environment
stage `:915-950`)
never ran: the CLI printed the error JSON (`ErrorReport`) or `Error: ...`, exit 1, and
the desktop Validate step showed a failed job with no report.

**User-visible difference** (`validate -i G -s mods -g game --output json --full`):

```
Case                              C#                         Rust before            Rust after
game Override/a.txt + A.txt       Environment fails,         error, no stages,      as C#, plus
game Override + override            1 error, 1 component       0 errors, 0 comps      one path line
game Override/sub/t.tga + T.TGA
duplicate + missing archive       duplicate message wins     error, no stages       as C#
duplicate + read-only mod folder  duplicate message wins     error, no stages       as C#
duplicates in mods and game       duplicate message          error (mod folder)     as C#
duplicate only in mods            passes                     error, no stages       unchanged (known)
```

## Behaviour after this slice

- **The scan's refusal is typed.** `CaseCollision` (`src/engine.rs:393-411`) carries
  the path; its text is unchanged, so `install`, plain `validate`, `conflicts`, the
  basic report and every other caller print the same error as before
  (`src/engine.rs:617-677`).
- **The full report.** When `Engine::new` refuses for a case collision,
  `Engine::case_collision_report` (`src/engine.rs:2179-2221`) builds the full report:
  it walks the game folder alone with the scan's keys (`game_case_collision`,
  `:413-432`), so a mod folder whose path differs from the game folder's only in case
  cannot make a game name look duplicated; if the game folder has none, the refusal
  stands. The report records no telemetry and, for `--phase remaining`, does not
  repeat the completed-prefix checks (the phase preparation has already run them);
  both still fail.
  Rust's recovery-state checks come first, as in the full report; otherwise the
  Environment stage fails with C#'s message and the found path
  (`Report::environment_case_duplicates`, `src/validation.rs:317-339`), counting one
  error, and the report ends. `componentCount` is the selected components (less the
  completed ones under `--phase remaining`).
- **Callers.** The CLI does this for `validate --full` (with or without `--dry-run`,
  `--dry-run-only`), and for the `--phase remaining` report (`src/main.rs:3279-3313`);
  the desktop Validate step does it too and shows the failed stage
  (`src/ui.rs:4115-4138`). `install`, `conflicts`, plain `validate` and the basic
  report keep the plain refusal.

## Proof

[case-duplicates-proof](case-duplicates-proof/README.md) runs 9 cases under `unshare
-Urnm` on both sides against the live C# CLI with a HoloPatcher stub: `validate
--output json --full` (decision, counts with `componentCount`, stages, the Environment
message), text `--full` (decision, C#'s message present), `--full --dry-run-only`
(decision, counts, message), the basic report `--dry-run` (decision), the line Rust
lists after the message, and a Rust-only `install` check, 107 checks.

| Binary | case-duplicates |
| --- | --- |
| this slice, live C# CLI | 95 match / 12 known difference / 0 fail |
| this slice, recorded references | 95 match / 12 known difference / 0 fail |
| `0f41c1f` (pre-fix), recorded references | 53 match / 12 known difference / 42 fail |

On `0f41c1f` each of the six game-duplicate cases fails seven checks: `full/counts`,
`full/stages`, `full/environment`, `dro/counts`, `dro/environment`, `text/message` and
`details`. The decisions, `install` and the controls are the same on both binaries.

## Known differences

- **A duplicate only in the mod folder** (`mods_files`, 7 checks): C# does not check the
  mod folder and passes; Rust's scan refuses it before any report (fail closed, as
  before).
- **The basic report** (`basic/decision` in five game-duplicate cases): C# checks only
  in its Environment stage, so `--dry-run` without `--full` passes; Rust's scan refuses
  the tree in every mode (fail closed, as before).
- **Which names are listed.** C# logs every duplicate (both spellings); Rust lists the
  one name its scan met second. Which spelling that is depends on the directory
  listing order.
- **Windows.** C# skips the check there; Rust's scan still refuses a duplicate (only
  possible in a case-sensitive Windows folder) and its full report shows C#'s message.
- **`install`** keeps Rust's `Case-colliding files: <path>` refusal before any write;
  C#'s install fails its validation pipeline with the Environment message.

## Notes for guide authors

Nothing changes for guides. Users with two spellings of a file or folder in the game
folder must keep one before Rust (or C#) validates or installs.
