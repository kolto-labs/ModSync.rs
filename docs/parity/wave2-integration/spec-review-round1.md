This confirms the second key consumer (UpdateCheck) has no endpoint override and is untouched by the merge — no leak risk. Requirement (1) confirmed intact.

## SPEC_COMPLIANCE review — wave-2 integration

**Requirement (1) — explicit-only credentials / bypass settings:** Holds. `src/main.rs:989-1008` (NxmDownload) and `src/main.rs:1077-1084` (UpdateCheck) preserve explicit-flag-wins-over-settings, and `nexus_api_base.is_some()` forces an explicit-key-only requirement (`main.rs:989-995`), unchanged by the deployment merge — these lines sit outside the diff entirely.

**Requirement (2) — deployment lane merge:**
- Single `DeploymentAction` match in `main.rs:722-820`, no duplicate/stale routing; `DeploymentRoots::service()` (`main.rs:302-311`) is the sole resolution path, used uniformly by all seven subcommands.
- Lock coverage on mutating paths: `Deploy` (`:733`), `Uninstall` (`:740`), `Purge` (`:747`), `Record` (`:770`) all hold `_lock` before mutating. `Snapshot` (`:755-758`) is read-only against the game dir (writes only to `--out`), correctly unlocked. `record_live` (`deployment.rs:1497`) calls `ensure_exclusive_game()`, not just the weaker `ensure_own_game()` — confirms peer/owner-record checks apply to Record too.
- Lock scheme genuinely excludes installs: `Service::lock` (`deployment.rs:199-221`) and `recovery::Lock::acquire` (`recovery.rs:46-74`) hash the same canonicalized game path with the same `.modsync-lock-<hash[..16]>` naming; both `Service::new` (`deployment.rs:178`) and `Roots::new` (`paths.rs:44`) canonicalize `game` before that hash, so they collide correctly.
- Path safety and fail-closed-on-competing-roots: `ensure_exclusive_game`/`ensure_own_game`/`ensure_unowned_or_self`/owner-record adoption (`deployment.rs:226-413`) are exercised on Deploy, Uninstall, Purge, Record.

**One gap in Switch** (`main.rs:776-820`), not blocking but worth recording:
- The game lock is held only inside the `purge_current` branch (`:795`); the non-purge live-deployment check (`:800-806`, `service.deployed()?.len()`) and the subsequent `store.activate(&to)` (`:808`) run unlocked, leaving a narrow TOCTOU window where a concurrent `deploy` could land between the check and the profile switch.
- `else if let Ok(service) = service` (`:800`) silently treats a `for_profile` construction error (e.g., missing profile JSON, `deployment.rs:429-433`) as "no live deployments" and proceeds to switch — fail-open on that specific error path, contrary to the fail-closed posture required elsewhere in this lane.

**Requirement (3) — no edits to AGENTS.md/Rust tests/frozen UI tests:** Not independently verifiable from this read-only, no-git session; the README's self-claim ("No Rust tests were created, modified or run; the frozen UI test block was compared byte-for-byte") is an implementation report, not verified evidence. Mark **unresolved** — would need `git diff 4bfacda..285f55d -- AGENTS.md <test files>` to settle.

**Requirement (4) — docs don't overclaim:** Holds. `docs/parity/wave2-integration/README.md:26-31` explicitly states F01–F28 partial, F29 unverified, Fetch Downloads not exposed through both frontends, and Peer adapter/wiring not done. `seven-lane-review.md` independently confirms the Peer non-wiring is documented, not misrepresented.

**Overall:** SPEC_COMPLIANT **with two caveats**: (a) the Switch lock/fail-open gap above should be fixed or explicitly accepted, and (b) requirement (3) (no AGENTS.md/frozen-test edits) is unresolved for lack of git access in this session — recommend a `git diff` check before sign-off.
