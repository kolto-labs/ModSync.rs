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
- F01: guide `LastModified` metadata uses `DateTime.TryParse` in the reference;
  Rust keeps the authored text. Decide whether save output must normalize it.
- F01: YAML typed mapping keys. Closed 2026-10-03. A live C# check showed the
  reference loads valid Boolean, Int32 and Double keys and rejects null, invalid
  and complex keys. Rust now admits the same set (`src/model/yaml.rs`).
  Explicit `!!timestamp` values stay refused, because the reference renders them
  with the host culture and time zone. See
  [the YAML intake contract](yaml-scalar-intake.md).
- F11: checkpoint duplicate-definition check folds GUID text with Unicode
  lowercase. Equivalent for valid GUIDs; admission is intentionally unchanged.

The detailed tracker and acceptance ledger retain all remaining domains. This
source triage replaces neither their requirements nor their acceptance owners.
