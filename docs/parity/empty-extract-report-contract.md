# An Extract with no Source in the validation reports

This contract covers an `Extract` step whose `Source` list is empty after loading
(`Source = []`, `Source = [""]` or no `Source` key) in a component none of whose
Extract steps names an archive that exists. The C# CLI refuses such a guide in every
report and in `install`; Rust's plain `validate` and `install` refused it too, but its
reports with a dry run passed it with a `DryRunSkipped` warning. The reference is
pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there. Rust lines before
this slice are at `b8b4d57`. Evidence:
[empty-source-proof](empty-source-proof/README.md) and
[missing-archives-proof](missing-archives-proof/README.md). The tracker item
"empty-Source full report" (found by the extract-destination slice) is closed;
F05-EXTRACT and F08 stay partial.

## Why this gap

It changes a pass/fail decision: the desktop Validate step and `validate --full
--dry-run` passed a guide that the C# CLI and GUI refuse and that Rust's own `install`
then refused. The validation report is meant to predict the install, so a report that
passes and an install that fails is the wrong answer. The step is usually a
placeholder left while editing a guide, or an archive name lost in conversion.

## Triage

**C# at `d31ccbd3`.** `ComponentValidation.Run` runs `VerifyExtractPaths` first
(`ComponentValidation.cs:42-44`). When `GetAllArchivesFromInstructions` (`:191-243`)
finds no archive that exists, each of the component's own Extract steps is one error,
`Missing Required Archives: [<Sources joined by ",">]`, so `[]` for an empty Source,
and the check returns (`:74-89`). When an archive exists, Extract steps are skipped
(`:108-110`), so an empty Extract next to one passes validation. The full pipeline
runs `ComponentValidation` in its Environment stage
(`Services/InstallationService.cs:1007-1009`) and stops on failure
(`Services/Validation/InstallationValidationPipeline.cs:66-81`); the basic report runs
it in its ComponentValidation stage (`:374-425`). The CLI `install` runs the full
pipeline first and stops unless given `--skip-validation`
(`CLI/ModBuildConverter.cs:3576-3599`). Execution fails an empty Extract with
`FileNotFoundPre` (`ModComponent.cs:1575-1583`), so when an archive exists the install
fails at that step.

Run live (contained, see the proof), for the empty-source cases that copy two loose
files and then `Extract []` (with or without a Destination, or no Source key): the
full report fails in its Environment stage, the basic report fails with
`Missing Required Archives: []`, and `install` stops before any write. An empty
Extract beside an existing `m.zip` (`extract_empty_beside_archive`) passes both
reports and fails `install`.

**Rust at `b8b4d57`.** The reports' archive stage (`validate_component_archives`,
`src/engine.rs:1166-1267`, called at `:922-924`) skipped an Extract with no Source
while collecting archives (`archive_members`, `:1326-1330`), so it reported nothing,
and the dry run reported `DryRunSkipped` and passed (see
[empty-source-contract.md](empty-source-contract.md)). Plain `validate` and `install`
simulate the step, which fails with `Extract has no Source`
(`src/engine/source_resolution.rs:142-146`), so they refused, before any write, as
C# `install` does. The CLI report without `--dry-run` simulates the steps too and
failed.

**User-visible difference** (component `A` copies `a.txt` and `b.txt`, then
`Extract []`):

```
                            C# CLI, GUI         Rust before         Rust after
--full --dry-run, desktop   fails (Environment) passes, 1 warning   fails
basic report (--dry-run)    Missing Required    passes (with no     Missing Required
                            Archives: []        Destination)        Archives: []
plain validate, install     refused             refused             refused
```

With an Extract Destination the Rust basic report already failed, with the
Destination error of the extract-destination slice instead of C#'s.

## Behaviour after this slice

- `archive_members` (`src/engine.rs:1321-1378`) records an Extract with no Source
  with the steps whose Source names no file (`:1335-1339`).
- `validate_component_archives` keeps it only when the component has no archive and
  the step is the component's own (`:1179-1185`), so it reports `Missing Required
  Archives: []` there, in step order with any missing archive, and skips the
  component's other checks, as C# does. Otherwise it is left out, and execution
  refuses it as before.
- So the basic report, the full report (with or without `--dry-run`), the desktop
  Validate step and `--phase remaining` fail such a component in their
  ComponentValidation stage. `[Extract [], Extract nope.zip]` now gives two errors, as
  in C#.
- Unchanged: plain `validate` and `install` (they do not run this stage, and already
  refused before any write); an empty Extract beside an existing archive (the reports
  pass with `DryRunSkipped`, plain `validate` and `install` refuse); an empty Extract
  in an option (not reported, as in C#); the FOMOD gate, which skips it; the rootless
  basic validation (`src/validation/basic.rs:374-385`), which already reported it.

Callers checked: `archive_members` is used only by `validate_component_archives`,
whose only caller is the report pipeline (`validate_archives`, `:1113-1154`); the
`missing` list it returns is read only there.

## Proof

The [empty-source probe](empty-source-proof/README.md) now also runs the C# CLI on
every case (`--cli-reference`, a copy of the `d31ccbd3` build with a HoloPatcher stub
under `unshare -Urnm`) and compares Rust's `validate --output json --full --dry-run`
(exit code and `success`) and `validate --output json --dry-run` (no error, and the
ComponentValidation stage's error lines) with it; a new case,
`extract_empty_beside_archive`, is compared with the C# CLI only (89 checks).

| Binary | Result |
| --- | --- |
| this slice, live C# CLI (`--cli-reference ... --record` on `b8b4d57`, then without `--record`) | 86 match / 3 known difference / 0 fail |
| this slice, recorded references | 86 match / 3 known difference / 0 fail |
| `b8b4d57` (pre-fix), live and recorded | 80 match / 2 known difference / 7 fail |

On `b8b4d57` the seven failures are the full report of `extract_empty`,
`extract_blank_nodest` and `extract_nokey` (Rust passed), their basic report's
ComponentValidation errors (none, or the Destination error), and
`extract_blank_nodest`'s dry-run check, now a known difference that requires Rust
and the C# CLI basic report to refuse.

The [missing-archives probe](missing-archives-proof/README.md) case
`empty_source_only` was a known difference (C# `Missing Required Archives: []`, Rust
passed); it now matches: 50 match / 19 known of 69 (47 / 19 / 3 fail on `b8b4d57`
with the updated `KNOWN`; recorded reference).

## Known differences

- **The dry run alone.** The empty-source probe's own reference,
  `DryRunValidator.ValidateInstallationAsync`, reports `DryRunSkipped` and passes
  these three cases; it does not run `ComponentValidation`. Rust's basic report fails
  them, as the C# CLI basic report does. 3 checks.
- **Stage.** C#'s full pipeline stops in its Environment stage with one error; Rust's
  passes Environment and fails its ComponentValidation stage. The decision is the
  same; only the decision is compared.
- **An Extract that does not run.** C# reports every own Extract of a component with
  no archive, including one whose instruction-level dependency is not selected; Rust
  checks only steps that run (unchanged, see
  [missing-archives-contract.md](missing-archives-contract.md)).
- **Not probed.** `install --skip-validation` (C# fails at the step after the earlier
  steps; Rust has no such flag). Windows: not run; nothing here depends on the host.

## Notes for guide authors

An `Extract` with no Source now fails validation, not only the install. Give it the
archive's path (`<<modDirectory>>/name.zip`), or remove the step.
