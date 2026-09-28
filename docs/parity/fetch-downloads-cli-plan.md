# Selected-guide CLI acquisition: implementation contract

Status: controller-reconciled plan; implementation and independent review pending.
F12–F15/F26 remain partial. Source baseline: integrated wave two, `2252894`;
subsequent Switch fixes are independent. This concise contract replaces the draft
pseudocode, whose contradictory settings/race instructions must not be implemented.
The earlier approved [download plan](download-orchestration-plan.md) provides
feature context; this document defines the next bounded vertical slice.

## Scope and ownership

One fresh implementer owns one complete vertical slice, in this order:

1. Orchestrator gate/event hardening in `src/download_orchestrator.rs`.
2. Concrete peer adapter in new `src/peer_acquire.rs`, exported from `src/lib.rs`.
3. Additive typed cancellation helper in `src/download.rs` and visibility-only
   `p2p::stop_signal_flag` change in `src/p2p.rs`.
4. `fetch-downloads` command in `src/main.rs`, small command-only helpers as needed.
5. Direct production-CLI fixture driver/evidence in `docs/parity/fetch-downloads/`.

No GUI, full host automation, TLS trust hook, FOMOD interactive choices, persistent
background seeding, profile-switch, or loader-normalization edits in this slice.
These remain required later. Do not change `Acquire::acquire`, or add required fields
to `Source`, `ArchiveTarget`, or `RunSettings`. Preserve all existing Rust tests
byte-for-byte. No new Rust tests or temporary/custom Rust harnesses and no test
execution. Production build/Clippy and direct CLI fixtures are authorized.

## Verified interfaces

- `acquisition::Acquire::acquire(&self, &ArchiveTarget, &Path, &AtomicBool,
  &mut dyn FnMut(AcquisitionEvent)) -> Result<Acquired>` has no target index argument.
- `AcquisitionEvent` fields: target_index, component_guid, filename, source, phase.
  There is no registry_url field. Adapter events are rebound by the orchestrator.
- `ArchiveTarget::destination` returns a FILE path for Some(filename), a DIRECTORY
  for None. The adapter must honor this distinction, including peer race staging.
- `selection::download_candidates` yields top-level index/GUID pairs for selected
  components with resources. Reuse it, `SelectionFlags`, and `apply_selection`.
- `profiles::SettingsFile::resolve(optional_dir)?.load()` reads global settings
  without writes; absent/null yields `{}`, malformed current settings errors.
- `profiles::Store::new` is also read-only (just stores an absolute path).
  `Store::nexus_api_key` implements established current/legacy credential lookup.
  An earlier controller concern about constructor writes was incorrect.
- `p2p::PeerSettings::{from_settings,effective_mode,policy}` exists. Only `policy`
  bootstraps tracker/proxy data; do not invoke it during preparation or dry run.
- `p2p::fetch_authored` accepts AuthoredTorrent, a FILE destination, SHA256,
  &PeerPolicy, &AtomicBool, &archive::Options and a PeerProgress callback.
  Using it directly instead of wrapping PeerSource's identical forwarding method
  is allowed; avoid needless public wrappers.
- `p2p::stop_signal_flag` already handles SIGINT/SIGTERM; expose it publicly with
  documentation for binary use. Do not add a second signal dependency.

## Orchestrator requirements

O1. Rebind Peer event target_index/component_guid/source in both single-source and
race callbacks before forwarding. Filename may come from the adapter for initially
unnamed targets; known filenames must remain the target's known name. Do not let
an adapter's placeholder index or source escape to observers.

O2. Origin login/CAPTCHA/free-wait/HTML gates terminate with ManualRequired and
never start Peer. In Race mode start Origin first; invoke Peer Acquire::acquire only after Origin's
first Transferring (after status/body gate classification) or a confirmed non-gate
failure. A spawned thread may wait inertly, but it must not invoke acquire or
bootstrap the adapter until the orchestrator grants this condition. A finished Origin needs no peer start. Cancel before launch opens no peer
activity. Keep private staging, no-clobber publish, loser cancellation/join/cleanup,
and Superseded behavior for actually started losers. Do not report a never-started
Peer as running or superseded. Each racer result must be consumed once; no hangs
when Origin completes/fails before Peer starts.

O3. NexusPremium can fall through to Peer only after Some(Ok(true)) premium
validation. Known free is ManualRequired(FreeUserWait). Validation errors/unknown
status are terminal Failed with will_fallback=false, not ordinary transport
failure and not a claim that the user is free. Apply in Race AND Fallback modes.
Use a private typed outcome/marker; no public trait/struct break required.

## Peer adapter requirements

P1. Build a per-run adapter with PeerSettings, caller's archive::Options, optional
cache path, and authored TorrentInfo lookup keyed by component GUID and registry
URL. Traverse component/options privately if needed. No new global state.

P2. Check cancellation, metadata validity and path/name safety BEFORE initializing
policy. Missing TorrentInfo is optional; present non-string/bad-base64/mismatched
infohash is a per-target Peer failure, never silently discarded. Validate an
authored info dictionary even for a known filename. Known filename uses the passed
file destination directly after one_component validation. For unknown filename,
derive a safe single-component name from checked info or sanitized magnet dn and
join it to the passed directory. Never join a second filename to a file path.

P3. Cache policy once per run (e.g. OnceLock over a private policy outcome); adapter
is already Arc-shared, so no redundant inner Arc. Lazy bootstrap occurs only inside
an eligible acquire call. No policy/network for dry run, peer off, existing-only,
invalid metadata, pre-cancelled requests, or gated Origin. Proxy failure stays
fail-closed; no direct peers/DHT/LSD/UPnP fallback. User endpoints/settings are
trusted as settings; guide-authored peer socket addresses remain ignored.

P4. Pass caller scratch_dir/max_expanded_bytes to fetch_authored. Map progress to
Transferring/Verifying; preserve typed cancellation using an additive download
helper if needed. Do not expose proxy endpoints in normal acquisition events.
Verified path+SHA256 return to the existing orchestrator publication/record flow.
Never publish hash-invalid or archive-invalid bytes or leave peer staging behind.
Persistent keep-sharing is a later frontend lifecycle task, explicitly incomplete.

## CLI and settings contract

C1. Add `fetch-downloads --input PATH --source-dir PATH`, optional --game-dir,
--use-file-selection, repeatable --select/--option, existing SelectionFlags,
--workers (4), --retries (1), --nexus-mods-api-key, --settings-dir, hidden
--nexus-api-base, optional --peer-mode off|race|fallback, --scratch-dir,
--max-expanded-bytes, --dry-run, --progress-json, --record. No hidden implied
selection writes: retain a pristine loaded guide for record. Default selects all
top-level components unless file selection or explicit selectors are requested;
reuse the shared selection/enforcement semantics for options/filters.

C2. Prepare the EFFECTIVE peer settings/mode before the final AcquisitionPlan.
Default from PeerSettings is Race; persisted Off/Fallback must affect both plan
and execution. An explicit CLI mode overrides stored mode, but peerSharing=false
always wins. Explicit peer-off can skip peer settings. Relevant settings reads
are allowed in dry run; filesystem writes and network are not. Do not return a
dry-run plan before applying relevant persisted settings. Absent settings use
defaults; malformed needed settings error rather than silently default.

C3. Sensitive credential lookup only when selected candidate resources require
Nexus and no explicit key is present. Explicit key bypasses stored-key lookup.
An overridden API base ALWAYS requires explicit nonblank key and must never fall
back to settings/environment credentials. Keep the existing client HTTPS or
loopback-HTTP policy and origin-scoped redirects. Peer settings reads are separate
from credential precedence. Do not echo keys/credential URLs in new diagnostics.
Store::nexus_api_key is read-only and may be reused to preserve legacy precedence.

C4. Plan/run/record/fomod_followups stay in the shared library. `--dry-run` prints
the effective plan and returns without run, policy creation, signals, saves or
FOMOD extraction. Respect cancellation via existing stop flag. `--record` updates
successful/already-present resource facts on pristine guide only; never persist
transient selection changes. File write errors fail visibly.

C5. Ordinary output is one pretty JSON summary containing report, recordSummary,
fomodFollowups. With --progress-json every stdout line is a standalone JSON object:
acquisition event envelopes followed by one single-line summary envelope. Send
human diagnostics to stderr. Exit 130 for cancellation, otherwise 1 for failed
outcomes or FOMOD inspection errors, 3 for remaining manual work, 0 for complete
acquisition. Emit summary first; run must join workers/clean temporary resources
before exit. Advanced FOMOD choices remain explicit followups, not auto-installed.

## Direct acceptance and source binding

Use production binary only, fresh disposable roots, generic fixture names, local
recording SOCKS/seeder/API servers. Disable public proxy/tracker lists in ALL
fixture settings. No real host downloads, no user game trees. Record exact argv,
exit/stdout/stderr, source and binary hashes before/after, and server connection
logs where absence of network is claimed. Existing source tests stay unchanged.

1. Dry-run selected/unselected/disabled/resolution/default-file cases; mode respects
   stored Off/Fallback and sharing=false; no connects, saves, source writes or
   cache creation. Malformed needed settings errors; explicit key + peer off
   bypasses unreadable settings. API override without explicit key sends no request.
2. Local fake Nexus validate free/error in Race and Fallback: correct terminal
   ManualRequired/Failed, no Peer progress or proxy/list connection.
3. Unsupported-host fallback through LOCAL SOCKS to LOCAL seeded archive: real
   Peer completion for known filename and initially unknown filename; returned
   bytes/hash match, no unaccepted peer staging remains, event identities correct.
   Inspect any retained external fixtures before reuse; none are checked in here.
   Build minimal Python/shell local SOCKS/seeder drivers in the owned evidence
   directory where needed, preferably inside an isolated network namespace.
   Stop only task-owned child PIDs. No fake Acquire or throwaway Rust executable.
4. Bad base64/infohash/filename refuses Peer before any bootstrap; tiny expanded
   budget refuses an otherwise valid archive and publishes nothing.
5. Existing verified archive skips transfer and supports record/fomod followups;
   record preserves original IsSelected values, including excluded components.
6. NDJSON is parseable per line including summary. Peer cancellation via SIGINT
   returns nonzero and cleans temporary files; use a slow local peer fixture.
7. Shared proxy pool connection logs establish reuse where observable. The current
   progress schema deliberately omits proxy-rotation events, so don't infer reuse
   from nonexistent events or a source-only assertion.
8. Origin HTTPS login/challenge/body race/cancel fixtures require the planned
   loopback CA hook, not currently implemented. Explicitly record these unrun
   runtime cases and do not claim full acquisition acceptance from Nexus API fakes.
   Source-review O2 now; add that independent production trust-hook slice next.

Gate this whole vertical slice with spec review then independent quality review.
Run production CLI/default builds and strict Clippy `--lib --bin modsync`, fmt,
and frozen-source checks, never cargo test/all-targets. Commit owned files locally
only. No push while test-freeze/CI conflict remains unresolved. Update progress
with implemented vs proven vs still-open behavior; do not promote whole domains.
