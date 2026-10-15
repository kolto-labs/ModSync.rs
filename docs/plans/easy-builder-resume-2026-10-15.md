# Easy-builder continuation, 2026-10-15

This checkpoint supplements the October 9 brief and October 14 consolidation
handoff. It does not mark the complete easy builder or installation parity done.

## Completed source work

- Correct the production Clippy failure in the CAS emptiness check without
  changing its first-object behavior.
- Cut predicted instructions at the selected dependency-ordered execution point,
  not the component's position in the original array. Excluded targets fail.
- Store ordering policy in a versioned private authoring revision. Reopen,
  undo/redo, and checkpoint matching preserve or compare that policy. Public guide
  exports remain unchanged. Legacy revisions retain their explicit default policy;
  their original import policy cannot be recovered from missing data.
- Make oracle comparison exact by default, freshly hash observed trees, and fail
  closed on invalid inputs or missing checkpoints. Exclusions are explicit and
  reported. See `../../tools/ORACLE_STATE.md` for the complete contract.

## Verification boundaries

- The final source passed production Rust build and strict Clippy in both feature
  modes, the separate Qt build and strict Clippy, and both formatting checks.
- The actual Qt window opened on an owned Xvfb display with software rendering,
  isolated settings, and no QML runtime diagnostics. No game was modified.
- The strengthened oracle reproduced five existing K1 anchors and one existing
  TSL anchor exactly. The ledgers end at steps 59 and 17 respectively. Neither
  anchor reconstruction nor a historical ledger proves a completed manual install.
- Both changes passed independent specification and quality reviews. A final
  integration source review found no blocking interactions.
- No new tests or custom harnesses were created or run. Existing parity restrictions
  remain in force. Production checks are not per-mod installation proof.

## Cloud prerequisites and remaining work

The repository currently has zero registered self-hosted Actions runners. Production
parity CI is manually disabled. Existing synthetic Linux successes use an older
commit and are not real-game evidence. Do not relabel synthetic runs as manual
or automatic full-build parity.

1. Obtain explicit approval before enabling the manually disabled CI workflow.
2. Provision an authorized cloud execution host with legitimate clean game inputs,
   archive access, and the required supplied-patcher execution environment. Do not
   publish licensed game data or private authoring/archive material in artifacts.
3. Follow the pinned `content/k1/full.md` and `content/k2/full.md` manually, in order;
   capture provenance and an independent checkpoint after each applicable step.
4. Author the same actions through easy mode, then compare the automatic built-in
   and supplied-patcher lanes after every mod. Stop and restore before any retry.
   Explicitly disclose exclusions; filtered equality is not exact equality.
5. Complete remaining Qt explorer interaction, accessibility, other application
   workflow migration, and platform validation before replacing the legacy GUI.

The conflicted primary checkout remains untouched. Do not reset it, remove its
worktree, publish a release, or open a pull request as continuation cleanup.
