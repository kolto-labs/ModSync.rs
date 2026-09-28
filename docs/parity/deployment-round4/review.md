Confirmed: `src/engine.rs` has zero references to the deployment module — it's unrelated to this diff (desktop/CLI install engine vs. deployment/manifest ownership logic). No gap there.

Now finalizing the review.

---

## Review: deployment round 4 diff (src/deployment.rs)

**Verdict: Approve** — no concrete defects found. All checks are static/reading only, per the read-only constraint; no build/test/probe run (`checked: not checkable here` for any runtime claim below).

### Sequence checked
1. `ensure_unowned_or_self` flip, `is_none_or` → `is_some_and` (deployment.rs:264) — deployment.rs:227-241 (`ensure_exclusive_game`) calls `ensure_own_game` then `adopt_owner_record_if_unowned` then peer loop then `ensure_unowned_or_self`.
2. `ensure_own_game` marker-less path rewritten to a live-owned (newest-wins) map (deployment.rs:311-380).
3. `adopt_owner_record_if_unowned` moved out of `ensure_own_game`, gained its own `has_manifests` guard (deployment.rs:391-413), and callers re-wired (deployment.rs:227-228, 1221-1222).
4. Cross-check against `docs/parity/deployment-probe.sh` P25c/P29/P33/P34/P35, and `foreign_index`/`later_owner`/`relative_parts` for separator handling.

### Findings

1. **`is_none_or`→`is_some_and` at deployment.rs:264 — correct.** `owner_record_path` is already keyed per-`self.game`, so any recorded owner there is, by construction, scoped to this game directory; a marker-less owner should still block (previously it didn't — the marker-less partial-match gap this round claims to close). The new condition blocks unless the owner's own marker names a *different* game, which only happens if that root was later repurposed — a legitimate release case. Matches P29/P33 in deployment-probe.sh:334-399. `checked: file:line + probe script exists, not executed here.`

2. **`ensure_own_game`'s marker-less "live-owned" dedup (deployment.rs:340-377) — correct, and it's a genuine improvement over the prior "every entry ever written" check.** `deployed()` (deployment.rs:535) sorts oldest-first; the `BTreeMap::insert` in ascending manifest order means the last write per normalized path key is the newest entry, matching the comment's stated intent (superseded entries from an ordinary earlier-component/later-component conflict, as in P5/P6/P21/P26, no longer force a refusal). This directly targets P32/P35 in the probe.

3. **Backslash+lowercase normalization at deployment.rs:345 — checked for spillover into the mutating path, none found.** `relative_parts` (deployment.rs:1742-1755) independently normalizes `\`→`/` at the point every entry is actually resolved (`locate`, backup path construction), so the dedup key choice in `ensure_own_game` only affects which paths collapse for the *acceptance gate*, not how `uninstall_report`/`purge_report` later resolve or hash-compare paths. `foreign_index` (568-587) and the two other `owned` maps (928-933, 1511-1516) lowercase but don't swap separators — consistent, since those only run against manifests this same Rust build wrote (which are already forward-slash on this platform); the backslash case is specific to legacy/foreign manifests, which is exactly what P35 exercises. I don't see a path where the widened acceptance from this key normalization causes uninstall/purge to mishandle a live file — the actual data-mutating comparisons are separanormalized already. Non-blocking.

4. **`adopt_owner_record_if_unowned` re-homed correctly.** Now guarded by its own `has_manifests(&self.root)` check (deployment.rs:392-394), matching P34's "no-op/failed deploy does not claim ownership" (deployment-probe.sh:401-409). Callers: `ensure_exclusive_game` (deploy/record/purge-via-uninstall paths) and `uninstall_report` directly (deployment.rs:1221-1222) both cover it; `purge_report` (1424) and `prepare_reinstall` (1181-1198) reach it transitively via `uninstall_report`; `reinstall_blockers` stays read-only as its doc comment now states (307-310), and is never itself a mutation entry point.
   `checked: prepare_reinstall (1181-1198) → uninstall_report (1219-1222); purge_report (1418-1431) → uninstall_report per manifest.`

5. **Engine.rs is not implicated.** `grep -i deployment src/engine.rs` returns no matches — this diff's surface is entirely within `src/deployment.rs`; AGENTS.md's "installation behavior in engine.rs" is a separate concern this diff doesn't touch.

6. **Locking.** Every CLI entry point that reaches these functions takes `service.lock()` first (main.rs:496, 503, 510, 533, 558) before calling `deploy_report`/`uninstall_report`/`purge_report`/`record_live`, so the doc comments' "runs under the caller's game lock" claim holds for the shipped CLI paths I can see. `checked: main.rs:490-538, not run.`

### Non-blocking observations (speculative / pre-existing, not introduced by this diff)
- `ensure_unowned_or_self`'s "free" definition (marker names a different game) and `adopt_owner_record_if_unowned`'s "free" definition (no manifests) diverge; reaching a state where they disagree requires a stale owner record left behind by `release_owner_record_if_empty`'s swallowed `let _ = fs::remove_file` (deployment.rs:298) — legacy corruption scenario, not exercised by this diff.
- `adopt_owner_record_if_unowned` runs before the peer-profile and cross-root checks in `ensure_exclusive_game` (228 vs 229-239), so a command that's ultimately refused can still have claimed the owner record first. Same ordering existed before this diff (adoption used to run inside `ensure_own_game`, called at the same point) — not a regression.

No required fixes identified from this diff in isolation.
