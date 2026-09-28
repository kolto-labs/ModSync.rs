# Selected-guide acquisition: production CLI evidence

Candidate implementation; independent spec and quality review are still required.
F12–F15/F26 remain partial. These receipts cover the bounded fetch CLI slice,
not full acquisition parity, GUI integration, host automation, or release readiness.
The incomplete earlier draft receipt and single empty-ZIP driver are superseded.

Final candidate source: `efefba1420ff7eb167f4de39a17db0acbce77b6e`. Default-feature production binary SHA256:
`f7c7f2ba55ea9a59ab6968416aa86075e3980a6978a820214d7458cec5909bc3`. Final isolated fixture run: **305/305
assertions passed**, exit 0. Five production gates exited 0. `source-binding.json`
checks each changed source hash against that commit; before/after fixture hashes
match. Assertion counts include individual event identity checks, not independent
feature coverage estimates.

## What changed

`fetch-downloads` now selects and acquires resources through the shared library,
resolves effective persisted peer settings before planning, emits acquisition NDJSON
and a final summary, and optionally records verified resource facts. A separate
execution guide holds transient selectors. The new preserve-selection loader also
preserves all-false authored flags through fenced structured Markdown; existing
load/parse callers retain their established select-all default.

The concrete Peer adapter validates authored metadata and names before lazily
constructing its per-run policy. It uses the caller's archive limits and scratch
path, honors known-file versus unknown-directory destinations, and returns typed
cancellation. Orchestrator callbacks rebind event identities, Nexus validation
errors are terminal, and Race waits for Origin eligibility before claiming Peer
startup. Origin completion revokes an unclaimed startup grant.

## Reproduction and source binding

Run from this worktree:

```sh
python3 docs/parity/fetch-downloads/run_gates.py
FETCH_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn \
  python3 docs/parity/fetch-downloads/run_acceptance.py
```

`gates/receipt.json` records exact argv, real exit status, elapsed time, source
hashes and corresponding raw build/Clippy logs. Checks are production `--lib --bin
modsync` only, with two jobs and the existing debug target directory. No Rust test
or temporary Rust harness was created, modified, compiled, or run.

`acceptance/receipt.json` records before/after binary and source hashes, namespace,
empty route table, individual assertion results, observed SOCKS connections, fake
API requests, and owned child exit codes. Every scenario JSON retains its exact
argv, stdout, stderr, exit, parsed report/events and filesystem snapshots. Seeder
argv, full stdout and stderr are retained separately. `frozen-source.json` verifies
53 complete files or inline-test suffixes against `9ecf8e4`, including manifests,
AGENTS.md and every existing test file.

## Runtime scope

The fixture uses a real production `torrent-seed` process serving a synthetic ZIP
with a one-MiB payload and an authored info dictionary. Seeder, relay, fake API and
all fetch children share one fresh user/network namespace. The driver checks that
its namespace differs from the caller and has no external route. Public proxy and
tracker lists are disabled in transfer settings. Only fixture-owned child PIDs
are stopped. No user archive, game tree or baseline is used.

Measured cases:

- Known filename, unknown filename from TorrentInfo, and unknown filename from
  magnet discovery all publish the expected archive bytes and hash through SOCKS.
- Two distinct targets complete through one configured relay; event indices,
  component GUIDs and filenames identify the correct target. Relay observations
  show endpoint reuse. They do not independently prove internal cache allocation
  counts; policy's once-only initialization is established by source inspection.
- Fake Nexus free-user and server-error validation in both Race and Fallback
  makes exactly the validation request, emits the required terminal outcome,
  and never contacts the relay or emits Peer activity. Failed validation emits
  `will_fallback: false`.
- Empty, non-string, null, malformed-base64 and mismatched-hash TorrentInfo refuse
  acquisition before any relay request. Unsafe filenames publish nothing.
- A one-byte expanded budget and a wrong expected SHA256 reject real transferred
  bytes and publish nothing. Because the unsupported-host source order ends in
  Manual, these cases return ManualRequired/exit 3 with the Peer source failure
  retained; they never return successful acquisition.
- SIGINT during a throttled real Peer transfer returns 130, joins its workers and
  leaves no published archive, private staging or scratch extraction behind.
- Dry-run default/file/component/option/category/search selection, disabled file
  facts, persisted modes, sharing=false, explicit overrides, malformed settings
  and explicit-key bypass are exercised. Case trees and isolated HOME/XDG trees
  remain unchanged, with no observed relay/API traffic. No screen resolution is
  supplied by this CLI: authored resolution variants remain available. Explicit
  resolution filtering still belongs to the shared planner's existing API.
- `--record` preserves authored false and excluded true selection values. Existing
  valid archives skip transfer. A real synthetic FOMOD archive returns a manual
  followup (exit 3), without installing it or changing authored selection.
- NDJSON consists of `type: acquisition` event envelopes and one final single-line
  `type: summary` envelope; ordinary output is parsed as one JSON summary.

## Explicitly unrun and incomplete

Origin HTTPS login/CAPTCHA/body gating, eligible Origin/Peer races, Origin retry,
body cancellation and HTTPS redirect confinement still require the separately
approved loopback CA fixture slice. The Nexus fake API checks do not stand in for
those transfer cases. O2 is source-reviewed here pending those runtime fixtures.

Persistent keep-sharing, GUI lifecycle wiring, advanced FOMOD interaction and
full host download automation remain unfinished. There is no full-build K1/K2
or live-game acceptance in this slice. The fixture's network isolation prevents
external communication; relay logs alone only report traffic through that relay.
