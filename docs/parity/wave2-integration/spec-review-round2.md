# Switch/integration specification re-review

Source 714e59b, receipts through 90efdf2; reviewer d63e243a-9ca4-4e03-bcdd-3d61a72978b7.

This empirically confirms the "ordinary uninstall semantics preserved" requirement — p10 shows a mixed purge (one ordinary component, one blocked component) refuses cleanly before mutating anything, leaving both manifests retained ("no_partial_mutation"), rather than partially uninstalling the ordinary one. That matches reading the preflight-blockers-before-any-purge-mutation logic at `main.rs:825-839`.

## SPEC re-review — after 714e59b/90efdf2

claim: partial purge cannot activate target
referent: `main.rs:841-855` — `purge_report()` skipped-flag + live-count `ensure!` at :852-855 runs strictly before `store.activate(&to)` at :865, sequential control flow, no path around it.
checked: read `main.rs:776-865` directly.
verdict: holds. confidence: high.

claim: current-profile read under game lock
referent: `main.rs:790-797` — `target.lock()` at :795 executes before `store.active_name()` at :797.
checked: read `main.rs:776-800` directly.
verdict: holds. confidence: high.

claim: preserve ordinary uninstall semantics (no soft-skip mid-purge, no partial mutation on mixed blocker case)
referent: `main.rs:825-839` preflights `reinstall_blockers` for every live manifest before any purge call; empirical `switch-probes.json` p10 (mixed ordinary+blocked deploy) shows `p10_mixed_purge_refused`, `p10_no_partial_mutation_ordinary_still_present`, `p10_both_manifests_retained` all passed — the ordinary component's manifest and files stayed untouched when the blocked one couldn't be purged.
checked: source read + `switch-probes.json` p10 block.
verdict: holds. confidence: high.

claim: no source changes outside main.rs Switch since 2252894
referent: `git diff 2252894..90efdf2 --stat` → only `src/main.rs` under `src/`; `deployment.rs`/`reinstall_blockers`/`uninstall_report` untouched (confirmed also by their absence from the diff and by prior-round reading of those functions).
checked: command output above.
verdict: holds. confidence: high.

claim: malformed manifests are not universally rejected — `deployed_with_warnings` still warns/skips unreadable manifests rather than refusing
referent: `deployment.rs:514-537` (unchanged, read in prior round) — unreadable manifest → pushed to `warnings`, loop continues; this is a pre-existing, documented limitation, not something this Switch fix claims to close.
checked: not re-read this round (file unchanged per diff stat); relying on prior-round direct read.
verdict: holds, scoped correctly — the worker's report does not overclaim here.
confidence: medium (not re-read this round, but file is provably unchanged so prior reading still applies).

**Verdict: SPEC_COMPLIANT** for the bounded Switch/integration scope. All four required properties (target-lock-before-active-name, preflight blockers before purge, skip/live recheck after purge, activation gated on all of the above) verified directly in source and corroborated by independent-looking CLI evidence (p8–p10), not merely accepted from the receipt summary. No open findings. Ready for the QUALITY re-review.
