# Missing archives in the validation report

This contract covers how the validation report's `ComponentValidation` stage reports
an Extract whose archive is not in the mod folder: the message, how many errors one
component gives, and which steps are named. That stage is what `validate --output
json` prints and what the desktop Validate step shows as the "mod archives" card. The
reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Evidence: [missing-archives-proof](missing-archives-proof/README.md). F08 stays
partial.

## Triage at `3024e4d`

**C# behaviour.** The pipeline's archive stage runs `ComponentValidation.Run` for each
component to validate (`Services/Validation/InstallationValidationPipeline.cs:132`, `:374-425`).
`Run` is `VerifyExtractPaths() && ParseDestinationWithAction()`
(`ComponentValidation.cs:42-44`). `VerifyExtractPaths` collects the component's
archives from every Extract that runs, its own and its options'
(`GetAllArchivesFromInstructions`, `:191-242`). When none of them exists, it adds one
error for each of the component's own Extract steps,
`Missing Required Archives: [<Sources joined by ",">]`, and returns, so the
destination and archive-member checks are skipped (`:74-88`). The stage prints each
error as `ERROR: <component>: <message>`, counts every error in `errorCount`, and
sets the summary to `<n> component error(s)` (`InstallationValidationPipeline.cs:374-425`).
The wizard's Validate page runs the same pipeline
(`ModSync.GUI/Dialogs/WizardPages/ValidatePage.axaml.cs:367-376`).

**Rust at `3024e4d`.** The report's archive stage is `Engine::validate_archives`
(`src/engine.rs:823-851` at `3024e4d`), which wraps each component's
`validate_component_archives` error as `<component>: archive/destination validation:
<cause>`. Archive collection called `self.matches(&instruction.source, false)?`
(`:876`), which stops at the first Source that matches no file. So a missing download
gave one error per component, `Source matches no files: <<modDirectory>>/nope.zip`,
naming only the first missing archive.

**User-visible difference.** Missing downloads are the most common validation
failure. For a component with two missing archives, C# lists both and counts two
errors; Rust listed one, with a different message, and counted one:

```
C#    ERROR: A: Missing Required Archives: [<<modDirectory>>/nope.zip]
      ERROR: A: Missing Required Archives: [<<modDirectory>>/nope2.zip]      errorCount 2
Rust  ERROR: A: archive/destination validation: Source matches no files: <<modDirectory>>/nope.zip
                                                                              errorCount 1
```

The decision (the report fails) was already the same.

## Change

`validate_component_archives` (`src/engine.rs:875`) now matches each Extract Source
with `missing_ok` and records every Extract step that has a Source matching no file
(`:891-901`). If any is recorded, it returns one `Missing Required Archives: [...]`
message per such step and skips the remaining checks, as C# does (`:933-951`). When
the component has no archive at all and at least one recorded step is the component's
own, only its own steps are reported, as in C# (`is_top_level`, `:217`). Other
failures (an unsafe path, a corrupt archive, a missing destination) stay a single
`archive/destination validation` error, as before.

`validate_archives` (`:831`) returns a list of errors per component, and
`Report::component_validation` (`src/validation.rs:273`) counts and prints each one;
its only caller is the report pipeline (`src/engine.rs:684-686`), which the CLI and the
desktop Validate step share. Plain `validate`, `install` and the dry run do not use
this stage and are unchanged.

## Cases and results

`validate --output json --dry-run` on both sides; each case is checked three ways
(decision, counts, archive stage). `MD` is `<<modDirectory>>`.

| Case | Steps | Reference | Rust now | Rust at `3024e4d` |
| --- | --- | --- | --- | --- |
| `control_copy`, `control_extract` | no Extract; an existing archive | pass | match | match |
| `control_empty_source_and_present` | `Extract []`, then an existing archive | pass | match | match |
| `missing` | `Extract MD/nope.zip` | one error | match | other wording |
| `two_missing` | two missing archives | two errors | match | one error |
| `two_sources_missing` | one Extract, two missing Sources | `[MD/nope.zip,MD/nope2.zip]` | match | first Source only |
| `wildcard_missing` | `Extract MD/*.7z`, no match | `[MD/*.7z]` | match | other wording |
| `backslash_missing` | `<<modDirectory>>\nope.zip` | `[MD/nope.zip]` | match | other wording |
| `duplicate_missing` | the same missing Extract twice | two errors | match | one error |
| `missing_then_copy` | missing archive, then a Copy from it | one error | match | other wording |
| `missing_bad_destination` | missing archive, Copy to a missing folder | one error (destination not checked) | match | other wording |
| `base_and_option_missing` | own and option Extract missing | own step only | match | other wording |
| `second_component_missing` | `OK: A`, B missing | `OK: A`, one error | match | other wording |
| `two_components_missing` | A one missing, B two | three errors | match | two errors |
| `unselected_missing` | missing archive in an unselected component (`--use-file-selection`) | pass | match | match |
| `partly_missing` | an existing archive and a missing one | stage passes | known difference | known difference (other wording) |
| `partly_missing_sources` | one Extract, `a.zip` and `nope.zip` | stage passes | known difference | as above |
| `option_only_missing` | only an option Extract, missing | stage passes; dry run fails | known difference (counts, stage) | as above |
| `option_present_base_missing` | own Extract missing, option archive present | stage passes | known difference | as above |
| `empty_source_only` | `Extract []` only | `Missing Required Archives: []` | match (since empty-extract-report) | known difference |
| `conditional_present` | an Extract that does not run (unmet instruction dependency), archive present | one error | known difference | known difference |
| `conditional_missing` | the same, archive missing | one error | known difference | known difference |
| `missing_and_conditional_missing` | a missing Extract and one that does not run | two errors | known difference (counts, stage) | other wording, one error |

## Known differences

- **Missing archives hidden by a later archive failure.** When a present archive in the
  same component is corrupt or its extraction folder is blocked, Rust reports only that
  failure, so the missing downloads appear after it is fixed (`src/engine.rs`, the
  `?` in the first loop of `validate_component_archives`). C# passes the stage in that
  case. Still fail-closed. A missing archive also hides the component's destination and
  action checks, as in C#.

- **Some of a component's archives are missing (deliberate, fail-closed).** When at
  least one of a component's archives exists, C# does not report a missing one in this
  stage; with an existing archive in an option it does not report a missing one of
  the component's own either. The report can then pass, and the install fails at that
  Extract. Rust reports each Extract step with a missing Source
  (`partly_missing`, `partly_missing_sources`, `option_present_base_missing`), and
  an option-only Extract whose archive is missing (`option_only_missing`, where C#
  fails the dry run instead). The message names all of that step's Sources, as C#
  does, even those that exist. Rust also requires every Source of an Extract to match
  (see the CleanList and wildcard notes in `COMPATIBILITY.md`).
- **An Extract that does not run (deliberate).** C# reports every own Extract step of
  a component with no archive, including a step whose instruction-level dependency is
  not selected, and so fails the report even when that archive exists
  (`conditional_present`). Rust checks only steps that run, as its other checks do.
- **An Extract with no Source (resolved 2026-10-05).** C# reports `Missing Required
  Archives: []` whenever the component has no archive (so `[Extract [], Extract
  nope.zip]` gives two errors). Rust passed the empty Extract here; since
  [empty-extract-report-contract.md](empty-extract-report-contract.md) it reports it
  as C# does (50 match / 19 known of 69).
- **Report fields outside this slice (not changed).** Found while triaging, compared
  with the live C# CLI and left open: the `DryRun` stage's `hasWarnings` is always
  `false` in C# but follows its warnings in Rust; the stage lists components in guide
  order in Rust and in dependency order in C# (`InstallAfter`); without `--dry-run` or
  `--full`, C# ignores `-s`/`-g` and fails the FOMOD stage (`mod directory is not
  set`), while Rust uses them; a component-level restriction or missing dependency fails the
  Rust FOMOD and dry-run stages, while C# reports it only in its `Conflicts` stage,
  which needs `--full`.
- **Other archive-stage messages (not compared).** A corrupt archive, a blocked
  extraction folder and an archive-member miss keep their Rust wording
  (`archive/destination validation: ...`); C# has its own messages for these. A
  missing Destination folder now uses the C# message, one per step; see
  [missing-destination-contract.md](missing-destination-contract.md).
- **Platform.** The reference ran on Linux with `DOTNET_ROLL_FORWARD=Major` (the
  pinned build targets .NET 9; the host has .NET 10). Nothing here depends on the
  platform.
