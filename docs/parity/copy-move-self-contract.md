# Copy/Move onto itself with Overwrite=false

This contract covers one F05-X3 edge from the
[actions audit](engine-actions-archives-audit.md): a `Copy` or `Move` whose
target `<Destination>/<source filename>` is the source file itself, for example
`Copy <<kotorDirectory>>/Override/*.tga -> <<kotorDirectory>>/Override`. The
reference is pinned C# `d31ccbd3`; C# paths below are under `src/ModSync.Core/`
there. Evidence: [copy-move-self-proof](copy-move-self-proof/README.md). F05
stays partial.

## Triage at `354055b`

**C# behaviour.** `ModComponent.ExecuteSingleInstructionAsync` resolves the
sources and calls `Instruction.CopyFileAsync` or `MoveFileAsync`
(`ModComponent.cs:1597-1623`). Neither compares source and target. For each
source they build `Path.Combine(destination, fileName)` and test
`FileExists` on it (`Instruction.cs:1095` for Copy, `:1229` for Move). The
source itself is there, so:

- **`Overwrite = false`.** They log `File '<name>' already exists ... skipping
  file. Reason: Overwrite set to False` and return for that source
  (`Instruction.cs:1097-1103`, `:1232-1238`). The other sources are processed and
  the action returns `Success`. Install and dry run both pass; the file is
  untouched.
- **`Overwrite = true`, the default (`Instruction.cs:66`).** They delete the
  "pre-existing" target, which is the source (`:1109`, `:1244`), then call the
  provider. The real provider's `File.Copy`/`File.Move`
  (`Services/FileSystem/RealFileSystemProvider.cs:31-58`) throws because the
  source is gone; the action returns `UnknownError` (`Instruction.cs:1152`,
  `:1284`) and the component fails. The file is lost. The dry run fails too:
  `Source file does not exist: <path>`.

`Rename` onto its own name returns `RenameTargetAlreadyExists` with
`Overwrite = false` and fails (`Instruction.cs:986-996`), as in the
[rename overwrite contract](rename-overwrite-contract.md).

**Rust at `354055b`.** The shared `Copy | Move | Rename` loop
(`src/engine.rs:2214-2218`) failed every self-target with `Source equals
destination: <path>` before reaching the existing warn-and-skip collision path
(`:2226`). Validate and install therefore refused guides that C# installs, with
`Overwrite = false`. A wildcard Copy back into its own folder, or a Source list
that includes a file already in the destination, was enough.

**Status.** Open gap: a wrong refuse decision for `Overwrite = false`. This slice
fixes it. The `Overwrite = true` refusal already matched C#'s decision.

## Behaviour after this slice

- **Copy/Move, `Overwrite = false`.** A self-target is treated like any existing
  target: the engine records the usual validation warning
  (`Destination file already exists and overwrite is false: <path>`), skips that
  source, handles the other sources and goes on (`src/engine.rs:2214-2240`).
  Validate and install pass; the file is untouched.
- **Copy/Move, `Overwrite = true` or omitted.** Unchanged: `Source equals
  destination: <path>`. Validate exits non-zero and install is refused by its
  validation before any write. C# also fails, but only after deleting the file;
  Rust does not copy that data loss.
- **Rename.** Unchanged: a self-target fails with `Source equals destination`
  whatever `Overwrite` says. C# fails too (`RenameTargetAlreadyExists` or, with
  `Overwrite = true`, an I/O error after deleting the file).
- **Shared engine.** `validate --output json --dry-run`, `conflicts` and the
  desktop run the same engine. A skipped self-target does not count as a write
  in `conflicts`, as for any skipped `Overwrite = false` collision.

## Follow-up at `6b8baff`

Two differences were left open by the first slice and are closed here.

**Recovered-source self-target.** When a Copy/Move source is missing and the
miss-only recovery binds it, `qualify_action`
(`src/engine/source_resolution.rs:802-871`) re-checks every Source of the
instruction, including direct sources that were never recovered. It still refused
a self-target with `Source equals destination`. So `Copy
["<<modDirectory>>/Mod/z.txt", "<<kotorDirectory>>/Override/x.txt"] ->
<<kotorDirectory>>/Override`, `Overwrite = false`, with `ResourceRegistry."Mod.zip".Files."z.txt" = true`
and `Mod/z.txt` still in `Mod.zip`, failed validate and install, but passed once
`Mod/z.txt` was extracted. The live oracle returns `Success` and keeps `x.txt`.
Qualification now applies the Copy/Move arm's rule: with `Overwrite = false` a
self-target is left out of the target checks and the arm then skips it with the
usual warning. With `Overwrite = true` or omitted it still refuses before any write
(the oracle deletes `x.txt` and returns `UnknownError`). Rename does not recover.

**Install-time skip notice.** C# `CopyFileAsync`/`MoveFileAsync` log `[Warning]
File '<name>' already exists in <dir>, skipping file. Reason: Overwrite set to
False` for every file skipped this way, during install as well (the oracle logs one
line per skipped file). Rust recorded only a validation warning, which is silent
while installing. The Copy/Move arm (`src/engine.rs:2230-2250`) now also emits one
ordinary install event per skipped file, with the validation text:
`WARNING: Destination file already exists and overwrite is false: <path>`. It
applies to every `Overwrite = false` collision on Copy/Move, not only to
self-targets, and to FOMOD-configured copies that run through the same arm. The
event carries the component name and the step number. Where it shows:

- plain `install`: one stdout line `<component> / <step>: WARNING: ...`, before
  the step's own line;
- `install --json`: one more `Event` JSON line on stdout, same shape as the others;
- `install --install-progress-json`: on stderr, like every ordinary event, so the
  typed stdout stream is unchanged;
- desktop: one line in the install log.

It is not emitted during validate, dry run or `conflicts`, which keep their
existing warning in the report. It does not change exit codes, the install result
or the files written.

## Edge cases (live oracle, all `Overwrite = false` unless noted)

| Case | Result |
| --- | --- |
| Copy, Move | matched (accept, file kept) |
| Copy, Move with `Overwrite = true` or omitted | matched (refuse) |
| Wildcard matching the destination's own files | matched |
| Mixed Source list: one self-target, one real copy/move | matched (the other file is copied/moved) |
| `<<modDirectory>>` file into `<<modDirectory>>` | matched |
| Subdirectory file into the same subdirectory | matched |
| Read-only self-target | matched (accept, file kept) |
| Source spelled `X.TXT`, file `x.txt` | matched |
| Destination spelled `OVERRIDE`, folder `Override` | matched |
| Source is a symlink in `Override` | known difference (below) |
| Rename onto its own name (control) | matched (refuse) |
| Ordinary collision (`mods/x.txt` onto an existing `Override/x.txt`) | matched (accept, file kept) |
| Mixed list, one recovered from `Mod.zip`, one self-target: Copy, Move | decision matched; final tree is a known difference (below) |
| The same, Copy with `Overwrite = true`, Move with it omitted | matched (refuse) |
| Install warnings: one per skipped file | matched in every accepted case |

## Known differences

- **Overwrite = true.** Both reject the step. C# deletes the only copy of the
  file first (the oracle's failed install leaves `Override` empty); Rust refuses
  before any write and keeps it. This is a deliberate fail-closed choice.
- **Symlinks.** With a symlink `Override/l.txt -> x.txt` as Source, C# finds the
  target present, skips it and succeeds. Rust refuses any symlink in an install
  tree (`Symlinks are unsupported in install trees`, `src/engine.rs:399`), before
  the instruction runs. This is an existing deliberate rule, not changed here.
- **Dry-run messages.** C#'s dry run reports no warning for a skipped
  self-target (the skip is only logged); Rust's validation keeps the warning it
  already gives for any `Overwrite = false` collision. Decisions match.
- **Recovered sources, final tree.** In a mixed Source list where one source is
  still inside a `ResourceRegistry` archive, Rust's miss-only recovery extracts the
  archive and copies or moves that file; C#'s direct binding drops the missing
  literal because another source exists, and handles only the self-target. Both
  accept and keep the self-target file. This is the recovery design
  ([source-resolution-contract.md](source-resolution-contract.md)), not the
  self-target rule; the probe reports it as a known tree difference.
