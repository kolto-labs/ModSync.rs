# An install root as a step's Destination

This contract covers a `Copy`, `Move`, `CleanList` or `DelDuplicate` step whose
Destination is an install root itself, and the follow-up cases the review of
`6e67b3c` found: a bare root as Source, a `.` Destination part at execution, and an
Extract into the mod root. The Destination is an install root when it is
`<<kotorDirectory>>`, `<<gameDirectory>>` or `<<modDirectory>>`, with or without a
trailing separator. The `DelDuplicate` form was
recorded in the sweep findings of
[delete-directory-contract.md](delete-directory-contract.md) and in the F05-DEDUP row
of the [actions audit](engine-actions-archives-audit.md). The reference is pinned C#
`d31ccbd3`; C# paths below are under `src/ModSync.Core/` there. Evidence:
[root-destination-proof](root-destination-proof/README.md). F05 and F07 stay partial.

## Triage at `8c79761`

**C# behaviour.** `UtilityHelper.ReplaceCustomVariables` swaps each placeholder for
the root's full path (`Utility/Utility.cs:20-38`), and `PathHelper.FixPathFormatting`
collapses repeated separators and trims a trailing one
(`FileSystemUtils/PathHelper.cs:823-851`). So `<<kotorDirectory>>`,
`<<kotorDirectory>>/` and `<<kotorDirectory>>\` all name the game directory.

- The archive/destination stage of the validation pipeline
  (`Services/Validation/InstallationValidationPipeline.cs:374-425`, run by the wizard
  preset `ValidationPipelineOptions.WizardFull`, `ValidationPipelineOptions.cs:53-58`)
  runs `ComponentValidation.ParseDestinationWithAction` (`ComponentValidation.cs:246-365`).
  For Copy, Move, CleanList and DelDuplicate it passes when the formatted path passes
  `PathValidator.IsValidPath` and `Directory.Exists` (`ComponentValidation.cs:330-361`).
  The game directory exists, so the stage passes.
- `DelDuplicate` (`ModComponent.cs:1591-1595`) runs `Instruction.DeleteDuplicateFile`
  in `RealDestinationPath` (`Instruction.cs:558-671`), which throws only when that
  directory does not exist (`Instruction.cs:575-583`). It deletes the duplicate in the
  game root.
- `PathValidator.IsValidPath` refuses a path part that ends in `.`
  (`FileSystemUtils/PathValidator.cs:30-96`, `HasInvalidWindowsFileNameParts`
  `:166-195`), so `<<kotorDirectory>>/.` and `<<kotorDirectory>>/Override/.` fail the
  archive/destination stage. At execution Copy and Move into such a path install,
  but DelDuplicate throws `Invalid directory path` (`UnknownError`; the dry run
  reports `Failed to validate component: Invalid directory path`) and CleanList ends
  with `UnknownError`, so both fail install and the dry run.
- A Source that is a bare root (`Extract [<<modDirectory>>]`, `Copy
  [<<kotorDirectory>>] -> ...`, `Delete [<<modDirectory>>]`) is one literal path that
  is not a file, so it matches nothing: Extract, Copy and Move fail with
  `MissingSourceFiles` (`DryRunSkipped`), Delete deletes nothing (strict Delete
  fails). Nothing beneath the root is reached.

**Rust at `8c79761`.** `Roots::resolve` built every placeholder path as
`root.join(relative(tail))` (`src/paths.rs:156`). For a bare root the relative part is
empty, and `Path::join("")` appends a trailing separator, so the resolved path's key
(`GAME/`) missed the root's directory key (`GAME`), which `Engine::scan` records from
the walk of each root (`src/engine.rs:423-472`). Two checks look that key up:

- the archive/destination stage (`validate_component_archives`,
  `src/engine.rs:896-901`): Copy, Move, CleanList and DelDuplicate failed with
  `Destination cannot be found: <<kotorDirectory>>`;
- the `DelDuplicate` arm (`src/engine.rs:2425-2430`): `DelDuplicate destination cannot
  be found` in the dry run, plain `validate` and `install`.

Path comparisons elsewhere use components (`Path::starts_with`, `==`), which ignore
the trailing separator, so Copy, Move and CleanList into a root installed correctly.

**Status.** A visible difference. The desktop Validate step and `validate --output
json` failed every guide with a Copy, Move, CleanList or DelDuplicate into a root (for
example a `dialog.tlk` copied to `<<kotorDirectory>>`), where C# passes. A
DelDuplicate in a root failed install outright, where C# deletes the duplicate.

## Behaviour after this slice

- `Roots::resolve` returns the root itself when the relative part is empty
  (`src/paths.rs:156-168`), as C# `FixPathFormatting` trims the trailing separator. A
  bare root Destination now finds the root's directory key in both checks above.
- **A bare root Source still names nothing** (follow-up). Before, the trailing
  separator made its key (`MODS/`) miss every file and its descendant prefix
  (`MODS//`) miss every path; with the root fix alone the prefix became `MODS/` and an
  `Extract [<<modDirectory>>]` extracted every archive under the mod root,
  recursively. `Roots::is_root` (`src/paths.rs:181-183`) now marks such a Source, and
  `Engine::source_prefix` (`src/engine.rs:1957-1961`) gives it no descendant prefix,
  so `source_matches` (`:1963-1971`), `matches_scoped`, the archive check in
  `validate_component_archives`, `recovery_matches` (`src/engine/source_resolution.rs:297-306`,
  which also no longer treats it as a Patcher folder) and the FOMOD choice matcher
  (`src/fomod/resolved_choices.rs:137-150`, `:183-186`) match no file for it.
- **`.` Destination parts.** The archive/destination stage refuses a Destination with
  a `.` part (`has_dot_part`, `src/engine.rs:210-214`, used at `:902-912`), as C#
  `IsValidPath` does; `relative` drops `.` parts while resolving. The DelDuplicate and
  CleanList arms (`src/engine.rs:2448-2455`, `:2523-2529`) now refuse it at execution
  too (follow-up), so the dry run reports an error and plain `validate` and `install`
  refuse, as C# does. Copy and Move into such a path still install, as in C#.
- **Other effects of the root fix**, each now as in C#:
  - `Extract` with Destination `<<modDirectory>>` of an archive at the mod root now
    extracts into `mods/<archive stem>/`, as C# does (`extraction_destination`,
    `src/engine.rs`, compares the Destination key with the archive's folder). On
    `8c79761` it extracted straight into the mod root. Probed
    (`extract_destination_mod_root`). A configured FOMOD extracted there now reaches
    its own folder instead of failing with `FOMOD extraction directory is empty`
    (not probed).
  - The Patcher check `Patcher destination must be the game directory`
    (`src/engine/source_resolution.rs:233-241`) compares keys, so a bare
    `<<gameDirectory>>` or `<<kotorDirectory>>` Destination now passes it, as C#
    accepts both. Not probed (it needs a patcher executable).
- Every other resolved path had a non-empty relative part and is unchanged; path
  comparisons by component (`Path::starts_with`, `==`) ignore a trailing separator.

## Edge cases compared with the live reference

Fixture: `game/{chitin.key, x.tga, x.tpc, r.txt}`, `game/Override/{o.txt, q.tga,
q.tpc}`, `mods/{m.txt, y.tga, y.tpc}` and `mods/l.csv` (`Mandatory Deletions,r.txt`).
`KD` is `<<kotorDirectory>>`, `GD` `<<gameDirectory>>`, `MD` `<<modDirectory>>`, `OV`
`KD/Override`. DelDuplicate steps use `Arguments = ".tga"` unless noted. "Before" is
`8c79761`; "stage" is the archive/destination stage.

| Case | Step | Result (both after this slice) | Before |
| --- | --- | --- | --- |
| `copy_override_control` | Copy `MD/m.txt` -> `OV` | passes, `Override/m.txt` | same |
| `copy_kotor` | Copy -> `KD` | passes, `m.txt` in the game root | stage failed |
| `copy_kotor_slash`, `copy_kotor_backslash`, `copy_kotor_double_slash` | Copy -> `KD/`, `KD\`, `KD//` | passes | stage failed |
| `copy_game`, `copy_game_slash` | Copy -> `GD`, `GD/` | passes | stage failed |
| `copy_to_mod_root` | Copy `OV/o.txt` -> `MD` | passes, `mods/o.txt` | stage failed |
| `move_kotor` | Move -> `KD` | passes, `m.txt` moved | stage failed |
| `cleanlist_kotor`, `cleanlist_kotor_slash` | CleanList `MD/l.csv` -> `KD`, `KD/` | passes, `r.txt` deleted | stage failed |
| `cleanlist_mod_root` | CleanList -> `MD` | passes, nothing to delete | stage failed |
| `deldup_override_control` | DelDuplicate -> `OV` | passes, `q.tga` deleted | same |
| `deldup_kotor`, `deldup_kotor_slash`, `deldup_game` | DelDuplicate -> `KD`, `KD/`, `GD` | passes, `x.tga` deleted | every check failed |
| `deldup_mod_root` | DelDuplicate -> `MD` | passes, `mods/y.tga` deleted | every check failed |
| `deldup_kotor_upper_extension` | `Arguments = ".TGA"` -> `KD` | passes, `x.tga` deleted | every check failed |
| `deldup_kotor_extension_sources` | Source `[.tga, .tpc]`, `Arguments = ".tpc"` -> `KD` | passes, `x.tpc` deleted | every check failed |
| `copy_then_deldup_kotor` | Copy -> `KD`, then DelDuplicate -> `KD` | passes, `m.txt` added, `x.tga` deleted | every check failed |
| `option_copy_kotor` | Choose an option whose Copy goes to `KD` | passes | stage failed |
| `copy_kotor_dot` | Copy -> `KD/.` | stage fails in both; install passes | same (by accident) |
| `copy_override_dot` | Copy -> `OV/.` | stage fails in both; install passes | stage passed |
| `move_kotor_dot` | Move -> `KD/.` | stage fails in both; install passes | same |
| `deldup_kotor_dot` | DelDuplicate -> `KD/.` | stage, dry run, validate and install fail in both | same (by accident); `6e67b3c` passed and deleted |
| `deldup_override_dot` | DelDuplicate -> `OV/.` | fails in both | dry run, validate and install passed and deleted `q.tga` |
| `cleanlist_kotor_dot`, `cleanlist_override_dot` | CleanList -> `KD/.`, `OV/.` | fails in both | the dry run, validate and install passed; `KD/.` deleted `r.txt` |
| `extract_source_mod_root_archives_only` | Extract `[MD]`, mod root holds only `a.zip`, `sub/b.zip` | `MissingSourceFiles` in both (`DryRunSkipped`), nothing extracted | same; `6e67b3c` extracted both archives |
| `extract_source_mod_root_mixed` | Extract `[MD]`, archives beside other files | as above | same; `6e67b3c`: a dry-run error |
| `extract_source_kotor` | Extract `[KD]` | as above | same; `6e67b3c` dry run error |
| `copy_source_mod_root`, `move_source_mod_root_archives_only` | Copy `[MD]` -> `OV`, Move `[MD]` -> `OV` | `MissingSourceFiles` in both | same |
| `copy_source_kotor` | Copy `[KD]` -> `MD` | `MissingSourceFiles` in both | stage also failed |
| `delete_source_kotor`, `delete_source_mod_root` | Delete `[KD]`, `[MD]` | passes, nothing deleted | same |
| `delete_source_kotor_strict`, `delete_source_mod_root_strict` | the same with `Overwrite = true` | `MissingSourceFiles` in both | same |
| `extract_destination_mod_root` | Extract `[MD/a.zip]` with Destination `MD` | install extracts to `mods/a/a.txt` in both; stage: known difference (below) | install extracted to `mods/a.txt` |
| `missing_folder_control` | Copy -> `KD/Missing` | stage fails in both; install creates the folder | same |
| `rename_destination_kotor` | Rename `KD/r.txt` -> `KD` | known difference (below) | same |

Every case is checked three ways: the validation pipeline (stage, dry-run decision
and `DryRunSkipped`), plain `validate` and `install` (decision and final tree).

## Known differences

- **Rename to a bare placeholder (unchanged).** `Rename KD/r.txt -> KD`: C# treats a
  Rename Destination as a file name and renames `r.txt` to a file literally named
  `<<kotorDirectory>>` in the game root. Rust resolves a placeholder Rename
  Destination to a path, finds the game directory and refuses (`Cannot overwrite
  directory with file`) in the dry run, plain `validate` and `install`. Fail-closed and
  kept; it belongs with the placeholder-path Rename finding in
  [delete-directory-contract.md](delete-directory-contract.md). Counted as three known
  differences.
- **Extract with a Destination in the archive/destination stage (predates this
  slice).** C# `ParseDestinationWithAction` fails every Extract that has a
  Destination (`'Destination' key cannot be used with this action`); Rust's stage
  does not check Extract Destinations and passes. Install matches. Counted as one
  known difference (`extract_destination_mod_root`, pipeline), where Rust accepts.
- **`..` parts (deliberate).** Rust refuses a `..` part anywhere in a path
  (`paths::relative`, sandboxing); C# resolves it and can install into the parent of
  a root. Not probed here; recorded in
  [delete-directory-contract.md](delete-directory-contract.md).
- **The path checked by the archive/destination stage.** C# runs `IsValidPath` on the
  whole formatted path, including the user's root folders; Rust checks the authored
  Destination. A root folder whose own name ends in `.` or a space would fail C#
  validation for every step; Rust refuses such roots when resolving any path. Not
  probed (it needs an unusual install folder name).
- **Case.** Not probed here: the roots are the canonical folders the user picked, and
  placeholder names are matched exactly in both.

## Results

| Binary | Root destination proof |
| --- | --- |
| follow-up, live reference (`--csharp ... --record`) | 119 match / 4 known difference / 0 fail (123 checks) |
| follow-up, recorded reference | 119 match / 4 known difference / 0 fail |
| `6e67b3c` (first fix) | 102 match / 4 known difference / 17 fail |
| `8c79761` (pre-fix) | 74 match / 4 known difference / 45 fail |
| first slice (`6e67b3c`, 78 checks) | 75 match / 3 known / 0 fail; `8c79761`: 41 / 3 / 34 |
