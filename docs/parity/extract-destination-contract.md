# Extract with a Destination

This contract covers an `Extract` step that names a `Destination`, such as
`Extract [<<modDirectory>>/m.zip] -> <<kotorDirectory>>/Override`. C# validation
clears that Destination before the dry run and the install, so the C# CLI and GUI
always extract to a folder named after the archive, next to it (`mods/m/`). Rust
extracted into the Destination, so the same guide put different files in different
places, and a later step written for C# (`Move <<modDirectory>>/m/Override/*`) failed
in Rust. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Rust lines before this slice are at `c0e9cef`. Evidence:
[extract-destination-proof](extract-destination-proof/README.md). F05-EXTRACT and F06
(layout) stay partial.

## Why this gap

The actions audit lists default/explicit Extract layout as an open F06 row
(F06-LAYOUT) and records C#'s real provider as extracting directly into an explicit
Destination (`RealFileSystemProvider.cs:153-202`). Run live, the C# CLI never does:
its validation empties the Destination first. A guide authored and tested with C#
that names an Extract Destination therefore names the archive-stem folder in its
later steps, and Rust failed it (or, with no later step, wrote the archive's files,
readme included, into `Override` or the game root). It changes a pass/fail decision
and the files installed, and matching C# is safer: an Extract only ever writes next
to its archive.

## Triage

**C# at `d31ccbd3`.** `ComponentValidation.Run` runs `VerifyExtractPaths` and then,
only if it passed, `ParseDestinationWithAction` (`ComponentValidation.cs:42-44`).
That checks every step of the component and of its immediate options (`:246-259`);
for `Choose`, `Extract` and `Delete` with a non-empty Destination it adds the error
`'Destination' key cannot be used with this action. Got '<Destination>'` and, with
`MainConfig.AttemptFixes`, sets the Destination to empty and reports success
(`:293-312`). AttemptFixes is on by default (`MainConfig.cs:51-54`; the GUI's
`AppSettings.AttemptFixes`, `src/ModSync.GUI/Models/AppSettings.cs:39` from the
repository root). The full pipeline's Environment stage
(`Services/Validation/InstallationValidationPipeline.cs:270-290`) calls
`InstallationService.ValidateInstallationEnvironmentAsync`
(`Services/InstallationService.cs:794`), which runs `ComponentValidation` on every
selected component (`:1007-1009`) and so clears the Destination; the pipeline's own
ComponentValidation stage then finds nothing. Without the Environment stage (the
basic report, `validate --dry-run` without `--full`) the ComponentValidation stage
runs it first and counts the error
(`Services/Validation/InstallationValidationPipeline.cs:374-400`), so the report
fails, although the Destination is cleared for the dry run after it. The CLI `install` runs the full
pipeline before installing unless given `--skip-validation`, and the GUI installs
after its Validate step. The Extract arm (`ModComponent.cs:1575-1584`) then calls
`ExtractFileAsync`, which uses `RealDestinationPath ?? Path.GetDirectoryName(source)`
(`Instruction.cs:496`); with no Destination that is the archive's folder, which the
real provider treats as the default layout and extracts into
`<archive folder>/<archive stem>` (`Services/FileSystem/RealFileSystemProvider.cs:159`,
`:199-202`). A Delete never uses its Destination and a Choose has none to use.

Run live (contained, see the proof): for every Destination tried (`Override`, with
`/` or `\`, the game root, an existing or a missing mod folder, `<<modDirectory>>`
itself, one outside the roots, and the same Extract in a chosen option) the full
report passes with no error, the basic report fails with that one error, and the
install extracts to `mods/m/`; a later `Move <<modDirectory>>/m/Override/*` installs.
(A later step that names the Destination fails on its own; see Known differences.)

**Rust at `c0e9cef`.** `extraction_destination` (`src/engine.rs:2310-2324`) used the
Destination unless it resolved to the archive's own folder. Its callers are the
ComponentValidation (archive) stage's member checks (`validate_component_archives`,
`:1089`), the engine's Extract arm (`:2499-2507`; the dry run, plain `validate`, the
install, `conflicts`, checkpoint runs) and implicit source recovery (`plan_extract`,
`src/engine/source_resolution.rs:469`, which always passes an empty Destination). The
archive stage passed an Extract, Choose or Delete Destination unchecked
(`src/engine.rs:1173`).

**User-visible difference** (`A` extracts `mods/m.zip`, holding `Override/a.tga` and
`readme.txt`):

```
Extract Destination          C# (CLI, GUI)              Rust before                    Rust after
Override, game root,         basic report fails;        reports pass; extracts into    as C#
a mod folder                 extracts to mods/m/        the Destination
... then Move m/Override/*   installs                   refused (Move finds nothing)   installs
... then Copy <Dest>/a.tga   refused                    installs                       refused
<<modDirectory>>             basic report fails;        reports pass; mods/m/          as C#
                             mods/m/
outside the roots (../x)     installs to mods/m/        refused (Unsafe path)          refused (unchanged)
```

## Behaviour after this slice

- **The rule.** `extraction_destination` (`src/engine.rs:317-323`) is now a function
  of the archive alone: every Extract unpacks into a folder named after the archive's
  stem, next to the archive, whatever its Destination, in the archive stage's member
  checks (`:1098`), the Extract arm (`:2522`) and recovery
  (`src/engine/source_resolution.rs:469`). The Extract arm (`src/engine.rs:2504-2517`)
  still resolves a non-empty Destination as a path, so an unsafe one (outside the
  roots, `..`, an unknown placeholder) is still refused before any write (fail
  closed), and it logs `Extract Destination '<Destination>' is not used; each archive
  extracts to a folder named after it, next to the archive`.
- **The basic report.** The archive stage (`validate_component_archives`,
  `:1141-1221`) now gives `'Destination' key cannot be used with this action. Got
  '<Destination>'` for each Extract, Choose or Delete step with a Destination when
  the report is not the full pipeline (`:1183-1193`), one error per step, and goes
  on, as the C# ComponentValidation stage does; the full pipeline (`validate --full`,
  the desktop Validate step) passes it, as C#'s Environment stage has cleared it.
  Plain `validate` and `install` are not reports and pass.
- **Unchanged.** An Extract without a Destination, or with the archive's own folder,
  extracts as before. A missing archive still ends the component's archive checks
  first (C# `Run` stops at `VerifyExtractPaths`). Configured FOMOD extraction maps
  from the archive's folder, which is always in the mod directory when the archive
  is.

Callers checked. `extraction_destination` has the three callers above; the Extract
arm serves the dry run, plain `validate`, `install`, the reports' DryRun stage,
`conflicts`, the FOMOD gate's extraction and checkpoint runs, so they all follow the
new layout (a `conflicts` report no longer lists an Extract as a writer of game files
it now extracts into the mod folder). The rootless basic validation
(`src/validation/basic.rs:599-602`) already reported the same error and is unchanged.
No other code reads an Extract Destination (`download_plan`, `download_orchestrator`
and `fomod::resolved_choices` read only its Sources).

## Proof

[probe.py](extract-destination-proof/probe.py) runs 13 cases through the full report
with a dry run, the basic dry-run report, `install` and, for Rust, plain `validate`,
compared with the live C# CLI with a HoloPatcher stub (91 checks).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 82 match / 9 known difference / 0 fail |
| this slice, recorded reference | 82 match / 9 known difference / 0 fail |
| `c0e9cef` (pre-fix), recorded reference | 44 match / 6 known difference / 41 fail |

On `c0e9cef` the 41 failures are: the six cases whose Extract (in `A` or its chosen
option) names a Destination other than the archive's folder and has no later step
passed the basic report and installed into the Destination (decision, counts and
install: 18); `archive_folder` and `delete_destination` passed the basic report
(decision and counts: 4); the two `_then_move` cases were refused by Rust's full
report, plain `validate` and `install`, which C# installs (all but the basic
decision: 12); and `mod_folder_then_copy` passed everywhere and installed, which C#
refuses (7). `outside_roots` gave the same refusal on both builds.

Other proofs whose expectations changed:
[root-destination](root-destination-proof/README.md) (its known acceptance of an
Extract Destination in the reference pipeline is now a match: 114 match / 9 known),
[delete-directory](delete-directory-proof/README.md) (167 match / 13 known),
[empty-source](empty-source-proof/README.md) (49 match / 2 known),
[extract-overwrite](extract-overwrite-proof/README.md) (its cases with a Destination
are compared with the live C# CLI: 4 match / 16 known) and
[fomod-s2b](fomod-s2b-proof/README.md) (`scope_bracket_literal` no longer names a
Destination; 265/265). Their references other than the C# CLI are
`ModComponent.InstallAsync` or `DryRunValidator` alone, which never run
`ComponentValidation`.

## Known differences

- **A Destination outside the roots (unchanged, fail closed).**
  `<<modDirectory>>/../outside`: C# clears it and installs to `mods/m/`; Rust refuses
  it as a path in the dry run, plain `validate` and `install`, as before, though it no
  longer uses it. Six checks. An Extract Destination has no use in either; delete it.
- **A later step that names the Destination (not this slice).** In
  `mod_folder_then_copy` both refuse, but C# first fails `VerifyExtractPaths` (two
  `Failed to find ... in any archives!` errors, so no Destination error) and fails the
  full pipeline in its Environment stage, where Rust gives one archive-stage error.
  Three checks (counts and stages).
- **`install --skip-validation`.** C# skips validation, so the Destination is kept
  and its real provider extracts into it. Rust has no such flag.
- **C#'s dry run without ComponentValidation.** `validate --dry-run-only` skips the
  ComponentValidation stage, so C#'s virtual provider keeps the Destination and
  extracts to `<Destination>/<archive stem>` (`VirtualFileSystemProvider.cs:932`).
  Rust's dry run follows its install. Not probed.
- **Messages.** The basic report's error text is C#'s; the log line is Rust's own.
- **Windows.** Not run; nothing here depends on the host.
- **Found, left open.** The C# CLI's validation also refuses a guide whose later
  Copy names a loose file in the mod folder that no archive of the component holds
  (`Failed to find '<Source>' in any archives!`), even when the file exists; Rust's
  plain `validate` and `install` do not run that check (seen in the extract-overwrite
  proof's `stale_*` cases).

## Notes for guide authors

Do not give an `Extract` step a Destination: neither C# nor Rust uses it, and the
basic validation report fails it. Name the extracted files under the folder named
after the archive (`<<modDirectory>>/m/...` for `m.zip`) in later Move or Copy steps.
