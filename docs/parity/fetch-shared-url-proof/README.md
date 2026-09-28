# Fetch: one transfer for components that share a URL and file

Base `728fa2f`. The source change is uncommitted in `src/acquisition.rs` and
`src/download_orchestrator.rs`. The receipt binds it by hash. It has had no
independent review yet. It fixes the pre-existing defect recorded in the
[final S2a quality check](../fomod-s2a-review.md#final-quality-check-of-ae8d7e9-approved).

## (a) Defect at `728fa2f`

`plan` skipped duplicate component indices only. `plan_url` pushed one
`ArchiveTarget` per component, so two components with the same registry URL and
`Files` name gave two targets with one destination. `run` queued both:

- `--workers 4`: both requested the file. The later publish failed in
  `persist_noclobber` with `File exists (os error 17)`.
- `--workers 1`: the second target failed before any request with `Download
  destination already exists`.

Either way the loser was `failed`. `record` and `collect_fomod_consumers` read only
`completed` outcomes, so the loser was not recorded or bound as a FOMOD consumer,
and Fetch exited 1. Two different URLs naming one file raced the same way, and the
race decided which URL's bytes stayed. A Peer publish (`PeerStaging::publish`) had
the same no-clobber collision.

## (b) Change

- `src/acquisition.rs:103-118`. `ArchiveTarget` gains
  `shared_destination: Option<SharedDestination>`, which is not serialized when
  `None`. `SharedDestination` is `SameUrl(owner)` or `DifferentUrl(owner)`, and
  serializes as `{"kind":"sameUrl"|"differentUrl","targetIndex":owner}`.
- `src/download_orchestrator.rs:294,298-323`. `plan` finishes with
  `link_shared_destinations`. In guide (plan) order, the first target with a
  destination owns it. A later target matches when both known names are equal
  ignoring ASCII case, or when both names are unknown and the registry URL is the
  same. It becomes `SameUrl` when its registry URL equals the owner's and
  `DifferentUrl` otherwise. A known name never matches an unknown one.
- `src/download_orchestrator.rs:486-489,663`. `TargetOutcome` gains
  `shared_with: Option<usize>`, which is not serialized when `None`.
- `src/download_orchestrator.rs:541-550`. `run_with_loopback_ca` refuses a
  hand-built plan whose shared target does not name an earlier target that
  transfers.
- `src/download_orchestrator.rs:603-627`. `SameUrl` targets are never queued. The
  worker that finishes an owner calls `shared_outcomes` and returns the owner's
  outcome together with its sharers'. The results are sorted by `targetIndex`, so
  `report.outcomes` is still one per target in plan order.
- `src/download_orchestrator.rs:667-732`. `shared_outcomes` copies the owner's
  outcome to each sharer. The sharer keeps its own index, component GUID and
  registry URL, and gets `sharedWith`. On success it takes the published file name;
  otherwise it keeps its planned name. A sharer's own authored `ContentHashSHA256`
  is compared with the shared bytes, and a mismatch makes that sharer `failed`.
  Each sharer emits one event with the owner's final phase (`completed`,
  `manualRequired`, `cancelled` or `sourceFailed`), sent right after the owner's
  events.
- `src/download_orchestrator.rs:841-865`. A `DifferentUrl` target fails at once,
  with no request. Its one `sourceFailed` event has `will_fallback: false` and the
  error `Download destination <name> is also planned from <owner URL> for component
  <owner GUID>; one file cannot come from two URLs, so <URL> was not requested`.
- `src/download_orchestrator.rs:431,1750`. The two other `ArchiveTarget`
  constructors (`plan_url`, `nxm_target`) set `None`.

`record_with`, `collect_fomod_consumers`, `fomod_cli::dispatch` and `main.rs` are
unchanged. Every owner now has a `completed` outcome with the same path, so
recording writes each owner's own registry entry. Binding already merges consumers
by archive path, so one FOMOD session covers all of them.

JSON stays backward compatible. Plan and report rows keep every key they had. Rows
without a shared destination are byte-identical, and sharing rows add only
`sharedDestination` or `sharedWith`. Event shape is unchanged.

## (c) C# reference comparison

- **C# does not deduplicate URLs across components.** GUI `DownloadOrchestrationService`
  (`ProcessComponentDownloadAsync`, `Task.WhenAll` over selected components, or a
  sequential loop) and CLI `ModBuildConverter` (`SemaphoreSlim(10)`, lines
  1264-1305) each call `DownloadCacheService.ResolveOrDownloadAsync` per component.
- **Existing files.** That method checks for an existing file by name
  (`CheckCacheAndFileExistenceAsync`). A file already present counts as that
  component's cache entry.
- **Publish overwrites.** A fresh download goes to a random `.tmp`. Then
  `DownloadHelper.MoveToFinalDestination` deletes or replaces any existing final
  file.
- **Sequential outcome:** the second component finds the first one's file.
  **Concurrent outcome:** both download, the last move wins, and both succeed. In
  both, every owning component records the file name.

Rust follows the observable result: every owning component gets the archive and
records it, and there is no failure. It deviates on purpose in these ways:

1. **One request.** Rust never makes a second request and never overwrites a
   published archive (no-clobber publish).
2. **Different URLs fail explicitly.** In C#, a later component either overwrites
   the file or quietly treats another URL's file as its own. Rust never accepts one
   URL's bytes as another URL's archive.
3. **The owner's hash governs the one transfer.** A sharer's own hash is also
   checked. If the owner's authored hash rejects the bytes, every sharer fails with
   it. Before, a later sharer without a hash could download again and accept them.
4. **Case-insensitive names.** Known names compare ignoring ASCII case, like C#'s
   `OrdinalIgnoreCase` `Files` map, Rust `find_existing` and engine source matching.
   On a case-sensitive filesystem, `Example.zip` and `example.zip` from one URL are
   now one file instead of two copies.

Out of scope: two unknown names from different URLs that turn out equal still
collide at publish. So do a known name and an unknown name that the transfer
reveals to be the same. The desktop download path (`ui.rs`, `download::fetch`)
does not use this orchestrator and is unchanged.

## (d) Controlled production proof

`probe.py` follows the S2a probe conventions:

- **Isolation guard.** `FETCH_PARENT_NETNS` must differ from the current namespace,
  and the namespace must have no routes.
- **Origin.** A loopback HTTPS origin whose leaf certificate is signed by a
  throwaway CA, passed with `--extra-ca-file`. It records each request (path,
  start, end, thread) and holds each archive response for 0.4 s, so two concurrent
  requests overlap as they did before the fix.
- **Fixtures.** Private HOME/XDG/settings/mods/scratch directories and generic
  archives. The FOMOD archive comes from the S2a `package()` generator.
- **Verb.** Only the real `fetch-downloads`, with `--peer-mode off --retries 0`.
- **Receipts.** Output goes to `target/cloud-proof/fetch-shared-url`. Each run keeps
  its argv, exit, stdout, stderr, parsed summary, events, per-run requests,
  before/after trees and fixture copies. `receipt.json` binds git HEAD, porcelain,
  source hashes, binary SHA-256 and mtime. The run fails if the binary is older
  than the sources, unless `FETCH_SHARED_ALLOW_UNBOUND=1` (used only for pre-fix
  binaries). It also checks that the binary and sources are unchanged after the
  run.
- **Cleanup.** The fixture root is removed at exit unless `MODSYNC_PROOF_KEEP=1`.

No archive is placed in the mods folder beforehand in any case.

| Case | Checks |
| --- | --- |
| `control`: one component, FOMOD archive, workers 4, `--record --fomod-choices` | Plan and outcome keys exactly as before. Exit 0, one request, `completed`, recorded, one `configured` consumer, choices saved. |
| `shared_w4`, `shared_w1`: two components, one URL and name, FOMOD archive, workers 4 and 1 | Dry-run plan: second target `sameUrl` 0, keys additive, no request, no writes. Fetch: exit 0, **one** request. Outcomes `(0, A, completed)` and `(1, B, completed, sharedWith 0)`, the same path and SHA-256, keys additive only. Both recorded and saved `Files` true. Both are FOMOD consumers (`configured`) in one session `[A, B]`, and both have choices saved. Last event per index is `completed`, and the sharer has exactly one event. The mods folder holds only the archive. |
| `different_urls`: `/a/example.zip` and `/b/example.zip`, both named `example.zip`, workers 4 | Plan `differentUrl` 0. Exit 1. Requests: `/a` 1, `/b` 0. Outcomes `completed`, `failed` with the exact explicit error (no `already exists` or `File exists`), and one `sourceFailed` event with `will_fallback: false`. Only A is recorded (B's `Files` flag stays `null`), and the file holds A's bytes. |
| `hash_follower`: the sharer authors a wrong `ContentHashSHA256` | Exit 1, one request, the sharer `failed` (`sharedWith` 0) with the shared-download mismatch error. Only the owner is recorded. |
| `interleaved`: A(X), B(Y), C(X), workers 4 | Plan: only C is `sameUrl` 0. Exit 0, one request each for X and Y. Outcomes in guide order 0, 1, 2 with C `sharedWith` 0. All recorded in order, and the terminal events are ordered. |
| `distinct_names` (control): one URL, names `example.zip` and `copy.zip` | No `sharedDestination`, two requests, both `completed`, two files, each recorded under its own name. |
| `case_variant`: one URL, names `Example.zip` and `example.zip` | `sameUrl` 0, one request, both `completed`, one file `Example.zip`. Each component records its own key. |

The seeded `fetch-downloads` `shared_pool` case (one URL, two names, over Peer)
cannot run here (section (e)). `distinct_names` covers the same no-over-merge rule
over Origin.

## (e) Results (cloud Linux)

- **Post-fix.** `probe.py` passes **106/106 checks** in 15 production invocations,
  exit 0. The default-feature debug binary SHA-256 is
  `4e63a247160452caa187bdfc80a467f473ad187fb3de8f367c6a3adf6b05e70b`
  (host-specific), built after the last source edit. Git HEAD is `728fa2f` plus
  the uncommitted source above, bound by hash.
- **Pre-fix.** The same probe ran against a `--no-default-features` binary built
  from a `git archive` of `728fa2f` in its own target directory (SHA-256
  `a836781d6bb988673504ac7170ace92d7d3c7764c3fcd7468bc0d25bdd890789`, output
  `target/cloud-proof/fetch-shared-url-pre728fa2f`). It passes **63/106; 43 checks
  fail**:
  - `shared_w4`: 13 checks (plan, exit, requests, outcomes, keys, same file,
    record, consumers, session, choices, events). There were 2 requests, and the
    loser got `File exists (os error 17)`.
  - `shared_w1`: the same checks except the request count, which was already 1: 12
    checks. The loser got `Download destination already exists`.
  - `different_urls`: plan, requests (1/1), explicit error, not-a-race and event: 5
    checks.
  - `hash_follower`: one request, outcomes (`sharedWith`) and error: 3 checks.
  - `interleaved`: plan, exit, requests (2/1), outcomes, recorded and events: 6
    checks.
  - `case_variant`: plan, one request, outcomes and one file: 4 checks. Before,
    it made two requests and wrote two files.

  The `control` and `distinct_names` cases pass at both commits. Which request wins
  the pre-fix `different_urls` race varies: in an earlier pre-fix run B won, and
  its outcome, record, flag and bytes checks also failed.
- **Regressions**, each in its own `unshare -Urn` namespace with the same binary:
  - S2a `probe.py`: 224/224.
  - S1 `probe.py`: 339/339 (61 cases).
  - `inherited_controller.py`: 41/41.
  - Nexus H1a `probe.py`: 138/138.
- **`docs/parity/fetch-downloads`** has no portable entry point. `run_acceptance.py`
  and `run_gates.py` hard-code `/home/brunner56/.cache/modsync-rs-target-lead`, and
  they write their receipts into the tracked `docs/parity/fetch-downloads/` tree.
  A scratch copy with only `REPO`, `BIN` and `OUT` rewired was tried, writing to
  `target/cloud-proof/fetch-downloads-acceptance`. It stops at `torrent-seed`
  before any Fetch case: this cloud kernel has no IPv6 (`/proc/sys/net/ipv6` is
  absent), and the seeder's UDP tracker socket on `[::]:0` fails with `Address
  family not supported by protocol (os error 97)`. Only its two run-invariant
  checks were produced. It is not counted as evidence.
- **Gates**, all exit 0, run serially with the default build last:
  - `cargo fmt --all -- --check`;
  - `cargo build`, and `cargo clippy -D warnings`, with `--locked --lib --bin
    modsync -j2`, with and without `--no-default-features`;
  - `x86_64-pc-windows-gnu` Clippy `-D warnings` in both feature modes.

  No Rust test or harness was created, modified, compiled or run, and
  `--all-targets` was not used.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j2
FETCH_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fetch-shared-url-proof/probe.py
```

For a pre-fix comparison, build a `git archive` of `728fa2f` with its own
`CARGO_TARGET_DIR`, then run the probe with `MODSYNC_BINARY=<that binary>`,
`MODSYNC_PROOF_OUT=target/cloud-proof/fetch-shared-url-pre728fa2f` and
`FETCH_SHARED_ALLOW_UNBOUND=1`. That run is expected to fail.
