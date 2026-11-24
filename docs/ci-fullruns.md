# Repository-owned full-run CI

This repository owns its entire CI variant: workflow entry points, local reusable stages, Python integration tools, policies and test fixtures. No workflow calls another repository's workflow. Each repository can evolve its variant independently.

## The repository-specific boundary

| Concern | ModSync.rs variant | mod-builds variant |
| --- | --- | --- |
| Subject being tested | Triggering engine commit | Triggering guide commit |
| Engine revision | Own `github.sha` | Reviewed exact counterpart SHA |
| Guide revision | Reviewed exact counterpart SHA | Own `github.sha` |
| Counterpart pin | `.github/fullrun.json` guide pin | `.github/fullrun.json` engine pin |
| Push policy | Quick checks on every branch push | Independently owned event policy |
| Local stages/tools | Owned by ModSync.rs | Owned by mod-builds |
| Actions caches and new artifacts | Owned by the execution repository/ref | Owned by the execution repository/ref |
| Existing domain CI | Production gates and manual packages; synthetic CLI/GUI workflows removed | Target-owned engine-check, native synthetic proof and manual package variants; metadata/release stay separate |

The small role configuration selects the data boundary; it does not fetch shared CI code. Local `./.github/workflows/...` calls and `github.repository`/`github.sha` checkouts are deliberate: every caller is local. The plan rejects execution under a foreign repository.

All counterpart revisions must be 40 lowercase hexadecimal characters. Update the reviewed pin in a commit or pass an exact manual `counterpart_sha`. The engine, guides, run, plan and attempt identities are recorded; a default-branch checkout cannot silently choose the tested engine.

## Entry points and composition

`Engine full-run CI` is the operator entry point. Every branch push runs the quick plan, Python syntax/configuration checks and workflow lint in a job with a two-minute timeout; it does not acquire inputs, build the application or install games. Same-repository PRs request the full pipeline against the exact PR head. Fork PRs cannot enter private installation stages; the separate `PR tooling contracts (no installation coverage)` workflow supplies credential-free tooling checks.

Defaults live in `FULLRUN_DEFAULTS` in `.github/workflows/fullrun.yml`, without required policy variables. Ordinary trusted PRs use **Full** with **cache-first**. A PR targeting `main` or `master`, whose head starts with `release/` or `release-please--`, or carrying `release` or `release-candidate` is a release candidate: it requires **Exhaustive** with **refresh**, including downloader-attempt evidence. Those release requirements cannot be downgraded by dispatch inputs. This is a PR gate, not a release-publication trigger.

Manual dispatch defaults to operation `full`, profile **Minimal**, download mode **cache-first**, both games and both CLI/GUI routes. Profile and operation are separate: operation `full` composes the pipeline; profile **Full** defines its selected mods. Full/install scopes build each requested application mode once inside the same workflow run, then verify and reuse those exact binaries across patcher backends. Quick checks do not compile the application. Standalone manual production builds remain optional. Dispatch can select:

| operation | Required stages | Side effects |
| --- | --- | --- |
| check | Plan + tooling | Public contract-only plan/verdict |
| full | Plan + tooling + application builds + games + archives + install matrix + six-way comparisons + P2P device proof | Explicitly admitted acquisition/install; optional producer cache writes |
| games | Plan + tooling + baseline preparation/verification | Optional verified sealed-cache and encrypted-backup publication |
| archives | Plan + tooling + archive preparation | Optional bounded public-mod cache save |
| install | Plan + tooling + application builds + requested install lanes and comparisons | Consume existing verified game/archive inputs; game/archive producers skipped |
| seed | Plan + tooling + reviewed staged import | Dry-run by default; explicit import uses the seed environment |
| retain-inputs | Plan + tooling + verified K1 auxiliary retention | Explicit ciphertext publication and independent restore verification; no game installation |
| p2p | Plan + tooling + production two-device seeding | Isolated device-to-device transfer, wrong-hash rejection and reverse seeding through ModSync |

Select K1/TSL/both and CLI/GUI/both. Install matrices use `fail-fast: false`; valid lanes still attempt recovery when another producer/lane fails. Each selected game has independent six-way comparison reporting, so a failure in the other game cannot suppress its evidence. The final collector requires every job in the requested scope to succeed and preserves a failed verdict. Skipped, cancelled, absent and incomplete required work fails that scope. Nonproducer operations ignore the shared producer-write checkbox.

Cancellation has separate `quick`, `installation` and explicit `retention` scopes per branch. A quick push cannot cancel the same commit's required PR installation; newer runs still cancel older runs within their own scope (`cancel-in-progress: true`).

The stage workflows are reusable **inside this repository**. The operator entry point can run each stage separately without duplicating its implementation. Branch pushes run only the quick path; a manual `check` remains available. `seed` is intentionally skipped during `full`: staged import is an explicit separate operation. There are no periodic downloads or release publication. The mod-builds variant retains its own event policy independently.

### Peer seeding and archive import

The required `p2p` stage uses two private Linux network namespaces connected only to each other, with separate filesystem views, home directories and caches. Production ModSync authors the torrent, seeds on device 1 and downloads on device 2. The proof checks exact payload bytes, SHA-256 and the actual sender upload counter. A fresh receiver must reject a wrong SHA-256 after a real transfer. Device 2 then seeds its downloaded archive back to a fresh device-1 workspace after the original is moved outside both views. Tracker/proxy-list acquisition is disabled and the devices have no Internet route. The bounded public receipt records isolation, transfer outcomes and cleanup.

P2P is checked by two separate tests. The first, identity, needs no binary and no network: the `p2p` stage's `P2P identity (info hash creation)` job runs `test_torrent_identity.py`, and the `archives` stage creates the info hash of every pinned archive version it holds verified bytes for (`torrent_identity.py survey`), compares it with `tools/modsync-ci/fixtures/ci/{k1,k2}-p2p-identity.json` and uploads a public `p2p-identity` receipt. To pin new versions, copy the `created-unpinned` entries from that receipt into the pin file, or run `survey --write-pins` against a verified object cache; an existing pin is never replaced. The second test is the two-device transfer above, shown as `P2P availability (two-device loopback, seeder pending)` because no long-lived seeder exists yet; it runs only after the identity job passes.

The legacy operation named `seed` is an optional staged archive-store import. Its S3/rclone credentials belong only to that import; ModSync peer seeding runs in the required `p2p` stage without those credentials. The two-device proof requires Linux namespace provisioning and fails when isolation cannot be established.

## Configuration and credentials

### Minimum setup

1. **Quick checks, fork tooling contracts and P2P device proof:** no custom secrets. These results establish no installation coverage. Trusted PR installation and eligible dispatches require the private-stage configuration below.
2. **Full runs:** expose `CACHE_ENCRYPTION_KEY` (at least 32 bytes) through [Actions secrets](https://github.com/kolto-labs/ModSync.rs/settings/secrets/actions). Existing encrypted games/archive packs need their original matching key; a new key cannot unlock them. For diagnostics, set the **public** native age recipient in the `DIAGNOSTICS_ENCRYPTION_RECIPIENT` [Actions variable](https://github.com/kolto-labs/ModSync.rs/settings/variables/actions). Generate a dedicated identity with `age-keygen -o diagnostics.agekey`, store the private file offline, and obtain its public recipient with `age-keygen -y diagnostics.agekey`. CI needs no diagnostics decryption secret. Do not upload the private identity, reuse the game key for diagnostics, or send either private key in chat. Backwards compatibility: when no recipient is configured, an independent `DIAGNOSTICS_ENCRYPTION_KEY` secret (at least 32 bytes) retains the original authenticated envelope format. A configured recipient takes precedence and never silently falls back on encryption failure. Admission validates its checksum; native preparation checks encryption before acquisition. When that route is selected, the caller does not forward the legacy diagnostics secret.
3. **Verified inputs:** use sealed baselines already available to this repository for both games, plus access to every archive in the pinned guide. Same-repository artifact reads use the built-in read-only token. For a cold start, point `FULLRUN_ARTIFACT_REPOSITORY` at an authorized legacy artifact repository and provide an `ARTSTORE_READ_TOKEN` limited to Actions read on that repository. The legacy token is only needed for inputs not available locally; do not remove it until both games and dependent archive inputs are independently available. Other existing options are per-game pinned encrypted seeds or explicitly enabled, licensed Steam acquisition.

Secrets may be defined at repository scope or organization scope with this repository selected in the organization's access policy. All workflows use GitHub's resolved `secrets.NAME` context and explicitly forward the selected secrets into local reusable workflows; neither scope is enumerated or required separately. [GitHub resolves identical names as repository over organization](https://docs.github.com/en/actions/reference/security/secrets): repository-only, organization-only and both-defined settings work without duplicate-setting errors. A job environment can override both; `seed` deliberately uses `mod-cache-seed`, so review its environment secrets too.

The optional `Bootstrap retained legacy ciphertext` workflow is a one-time migration for the reviewed November 2026 Odyssey artifact snapshot. It pins source identities/digests, copies only encrypted members under their original names, merges all four index fragments, and publishes the reconciled index after every payload upload succeeds. Each destination artifact records its source provenance. It never decrypts source material. Use a temporary source-scoped Actions-read capability only for that migration; after verifying both games and all destination pack references, remove it and clear `FULLRUN_ARTIFACT_REPOSITORY` so normal reads use `github.token`. This is an explicit migration, not retired pack renewal or seed publishing. Artifact expiry still requires an authorized data-retention plan.

K1's six pinned native widescreen inputs have a separate verified public-input accelerator. Trusted PRs restore it using the same pinned identity as branch runs; reading the accelerator does not enable publication. Each K1 install lane also restores it independently. Every payload is still checked against its closed filename, whole SHA-256 and size before use.

The optional `retain-inputs` operation (or **Retain verified K1 public inputs** direct dispatch once registered) verifies those six inputs and retains missing bodies in the existing encrypted repository-wide store. It uses the same matching `CACHE_ENCRYPTION_KEY` and built-in token, uploads ciphertext before a cumulative index, preserves every prior entry and independently decrypts/verifies the uploaded inputs. Publication remains explicit and serialized with the other index writers; normal archive preparation remains read-only. The registered full-run entry point calls this reusable stage, so it can bootstrap a feature branch before the new workflow exists on the default branch. This lets sibling branches recover the inputs without depending on a publisher or another branch's Actions cache.

For a bootstrap run, use the `games` operation with cache writes enabled to verify and publish both baselines, then `archives` to verify every guide input. Choose `full` to verify the four K1/K2 × CLI/GUI install lanes. Missing required inputs still fail; a key alone will not make unavailable source bytes accessible. A branch push exercises only quick checks; it cannot replace the trusted-PR or manual installation gate.

### Optional setup

| Setting | Purpose |
| --- | --- |
| `.github/fullrun.json` | Owned role, repository identities, immutable counterpart pin and inherited-toolkit provenance |
| `CACHE_ENCRYPTION_KEY` | Authenticate/decrypt sealed game and retained archive objects |
| `DIAGNOSTICS_ENCRYPTION_RECIPIENT` (public variable) | Encrypt private evidence to an offline age identity; preferred, no diagnostics CI secret |
| `DIAGNOSTICS_ENCRYPTION_KEY` (legacy alternative) | Independent secret for the old authenticated evidence format; unnecessary when a public recipient is set |
| `MODSYNC_SOURCE_TOKEN` (optional) | Locked dependency prefetch is anonymous first. Only a failed anonymous fetch may retry with this private-source credential, in a temporary config with private output deleted afterward. Current public dependencies need none; checkout uses `github.token`. |
| `ARTSTORE_READ_TOKEN` (optional) | Read an explicitly configured legacy artifact repository |
| `FULLRUN_ARTIFACT_REPOSITORY` (optional variable) | Optional legacy artifact read namespace; new publication remains in this repository |
| `MOD_CACHE_BUCKET`, `MOD_CACHE_S3_ENDPOINT` (optional variables) | Existing S3-compatible archive store, not a required new service. Use a variable for a plain, unsigned endpoint only. The existing endpoint secret takes precedence if configured. |
| `MOD_CACHE_S3_READ_ACCESS_KEY_ID`, `MOD_CACHE_S3_READ_SECRET_ACCESS_KEY` | Read-only archive consumers; passed only when `MOD_CACHE_BUCKET` selects the optional store |
| `SEED_STAGING_RCLONE_CONF_B64`, write credentials | Staging configuration is required even for dry-run reads; destination write credentials are passed only for actual publication to `MOD_CACHE_BUCKET` |
| `GAME_SEED_URL_K1/K2`, `GAME_SEED_KEY_K1/K2`, `GAME_SEED_SHA256_K1/K2` | Reviewed owned game seeds. Keep URLs and decryption keys secret; the mandatory ciphertext SHA-256 pins can be Actions variables. An existing checksum secret takes precedence. |
| `FULLRUN_ALLOW_STEAM` (optional variable) | Explicitly enable fallback acquisition in the designated account-owning variant |
| `STEAM_USERNAME`, `STEAM_PASSWORD`, `STEAM_CONFIG_VDF` | Only opted-in writer fallback after verified recovery/seed miss. Saved base64 auth configuration is tried before password; never required for retained-baseline recovery. |
| `STEAM_RECOVERY_CODE`, `STEAM_GUARD_CODES` | Forwarded only when Steam is enabled and a dispatch sets `use_steam_guard_codes: true`; never forwarded by normal pushes. |

Game materialization requires the cache key; private archive/seed/install evidence requires a public diagnostics recipient or the legacy independent diagnostics key. Secrets are passed only to admitted private stages on same-repository PRs or manual dispatch. Planning is credential-free: each private stage checks its own required configuration immediately after checkout, before acquisition or installation. Missing configuration fails that stage rather than downgrading a requested installation to a green contract-only check or suppressing independent P2P/tooling work. The final collector still requires every requested stage to succeed. Secret values cannot be read back from Actions; obtain matching existing keys from the authorized custodian or secret manager.

The following historical Odyssey settings have **no production consumer** in this variant and must not be copied as full-run prerequisites: `ALLDEBRID_API_KEY`, `ALLDEBRID_PASSWORD`, `ALLDEBRID_USERNAME`, `CACHE_ACCESS_KEY_ID`, `CACHE_SECRET_ACCESS_KEY`, `FULL_BUILD_CONFIG`, `PRIVATE_CI_RECIPIENT`, `PRIVATE_CI_TOKEN`. AllDebrid/private-CI adapters are not selected by this workflow. The caller and archive consumers pass S3 read credentials and the credential-bearing endpoint only when `MOD_CACHE_BUCKET` selects the optional store; the existing public endpoint-variable fallback remains available on that route. Seed dry-runs receive `SEED_STAGING_RCLONE_CONF_B64` to read and verify staged objects, but no destination S3 write pair or endpoint. Actual seed publication receives those destination credentials when the bucket is configured. Steam account credentials are forwarded only when `FULLRUN_ALLOW_STEAM` is `true`; one-use recovery and Guard codes additionally require an opted-in manual dispatch. Existing repository/organization secret resolution applies unchanged to every selected route.

A Nexus credential alone does not make pinned archives available: the cache tool has no Nexus headless acquisition adapter. Review seed hash coverage and supply missing reviewed bytes through the existing authorized staged seed/remote path, preserving the pins. Retained index membership proves neither decryptability nor complete guide coverage. Do not relax archive selection or required-lane checks to compensate.

Use separate Steam accounts or designate a single acquisition writer for an account shared across repositories; other variants restore verified seeds/backups. Writer and Steam-enabled game acquisitions remain serialized within a repository, not across repositories. Read-only K1/K2 recovery runs in parallel on isolated runners. Draft PRs already run full CI; marking them ready does not start another run. A public PR cannot receive these private input keys simply by changing CI code. Repository writers control source admitted to trusted PR and manual CI and must review that source before executing private stages. Manual package workflows remain available; the six synthetic CLI/GUI workflow entry points have been removed from this repository.

Private source tokens are scoped to checkout/dependency prefetch and never written to persistent global Git configuration. The installer receives a scrubbed environment without cache keys, provider keys or runtime tokens. This environment boundary is not a sandbox for malicious approved source; execute only reviewed engine/guide revisions.

## Storage and evidence

Game accelerators/backups contain authenticated ciphertext. Never add decrypted games, private source, credentials or private evidence to Actions cache paths. Public repository caches are readable by untrusted PR code. The archive accelerator contains only the approved mod archive input inventory.

Evidence uploads contain ciphertext plus a public `fullrun-provenance.json` receipt binding its SHA-256 to repository, commit, workflow, run and attempt. The same execution identity is inside the encrypted evidence. Age provides confidentiality and integrity, **not sender authentication**: before decrypting/trusting evidence, retrieve the artifact from the expected GitHub run and verify its API digest and run/head metadata, then check the receipt and inner identity. A receipt by itself is not a signature. Decrypt with `python tools/modsync-ci/seal.py unseal --source evidence.seal --destination fresh-directory --identity /offline/diagnostics.agekey`. Legacy evidence still decrypts without `--identity` using the independent original diagnostics key in `CACHE_ENCRYPTION_KEY`; existing game/cache envelopes keep their original format. Never put a private identity in CI or reuse the game key to read diagnostics.

Existing cache keys/formats and immutable input pins are retained; the selected profile determines which inputs are required. No workflow deletes old caches/artifacts or enables retired mod pack publication/renewal. Artifact quota and retention still apply; another workflow run does not create more allowance.

Current-repository game backups are read before an optional legacy namespace. A verified legacy backup may be reused, but it cannot pretend that a current-repository backup was published. Every candidate baseline must pass unpacked-byte and required-module validation before it is used or published. The installed game tree and archive/extraction workspaces are separate from the small encrypted evidence directory.

A cache save is an accelerator, not a portable output across repositories. Migration of existing bytes is separate operational work; preserve original copies and prove exact restoration before changing read ownership or removing a source.

### Private checkpoint storage and exhaustive capacity

On Linux, checkpoint CAS capture attempts `FICLONE` into a new private inode. It hashes the private clone and verifies object size/hash before immutable publication; existing objects must also match. Reflinks share extents while preserving independent bytes on later writes. Unsupported filesystems/platforms use the bounded streaming copy-and-hash path. Hard links are not used for this capture.

Exhaustive hosted lanes create a disposable private Btrfs loop filesystem with `compress=zstd:1`, co-locating game, mod/extraction work and checkpoint CAS so copy-on-write storage can apply. The preflight reserves 6 GiB of host free space and requires at least 40 GiB remaining for the image, then unmounts/removes it during cleanup. That measured minimum is a capacity admission check, not proof that every installation will fit or complete. Final hosted runtime validation of this storage path remains pending.

## Proof boundaries

A successful profile install requires an installed Rust report, matching requested profile, route and guide digest, a validated policy receipt, every selected component completed, no unexplained omissions, no partial result and zero reconciled lane errors. The receipt partitions the entire pinned inventory into selected, profile-excluded and platform-inactive GUIDs. GUI Base/Remaining receipts must cover the selected scope. Minimal or Full success cannot establish Exhaustive coverage; useful partial diagnostics remain failures.

The inherited runner still does its existing conversions, reviewed omissions, quarantine and GUI receipt checks. This integration does not claim independent app-selection parity, independent creator journeys, cross-lane per-mod barriers or native-platform full-guide parity. Those product/harness capabilities remain separate work. `cross_lane_parity: unverified` is explicit in plans/verdicts.

Removing the synthetic CLI/native-GUI workflow entry points does not establish any new product proof. The real install matrix still covers both CLI and GUI routes. P2P receipts prove direct transfer between isolated emulated devices. Internet discovery and NAT traversal require their own evidence. Manual package artifacts are not releases.

Rust tests and custom Rust harnesses remain frozen. Production builds use only lib/bin targets; no `--all-targets` or Rust test execution is introduced.

## Maintenance

Run the credential-free checks from the repository root:

```sh
python -m pip install pykotor==2.3.12 PyYAML==6.0.2
python -m unittest discover -s tools/modsync-ci -p 'test_*.py'
actionlint -config-file .github/actionlint.yaml .github/workflows/fullrun*.yml .github/workflows/cache-*.yml
```

The tools/format readers and immutable fixture corpus originate at Odyssey commit `1bbca57afc74c102bce6075cd40da205f98959e3`. They are now locally owned copies, not fetched at runtime. Legacy real-guide tests read their pinned fixture corpus under `tools/modsync-ci/fixtures`, not whichever live guide revision happens to be tested. Obsolete TypeScript/R2 YAML-wording assertions are replaced by tests of these variants' actual routing, ownership, credential, coverage and stage boundaries.

Keep behavior in the Python adapters and reusable local stages; keep event admission, runner choice and explicit secret mapping in YAML. Update this repository's tests/docs with its own changes. Cross-repository synchronization is a reviewable maintenance choice, not an execution dependency.

Workflow dispatch files generally must first exist on the default branch to appear in the normal Run workflow UI. The push trigger runs quick checks on the feature branch. Runtime success still requires successful producer, installation and P2P jobs; the existence of a branch or workflow file is not full-game evidence.

### Verified materialization before cache pruning

The archive producer restores and verifies each requested guide's required inputs before pruning the reusable accelerator to 2.5 GiB. Required inputs are scoped to the selected profile, with shared/setup and compatibility inputs retained. Per-game verification workspaces are removed between games. Policy-declared inputs stay explicit; unavailable required inputs fail, and install-stage omissions can never satisfy complete coverage. Install runs stop at the first failure instead of automatically quarantining required components.

Install lanes separately opt into releasing disposable hash objects after restore has produced independent archive copies. This requires an isolated GitHub-hosted `RUNNER_TEMP` cache, successful restore evidence and disjoint consumer/evidence paths. Archive copies, their map, the cache manifest and restore report remain intact; a cleanup receipt joins the encrypted lane evidence. Shared caches and retained artifacts are unaffected. The retained input inventory represents about 22.44 GiB of duplicate K1 objects and 9.84 GiB for K2; this reduction does not establish a full installation disk bound.

Normal artifact member acquisition reuses signed URLs only inside the authenticated owning-repository client, with a 64-entry limit and at most 480 seconds from resolution start. Expired or rejected URLs require authenticated refresh, and blob requests carry no API authorization header. Large stored members use bounded 32 MiB reads; ZIP CRC and expected object hashes remain mandatory. URLs are neither logged nor persisted, and legacy/default client and compressed-member paths retain their behavior.

For cold caches, a same-repository `mod-archive-manifest-sealed` artifact supplies authenticated URL-to-object associations. Recovery checks its transfer/source run and commit, the exact guide files, current reviewed pins and the decrypted manifest's digest. Only approved hashes, sizes, archive names and reviewed selection labels enter the public cache; arbitrary source metadata, credentials and private tracker URLs are discarded. This is input metadata, not proof that archive bytes are available: restore still verifies every object.

Reviewed seed migration accepts an exact source run and commit, one aggregate index and up to six bounded ciphertext packs, plus an optional sealed association manifest. Source credentials stay on the source runner. After verified import, normal acquisition uses owning-repository built-in read credentials and the existing material key.

The native GUI full-run path uses `--complete-guide` to drive both Base and Remaining phases. The coverage gate accepts completed widescreen coverage only with two phase receipts and a verified final component/file union. Legacy subset drivers remain available but cannot satisfy full-run coverage.

### Installation profiles and download modes

The closed policy in `tools/modsync-ci/install_profiles.json` binds the exact guide bodies and all original K1 197 / K2 151 component definitions. Every profile retains those definitions, their fields and original order; selection changes do not truncate the guide.

| Profile | Selected scope |
| --- | --- |
| Minimal (manual default) | Excludes the closed, measured optional environment/texture owners with at least 256 MiB of associated archive bytes, plus cinematic movie owners. Protects UCO and its patches, community/restoration patches, HR menus and known prerequisites; dependent selections follow the policy closure. |
| Full (ordinary trusted PR default) | Includes normal guide mods; excludes only the massive cinematic movie owners. |
| Exhaustive (release candidate) | Includes every applicable documented mod and the selected movie variant. Mutually exclusive alternatives are not installed together. |

Exhaustive uses K1's reviewed rescaled cutscenes and K2's reviewed remastered cutscenes. The other movie variant remains defined and inactive. Verified baseline applicability can additionally exclude an incompatible helper, such as the K2 4GB patch on Aspyr. Windows-only guide context remains recorded. Profile exclusions and applicability are explicit in `install_profile` receipts, bound to the policy hash, guide hash, original inventory and admitted input hash. Any unexplained omission or missing selected input fails. The 256 MiB value describes reviewed policy data, not a runtime rule that silently drops newly large downloads.

Acquisition discovers the full guide before filtering: an archive is omitted only when every referencing component is excluded. Shared selected inputs, setup inputs and required compatibility inputs remain required. CLI file selection, explicit deselection and GUI selection carry the same scope through conversion, review and installation.

**Cache-first** verifies destination/local objects, the Actions accelerator and authorized retained artifact tiers before contacting a publisher on a miss. A verified hot cache avoids upstream requests. **Refresh** attempts the configured upstream download paths even when cached bytes exist; download failure may fall back to verified cached bytes matching the immutable pins. It does not accept a changed publisher version under an old hash. Missing required bytes still fail. Acquisition receipts distinguish upstream attempts/downloads, valid cache fallback and explicit provider-policy limitations; cache/artifact reads are not counted as publisher downloads. In a composed `full` pipeline, the archive producer performs the requested refresh once and publishes its exact verified immutable cache key; CLI/GUI consumers use cache-first rather than repeat upstream refresh. A standalone `install` operation honors its requested download mode. Archive preparation has no global writer lease: existing generations are read-only inputs and producer cache keys are immutable. Release candidates require the refresh-attempt gate as well as exhaustive install coverage.

These policies and local component receipts do not establish an exact-head full CI installation pass. That claim requires successful requested producer, downloader, CLI/GUI installation and collector jobs on the reviewed head.

Archive preparation and install/seed runtimes include the distribution `unrar` reader. Existing integrity validation prefers it for RAR inputs; the exact Extended Enclave and Quarterstaff RARs pass its full CRC checks. The hosted Ubuntu 24.04 7-Zip reader crashes on those inputs and libarchive lacks their VM/solid features, so those readers alone do not cover the pinned guide. No integrity checks are skipped.

One-time migration publishes a cumulative owning-repository index after verifying all payload transfers. It preserves prior object associations and records prior index provenance, so the consumer’s bounded three-index lookup cannot forget older imports when another reviewed seed is added. The merge uses only the owning repository’s built-in read token.

Complete-guide lanes build optimized production binaries because archive inspection, validation, simulation and extraction consume full decoded streams. Checked arithmetic and debug assertions remain enabled in this CI release profile. CLI uses `--no-default-features`; GUI retains desktop features. Both use two compiler jobs, and every subsequent command and GUI driver uses the selected binary. Legacy adapter invocations retain their debug builds. The shared installer engine and all coverage requirements remain the same; no full-stream integrity pass is removed.

Seed callers select destination credentials through the publication/dry-run choice. The receiving `mod-cache-seed` job resolves the bucket and endpoint in its own environment, so an environment-only bucket can use repository or organization credentials. Dry-runs still omit destination credentials at both boundaries.
