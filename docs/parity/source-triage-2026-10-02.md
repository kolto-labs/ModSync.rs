# Current source triage: implementation and deferred acceptance

This record follows `79ebd5a` on the sole Rust `main` checkout. The read-only
reference remains C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`. The audit used
current production source. No handoff, test, runtime, native probe, install or CI
was used to establish this source classification. It is not exhaustive behavioral
acceptance and does not close F01-F29.

## F05-F15 findings

| Domain | Current source | Concrete remaining implementation |
| --- | --- | --- |
| F05 actions | All non-Unset action dispatches exist; shared virtual/install engine | Registry archive auto-extraction and descendant/variant/redundant Move source recovery |
| F06 archives | ZIP/7z/RAR/tar/gzip/embedded 7z SFX, streaming, integrity, containment, timestamps | No new missing decoder demonstrated; encrypted/multipart/executed SFX retain declared refusal |
| F07 cleanup | CleanList and texture cleanup exist | Ordinal/count-versus-candidate DelDuplicate correction; bounded BOM-aware CleanList decoding and matching |
| F08 validation | Full/basic reports, dry-run, stages, FOMOD gates exist | F05 source recovery must also reach virtual validation; external chmod/backend probes remain forbidden here |
| F09 patchers | Native/Python/package discovery, argv, fresh log checks and InstallList simulation exist | No new missing invocation function demonstrated; real patched-resource/platform comparison remains proof |
| F10 coordinator | Cancellation, component progress, failure receipts/recovery, optional checkpoints exist | Execution GUID value comparison still needs reconciliation; unsafe continuation stays refused |
| F11 checkpoints | Component CAS/provenance/restore/resume/browser source exists | Power-loss, platform and restored-game acceptance remain proof |
| F12 resources | Planning, filename filtering, discovery, metadata/cache recording exist | One bounded cancellable inventory per planning/discovery context; qualification errors must propagate |
| F13 hosts | Nexus, anonymous DeadlyStream, MEGA and provider paths exist | No new absent provider function demonstrated; GameFront manual handling matches the pinned reference |
| F14 queues | Worker/retry/progress/stop/race/fence source exists | No new missing download function demonstrated; protocol UI FIFO is adjacent F23 work |
| F15 FOMOD | Choices, visibility, priority, flat conditional fallback, rename warnings, persistence exist | Bounded cancellable module/info metadata reads and XML node/string work |

Archive memory/corpus/platform receipts, actual backend resources, crash/restart,
live providers and games remain deferred until source implementation is complete.
Source presence is not proof of their outcomes.

## Next bounded source queue

- F23: preserve pending Modsync links in FIFO order and stop draining bounded
  transports when the combined desktop pending queue is full.
- F07: independently reviewed cleanup correctness/decoder contract.
- F12: independently reviewed cancellable inventory/qualification contract.
- F15: independently reviewed metadata/XML resource contract.
- F05/F08: derive and implement the shared read-only virtual source resolver.
- F04/F10: typed GUID graph identity without rewriting stored receipts or IDs.
- F01: preserve pinned YAML plain-scalar/tag/null/alias semantics; separately
  retain JSON integer origin beyond the current unsigned storage range.
- F01: broader floating/string getter and complex native-type contracts.

The detailed tracker and acceptance ledger retain all remaining domains. This
source triage replaces neither their requirements nor their acceptance owners.
