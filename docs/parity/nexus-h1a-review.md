# Nexus H1a independent reviews

Source `33e2612` (on `c02a060`). Read-only independent reviewers; no cargo, no tests.

## SPEC: APPROVED

All H1a contract points met: both constructors reach one `build` that installs
`api_redirect_policy` from their own joined base origin; 10-request bound;
production HTTPS preserved; loopback HTTP only for an explicit base; cross-origin
and scheme change refused inside reqwest's policy hook before the destination
request; the `apikey` header is set only in `NexusClient::request` and the archive
client has no key; nxm key/expiry use query construction; public APIs and call
sites unchanged. The orchestrator `nxm_url` fix is H1-owned and proved by the
`nxm_key_orchestrated` round trip.

Ruling: the missing runtime https-to-http downgrade case is acceptable for H1a.
One refusal branch compares the full origin tuple including scheme; the reverse
scheme change was refused at runtime with zero TLS contact; adding API-client CA
trust would be a new production trust surface outside H1a. Carried forward as an
open item. Also open for later H1 slices: typed invalid/missing/expired
authorization boundary. README "uncommitted" wording and line refs were stale.

## QUALITY: APPROVED (with should-fix follow-ups)

Verified against reqwest 0.13.5 / url 2.5.8 sources: origin comparison
normalization (case, IDNA, IPv4 forms, IPv6 compression, default ports) is sound
and trailing-dot/alias hosts are refused (safe direction); `+`-to-`%20` rewrite
only touches the query holding the appended pairs and matches C#
`Uri.EscapeDataString`; `nxm_url` form encoding round-trips through
`protocol::parse`; no new unwrap on untrusted input.

| # | Severity | Finding |
| --- | --- | --- |
| 1 | should-fix | `previous()` includes the first URL, so `>= 10` follows 9 hops while the doc says 10. |
| 2 | should-fix | Probe does not bind the binary to the hashed source; redirect cases use `with_base_url`, whose policy pre-existed, so a stale binary would pass them. |
| 3 | should-fix | Redirect/429 error text includes the full URL carrying the nxm key. |
| 4-7 | nit | Probe fixed-sleep zero-count window; undocumented `destination=None`; confusing encoding comment; missing cross-reference between the two encoders. |
| - | pre-existing | `join("./")` drops the last segment of a base without a trailing slash. |

All follow-ups dispatched to a fresh implementer; re-review follows.

## Re-review of `5cbd19c`: QUALITY APPROVED

Verified against reqwest 0.12.28 (the pinned major): `previous()` includes the
origin URL, so `> 10` follows exactly 10 hops (11 requests); every reqwest error
site is redacted, including redirect-policy errors, whose only URL is the keyed
one, and no anyhow context re-inserts the key; trailing-slash normalization handles
query/fragment/root bases; `quiesce()` cannot deadlock or pass vacuously; mtime
binding errs toward refusal. One docs nit (stale "uncommitted" provenance) fixed by
the controller after a clean-tree re-run: 138/138 at `5cbd19c`, empty porcelain.

Nexus H1a is accepted: SPEC and QUALITY approved. It does not promote H1, F13 or
live Nexus behavior. Open for later H1 slices: runtime https-to-http downgrade
(needs a narrowly scoped trust seam), typed invalid/missing/expired authorization,
premium/free/rate-limit matrix, bare-page discovery and per-file hashes.
