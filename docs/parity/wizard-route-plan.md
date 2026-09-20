# Reference wizard route and desktop integration

Spec: full C# parity, with exact GUI behavior and shared GUI/CLI operation logic. This package replaces the four-tab installer navigation with reference wizard routes; full visual/accessibility parity remains an acceptance obligation.

## Global Constraints

- Active GUI reference is MainWindow with Controls/WizardHostControl. Loading is a separate landing surface; wizard starts at Welcome. Shell is 1280x800, minimum 600x400; use the host progress header and footer.
- Preserve successful-message completion gates even where C# has stale Next state or incorrectly advances after a failed base install; document these deliberate safety differences.

- Reference C# checkout: `/home/brunner56/.codex/worktrees/5813/ModSync`, commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`.
- User-owned dirty AGENTS.md must remain untouched. Parent owns publication to already-authorized origin/main.
- Shared engine owns install, validation, download and selection rules. Wizard state controls presentation and dispatch only.
- Never fabricate progress, completion statistics, successful validation, or install outcomes.
- Preserve failed-install recovery, cancellation, verified base-prefix and source-consumption safety.
- Tests and native runs use disposable directories; no user's game mutation.
- Work in existing authorized codex/rust-port checkout; serialize source implementations.

## Task 1: Reference route model

Create a small typed route/state implementation in `src/wizard.rs`, exposed through `src/lib.rs`, with tests in `tests/wizard.rs`. Read C# WizardHostControl InitializePages/AddWidescreenPages/Next/Back and page Title/Subtitle/gates. Model its Welcome-first sequence and effective metadata predicates (preamble nonblank, Aspyr only target KOTOR2 or TSL and nonblank warning); insert widescreen routes only after leaving completed base and only if widescreen components exist, with the warning page only when nonblank. Preserve exact titles/subtitles, sequential Back/Next and reset on a new guide. Gates accept engine/UI facts without duplicating filesystem/install rules. Test target-game predicates, absent/blank metadata, delayed/idempotent widescreen insertion including zero selected, navigation bounds and phase transitions. Keep format-specific metadata defaults as a separately recorded shared-loader obligation. Commit owned files and report focused/required checks. No subagents or push.

## Task 2: Wire reference wizard into desktop

Own `src/ui.rs` and focused UI tests, plus necessary route integration corrections. Replace the four installer tabs with the typed route, title/subtitle, Step N of M, progress, and Back/Next/Finish footer. Follow the active shell dimensions above; add its persistent 320px download sidebar/footer status from Download Process onward. Keep editor/settings/profile/recovery/FOMOD and existing downloads reachable as utility surfaces. Render functional landing, welcome, authored notices, separate directory pages, selection, downloads, validation, review, installing, base completion, widescreen stages and finished. Reuse existing shared actions; expose limitations honestly where guide-wide automatic fetch or exact visual controls remain incomplete. Gates prevent bypassing prerequisites; loading a replacement resets the route. Full base validation covers all selected components, while base install excludes widescreen; remaining validation uses the shared remaining pipeline. Terminal worker results determine completion, never route position or apparent progress. Base completion inserts widescreen pages without replaying base. Finish exits the wizard while retaining the loaded guide. Retain accessible cancellation and document differences from the active host. Test stale-result invalidation and message-driven successful/failed/empty-widescreen transitions; preserve existing tests and document intentional changes. Update GUI-INVENTORY.md/COMPATIBILITY.md for implemented behavior and open visual/loader/download obligations. Run required checks and disposable native interactions. Commit only owned files; no push/subagents.

## Task 3: Reviews and integration

Task-scoped spec/quality reviews after each implementation. Resolve findings through implementer and scoped re-review. Final whole-package review and native acceptance evidence before publication. Never equate route/state tests with full exact-GUI proof.
