# Nexus H1a: shared API redirect policy and nxm key encoding

Bounded slice H1a of [host-providers-plan.md](../host-providers-plan.md) (H1 contract
items 1-2 and the redirect/encoding parts of its controlled production proof).
Source is commit `33e2612` (on top of `c02a060`) plus the uncommitted review
follow-up in `src/updates.rs` and `src/download_orchestrator.rs` described below;
`854c89e` on top of `33e2612` only adds `docs/parity/nexus-h1a-review.md`. Line numbers
are those of the working tree with the follow-up applied. It does not cover the rest of H1 (bare-page discovery, categories,
per-file hashes, premium/free/expired/rate-limit matrix), live Nexus behavior, UI
authorization or F13.

## Source change

`src/updates.rs`

- `api_redirect_policy(base_url)` (line 176) is the only Nexus API redirect policy.
  It follows a hop only when the target's origin (scheme, host, port) equals the
  client's own base origin and at most `NEXUS_API_REDIRECT_LIMIT` (10) hops have been
  followed; otherwise it returns a policy error. reqwest's `Attempt::previous()` also
  holds the original URL, so the check is `previous().len() > 10` (line 184), the
  same comparison as reqwest's own `Policy::limited`. reqwest evaluates the policy
  before it contacts the redirect target, so a refused hop sends no request.
- `NexusClient::new` (line 193) calls `Self::build(api_key, NEXUS_API_BASE)` and
  `NexusClient::with_base_url` (line 201) calls `Self::build(api_key, base_url)`.
  `build` (line 206) has no redirect parameter; it appends a trailing `/` to the base
  path when missing (lines 210-213, so `.../v1` and `.../v1/` both resolve API paths
  under `v1/`; before, `join("./")` dropped `v1`), validates the base (https, or http
  only for a loopback host) and installs `api_redirect_policy(&base_url)`
  (line 227). So both constructors use the same policy builder, each with its own
  base origin. Before this change `new` used `reqwest::redirect::Policy::default()`,
  which follows cross-host redirects and does not strip the custom `apikey` header.
- `nxm_download_link` builds the `download_link.json` URL with
  `Url::query_pairs_mut().append_pair("key", ..).append_pair("expires", ..)`
  (line 304), then writes form `+` as `%20`. A literal `+` is already `%2B`, so
  strict percent-decoders and form decoders both recover the exact key. It is sent
  through the new `get_url_value` (line 409); `get_value(path)` now delegates to it.
- The key must never reach error text. Every reqwest error from the client (`send`,
  `error_for_status`, body read; lines 389-391, 429-431, 449) goes through
  `redact_query` (line 460), which drops the query from the URL reqwest reports and
  keeps the path. A refused or looping redirect previously printed the original
  `download_link.json?key=...` URL. The 429 message (line 423) names only
  `url.path()`.

`src/download_orchestrator.rs`

- `nxm_url` (line 1529) rebuilt the nxm origin URL by string interpolation, so a
  keyed handoff lost reserved characters before reaching `nxm_download_link` on the
  shared `nxm-download --input` path. It now uses `query_pairs_mut` (lines
  1549-1558); `protocol::parse` decodes it back with `query_pairs`. Here a space
  stays form `+` because only `query_pairs` reads it; the API request uses `%20`
  because the Nexus server's decoder is not ours. Comments at both sites say so.

Public APIs are unchanged. The archive transfer client in `src/download.rs` is a
separate client with no API-key parameter; this change does not touch it.

## Default-constructor boundary

`NexusClient::new` targets the production `https://api.nexusmods.com/v1/` base. It
cannot be exercised at runtime in an isolated namespace, and no fixture here claims
to have observed a real Nexus redirect. Its coverage is source-only: the argument
above. Every runtime redirect case below exercises `with_base_url`, whose same-origin
policy already existed before `33e2612`; those cases are regression checks for the
shared `build`/`api_redirect_policy` code that `new` now also uses, not evidence that
distinguishes the commit. Of the `33e2612` changes, only the `nxm_key_*` cases
distinguish pre- from post-commit source. The follow-up's hop count, trailing-slash
base and key redaction are distinguished by `same_origin_loop_request_count`,
`base_without_trailing_slash_*` and `nxm_key_{loop,cross_host}_*`: against
`src/updates.rs` from `33e2612` those 9 checks fail and the other 129 pass.

## Runtime proof

`probe.py` builds nothing. It runs the existing binary (`MODSYNC_BINARY`, else
`$CARGO_TARGET_DIR/debug/modsync`, else `target/debug/modsync`) and writes to
`MODSYNC_PROOF_OUT` (default `target/cloud-proof/nexus-h1a`):

```sh
cargo build --locked --lib --bin modsync
NEXUS_H1A_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/nexus-h1a-proof/probe.py
```

It refuses to run unless it is in a new network namespace with no routes, and unless
the binary's mtime is newer than every hashed source file (`src/**/*.rs`,
`Cargo.toml`, `Cargo.lock`), so a stale binary fails loudly. It
generates a throwaway CA and leaf, then starts recording servers on loopback:

- `api`: synthetic Nexus API, http `127.0.0.1:P`
- `cross-host`: the same API on `127.0.0.2:P` (different host, same port)
- `cross-port`: the same API on `127.0.0.1:P2`
- `archive`: HTTPS ZIP on `127.0.0.1`, trusted only through `--extra-ca-file`
- `https-api`: HTTPS API used only for the boundary case

Every accepted connection records its first byte (`0x16` means a TLS ClientHello)
and every request records its exact request line and all headers. The fixture uses
`fetch-downloads --nexus-api-base ... --nexus-mods-api-key ... --extra-ca-file ...
--peer-mode off --retries 0` with a generic page URL
(`https://www.nexusmods.com/kotor/mods/1?tab=files&file_id=1`), and `nxm-download`
with and without `--input`. The first API path segment selects the scenario.

Zero-count checks run only after the servers are quiescent: no pending connection on
any listening socket, no accepted connection whose handler is still running, and an
unchanged log over 4 consecutive polls (no fixed sleep).

Result on the cloud run: **138/138 checks**, 15 production invocations.

| Case | Observed |
| --- | --- |
| Positive controls | Python reaches and records at `cross-host`, `cross-port` and `api`. A real TLS ClientHello sent to the API port is recorded as `tls=true`. The production binary with `cross-host` or `cross-port` as its own base completes the archive; that server records 3 API requests carrying `apikey: fixture-api-key-7f3c`. So a zero count at those servers means no request was made, not that the server was unreachable. |
| Plain success | Exit 0, SHA-256-verified `example-archive.zip`. API paths are exactly validate, file metadata and download_link, all with the explicit key only. The stored settings key is never sent. |
| Archive requests | Every archive request in every completing case (4 cases, 1 request each) has no `apikey` header and does not contain the key value. |
| Same-origin redirect | Two hops (`?hop=1`, `?hop=2`) on the same origin are followed with the key; the archive completes. |
| Same-origin loop | Fails in about 0.3 s with `Too many Nexus API redirects`. The API records exactly 11 `download_link` requests (the first plus 10 followed hops). Nothing is published and there are no archive requests. |
| Base without trailing slash (`http://127.0.0.1:P/noslash/v1`) | Completes; the API paths are exactly `/noslash/v1/users/validate.json`, `/noslash/v1/games/kotor/mods/1/files/1.json` and `/noslash/v1/games/kotor/mods/1/files/1/download_link.json`. |
| Cross-host (`127.0.0.2`, same port) | `Refusing Nexus API redirect to another origin: http://127.0.0.2:P`. The destination records 0 connections, 0 requests and 0 key occurrences. |
| Cross-port | Same refusal for `http://127.0.0.1:P2`. The destination records 0 connections, 0 requests and 0 key occurrences. |
| Host alias (`localhost`, same IP and port) | Refused. The only `download_link` request on the API socket is the original one, and no request has a `localhost` Host header. |
| Scheme change (`http` to `https`, same host and port) | Refused as `https://127.0.0.1:P`. There are 0 TLS ClientHellos on the API port and a single `download_link` request. |
| nxm key `a+b/c=d&e f%` (direct `nxm-download`) | The API request line has `?key=a%2Bb%2Fc%3Dd%26e%20f%25&expires=4102444800`. Python `parse_qs` and strict `unquote` both decode it to exactly the original key and exactly two parameters. |
| Keyed loop and keyed cross-host refusal (direct `nxm-download`) | Every `download_link` request carries the encoded key (11 for the loop, whose hops keep the query; 1 for cross-host). The command fails with the policy message, the cross-host server records 0 connections, and stdout/stderr contain none of the raw, `%20`-encoded or form-encoded key. The error names the path only. |
| Same key via `nxm-download --input` (`nxm_target`, `nxm_url`, `run`) | Identical request line and decoding, with no premium validation because the handoff is keyed. |

In both nxm cases the command then fails at the archive with
`invalid peer certificate: UnknownIssuer`. `nxm-download` has no per-invocation CA
option, and the proof asserts this failure rather than hiding it. The key's arrival
at the API is the claim; archive completion and archive zero-key are proven through
`fetch-downloads`.

## Open for later H1 slices

- Runtime https to http downgrade (below). It needs a narrowly scoped trust seam: an
  explicit `--nexus-api-base` loopback HTTPS base plus a per-invocation CA for the API
  client, persisting nothing.
- A typed boundary for invalid, missing and expired authorization.
- The premium/free/rate-limit matrix.
- Discovery (bare-page discovery, categories, per-file hashes).

## Not proven at runtime: https to http downgrade

The per-invocation `--extra-ca-file` is scoped to loopback archive transfers
(`download::LoopbackCa`). The Nexus API client uses only built-in webpki roots, so
an HTTPS API base cannot be trusted in the fixture. The `https_api_base_boundary`
case records this: validation fails with `UnknownIssuer`, `https-api` sees the TLS
handshake rejected and no HTTP request. Therefore an https-base redirect to http
cannot be driven through the existing CLI without a new production trust seam, and
none was added. The coverage is:

- source: the refusal compares full origins, and the scheme is part of the origin
  tuple, so `https://h:p` does not equal `http://h:p`;
- runtime: the same comparison refuses the reverse scheme change (http to https on
  the same host and port) with zero destination TLS contact.

Separately, an https base with an http target also changes the default port unless
both ports are explicit. A future, reviewed API-client trust seam for loopback
HTTPS fixture bases would allow a direct downgrade case.

## Receipt contents

`$MODSYNC_PROOF_OUT/receipt.json` holds the binary path, SHA-256 and mtime, the
SHA-256 of every `src/**/*.rs` file plus `Cargo.toml` and `Cargo.lock` (checked
unchanged before and after), `git rev-parse HEAD` (checked unchanged) and
`git status --porcelain -- src Cargo.toml Cargo.lock`, the proof hash, the
namespace and its empty route table, all checks, and the full connection/request
log. Each case also writes `<case>.json` with argv, exit, stdout, stderr, the guide
and observed traffic. These outputs live under the target directory and are not
committed.
