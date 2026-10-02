# Structured application log and saved update presentation

Source implemented on 2026-10-02; native and provider acceptance remain deferred.

The application logger retains additive typed records containing a producer timestamp, severity, message and sequence identity. It never infers severity or time from raw process output. Typed retention is capped at the smaller of configured history and 4,096 records, with 1 MiB of message text and UTF-8-safe 64 KiB per-message truncation marked explicitly. Existing string-history APIs remain intact.

The Output log surface has an expandable Application records view and separate Activity output. Typed rows support selection, selected/all copy, multiline text selection, clear, auto-scroll and saving the exact bounded snapshot through the existing background save worker. A busy logger never blocks the renderer: nonblocking snapshot/clear APIs preserve the cached view, show the busy state and retry pending clear on later repaints. Disk writes remain on existing logging paths; the panic path preserves its previous try-lock behavior and does not claim disk I/O is nonblocking.

The desktop now loads saved update summaries once per settled guide/settings input identity after preferences load. This is a read-only background load, with no provider request or metadata application. Results retain the existing stale-input check, and failed reads wait for an explicit retry or changed input. Fresh and partial provider results bind the post-result identity so automatic loading cannot immediately overwrite them.

Update checks also retain whether their settings came from the implicit default or an explicit directory. They use the shared provenance-aware settings factory and validate the captured directory before reading the key, preserving implicit legacy migration without exposing fallback to explicitly isolated settings stores.

Source reviews corrected UI mutex blocking. No application launches, tests, filesystem harnesses, provider probes or CI were performed for these changes. Production compilation is recorded separately.

## Focused record highlighting

The retained record view now supports the pinned reference's H action: with a
record row focused, H toggles highlighting for each selected record. An explicit
button exposes the same action. A separate bounded ID set is pruned against the
current cached history and reset after successful clear. Highlight borders remain
independent of row selection; producer records, copied text and saved snapshots
are unchanged. Other focused inputs do not consume this shortcut. This production
presentation change passed independent source review; native interaction remains
unverified.
