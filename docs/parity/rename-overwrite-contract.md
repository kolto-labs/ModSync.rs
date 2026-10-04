# Rename with Overwrite=false onto an existing file

This contract covers F05-RENAME's overwrite case from the
[actions audit](engine-actions-archives-audit.md): a `Rename` instruction with
`Overwrite = false` whose target file already exists. The reference is pinned C#
`d31ccbd3`, and C# paths below are under `src/ModSync.Core/` there. Evidence:
[rename-overwrite-proof](rename-overwrite-proof/README.md). F05 stays partial.

## Triage at `5643276`

**C# behaviour.** `Instruction.RenameFile` (`Instruction.cs:936-1027`) loops over
the resolved sources. When the target `<source dir>/<Destination>` exists and
`Overwrite` is false, it logs "already exists ... skipping file", sets
`exitCode = RenameTargetAlreadyExists` (`:984-996`), and `continue`s. The other
sources are still renamed, and the method returns `RenameTargetAlreadyExists`.
`ModComponent.ExecuteSingleInstructionAsync` returns that code for `Rename`
(`ModComponent.cs:1625-1629`). `ExecuteInstructionsAsync` treats any non-success
code that is not `OptionalInstallFailed`, `FileNotFoundPre` or `FileNotFoundPost`
as `InstallExitCode.UnknownError`, logs "FAILED Instruction #n" and stops the
component (`ModComponent.cs:1852-1869`).

- Install (`ModComponent.InstallAsync`, `:841-873`) fails the component with
  `UnknownError`.
- Dry-run validation (`Services/Validation/DryRunValidator.cs:507-547`) runs the
  same pipeline on the virtual file system and adds the error
  `Dry-run failed for '<name>' with exit code UnknownError.`

`Copy` and `Move` differ. They log a warning, skip the file and still return
`Success` (`Instruction.cs:1034-1155`, `:1157-1287`). The default `Overwrite` is
`true` (`Instruction.cs:66`).

**Rust at `5643276`.** The shared `Copy | Move | Rename` branch of the engine
(`src/engine.rs:2154-2214`) treated all three alike. At `:2197-2206` it recorded a
validation warning and skipped the source. Validate and install therefore both
succeeded. The un-renamed file was left in place, and later steps ran against the
wrong tree. A later `Delete` of the target, for example, removed the
pre-existing file the guide meant to replace.

**Status.** This was an open gap: a wrong accept decision and a wrong file outcome.
This slice fixes it.

## Behaviour after this slice

- **Rename collision.** The engine skips each source whose target exists, renames
  the remaining sources, and then fails the instruction:
  `Rename target already exists and overwrite is false: <first blocked target>`.
  The usual `<component> / step <n> (Rename)` context is prefixed. Validate exits
  non-zero. Install is blocked by its validation before any write.
- **Unchanged.** `Overwrite = true` (and the default) replaces the target.
  A free target is renamed. `Copy` and `Move` with `Overwrite = false` still
  warn and skip, as in C#.
- **Shared engine.** `validate --output json --dry-run` now fails its `DryRun`
  stage with `ERROR: [EngineSimulation] <the error above>`, where C# reports its
  `DryRun` error. `conflicts` exits 1 with the same error, because it aborts on
  simulation errors, as before. The desktop runs the same engine.

## Known differences

- **Timing.** C# install runs the earlier instructions, and any other Rename
  sources, before the component fails. Rust validates the whole selection
  before installing, so it refuses with no game writes. Both reject the guide.
  The final trees after the failure differ. The probe compares only the decision
  in failure cases.
- **Message.** C# logs `File '<source name>' already exists in <dir>, skipping
  file. Reason: Overwrite set to False )` and reports `UnknownError`. Rust names
  the blocked target path in its error.
