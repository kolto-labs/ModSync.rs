# Repository-owned full-run CI

This repository owns its entire CI variant: workflow entry points, local reusable stages, Python integration tools, policies and test fixtures. No workflow calls another repository's workflow. Each repository can evolve its variant independently.

## The repository-specific boundary

| Concern | ModSync.rs variant | mod-builds variant |
| --- | --- | --- |
| Subject being tested | Triggering engine commit | Triggering guide commit |
| Engine revision | Own `github.sha` | Reviewed exact counterpart SHA |
| Guide revision | Reviewed exact counterpart SHA | Own `github.sha` |
| Counterpart pin | `.github/fullrun.json` guide pin | `.github/fullrun.json` engine pin |
| Push paths | CI/tooling plus Rust production inputs | CI/tooling plus guide/user-script inputs |
| Local stages/tools | Owned by ModSync.rs | Owned by mod-builds |
| Actions caches and new artifacts | Owned by the execution repository/ref | Owned by the execution repository/ref |
| Existing domain CI | Production gates and manual packages; synthetic CLI/GUI workflows removed | Target-owned engine-check, native synthetic proof and manual package variants; metadata/release stay separate |

The small role configuration selects the data boundary; it does not fetch shared CI code. Local `./.github/workflows/...` calls and `github.repository`/`github.sha` checkouts are deliberate: every caller is local. The plan rejects execution under a foreign repository.

All counterpart revisions must be 40 lowercase hexadecimal characters. Update the reviewed pin in a commit or pass an exact manual `counterpart_sha`. The engine, guides, run, plan and attempt identities are recorded; a default-branch checkout cannot silently choose the tested engine.

## Entry points and composition

`Engine full-run CI` is the operator entry point. Matching pushes in the owning engine repository execute `full`, including both games and both CLI/GUI install routes, with producer cache writes enabled. PRs execute `check`: local Python/tooling/workflow checks, with no game-decryption, diagnostics, Steam or provider credentials passed to that path. The owning-repository identity is checked before admission. Manual dispatch selects:

| operation | Required stages | Side effects |
| --- | --- | --- |
| check | Plan + tooling | Public contract-only plan/verdict |
| full | Plan + tooling + games + archives + install matrix + P2P loopback | Explicitly admitted acquisition/install; optional producer cache writes |
| games | Plan + tooling + baseline preparation/verification | Optional verified sealed-cache and encrypted-backup publication |
| archives | Plan + tooling + archive preparation | Optional bounded public-mod cache save |
| install | Plan + tooling + requested install lanes | Consume existing verified inputs; producers intentionally skipped |
| seed | Plan + tooling + reviewed staged import | Dry-run by default; explicit import uses the seed environment |
| p2p | Plan + tooling + production CLI loopback | Random synthetic transfer and wrong-hash rejection |

Select K1/TSL/both and CLI/GUI/both. Install matrices use `fail-fast: false`; valid lanes still attempt recovery when another producer/lane fails. The final collector requires every job in the requested scope to succeed and preserves a failed verdict. Skipped, cancelled, absent and incomplete required work fails that scope. Nonproducer operations ignore the shared producer-write checkbox.

The stage workflows are reusable **inside this repository**. The operator entry point can run each stage separately without duplicating its implementation. Source pushes run the full graph automatically; a manual `check` remains available. `seed` is intentionally skipped during `full`: staged import is an explicit separate operation. There are no periodic downloads or release publication. The mod-builds variant retains its own event policy independently.

## Configuration and credentials

### Minimum setup

1. **Contract checks and P2P loopback:** no custom secrets. A green PR run is contract-only; the install matrix runs on an owned push or eligible dispatch.
2. **Full runs:** expose `CACHE_ENCRYPTION_KEY` (at least 32 bytes) through [Actions secrets](https://github.com/kolto-labs/ModSync.rs/settings/secrets/actions). Existing encrypted games/archive packs need their original matching key; a new key cannot unlock them. For diagnostics, set the **public** native age recipient in the `DIAGNOSTICS_ENCRYPTION_RECIPIENT` [Actions variable](https://github.com/kolto-labs/ModSync.rs/settings/variables/actions). Generate a dedicated identity with `age-keygen -o diagnostics.agekey`, store the private file offline, and obtain its public recipient with `age-keygen -y diagnostics.agekey`. CI needs no diagnostics decryption secret. Do not upload the private identity, reuse the game key for diagnostics, or send either private key in chat. Backwards compatibility: when no recipient is configured, an independent `DIAGNOSTICS_ENCRYPTION_KEY` secret (at least 32 bytes) retains the original authenticated envelope format. A configured recipient takes precedence and never silently falls back on encryption failure.
3. **Verified inputs:** use sealed baselines already available to this repository for both games, plus access to every archive in the pinned guide. Same-repository artifact reads use the built-in read-only token. For a cold start, point `FULLRUN_ARTIFACT_REPOSITORY` at an authorized legacy artifact repository and provide an `ARTSTORE_READ_TOKEN` limited to Actions read on that repository. The legacy token is only needed for inputs not available locally; do not remove it until both games and dependent archive inputs are independently available. Other existing options are per-game pinned encrypted seeds or explicitly enabled, licensed Steam acquisition.

Secrets may be defined at repository scope or organization scope with this repository selected in the organization's access policy. All workflows use GitHub's resolved `secrets.NAME` context and explicitly forward the selected secrets into local reusable workflows; neither scope is enumerated or required separately. [GitHub resolves identical names as repository over organization](https://docs.github.com/en/actions/reference/security/secrets): repository-only, organization-only and both-defined settings work without duplicate-setting errors. A job environment can override both; `seed` deliberately uses `mod-cache-seed`, so review its environment secrets too.

For a bootstrap run, use the `games` operation with cache writes enabled to verify and publish both baselines, then `archives` to verify every guide input. Choose `full` to verify the four K1/K2 × CLI/GUI install lanes. Missing required inputs still fail; a key alone will not make unavailable source bytes accessible. If the workflow is not yet on the default branch and dispatch is unavailable, an owned push with matching paths exercises the full graph on that ref.

### Optional setup

| Setting | Purpose |
| --- | --- |
| `.github/fullrun.json` | Owned role, repository identities, immutable counterpart pin and inherited-toolkit provenance |
| `CACHE_ENCRYPTION_KEY` | Authenticate/decrypt sealed game and retained archive objects |
| `DIAGNOSTICS_ENCRYPTION_RECIPIENT` (public variable) | Encrypt private evidence to an offline age identity; preferred, no diagnostics CI secret |
| `DIAGNOSTICS_ENCRYPTION_KEY` (legacy alternative) | Independent secret for the old authenticated evidence format; unnecessary when a public recipient is set |
| `MODSYNC_SOURCE_TOKEN` (optional) | Locked dependency prefetch only when private Git dependencies need access; current public dependencies need none. Same-repository checkout uses `github.token`. |
| `ARTSTORE_READ_TOKEN` (optional) | Read an explicitly configured legacy artifact repository |
| `FULLRUN_ARTIFACT_REPOSITORY` (optional variable) | Optional legacy artifact read namespace; new publication remains in this repository |
| `MOD_CACHE_BUCKET`, `MOD_CACHE_S3_ENDPOINT` (optional variables) | Existing S3-compatible archive store, not a required new service. Use a variable for a plain, unsigned endpoint only. The existing endpoint secret takes precedence if configured. |
| `MOD_CACHE_S3_READ_ACCESS_KEY_ID`, `MOD_CACHE_S3_READ_SECRET_ACCESS_KEY` | Read-only archive consumers |
| `SEED_STAGING_RCLONE_CONF_B64`, write credentials | Existing staged import; `mod-cache-seed` environment; no implicit seed during installs |
| `GAME_SEED_URL_K1/K2`, `GAME_SEED_KEY_K1/K2`, `GAME_SEED_SHA256_K1/K2` | Reviewed owned game seeds. Keep URLs and decryption keys secret; the mandatory ciphertext SHA-256 pins can be Actions variables. An existing checksum secret takes precedence. |
| `FULLRUN_ALLOW_STEAM` (optional variable) | Explicitly enable fallback acquisition in the designated account-owning variant |
| `STEAM_USERNAME`, `STEAM_PASSWORD`, `STEAM_CONFIG_VDF` | Only opted-in writer fallback after verified recovery/seed miss. Saved base64 auth configuration is tried before password; never required for retained-baseline recovery. |
| `STEAM_RECOVERY_CODE`, `STEAM_GUARD_CODES` | Only fallback dispatches with `use_steam_guard_codes: true`; never consumed by normal pushes. |

Game materialization requires the cache key; private archive/seed/install evidence requires a public diagnostics recipient or the legacy independent diagnostics key. Secrets are passed only to the requested private stages on owned pushes or manual dispatch. Planning is credential-free: each private stage checks its own required configuration immediately after checkout, before acquisition or installation. Missing configuration fails that stage rather than downgrading a full push to a green contract-only check or suppressing independent P2P/tooling work. The final collector still requires every requested stage to succeed. Secret values cannot be read back from Actions; obtain matching existing keys from the authorized custodian or secret manager.

The following historical Odyssey settings have **no production consumer** in this variant and must not be copied as full-run prerequisites: `ALLDEBRID_API_KEY`, `ALLDEBRID_PASSWORD`, `ALLDEBRID_USERNAME`, `CACHE_ACCESS_KEY_ID`, `CACHE_SECRET_ACCESS_KEY`, `FULL_BUILD_CONFIG`, `PRIVATE_CI_RECIPIENT`, `PRIVATE_CI_TOKEN`. AllDebrid/private-CI adapters are not selected by this workflow. S3 read credentials are only for the optional read accelerator; `MOD_CACHE_S3_ACCESS_KEY_ID` and `MOD_CACHE_S3_SECRET_ACCESS_KEY` are only for explicit seed publication. `SEED_STAGING_RCLONE_CONF_B64` supplies the reviewed staging source, including during seed dry-run.

A Nexus credential alone does not make pinned archives available: the cache tool has no Nexus headless acquisition adapter. Review seed hash coverage and supply missing reviewed bytes through the existing authorized staged seed/remote path, preserving the pins. Retained index membership proves neither decryptability nor complete guide coverage. Do not relax archive selection or required-lane checks to compensate.

Use separate Steam accounts or designate a single acquisition writer for an account shared across repositories; other variants restore verified seeds/backups. GitHub concurrency groups serialize within a repository, not across repositories. A public PR cannot receive these private input keys simply by changing CI code. Repository writers control push-triggered CI and must review source before pushing. Manual package workflows remain available; the six synthetic CLI/GUI workflow entry points have been removed from this repository.

Private source tokens are scoped to checkout/dependency prefetch and never written to persistent global Git configuration. The installer receives a scrubbed environment without cache keys, provider keys or runtime tokens. This environment boundary is not a sandbox for malicious approved source; execute only reviewed engine/guide revisions.

## Storage and evidence

Game accelerators/backups contain authenticated ciphertext. Never add decrypted games, private source, credentials or private evidence to Actions cache paths. Public repository caches are readable by untrusted PR code. The archive accelerator contains only the approved mod archive input inventory.

Evidence uploads contain ciphertext plus a public `fullrun-provenance.json` receipt binding its SHA-256 to repository, commit, workflow, run and attempt. The same execution identity is inside the encrypted evidence. Age provides confidentiality and integrity, **not sender authentication**: before decrypting/trusting evidence, retrieve the artifact from the expected GitHub run and verify its API digest and run/head metadata, then check the receipt and inner identity. A receipt by itself is not a signature. Decrypt with `python tools/modsync-ci/seal.py unseal --source evidence.seal --destination fresh-directory --identity /offline/diagnostics.agekey`. Legacy evidence still decrypts without `--identity` using the independent original diagnostics key in `CACHE_ENCRYPTION_KEY`; existing game/cache envelopes keep their original format. Never put a private identity in CI or reuse the game key to read diagnostics.

Existing cache keys/formats, archive selections and seed pins are retained. No workflow deletes old caches/artifacts or enables retired mod pack publication/renewal. Artifact quota and retention still apply; another workflow run does not create more allowance.

Current-repository game backups are read before an optional legacy namespace. A verified legacy backup may be reused, but it cannot pretend that a current-repository backup was published. Every candidate baseline must pass unpacked-byte and required-module validation before it is used or published. The installed game tree and archive/extraction workspaces are separate from the small encrypted evidence directory.

A cache save is an accelerator, not a portable output across repositories. Migration of existing bytes is separate operational work; preserve original copies and prove exact restoration before changing read ownership or removing a source.

## Proof boundaries

A complete install requires an installed Rust report, matching requested mode and guide digest, positive component coverage, no omitted components, no partial result and zero reconciled lane errors. GUI subset/widescreen omissions fail complete-guide coverage. Useful partial diagnostics remain retained as failures.

The inherited runner still does its existing conversions, reviewed omissions, quarantine and GUI receipt checks. This integration does not claim independent app-selection parity, independent creator journeys, cross-lane per-mod barriers or native-platform full-guide parity. Those product/harness capabilities remain separate work. `cross_lane_parity: unverified` is explicit in plans/verdicts.

Removing the synthetic CLI/native-GUI workflow entry points does not establish any new product proof. The real install matrix still covers both CLI and GUI routes. P2P loopback is not external reachability/seeding proof. Manual package artifacts are not releases.

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

Workflow dispatch files generally must first exist on the default branch to appear in the normal Run workflow UI. The push trigger can run the full graph directly on this feature branch. Runtime success still requires successful producer, installation and P2P jobs; the existence of a branch or workflow file is not full-game evidence.
