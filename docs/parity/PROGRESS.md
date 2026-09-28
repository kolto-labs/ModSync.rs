# Full Rust functionality progress

Updated 2026-09-28 03:48 UTC. Goal: every F01–F29 requirement in [ACCEPTANCE.md](ACCEPTANCE.md)
verified against pinned C# `d31ccbd3`, including identical K1/K2 full installs
against C# and manual references. A delivered slice does not close a whole domain.

**Overall: incomplete.** F01–F28 are partial; F29 is unverified. No percentage
is assigned because the remaining domains differ greatly in scope and proof cost.

## Current work

- Wave 1 is recorded at `origin/main` `4bfacda`. Wave 2 is local on
  `codex/rust-parity-wave2-resume` in the isolated integration worktree.
- Seven earlier approved lanes are merged: P2P proxy core, profiles/settings,
  download library, texture priority, loader deduplication, completion CLI,
  and completion-page UI. Deployment round four is merged as well.
- Combined production builds and Clippy pass in CLI-only and desktop modes.
  All 82 deployment CLI checks pass against the combined binary.
- Twelve focused CLI assertions cover an actual disposable install, peer-staging
  exclusion, completion summaries, and API-key/endpoint merge behavior. Native
  Xvfb proof covers startup/preload rendering only. Earlier completion-page
  native evidence keeps its original source/binary binding.
- Final integration spec review identified profile-switch locking and ignored
  source-profile errors. Commit `2252894` fixes these; seven direct production
  CLI scenarios pass, including held locks and failed-purge refusal. Spec
  re-review passed. Quality review reproduced a different soft-skip case where
  modified files lose their manifest and switching proceeds; the follow-up
  uses existing read-only blockers before any purge and moves the active-profile
  read under the game lock. Correction `714e59b` plus receipts `90efdf2` passes
  all 39 switch CLI assertions and both production feature gates. Spec re-review
  and quality re-reviews pass for the bounded integration.
  Wave 2 has not been pushed.

[Evidence index](wave2-integration/README.md) ·
[Deployment receipts](deployment-round4/build-receipt.json) ·
[Unchanged frozen tests](wave2-integration/freeze-check.json)

## Next implementation sequence

1. Wave-two integration and profile-switch findings are reviewed and recorded
   locally; maintain their source-bound receipts as later slices land.
2. Implement the [approved selected-guide CLI slice](fetch-downloads-cli-plan.md):
   orchestrator gate hardening, a concrete peer adapter, `fetch-downloads`, and
   direct local transfer proof. Candidate source `efefba1` and receipts `54383a8` are committed locally;
   independent spec review is active, with quality review still required.
   Recorded acquisition checks pass 305/305, production gates pass in both feature
   modes, and 53 frozen test files/regions are unchanged. The controller's separate
   [41-check rerun](fetch-controller-review/README.md) confirms the all-false selection
   and record fixes. These counts describe covered fixtures, not domain completion.
   Follow with the [loopback HTTPS proof slice](origin-loopback-probe-plan.md),
   then advanced FOMOD modes and shared wizard worker/panel/settings wiring.
   The library and standalone torrent verbs do not satisfy those frontend tasks.
3. Fix loader Source normalization. [Seven JSON cases](source-normalization-probe/receipt.json)
   and equivalent [TOML/YAML/XML cases](source-normalization-formats/receipt.json)
   reproduce five differences per format against C#. Expanded evidence reproduces
   the same gap in Markdown embedded YAML and shows an actual bare-relative Copy
   succeeding in C# but rejected by Rust. Rust copies identical bytes after the
   Source is normalized by C#. The expanded implementation plan is independently
   approved; the production loader fix has not started. Rust Markdown export and
   its missing legacy-bullet metadata route remain separate open work.
4. Use the [full remaining-scope map](remaining-scope-map.md) to reconcile actual
   source and derive bounded tasks where a current plan is missing: engine actions/
   archives, patchers, per-component checkpoints, hosts/FOMOD, editor/conflicts/
   update/profile UI, complete wizard/main-window controls, CLI contracts,
   diagnostics and packaging. Historical checkpoints do not establish that all
   remaining domains already have complete implementation plans.
5. Perform requirement-by-requirement acceptance, ending with F29: authorized,
   isolated K1/K2 installs with identical inputs/selections, ordered per-step
   comparisons, final resource content checks, and live-game evidence.

## Rules and unresolved proof boundaries

- Fresh implementer per task; specification review before code-quality review.
- No Rust tests or temporary Rust harnesses created, changed or run until full
  parity. Production compilation is not all-target/test/CI acceptance. The
  inherited frozen UI test error and CI test-execution conflict remain open.
- Preserve the dirty primary checkout, AGENTS.md, live games/baselines and
  other agents' work. No new live-host downloads in acquisition fixture work.
- Keep authentication, CAPTCHA and free-user wait boundaries explicit; peer
  copies never bypass them. Proxied peers fail closed without a usable proxy.
- No full-build evidence or cross-platform artifact acceptance exists for this
  integration. Current machine-local paths and tmpfs evidence must be refreshed
  or preserved before relying on them in the final acceptance run.
