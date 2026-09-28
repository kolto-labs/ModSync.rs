# Fetch: one transfer for components that share a URL and file

Source: commit `4bf5c54` (PR bodencrouch/ModSync.rs#2), its review follow-up commit
`339b3d6`, a second follow-up `3f0caf4` (merged at `45b3c64`), and a third follow-up on
HEAD `45b3c64` (section (b)). The receipt binds git HEAD, porcelain and source hashes.
`4bf5c54` fixed the pre-existing defect recorded in the
[final S2a quality check](../fomod-s2a-review.md#final-quality-check-of-ae8d7e9-approved).
An independent review of `4bf5c54` required changes, which `339b3d6` makes. A
re-review of `339b3d6` found that its grouping was not transitive and that failure
inheritance ignored which name a member asked for; `3f0caf4` fixes both. The review
that approved `3f0caf4` recommended two follow-ups, which the third follow-up makes:
an unnamed target adopts a file already present under the name its transfer reveals,
and an unnamed target no longer inherits a named member's failure.

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

The review that approved `3f0caf4` recommended two follow-ups:

- **An unnamed target failed on a present file.** When the name its transfer revealed
  was already present, the target failed with `Download destination already exists`
  after the transfer and digest were complete, although a named target takes a present
  file by name (`AlreadyPresent`). This held in a group (A `x.zip` U1, B unnamed U2, C
  `x.zip` U2: B failed after A published), without a group (A `x.zip` U1 and B unnamed
  U2 run one after the other), and on every rerun of an unnamed target, since an
  unnamed target is never `AlreadyPresent`.
- **An unnamed target inherited a named failure.** Rule 2 below let an unnamed target
  take the failure of a member with the same sources that asked for a name, although
  that failure can be specific to the name (the Nexus link is chosen by name).

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
- **Settling a group.** `src/download_orchestrator.rs:686-744`. After the owner,
  `acquire_group` takes each linked target in plan order. It keeps a list of the
  members that ran their own sources, and applies the first rule that fits:
  1. **Published file.** An earlier member completed and published this target's
     file. For a known name, that means the published name equals it ignoring ASCII
     case, whatever that member's URL. This is the rule by which a rerun finds the
     file present by name. For an unknown name, it means that member has the same
     registry URL. The target takes that file with no request, subject to its own
     `ContentHashSHA256`.
  2. **Same sources.** An earlier member did not complete, and its source list
     equals this target's (same URL and the same fallbacks). That member must have
     asked for the same name, ignoring ASCII case, or, like this target, for no
     name (`src/download_orchestrator.rs:719-731`): a known name selects the file
     (Nexus picks the file by `target.filename`), so a member that asked for
     another name, or for a name where this target asks for none, made a different
     request that may have failed for that name alone. The target takes that
     outcome with no request, because a repeat would ask the same sources for the
     same file again. `339b3d6` accepted any known name here, and `3f0caf4` let an
     unnamed target take a named member's failure (`names_404`).
  3. **Otherwise** the target runs its own `acquire_sources`, as an unlinked target
     would. That covers a different URL, or extra Peer/magnet sources, after an
     owner that did not complete, and an unnamed target after a named member that
     failed. The target is added to the list. If its transfer reveals a name that an
     earlier member published, it adopts that file (below).
- **Taking a result.** `src/download_orchestrator.rs:746-805`. `shared_outcome`
  gives the target its own index, component GUID and registry URL, and sets
  `sharedWith` to the index of the member whose result it took. On success it takes
  the published file name; otherwise it keeps its planned name. The target's own
  authored `ContentHashSHA256` is compared with a published file's SHA-256, and a
  mismatch makes it `failed`. The target gets exactly one event, with its final
  phase (`completed`, `manualRequired`, `cancelled` or `sourceFailed`). The event
  comes from the owner's worker, after the events of the members before it.
- **Adopting a present file.** When the name an unnamed target's transfer reveals is
  already present, `fetch_inner` (`src/download.rs:501-510`) no longer fails with
  text. It returns the typed `DestinationExists { path, sha256 }`
  (`src/download.rs:110-144`), where `sha256` is the digest of the completed transfer.
  The temp file is removed on return and nothing is published. `step_from_error`
  (`src/download_orchestrator.rs:1049-1051`) turns that error into
  `Step::Present`, and `acquire_sources` (`:941-947`) hands it to `adopt_present`
  (`:1061-1109`), which applies the rule a named target follows via
  `AlreadyPresent`: the target's own `ContentHashSHA256` must match the present file,
  and the present file must pass `archive::verify`. With no authored hash, the present
  file's SHA-256 must equal the downloaded digest; otherwise nothing says which bytes
  the component wants, and the target fails with `The present file ... differs from
  the downloaded bytes, and this component has no ContentHashSHA256 to choose between
  them; it was not replaced`. A hash mismatch fails with `SHA-256 mismatch: the present
  file ... does not match this component's ContentHashSHA256; it was not replaced`.
  Either failure ends the target (`Step::Terminal`), since a later source would find
  the same file. On success the target is `completed` with the present path and its
  SHA-256, with no `sharedWith`, because it ran its own source. The present file is
  never replaced or removed.
- **The Peer paths do the same.** An unnamed Peer copy learns its name from the
  torrent. In a race, `PeerStaging::publish` (`src/download_orchestrator.rs:1251-1292`)
  now returns `DestinationExists` with the staged copy's digest when that name is
  present, before or at its no-clobber move, instead of `Could not publish the peer
  copy`; the staged copy is removed with the staging folder. The race
  (`:1485-1506`, `:1575-1586`, `:1634-1637`) settles on the present file whether Origin
  or the Peer publish finds it: it stops the other racer, marks the present path as the
  winner so a loser's cleanup never removes it, and returns `Step::Present`.
  `origin_denies_peer` (`:872-887`) keeps a waiting Peer from starting after Origin
  found the name present. Outside a race, `PeerAcquire` (`src/peer_acquire.rs:166-175`)
  returns `DestinationExists` with the authored digest, which every Peer copy must
  have, before any transfer, where `p2p::fetch_authored` previously failed with
  `Download destination already exists`. A Peer source always authors a hash, so
  `adopt_present` then checks the present file against it. These Peer paths are
  source-only: no Peer transfer can run here (section (e)).
- **No explicit different-URL failure.** `4bf5c54`'s `DifferentUrl` failure in
  `acquire_sources` is removed by `339b3d6`.
- **Other constructors.** `src/download_orchestrator.rs:455,1913`. `plan_url` and
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
7. **An unnamed component adopts a present file only when it can tell it is the right
   one.** C# replaces the file (or, sequentially, finds it by name). Rust never
   replaces it. After its transfer, an unnamed component takes the present file under
   its own authored hash, or, with no hash, only if the present bytes equal the bytes
   it downloaded (`transitive_named_first`, `ungrouped_sequential`, `unnamed_rerun`).
   Otherwise it fails with a message that names the present file and says it was not
   replaced (`transitive_named_first_mismatch`, `ungrouped_hash`). C# would keep the
   last download's bytes instead. Before the third follow-up, it always failed with
   `Download destination already exists`.
8. **An unnamed component inherits only an unnamed member's failure.** It asked for no
   name, so a failure of a member that asked for one can be specific to that name, and
   the unnamed component makes its own request (`names_404`). Two unnamed components
   with identical sources still share one outcome (`unnamed_404`).

Out of scope: two unknown names from different URLs that turn out equal still
collide at publish when their transfers run concurrently. So do a known name and an
unknown name from different URLs that the transfer reveals to be the same, when no
third component links them and several workers run them at once. Only a name that is
already present when the transfer ends is adopted; a publish that loses a concurrent
no-clobber race still fails with the operating system's `File exists` error. An
unnamed Nexus target whose name the API reveals before the transfer still fails
before transferring when that name is present (the pre-transfer check in
`fetch_inner` for a `Directory` name hint), because no digest exists yet to compare
without a hash. The desktop download path (`ui.rs`, `download::fetch`) does not use
this orchestrator and is unchanged.

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
| `transitive_named_first_w4`, `_w1`: A (`example.zip`, U1), B (no `Files`, U2), C (`example.zip`, U2), U1 and U2 serving the same bytes, workers 4 and 1 | Plan: A unlinked, B and C `differentUrl` 0. Exit 0. One request each for U1 and U2, and U2's starts at least 0.4 s after U1's (serialized). Outcomes `completed`, `completed` (no `sharedWith`: B ran U2 and adopted the present file), `completed, sharedWith 0`, no errors, all with `example.zip` and the same SHA-256. Terminal events all `completed`. All three recorded under their own URLs. The file holds A's bytes and nothing else is in the mods folder. Recording names B's file, so a dry-run rerun plans no target and three `alreadyPresent` skips. |
| `transitive_named_first_mismatch_w4`, `_w1`: the same order, U1 serving Q and U2 serving other bytes P, no hash | Plan as above. Exit 1. One request each, serialized. Outcomes `completed`, `failed` (no `sharedWith`), `completed, sharedWith 0`. B's error says the present file differs from the downloaded bytes, that no `ContentHashSHA256` is authored, and that the file was not replaced. A and C have `example.zip` and Q's SHA-256. Terminal events `completed`, `sourceFailed`, `completed`. A and C recorded. The file holds Q and no temp file is left. A dry-run rerun plans only B (unnamed, unlinked) and `alreadyPresent` for A and C. |
| `names_404`: A (`a.zip`), B (no `Files`), C (`c.zip`), D (`a.zip`), one missing (404) URL | Plan: B, C and D `sameUrl` 0. Exit 1. **Three** requests, each at least 0.4 s after the one before. Outcomes `failed`, `failed` (no `sharedWith`: B asked for no name, A for `a.zip`), `failed` (no `sharedWith`: C's name differs), and `failed, sharedWith 0` (D asked for A's name). B's and C's events start with `resolving` and end with `sourceFailed`; D has one `sourceFailed` event. Nothing recorded, no file. |
| `unnamed_404`: A and B (both no `Files`), one missing (404) URL | Plan: B `sameUrl` 0. Exit 1, exactly one request. Outcomes `failed` and `failed, sharedWith 0`; B has one `sourceFailed` event. No file. |
| `ungrouped_sequential`: A (`example.zip`, U1), B (no `Files`, U2 serving the same bytes, B authors their hash), `--workers 1` | Plan: neither linked. Exit 0. One request each, U2 at least 0.4 s after U1. Outcomes both `completed` with no `sharedWith` and no error, both `example.zip` with A's SHA-256. Terminal events `completed`. Both recorded under their own URLs. One file. |
| `ungrouped_hash`: A (`example.zip`, U1 serving Q), B (no `Files`, U2 serving P, B authors P's hash), `--workers 1` | Exit 1. One request each. Outcomes `completed` and `failed` (no `sharedWith`), B's error `SHA-256 mismatch: the present file ...`, saying it was not replaced. Terminal events `completed`, `sourceFailed`. Only A recorded. The file holds Q. |
| `unnamed_rerun`: one component, no `Files`, no hash | A first Fetch without `--record`: exit 0, one request, `completed`. A second Fetch with `--record`: exit 0, one request (the name is unknown until the transfer), `completed` with no error, `example.zip` and its SHA-256, terminal event `completed`. Recorded, and the saved `Files` becomes `{"example.zip": true}`. The file keeps its inode, mtime and size (not replaced), and it is the only file. A dry-run rerun plans no target and one `alreadyPresent` skip. |

`same_manual` needs no network: the host class comes from the URL alone, and a
manual host is never requested. The namespace has no route, so any attempt would
end `failed`, not `manualRequired`; the outcomes and events are the evidence. The
probe no longer has a `same_manual_no_request` check: the loopback origin cannot
see a request to that host, so the check could never fail.

The seeded `fetch-downloads` `shared_pool` case (one URL, two names, over Peer)
cannot run here (section (e)). `distinct_names` covers the same no-over-merge rule
over Origin.

## (e) Results (cloud Linux)

- **Post-fix.** `probe.py` passes **354/354 checks** in 56 production invocations,
  exit 0. The default-feature debug binary SHA-256 is
  `912a31106263feec40e0ee3670caa28aa2182919cc196d18be8dadc2531778e8`
  (host-specific), built after the last source edit. The receipt records git HEAD
  `45b3c64`, with `src/download.rs`, `src/download_orchestrator.rs`,
  `src/peer_acquire.rs`, this probe, this README, `COMPATIBILITY.md` and
  `docs/parity/PROGRESS.md` modified. The sources are bound by hash.
- **Pre-fix at `45b3c64`.** The same 354-check probe ran against a binary built with
  `--no-default-features` from a `git archive` of `45b3c64` in its own target
  directory under the scratchpad (SHA-256
  `2448f2127e46bae9b25a7f81433726fae090bc6df2bfa11cc19e855031d5d50c`, output
  `target/cloud-proof/fetch-shared-url-pre45b3c64`, `FETCH_SHARED_ALLOW_UNBOUND=1`).
  It passes **316/354; 38 checks fail**, all in new or changed cases:
  - `transitive_named_first_w4` and `_w1`: exit, outcomes, errors, same file,
    terminal events, recorded, rerun targets and rerun present: 8 checks each. B
    failed with `Download destination already exists` after its request, although
    the present file held its bytes, so it was not recorded and a rerun planned it
    again.
  - `transitive_named_first_mismatch_w4` and `_w1`: the clear error: 1 check each.
    B failed with the bare `Download destination already exists`; the outcomes and
    the kept file already matched.
  - `names_404`: requests (2, not 3), serialized, outcomes (B `sharedWith` 0) and
    events: 4 checks. B inherited A's failure for `a.zip` although it asked for no
    name.
  - `ungrouped_sequential`: exit, outcomes, errors, same file, terminal events and
    recorded: 6 checks. B failed on the file A published.
  - `ungrouped_hash`: the hash error: 1 check.
  - `unnamed_rerun`: exit, outcomes, errors, file outcome, terminal event, recorded,
    saved `Files`, rerun targets and rerun present: 9 checks. Every rerun of an
    unnamed target failed on its own earlier file.

  `unnamed_404` and all older cases pass unchanged.
- **The 266-check probe of `3f0caf4`.** Before the third follow-up, the probe had 266
  checks in 42 invocations, and `3f0caf4`'s default-feature binary (SHA-256
  `4f003382fed63306d0780a48f40096c36b54fb9238b45c781d440d8a430ee1ce`) passed all of
  them. In that version, `transitive_named_first` served different bytes from U1 and
  U2 and expected B to fail with `Download destination already exists`, and
  `names_404` had three components and expected B to inherit A's failure; those
  expectations are the defects the third follow-up fixes.
- **Pre-fix at `339b3d6`.** The 266-check probe ran against a binary built with
  `--no-default-features` from a `git archive` of `339b3d6` in its own target
  directory under the scratchpad (SHA-256
  `7d08e0b023699eb9b7173554aa52cb3af2289d81c68c14ec882925eeb694feef`, output
  `target/cloud-proof/fetch-shared-url-pre339b3d6`, `FETCH_SHARED_ALLOW_UNBOUND=1`).
  It passes **244/266; 22 checks fail**, all in the cases `3f0caf4` added:
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

- **Earlier probe versions.** The two results below were measured with the
  181-check probe of `339b3d6`, before the transitive, `names_404` and later cases
  existed.
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
  binary of the third follow-up:
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
  is not counted as evidence, and it was not retried for any follow-up. The Peer
  paths of the third follow-up are therefore covered by source only.
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

For a pre-fix comparison, build a `git archive` of `45b3c64` (or `339b3d6`, `4bf5c54`
or `728fa2f`) with `cargo build --locked --no-default-features --lib --bin modsync` and
its own `CARGO_TARGET_DIR`. Then run the probe with `MODSYNC_BINARY=<that binary>`,
`MODSYNC_PROOF_OUT=target/cloud-proof/fetch-shared-url-pre45b3c64` (or
`-pre339b3d6`, `-pre4bf5c54`, `-pre728fa2f`) and `FETCH_SHARED_ALLOW_UNBOUND=1`. That
run is expected to fail.
