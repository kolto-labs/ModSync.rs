# Final bounded Switch/integration quality review

Source714e59b; receipts through90efdf2. Reviewer82698f80-d389-40c9-9878-6ee57f9de6f0.

## Reassessment

Independently verified both fixes against source before reading any other agent's writeup.

**Finding 1 (soft-skip purge deletes manifest, orphans modified file) — fixed.**
`main.rs:824-834` now runs `service.reinstall_blockers()` (deployment.rs:1137-1175) over every live manifest *before* any purge mutation. That function checks exactly the two skip conditions my repro relied on — hash mismatch (:1156-1167) and non-regular-file (:1169) — plus the unsafe-path case (:1147-1152), and `ensure!`s a clean refusal with "nothing was changed" if any are found. Post-purge, a new `skipped` check (`purged.iter().any(|r| !r.skipped.is_empty())`, :843) also refuses even if a blocker appears between preflight and purge (TOCTOU/hostile-writer case), consistent with the code's own comment that this isn't claimed atomic.

exchange:
claim: the hash-mismatch/non-regular-file soft-skip path is now blocked before any mutation
referent: deployment.rs:1156-1169 (reinstall_blockers) called from main.rs:826-834, gated by `ensure!(blockers.is_empty(), ...)`
checked: read deployment.rs:1137-1175, main.rs:816-834
verdict: holds
confidence: high

exchange:
claim: probe p8 reproduces my exact repro (modify deployed file post-deploy, switch --purge-current) and now asserts zero mutation, not just exit code
referent: switch-probes.py:225-247, `p8_game_and_artifacts_unchanged` compares whole-tree hash before/after via `hash_tree`
checked: read switch-probes.py:225-247; switch-probes.json shows `p8_modified_after_deploy_refused: true`, `p8_game_and_artifacts_unchanged: true`
verdict: holds
confidence: high

**Finding 2 (`active_name()` read outside lock, stale-`from` race) — fixed.**
`main.rs:788-795` now builds `target` and calls `target.lock()` *before* `store.active_name()` is read at :797. Since `Service::lock()` is keyed only on the game path (deployment.rs:200-208), this is the same lock file `source` would use, so locking once via `target` first and reading `from` under it closes the ordering gap I flagged. I accept the correction that a competing switch doesn't queue — `fs2::try_lock_exclusive` fails immediately (confirmed at deployment.rs:218-219, `.context("Another ModSync process is using the game directory")`) rather than blocking — which makes the ordering fix sufficient rather than merely mitigating.

exchange:
claim: lock is acquired before active_name is read, closing the race I described
referent: main.rs:788-797 (`target.lock()` at line ~795, `store.active_name()` at line ~797)
checked: git diff 2252894..714e59b -- src/main.rs
verdict: holds
confidence: high

exchange:
claim: try_lock_exclusive fails immediately rather than blocking, so my "queues on game lock" premise was wrong
referent: deployment.rs:218-219, `fs2::FileExt::try_lock_exclusive(&file).context(...)`
checked: read deployment.rs:199-221
verdict: holds
confidence: high — `try_lock_exclusive` is non-blocking by definition (fs2 docs/semantics), matches the `.context` error message implying immediate refusal, not a wait

**Remaining scope, explicitly not re-litigated:** malformed-manifest warnings and `Store::activate`'s separate settings-lock are named as pre-existing/out-of-scope by the fix's own `scope_note` in switch-round2-build-receipt.json, and I have no new demonstrable finding against them.

**No new Important/Critical finding.** I looked for whether the `reinstall_blockers` preflight is skipped on the no-purge branch (`purge_current == false`) — it isn't; it runs unconditionally at :816-834 before the `if purge_current` split, so both branches benefit.

Build/clippy evidence checked: `switch-round2-build-receipt.json` records `source_git_sha: 714e59b`, no-default-features build+clippy both exit 0 with logs (`switch-round2-build-receipt.log`: "build exit=0", "clippy exit=0"); `switch-round2-desktop-receipt.json` records default-features build+clippy exit 0 at `90efdf2` (docs-only commit atop `714e59b`), with `src/main.rs` hash `366b9a7b...` matching `sha256sum src/main.rs` in the current worktree exactly. 39/39 probes pass, including the four new ones (p8-p11) that directly target both fixed gaps plus the two-overlap reverse-purge and mixed-blocker no-partial-mutation cases.

**Ready to merge: Yes**, for this bounded `DeploymentAction::Switch` integration, with the pre-existing limitations (malformed-manifest warnings, separate settings-lock on plain profile activate, no hostile-writer atomicity guarantee) understood as explicit, documented boundaries rather than gaps in this fix.
