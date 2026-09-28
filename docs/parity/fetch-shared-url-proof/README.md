# Fetch: one transfer for components that share a URL and file

Source: commit `4bf5c54` (PR bodencrouch/ModSync.rs#2), its review follow-up commit
`339b3d6`, and a second follow-up on HEAD `339b3d6` (section (b)). The receipt binds
git HEAD, porcelain and source hashes. `4bf5c54` fixed the pre-existing defect
recorded in the
[final S2a quality check](../fomod-s2a-review.md#final-quality-check-of-ae8d7e9-approved).
An independent review of `4bf5c54` required changes, which `339b3d6` makes. A
re-review of `339b3d6` found that its grouping was not transitive and that failure
inheritance ignored which name a member asked for; the second follow-up fixes both.

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
race decided which URL's bytes stayed. So did a component with a `Files` name and
one without, for the same URL. A Peer publish (`PeerStaging::publish`) had the same
no-clobber collision.

## (b) Change

`4bf5c54` linked a later target to an earlier one that owns its destination. A
same-URL follower was never queued and received the owner's outcome. A
different-URL follower failed at once, with no request. The review found three
problems:

- A different-URL follower failed on every run where the owner did not complete,
  although before `4bf5c54` it could download from its own URL.
- The failure contradicted a rerun, which accepts the owner's file by name
  (`AlreadyPresent`).
- A target with a `Files` name and one without, for the same URL, still raced, and
  `run` checked only the structure of a hand-built plan's links.

`339b3d6` replaced the explicit failure with serialized groups. The re-review of
`339b3d6` found:

- **Grouping was not transitive.** `shared_destinations` joined a target to the
  group of its *first* earlier match only and never merged groups. With B (no
  `Files`, URL U2), A (`x.zip`, URL U1) and C (`x.zip`, URL U2), C joined B's group,
  so A and C, both `x.zip`, were in different groups: they raced with several
  workers, and with `--workers 1` A failed on the existing file. The order A, B, C
  split the same way (B alone, A with C), so A and B raced.
- **Failure inheritance ignored the name.** A named target inherited the failure of
  any earlier member with the same sources that knew *some* name, although a known
  name selects the file (the Nexus link is chosen by `target.filename`).
- The probe's `same_manual_no_request` check was vacuous (below).

The current source works as follows:

- **Types.** `src/acquisition.rs:103-130`. `ArchiveTarget` has
  `shared_destination: Option<SharedDestination>`, which is not serialized when
  `None`. `SharedDestination` is `SameUrl(owner)` or `DifferentUrl(owner)`, and
  serializes as `{"kind":"sameUrl"|"differentUrl","targetIndex":owner}`. `owner()`
  returns the index. The kind only says whether the URL equals the owner's.
- **Linking.** `src/download_orchestrator.rs:294-347`. `plan` sets each target's
  link from `shared_destinations`. `may_share_file` holds when both names are known
  and equal ignoring ASCII case, or when either name is unknown (revealed only by
  the transfer) and the registry URL is the same. `shared_destinations` is a
  union-find over every pair of targets that `may_share_file`: each target is merged
  with *all* earlier matches, not just the first, so groups are the connected
  components of that relation and any two targets that may publish one file are in
  one group. Unions keep the lower root, so each group's owner is its lowest plan
  index, and every other member links to it. A member can therefore be linked
  through a later target (in B, A, C above, A links to B through C).
- **Checking hand-built plans.** `src/download_orchestrator.rs:566-575`.
  `run_with_loopback_ca` recomputes `shared_destinations` and refuses any plan whose
  links differ, including a missing link. `4bf5c54` checked only that a link named
  an earlier unlinked target.
- **Queueing.** `src/download_orchestrator.rs:626-645`. Only unlinked targets
  (owners) are queued. The worker that takes an owner runs `acquire_group`, so the
  owner and all its linked targets are settled one after another in one worker.
  Since any two targets that may publish one file are in one group, two transfers
  never write one destination at the same time. Results are sorted by
  `targetIndex`, so `report.outcomes` is still one per target in plan order.
- **Settling a group.** `src/download_orchestrator.rs:686-741`. After the owner,
  `acquire_group` takes each linked target in plan order. It keeps a list of the
  members that ran their own sources, and applies the first rule that fits:
  1. **Published file.** An earlier member completed and published this target's
     file. For a known name, that means the published name equals it ignoring ASCII
     case, whatever that member's URL. This is the rule by which a rerun finds the
     file present by name. For an unknown name, it means that member has the same
     registry URL. The target takes that file with no request, subject to its own
     `ContentHashSHA256`.
  2. **Same sources.** An earlier member did not complete, and its source list
     equals this target's (same URL and the same fallbacks). If this target knows
     its name, that member must have asked for the same name, ignoring ASCII case: a
     known name selects the file (Nexus picks the file by `target.filename`), so a
     member that asked for another name made a different request. The target takes
     that outcome with no request, because a repeat would ask the same sources for
     the same file again. `339b3d6` accepted any known name here (`names_404`).
  3. **Otherwise** the target runs its own `acquire_sources`, as an unlinked target
     would. That covers a different URL, or extra Peer/magnet sources, after an
     owner that did not complete. The target is added to the list.
- **Taking a result.** `src/download_orchestrator.rs:743-802`. `shared_outcome`
  gives the target its own index, component GUID and registry URL, and sets
  `sharedWith` to the index of the member whose result it took. On success it takes
  the published file name; otherwise it keeps its planned name. The target's own
  authored `ContentHashSHA256` is compared with a published file's SHA-256, and a
  mismatch makes it `failed`. The target gets exactly one event, with its final
  phase (`completed`, `manualRequired`, `cancelled` or `sourceFailed`). The event
  comes from the owner's worker, after the events of the members before it.
- **No explicit different-URL failure.** `4bf5c54`'s `DifferentUrl` failure in
  `acquire_sources` is removed by `339b3d6`.
- **Other constructors.** `src/download_orchestrator.rs:455,1795`. `plan_url` and
  `nxm_target` set `None`.

`record_with`, `collect_fomod_consumers`, `fomod_cli::dispatch` and `main.rs` are
unchanged. Every owner that takes a published file has a `completed` outcome with
the same path, so recording writes each component's own registry entry. Binding
already merges consumers by archive path, so one FOMOD session covers all of them.

JSON stays backward compatible. Plan and report rows keep every key they had. Rows
without a link are byte-identical, and linked rows add only `sharedDestination` or
`sharedWith`. Event shape is unchanged.

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

Rust follows the sequential C# result: every owning component gets the archive and
records it. It deviates on purpose in these ways:

1. **One request per source list.** Rust never repeats a request that an earlier
   member of the group already made with the same source list. It never overwrites
   a published archive (no-clobber publish).
2. **A present file is taken by name, whatever its URL.** When a component names a
   file that another component's URL has just published, it takes that file, as
   C# and a Rust rerun (`AlreadyPresent`) do. Its own authored hash is still
   checked, so a component whose hash names other bytes fails instead
   (`different_hash`). When the owner did not complete, a component with another URL
   downloads from its own URL, as before `4bf5c54`. Rust therefore can accept one
   URL's bytes for another URL's `Files` name, exactly when a rerun would.
3. **With identical sources, the first member's outcome governs.** If the owner's
   authored hash rejects the bytes, a later component with the same URL and
   fallbacks fails with it, even if it authors no hash (`hash_owner`). The same holds
   for a 404, a gate or a manual host (`same_404`, `same_manual`). Before `4bf5c54`,
   a later component without a hash downloaded again and accepted the bytes.
4. **Case-insensitive names.** Known names compare ignoring ASCII case, like C#'s
   `OrdinalIgnoreCase` `Files` map, Rust `find_existing` and engine source matching.
   On a case-sensitive filesystem, names that differ only in case now share one
   destination. For example, `Example.zip` and `example.zip` from one URL give one
   file, `Example.zip`, instead of two copies (`case_variant`). The component that
   authored `example.zip` records that key, and its outcome and event name
   `Example.zip`. No `example.zip` file exists.
5. **A name revealed only by the transfer follows the same URL.** A component
   without a `Files` name shares a group with every component with the same URL,
   named or not (`unknown_follower`, `chain`), and a named component with an unnamed
   one with the same URL (`unknown_first`). It takes that component's file only when
   the revealed name is its own name. For a page URL, such as a Nexus mod page, an
   unnamed component takes the file that the earlier component named.
6. **Groups are transitive.** Components that are linked only through a third one
   (by name on one side, by URL on the other) are one group. In
   `transitive_unnamed_first`, B (unnamed, U2) publishes `example.zip`, and A
   (`example.zip`, U1) takes it by name without requesting U1, as a rerun would
   (`AlreadyPresent`); its own hash still applies.

Out of scope: two unknown names from different URLs that turn out equal still
collide at publish. So do a known name and an unknown name from different URLs that
the transfer reveals to be the same, when no third component links them. Within a
group, an unnamed member that must run its own URL after another URL published the
same name fails deterministically with `Download destination already exists`, after
its one request and never concurrently (`transitive_named_first`). A rerun gives the
same result, since an unnamed target is never `AlreadyPresent`. The desktop download path (`ui.rs`,
`download::fetch`) does not use this orchestrator and is unchanged.

## (d) Controlled production proof

`probe.py` follows the S2a probe conventions:

- **Isolation guard.** `FETCH_PARENT_NETNS` must differ from the current namespace,
  and the namespace must have no routes.
- **Origin.** A loopback HTTPS origin whose leaf certificate is signed by a
  throwaway CA, passed with `--extra-ca-file`. It records each request (path,
  start, end, thread). It holds every response, 404 included, for 0.4 s. So two
  concurrent requests overlap as they did before the fix, and a request made only
  after another has been answered starts at least 0.4 s after it.
- **Fixtures.** Private HOME/XDG/settings/mods/scratch directories and generic
  archives. The FOMOD archive comes from the S2a `package()` generator. A component
  can author no `Files` map, or a full registry URL instead of a loopback path.
- **Verb.** Only the real `fetch-downloads`, with `--peer-mode off --retries 0`.
- **Receipts.** Output goes to `target/cloud-proof/fetch-shared-url`. Each run keeps
  its argv, exit, stdout, stderr, parsed summary, events, per-run requests,
  before/after trees and fixture copies. `receipt.json` binds git HEAD, porcelain,
  source hashes, binary SHA-256 and mtime. The run fails if the binary is older
  than the sources, unless `FETCH_SHARED_ALLOW_UNBOUND=1` (used only for pre-fix
  binaries). It also checks that the binary and sources are unchanged after the
  run.
- **Cleanup.** The fixture root is removed at exit unless `MODSYNC_PROOF_KEEP=1`.

No archive is placed in the mods folder beforehand in any case. Every Fetch run
below uses `--workers 4` unless it says otherwise. Every case with `--record` also
checks the recorded rows.

| Case | Checks |
| --- | --- |
| `control`: one component, FOMOD archive, `--record --fomod-choices` | Plan and outcome keys exactly as before. Exit 0, one request, `completed`, recorded, one `configured` consumer, choices saved. |
| `shared_w4`, `shared_w1`: two components, one URL and name, FOMOD archive, workers 4 and 1 | Dry-run plan: second target `sameUrl` 0, keys additive, no request, no writes. Fetch: exit 0, **one** request. Outcomes `(0, A, completed)` and `(1, B, completed, sharedWith 0)`, the same path and SHA-256, keys additive only. Both recorded and saved `Files` true. Both are FOMOD consumers (`configured`) in one session `[A, B]`, and both have choices saved. Last event per index is `completed`, and the follower has exactly one event. The mods folder holds only the archive. |
| `different_urls` (e): `/a/example.zip` and `/b/example.zip`, both named `example.zip`, null `Files` flags | Plan `differentUrl` 0. Exit 0. Requests: `/a` 1, `/b` 0. Outcomes `completed` and `completed, sharedWith 0`, the same path and A's SHA-256. The follower has one `completed` event. Both are recorded under their own URLs, and both `Files` flags become true. The file holds A's bytes. |
| `different_hash` (e): the same, but B authors the SHA-256 of `/hb`'s own bytes | Exit 1. Requests: `/ha` 1, `/hb` 0. B is `failed, sharedWith 0` with the shared-download mismatch error. Only A is recorded. The file holds A's bytes. |
| `different_owner_404` (d): A's URL is missing (404), B's URL serves the file | Plan `differentUrl` 0. Exit 1 (A failed). One request each, and B's starts at least 0.4 s after A's (serialized). Outcomes `failed` and `completed` (no `sharedWith`). Terminal events `sourceFailed` and `completed`. Only B is recorded, and the file holds B's bytes. |
| `hash_follower`: the follower authors a wrong `ContentHashSHA256` | Exit 1, one request. The follower is `failed, sharedWith 0` with the shared-download mismatch error. Only the owner is recorded. |
| `interleaved`: A(X), B(Y), C(X) | Plan: only C is `sameUrl` 0. Exit 0, one request each for X and Y. Outcomes in guide order 0, 1, 2 with C `sharedWith` 0. All recorded in order, and the terminal events are ordered. |
| `distinct_names` (control): one URL, names `example.zip` and `copy.zip` | No `sharedDestination`, two requests, both `completed`, two files, each recorded under its own name. |
| `case_variant`: one URL, names `Example.zip` and `example.zip` | `sameUrl` 0, one request, both `completed`, one file `Example.zip`. Each component records its own key. |
| `same_404` (a): one missing (404) URL and name | Exit 1, exactly one request. Outcomes `failed` and `failed, sharedWith 0`, with the same error. The follower has one `sourceFailed` event. Nothing is recorded and no file is written. |
| `same_manual` (b): one `https://deadlystream.com/...` URL and name | Plan `sameUrl` 0. Exit 3. Both are `manualRequired` with `{url, reason: unsupported}`, and the follower has `sharedWith` 0. Events: the owner has `resolving` and `manualRequired`, and the follower has one `manualRequired`. No file is written. |
| `hash_owner` (c): the owner authors a wrong hash, the follower none, one URL | Exit 1, exactly one request. Outcomes `failed` and `failed, sharedWith 0`, both with the owner's `SHA-256 mismatch; download discarded`. Nothing is recorded, and no file is written. |
| `unknown_follower`: one URL, A names `example.zip`, B authors no `Files` | Plan: A `example.zip` unlinked, B unnamed `sameUrl` 0. Exit 0, one request. Both `completed`, B `sharedWith` 0. Both recorded as `example.zip`, and B's saved `Files` becomes `{"example.zip": true}`. One file. |
| `unknown_first`: the reverse order | Plan: B `sameUrl` 0. Exit 0, one request (the name A reveals is B's). Both `completed`, B `sharedWith` 0, both recorded, one file. |
| `chain`: A (`example.zip`, missing URL X), B (`example.zip`, URL Y), C (no `Files`, URL Y) | Plan: B and C are `differentUrl` 0 (C joins through B). Exit 1. One request each for X and Y. Outcomes `failed`, `completed`, and `completed, sharedWith 1`. B and C are recorded under Y, and the file holds Y's bytes. |
| `transitive_unnamed_first_w4`, `_w1`: B (no `Files`, U2 serving bytes P), A (`example.zip`, U1 serving other bytes Q), C (`example.zip`, U2), workers 4 and 1 | Plan: B unlinked, A `differentUrl` 0 (through C), C `sameUrl` 0. Exit 0. Requests: U1 0, U2 1. Outcomes `completed`, `completed, sharedWith 0`, `completed, sharedWith 0`, no errors, all with `example.zip` and P's SHA-256. A and C each have one `completed` event. All three recorded under their own URLs. The file holds P. A dry-run rerun plans no target and three `alreadyPresent` skips for `example.zip`, which is the rule A and C followed. |
| `transitive_named_first_w4`, `_w1`: A (`example.zip`, U1 serving Q), B (no `Files`, U2 serving P), C (`example.zip`, U2), workers 4 and 1 | Plan: A unlinked, B and C `differentUrl` 0. Exit 1. One request each for U1 and U2, and U2's starts at least 0.4 s after U1's (serialized). Outcomes `completed`, `failed` (no `sharedWith`, error `Download destination already exists`), `completed, sharedWith 0`. A and C have `example.zip` and Q's SHA-256. Terminal events `completed`, `sourceFailed`, `completed`. A and C recorded. The file holds Q. A dry-run rerun plans only B (unnamed, unlinked) and `alreadyPresent` for A and C. |
| `names_404`: A (`a.zip`), B (no `Files`), C (`c.zip`), one missing (404) URL | Plan: B and C `sameUrl` 0 (C through B). Exit 1. **Two** requests, the second at least 0.4 s after the first. Outcomes `failed`, `failed, sharedWith 0` (B asked for no name, so it inherits A's failure), and `failed` with no `sharedWith` (C's name differs from A's, so it made its own request). B has one `sourceFailed` event; C's events start with `resolving` and end with `sourceFailed`. Nothing recorded, no file. |

`same_manual` needs no network: the host class comes from the URL alone, and a
manual host is never requested. The namespace has no route, so any attempt would
end `failed`, not `manualRequired`; the outcomes and events are the evidence. The
probe no longer has a `same_manual_no_request` check: the loopback origin cannot
see a request to that host, so the check could never fail.

The seeded `fetch-downloads` `shared_pool` case (one URL, two names, over Peer)
cannot run here (section (e)). `distinct_names` covers the same no-over-merge rule
over Origin.

## (e) Results (cloud Linux)

- **Post-fix.** `probe.py` passes **266/266 checks** in 42 production invocations,
  exit 0. The default-feature debug binary SHA-256 is
  `4f003382fed63306d0780a48f40096c36b54fb9238b45c781d440d8a430ee1ce`
  (host-specific), built after the last source edit. The receipt records git HEAD
  `339b3d6`, with `src/acquisition.rs`, `src/download_orchestrator.rs`, this probe,
  this README, `COMPATIBILITY.md` and `docs/parity/PROGRESS.md` modified. The
  sources are bound by hash. `339b3d6` itself passed the earlier 181-check version
  of this probe (28 invocations, binary
  `6f5bd1a0e58a1376e377782f8422b0a256f04a983e6e06bfc56c28fa2650eeec`).
- **Pre-fix at `339b3d6`.** The same 266-check probe ran against a binary built with
  `--no-default-features` from a `git archive` of `339b3d6` in its own target
  directory under the scratchpad (SHA-256
  `7d08e0b023699eb9b7173554aa52cb3af2289d81c68c14ec882925eeb694feef`, output
  `target/cloud-proof/fetch-shared-url-pre339b3d6`, `FETCH_SHARED_ALLOW_UNBOUND=1`).
  It passes **244/266; 22 checks fail**, all in the new cases:
  - `transitive_unnamed_first_w4`: plan (A unlinked), exit, requests (U1 1, U2 1),
    outcomes, same file, follower events, errors and recorded: 8 checks. B and A
    raced, and A lost with `File exists (os error 17)`.
  - `transitive_unnamed_first_w1`: plan, exit, outcomes, same file, follower
    events, errors and recorded: 7 checks. B's group ran first, and A then failed
    before requesting with `Download destination already exists`.
  - `transitive_named_first_w4`: plan (B unlinked) and serialized: 2 checks. A and
    B requested concurrently; A published first in this run, so the outcomes
    happened to match.
  - `transitive_named_first_w1`: plan: 1 check. With one worker the split groups ran
    A, C, then B, which gives the same outcomes.
  - `names_404`: requests (1, not 2), serialized, outcomes (C `sharedWith` 0) and
    events: 4 checks. C inherited A's failure although it asked for `c.zip`.

  All older cases pass unchanged.
- **Earlier probe versions.** The two results below were measured with the
  181-check probe of `339b3d6`, before the cases above existed.
- **Pre-fix at `4bf5c54`.** The same probe ran against a binary built with
  `--no-default-features` from a `git archive` of `4bf5c54`, in its own target
  directory (SHA-256
  `5019ef5ec24a3c0838e145cbbd421d164eef37c52496d5ab31a02800249ee2e7`, output
  `target/cloud-proof/fetch-shared-url-pre4bf5c54`). It passes **154/181; 27 checks
  fail**, all in new or changed cases:
  - `different_urls`: exit, outcomes, same file, follower event, recorded and
    `Files` flags: 6 checks. B failed explicitly without a request, so it was not
    recorded.
  - `different_hash`: outcomes (`sharedWith`) and the mismatch error: 2 checks.
  - `different_owner_404`: requests (1/0), serialized, outcomes, terminal events,
    recorded and file: 6 checks. B failed explicitly without a request, so nothing
    was downloaded.
  - `unknown_follower` and `unknown_first`: plan, exit, one request (there were
    2), outcomes and recorded: 5 checks each. The loser got `File exists (os error
    17)`.
  - `chain`: plan (C unlinked), outcomes and recorded: 3 checks.

  `same_404`, `same_manual` and `hash_owner` pass at `4bf5c54`, because a same-URL
  follower already took the owner's outcome. All other cases pass unchanged.
- **Pre-fix at `728fa2f`.** The same probe ran against the earlier binary, also
  built with `--no-default-features` from a `git archive` of `728fa2f` in its own
  target directory (SHA-256
  `a836781d6bb988673504ac7170ace92d7d3c7764c3fcd7468bc0d25bdd890789`, output
  `target/cloud-proof/fetch-shared-url-pre728fa2f`). It passes **102/181; 79 checks
  fail** across every linked case: `shared_w4` 13, `shared_w1` 12, `different_urls`
  9, `different_hash` 5, `different_owner_404` 2 (plan and serialized),
  `hash_follower` 3, `interleaved` 6, `case_variant` 4, `same_404` 3, `same_manual`
  3, `hash_owner` 5, `unknown_follower` 5, `unknown_first` 5 and `chain` 4. Only
  `control` and `distinct_names` pass in full. With `--workers 1` (`shared_w1`) the
  pair made one request, not two: the second target failed on the existing file
  before requesting. In `hash_owner`, the follower downloaded again and accepted the
  bytes that the owner's hash had rejected. Which request wins the `different_urls`
  race varies (B won this run).

  The first version of this proof (106 checks, before the follow-up) failed 43 of
  them against the same `728fa2f` binary.
- **Regressions**, each in its own `unshare -Urn` namespace with the post-fix
  binary of the second follow-up:
  - S2a `probe.py`: 224/224.
  - S1 `probe.py`: 339/339 (61 cases).
  - `inherited_controller.py`: 41/41.
  - Nexus H1a `probe.py`: 138/138.
- **`docs/parity/fetch-downloads`** has no portable entry point. `run_acceptance.py`
  and `run_gates.py` hard-code `/home/brunner56/.cache/modsync-rs-target-lead`, and
  they write their receipts into the tracked `docs/parity/fetch-downloads/` tree.
  For `4bf5c54`, a scratch copy with only `REPO`, `BIN` and `OUT` rewired was tried.
  It stopped at `torrent-seed` before any Fetch case: this cloud kernel has no IPv6
  (`/proc/sys/net/ipv6` is absent), and the seeder's UDP tracker socket on
  `[::]:0` fails with `Address family not supported by protocol (os error 97)`. It
  is not counted as evidence, and it was not retried for either follow-up.
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

For a pre-fix comparison, build a `git archive` of `339b3d6` (or `4bf5c54`, or
`728fa2f`) with `cargo build --locked --no-default-features --lib --bin modsync` and
its own `CARGO_TARGET_DIR`. Then run the probe with `MODSYNC_BINARY=<that binary>`,
`MODSYNC_PROOF_OUT=target/cloud-proof/fetch-shared-url-pre339b3d6` (or
`-pre4bf5c54`, `-pre728fa2f`) and `FETCH_SHARED_ALLOW_UNBOUND=1`. That run is
expected to fail.
