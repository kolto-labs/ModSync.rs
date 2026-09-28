# Final bounded integration spec review

Source `2252894`; reviewer session `d63e243a-9ca4-4e03-bcdd-3d61a72978b7`.

This is consistent with my independent `git diff 4bfacda..HEAD` check showing empty diffs for AGENTS.md and test files (my check used HEAD=2252894, this doc used 285f55d, but my diff covered the superset through HEAD and found nothing either).

## SPEC-COMPLIANCE re-review — after 2252894

**Scope note:** this verifies the bounded wave-2 integration contract (seven lanes + deployment merge + Switch fix), not full F01–F29 parity, which remains partial per `docs/parity/wave2-integration/README.md:26-31`.

**Switch fix — verified independently, holds:**
- Single lock across the whole flow: `main.rs:811-814` acquires `_lock` from whichever of `source`/`target` exists, before any check/purge/activate runs; both point at the same canonicalized `game_dir` so they share one lock file (`deployment.rs:199-208`) — confirmed by reading, not just trusting the comment.
- Source-profile construction errors now propagate (`main.rs:793-801`, `.transpose()?`) instead of the old `else if let Ok(service) = service` fail-open swallow. Corroborated by evidence `switch-probes.json` `p3_corrupt_current_profile_refused` (exit 1, "not found on disk", active unchanged).
- Retained-live-manifest check after purge: `main.rs:824-828` re-checks `live == 0` post-purge and refuses activation otherwise. Corroborated by `p7_failed_purge_refused`/`p7_manifest_retained` (permission-denied purge leaves manifest intact, active profile unchanged, exit 1).
- Activation is the last mutating step (`main.rs:844`), after all read-only target checks (`:841-842`) — a corrupt target can't leave `activeProfileName` pointing at a profile whose report then fails.

claim: Switch now resolves both services, holds one game lock through checks/purge/activation, propagates errors, and checks retained live manifests.
referent: `main.rs:776-851` (read directly) + `switch-probes.json` p1–p7.
checked: `git show 2252894 -- src/main.rs`; current file read at `main.rs:776-851`.
verdict: holds.
confidence: high — code and independent CLI-probe evidence agree.

**Requirement (1) credentials isolation:** still holds, unchanged since prior review.
referent: `git diff 28b32db..HEAD -- src/main.rs` for credential lines → empty.
checked: command run above, no output.
verdict: holds. confidence: high.

**Requirement (3) AGENTS.md/tests untouched:** verified independently, not just trusted from `freeze-check.json`.
referent: `git diff 4bfacda..HEAD -- AGENTS.md` and `git diff 4bfacda..HEAD --stat` filtered for test paths → both empty.
checked: commands run above.
verdict: holds. confidence: high (own diff, not the doc's self-report — doc corroborates but isn't the basis).

**Requirement (2) deployment lane merge / lock / fail-closed:** holds, as previously verified plus this fix closes the one gap flagged in the prior round (Switch TOCTOU + fail-open on source error).

**Requirement (4) docs don't overclaim:** unchanged from prior review, still holds (no new docs claim full parity or working Fetch Downloads/Peer wiring).

**Verdict: SPEC_COMPLIANT** for the bounded wave-2 integration scope. The previously flagged Switch gap is fixed and independently confirmed at the source level, not just from the probe summary. No open findings remain in this scope; full F01–F29 parity is explicitly and correctly not claimed.
