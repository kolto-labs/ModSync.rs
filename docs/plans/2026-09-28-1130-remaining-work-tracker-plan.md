# ModSync remaining-work tracker

Updated: 2026-09-28. Baseline: main `254b8d1`, downloads GUI step 2 merged in PR #6.

Purpose: one checklist of everything still needed for full Rust parity and the related session commitments. This is a tracker, not an implementation guide. Unchecked means implementation **or acceptance evidence** remains; it does not mean nothing exists. Check an item only when its scope is reviewed, verified and integrated.

## Live implementation reconciliation — 2026-10-02

Continuation is based on the actual ModSync.rs checkout and commits through
`eed8089`, followed by the current working changes. Earlier acceptance remains
bound to its recorded source. None of the rows below closes a full parity domain.

| Area | Current implementation | Remaining work |
| --- | --- | --- |
| Downloads GUI step 3 | Direct Install review/confirmation routing landed in `a4f3cad`; retained static review exists | Remaining native/race/save cases and acceptance |
| Persistent sharing | Catalog/CLI enrollment landed; application lifetime scheduler, observed transport/accounting and asynchronous desktop lifecycle implemented; scoped review corrections and both production feature gates passed | Runtime acceptance |
| Host providers H1–H3 | Nexus discovery/authorization, DeadlyStream anonymous sessions and MEGA decryption landed through `eed8089` | Bounded cancellable Nexus intake and diagnostic URL redaction implemented and reviewed; controlled/live provider acceptance remains |
| Host metadata H4 | Provider normalization, bounded reads, CLI/desktop collection and conservative record integration implemented; static review corrections applied | Provider runtime acceptance; the earlier library-only completion claim was incorrect |
| Markdown M1–M3 | Exporter, strict legacy syntax, centralized review validation and recursive draft accounting implemented; scoped static reviews completed | Cross-format/reference/corpus/runtime acceptance; explicit draft resolution now implemented with retained diagnostics and edit invalidation, source reviewed |
| Engine E1–E4/E7 | Expected-log failure handling, bounded InstallList VFS simulation, external source arrays and argument expansion implemented; shared owned process runner now covers engine execution, with terminal cleanup independently reviewed; E4 owned payload staging and INI preparation implemented and source reviewed; combined production gates passed | controlled backend, external-process and instruction matrix acceptance |
| Patcher discovery E6 | Typed native/Python invocation and packaged-resource lookup implemented; independent source review and both production feature gates passed | Actual resource delivery and native backend/platform acceptance |
| Archive times E5 | Decoder timestamp metadata reaches extraction/copy and managed fallback copies; source reviewed and production gates passed | Cross-format/native timestamp and archive corpus acceptance |
| Checkpoint T1–T6 | Capture/history/journal, opt-in engine boundaries, managed participants, durable finalization, restore/recover/resume, deletion/GC/repair and desktop history implemented; source reviewed and production gates passed | Windows local-NTFS process-interruption library implemented and source reviewed; persistent external-job ownership, guards, explicit recovery and status integrated and source reviewed; temporary engine gate removed; runtime acceptance remains open |
| Desktop FOMOD scope | Captured consumer bindings, targeted edits/dismissal and normalized prefill implemented; independent review and both production feature gates passed | Native acceptance |
| GUI tools F19–F22 | Shared editor/conflict/update libraries and desktop adapters passed the prior production boundary; immutable F20 merge preview/apply and distinct-save adapters implemented and reviewed; bounded resource hierarchy/multi-selection and automatic saved update presentation now implemented and reviewed; combined production gates passed | Native/provider acceptance |
| Profiles/settings F16/F17 | Bounded profile decoder, typed preferences, validated key persistence, private storage/locking and desktop integration implemented and source reviewed; Unix unmanaged case-policy consumer implemented | Resource watcher/migration and desktop NXM preference consumers implemented and reviewed; additional pinned ISO date forms implemented; broader culture/date normalization and native acceptance remain |
| Protocol F23 | Authenticated Unix/Windows transport, package-channel identity, owned status/register/unregister CLI, and native macOS URL receiver implemented and source reviewed; legacy Linux registration delegates to ownership checks; confirmed desktop replacement rechecks the displayed owner under the platform mutation lock | Latest production gates and native platform acceptance |
| Cancellation | Shared Nexus checks and DNS-pinned instruction fetch own cancellable request futures; CLI checks stale guide identity and cancellation before save | Combined production gates and runtime acceptance |
| Packaging F28 | Offline artifact builder, staged-byte receipts, package-bound immutable patcher lookup, platform launchers/resources/install scripts implemented and source reviewed; actual backend provenance inventory recorded and manual producer/workflow source implemented | Target builds, reviewed backend source/license/build bindings, deliverable resources and native acceptance |
| Deployment F18 | Managed engine/checkpoint integration exists; bounded manifest intake and reviewed-inventory management receipts implemented | Confirmed desktop management implemented; production gates and runtime acceptance |
| Diagnostics F27 | Shared CLI/desktop lifetime, consent/configuration receipts, bounded log queue, automatic shared operation records and nonblocking panic telemetry implemented; source review corrections applied | Production gates and consent/transport/lifecycle runtime acceptance |
| Wizard/main window F24/F25 | Shared bounded CommonMark renderer and full component summary cards implemented; desktop integration plus shell/menu/raw-document/keyboard controls implemented; literal notice rendering, reference help links and nonblocking typed log rows now implemented and reviewed; combined production gates passed | Observer suspension/rebaseline invalidation, component-removal confirmation and focused log highlighting implemented and independently reviewed; native acceptance remains |
| CLI F26 | Legacy preload, validation output selectors, conversion/merge acquisition and spoiler overlay, URL filename draft generation, install source default, completed-prefix protection and standalone HoloPatcher implemented; desktop standalone adapter and CLI stored-backend/key provenance implemented and reviewed; combined production gates passed | Invocation/reference acceptance |

T2 cleanup and desktop review corrections have since passed independent source
review. Both production build/strict-Clippy feature modes and formatting passed
at the boundary recorded in
[`production-gates-2026-10-02.md`](../parity/production-gates-2026-10-02.md).
The receipt also records the subsequent T3 opt-in engine/finalization/managed
participant and E5 timestamp production gates, followed by the T4 and desktop
FOMOD scope gates, then the T5/T6 and shared GUI-tools boundary. Subsequent
profile, desktop tool, cancellation, single-instance, telemetry and packaging
changes are not covered by those earlier results. The subsequent integrated
receipt records both Linux feature modes and Windows x86_64 GNU CLI
compilation/linking, including the durable archive dependency correction.
Native execution and parity acceptance remain open.

The user subsequently authorized continuing building/testing/working; the active
repository policy permits production library/binary build and Clippy checks while
freezing Rust tests and custom harnesses. Filesystem test creation/execution and CI
remain deferred. Build success does not establish behavioral acceptance.

## Latest source continuation

The current source additionally includes worker-owned Save Selection, an isolated
raw-document UUID helper, dropped-guide modal dismissal/focus, explicit issue and
sponsor links, empty-selection validation setup errors with reference selector
messages, and editor section expansion controls. Locale-aware settings parsing
uses native symbols and parses before the settings lock; native dataset/parsing
acceptance and the documented Windows ICU/NLS difference remain open. Local
archive instruction generation now has an identity-bound preview/adoption flow;
provider-link filename discovery uses the same guarded preview/adoption flow.

Case-normalization and permission repair now use a shared explicit maintenance
transaction, with CLI and worker-owned desktop preview/apply/status/recover.
Linux source includes bounded identity/content planning, native exclusive rename,
owner permissions, durable progress and verified reverse rollback. Independent
review corrected intent publication, terminal cleanup/recovery, projection bounds,
portable path admission and pending-write fences. Sharing requires explicit resume
after a mutation attempt. The next reviewed source slice adds the Darwin native
maintenance profile, durable mod-root fence, target-dependent terminal cleanup,
shared acquisition locks and current-guide existing-file revalidation. Standalone
CLI/desktop acquisition guards, worker/retry bounds, keyed execution grouping
and direct torrent metadata cancellation are also source reviewed. Windows
maintenance, Unicode mapping and native interruption/acquisition acceptance
remain open. See [the maintenance contract](../parity/maintenance-contract.md)
and [acquisition coordination](../parity/acquisition-root-coordination.md).

Backend packaging now has an explicit offline OdyPatcher producer source with
reviewed root-package/compiler artifact binding, retained source/license/archive
digests and nonhermetic/failure receipts. It has not been executed and does not
populate the empty payload allowlist. Actual producing evidence, dependency
license/source obligations and native bundled backend acceptance remain open.

The next source continuation adds a recorded Unicode 15.0 maintenance mapper,
shared versioned ordinal name matching, bounded keyed planning/discovery and
empty-only stale peer cleanup. Windows maintenance now has a fixed local NTFS
profile with full native identity, default-stream-only admission, unchanged
owner/group/DACL proof, readonly repair, exclusive rename and protected metadata
creation. Source review corrected stream admission and interrupted journal
truncation/append rights. Native execution/interruption remains open. Profile
dates now canonicalize UTC/Unspecified values and have a bounded immutable Unix
local-zone provider. Unsupported forms retain authored text with warnings;
Windows local dates, adjacent-year stitching, complex boundary dates and broad
framework fallback remain named implementation follow-ups. Both Linux production build/strict-lint modes and Windows GNU library/CLI
cross-compilation/linking passed for this source boundary. These gates do not
close the behavioral acceptance rows.

The current continuation adds shared DateTime calendar operations, Unix
neighbor-year marker/ambiguity handling, final-rule replacement and endpoint
lookup, plus bounded Windows native/registry capture and ordinary pure rules.
Both providers preserve authored text for unsupported conversions. Windows
year markers, shifted cross-year windows, extreme daylight rebasing, adjusted
endpoints and native fallback discovery remain named source follow-ups; broad
invariant fallback/rounding is also incomplete. Independent source review
accepted the providers after local negative-daylight ambiguity corrections.

Telemetry now projects admitted base or full-signal URLs into one canonical
OTLP base, preserves encoded prefixes and refuses ambiguous terminal spellings.
The owned first-run prompt persists allow/decline with save revision, readback
and lifecycle-ticket checks; failures stay retryable and event-gated. Logging
and configuration errors are distinct. These are reviewed source changes,
with compilation and native/transport acceptance recorded separately.

Rooted FOMOD phase preparation and Save Selection now resolve archive-shaped
directories from actual members using the engine's source policy, preserving
ordinary folder/glob ownership and future VFS archive scope. Registry ownership,
bounded cancellation, completed-prefix exclusion and atomic overlay publication
received independent source review. Missing registered archives without prompt
state produce a soft warning; recorded state retains its hard error. Rootless
pure preparation is unchanged. Hard-dependency and corpus/native acceptance
remain open; see [the rooted-choice contract](../parity/fomod-root-aware-choices.md).

The install input-parent source default is now reachable from argument parsing,
and -g/-s aliases match the reference. Basic validation without roots now has a
dedicated read-only component report and failed FOMOD stage, independently source
reviewed. Exact-case archive inventories retain entry type and bounded readers;
authored all-false selection is preserved in both validation paths. Automatic
guide/directory fixes and the model's lost null/empty Source distinction remain
explicit differences. JSON setup errors use the report boundary. Convert/merge
default to TOML independent of output extension. Runtime acceptance is deferred.
See [the basic report contract](../parity/basic-validation-without-roots.md).

The shared Nexus getter now imports a bounded private legacy nexusmods.config
under the settings lock when no effective key exists; desktop settings loading
uses the same migration. Microsoft-date acceptance follows the pinned offset
grammar, with authored strings retained. Both changes have independent source
review. Broader invariant/local date normalization remains open.

## Current execution policy

User instruction on 2026-09-28: implement everything before testing or waiting for CI; do not run, edit, or create filesystem tests. Source implementation and independent static review continue. Runtime probes, test work, CI dispatch/polling, and acceptance runs are deferred. Earlier receipts apply only to their recorded source and do not validate later changes. Implementation may advance after static review while the acceptance checkbox remains open.

The user authorized the ModOrchestration workflow correction, which reached default-branch head `66e468a7` before this deferral. Its latest observed CI run reached real installs but failed on fontconfig timestamp stderr; subsequent CI work is deferred. The K2 schema question is prepared only, per the user's contact preference; no author message is authorized.

### Current source continuation on main

The checkpoint `1d8ea0c` added shared pinned merge casing, bounded invariant
full-date fallback with typed local range errors, and Windows rule mechanics.
The next reviewed wave pins UTF-16 punctuation to Unicode 15, corrects the exact
HTTP(S) loopback alias and extends deterministic full-date weekday, AM/PM,
GMT, lowercase ISO separator, numeric dash and hash/NUL syntax. Unix future
rules now validate the effective daylight total without separately limiting
its rebased standard component. Windows native discovery now captures failed
direct lookup, first-match ordered registry search, native construction retries
and qualified UTC fallback through two equal bounded decision observations.
Alternative IANA-ID mapping, broader format/date/URI contracts and all native
acceptance remain open. The production receipt records the actual frozen
source gates. No tests, runtime probes or CI were used; implementation continues
in the sole checkout on `main`.

## Next sequence

- [ ] Finish downloads GUI step 3: peer races/cancellation, window close, completion-page status, Save/record diffs, stale guide/root outcomes, direct Install blocking, remaining sizes/platforms.
- [ ] Implement and verify persistent proxy-only peer sharing, including lifecycle, budgets, restart, Stop/exit and install handoff.
- [ ] Finish host-provider H1–H4: Nexus residuals, DeadlyStream, MEGA, metadata/GameFront contracts and authorized live proof.
- [ ] Finish Markdown M1–M3: export, legacy metadata and real full-guide ingestion/drafts.
- [ ] Implement and verify per-component checkpoints, crash recovery and ordered resume/replay.
- [ ] Finish engine/archive/patcher E1–E7.
- [ ] Close the remaining domain checklist below, then complete F29 and the final acceptance audit.

## Known acquisition and FOMOD follow-ups

The first five acquisition items have working-source changes: verified adoption
retains eligible Peer fallback, planning/hash work accepts cancellation, both
unnamed and known-name publication collisions use typed adoption, and Origin
progress carries HTTP content length (or provider size for MEGA). The checkboxes
remain open for deferred runtime acceptance. They are not instructions to
reimplement those paths.

- [ ] Fix Origin-present race adoption so refusal can still fall back to Peer.
- [ ] Make existing-file hash/adoption cancellation responsive.
- [ ] Handle an unnamed Nexus target whose API-revealed filename already exists.
- [ ] Handle concurrent known-name publication outside a serialized group.
- [ ] Report actual Origin download totals where available.
- [ ] Reconcile residual FOMOD ownership, hard-dependency scope, missing-archive warnings and folder sources named like archives.
- [ ] Verify per-archive embedded/run-choice preservation; reconcile the older replacement warning against merged S2b.
- [ ] Finish desktop FOMOD reconfiguration and S3 semantic/corpus acceptance.

## Full parity checklist

- [ ] **F01 — Formats:** complete format/error corpus, GUIDs, stdin/autodetection and bilateral round trips.
- [ ] **F02 — Markdown:** full/spoiler-free K1/K2 fidelity, metadata/IDs and native paste/import.
- [ ] **F03 — Drafts:** complete direction coverage, archive-generated instructions, ambiguous-draft review and direct-Markdown execution.
- [ ] **F04 — Selection:** remaining desktop actions, nested/Unicode models, dependencies, ordering and shared CLI/GUI selections.
- [ ] **F05 — Actions:** complete real/VFS action, path, wildcard, condition, overwrite and missing-source behavior.
- [ ] **F06 — Archives:** supported formats/SFX, empty directories, large/nested/corrupt inputs, containment and resource measurements.
- [ ] **F07 — Cleanup:** full CleanList matching and texture-priority final-file comparisons.
- [ ] **F08 — Validation:** remaining backend classes, exact reports and native navigation/copy across clean/warning/failure cases.
- [ ] **F09 — Patchers:** packaged discovery, arguments/logs/failures and real patched-resource equivalence across backends/platforms.
- [ ] **F10 — Coordinator:** component boundaries, cancellation, nested Choose, crash durability and best-effort contract reconciliation.
- [ ] **F11 — Checkpoints:** durable storage/provenance, browse/restore, restart/resume and exact ordered replay through CLI/GUI.
- [ ] **F12 — Resources:** stale/cache behavior, complete filename resolution and shared fetch/record metadata.
- [ ] **F13 — Hosts:** complete provider discovery/auth/transfer/update behavior and live acceptance.
- [ ] **F14 — Download queue:** retries, races, progress, cancellation, restart and stale-event acceptance.
- [ ] **F15 — FOMOD:** remaining conditions/navigation/priorities, consumer scope, persistence/recovery and corpus equivalence.
- [ ] **F16 — Profiles:** full schema/text contracts, native CRUD/apply/activate and active-profile/platform integration.
- [ ] **F17 — Settings:** complete typed desktop settings/defaults, theme/game/locale mapping, API-key validation and safe persistence.
- [ ] **F18 — Deployment:** engine/session integration, GUI management, ownership/reinstall/purge and C#/platform interoperability.
- [ ] **F19 — Conflicts:** option/order corpus, attribution/winner reports and remaining-phase desktop preview.
- [ ] **F20 — Merge:** URL/domain heuristics, field/resource/option decisions, dialogs and saved-guide fidelity.
- [ ] **F21 — Editor:** complete option/raw/import/regex/file-browser/edit actions and save/reload equivalence.
- [ ] **F22 — Updates:** file/hash/key UI, cancellation, persisted results/badges and provider acceptance.
- [ ] **F23 — Protocols:** NXM registration, second-instance handoff, cancellation and URI security on supported platforms.
- [ ] **F24 — Wizard:** every remaining onboarding/review/conditional/completion flow, real folder opening and native page acceptance.
- [ ] **F25 — Main window:** legacy controls/menus, rich text/spoilers, keyboard/focus, accessibility, themes and DPI.
- [ ] **F26 — CLI:** complete verb/alias/option/preload/pipeline/stdout/stderr/schema/exit contract comparison.
- [ ] **F27 — Diagnostics:** persisted consent, automatic events, diagnostics UI, authenticated transport and secret redaction.
- [ ] **F28 — Packaging:** native artifacts/resources/patchers, migration, installation/launch and supported-platform release validation.
- [ ] **F29 — Full builds:** K1 and K2 Rust/C#/manual lanes with identical guides, archives and selections; per-step and final-content comparisons; CLI/GUI equivalence and live-game proof.


The next integrated source boundary adds format-specific known-property access
and raw review fences, Unix effective-total offset clamping with conditional
rule reconstruction, qualified HTTP(S) numeric IP merge domains and bounded
System32 ICU alternative Windows timezone IDs. Independent specification and
quality review and both Linux production build/strict-lint configurations passed;
Windows GNU no-default compile/link also passed. See the format-property, merge
and profile-date contracts and production receipt. Behavioral/native acceptance
and the remaining Action, GUID, input/autodetection and IRI source work stay open.


The subsequent source boundary adds the pinned Action enum/getter profile,
shared .NET character GUID parsing for profiles, bounded guide input and terminal
autodetection errors, UTF-8 stdin replacement preservation and qualified Unicode
DNS host canonicalization. Independent specification/quality reviews accepted
all four scopes; both Linux production build/strict-lint configurations and
Windows GNU no-default compile/link passed. Stored guide IDs and checkpoint
proofs are not rewritten. No tests, harnesses, runtime or CI ran. Boolean getter,
JSON nonobject intake, merge GUID comparison and CLI routing remain next source
contracts; behavioral and whole-domain acceptance stay open.

## Integration and external blockers

- [ ] Reconcile historical status prose and newly merged/uncommitted audits against current source; derive bounded tasks where a domain still lacks a current plan.
- [ ] Resolve GitHub Actions billing/spending-limit failure and obtain actual CI results, including the unrun Windows cross-check.
- [ ] Resolve the frozen-test/CI contract at the authorized post-parity point; then repair obsolete fixtures and run the deferred test/all-target checks.
- [ ] Refresh/preserve source-bound evidence and reference inputs before final acceptance; do not reuse old counts as current-source proof.
- [ ] Complete independent specification/quality review and integration for each remaining slice.
- [ ] Resolve the ModOrchestration CI-change permission blocker and obtain reference CI evidence.
- [ ] Complete ModOrchestration duplicate-sweep dry-run parity before restoring its wiring.
- [ ] Resolve the mod-builds K2 schema question about the required `main` section with its author.
- [ ] Reconcile and close every acceptance-ledger row and documented compatibility difference.

## Completion rules

Preserve the dirty primary checkout and AGENTS.md. Keep the Rust-test freeze until its stated release point. Use isolated production/native evidence meanwhile. Full installs follow literal full.md order: stop, restore before the failed step, fix, then replay. Preserve existing archives and live baselines. Authentication, CAPTCHA and permission blockers remain explicit. Release publication remains a separate manual action.

Done means every applicable item above has evidence and all F01–F29 acceptance requirements are met; compilation or a narrow successful slice is insufficient. Newly discovered gaps join this checklist.

## Detailed sources

- [Acceptance ledger](../parity/ACCEPTANCE.md), [progress](../parity/PROGRESS.md), [remaining-scope map](../parity/remaining-scope-map.md), [latest handoff](../handoffs/rust-parity-cloud-2.md).
- [Downloads](../parity/downloads-gui-plan.md), [step-2 evidence and residuals](../parity/downloads-gui-step2/README.md), [persistent sharing](../parity/persistent-sharing-plan.md), [providers](../parity/host-providers-plan.md), [FOMOD](../parity/fomod-remaining-plan.md).
- [Markdown](../parity/markdown-remaining-plan.md), [checkpoints](../parity/checkpoint-plan.md), [engine/archive/patchers](../parity/engine-archive-patcher-remaining-plan.md).
- [Selection](../parity/mod-selection-plan.md), [GUI tools](../parity/gui-tools-plan.md), [wizard/main window](../parity/wizard-mainwindow-plan.md), [platform/protocol/packaging](../parity/platform-protocol-packaging-plan.md).
