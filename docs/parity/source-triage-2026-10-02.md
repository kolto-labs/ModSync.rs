# Current source triage: implementation and deferred acceptance

This record follows `79ebd5a` on the sole Rust `main` checkout. The read-only
reference remains C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`. The audit used
current production source. No handoff, test, runtime, native probe, install or CI
was used to establish this source classification. It is not exhaustive behavioral
acceptance and does not close F01-F29.

## F05-F15 findings

| Domain | Current source | Concrete remaining implementation |
| --- | --- | --- |
| F05 actions | All non-Unset action dispatches exist; shared virtual/install engine; miss-only source recovery (registry auto-extract, descendants, Move variant, redundant game Move) implemented | Ordinal (.NET OrdinalIgnoreCase) path identity coordinated across engine VFS, managed and checkpoint keys |
| F06 archives | ZIP/7z/RAR/tar/gzip/embedded 7z SFX, streaming, integrity, containment, timestamps | No new missing decoder demonstrated; encrypted/multipart/executed SFX retain declared refusal |
| F07 cleanup | CleanList and texture cleanup exist | Bounded ordinal/count-versus-candidate and BOM-aware corrections now implemented; runtime comparison deferred |
| F08 validation | Full/basic reports, dry-run, stages, FOMOD gates exist; source recovery shared by virtual validation | External chmod/backend probes remain forbidden here |
| F09 patchers | Native/Python/package discovery, argv, fresh log checks and InstallList simulation exist | No new missing invocation function demonstrated; real patched-resource/platform comparison remains proof |
| F10 coordinator | Cancellation, component progress, failure receipts/recovery, optional checkpoints exist; execution GUID value comparison implemented | Unsafe continuation stays refused, including named refusal of reference skip/continue/auto-confirm flags |
| F11 checkpoints | Component CAS/provenance/restore/resume/browser source exists | Power-loss, platform and restored-game acceptance remain proof |
| F12 resources | Planning, filename filtering, discovery, metadata/cache recording exist | Bounded invocation inventory and propagated qualification errors now implemented; live acceptance deferred |
| F13 hosts | Nexus, anonymous DeadlyStream, MEGA and provider paths exist | No new absent provider function demonstrated; GameFront manual handling matches the pinned reference |
| F14 queues | Worker/retry/progress/stop/race/fence source exists | No new missing download function demonstrated; protocol UI FIFO is adjacent F23 work |
| F15 FOMOD | Choices, visibility, priority, flat conditional fallback, rename warnings, persistence exist | Bounded cancellable metadata/XML reads now implemented; metadata/corpus cancellation acceptance deferred |

Archive memory/corpus/platform receipts, actual backend resources, crash/restart,
live providers and games remain deferred until source implementation is complete.
Source presence is not proof of their outcomes.

## Next bounded source queue

Implemented on the current source after `f528e4b`: F05/F08 miss-only source
recovery, F04/F10 typed execution GUID comparison, F01 JSON integer origin with
native TOML/YAML number rendering and wide-integer getter coercions, F22 scalar
Name/Version conversion and F26 named refusal of unsafe reference install flags.
See the source-resolution, GUID, JSON number-origin and update-check contracts.

Remaining source work found by the post-wave audit:

- F05/F18/F19: the engine VFS, managed process capture, deployment and conflict
  keys now share the pinned .NET `OrdinalIgnoreCase` key (in-memory only; stored
  spellings are unchanged). F11 checkpoint tree alias checks use the same key
  (decided 2026-10-03: follow the reference).
- F01: guide `LastModified` metadata. Closed 2026-10-03 (follow the
  reference). A live C# differential showed that TOML and YAML saves normalize
  the value through invariant `DateTime.TryParse` with `DateTimeStyles.None`,
  and omit it when the parse fails. Rust now does the same on save through the
  shared date model. Forms outside that model keep the authored text with a
  warning. See [the lastModified contract](lastmodified-contract.md).
- F01: YAML typed mapping keys. Closed 2026-10-03. A live C# check showed the
  reference loads valid Boolean, Int32 and Double keys and rejects null, invalid
  and complex keys. Rust now admits the same set (`src/model/yaml.rs`).
  Explicit `!!timestamp` values stay refused, because the reference renders them
  with the host culture and time zone. See
  [the YAML intake contract](yaml-scalar-intake.md).
- F15: C#'s FOMOD gate warning "configured without archive-scoped
  instructions". Closed 2026-10-03. Rust at `223ed92` passed silently when a
  `configured` archive's installer mapped no files for any choice. It now warns per
  component, as the reference does (live oracle on the same fixtures). See
  [the archive-scoped contract](fomod-archive-scoped.md).
- F05: Rename with `Overwrite=false` onto an existing target. Closed 2026-10-04.
  C# `RenameFile` returns `RenameTargetAlreadyExists`, which fails the component
  in install and in dry-run validation. Rust at `5643276` warned, skipped and
  passed both. Rust now fails the instruction after renaming the other sources
  (live oracle on the same fixtures). Copy/Move keep the reference warn-and-skip.
  See [the rename overwrite contract](rename-overwrite-contract.md).
- F05: Delete with `Overwrite=true` onto a missing file. Closed 2026-10-04.
  C# `DeleteFile` returns `FileNotFoundPost` for a missing literal or an empty
  expansion; install fails with `MissingSourceFiles` and dry run warns
  `DryRunSkipped`. Rust at `8fad43c` ignored `Overwrite` on Delete and passed.
  Rust now deletes the present matches, then raises the typed missing-source
  cause (live oracle on the same fixtures). See
  [the delete strict contract](delete-strict-contract.md).
- F05: Delete/Copy/Move/Extract with an empty Source list. Closed 2026-10-04.
  C# `DeleteFile` returns `Success` for no paths, or `FileNotFoundPost` with
  `Overwrite=true`; Copy/Move/Extract resolution fails with `FileNotFoundPre`.
  Rust at `fba4eb1` rejected all of them with an untyped `Instruction has no
  Source`. A non-strict empty Delete is now a no-op; the others raise the typed
  missing-source cause (live oracle on the same fixtures). See [the empty source contract](empty-source-contract.md).
- F05: Extract with `Overwrite=false` onto existing files. Closed 2026-10-04.
  C# `ExtractFileAsync` never reads `Overwrite`, and the real provider writes
  every entry with `FileMode.Create`, so existing files are replaced and the last
  archive wins. Rust at `5905259` kept the existing file. Rust now always replaces;
  configured FOMOD extraction keeps its fail-closed refusal (live oracle on the
  same fixtures). See [the extract overwrite contract](extract-overwrite-contract.md).
- F05: Copy/Move onto itself with `Overwrite=false`. Closed 2026-10-04.
  C# `CopyFileAsync`/`MoveFileAsync` find the target (the source) present, warn,
  skip it and return `Success`. Rust at `354055b` refused the step with `Source
  equals destination`. Rust now warns and skips; with `Overwrite=true` it still
  refuses before any write, where C# deletes the file and fails (live oracle on
  the same fixtures). See [the copy/move self contract](copy-move-self-contract.md).
- F11: checkpoint duplicate-definition check folds GUID text with Unicode
  lowercase. Equivalent for valid GUIDs; admission is intentionally unchanged.

The detailed tracker and acceptance ledger retain all remaining domains. This
source triage replaces neither their requirements nor their acceptance owners.
