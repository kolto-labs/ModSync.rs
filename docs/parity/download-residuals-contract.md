# Download residuals contract (Fetch / nxm orchestrator)

User-visible behavior of `fetch-downloads` (and desktop Fetch through the same
orchestrator) after the download-residuals change. Evidence:
[download-residuals-proof](download-residuals-proof/README.md).

1. **A known name taken after planning is adopted, not an untyped failure.** A
   component with a `Files` name can find that name taken when its Origin transfer
   runs. This happens when an ungrouped target published it first, or when a concurrent
   transfer wins the no-clobber move. It then follows the present-file rule for revealed
   names. It adopts under its own `ContentHashSHA256`, or with no hash only when the
   bytes match. Otherwise it fails with an error that names the present file and says
   it was not replaced. The present file is never replaced. C# would download and
   overwrite.
   - **With an authored hash**, a name already present before the transfer is settled
     at once, with no request: the present file alone decides (adopt on the hash,
     refuse otherwise). A name revealed by the provider before the transfer (a Nexus
     API name) is settled the same way. A name taken only while the transfer runs still
     goes through the no-clobber publish and is adopted or refused after it.
   - **With no hash**, the component still transfers first, because only the
     transferred bytes can say whether the present file is the same file. The cost is
     one transfer that may be discarded.
2. **A present file that fails the plan's existing-file check.** A present but corrupt
   or mismatched archive under the authored name used to fail at once with the untyped
   `Download destination already exists`. With an authored hash it again fails at once,
   now with the typed refusal ("SHA-256 mismatch: the present file ... it was not
   replaced"). With no hash it costs a transfer, then fails with "differs from the
   downloaded bytes ...". The file is kept either way.
3. **Unnamed Nexus targets**: a name revealed by the API that is already present is
   settled by the same rule after the transfer, or before it with no request when the
   component authors a hash (item 1). MEGA follows the same rule (source only).
4. **Ctrl+C during planning** (for example while hashing a large present archive)
   **or during execution-time discovery** (for example while a bare Nexus mod page's
   file list is fetched) stops promptly and exits 130, as a cancel during the run does.
   Nothing is downloaded or written. Without a signal, a planning or discovery error
   exits 1 with its own message, as before.
5. **Origin progress totals**: `transferring.total` is the response's Content-Length,
   or `null` when the server sends none (chunked). It is no longer the length left after
   the 64 KiB sniff prefix, which was `0` for small archives. MEGA reports its node
   size (source only).
6. **Peer known-name collisions are typed** (source only: Peer cannot run on the proof
   kernel, which has no IPv6). A Peer copy for a `Files` name that is already present,
   or that loses its no-clobber move to an ungrouped publisher, returns the same typed
   `DestinationExists` as Origin and is adopted or refused by the same rule. It no
   longer fails with the untyped "Could not publish the peer copy ... File exists" (in
   a race) or "Download destination already exists" (Peer alone). Every Peer copy
   carries the authored hash. Peer alone settles a present name before its transfer; in
   a race the copy is staged first and the name is settled when the copy is published.

Unchanged and still open: the race's refusal-to-Peer path has source evidence only.
Standalone `download::fetch` callers keep their pre-transfer refusal of a present
destination.
