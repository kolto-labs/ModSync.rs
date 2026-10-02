# Download transport diagnostics

The shared origin transfer removes the attached URL from every reqwest request-send error, including provider-issued signed CDN links and redirect failures. This applies regardless of cookies, loopback trust, or origin confinement. The former conditional policy left production Nexus CDN failures able to retain signed query parameters in acquisition error text.

Blocking response prefix/body reads can wrap a reqwest error in an I/O error. The reader now removes attached reqwest URLs from these wrappers while retaining I/O kinds and reqwest timeout/status metadata. Existing typed failure classification, retry policy, cancellation checks, status-only HTTP diagnostics, and structured authored resource/manual action URLs are unchanged. This is a narrow URL diagnostic rule; it does not claim arbitrary downstream error text is free of every possible secret.

Source inspection included the locked reqwest blocking response reader. No requests, runtime checks, builds, tests, or fixtures were run for this change. Provider discovery warning interpolation and legacy Nexus bounded/cancellable reader migration remain separate audit findings.

## Nexus acquisition and discovery follow-up

Discovery and acquisition now use bounded cancellable key validation and file listings. The additive `NexusClient::nxm_download_link_with_cancel` applies the same owned request future and 1 MiB document limit to optional file metadata and keyed download-link resolution. The URL is constructed by query encoding as before; HTTP authorization refusal remains typed and CDN requests remain separate from the API-key client. Legacy public synchronous methods remain available for compatibility.

The existing cancellable transport polls cancellation every 40 ms, drops owned HTTP/body/rate-limit futures on cancellation, and bounds runtime shutdown to 100 ms. Platform resolver jobs are not forcibly interruptible; this is not a hard operating-system cleanup guarantee. Acquisition retains deferred rate-limit errors with capped Retry-After for its outer retry loop; discovery's internal bounded retry sleep is cancellable. Optional filename metadata can fail without blocking download-link resolution, but cancellation is checked before proceeding. A cancellation observed while mapping an acquisition failure now produces the existing Cancelled outcome.

Orchestrator warning labels no longer copy resource URL queries/fragments/credentials. Where resource distinction is useful, a short SHA-256-derived label identifies it without exposing URL text. Nexus malformed-page diagnostics and missing registry-entry diagnostics omit raw URLs. Structured registry and manual-action URL fields remain functional data; consumers retaining such fields must treat them according to their data contract.

These changes are source-reviewed implementation work, not provider or cancellation runtime acceptance. No tests, builds, fixtures, or network requests were performed by this implementation worker.

Independent review additionally required the origin-race cancellation token through both Nexus resolution stages. Waiting for the shared client mutex now polls cancellation instead of blocking on an unrelated request. A deferred typed rate-limit error from optional filename metadata is propagated to the outer retry loop rather than immediately issuing the download-link request.
