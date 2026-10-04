# Delete with Overwrite=true onto a missing file

This contract covers F05-DELETE's missing-source case from the
[actions audit](engine-actions-archives-audit.md): a `Delete` instruction with
`Overwrite = true` whose source does not exist when the step runs. The reference
is pinned C# `d31ccbd3`, and C# paths below are under `src/ModSync.Core/` there.
Evidence: [delete-strict-proof](delete-strict-proof/README.md). F05 stays partial.

## Triage at `8fad43c`

**C# behaviour.** For `Delete` the loader sets `Overwrite` from the guide, and
defaults it to `false` only for `Delete` when the key is absent
(`Services/ModComponentSerializationService.cs:2172-2180`). Execution calls
`SetRealPaths(skipExistenceCheck: true)` (`ModComponent.cs:1586-1590`). That
expands wildcards and appends every literal source, present or not
(`Instruction.cs:313-345`). `Instruction.DeleteFile` (`Instruction.cs:828-933`)
then deletes each existing path. For a missing path it logs and continues:

- `Overwrite = false`: verbose "File does not exist (skipping)", still `Success`.
- `Overwrite = true`: "Invalid wildcards or file does not exist", and the method
  returns `FileNotFoundPost` (`:866-874`). When nothing resolves at all (a
  wildcard that matches nothing), it also returns `FileNotFoundPost` (`:894-901`).
  A wildcard miss beside a resolved path is not an error.

`ExecuteInstructionsAsync` maps `FileNotFoundPost` to
`InstallExitCode.MissingSourceFiles` and stops the component
(`ModComponent.cs:1852-1866`).

- Install (`ModComponent.InstallAsync`) fails with `MissingSourceFiles`, after
  the earlier steps and the present paths of this step have been deleted.
- Dry-run validation (`Services/Validation/DryRunValidator.cs:515-527`) turns
  `MissingSourceFiles` into the component-level warning `Skipped '<name>' during
  dry-run: source archives or files are not available.` (`DryRunSkipped`) and
  reports no error.

**Rust at `8fad43c`.** The `Delete` branch (`src/engine.rs:2229-2245` at that
commit) ignored `Overwrite`. A missing source only produced the validation
warning `Attempting to delete non-existent file` when nothing matched. Validate,
the dry-run report and install all succeeded, and install continued with later
steps.

**Status.** An open gap: a wrong accept decision on install (the reference
fails the component) and a wrong final tree, because later steps ran.

## Behaviour after this slice

- **Strict miss.** With `Overwrite = true` (explicit, since the Delete default is
  `false`), a missing literal source, or a step whose sources resolve to nothing,
  fails the step with the typed missing-source cause
  `Source matches no files: <first missing source>`, after the present matches
  are deleted. It is the same cause a missing Copy/Move/Extract source raises.
- **Dry-run report.** `validate --output json --dry-run` classifies it as C#
  does: the component's provisional issues are replaced by the single
  `DryRunSkipped` warning, and the report passes.
- **Strict gates.** Plain `validate` exits non-zero with the error above, the
  existing missing-source policy for the install gate. `install` is refused by
  its validation before any write. `conflicts` exits 1 because it aborts on
  simulation errors. The desktop runs the same engine.
- **Unchanged.** `Overwrite = false` and an omitted `Overwrite` still skip a
  missing source (with Rust's existing validation warning when nothing matched).
  A wildcard miss next to a resolved path is not an error. Present sources are
  deleted as before.

## Known differences

- **Timing.** C# install deletes the present paths, and runs every earlier
  step, before the component fails. Rust validates the whole selection before
  installing, so it refuses with no game writes. Both reject the guide. The
  probe compares only the decision in failure cases.
- **Message.** C# logs `Invalid wildcards or file does not exist: '<path>'` and
  reports `MissingSourceFiles`. Rust names the first missing source as authored.
- **Literal directory.** A literal source that names a directory is missing for
  the strict check in both (C# `FileExists` is false). Rust's lenient delete
  still removes the files under such a directory, as before; C# does not.
- **Case.** The check uses Rust's case-insensitive path keys. C# on Linux
  resolves case through `CaseInsensitivePathing`. The probe uses exact case only.

## Notes for guide authors (review of ba3da94)

- A cleanup `Delete` with `Overwrite = true` on an optional file that may be absent
  now fails plain `validate` and `install`, as it already did in C#. Omit `Overwrite`
  or set it to `false` for optional cleanup.
- The guide loaders default a Delete's missing `Overwrite` to `false`. The
  `guide-edit` transaction path deserializes instructions directly and uses the model
  default of `true`, which is also C#'s `new Instruction()` default. A Delete inserted
  through `guide-edit` without `Overwrite` is therefore strict.
- When nothing resolves, the error names the first source, which may be a wildcard
  rather than the missing literal. Only the message differs; the decision matches C#.
- With managed deployment, the per-component preflight refuses before that
  component's writes, but earlier components may already be committed, as with
  missing Copy/Move sources.
