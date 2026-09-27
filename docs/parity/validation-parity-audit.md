# ValidatePage parity implementation specification

Audit date: 2026-09-27. Source-only audit; no build, native launch, install, or product edits performed.

## Bound sources and ownership

C# oracle: `/home/brunner56/.codex/worktrees/5813/ModSync`, HEAD `d31ccbd3bfcddb4705917eafa6483715c166de3b`, clean status at inspection. Rust: `/home/brunner56/Workspaces/ModSync.rs`, HEAD `915ec69a21d90fac0395932d54e1622f8c049a18`; existing modified `AGENTS.md` is user-owned and untouched. Only this audit file is owned by this task.

Applicable guidance read: supplied C# AGENTS instructions, Rust AGENTS.md, `/home/brunner56/AGENTS.md`, `/home/brunner56/.agents/AGENTS.md`, C# `.github/copilot-instructions.md`, `docs/local_desktop_agent_runbook.md`, and `docs/knowledgebase/gui-validation-surfaces.md`. The local archive exists; no private archive content was needed for this source comparison. Memory only supplied the oracle/check-current-state direction; current files establish the findings below.

Source shorthand below resolves relative to the bound checkout:

- **X**: C# `src/ModSync.GUI/Dialogs/WizardPages/ValidatePage.axaml`.
- **V**: C# `src/ModSync.GUI/Dialogs/WizardPages/ValidatePage.axaml.cs`.
- **P**: C# `src/ModSync.GUI/Services/WizardValidationStagePresenter.cs`.
- **M**: C# `src/ModSync.GUI/Services/ValidationPipelineDialogMapper.cs`.
- **H**: C# `src/ModSync.GUI/Controls/WizardHostControl.axaml.cs` (active host, not InstallWizardDialog).
- **U**: Rust `src/ui.rs`; **R**: Rust `src/validation.rs`; **E**: Rust `src/engine.rs`.

## Finding and scope

Route integration is present, but `Page::Validate` still delegates to generic `surface(..., 2)` (U:572,1182-1363). That surface exposes trust/FOMOD/scratch/patcher/recovery/conflict utilities and a collapsed JSON report, instead of the C# ValidatePage hierarchy. Keep those capabilities reachable through existing utility/settings surfaces while giving Validation its own presentation; do not erase working shared-engine operations to imitate a screenshot.

The worker already calls `validate_report_with_dry_run(..., true)` or `validate_report_remaining` (U:2157-2167). E:187-256 owns stages and selection; CLI uses the same report APIs (`src/main.rs`:1060-1083). Retain this boundary. Presentation must consume shared reports/events; it must never independently rerun checks, mutate game/mod trees, run external programs, or infer readiness from rendered text.

## Control, layout, interaction and state matrix

| Requirement | C# authority | Current Rust gap / implementation acceptance |
|---|---|---|
| Active host | H:167,735-799; host XAML:14-110 | Keep embedded host, shell title `Validation`, subtitle `Validating your mod selection and installation environment` (V:78-80), download sidebar and footer. Do not copy the older dialog geometry/sidebar. Host wraps page in a ScrollViewer; verify actual nested scrolling natively. |
| Page grid | X:9-171 | Six rows `Auto,Auto,*,Auto,*,Auto`: heading, action, log, splitter, results, summary. U:1182-1363 is a single utility flow. Dedicated renderer must preserve independent log/results panes and footer. |
| Heading | X:11-23 | Padding 20×16, radius 10, bottom margin 12, stack gap 8; 20 bold `🔍 Pre-Installation Validation`; 14 / opacity .85 wrapping `We'll check for potential issues before starting the installation to ensure everything works smoothly.` |
| Action | X:26-49 | Padding 16×12/radius 8/bottom margin 8. `🔍 Run Validation`, padding 16×10, tooltip `Check for compatibility issues and missing files`; right-aligned 200×8 indeterminate progress and 14 semibold status, gap 12. Existing `Run validation` and global spinner do not match. |
| Log | X:52-118 | Initially hidden/collapsed; min height 100, padding 12, stretch. Header `📋 Validation Log` (13 semibold), progress (12/.7), first-issue, copy, `Drag the splitter below to resize` (11/.6). Bordered inner pane: thickness 1/radius 4/padding 12/top margin 8/min 80; vertical auto, horizontal disabled; wrapped 11 Consolas/Courier New/monospace. Global Activity is not this log. |
| Splitter | X:120-128 | Height 6, full width, row resizing with preview; initially hidden; tooltip `Drag to resize the validation log`. Need persistent drag state and separate pane bounds. |
| Results and cards | X:131-138; V:574-611 | Results container padding 12/radius 8/min 80 with own vertical scroller and gap 10. Each card padding 16×12/radius 8/bottom margin 8, internal gap 6; title 14 semibold, message 13/.85/wrapped. U:1342-1360 colored labels lack card composition. |
| Footer | X:140-170 | Padding 20×16/radius 10/top margin 12. Left summary 16 bold and details 13/.7/wrap, gap 8; right three columns, gap 16, each label 11/.6 uppercase `ERRORS`, `WARNINGS`, `PASSED` and count 20 bold. Counters come from report; never count displayed cards. |
| Entry/empty selection | V:82-127 | Selected >0: `Click 'Run Validation' to begin`; `Ready to validate {selected} of {total} mod(s). Click Run Validation to check your setup.` Empty: `Select mods before validating`; `No mods are selected. Go back to Mod Selection and choose mods to install.` Run disabled when empty. Existing utility run button has no equivalent per-control state. |
| New run | V:286-359 | Disable Run, show progress/log/splitter, status `Running validation...`, clear cards/entries/log/counters/header operation, hide copy and first-issue, reset both scroll offsets. Append `Starting validation...`. Do not wipe unrelated global Activity. Existing U:2010-2015 clears report/readiness but has no page-local lifecycle. |
| Progress and log timing | V:171-260,376-394 | Pipeline callback carries stage, step, total, operation; header shows rounded percent, operation, ETA (`s` under 60 seconds, otherwise one-decimal `m`) separated by ` • `. Completion sets current step to total and clears operation. Log flush is batched; page log includes presenter lines plus completion duration and passed/failed line. E:187-256 only forwards ordinary component events during component validation: needs shared typed stage progress, not fabricated UI percent. |
| Finish pass/warn/fail | V:396-434,632-663 | Hide progress/re-enable Run; show Copy after completed validation; first-issue iff aggregate errors or warnings. Expand log for warnings/errors, collapse clean pass. `✅ Validation Passed` / `❌ Validation Failed`; status `Validation complete` / `Validation failed`. Exact details: clean `Everything looks good! You're ready to proceed with the installation.`; warn `Validation passed with {n} warning(s). These will be handled automatically during installation.`; fail `Please resolve all critical errors before continuing. Check the results above for details.` Copy warning text only when shared behavior actually supports the claim; otherwise track semantic blocker, not silently invent handling. |
| Report copy | V:665-733 | `Copy report` briefly becomes `Copied!` for 1500ms. Flush pending page log, produce `ModSync — Validation Report`, UTC generated line, summary/detail, `Errors: {e}  Warnings: {w}  Passed checks: {p}`, blank lines, `--- Results ---` with every displayed title/message, `--- Log ---` full page log. U:1328-1332 copies JSON; global copy copies Activity. Neither satisfies this contract. Keep JSON only as separately labeled utility export. |
| First issue | V:454-569 | Flush before action; expand log; independently find first ERROR:/❌ line and first ❌ card, falling back to WARNING:/⚠ and ⚠️ card. Log offset clamp uses 15px line estimate minus 30px lead; card BringIntoView plus 2px border #E53935/#FB8C00. U:1335-1358 merely scrolls one message and appends Activity. R:133-140 picks first message of failed/warned stage; it can select a non-error preamble or return None for an issue stage without messages. Use mapped card/log indices, not this helper as sole authority. |
| Navigation/readiness | V:109-127; H:772-773,820; WizardPageBase.cs:21-23 | C# host leaves Next enabled by default; click invokes validation guard with missing-selection / unvalidated / critical-error text. Rust proactively gates Next (U:311-331). Record this deliberate behavioral difference; do not assert exact disabled-state parity. Preserve stale-input invalidation and fail-closed shared readiness. |
| Theme and keyboard | X plus active theme resources | Default egui colors/fonts are not equivalent to C# implicit styles. Port material tokens from Light/K1/K2; verify hover, focus, disabled, wrapping, glyph fallback, keyboard activation and accessible labels natively. Source `FocusFirstValidationIssue` scrolls/highlights but never explicitly sets keyboard focus; do not promise focus transfer without observing it. |

## Exact stage/card mapping

Apply in report stage order, with one-based step log headings. Pure shared-library presentation helpers may map reports to ordered cards/log lines and a plain report without owning validation rules. Both frontends can consume those helpers; CLI JSON remains stable.

| Stage | Mapping to port | Authority |
|---|---|---|
| Environment | Log `Step N: Validating installation environment`, each message indented two spaces. Parse prefixed warning/error cards. Passed adds `✅ Environment`; failed adds `❌ Environment Error` only if zero prefixed cards, preventing duplicate failure summary. | P:76-106 |
| Conflicts | Log `Step N: Checking conflicts for {selected} selected mod(s)` plus messages. Prefixed cards first, then `✅ Conflicts`, `⚠️ Conflicts`, or `❌ Conflicts` using passed/warnings state and stage summary. Preserve empty-message log fallback. | P:108-145 |
| Install order | Log `Step N: Validating mod installation order`; prefixed cards then passed/warn/failed `Install Order` aggregate; exact fallback messages are in method. | P:147-183 |
| Archive/component | Log `Step N: Validating mod archives`; `OK:` lines become indented ✅ lines. Other messages retained, ERROR:/WARNING: mapped. Add failed/warned `Archive Validation` aggregate, **no successful archive aggregate card**. | P:185-216 |
| FOMOD | Log `Step N: Checking FOMOD configuration`, messages/prefixed cards, failed or passed `FOMOD Configuration` aggregate. C# passed branch uses ✅ even with HasWarnings; faithfully map result while keeping severity metadata. | P:218-240 |
| Dry-run | Log instruction execution validation heading. Missing dryRun -> no card. Clean -> `✅ Instruction Execution` / `All instructions validated successfully. Dry-run completed without errors.` Errors take priority: first five Error/Critical issues only; warnings shown only when no errors, first five. Title `{❌/⚠️} {component or Unknown} ({category or Validation})`; message + ` — {solution}` when present. Then aggregate with error/warning count and ` Showing first {shown} of {total}.` only on overflow. Informational-only anomalies require fixture comparison; do not add extra cards. | P:242-354 |
| Prefix/solution rules | Ordinal case-sensitive prefix match; strip prefix/trim; mod name is detail before first colon when colon index >0, otherwise Unknown; card message is **whole stripped detail**, not just substring after colon. Solutions depend on exact ArchiveValidation/ExtractArchive, MoveFile/CopyFile missing/already-exists messages, DeleteFile, ExecuteProcess; use M's exact strings. | P:309-354; M:317-391 |

R:24-101 already carries issue category, severity, component, path, instruction index, optional solution, stage summaries/messages, counters and dry-run issues. Structural sufficiency is not semantic parity: E:213-220 environment currently checks recovery/completion; R:180-213 component validation reports only one aggregate error on a failed engine call; raw errors may lack C# ERROR: prefixes. Add shared diagnostics where needed instead of parsing arbitrary UI strings into invented mod identity. Shared solution mapping must agree with CLI serialized issue solutions. Keep unknown values unknown.

## Smallest implementation sequence and acceptance boundary

1. Add a pure shared validation presentation mapper and deterministic text-report formatter; input is existing Report plus explicit selected count, log, timestamp. Port P/M exactly, with independent tests for every stage, prefixed/unprefixed messages, no duplicate Environment failure, success archive omission, five-error/warning cap, errors over warnings, missing dry-run, solution rules, unknown fields, counts independent of cards, and exact copy text. Do not alter report JSON.
2. Add page-local run state and shared engine stage-progress events. Define reset/running/completed/setup-failure/discarded/cancelled states; preserve original typed report and stale-input guard. Test both GUI worker and CLI consume the same outcomes, stage order and selection. Completion report/events cannot accidentally resurrect a stale prior run. A setup error must leave navigation blocked and render actionable local text, even though C# exception handling is incomplete.
3. Replace only Page::Validate's delegation with the six-row renderer. Keep utility settings reachable outside the exact page composition; maintain existing full/remaining pipeline calls, recovery/consent safeguards, and widescreen behavior. Implement pane resize, independent scroll IDs/reset, log visibility, result borders, counts/status, and copy feedback. Global Activity is separate diagnostics.
4. Add exact first-issue and log flush/report behavior from presentation indices, then run focused shared/worker regressions and required repository format/Clippy/all-targets/no-default checks. Existing tests U:3527 onward establish shared full/dry-run dispatch, not native layout. Compare C# presenter tests and `scripts/agents/test_pr110_validation.sh` expected cases; only build/run C# under explicit implementation/verification task ownership.
5. Drive native matched C#/Rust fixtures at pinned source/binary hashes. A source mapper and tests may close mapping requirements; page parity remains open until the native matrix below is captured and reviewed. Never label a simplified report implementation complete parity.

Your options (Compound Engineering): `/ce-work` can implement steps 1–3 from this spec with a bounded implementer; `/ce-code-review` can review shared mapping/readiness changes before native verification. This audit does not invoke either skill or launch additional agents.

## Required fresh native verification

Existing `docs/parity/wizard-native/README.md` binds **d4fdb45**, explicitly before subsequent FOMOD fixes. It proves a two-component route/base-Move/remaining-Copy fixture only. It disclaims validation cards/report, themes, keyboard/accessibility and minimum-size visual parity. It is not native evidence for HEAD 915ec69 or future validation edits. Existing `GUI-INVENTORY.md` top integration status is current source context; its “Original gap inventory” is historical and includes obsolete older-dialog navigation/dimensions.

For each new run bind C#/Rust commit, dirty diff if any, running executable hash (including `/proc/<pid>/exe` on Linux), fixture hash, theme, scale, client size, command, interaction transcript and captures. Use isolated disposable game/mod trees and assert before/after hashes unchanged by validation.

Native cases: (a) initial selected and empty states; (b) in-flight long enough to observe progress, status and disabled Run; (c) clean pass; (d) warning-only; (e) failed Environment with and without prefix; (f) conflicting/reordered/archive issues; (g) >5 dry-run errors plus warnings, and >5 warnings alone; (h) rerun after manually scrolling/resizing both panes; (i) Copy immediately after completion while logs are queued; (j) manual first-issue after scrolling away; (k) settings/selection change during worker and stale completion; (l) setup failure and cancellation. Capture native clipboard contents independently and compare normalized timestamp/duration only; compare card order/text/counts and actual pane state, not screenshot appearance alone.

Run controlled 1280×800 and supported minimum 600×400, Light/K1/K2 and high DPI. Check no clipped controls, both scroll panes and footer usable with download sidebar; splitter drag, keyboard Tab/Shift+Tab/Enter/Space, issue navigation, tooltip, copy feedback, navigation errors, and accessibility tree labels/status. Native layout, interaction, screen-reader behavior, and warning/error end-to-end data parity are independent proof obligations. Synthetic presenter fixtures may prove presentation but must not be described as engine-generated diagnoses.

## Memory provenance for parent final citation

Used `/home/brunner56/.codex/memories/MEMORY.md:707-716` for active checkout/oracle context and `:734-740` for full-parity/shared selection/safety boundaries. Rollout ID `01a0a7d5-e1a7-71e1-9c2e-77a0cacd63d5`. No memory file edited; no historical implementation status accepted as current without source inspection.
