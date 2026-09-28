# Remaining FOMOD plan review

**PLAN APPROVED** after four corrections. Independent reviewer inspected Rust and
pinned C# resolver/orchestrator/parser/mapper source before the proposed plan.
No edits, builds, tests, Rust harnesses, network or Git mutations by reviewer.

1. Existing path dedup precedes consumer eligibility, allowing configured A to
   suppress pending B. S1 now owns a narrow shared collector/consumer-binding
   correction. Evaluate each authorized consumer first, deduplicate inspection
   separately, apply valid sessions atomically to all pending consumers, preserve
   configured/unselected/completed/unrelated scope. Both orders require proof.
2. Prompt suppression no longer determines success. Explicit precedence: run/signal
   cancellation130, present-invalid choices or other failures1, missing usable
   choices/manual3, fullyresolved0. Repeated Fetch/restart acceptance is required.
3. Current prompt_choices blocks in read_line and lacks cancellation. Planned
   additive adapter preserves standalone API and uses typed refusal/EOF/quit/signal/
   error outcomes. Actual PTY SIGINT at confirmation and selection must end without
   another line or a detached blocked reader. EOF/q leaves unresolved work.
4. Workflow corrected to planapproval → implementation → sourceSPEC → QUALITY,
   with fixes returning to the relevant gate.

Reference mode precedence, existing sidecars/session/editor, targeted record
persistence and narrow parser/mapper scope were independently confirmed. No new
parser, editor, transport or sidecar format is needed. No F15 implementation or
runtime completion is implied; see [approved plan](fomod-remaining-plan.md).
