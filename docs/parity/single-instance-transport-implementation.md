# F23 single-instance transport source slice

Status: source implementation; no runtime, native transport, package, or full F23 acceptance claim. This slice implements the transport portion of platform plan R1–R3/KTD2. Registration, macOS AppKit callbacks, launch packaging, and release identity remain separate.

## Preserved public contract

`claim_or_forward(Option<&str>)` still returns either a primary `Receiver<String>` and `Guard`, or `None` after forwarding. Activation retains `ACTIVATE_MESSAGE`; the existing desktop adapter still consumes ordinary strings. The frozen inline Unix test suffix is unchanged. Cold-start protocol handling remains the caller's responsibility, as before.

The primary receiver comes from a synchronous channel with capacity 64. Admission uses `try_send`: a full or disconnected queue produces a negative acknowledgement. Successful acknowledgement means shared-parser validation and queue acceptance, not that the GUI downloaded or executed anything.

## Shared wire contract

- Four-byte big-endian JSON frame length, maximum 64 KiB encoded frame.
- Version 1 request, UUID request ID, activation or protocol message of at most 8192 UTF-8 bytes. JSON escaping has separate framing allowance.
- URLs are validated with the existing shared protocol parser at both dispatch and admission.
- Versioned acknowledgement carries the same request ID and accepted/rejected result.
- Secondary retries one uncertain delivery with the same ID. The primary remembers the newest 256 accepted ID/message pairs, acknowledges exact retries without queueing them again, and rejects changed content for a retained ID.
- One actively processed client; bounded OS listening queue/pipe instances and two-second connect/exchange deadlines. No unbounded per-client worker creation.

Deduplication is a bounded recent-request window, not durable delivery across primary restarts. The wire protocol deliberately refuses legacy fire-and-forget peers instead of treating their writes as acknowledged delivery.

## Unix

The endpoint lives under an owner-only `modsync-rs-stable-<effective UID>` directory in XDG_RUNTIME_DIR, or the temporary directory when XDG_RUNTIME_DIR is absent. It uses effective UID rather than the mutable USER string. Directory ownership/mode and endpoint type/owner are checked; Linux/Android use SO_PEERCRED, while macOS/BSD use getpeereid for authentication in both directions.

A safely opened, private regular lock file is held for the primary lifetime. Only missing/refused connection results permit claiming that lock; another probe while holding it must again prove no listener before removing a stale owned socket. Permission errors, timeout, write/acknowledgement failure, wrong peer, and full queue never authorize unlink or takeover. Lock contention is bounded and repeatedly probes a starting owner. Nonblocking connect/read/write loops have deadlines.

The guard signals cancellation and joins the owned listener. Listener teardown removes only the recorded socket inode/device while still holding the ownership lock. The persistent lock inode is never deleted.

## Windows

The backend uses Tokio named pipes with `PIPE_REJECT_REMOTE_CLIENTS`, a protected DACL granting the current user access, and a stable SID plus session namespace. Both endpoints verify the peer process token's user SID and session. `FILE_FLAG_FIRST_PIPE_INSTANCE` provides an atomic ownership claim; kernel pipe lifetime replaces filesystem stale-endpoint recovery. Failed authentication or access never triggers replacement.

At most two pipe instances exist: the connected client and its already-created listening successor. A successor is kept alive before closing the connected instance to preserve name ownership. The client confirms it read the acknowledgement, avoiding a blocking `FlushFileBuffers` shutdown. Async deadlines bound connection and frame exchange; shutdown joins the listener after its active bounded exchange.

## Remaining acceptance and boundaries

No Cargo, tests, filesystem fixtures, sockets, pipes, registrations, or runtime probes were executed for this slice. The parent owns production compilation and independent source review. Native acceptance still needs cold/warm activation and both schemes, peer rejection, queue saturation, lost acknowledgement retry, stalled clients, crash/stale takeover, and shutdown on Linux, Windows, and macOS. Package/channel selection currently has the fixed `stable` namespace; distributing additional channels requires explicit namespacing policy before those packages ship. Existing running binaries with the old endpoint/wire protocol are not transparently upgraded in place.
