# Shared desktop acquisition contract

Status: [independently approved](downloads-gui-plan-review.md), 2026-09-28; not implemented.
F12–F15/F24/F26 remain partial. This is a bounded desktop slice, not full parity.
References below are worktree-relative and were checked before the fetch worker's
final delivery; rebind changing orchestrator references to its accepted commit.

## Prerequisites and source facts

- Accept the [fetch CLI contract](fetch-downloads-cli-plan.md), its concrete adapter,
  and independent spec/quality reviews before starting desktop implementation.
  `PeerAcquire::new` exists at `src/peer_acquire.rs:55`, but is currently a draft.
- Accept [loopback transport](origin-loopback-probe-plan.md) and its real Origin /
  Peer fixtures before desktop transfer/race/cancel/gate proof; additive entry-point names remain undecided.
- `src/ui.rs:18` has unbound Downloaded/NxmDownloaded/Done messages; `:479` reports
  only active/idle; `:494` offers a direct utility, not selected-guide Fetch.
  NXM currently bypasses orchestration through NexusClient + fetch (`:1829`).
- Shared facts already exist: AcquisitionEvent/Phase (`src/acquisition.rs:152,183`),
  AcquisitionPlan/RunSettings/RunReport (`src/download_orchestrator.rs:77,408,468`),
  plan/run (`:258,486`), nxm_target (`:1548`), record (`:1614`), and
  fomod_followups (`:1717`). There is no shared Snapshot type. GUI state is a
  presentation projection of these facts, not another source-selection engine.
- `selection::download_candidates` selects top-level selected resources, including
  widescreen under Scope::All (`src/selection.rs:919`). Do not substitute the
  base-only selection count or invent recursive acquisition expansion here.
- Existing input stamp includes serialized guide, folders and engine settings
  (`src/ui.rs:320`); install state already uses run + stamp (`src/ui/installing_page.rs:150`).
  Wizard allows reading during downloads but serializes other work (`src/ui.rs:409`).
- FOMOD Editor has launch/show and Choices/Dismissed outcomes (`src/fomod_ui.rs:9,32,67`);
  existing choice application preserves completed base components (`src/ui.rs:2214`).
- Startup currently reads default-store credentials/telemetry in App::new
  (`src/ui.rs:1706`), then accepts protocol BEFORE assigning settings_dir
  (`:3228–3232`). Explicit settings/probe isolation must precede both operations.
- Unix forwarding carries only the protocol string, not probe configuration;
  socket derives from USER and XDG_RUNTIME_DIR (`src/single_instance.rs:27–45,72`).
  allow_multiple_instances bypasses claim_or_forward (`src/ui.rs:3205`).
- Oracle: [pinned C# audit](download-orchestration-csharp-audit.md) §§4,7 documents
  Fetch's selected resources, status/sidebar defects, missing install gating and
  FOMOD prompt rules. Preserve useful behavior, not those documented defects.

## Serialized implementation boundaries

One source implementer at a time, with spec then independent quality review per step.
Do not overlap the active CLI/transport implementer or change its public contracts.

1. Desktop job adapter and safe initialization: src/ui.rs, new
   src/ui/downloads_panel.rs, src/main.rs GUI arguments only; additive launch or
   constructor helper as needed without breaking existing frozen callers.
   Deliver Fetch/NXM/shared progress/Stop and binding rules below together.
2. Complete settings, guarded FOMOD queue and navigation in those files;
   src/wizard.rs for copy, and src/fomod_ui.rs for an additive read-only lifecycle
   helper only (preserve Outcome/callers/frozen tests). profiles.rs may receive a
   narrow preserving peer-settings update helper. No engine/host-policy rewrite.
3. Run native acceptance; fix owned source, then record evidence under
   docs/parity/downloads-gui/. Parent updates COMPATIBILITY.md/progress/inventories.
   Do not declare either preceding step accepted from compilation alone.

## Fetch, job identity and terminal handling

G1. Fetch Downloads is available from DownloadsExplain onward and in utility view.
Require a loaded guide and valid mod workspace; game path is not an acquisition
prerequisite. Preserve current selections/options, derive candidates with Scope::All,
and capture guide + normalized roots + archive options + effective settings once.
Use shared plan/run, workers=4/retries=1, and per-run concrete PeerAcquire. Planning,
existing-file verification and FOMOD inspection run off the UI thread. No implicit
fetch just because a page opens; explain the actual trigger in page text.

G2. Assign a monotonically changing run identity before asynchronous preparation.
Bind it to guide revision, captured workspace/game identity and completed-base
context; frontend row keys use run + target index + source, never filename alone.
Every preparation/progress/terminal message carries run identity. Only the current
run may clear its receiver/busy state. Reject stale events before any guide, log-row,
FOMOD or validation mutation. Refresh/check context at terminal as well as per frame.
A stale terminal reports that files belong to captured folders; never silently
applies them to a reloaded guide, even if component GUIDs happen to be identical.

G3. Run terminal owns the authoritative RunReport; Phase::Completed alone does not
commit guide changes. On a matching terminal use shared record on a cloned bound
guide and replace current guide only if record succeeds. Preserve selection and
Files=false; do not add hashes without approval. Successful entries in a mixed
failed/cancelled report may be recorded for that SAME bound guide via shared record;
never record failed targets or call the whole batch successful. No automatic guide
file save: expose unsaved resource updates through existing explicit save flow.
Invalidate validation at acquisition start and after accepted metadata/choice edits.
Download success must never create install/completion receipts or validated=true.

G4. NXM queue uses nxm_target → AcquisitionPlan::single → shared run/record;
retain ambiguity/unmatched rejection and keyed-link rules. The current single plan
is Peer Off; do not invent Peer eligibility for a browser handoff. Serialize NXM
with Fetch/engine work, re-resolve queued links against the bound guide on dispatch,
and never let a late NXM terminal mutate another guide. Preserve direct URL utility
and its integrity checks; bind its optional FOMOD follow-up and terminal too.

## Presentation, Stop and readiness

G5. Rows display real Origin/Peer/Existing/Manual identity, phase, bytes/known total,
peer count when supplied and actionable failure/manual reason. Map registry URLs
from the bound plan/outcome: AcquisitionEvent has no registry_url. Redact keys,
NXM query credentials and proxy endpoints from UI logs/copyable diagnostics.
Manual links open only on explicit user action. Do not infer Superseded from text,
show never-started Peer as active, count race sources as separate archive successes,
or label unknown-total progress as a measured percent. Final counts come from
outcomes + skipped, with archive/target units explicit; selected component count is
separate. Keep status accessible after navigating or completing wizard pages.

G6. Stop requests cancellation for the active run and shows stopping until all
workers join and cleanup/terminal arrives. A new run cannot reuse its token or
start before it settles. Closing during work uses the existing confirmation path;
explain download cancellation separately from install rollback. Report partial
success, manual work, failure and cancellation distinctly. Repaint on updates;
bound retained presentation/log data. Retrying starts a fresh shared plan, preserving
verified existing archives rather than replaying a stale target list.

G7. Reading pages may continue during acquisition. Disable source/guide/profile/
selection mutations and engine Validate/Install/Restore while acquisition is active;
programmatic changes still invalidate binding. Disable Validate/Install while a
FOMOD dialog or queued choice remains unresolved. After acquisition ends, allow
validation to diagnose manual/failed/missing archives; install still requires fresh
successful validation and ordinary install guards. Preserve consumed base choices,
failed-install recovery gates, phase ordering and completion receipts.

G8. Run shared fomod_followups only for accepted, noncancelled report context;
retain inspection errors as visible failures, not empty-success. Queue positively
identified follow-ups in shared order, one existing Editor at a time. Bind each
editor launch AND Choices/Dismissed outcome to guide/root/run context; clear stale
queues and reject stale outcomes. Use saved_fomod_choices/apply_fomod_choices and
remaining-component rules. Configured/warned suppress; dismissed may prompt again.
Observe editor closure even without an Outcome: inspection/error Cancel currently
returns None (`fomod_ui.rs:26,193`). Clear queue ownership/readiness when it closes.
Never relaunch on repaint, overwrite an open editor, or equate inspection with choices.

## Settings and isolated probe launch

G9. Read PeerSettings (`src/p2p.rs:367,443`) before planning; sharing=false
wins over mode. Show default Race, proxy-only/fail-closed behavior, upload cap and
public proxy/tracker discovery disclosure before Fetch, plus Sharing on/off and
Off/Race/Fallback controls. Persist only explicitly changed keys through preserving
read-modify-write with atomic save (`src/profiles.rs:647–686`); malformed files
fail visibly; unknown keys/secrets survive; case-variant keys cannot override Off. Off takes
effect for the next run; controls are disabled during active runs. Do not claim
keepSharing starts a persistent seeder: that lifecycle remains absent.

G10. Hidden GUI probe options mirror accepted CLI explicit API key/base and loopback
CA rules, never settings/environment fallback for overridden endpoints. Any probe
option forces separate-instance mode with NO socket claim/forward and requires an
explicit non-default settings directory (resolve aliases, reject default identity).
Initialize that settings context and explicit credential override before ANY store,
telemetry, guide-triggered job or protocol dispatch. Probe arguments never persist.
Normal launches retain normal trust/instance behavior. Public library types and
frozen tests stay source-compatible; no new Rust tests/harnesses or test execution.

## Native acceptance and completion boundary
Use the real production desktop binary on private Xvfb, disposable HOME/XDG/settings/
roots and isolated network namespace. Disable public lists/trackers and telemetry
in fixture settings; inherit the reviewed local TLS, recording HTTP proxy, SOCKS
relay and production seeder fixtures. No real-host traffic or default KDE session.
Record exact argv/source/binary hashes, actions, screenshots, server logs, terminal
facts, guide/root manifests and assertions; screenshots alone are not runtime proof.

- At 1280×800 and 600×400: idle, Fetch preparation, two live transfer rows, status
  after navigation, Manual reason/link, stopping, terminal summary and completion sidebar.
- Compare GUI Fetch and CLI on the same selected/unselected/widescreen/existing/false
  fixture; compare record diffs and exact archive bytes. Prove Origin-win and Peer-win,
  gates with zero Peer connections, disabled sharing zero bootstrap, retry and cleanup.
- Cancel mid-body and mid-race; no premature idle/new run, unaccepted publication or
  residue. Exercise context changes through actual UI/protocol operations using only
  approved API-base/CA/settings overrides. No synthetic-message or hidden Rust harness;
  unreachable late-message runtime cases remain pending, with source review labeled.
- GUI NXM versus CLI produces matching resource changes. Probe launch with stored/env
  sentinel secrets sends only explicit synthetic key; missing/default settings rejects,
  user's socket receives no forwarding, wrong CA/redirect escape refuses.
- Two FOMOD archives open serially; apply/dismiss/configured/warned/inspection-failure
  and stale-editor cases preserve guide ownership and fresh-validation requirements.
- Settings survive restart with unknown-key preservation; original guide selections,
  game bytes and install completion facts remain unchanged by acquisition alone.

Production builds/strict Clippy (lib/bin only), fmt and frozen-source checks follow
native proofs. If a runtime case cannot be reached safely, mark it pending instead
of replacing it with source inspection. Persistent seeding, advanced CLI FOMOD modes,
full host automation, cross-platform instance parity and F29 installs remain open.
