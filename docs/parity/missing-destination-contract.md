# Missing destination folders in the validation report

This contract covers how the validation report's `ComponentValidation` stage reports
a `Copy`, `Move`, `CleanList` or `DelDuplicate` step whose Destination folder does not
exist: the message, how many errors one component gives, and which steps are named.
That stage is what `validate --output json` prints and what the desktop Validate step
shows as the "mod archives" card, with one card per error. It follows
[missing-archives-contract.md](missing-archives-contract.md), which left this message
uncompared. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Evidence:
[missing-destination-proof](missing-destination-proof/README.md). F08 stays partial.

## Why this gap

A missing Destination folder is the archive stage's other common failure, after a
missing download: a guide copies into a folder the user's game does not have yet, or
the game path points one level too high or too low. The decision was already the
same (the report fails), but the message and the count were not, so the Validate page
and the JSON report differed from C# on every such guide. The other differences the
previous slice recorded were left alone: the `DryRun` stage's `hasWarnings` is always
false in C# (C# is inconsistent with its own counts), C# ignoring `-s`/`-g` without
`--dry-run` or `--full` is a C# CLI defect, a restriction or missing dependency failing
Rust's FOMOD and dry-run stages is a deliberate fail-closed choice, and the component
order of the archive stage follows Rust's guide-order install (see Known differences).

## Triage at `1e2d133`

**C# behaviour.** The pipeline's archive stage runs `ComponentValidation.Run` for each
component to validate (`Services/Validation/InstallationValidationPipeline.cs:127-151`,
`:374-425`). `Run` is `VerifyExtractPaths() && ParseDestinationWithAction()`
(`ComponentValidation.cs:42-44`). `ParseDestinationWithAction` (`:246-369`) walks the
component's own steps and then every option's steps, selected or not, and for Copy,
Move, CleanList and DelDuplicate (the `default` arm, `:331-365`):

- builds the path with `UtilityHelper.ReplaceCustomVariables` and
  `PathHelper.FixPathFormatting` (`:333-337`), so the placeholder becomes the root's
  full path, `\` becomes `/` on Linux and a trailing separator is trimmed;
- with case-insensitive pathing (the Linux default), replaces it with
  `PathHelper.GetCaseSensitivePath`, which spells the folders that exist as they are on
  disk (`:338-341`);
- when the path is empty, fails `PathValidator.IsValidPath` (for example a `.` part) or
  is not a directory, adds `Destination cannot be found! Got '<path>'` (`:343-348`)
  and goes on to the next step. An empty Destination gives `Got ''`.

The stage prints each error as `ERROR: <component>: <message>`, counts each in
`errorCount`, and the summary is `<n> component error(s)`
(`InstallationValidationPipeline.cs:388-425`). The wizard's Validate page runs the
same pipeline (`ModSync.GUI/Dialogs/WizardPages/ValidatePage.axaml.cs:367-376`).

The CLI has `AttemptFixes` on (`MainConfig.cs:54`; `CLI/ModBuildConverter.cs:323`), so
after adding the error C# **creates the missing folder** (`ComponentValidation.cs:349-363`).
A later step into the same folder, in the same component or a later one, then passes,
and the dry run that follows sees the folder. With `AttemptFixes` an archive-member miss
also returns `false` from `VerifyExtractPaths` (`:148-171`), so the destination checks
do not run for that component.

**Rust at `1e2d133`.** The report's archive stage is `Engine::validate_archives`
(`src/engine.rs:831-871` at `1e2d133`), which wraps any error of
`validate_component_archives` as `<component>: archive/destination validation:
<cause>`. The destination check was an `ensure!` (`:952-962`), so the first missing
folder ended the component's checks with one error, `Destination cannot be found:
<Destination as written>`. An empty Destination failed path resolution with `Path must
start with <<modDirectory>> or <<kotorDirectory>>: `.

**User-visible difference.**

```
C#    ERROR: A: Destination cannot be found! Got '/games/kotor/Nope'
      ERROR: A: Destination cannot be found! Got '/games/kotor/Nope2'         errorCount 2
Rust  ERROR: A: archive/destination validation: Destination cannot be found: <<kotorDirectory>>/Nope
                                                                              errorCount 1
```

## Change

`validate_component_archives` (`src/engine.rs:876`) now collects a
`Destination cannot be found! Got '<path>'` message for each Copy, Move, CleanList or
DelDuplicate step whose Destination folder is missing (`:951-974`) and returns them
after the remaining checks (`:1020`). An empty Destination gives `Got ''` (`:957-961`).
`shown_destination` (`:1023-1041`) prints the resolved full path, with the existing
folders in their on-disk spelling, as `GetCaseSensitivePath` does; the path uses the
host's separator. Any other failure still ends the component's checks with one
`archive/destination validation` error, so an archive-member miss later in the
component wins over the missing folders, as in C#. Missing archives still skip these
checks (`:929-950`).

`validate_archives` (`:831`) already returns a list of errors per component, and
`Report::component_validation` (`src/validation.rs:273`) counts and prints each one;
its only caller is the report pipeline (`src/engine.rs:682-687`), which the CLI and the
desktop Validate step share. The desktop builds one card per `ERROR:` message
(`src/validation/presentation.rs:125-145`), and `Report::first_issue`
(`src/validation.rs:218`) picks the first error, so both now show each missing folder.
Plain `validate`, `install` and the dry run do not use this stage and are unchanged.
The basic validator without roots (`src/validation/basic.rs:574-643`) already used the
C# wording and is unchanged.

## Cases and results

`validate --output json --dry-run` on both sides; each case is checked three ways
(decision, counts, archive stage). `KD` is `<<kotorDirectory>>`, `GD`
`<<gameDirectory>>`, `MD` `<<modDirectory>>`; `<root>` is the fixture folder.

| Case | Steps | Reference | Rust now | Rust at `1e2d133` |
| --- | --- | --- | --- | --- |
| `control_override`, `control_case`, `control_extract_copy` | Copy into `KD/Override`, `KD/override`; Extract then Copy | pass | match | match |
| `missing` | Copy into `KD/Nope` | `Got '<root>/game/Nope'` | match | other wording |
| `two_missing` | `KD/Nope`, `KD/Nope2` | two errors | match | one error |
| `missing_between_present` | Override, Nope, Override, Nope2 | two errors | match | one error |
| `backslash_nested` | `KD\Nope\Deeper` | `Got '<root>/game/Nope/Deeper'` | match | other wording |
| `trailing_separator` | `KD/Nope/` | `Got '<root>/game/Nope'` | match | other wording |
| `case_of_parent` | `KD/override/Nope` | `Got '<root>/game/Override/Nope'` | match | other wording |
| `game_directory`, `mod_directory` | `GD/Nope`, `MD/Nope` | full path | match | other wording |
| `move`, `cleanlist` | Move, CleanList into `KD/Nope` | one error | match | other wording |
| `empty_destination`, `no_destination` | `Destination = ""`, none | `Got ''` | match | other wording |
| `option_missing` | the selected option's Copy | one error | match | other wording |
| `base_and_option_missing` | own and option Copy | two errors | match | one error |
| `unselected_option_missing` | an unselected option's Copy | one error | match | other wording |
| `extract_then_copy_missing` | Extract, Copy of a member into `KD/Nope` | one error | match | other wording |
| `missing_archive_and_destination` | missing archive, Copy into `KD/Nope` | `Missing Required Archives` only | match | match |
| `two_components`, `second_component_missing` | errors in A and B, or B only | per component | match | other wording |
| `duplicate_destination` | `KD/Nope` twice | one error | known difference (counts, stage) | other wording |
| `same_folder_two_components` | A and B into `KD/Nope` | A only, `OK: B` | known difference (counts, stage) | other wording |
| `deldup` | DelDuplicate into `KD/Nope` | one error; dry run passes | known difference (counts) | other wording |
| `dot_part` | `KD/Override/.` | `Got '<root>/game/Override/.'` | known difference (stage) | other wording |
| `dot_dot_part` | `KD/Override/../Nope` | `Got '<root>/game/Nope'` | known difference (counts, stage) | known difference |
| `member_miss_and_missing` | Extract, Copy of an absent member into `KD/Nope` | two member errors | known difference (counts, stage) | missing folder only |
| `missing_then_bad_patcher` | Copy into `KD/Nope`, Patcher into `KD/Override` | two errors | known difference (counts, stage) | missing folder only |

## Known differences

- **Windows path prefix.** On Windows the roots are canonicalized, so the message reads
  `Got '\\?\C:\Games\...\Nope'` where C# prints `C:\Games\...\Nope`. A symlinked
  root on Linux likewise prints its canonical path. Not probed.
- **Steps with unmet instruction dependencies or restrictions.** Rust only checks steps
  whose instruction-level `Dependencies`/`Restrictions` apply (`instruction_applies`
  in `src/engine.rs`); C# checks every step, so a Copy into a missing folder behind an
  unmet dependency fails the C# stage while the Rust report passes. Both installers
  skip that step, so the outcome is safe. Predates this slice.
- **Other count differences (both fail).** A Destination with no placeholder or a
  placeholder in the wrong case: C# reports `Destination cannot be found! Got
  '<cwd-relative path>'` (1 error); Rust reports `Path must start with...` (2 errors).
  A Destination that is an existing file: C# adds `The file '...' already exists.` from
  its `CreateDirectory` attempt (2 errors); Rust reports 1.
- **Probe and symlinked temp roots.** The probe replaces the fixture root with
  `<root>` without resolving symlinks, so it would mismatch on a host whose temp
  directory is a symlink (for example macOS `/var`). Fine on Linux.

- **C# creates the missing folder while validating (deliberate).** With `AttemptFixes`
  on, C# creates each missing Destination folder in the game or mod directory as soon
  as it reports it. A second step into that folder is then not reported
  (`duplicate_destination`, and `same_folder_two_components`, where C# shows `OK: B`),
  the dry run that follows passes a DelDuplicate into it (`deldup`, where Rust's dry
  run still fails), and running validation again passes. Rust validation never changes
  the game or mod trees, so it reports every step into a missing folder, every time.
- **Other failures end the component's checks (unchanged, fail-closed).** A Patcher or
  Rename Destination that Rust refuses, an unsafe path, or an unsupported action ends
  the component's checks with one `archive/destination validation` error, and the
  missing folders found so far are not listed (`missing_then_bad_patcher`; C# lists
  both). Rust also does not report a Delete, Extract or Choose step with a Destination,
  which C# does. An archive-member miss hides the missing folders in both, but keeps its
  Rust wording, and C# adds a second message for its `AttemptFixes` retry
  (`member_miss_and_missing`).
- **`.` and `..` parts.** A `.` part fails in both; Rust prints the resolved folder
  without it (`<root>/game/Override`, C# `<root>/game/Override/.`). Rust refuses a `..`
  part as `Unsafe path` (fail-closed), so its dry run fails too and `errorCount` is
  higher; C# resolves it and reports the folder.
- **Paths.** The message holds the full path of the game or mod directory as Rust
  resolved it. On Windows, C# turns off case-insensitive pathing and prints the
  Destination's own spelling, while Rust prints existing folders in their on-disk
  spelling there too. Not probed.
- **Report fields outside this slice (not changed).** As recorded in
  [missing-archives-contract.md](missing-archives-contract.md): the `DryRun` stage's
  `hasWarnings`; the archive stage's component order (guide order in Rust, dependency
  order in C#, which reorders components when it loads the guide); C# ignoring `-s`/`-g`
  without `--dry-run` or `--full`; a restriction or missing dependency failing Rust's
  FOMOD and dry-run stages. Also seen in this comparison and left open: with
  `--use-file-selection` and nothing selected, C# selects every component
  (`ModComponentSerializationService.cs:895-903`), while Rust refused the guide (closed
  in [file-selection-fallback-contract.md](file-selection-fallback-contract.md)); a
  component with no instructions passes in C# and fails Rust's dry run; `--full` needs
  a packaged HoloPatcher in C# and fails its Environment stage without one.
- **Platform.** The reference ran on Linux with `DOTNET_ROLL_FORWARD=Major` (the pinned
  build targets .NET 9; the host has .NET 10). The probe's paths have no symlinks.
