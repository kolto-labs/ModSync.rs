# Isolated loopback HTTPS acquisition proof

This is production CLI evidence for the approved loopback transport slice at
source commit `6c08656`. It does not establish live-host automation, GUI download
integration, persistent background sharing, full F13/F14 parity, or F29.

The additive APIs are `download::LoopbackCa::from_pem_file`,
`download::fetch_to_with_loopback_ca`, and
`download_orchestrator::run_with_loopback_ca`. Existing `fetch`,
`fetch_with_options`, `fetch_to`, `run`, and `RunSettings` remain compatible.
The hidden `fetch-downloads --extra-ca-file PATH` applies only to that invocation's
archive HTTPS client. Initial and redirected URLs must use exactly `localhost`
or a literal loopback IP; every rejected redirect uses a policy error. Localhost
resolves explicitly to IPv4 loopback, and this private client ignores environment
proxies. Ordinary clients retain their prior trust, proxy and diagnostic behavior.
No TLS verification is disabled and no system trust store or settings are changed.

Explicit CA files are read and validated before dry-run returns. This is read-only:
valid-CA dry run creates no transfer/cache state, performs no requests, and does not
save the guide or settings. Unreadable, non-PEM, and PEM-wrapped malformed DER fail
visibly, including in dry run. Reqwest's rustls `from_pem` constructor defers actual
certificate validation; the loader therefore checks that PEM certificates exist
and builds an unused private client to validate their DER before returning.

## Reproduction and boundary

Run from this Rust worktree using the existing debug target:

```bash
python3 docs/parity/origin-loopback-proof/run_gates.py
LOOPBACK_PARENT_NETNS=$(readlink /proc/self/ns/net) unshare -Urnm \
  python3 docs/parity/origin-loopback-proof/run_probe.py
```

`run_gates.py` serializes fmt, both production feature builds and both strict
Clippy invocations. It never compiles or executes Rust tests. `run_probe.py`
checks namespace isolation before starting anything, enables loopback, and
starts the real production `torrent-seed` process before adding a TEST-NET
address. TLS servers, the SOCKS relay, a separate HTTP CONNECT proxy and every
production `fetch-downloads` child inherit that same isolated namespace. There
is no default route. Public proxy/tracker lists are disabled in fixture settings.
The direct seed's default discovery cannot leave the namespace.

The driver imports only fixture utilities from the accepted selected-guide
probe: exact-length SOCKS reads, socket pumping, synthetic guide/archive creation
and real seed startup. It does not call that module's acceptance main, mock any
acquisition adapter, or use a Rust harness. Both drivers, Cargo manifests, all
Rust source files and the production binary are bound by SHA-256 in the final
receipt. The source hash map is also compared with the production gate receipt.
`frozen-source.json` compares 94 unowned Rust/test/config files against accepted
fetch source `2889ec5032c7d4b2b3d4c4d5cee70b381900d536`; all remain unchanged.

Final retained run: **53 cases, 956/956 assertions, no error**. All five
production gates exited zero. `source-binding.json` records the exact binary
and source hashes; the fixture root is removed and all owned children reaped.

## Evidence map

| Requirement | Direct production cases and controls |
| --- | --- |
| CA absent/wrong/correct | `ca_None`, `ca_wrong`, `ca_correct`; absent/wrong assert certificate rejection; correct publication hashes equal the seeded archive |
| Certificate validation | `ca_invalid`, `ca_missing`, `ca_malformed`, and matching `dry_run_*` cases |
| Endpoint scope | Reachable TEST-NET IP and `public.example` / `localhost.evil` HTTP(S) controls precede initial refusals; `downgrade`, `escape`, `deceptive` assert no outside request |
| Redirect eligibility | `redirect`, `chain`, `redirect-localhost`, `redirect-ipv6`, and bounded `loop`; refusal is failed, never a manual gate |
| Localhost pinning | `localhost_pinned` succeeds after the fixture independently confirms system localhost resolution points to TEST-NET |
| IPv6 | `ipv6_loopback` and `redirect-ipv6` publish the same verified bytes |
| Environment proxy | `proxy_control` records the ordinary client's CONNECT; `proxy_override` succeeds with zero CONNECTs under the same environment |
| Host gates | Login form, challenge body, challenge header and countdown in Race and Fallback: exact manual reason, zero peer events and zero SOCKS connections |
| Race | `origin_wins` and `peer_wins`: actual production peer traffic, Origin's classification-clearing transfer event precedes first SOCKS connection, one completed publication, exact hash and no staging leftovers |
| Retry | `retry429` succeeds on the third request; `retry503` stops after three requests with bounded backoff |
| Cancellation | `cancel_body`, `cancel_backoff`, `cancel_race`: exact exit 130, overall `report.cancelled=true`, prompt termination, no published archive, no staging/scratch leftovers |
| Validation | Unknown content-disposition naming succeeds; unsafe/empty names, bad hash, corrupt archive, and expanded-byte budget publish nothing |
| Identity | Every acquisition event preserves component GUID, target index and known filename; NDJSON events precede one summary |
| Secrets and persistence | Fake Nexus API refuses an override without explicit key despite stored/environment keys; three positive API requests use only the explicit key, archive request receives none; settings hashes unchanged and no saved CA |
| Dry run | Valid CA + Race + record flags cause no network, peer work, case changes or HOME/XDG cache changes |

Cancellation during a retry delay preserves existing semantics: the target may
retain `status=failed` for the last HTTP failure, while the overall report has
`cancelled=true` and the process exits 130. Body/race cancellation reports the
target as cancelled. This slice does not rewrite that existing distinction.

Each `receipt/<case>.json` retains exact argv, isolated HOME/XDG paths, explicit
fixture environment changes, synthetic guide and settings inputs, exit code,
stdout/stderr, timestamped events, server requests, peer/CONNECT connections,
and before/after file hash manifests. The main `receipt/receipt.json` retains
all assertions, namespace/address/route facts, fixture archive manifest, source
and binary hashes, and child exit statuses. Its `error` must be null and every
assertion true. Assertion counts can vary with the number of real progress events;
the named case inventory identifies coverage.

The fixture finally reaps only its own children, stops its own servers, unmounts
its private hosts file, and removes its own temporary root. Source and binary
hashes are checked again after the run. The source commit and final retained
receipts are separate local commits; no push or release is performed.

## Earlier attempts

The `preflight-*` folders retain setup failures and are explicitly not acceptance:
TEST-NET was initially added too early and changed the seed's advertised address;
a Python variable shadowed the HTTP module; an early certificate fixture used a
CA as a server leaf; and the replacement CA lacked the key-usage extension required
by Python's strict verification. The final fixture uses a proper CA and signed
server leaf with key usage and SANs, preserving full TLS verification.
`first-complete/receipt.json` is an earlier 553-check successful matrix; the current
`receipt/` supersedes it with broader controls and complete source binding.
