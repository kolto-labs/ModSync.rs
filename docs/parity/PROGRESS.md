# Full Rust functionality progress

Updated 2026-09-28 14:20 UTC. Goal: every F01–F29 requirement in [ACCEPTANCE.md](ACCEPTANCE.md)
verified against pinned C# `d31ccbd3`, including identical K1/K2 full installs
against C# and manual references. A delivered slice does not close a whole domain.

**Overall: incomplete.** F01–F28 are partial; F29 is unverified. No percentage
is assigned because the remaining domains differ greatly in scope and proof cost.

## Current working source — 2026-10-02

Latest source continuation adds worker-owned Save Selection with bounded remaining
FOMOD overlay and stale-result guards, raw UUID generation/copy, drop-dialog
cancellation/focus, explicit Help issue/sponsor links, and ordinary validation's
empty-selection setup-error boundary. See the GUI-tools desktop receipt and
production-gate log for implementation/review versus compilation evidence.
Locale-aware settings parsing and editor section controls/local-archive
generation preview are also implemented; see the numeric and GUI-tools receipts.
The install source fallback and -g/-s aliases are corrected. The combined source
passes both Linux production feature configurations, strict Clippy, formatting and
the Windows GNU CLI build/link boundary recorded in the latest production receipt.
The subsequent rootless basic-validation adapter now produces a bounded read-only
component report and failed FOMOD stage, with independent review corrections for
authored selection, JSON setup errors and explicit archive directory membership.
Convert/merge TOML defaults, private legacy Nexus-key import shared by CLI/desktop,
and Microsoft-date grammar are also implemented and source reviewed. Production
checks for this combined boundary passed in both Linux feature modes; older
Windows gates do not cover it.
The source also implements explicit Linux maintenance through shared CLI/desktop
preview, apply, status and reverse recovery, with owned workers and manual sharing
resume. Independent review corrected interruption, persistence, path and work
bounds. Root-aware FOMOD choice attachment now handles real archive-shaped
directories without dropping ordinary folder or future VFS archive ownership.
Both slices are reviewed and passed the same production gates. Native platform,
cross-process acquisition fencing, dependency/corpus and behavioral acceptance
remain open. See the maintenance and rooted-choice contracts. Acceptance rows
remain open.

The subsequent source slice adds a durable mod-root maintenance fence and
target-dependent terminal cleanup, shared acquisition/root locks, current-guide
revalidation of existing-file skips, standalone output guards, bounded worker and
retry settings, cancellable keyed execution grouping and direct torrent metadata
cancellation. Independent review corrected stale claims, matching work bounds,
GUID identity, session cleanup and the game-only terminal cleanup window.
The Darwin maintenance source profile now uses pinned native ACL/flag checks and
exclusive rename, with stable capability admission preserving old Linux records.
Native execution is unverified. An offline OdyPatcher producer source now binds
the exact root package, source/license and emitted executable to a retained
receipt; it has not been invoked and the backend allowlist remains empty.
See the acquisition, maintenance and package contracts and current production
gate receipt. Windows maintenance, Unicode mapping, broader date normalization,
actual backend provenance and all domain/native acceptance remain open.

The next source continuation records the exact Unicode 15.0 maintenance mapper
and preserves legacy ASCII intent admission. Shared FOMOD/selection/authoring
name keys use a pinned simple-uppercase table with the .NET 9 ICU ordinal
exceptions. Planning, discovery and execution now share keyed cancellation/work
bounds; stale peer cleanup preserves nonempty contents and rejects Windows
reparse points. These corrections have independent source review.

Windows maintenance now has a local fixed NTFS source profile with full native
identity, unchanged owner/group/DACL proof, default-stream-only admission,
readonly repair, exclusive handle-relative rename and protected metadata at
creation. Independent review corrected named-stream proof, interrupted journal
truncation rights and checked EOF append semantics. Native acceptance remains
open. Profile date source now canonicalizes UTC/Unspecified values and includes
a bounded immutable Unix local-zone provider; unsupported local rules retain
authored values with warnings. Exact remaining provider/fallback forms are
listed in the date contract. Both Linux production build/lint configurations
and Windows GNU library/CLI cross-compilation and linking passed for this
reviewed boundary; see the production gate receipt. Native behavior remains
unverified.

The subsequent source continuation adds shared DateTime calendar mechanics,
Unix neighboring-year marker seasons, historical/future seam selection, daylight
day transfer and endpoint lookup. Windows now has bounded immutable native/
registry capture and a pure ordinary adjustment-rule engine. Windows year markers,
shifted cross-year windows, extreme delta rebasing, adjusted endpoints and native
fallback discovery remain explicit follow-ups. Unsupported conversions retain
authored text with bounded warnings. Both local providers received independent
review corrections for negative-daylight local ambiguity; their distinct UTC
intervals are unchanged.

Telemetry source now shares canonical OTLP endpoint admission across readiness
and transport, fixing migrated C# full-signal URLs without doubling paths.
The desktop has an owned first-run consent prompt that persists both choices
through matching save/lifecycle receipts, checks readback and retains retries
on failure. Logging errors are separate from configuration errors. Independent
source review accepted this slice; both Linux build/strict-lint modes and
Windows GNU production compilation/linking passed in the current gate receipt.
No native/transport acceptance is inferred from source review.

The next reviewed source wave shares compiled pinned Unicode casing across
maintenance, FOMOD and merge, preserving recorded table bytes and descriptors.
Merge name/author/instruction/option keys use simple invariant lowercase;
destination/argument comparisons and Language union use ordinal casing.
Profile fallback now admits bounded complete ISO, slash and English month-name
forms, with binary64 nearest-even fractions and distinct typed fallback-local
range handling. Windows rules now cover year markers, neighboring seasons,
cross-year fold retries, single-day rebasing and endpoint lookup. Native fallback
discovery and broader invariant grammar remain named implementation follow-ups.
Independent source review accepted this wave. Production evidence is recorded
in the gate log; full domains and runtime acceptance remain open.

Continuation uses the actual `ModSync.rs` checkout through `eed8089` and its
working changes. The [remaining-work tracker](../plans/2026-09-28-1130-remaining-work-tracker-plan.md)
is the current source reconciliation. The sections below retain historical
commit-bound evidence and must not be treated as a current missing-source list.

Recent reviewed source includes explicit draft resolution, filename-based
inference, shared conversion/merge/download composition, completed-prefix
protection, active-profile execution binding, theme/game presentation context,
visible bounded FOMOD warnings, Unicode selection and bounded guide ingestion.
Owned standalone backend execution is implemented for Linux/macOS/Windows;
engine and desktop integration are implemented and source reviewed. Nexus
acquisition uses bounded cancellable readers, race-token-aware client waits and
URL-safe diagnostics; desktop patcher settings use matching engine/path receipts.
Native assets and packaging
production sources are prepared with provenance; no package was produced here.

Subsequent reviewed corrections invalidate readiness across resource-observer
pauses/rebaselines and reject failed or mismatched packaging receipts. The
desktop NXM preference now has a reviewed owned worker and settings consumer,
including guarded replacement confirmation and independent preference disable.
Further source corrections add component-removal confirmation with busy/focus
guards, focused structured-log highlighting, additional pinned ISO date spellings
and case-insensitive validation output/backend selectors. The combined source
passed both Linux production feature gates, formatting and the Windows x86_64
GNU CLI compilation/link boundary; native acceptance remains open.

All F01–F28 acceptance rows remain partial and F29 remains unverified. The user
requires implementation before tests/CI and prohibits filesystem-test creation,
editing and execution. Source review does not establish runtime acceptance.
The [production gates](production-gates-2026-10-02.md) now include both feature
configurations for CLI composition, owned process execution, bounded Nexus intake
and desktop bindings. They cover only their named source boundaries. The latest combined boundary additionally covers Windows checkpoint library and
persistent external-job integration, staged patcher INI preparation, resource
hierarchy/multi-selection, saved update rehydration, nonblocking typed logs and
CLI preference/key provenance. Linux production build and strict Clippy passed
in both feature modes. Windows x86_64 GNU CLI production compilation/linking
also passed with the documented production-only unrar_sys target correction.
Native Windows execution, the separate GUI launcher, package delivery and all
behavioral acceptance remain unverified.

## Historical work snapshot — 2026-09-28

- Current continuation starts from merged downloads GUI step 1, `d438528` (PR #5).
  Step 2 adds peer-settings disclosure/preserving saves, a bound serial FOMOD queue,
  error retry/dismiss ownership, readiness guards and responsive navigation.
  Independent source reviews are in [the step-2 review](downloads-gui-step2-review.md);
  production/native evidence is tracked in [the step-2 proof](downloads-gui-step2/README.md).
  The bounded step-2 slice is accepted: five production gates, 46/46 native
  assertions across five isolated scenarios, and 53 unchanged frozen files/regions.
  Next: the remaining downloads step-3 native matrix, then persistent sharing.
  Earlier in-flight statements below are historical to their recorded commits.
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
   Shared wizard/panel/settings wiring follows. [Downloads GUI](downloads-gui-plan.md)
   step 1 is implemented but not reviewed: commit `6ac257f` plus an uncommitted
   review follow-up. It covers the
   desktop job adapter (Fetch, the nxm handoff through the shared orchestrator,
   run-bound progress, Stop, record-on-clone and redaction) and safe probe/settings
   initialization. [Native Xvfb proof](downloads-gui/README.md) passes 46/46 checks
   in an isolated network namespace, including a same-fixture CLI comparison. The
   regression probes still pass (S2b 254, S2a 224, S1 339, inherited 41, H1a 138,
   fetch-shared-url 381). Next: its SPEC then QUALITY review, then step 2 (settings,
   guarded FOMOD queue, navigation) and step 3 (native acceptance). Persistent
   sharing and remaining CLI FOMOD behavior have separately approved plans. Standalone torrent verbs
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
   `ae8d7e9`; S2a SPEC and QUALITY are [approved](fomod-s2a-review.md). S2b
   (commit `37cd4cc`, review follow-up `c28dee0` that dedupes and lists gate
   archives, restores C#'s exact fallback text, adds iota-subscript case mapping and
   scopes run choices to referencing components, and second follow-up `6dfbf63` (SPEC and QUALITY approved)
   that matches that scope to engine source resolution so folder and glob
   character-class Extract sources keep their choices; completed components, and so a
   completed component no entry references, are never touched; second follow-up not
   yet reviewed) makes the run-scoped
   `validate`/`install --fomod-choices` Rust extension merge per archive, matches
   FOMOD names like C# `OrdinalIgnoreCase` including Unicode, runs the FOMOD gate on
   plain `validate` and `install`, and logs C#'s mapper fallback warnings;
   [evidence](fomod-s2b-proof/README.md) is 254/254 (19 of 232 fail on `37cd4cc`; 10
   of 254 fail on `c28dee0`). Fetch now
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
