# Cancellation for update checks and URL import — 2026-10-02

Working source; production compilation and transport/native acceptance are pending.

`updates::check_with_cancel` shares the original check loop, cache, version
application and partial-error summary with `check`. The original public wrapper
remains available. The new result carries both the Summary and an explicit
cancelled flag. Completed resource metadata remains in the caller-owned clone;
desktop consumers must discard that clone after cancellation. The CLI prints the
partial result and exits 130 before writing either the output guide or snapshot.

`NexusClient::get_mod_info_with_cancel` and `validate_with_cancel` own async HTTP
and response-body futures. They retain the established origin-bound redirect
policy and API headers. Response bodies are capped at 1 MiB. One capped 429 retry
is retained, with an interruptible wait. The request future is dropped when the
cancel flag is observed; errors redact query values. The synchronous adapter is
intended for existing application worker threads.

`protocol::fetch_instruction_with_cancel` is now also the implementation behind
the original URL fetch wrapper. Every redirect has its URL and resolved addresses
validated. Connections use those pinned addresses with automatic redirects and
proxies disabled, preventing a second resolver or proxy from bypassing the public
address check. Credentials in URLs and HTTPS downgrade redirects are rejected.
The complete operation has a 60-second deadline, at most 10 redirects and a 5 MiB
body budget. Bytes are published with the existing no-clobber temporary-file
boundary only after the final cancellation check and file synchronization.

Both adapters poll cancellation every 40 ms while waiting for async work. Blocking
OS resolver jobs cannot be forcibly interrupted. Runtime shutdown therefore waits
at most 100 ms; a resolver may finish later, but it owns no HTTP request future,
guide clone or destination file. This is not a claim that all OS DNS work stops
within the polling interval. Cancellation observed before publication prevents
the final destination; once publication succeeds, the completed file is retained.

No Rust tests, custom harnesses, filesystem probes, provider requests, native
application runs or CI were executed for this implementation.
