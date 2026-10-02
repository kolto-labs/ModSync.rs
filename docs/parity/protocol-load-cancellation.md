# Protocol document loading and cancellation

## Library contract

The additive `protocol::fetch_modsync_guide_with_cancel(&str, &AtomicBool)`
returns the existing `(Link, Guide)` shape. The legacy `fetch_modsync_guide`
entry point remains and delegates with a false cancellation flag.

The helper checks cancellation with Acquire ordering before qualification,
before link parsing, before creating the temporary directory, before fetching,
before ordinary guide loading, and after guide loading. It qualifies the raw
handoff through `single_instance::validate_protocol_request`, using the shared
1–8,192 UTF-8 byte bound and protocol grammar. Qualification failures become a
generic error without the input URL or parser details. NXM links cannot become
guide imports; only ModSync links supply an instruction URL.

**Intentional intake change:** the legacy guide-opening wrapper now also applies
the shared 8 KiB raw handoff limit. The direct HTTP `fetch_instruction` wrapper
and CLI `protocol-fetch` retain their existing raw URL intake; this slice does
not claim an 8 KiB bound for those APIs.

An owned temporary directory contains the downloaded document and is cleaned
up on success, cancellation, decode error, or unwind. The existing cancellable
HTTP implementation remains authoritative: 5 MiB document limit, at most ten
redirects, 60-second operation timeout, individually qualified public addresses,
pinned direct connections, no HTTPS downgrade, and no-clobber publication.
OS DNS work may finish after bounded runtime shutdown, as already documented by
the fetch implementation; it does not retain a request or destination file.

Ordinary `Guide::load_for_install` parsing, draft generation, and selection
semantics are unchanged. Cancellation is checked around this bounded decode;
the helper does not claim cooperative interruption inside the decoder. A ready
guide is returned only after the final cancellation check.

## CLI adapters

`protocol-fetch` and `protocol-open` now obtain the existing process stop flag
and invoke the cancellation-aware library paths. Both pass results through the
existing `finish_acquisition_result` policy:

- An error with the cancellation flag set returns `cli_session::Exit(130)`.
- Other errors retain the existing command failure path.
- A completed publication or ready result wins over a signal observed after
  successful library completion.

Successful stdout remains the fetched destination path for `protocol-fetch`
and the existing `{"Link": ..., "Guide": ...}` JSON for `protocol-open`.
Cancellation produces neither a fabricated JSON result nor a duplicate ordinary
error diagnostic. CLI session cleanup remains on the existing `run` path.
Explicit `protocol-parse` and protocol resolution are unchanged.

## Desktop ownership and result admission

The desktop uses an owned named worker with a fresh cancellation token for each
load. The ordinary application receiver keeps existing busy gates in effect.
The worker emits one terminal as its last operation; the UI joins it before
accepting a guide or releasing the gate. A finished-worker poll joins and drains
remaining messages before treating a missing terminal as failure. Unwinding and
spawn failures report generic failures; dropping the worker cancels and joins it.

A result is bound to its run, captured installation input stamp, and exact Arc
cancellation token. A foreign terminal cannot release another operation's
receiver. Cancelled, stale, closing, or replaced-token results are discarded.
A successful guide carries that same binding through the sharing pause and is
checked again after sharing acknowledges, before any guide mutation. A late UI
cancellation therefore discards a ready guide; the CLI publication policy above
remains separately defined. Window close waits for this worker to settle.

The UI validates the shared 8 KiB grammar before starting its worker. Network,
temporary-file and guide decoding work stay in the worker. Existing periodic
repaint polling delivers the terminal without a post-terminal repaint callback.
Loading failures never log the authored URL or a parser/network error that might
contain a credential in its path or query. Stop explains guide-download cleanup;
this operation does not mutate the game's installation.

## Evidence boundary

This is production source implementation with independent source review.
No test, fixture, harness, runtime, network/native probe, or CI execution is
claimed. Desktop worker, cancellation, stale-result, sharing-continuation and close
integration are source implementations. FIFO/backpressure admission remains
a separately reviewed contract; this helper does not alter queued requests.
