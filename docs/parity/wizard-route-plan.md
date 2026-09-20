# Reference wizard route and desktop integration

Spec: full C# parity, with exact GUI behavior and shared GUI/CLI operation logic. This package replaces the four-tab installer navigation with reference wizard routes; full visual/accessibility parity remains an acceptance obligation.

## Global Constraints

- Active GUI reference is MainWindow with Controls/WizardHostControl, not the older InstallWizardDialog. Loading is a separate landing surface; wizard starts at Welcome. Shell is 1280x800, minimum 600x400; use host progress header and footer, not the dialog's 240px step sidebar. This correction supersedes the dialog-specific wording below.
- Preserve successful-message completion gates even where C# has stale Next state or incorrectly advances after a failed base install; document these deliberate safety differences.

- Reference C# checkout: `/home/brunner56/.codex/worktrees/5813/ModSync`, commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`.
- User-owned dirty AGENTS.md must remain untouched. Parent owns publication to already-authorized origin/main.
- Shared engine owns install, validation, download and selection rules. Wizard state controls presentation and dispatch only.
- Never fabricate progress, completion statistics, successful validation, or install outcomes.
- Preserve failed-install recovery, cancellation, verified base-prefix and source-consumption safety.
- Tests and native runs use disposable directories; no user's game mutation.
- Work in existing authorized codex/rust-port checkout; serialize source implementations.

## Task 1: Reference route model

Create a small typed wizard route/state implementation in `src/wizard.rs` and expose it through the library if that is the simplest integration. Own wizard.rs, lib.rs, and tests/wizard.rs. Read C# InstallWizardDialog InitializePages/AddWidescreenPages/UpdatePageStates/Next/Back and page Title/Subtitle/gates. Model exact base sequence and metadata predicates (preamble nonblank, Aspyr only target KOTOR2 or TSL and nonblank warning); insert widescreen routes only after leaving completed base and only if widescreen components exist, optional warning page only when nonblank. Preserve exact page titles. Model current/completed/accessibility (index <= current), back/next transitions and invalidation/reset on a new guide. Route model must be usable by the real UI in Task 2; avoid generic navigation frameworks. Gates should accept facts from shared engine/UI state, never duplicate filesystem/install rules. Include tests for K1/K2/TSL, blank/absent metadata, widescreen insertion and no duplicate insertion, navigation bounds/accessibility and base/remaining phase transitions. Record policy questions from exact C# code. Commit owned files only; run focused and required checks, report task-1-report.md in this plan's ignored workspace. No subagents or push.

## Task 2: Wire reference wizard into desktop

Own src/ui.rs and focused UI tests, plus wizard model changes only if Task 1 integration needs a correction. Replace the four installer tab buttons with the typed reference wizard route, step sidebar, current title/subtitle, Step N of M and Back/Next/Cancel/Finish controls. Read exact XAML/code-behind values; default shell 900x650, minimum 800x600, sidebar 240px. Keep guide editor/settings reachable as utility surfaces rather than invented wizard steps. Render actual page content using existing handlers and fields: loading, welcome, authored preamble/notices, separate mod/game directories, selection, downloads, validation, install review, installing, base completion, widescreen stages, finished. Move/reuse existing functional controls; do not create placeholder pages or duplicate business rules. Use shared engine for all jobs. Navigation gates must prevent bypassing load/directories/selection/validation/confirmation/install completion; sidebar cannot jump forward. Load replacement resets route. Base completion inserts widescreen pages at the reference transition without replaying base. Worker terminal messages drive completion transitions; failures do not advance. Maintain access to recovery and existing editor/profile/FOMOD/download functions. Read-only pages must not report fabricated metrics. Add tests for GUI route/gates and message-driven base/widescreen progression; preserve existing tests with intentional contract changes documented. Update GUI-INVENTORY.md/COMPATIBILITY.md accurately for closed route gaps and remaining native/rendering gaps. Run checks and a disposable native render/interaction if available. Commit owned files; no push/subagents.

## Task 3: Reviews and integration

Task-scoped spec/quality reviews after each implementation. Resolve findings through implementer and scoped re-review. Final whole-package review and native acceptance evidence before publication. Never equate route/state tests with full exact-GUI proof.
