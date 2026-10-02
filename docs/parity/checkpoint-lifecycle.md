# Checkpoint lifecycle implementation

T5 source implements held-lock deletion, garbage collection, and content repair through the shared `Store` API. CLI and native history use these methods. Production compilation/static review and runtime acceptance are separate gates; no lifecycle mutation scenarios or reference-service differentials have been executed in this implementation slice.

## Deletion and retention

`delete_session` requires exact session/checkpoint/generation, matching held recovery locks, and no pending operation. It rejects the selected active session, unfinished install sessions, and sessions referenced by parent/safety lineage, including published incomplete branches. It writes a bound deletion record, renames the session into store-local trash, then synchronizes both affected directories. Objects remain until a later fresh garbage collection scan. A durability failure is an error even if the rename already occurred.

`gc` refuses pending journals and cleanup HEADs. It validates bounded store metadata, computes retained ancestry and receipt objects, and verifies retained content before removing possible alternate repair sources. Corrupt metadata or retained content stops collection. Committed-session orphan checkpoints and uncommitted temporary files can be reclaimed. Incomplete sessions and their object closure are conservatively retained and counted in the report; when any incomplete session exists, orphan checkpoint documents are also retained. This avoids guessing whether an interrupted unpublished branch is disposable.

Trash cleanup retains its deletion record until the moved payload has been removed. Interrupted partial payload cleanup can continue; exact empty or temporary-only UUID trash directories can also be collected. Repair diagnostics and quarantined corrupt bytes are retained. This is intentional diagnostic retention, not automatic retention expiry. No undelete guarantee is made after GC.

## Repair

`repair` validates immutable metadata and pending intents without requiring object integrity first. It includes the requested committed checkpoint, finalized receipt, and applicable pending Finalize/Restore/Resume/SourceRebase closure. A prepared branch without a HEAD is eligible only when a validated pending transition names it. Pending branch/restore target and safety objects cannot be inferred from arbitrary orphan files.

Candidates are bound live paths, other CAS files, and deterministic completion receipt reconstruction. Every candidate must match the expected SHA256 and length. A second streamed hash verifies the bytes copied into a temporary object before publication. Expected metadata is never rewritten. Existing corrupt bytes move to a diagnostic quarantine; repaired bytes publish without replacement, synchronize, and are reverified. Install roots are never changed.

The typed report preserves successful repairs and unresolved identities. Missing candidates, cancellation after partial work, and per-object read/publication failures produce `complete: false`; CLI returns failure and the UI shows an incomplete result. Unsafe metadata/path failures can still return an error before a trustworthy report exists. Cancellation is not observed between quarantine and completing one verified object publication.

## Limits and remaining acceptance

The writer retains the existing Unix durability capability gate. Directory synchronization failures never report successful durable mutation. Lifecycle inventory and metadata are bounded, safe reads reject links and special files, and deletion uses a checked bounded inventory instead of an unchecked recursive removal call.

Still required by the overall plan: controlled interrupted deletion/GC/repair scenarios, actual reference-service behavioral differential, cancellation receipts, and native interaction acceptance. No build/static review result establishes those runtime outcomes.
