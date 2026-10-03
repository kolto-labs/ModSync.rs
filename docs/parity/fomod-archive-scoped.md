# FOMOD gate: configured without archive-scoped instructions

This contract covers C#'s soft gate warning "configured without archive-scoped
instructions" (`FomodConfigurationGate` R1). The [gate-scope contract](fomod-gate-scope.md)
had listed it as having no direct analogue. The reference is the pinned C# `d31ccbd3`, and
paths below are under `src/ModSync.Core/` there. Evidence:
[fomod-archive-scoped-proof](fomod-archive-scoped-proof/README.md). F15 stays partial.

## Triage at `223ed92`

**C# behaviour.** For each component in the gate's scope, the gate looks at each
registered archive. If the archive is on disk, is a FOMOD, and has the prompt status
`configured` (ordinal), the gate calls `FomodConfiguredComponentMerger.HasArchiveScopedInstructions`.
If that returns false, the gate adds a soft warning. The report prints it as
`WARNING: <component>: FOMOD archive '<f>' is marked configured but has no matching
archive-scoped instructions. Status is configured but no archive-scoped install
instructions were found. Re-run Configure FOMOD (or Fetch Downloads) so wizard output is
merged into the mod.` The warning counts in the stage summary and never blocks.

- `Services/Fomod/FomodConfigurationGate.cs:169-186` raises the warning, with the hint at `:25-27`.
- `Services/Validation/InstallationValidationPipeline.cs:457-466` prints it per component.
- `Services/Fomod/FomodConfiguredComponentMerger.cs:75-121` and `:152-190` define the check.
  It looks for any instruction, on the component or on any of its options (selected or
  not), whose source starts with `<<modDirectory>>/<archive stem>/`.

Configuring merges the mapper's whole output: `FomodPostDownloadOrchestrator.cs:85-98`,
then `FomodToComponentMapper.Map`. That output covers required files, every plugin's
option (chosen or not), unconditional patterns and patterns attached to one plugin
(`FomodToComponentMapper.cs:63-123`, `:244-287`). A freshly configured archive therefore
fails the check only when its installer maps no file for any choice.

**Rust at `223ed92`.** `validate_fomod_configuration` (`src/engine.rs:890`) checks that
registered archives have valid saved choices (`configured_choices`, `:1009`). Its only
related warning is a Rust-only one, raised when no selected Extract instruction installs
a configured archive (`:1069-1090`). An installer that maps no file passed silently:
validate reported "All detected FOMOD archives are configured." with no warning, and
install extracted the archive while copying nothing from it.

**Status.** This was an open gap, and this slice fixes it.

## Behaviour after this slice

- **Trigger.** It matches C#'s for a freshly configured component. One known
  difference: C# tests the component's actual instructions, and its configure step
  strips earlier authored instructions. Rust keeps authored instructions, so an
  authored Copy or Move that reads `<<modDirectory>>/<stem>/...` can still install
  files while Rust warns. The warning never blocks. The archive is registered by the component, is on
  disk and is a FOMOD. Its prompt status is exactly `configured`. Its installer maps no
  archive-scoped file under any choice.
- **The mapping rule.** `ModuleConfig::maps_any_file` (`src/fomod.rs`) counts a file
  that the reference mapper would place. That means a required file, a file of any
  plugin in any step (visible or not), or a file of a conditional pattern that is
  unconditional or attached to one plugin. Patterns the mapper skips do not count:
  Or/nested composites, and flags that no single plugin sets. A non-folder file whose
  source normalizes to empty does not count either, because C# maps it to the bare
  archive folder, which the prefix check misses.
- **What does not warn.** As in C#, there is no warning when only an unchosen plugin
  installs files. Rust's own chosen files can still be empty in that case.
- **Choices and status.** Rust still requires valid saved choices first. With
  `configured` status and no `FomodChoices`, Rust keeps its hard error, because it does
  not import C#-generated configuration. Valid choices with any other status, or none,
  pass Rust's gate without this warning. C# treats those as unconfigured.
- **Wording.** The first sentence is C#'s. The second explains the Rust cause: `Its
  installer maps no files for any choice, so this configuration installs nothing from
  the archive.` C#'s hint ("so wizard output is merged into the mod") describes C#'s
  merge, which Rust does not have.
- **Placement.** One warning per registering component, prefixed with the component
  name. It appears as a counted FomodConfiguration warning in `--validation-report`, as
  a `WARNING:` event on plain `validate`, and on `install`. It never blocks.
- **The Rust-only route warning.** "no selected Extract instruction installs it" is
  unchanged. It can appear alongside this warning.

## Reference observations

The live oracle in [csharp-oracle](fomod-archive-scoped-proof/csharp-oracle) uses the
pinned `ModSync.Core.dll`. It maps each fixture's `ModuleConfig.xml`, merges the result,
marks the component configured, and runs the gate. It warns for the empty,
no-steps and unattached-pattern installers. It does not warn for the mapped,
unchosen-plugin-only and attached-pattern installers. Rust now matches this on the same
fixtures.

The oracle builds its component in memory. When loading the probe's guides through the
reference TOML and JSON deserializers, registry `Files` and `HandlerMetadata` came back
empty, so the C# gate saw no registered archive. This slice does not chase that
reference loader behaviour.

## Out of scope

- The C# gate's issue texts (unreadable or missing archive with state) keep their Rust
  wording.
- Soft gate warnings are still dropped when a later readiness error fails the gate,
  such as an unselected dependency. That behaviour is shared with the missing-archive
  warning and predates this slice.
- C#'s mapper keeps the extension in the folder name for archives other than
  `.zip`/`.7z`/`.rar`/`.exe`, but the merger strips any extension. So C# warns for
  every such archive. Rust does not reproduce that mismatch.
