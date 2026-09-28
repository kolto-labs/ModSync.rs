# Bounded integration quality review, round 1

Source 2252894. Reviewer session 82698f80-d389-40c9-9878-6ee57f9de6f0.

Confirmed same digest scheme (`roots.game.to_string_lossy()` vs `self.game.to_string_lossy()`, both canonicalized paths, `[..16]` hex slice, same `.modsync-lock-` prefix) — genuine parity. Now writing the review.

---

## Review: wave2-integration switch lock fix (285f55d..2252894)

### Scope actually inspected
This dispatch's code delta (285f55d..2252894) touches only `src/main.rs`'s `DeploymentAction::Switch` handler; `src/deployment.rs` is unchanged in this range but its `uninstall_report`/`purge_report` semantics (touched by earlier lanes) are load-bearing for this fix's correctness, so I traced into them. Peer-staging exclusion (28b32db) and credential precedence (pre-existing, unchanged here) were checked as named requirements.

### Strengths
- `main.rs:781-841` — single lock acquisition point chosen correctly: since `Service::lock()` (deployment.rs:199-221) hashes only `self.game`, source and target `Service`s for the same game dir resolve to the identical lock file, so locking once via whichever service is present is sufficient — no double-lock/self-deadlock risk (verified: `fs2::try_lock_exclusive` is called once, on one `File` handle).
- Construction of `source`/`target` `Service::for_profile` happens before the lock is taken and errors propagate via `?`, so a bad profile can't leave a dangling lock — matches the stated intent.
- Post-purge `live == 0` check (main.rs:824-827) correctly reads `deployed()` fresh after `purge_report()`, and this genuinely blocks activation when `uninstall_report` populates `retained` (deployment.rs:1237-1352, entries pushed to `retained` at :1250 and :1297) — verified this is a real block, not cosmetic.
- Peer-staging exclusion (`is_peer_staging`, download_orchestrator.rs:829-834) is applied identically in both scan sites (download_plan.rs:72, engine.rs:232) and is separate from the acquisition path itself — scoped as required, doesn't touch download/staging logic.
- Lock hash scheme parity: `Service::lock` (deployment.rs:200-208) and `recovery::Lock::acquire` (recovery.rs:24-27, 61-65) use the identical `Sha256::digest(canonicalized-path)[..16]` → `.modsync-lock-<hash>` construction, so probe p2's independently-recomputed lock path (switch-probes.py:40-43) genuinely proves this switch command contends with the engine's own install lock, not just an internal fixture.
- Build/clippy receipts (`switch-build-receipt.json`) are bound to source hashes that match the actual worktree (`sha256sum src/main.rs` == recorded hash for `2252894`) — not stale.

### Critical
None found in the audited delta itself (the lock/serialization logic is sound as far as it goes). The two issues below are Important, not Critical, per the "warnings are printed, not silent" distinction below.

### Important

**1. Purge's "soft" skip paths delete the manifest while leaving the file live and untracked — switch proceeds anyway.**
- `uninstall_report` (deployment.rs:1239-1350) only pushes to `retained` for the "can't resolve destination" (:1250) and "restore failed" (:1297) cases. The hash-mismatch skip (:1340-1343, "file was modified after deployment") and the non-regular-file skip (:1271-1275) are **not** added to `retained`.
- Consequence: if `retained` ends up empty, `uninstall_report` deletes the manifest unconditionally at :1352, even though the on-disk file itself was left untouched (mismatch case) or an entry was silently dropped (non-regular-file case). `deployed()` after purge then reports 0 for that component, so `main.rs:824`'s `live == 0` check passes, and `switch --purge-current` exits 0 and activates the target profile.
- Result: an orphaned, no-longer-manifest-tracked file (and, in the mismatch case, its backup under `backups/<id>/`, since `remove_empty_tree` only prunes empty trees) is left in the game directory under the new active profile, invisible to any future `deployment status`/`purge` for either profile.
- Repro: deploy component under profile A over an existing file, hand-modify the deployed file's bytes (breaking the hash match), then run `switch --to B --purge-current`. Expect exit 0, `purged[0].warnings` mentioning "modified after deployment", but `deployed` in the JSON output showing 0 and profile B now active — with A's stale file and backup silently orphaned.
- This is not literally silent (the warning is printed via `warn(&report.warnings)` at main.rs:817), but it does **not** block activation the way the task's "partial purge cannot silently activate" requirement implies. The `switch-probes.json`/`switch-probes.py` p7 case (lines 182-203) only exercises the hard-error path (chmod 555 on the game dir → `fs::remove_file` returns `Err`, propagating through `purge_report()?` before `uninstall_report` even reaches the retained/manifest-delete logic) — confirmed by tracing p7's mechanism. There is no probe for the hash-mismatch/soft-skip path. This is the concrete gap the task description flagged; it is real and unprobed, not a hypothetical.

exchange:
claim: probe7 tests only the hard-error (process-level Err) purge failure, not the soft warn-and-delete-manifest path
referent: docs/parity/wave2-integration/switch-probes.py:192-193 (`os.chmod(game, 0o555)`) forces `fs::remove_file` at deployment.rs:1264 to fail, which propagates via `?` out of `uninstall_report`/`purge_report` before reaching the `retained`/manifest-delete logic at :1351-1352
checked: read deployment.rs:1261-1279 and switch-probes.py:182-203
verdict: holds
confidence: high — the two failure paths are structurally distinct (early `Err` propagation vs. successful `Ok` with warnings), and only one is exercised by any test in this evidence set

**2. `active_name()` is read outside any lock, before the game lock is acquired — concurrent switches can activate against a stale `from`.**
- `main.rs:783` (`let from = store.active_name()?;`) is a plain settings read (`profiles.rs:510-512`, no lock). The game lock isn't acquired until `main.rs:810`. `Store::activate` (profiles.rs:513-528) takes the settings-store's own separate lock (`profiles.rs:196-208`, a different lock file — `.lock` under the settings root, unrelated to the per-game `.modsync-lock-<hash>` file).
- Two concurrent `switch` invocations against the same game directory (e.g., a UI double-click, or the second one queued behind the first's game-directory lock) both read the same `from` before either commits. The first to acquire the game lock proceeds, purges/activates, and releases. The second, unblocked afterward, still holds its **stale** `from`/`source` `Service` (built at :793-801, before the lock, from the pre-race `from`) and will purge/report against a profile that may no longer be the actual current one, then unconditionally overwrite `activeProfileName` to its own `to` via `store.activate(&to)` — `activate()` performs no check that the profile it's activating from is still `from`.
- This doesn't corrupt file state (the game lock still serializes the filesystem mutations correctly), but it can silently discard or reorder profile-activation intent between two racing commands, and the second switch's `purged`/`deployed` counts reported to the caller describe a profile that is no longer the actual active one at the time it ran.
- The code comment at `main.rs:806-809` ("One game-scoped lock covers the live-manifest check, the purge, and the activation below") overstates the guarantee: it protects file mutations, not the `from` read.
- Fix: read `active_name()` after acquiring the lock (lock on `target` first, since it's the same file regardless of `source`), so `from` reflects the true current state under the lock.

exchange:
claim: `active_name()` is unlocked and read before the per-game lock is acquired, while `activate()` takes a different, settings-scoped lock
referent: main.rs:783 (unlocked read) vs main.rs:810 (game lock acquired) vs profiles.rs:510-512 (active_name, no lock) vs profiles.rs:196-208 + 513-514 (activate's own `.lock` file under settings root)
checked: read profiles.rs:190-230, 505-528; main.rs:780-812
verdict: holds
confidence: medium-high — the ordering and separate-lock-file facts are directly verified in source; the practical race requires two concurrent CLI invocations, which isn't demonstrated by an actual test in this evidence set (no probe exercises concurrent switches), so the failure mode is derived, not reproduced

### Minor
- `purge_report` (deployment.rs:1423-1431) uses `.collect()` over `.map(uninstall_report)`, so on the first hard `Err` (e.g., the p7 chmod case), any manifests already uninstalled earlier in the iteration are lost from the returned `Vec` — the caller sees the overall command fail but has no report of what was already mutated before the failure. Correct to refuse overall, but partial-progress visibility is dropped.
- `switch-build-receipt.json` only records `"desktop default features"` build/clippy runs; no no-default-features receipt exists anywhere under `docs/parity/wave2-integration/` or `docs/parity/deployment-round4/` bound to `2252894`. The task's validation bullet ("production default/no-default builds") is only half-covered for this head; if a no-default receipt exists elsewhere it isn't cited in the wave2-integration folder and should be treated as missing for this binding, not assumed satisfied.

### Assessment

**Ready to merge?** With fixes.

**Reasoning:** The core lock-scoping fix (285f55d..2252894) is a real, correctly-targeted improvement over the prior per-branch locking, and its "hard purge failure" guard genuinely blocks activation in the case it's designed for. But the fix's safety claim is narrower than advertised: the soft-skip purge path (hash-mismatch / non-regular-file) deletes the manifest and lets `switch` proceed while orphaning an untracked file, and the stated coverage (probe7) doesn't reach that path — worth a targeted fix (add mismatch/non-regular-file entries to `retained`, or add an explicit warnings-based check in the switch handler) before treating "partial purge cannot silently activate" as satisfied. The unlocked `from` read is a lower-severity concurrency gap that should be fixed by reordering (lock-then-read) but doesn't block merge on its own if the team accepts single-invocation usage as the near-term assumption. Neither issue is present in what was actually probed and passing; both are gaps in what wasn't probed.
