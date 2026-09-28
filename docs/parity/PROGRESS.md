# Full Rust functionality progress

Updated 2026-09-28 08:25 UTC. Goal: every F01–F29 requirement in [ACCEPTANCE.md](ACCEPTANCE.md)
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
   direct local transfer proof. Source `efefba1` plus correction `2889ec5`, with
   current evidence `14783d2`, passed independent spec and quality reviews.
   Acquisition checks pass 312/312, production gates pass in both feature modes,
   and 53 frozen test files/regions are unchanged. The controller's separate
   [41-check prior-candidate rerun and narrow final NDJSON probe](fetch-controller-review/README.md)
   retain their respective binary bindings. These counts cover fixtures, not domains.
   The [loopback HTTPS proof slice](origin-loopback-probe-plan.md) implementation at `6c08656` has passed independent source SPEC and QUALITY reviews. Its 53 isolated runtime cases, both production gates and
   frozen-source comparison are retained in [the receipt](origin-loopback-proof/README.md).
   Shared wizard/panel/settings wiring follows;
   persistent sharing and remaining CLI FOMOD behavior have separately approved plans. Standalone torrent verbs
   and library presence do not satisfy those frontend/lifecycle tasks.
3. Fix loader Source normalization. [Seven JSON cases](source-normalization-probe/receipt.json)
   and equivalent [TOML/YAML/XML cases](source-normalization-formats/receipt.json)
   reproduce five differences per format against C#. Expanded evidence reproduces
   the same gap in Markdown embedded YAML and shows an actual bare-relative Copy
   succeeding in C# but rejected by Rust. Rust copies identical bytes after the
   Source is normalized by C#. The expanded implementation plan is independently
   approved; source `62b6f70` and [production evidence](source-normalization-proof/README.md)
   are committed. Independent source SPEC and QUALITY reviews approved this bounded fix. Rust Markdown export and
   its missing legacy-bullet metadata route remain separate approved work.
4. The first [FOMOD integration slice](fomod-remaining-plan.md) is now the sole
   production implementation: CLI policy/dispatch, consumer binding and cancellable
   terminal input. S2a (source commit `0d6aad1`, review follow-up commit
   `93837e5`, round-2 follow-up commit `a436c05` and a round-3 follow-up) adds `--fomod-choices` same-archive reconfiguration, stale-choice
   refusal against the current archive, an actionable C#-configured import
   requirement, a repair hint for unparseable saved choices, and the no-Extract route
   warning; [evidence](fomod-s2a-proof/README.md) is 224/224. The round-3 follow-up is
   `ae8d7e9`; S2a SPEC and QUALITY are [approved](fomod-s2a-review.md). Fetch now
   runs components that may publish one archive file as one serialized group
   instead of a race (commit `4bf5c54`, its review follow-up `339b3d6`, and a second
   follow-up that makes groups transitive and requires the same name for inherited
   failures). A later component takes a file already published under its name
   (subject to its own hash) or the outcome of identical sources for the same name
   without a request; otherwise, for example a different URL after the owner failed,
   it downloads for itself. Every component is reported, recorded and bound as a
   FOMOD consumer. The second follow-up is `3f0caf4`; SPEC and QUALITY are approved.
   A third follow-up, `d4ccbc1` (PR bodencrouch/ModSync.rs#3), makes the two changes
   that review recommended: an unnamed target whose transfer reveals a name already
   present adopts that file under its own hash, or with no hash only when the bytes
   match, and otherwise fails clearly without replacing it (grouped, ungrouped with one
   worker, and on reruns; the Peer publish paths match by source only); and an unnamed
   target inherits only an unnamed member's failure (354/354; 38 new or changed checks
   fail on `45b3c64`). An independent review approved `d4ccbc1` for SPEC and QUALITY.
   The polish follow-up `7ba8b1b` (SPEC and QUALITY approved) addresses that review's five
   non-blocking findings: a race adopts a present file before it stops Origin, and a
   refused Peer adoption lets Origin continue; a refused adoption ends the target only
   when a later source would reveal the same name, else it falls through; a cancelled
   run never adopts; an unnamed Origin publish that loses a no-clobber race adopts
   instead of failing with `File exists`; and the README states the reported source and
   what `origin_denies_peer` does. [Evidence](fetch-shared-url-proof/README.md) is
   381/381 (10 new checks fail on `d4ccbc1`; the race and Fallback changes are
   source-only). Still open here: when Origin finds its name present inside a race, Peer is
   stopped before adoption is decided, so a refused adoption cannot fall back to Peer as
   Fallback mode does (`src/download_orchestrator.rs` ~1624); `adopt_present`'s hash pass
   ignores cancellation; and an unnamed Nexus target whose API-revealed name is
   present fails before transferring, and a known name's publish that loses a
   no-clobber race outside a group still fails with `File exists`. Also open:
   one-run `validate/install --fomod-choices` still replaces all embedded archive
   choices.
   Remaining S2 semantic items and native acceptance follow.
   The [actual full.md conversion baseline](full-markdown-baseline/README.md)
   retains eight production conversions and an independent anonymous comparison.
   Both engines draft 72 instructions for 186 K1 components and 56 for 145 K2
   components. Generated instruction content matches under documented GUID and
   omission rules; the Delete Overwrite export/default distinction is retained.
   These results do not establish complete prose coverage or any installation.
5. Use the [full remaining-scope map](remaining-scope-map.md) to reconcile actual
   source and derive bounded tasks where a current plan is missing: engine actions/
   archives, patchers, per-component checkpoints, hosts/FOMOD, editor/conflicts/
   update/profile UI, complete wizard/main-window controls, CLI contracts,
   diagnostics and packaging. Historical checkpoints do not establish that all
   remaining domains already have complete implementation plans.
   Nexus H1a (shared same-origin API redirect policy, query-encoded nxm keys) has
   [isolated production-CLI evidence](nexus-h1a-proof/README.md), 108/108 checks.
   It still needs independent SPEC and QUALITY review. The https-to-http downgrade
   is covered by source only, and the rest of H1 is still open.
6. Perform requirement-by-requirement acceptance, ending with F29: authorized,
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
