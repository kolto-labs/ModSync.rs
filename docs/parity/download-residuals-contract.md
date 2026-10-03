# Download residuals contract (Fetch / nxm orchestrator)

User-visible behavior of `fetch-downloads` (and desktop Fetch through the same
orchestrator) after the download-residuals change. Evidence:
[download-residuals-proof](download-residuals-proof/README.md).

1. **A known name taken after planning is adopted, not an untyped failure.** A
   component with a `Files` name can find that name taken when its Origin transfer
   runs. This happens when an ungrouped target published it first, or when a concurrent
   transfer wins the no-clobber move. The component still transfers, then follows the
   present-file rule for revealed names. It adopts under its own `ContentHashSHA256`, or
   with no hash only when the bytes match. Otherwise it fails with an error that names
   the present file and says it was not replaced. The present file is never replaced.
   The cost is one transfer that may be discarded. C# would download and overwrite.
2. **A present file that fails the plan's existing-file check now costs a transfer.**
   A present but corrupt or mismatched archive under the authored name used to fail at
   once with `Download destination already exists`. It now fails after the transfer,
   with the typed refusal ("SHA-256 mismatch: the present file ..." or "differs from the
   downloaded bytes ..."). The file is still kept.
3. **Unnamed Nexus targets**: a name revealed by the API that is already present is
   settled by the same rule after the transfer. MEGA follows the same rule (source only).
4. **Ctrl+C during planning** (for example while hashing a large present archive)
   stops promptly and exits 130, as a cancel during the run does. Nothing is requested
   or written.
5. **Origin progress totals**: `transferring.total` is the response's Content-Length,
   or `null` when the server sends none (chunked). It is no longer the length left after
   the 64 KiB sniff prefix, which was `0` for small archives. MEGA reports its node
   size (source only).

Unchanged and still open: the race's refusal-to-Peer path has source evidence only.
Standalone `download::fetch` callers keep their pre-transfer refusal of a present
destination.
