# Persistent sharing implementation

Status: production source implementation and independent source review in progress.
No compiled, runtime, desktop, network or interoperability acceptance is claimed.
The persistent-sharing plan's production receipts remain outstanding.

## Application API

`SharingOwner::start(SettingsFile, generation)` acquires the separate lifetime
owner lock and starts a controller with at most one transfer worker. A second
owner returns None. Pending producers continue using the short transaction lock.

The desktop must poll `snapshot()` and asynchronous barrier acknowledgements:

- `request_pause(PauseReason::{User,Settings,Mutation}) -> Barrier`: signals the
  active cancellation token immediately and acknowledges only after worker join
  and durable catalog pause. The owner remains alive to consume pending entries.
- `request_resume() -> Barrier`: retains uploaded bytes and active duration, never
  resets exhausted records, and does not release deferred context-approval holds.
- `authorize_enrollment_from(&Guide, &Enrollment, Option<PathBuf>) -> Barrier`:
  authorizes only explicitly verified enrollment identities against the captured
  guide. A supplied saved guide must independently retain matching authored
  metadata before its path becomes restart provenance. With an empty identity
  list this replaces the current in-memory guide following a mutation barrier.
- `authorize_enrollment` is the same operation without saved-path promotion.
- `pause_and_join` is the blocking background-call equivalent. `shutdown` and Drop
  cancel and join the transfer and controller; the lifetime lock survives until
  that cleanup completes. Neither creates a detached process or daemon.

`SharingSnapshot` carries generation, current identity/state/reason, cumulative
uploaded bytes, active milliseconds and whether the owner has stopped. The
Sharing state requires the real transport Started callback. Queued and verifying
never count as uploading; a shared monotonic statistics snapshot provides counts.

Desktop background admission uses `enroll_report_cancelled` with no guide path.
This creates durable approval-held records, including when a desktop worker is
already awake. Only a still-current accepted UI context may call authorization
and release those specific identities. Stale results remain paused. Authorizing
while the owner is paused preserves that pause durably. CLI callers retain the
existing `enroll_report` signature and supply saved local guide provenance.

## Scheduling, durability and budgets

The owner consumes a page of pending entries once per second while awake, using
existing idempotent publication. A durable identity cursor implements round-robin
selection over bounded pages of 128 active records. Paused, failed and finished
records move into a separate history directory. History preserves duplicate
suppression and accounting; finished tombstones omit the unused torrent-info blob.
Publication commits the destination before removing the old record. When a crash
leaves both locations, history wins, preserving pauses and exhaustion.

A worker receives only the remaining 24-hour active-duration and ratio-1.0 byte
budgets. Budget size comes from captured verified archive length, never the length
of a replacement file. Historical records without that field must pass fresh
hash/archive checks before the byte limit is calculated. Real monotonic intervals
run from transport unpause through stop, excluding preparation, queue waits,
application downtime and proxy selection between sessions. Counters checkpoint
once per second and settle after join. Every worker is durably marked in-flight
before spawn; unclean restart pauses rather than assuming missing accounting was
zero. Explicit Resume acknowledges that interruption without resetting counters.

An identity yields after five active minutes, or after 60 seconds without an
increase in real uploaded bytes. An idle identity is deferred five minutes; other
ready identities can run. Exhaustion leaves the active queue. Proxy/source
unavailability pauses the owner and records instead of repeatedly bootstrapping;
explicit Resume or a later startup retries transport-only failures. Other failed,
user-paused and unclean records require explicit action.

Off, KeepSharing Off and Mode Off are checked before policy/list bootstrap.
Pending records are still durably consumed into paused state. Settings load
failure also disables scheduling visibly. Policy changes take effect only after
the old worker stops and joins; the existing policy is never mutated live.

## Transport, cancellation and provenance

`share_verified_authored_observed` is an additive sibling of the existing public
primitive. `ShareOutcome` serialization and existing function signatures remain.
Started and progress callbacks report production librqbit transport state and
statistics, including across bounded proxy rotations. All sharing uses the
existing proxied-session implementation: no inbound listener, DHT, LSD, UDP
tracker or direct fallback is added.

The controller cancels preparation after 120 seconds. Hashing, source copying and
single-file torrent identity reconstruction check cancellation between bounded
reads. The no-info fallback reproduces the pinned librqbit 9.0.1 single-file
2 MiB layout and still requires the resulting infohash to equal the authored
magnet. Reconstructed info has the same 32 KiB admission bound. It does not mint
or authorize a replacement swarm. The independent private-copy hash and paused
engine piece recheck remain required.

The regular-file opener rejects symlink/special-file sources, uses nonblocking
no-follow final opens on Unix and rejects Windows reparse points. It validates
the opened descriptor as well as the directory entry. Parent-directory ownership
and canonical captured-root checks remain required. Shared engine copies retain
owner locks and are dropped only after transfer cleanup.

Preparation cancellation is cooperative. List requests retain their existing
20-second timeout, SOCKS health checks their 5-second per-attempt limit, and
active proxy rechecks can delay cancellation while their bounded checks finish.
Filesystem calls, kernel I/O stalls and third-party session shutdown do not have
a proven hard wall-clock deadline. The UI must remain in Stopping until the
actual acknowledgement; an AtomicBool alone is not shutdown proof.

Before each worker, the owner rechecks saved/current guide authorization, canonical
root and source location, authored SHA-256 and archive integrity. Unsaved guides
require explicitly authorized identities tied to this owner process; they cannot
auto-resume across restart. Saved guide timestamp/length changes stop an active
worker. These inexpensive stamps do not detect same-size/same-mtime replacement
instantly; full validation occurs before each subsequent session. Source mutations
receive the primitive's corresponding stamp and independent-copy checks.

The upload cap applies to the single completed-sharing session. Concurrent
acquisition sessions still have independent caps. Discovery-list HTTP(S) requests
remain ordinary bootstrap traffic, separate from SOCKS peer/tracker transport.
Outbound-only clients still need a reachable peer; no relay or universal NAT
traversal has been implemented.

## Source-only verification and remaining work

Source review checked the plan, existing catalog/admission paths, proxy transport,
archive cancellation and pinned librqbit creator/serializer source. Formatting
was restricted to production source. The frozen p2p test suffix was compared
byte-for-byte with HEAD and remained unchanged. No Cargo commands, tests, fixtures,
custom harnesses, runtime probes, C# processes or CI were used.

Independent source review and application wiring remain separate gates. All
network, GUI, restart, stop/exit, storage-failure, cap/accounting and real-receiver
receipts in the approved plan remain required. The implementation does not by
itself establish F12/F13/F14 acceptance or full Rust parity.
