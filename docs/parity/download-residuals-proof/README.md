# Download residuals: known-name collisions, Nexus-revealed names, cancellation, totals

This proof covers four of the five acquisition follow-ups in the
[remaining-work tracker](../../plans/2026-09-28-1130-remaining-work-tracker-plan.md)
("Known acquisition and FOMOD follow-ups"). Their source changes were already on `main`
(`9548a73`). The checkboxes stayed open because no runtime acceptance existed. This
proof drives the production `modsync fetch-downloads` CLI. It found and fixed two
runtime gaps. The contract is in [download-residuals-contract.md](../download-residuals-contract.md).

| Tracker item | Before this change (`9548a73`) | Now |
| --- | --- | --- |
| Origin-present race adoption; a refusal can still fall back to Peer | Source done (`race_origin_peer`: adopt before stopping Peer; a refusal sets `origin_failed` and Peer keeps running) | Unchanged. **Still open for runtime acceptance**: it needs a Peer source, and this kernel cannot seed one (no IPv6, see the [shared-URL proof](../fetch-shared-url-proof/README.md) section (e)). |
| Existing-file hash/adoption cancellation is responsive | Source done (`sha256_file_with_cancel`, cancel-aware `archive::verify`) | Accepted: SIGINT during the planning hash of a 512 MiB file stops within about 3 ms. Ctrl+C during planning now exits **130**, as it does during the run (it exited 1 before). |
| Unnamed Nexus target whose API-revealed filename already exists | Source done (no pre-transfer check for a `Directory` name hint) | Accepted: the target transfers, then adopts the present file (same bytes) or refuses it without replacing it. |
| Concurrent known-name publication outside a serialized group | A lost no-clobber move was typed. But a known name taken **before** its transfer started failed with no request and the untyped `Download destination already exists` | **Fixed**: Origin publishes a known name through the same `Destination::Directory { name }` path as a revealed name. MEGA drops its pre-transfer `exists` check (source only). Both the "taken before" and "lost the move" cases are accepted. |
| Report actual Origin download totals where available | `total` was read **after** the 64 KiB sniff prefix, so it was the remaining length: `0` for any archive at or under 64 KiB, and size minus 64 KiB above that | **Fixed**: `total` is read before the prefix. It is the response's Content-Length, or `null` when the server sends none (chunked). |

## Change

- `src/download_orchestrator.rs` `origin_once`: the destination is always
  `Destination::Directory { directory: source_dir, name: target.filename or the provider
  name hint }`. A known name that is present after the transfer, or that loses the
  no-clobber move, returns `DestinationExists { path, sha256 }`. `adopt_present` then
  decides, as it does for a revealed name. The authored name is still validated before
  any request (`target.destination`).
- `src/download_hosts/mega/transfer.rs`: a MEGA `Directory` name is validated but no
  longer refused before the transfer. `publish_staged` settles it after the transfer.
- `src/download.rs` `fetch_inner`: `let total = response.content_length()` moved before
  the sniff-prefix read.
- `src/main.rs` `fetch-downloads`: `plan_with_cancel` passes through
  `finish_acquisition_result`, so a planning error after a signal exits 130.

`download::fetch` / `fetch_with_options` (`Destination::File`) and the standalone
adapters keep their pre-transfer refusal. Only the orchestrator changed.

Review follow-ups (see the table above):

- `src/download_orchestrator.rs` `origin_once`: after the name is validated, a name
  with an authored hash that is already present returns `DestinationExists` with the
  authored hash before any transfer. `PeerStaging::publish` types a present or lost
  name for every target, not only an unnamed one.
- `src/peer_acquire.rs`: any present name (not only a torrent-revealed one) returns
  `DestinationExists` before the transfer, and a lost no-clobber move (`AlreadyExists`
  with the name now present) returns it after.
- `src/main.rs` `fetch-downloads`: `discover_with_loopback_ca` passes through
  `finish_acquisition_result`.

## Review follow-ups (after `59e4e57`)

The independent review of this proof left three Low follow-ups. They are fixed here:

| Follow-up | At `59e4e57` | Now |
| --- | --- | --- |
| 1. Wasted transfer for a hashed target whose name is already present | The target transferred, then `adopt_present` decided from the present file alone (the hash is authored, so the transferred bytes could not change the outcome) | `origin_once` settles it before any request: a name (authored or provider-revealed) with an authored `ContentHashSHA256` that is already present returns `DestinationExists { path, sha256: the authored hash }`, so `adopt_present` adopts on a match or refuses without replacing. A name that appears only during the transfer still goes through the no-clobber publish. Runtime accepted (cases 7a-7d). |
| 2. Peer known-name collision | A Peer copy for a `Files` name that lost to an ungrouped publisher failed untyped: "Could not publish the peer copy ... File exists" in a race (`PeerStaging::publish`), "Download destination already exists" when Peer ran alone (`p2p` precheck) | `PeerStaging::publish` returns `DestinationExists` for any present or lost name, not only an unnamed one. `PeerAcquire` returns it before the transfer for any present name, and after a lost no-clobber move. The race and `acquire_sources` then adopt or refuse as for Origin. **Source only**: Peer cannot run on this kernel (no IPv6, see the [shared-URL proof](../fetch-shared-url-proof/README.md) section (e)). |
| 3. Ctrl+C during execution-time discovery | `discover_with_loopback_ca(...)?` was not wrapped, so a signal during discovery exited 1 ("Acquisition cancelled") | `src/main.rs` passes it through `finish_acquisition_result`, as planning: exit 130 only when the signal flag is set; other errors pass through unchanged. Runtime accepted (case 8). |

One consequence for case 6: its control (a present 512 MiB `big.zip` that does not match
its authored hash) is now refused with no request, so the 404 origin is never asked. The
control's checks became `hash_cancel_control_no_request` (was `_one_request`) and
`hash_cancel_control_refused` (new, the typed refusal); its hash duration is the whole
run. That run hashes the file twice (planning, then `adopt_present`), about 22 s in
debug, where the old control hashed once and then failed on the 404.

## Cases (`probe.py`, 183 checks, 22 invocations)

| Case | Checks |
| --- | --- |
| `known_after_unnamed`: A (no `Files`, U1) and B (`example.zip`, U2), same bytes, no hash, `--workers 1` | Plan: two unlinked targets, A unnamed. Exit 0. One request each, U2 at least 0.4 s after U1. Both `completed`, no `sharedWith`, no error, both `example.zip` with A's SHA-256. Terminal events `completed`. Both recorded under their own URLs. One file, no temp file left. |
| `known_after_unnamed_mismatch`: the same order, U2 serves other bytes | Exit 1. One request each. B `failed` with "differs from the downloaded bytes ... no ContentHashSHA256 ... was not replaced", not `Download destination already exists`. Only A is recorded. The file holds A's bytes. |
| `known_loses_publish`: A (no `Files`, U1) and B (`example.zip`, U2), both serving the slow 1 GiB-expanded archive, U2 answering 1 s later, `--workers 4` | Exit 0. The requests overlap. The event stream orders B `verifying` before A `completed` before B `completed`. So B found its name absent, A published while B verified, and B's no-clobber move lost. Both `completed`, no error, the same file and SHA-256, both recorded, one file. |
| `nexus_present_same`: Nexus page `?file_id=1`, no `Files`, no hash; the API reveals `example-archive.zip`, already present with the same bytes | Plan: one unnamed target. Exit 0. One `download_link` call, one CDN transfer. `completed` with no error, `example-archive.zip` and its SHA-256. Recorded, and the saved `Files` becomes `{"example-archive.zip": true}`. The present file keeps its inode, mtime and size. |
| `nexus_present_other`: the same, with other bytes present | Exit 1. One transfer. `failed` with "differs from the downloaded bytes ... was not replaced". Terminal `sourceFailed`, nothing recorded, the file is untouched. |
| `total_known` / `total_unknown`: one named archive, with Content-Length / chunked | Exit 0, `completed`. Every `transferring` event's `total` is the body length (131) or `null`, and the last `bytes` is 131. `total_nexus_cdn`: the Nexus CDN transfer reports its length. |
| `hash_cancel`: a 512 MiB present `big.zip` under its `Files` name, with an authored hash it does not match; the URL is 404 | Control without a signal: exit 1, no request (since follow-up 1), the typed refusal, after at least 3 s of hashing (about 22 s in debug). SIGINT 1 s in: exit **130** within 1 s of the signal (about 3 ms), no request, no acquisition event, the file untouched, the guide unchanged. |

| `hashed_after_unnamed` (7a): case 1, with B authoring the hash of A's bytes | Plan: two unlinked targets. Exit 0. U1 requested once, **U2 never**, no `transferring` event for B. Both `completed` with A's file and SHA-256, both recorded, one file. |
| `hashed_after_unnamed_mismatch` (7b): the same, with B's hash of other bytes | Exit 1. **U2 never requested**, no B `transferring`. B `failed` with "SHA-256 mismatch: the present file ... was not replaced", terminal `sourceFailed`. Only A recorded, the file holds A's bytes. |
| `hashed_present_mismatch` (7c): one target, `example.zip` present with other bytes than its authored hash | Plan: one named target. Exit 1, **no request**, no `transferring`. The typed refusal, not `Download destination already exists`. Not recorded, the file keeps its inode, mtime and size. |
| `hashed_loses_publish` (7d): case 3, with B authoring the slow archive's hash | B's name is absent when its transfer starts. Exit 0, both requested, overlapping. B `verifying` before A `completed` before B `completed`: B lost the no-clobber move and adopted A's file. Both recorded, one file. |
| `discover_control` (8): a bare Nexus mod page (`mods/2`, no `file_id`, no `Files`) whose `files.json` answers 3 s late and lists one MAIN `example-archive.zip` | Plan: one unnamed target. Exit 0, one `files.json`, one `download_link`, `completed` as `example-archive.zip`. |
| `discover_cancel` (8): the same, SIGINT 1 s in | The `files.json` request started before the signal. Exit **130** (about 15 ms after the signal). No `download_link` or origin request, no acquisition event, nothing written, the guide unchanged. |
| `discover_error` (8): the same, no signal, with a non-UTF-8 file name in the mods folder | Exit 1. `files.json` was listed, then the page's filename selection refused its inventory ("non-UTF-8" on stderr): a discovery error without a signal still exits 1 with its own message. No acquisition event. |

Every invocation also checks that the scratch directory and the mods folder hold no
leftover temp files. The probe binds the binary and the source hashes, as the
shared-URL proof does.

## Results (cloud Linux, `unshare -Urn`)

### With the review follow-ups (183 checks)

- **Now**: **183/183**, exit 0, 22 invocations. Default-feature debug binary SHA-256
  `5f5eb59b5a08ff19717cd93dc20720c4ff01522304eb80bea3361e68f4079209` (host-specific),
  built after the last source edit. Discovery cancel latency about 13-15 ms.
- **Pre-fix at `59e4e57`**: the default-feature debug binary of that commit (SHA-256
  `7230cfab...ae490`, the post-fix binary below), `RESIDUALS_ALLOW_UNBOUND=1`.
  **174/183; 9 fail**:
  - `hashed_after_unnamed`, `hashed_after_unnamed_mismatch`, `hashed_present_mismatch`:
    U2 (or the one URL) was requested and transferred before the same outcome
    (requests and no-transfer checks, 6 checks).
  - `hash_cancel` control: one request to the 404 URL, which then failed with the 404,
    not the typed refusal (2 checks).
  - `discover_cancel`: exit 1, not 130 (1 check).

  `hashed_loses_publish`, `discover_control` and `discover_error` pass at `59e4e57`:
  the race-safe publish and the error path are unchanged. The Peer follow-up has no
  runtime check (no Peer on this kernel).
- **Regressions** with the new binary, each in its own `unshare -Urn` namespace:
  fetch-shared-url `probe.py` **382/382**, Nexus H1a `probe.py` **138/138**.

### Original change (102 checks, before the follow-ups)

- **Post-fix**: **102/102**, exit 0. Default-feature debug binary SHA-256
  `7230cfabb4bcb1009be558724f69fc3b9f915964cced222538fa9a29ac6ae490` (host-specific),
  built after the last source edit.
- **Pre-fix at `9548a73`**: a `git archive` built with `cargo +1.95.0 build --locked
  --no-default-features --lib --bin modsync` in its own target directory (SHA-256
  `dc86cfede319d2b8dd8d3b95214a7188726701d660f13e0a0a65eb85cadfbd80`,
  `RESIDUALS_ALLOW_UNBOUND=1`). **86/102; 16 fail**:
  - `known_after_unnamed`: exit, requests (1, 0), serialized, outcomes, errors, same
    file, terminal and recorded: 8 checks. B failed before any request with
    `Download destination already exists`.
  - `known_after_unnamed_mismatch`: requests and the typed error: 2 checks.
  - `total_known`, `total_unknown`, `total_nexus_cdn`: `total` was `0` (3 checks).
  - `hash_cancel`: the control made no request (the present invalid file failed
    untyped before any request), so its hash-duration check fails too (2 checks). The
    cancelled run exited 1 (1 check). The cancellation itself was already prompt
    (about 4 ms), as the source change claimed.

  `known_loses_publish` and both Nexus cases pass at `9548a73`: those source changes
  were already effective.
- **Regressions** with the post-fix binary, each in its own `unshare -Urn` namespace:
  fetch-shared-url `probe.py` **382/382**, Nexus H1a `probe.py` **138/138**.

No Rust test or harness was created, modified, compiled or run.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j2
RESIDUALS_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/download-residuals-proof/probe.py
```

For the pre-fix comparison, build a `git archive` of `9548a73` with its own
`CARGO_TARGET_DIR` (on a toolchain that accepts `if let` guards, 1.95 here). Then run
with `MODSYNC_BINARY=<that binary>`, `MODSYNC_PROOF_OUT=target/cloud-proof/download-residuals-pre9548a73`
and `RESIDUALS_ALLOW_UNBOUND=1`. That run is expected to fail.

For the follow-ups' pre-fix comparison, build `59e4e57` the same way (or keep its
binary from before the follow-up edits) and run with `MODSYNC_BINARY=<that binary>`,
`MODSYNC_PROOF_OUT=target/cloud-proof/download-residuals-pre59e4e57` and
`RESIDUALS_ALLOW_UNBOUND=1`. That run is expected to fail 9 checks.
