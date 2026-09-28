# Shared desktop acquisition plan review

Independent Codex reviewer: `/root/gui_plan_review`, 2026-09-28 UTC.
Final verdict: **APPROVED**, after the two corrections below. This is approval
of a bounded future implementation contract, dependent on accepted fetch CLI and
loopback transport slices. It is not source, native-runtime or full-parity acceptance.
The reviewer performed read-only source and document inspection; no builds, tests,
network calls, GUI launches, file edits or Git mutations.

## Required corrections, now verified

1. **FOMOD editor closure.** `Editor::dismissal` in `src/fomod_ui.rs:26` emits an
   outcome only after positive detection. Cancel at `:193` may close the editor
   while returning None, indistinguishable from an editor still open to its caller.
   Step 2 now permits a narrow additive read-only lifecycle helper, preserving
   Outcome, callers and frozen tests. G8 releases editor binding and queue/readiness
   on closure without claiming configuration. Reviewer re-read both changes: holds,
   high confidence.
2. **No disguised Rust harness.** Ambiguous permission for a production probe path
   could have allowed synthetic message injection despite the user freeze. Native
   acceptance now uses actual UI/protocol operations with only approved API-base,
   CA and settings overrides. Synthetic-message/hidden Rust harnesses are explicitly
   prohibited; unreachable stale-message runtime cases remain pending. Reviewer
   re-read the corrected acceptance bullet: holds, high confidence.

## Other independently checked boundaries

- Captured run/guide/root/completed-base context and terminal-authoritative record
  fit actual AcquisitionPlan/RunSettings/RunReport/record/followup APIs. No invented
  shared Snapshot or duplicated source selection is required.
- App construction currently reads default credentials/telemetry before alternate
  settings and protocol launch ordering are settled (`ui.rs:1703-1718,3228-3232`).
  Probe context must be initialized before these effects. Unix forwarding carries
  only protocol text (`single_instance.rs:27-45`), so probes must not claim/forward
  the ordinary socket.
- Peer settings case variants must be reconciled when a key is edited, unknown
  keys/secrets preserved, malformed settings errors visible. Sharing=false wins.
  No persistent-seeder claim follows from keepSharing settings.
- GUI Fetch/NXM integration, native transfer/gate/cancellation/FOMOD proof and
  cross-platform behavior remain work. F12-F15/F24/F26 remain partial; F29 open.
