# CI workflows, step by step

Every workflow in `.github/workflows`, what starts it, and what each job does in order. The step lists are generated from the workflow files, so each line is a real step name. For why the stages exist and how to operate them, see `docs/ci-fullruns.md`.

## Itemized list

| Workflow | File | Started by | Purpose |
| --- | --- | --- | --- |
| Engine full-run CI | `fullrun.yml` | push, pull request, manual run | Entry point. Plans the run, then calls the stage workflows below and collects one verdict. |
| PR tooling contracts (no installation coverage) | `fullrun-pr-contracts.yml` | pull request | Fast pull request check of the Python tooling. It gives no installation coverage. |
| Full-run tooling contracts | `fullrun-toolkit.yml` | called by another workflow, manual run | Runs every Python unit test under `tools/modsync-ci` and lints the full-run workflows. |
| Exact-head production application builds | `production-build.yml` | called by another workflow, manual run | Builds the production applications from the exact commit under test. |
| Game cache (reusable) | `cache-game.yml` | called by another workflow | Prepares and verifies the game baseline each install lane starts from. |
| Full-run archive preparation | `cache-archives.yml` | called by another workflow | Restores and verifies every mod archive, refreshing from the mod sites when asked. Also creates the P2P identity (info hash) of each pinned archive version. |
| Full-run Rust installation lane | `fullrun-lane.yml` | called by another workflow | One real installation: one game, one mode (CLI or GUI), one patcher backend. |
| Compare six real patcher installations | `fullrun-compare.yml` | called by another workflow | Compares the six installations of one game with each other. |
| Full-run P2P isolated device proof | `fullrun-p2p.yml` | called by another workflow | The two P2P tests: info hash creation, then a real transfer between two isolated devices. |
| Optional staged archive-store import | `cache-seed.yml` | called by another workflow | Optional import of staged archives into the archive store. Dry run unless asked. |
| Retain verified K1 public inputs | `retain-k1-public-inputs.yml` | manual run, called by another workflow | Stores the six verified public K1 widescreen inputs as ciphertext and proves they restore. |
| Production parity gates | `ci.yml` | pull request, manual run | Pull request gates on the Rust engine and its workflows. The job list below is the exact scope. |
| Full real-asset filesystem parity | `fullrun-parity.yml` | manual run | Manual filesystem parity run on real assets. Needs a self-hosted runner group. |
| Manual package artifacts (unverified) | `package-manual.yml` | manual run, called by another workflow | Manual, unverified package builds for each platform. |
| Release | `release.yml` | manual run, push | Builds and publishes a release. Never run without an explicit request. |
| Pages | `pages.yml` | push, manual run | Publishes the project site. |
| Bootstrap retained legacy ciphertext | `bootstrap-legacy-artifacts.yml` | manual run | One-off manual import of retained legacy ciphertext. |

## How a full run fits together

`fullrun.yml` runs `plan` first. On a full run it then starts `toolkit`, `production`, `games`, `archives` and `p2p` side by side. `install` waits for `production`, `games` and `archives` and runs one lane per game, mode and patcher backend. `comparison` follows `install`. `result` waits for everything and fails if any required job failed, was skipped or is missing.

Mod archives are refreshed from the mod sites once, in `archives`. On a full run the install lanes read the verified cache and do not download again. A standalone `install` run still uses the download mode it was given.

## Engine full-run CI

File: `.github/workflows/fullrun.yml`. Started by: push, pull request, manual run.

Entry point. Plans the run, then calls the stage workflows below and collects one verdict.

### Job `plan`

1. Checkout owned CI
2. Freeze repository-specific source and guide identities
3. Quick syntax and configuration checks
4. Quick workflow validation
5. Retain public plan

### Job `toolkit`

Waits for: `plan`.
Calls `fullrun-toolkit.yml`.

### Job `production`

Waits for: `plan`.
Calls `production-build.yml`.

### Job `games`

Waits for: `plan`.
Calls `cache-game.yml`.

### Job `archives`

Waits for: `plan`.
Calls `cache-archives.yml`.

### Job `install`

Waits for: `plan`, `production`, `games`, `archives`.
Calls `fullrun-lane.yml`.

### Job `comparison`

Waits for: `plan`, `install`.
Calls `fullrun-compare.yml`.

### Job `seed`: Optional archive-store import

Waits for: `plan`.
Calls `cache-seed.yml`.

### Job `retain_inputs`

Waits for: `plan`.
Calls `retain-k1-public-inputs.yml`.

### Job `p2p`

Waits for: `plan`.
Calls `fullrun-p2p.yml`.

### Job `result`

Waits for: `plan`, `toolkit`, `production`, `games`, `archives`, `install`, `comparison`, `seed`, `retain_inputs`, `p2p`.

1. Checkout owned CI
2. Fail on every required missing, incomplete, cancelled or failed result
3. Retain public requested-scope verdict

## PR tooling contracts (no installation coverage)

File: `.github/workflows/fullrun-pr-contracts.yml`. Started by: pull request.

Fast pull request check of the Python tooling. It gives no installation coverage.

### Job `toolkit`: Credential-free tooling contracts

Calls `fullrun-toolkit.yml`.

## Full-run tooling contracts

File: `.github/workflows/fullrun-toolkit.yml`. Started by: called by another workflow, manual run.

Runs every Python unit test under `tools/modsync-ci` and lints the full-run workflows.

### Job `contract`

1. Checkout owned CI
2. Use `actions/setup-python`
3. Install pinned Python test dependencies
4. Verify repository-owned Python tooling and workflow contracts
5. Quick syntax and configuration checks
6. Validate full-run workflow syntax and expressions

## Exact-head production application builds

File: `.github/workflows/production-build.yml`. Started by: called by another workflow, manual run.

Builds the production applications from the exact commit under test.

### Job `production`

1. Admit exact owned head
2. Use `actions/checkout`
3. Prepare production toolchain and native libraries
4. Build once with clean environment and record exact compiled inputs
5. Seal only application binary and provenance after credential-free build
6. Publish encrypted application bundle and nonsecret outer metadata only

## Game cache (reusable)

File: `.github/workflows/cache-game.yml`. Started by: called by another workflow.

Prepares and verifies the game baseline each install lane starts from.

### Job `game`

1. Use `actions/checkout`
2. Check game-stage credentials before acquisition
3. Reserve space on the disposable hosted runner
4. Look up encrypted game cache
5. Recover and verify baseline bytes from cache or backups
6. Restore game from seed
7. Require a restored baseline or seed for the probe
8. Download game with SteamCMD
9. Seal newly acquired game
10. Release the producer's unsealed working copy
11. Verify prepared seal before any publication
12. Save verified encrypted game cache
13. Stage verified encrypted game backup in the current repository
14. Upload encrypted game backup to this run's repository
15. Require publication of a needed current-repository backup

## Full-run archive preparation

File: `.github/workflows/cache-archives.yml`. Started by: called by another workflow.

Restores and verifies every mod archive, refreshing from the mod sites when asked. Also creates the P2P identity (info hash) of each pinned archive version.

### Job `archives`

1. Checkout owned CI
2. Check archive-stage credentials before acquisition
3. Checkout admitted guides
4. Use `actions/setup-python`
5. Reclaim unused hosted SDK space for selected archives
6. Install archive tools
7. Restore existing archive accelerator
8. Prepare applicable exact K1 public-input identity
9. Restore exact K1 six-public-input accelerator
10. Verify requested archive inputs
11. Create the P2P identity of every pinned archive version
12. Upload public P2P identity receipt
13. Admit only the six verified public objects for auxiliary caching
14. Save exact verified K1 six-public-input accelerator
15. Save verified public mod archive accelerator
16. Retain encrypted stage evidence

## Full-run Rust installation lane

File: `.github/workflows/fullrun-lane.yml`. Started by: called by another workflow.

One real installation: one game, one mode (CLI or GUI), one patcher backend.

### Job `install`

1. Checkout owned CI
2. Check install-stage credentials before materialization
3. Checkout admitted guides
4. Use `actions/setup-python`
5. Verify actual engine and guide identities
6. Reserve space on the disposable hosted runner
7. Prepare native production runtime
8. Install native GUI input libraries
9. Prefetch locked engine dependencies without persistent Git credentials
10. Prepare private copy-on-write storage for exhaustive installs
11. Restore sealed game accelerator
12. Recover and verify the lane-local game baseline
13. Restore archive accelerator
14. Resolve exact K1 auxiliary cache identity
15. Restore exact K1 auxiliary cache independently of its producer
16. Install the admitted full guide and require complete coverage
17. Compress complete comparison manifests
18. Retain encrypted comparison manifests for the same-run comparison job
19. Retain encrypted lane evidence
20. Release disposable exhaustive storage

## Compare six real patcher installations

File: `.github/workflows/fullrun-compare.yml`. Started by: called by another workflow.

Compares the six installations of one game with each other.

### Job `compare`

1. Checkout owned comparison implementation
2. Use `actions/setup-python`
3. Reserve space for complete checkpoint manifests
4. Verify same-run provenance and compare every component and final tree
5. Retain complete encrypted file differences

## Full-run P2P isolated device proof

File: `.github/workflows/fullrun-p2p.yml`. Started by: called by another workflow.

The two P2P tests: info hash creation, then a real transfer between two isolated devices.

### Job `identity`: P2P identity (info hash creation)

1. Checkout owned CI
2. Use `actions/setup-python`
3. Create info hashes for known versions and check every committed pin

### Job `isolated-devices`: P2P availability (two-device loopback, seeder pending)

Waits for: `identity`.

1. Checkout owned CI
2. Use `actions/setup-python`
3. Verify the triggering engine commit
4. Prepare production CLI toolchain
5. Prefetch locked engine dependencies without persistent Git credentials
6. Build production CLI only
7. Verify isolated device transfer, hash rejection and reverse seeding
8. Upload bounded public P2P receipt

## Optional staged archive-store import

File: `.github/workflows/cache-seed.yml`. Started by: called by another workflow.

Optional import of staged archives into the archive store. Dry run unless asked.

### Job `seed`: Import verified archives

1. Checkout owned CI
2. Check seed-stage credentials before import
3. Checkout admitted guides
4. Use `actions/setup-python`
5. Install verified transfer tool
6. Verify staged objects and optionally import
7. Retain encrypted stage evidence

## Retain verified K1 public inputs

File: `.github/workflows/retain-k1-public-inputs.yml`. Started by: manual run, called by another workflow.

Stores the six verified public K1 widescreen inputs as ciphertext and proves they restore.

### Job `publish`

1. Use `actions/checkout`
2. Use `actions/setup-python`
3. Prepare bounded ordinary archive readers
4. Compute exact six-input accelerator identity
5. Restore exact existing six-public-input accelerator
6. Verify all six public inputs and stage at most one encrypted pack
7. Publish encrypted pack first
8. Reconcile live API packs and preserve the complete cumulative inventory
9. Publish cumulative index last
10. Retain separate encrypted producer diagnostics

### Job `verify`

Waits for: `publish`.

1. Use `actions/checkout`
2. Use `actions/setup-python`
3. Prepare ordinary verification readers
4. Independently verify uploaded ZIPs and decrypt all six pinned inputs
5. Retain separate encrypted verification diagnostics

## Production parity gates

File: `.github/workflows/ci.yml`. Started by: pull request, manual run.

Pull request gates on the Rust engine and its workflows. The job list below is the exact scope.

### Job `workflows`: Validate workflows

1. Use `actions/checkout`
2. Validate workflow syntax and expressions

### Job `production`

1. Use `actions/checkout`
2. Use `dtolnay/rust-toolchain`
3. Use `Swatinem/rust-cache`
4. Install system and MinGW packages
5. Run an unnamed script
6. Run an unnamed script
7. Preinstall validator rejects impossible instructions
8. Run an unnamed script
9. Run an unnamed script
10. Run an unnamed script
11. Windows cross-check headers
12. Run an unnamed script
13. Run an unnamed script

### Job `native`

1. Use `actions/checkout`
2. Use `dtolnay/rust-toolchain`
3. Use `Swatinem/rust-cache`
4. Run an unnamed script
5. Run an unnamed script
6. Run an unnamed script
7. Run an unnamed script

### Job `tests`

1. Use `actions/checkout`
2. Use `dtolnay/rust-toolchain`
3. Use `Swatinem/rust-cache`
4. Install system packages
5. Tests (CLI-only build)
6. Tests (default build with desktop)

### Job `result`: Required CI result

Waits for: `workflows`, `production`, `native`, `tests`.

1. Require every mandatory or requested job to succeed

## Full real-asset filesystem parity

File: `.github/workflows/fullrun-parity.yml`. Started by: manual run.

Manual filesystem parity run on real assets. Needs a self-hosted runner group.

### Job `parity`

1. Use `actions/checkout`
2. Predict, install and compare on one runner

## Manual package artifacts (unverified)

File: `.github/workflows/package-manual.yml`. Started by: manual run, called by another workflow.

Manual, unverified package builds for each platform.

### Job `package`

1. Use `actions/checkout`
2. Initialize retained lane identity
3. Verify exact input and install selected toolchain
4. Linux build dependencies
5. Prepare locked dependency cache
6. Produce build receipt and assemble package
7. Retain source-bound receipts and artifacts, including failed lanes

## Release

File: `.github/workflows/release.yml`. Started by: manual run, push.

Builds and publishes a release. Never run without an explicit request.

### Job `prepare`

1. Resolve version and commit

### Job `package`

Waits for: `prepare`.
Calls `package-manual.yml`.

### Job `github-release`

Waits for: `prepare`, `package`.

1. Download the packaged targets
2. Collect release files
3. Publish the GitHub release

### Job `nexus`

Waits for: `prepare`, `github-release`.

1. Check Nexus Mods configuration
2. Download the released package
3. Build the Nexus archive
4. Read the file description
5. Upload to Nexus Mods
6. Record the Nexus upload so a retry does not repeat it

## Pages

File: `.github/workflows/pages.yml`. Started by: push, manual run.

Publishes the project site.

### Job `deploy`

1. Checkout
2. Configure Pages
3. Upload site
4. Deploy

## Bootstrap retained legacy ciphertext

File: `.github/workflows/bootstrap-legacy-artifacts.yml`. Started by: manual run.

One-off manual import of retained legacy ciphertext.

### Job `plan`

1. Use `actions/checkout`
2. Pin source artifacts and merge every retained index fragment
3. Use `actions/upload-artifact`

### Job `transfer`

Waits for: `plan`.

1. Use `actions/checkout`
2. Use `actions/download-artifact`
3. Verify ZIP and copy only original sealed members
4. Use `actions/upload-artifact`

### Job `publish-index`

Waits for: `plan`, `transfer`.

1. Use `actions/checkout`
2. Use `actions/download-artifact`
3. Preserve the current repository index in this cumulative publication
4. Use `actions/upload-artifact`
