# Loopback HTTPS transport for acquisition proof

Status: independently reviewed contract; not implemented. Start after the
selected-guide CLI slice is reviewed. This completes the currently unrun HTTPS
origin gate/race/cancellation fixtures; it does not authorize live-host downloads.

## Existing boundary

`download::fetch_to` owns classified errors, while private `fetch_inner` constructs
a reqwest blocking HTTPS-only client for every fetch. Existing public fetch,
fetch_with_options and fetch_to signatures must stay intact. `RunSettings` is a
public struct used by frozen callers: do not add required fields. The orchestrator
has a private RunContext and calls fetch_to from origin_once. Its new peer gate
waits for Origin's first Transferring event after classification.

## Proposed implementation

- Add an explicit, narrowly named loopback-CA configuration type in download.rs
  that parses a user-provided PEM certificate. Keep it out of environment/settings.
  Only new overload-style Rust functions accept it; existing entry points retain
  ordinary trust with no behavior change. Do not introduce global mutable state.
- Add a download entry point accepting optional loopback trust, with existing
  fetch_to delegating to the same private implementation with no override.
  Add an analogous orchestrator run entry point accepting this configuration;
  preserve current run and RunSettings. Thread the optional reference through
  private RunContext/origin_once only.
- Hidden `fetch-downloads --extra-ca-file PATH` creates this per-invocation
  configuration and uses the new run entry point. No persistence. The existing
  fake Nexus API may use the already-supported explicit-key loopback HTTP base;
  this hook applies to archive HTTPS transfers, not Nexus credential requests.
- When the override is present, require an HTTPS URL with literal loopback IP
  or exactly localhost, both initially and on EVERY redirect. Reject public
  hosts, deceptive suffixes and non-HTTPS before their requests. Bound redirects.
  Reject a disqualified redirect with policy `error`, not `stop`: returning the
  3xx body could misclassify a transport refusal as an HTML/manual host gate.
  Pin localhost resolution to loopback and disable environment proxy use for
  this private loopback client so the probe never goes through an external proxy.
  Preserve normal client's current proxy/trust behavior when no override exists.
- Add the certificate only to that narrowly scoped client; never disable TLS
  verification and never alter the system trust store. Invalid/unreadable PEM
  fails visibly. Do not add dependencies or modify frozen tests.

Exact added function/type names are implementation choices after checking the
post-CLI source, but the additive API shape and endpoint restrictions are fixed.
Read the locally locked reqwest source before choosing certificate/client APIs.

## Direct production acceptance

Use a disposable network namespace with loopback enabled, a Python TLS server
and recording SOCKS relay, and openssl-generated short-lived fixture CA/cert.
Bind receipts to source/binary hashes; all keys/archives/names are synthetic.
No Rust test or temporary Rust harness is permitted.

1. Without the hook, the fixture certificate is refused. With the explicit CA,
   the loopback archive verifies and publishes exact bytes. A different CA fails.
2. Initial public/deceptive HTTP(S) targets and redirect escapes are refused
   before the recording outside endpoint receives a request. Loopback HTTPS
   redirects succeed only while every hop stays eligible; downgrade refuses.
   Prove reachability independently: create a disposable non-loopback TEST-NET
   address/interface and namespace-local hostname mapping, with a fixture client
   that can reach the endpoint. A failed production request in an isolated
   namespace alone does not prove policy rejection. For environment-proxy proof,
   point HTTPS_PROXY to a recording HTTP CONNECT proxy and show that the ordinary
   client contacts it while the loopback override does not; the peer SOCKS relay
   is not a substitute for this separate HTTP-client control.
3. Login HTML, challenge headers/body and free-wait bodies terminate as the
   correct ManualRequired gate. Share-eligible targets cause ZERO peer adapter
   work/proxy connections before/after a gate, in Race and Fallback modes.
4. Slow valid Origin clears classification before a peer request starts; measured
   Origin-win and Peer-win cases publish once, join/cancel losers and leave no
   .part/unaccepted staging files. Use real production peer acquisition, not mocks.
5. HTTP429/5xx retry behavior remains bounded and cancellation-aware; SIGINT
   during body, retry delay, and race cleans temporary state and returns nonzero.
6. Unknown/unsafe content-disposition filenames, incorrect hashes and corrupted
   archives publish nothing. Archive budgets and JSON event identity stay intact.
7. An explicit API override never gains settings/environment secrets; the CA is
   never saved. Dry run performs no network, transfer/cache creation or save.

Spec review then independent quality review, production builds/Clippy/fmt only,
source freeze check, local commit. This fixture surface enables runtime proof;
it is not itself full host automation, GUI integration or F29 evidence.
