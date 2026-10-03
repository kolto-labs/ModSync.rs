# FOMOD gate: soft warnings survive a later hard failure

This contract covers the residual recorded in [fomod-archive-scoped.md](fomod-archive-scoped.md)
("Soft gate warnings are still dropped when a later readiness error fails the gate").
The reference is the pinned C# `d31ccbd3`; C# paths below are under `src/` there. Evidence:
[fomod-gate-warnings-proof](fomod-gate-warnings-proof/README.md). F15 stays partial.

## Triage at `438bb73`

**C# behaviour.** `FomodConfigurationGate.Validate`
(`ModSync.Core/Services/Fomod/FomodConfigurationGate.cs:106-197`) walks the whole scope and
collects every issue and every warning; it never stops early. The validation pipeline's
FOMOD stage (`ModSync.Core/Services/Validation/InstallationValidationPipeline.cs:452-478`)
adds each warning as `WARNING: <component>: <message>` (`:457-460`) and then, when the gate
failed, each issue as `ERROR: ...` (`:470-476`) in the same failed stage. So a C# report
always shows the gate's warnings next to its issues. On failure the pipeline adds every
stage message to `ErrorCount` (`:163-167`), warnings included. The install paths
(`ModSync.Core/Services/InstallationService.cs:1123-1140`, `:1190-1207`) and the wizard's
install gate (`ModSync.GUI/Dialogs/WizardPages/InstallStartPage.axaml.cs:65-79`) log or show
only issues; they never print gate warnings, even when the gate passes. The GUI mapper
(`ModSync.GUI/Services/ValidationPipelineDialogMapper.cs:245-276`) turns only `ERROR:` lines
into issue cards; warnings stay in the stage's messages.

**Rust at `438bb73`.** `Engine::validate_fomod_configuration` (`src/engine.rs:891`) collected
its soft warnings (missing registered archive, "configured but has no matching
archive-scoped instructions", mapper fallbacks and the Rust-only no-route warning,
`:957-1103`), then ran `ordered?` (`:1104`) and `check_component` (`:1105-1107`). Any error
returned through `?`, so the collected warnings were dropped. The same happened when a
hard FOMOD issue inside the loops (`bail!`/`?`, for example configured status without saved
choices) followed a warning collected for an earlier component.

| Surface | Path at `438bb73` | Warnings lost when the gate then fails? |
| --- | --- | --- |
| `--validation-report` FOMOD stage (CLI `main.rs:3332`, desktop `ui.rs:4152`, remaining phase `engine.rs:1425`) | `engine.rs:619-623` maps the error, `validation.rs:315-322` pushes a bare failed stage | **Yes.** The stage held only the error; `hasWarnings` was false and `warningCount` 0. |
| Desktop report rendering | `validation/presentation.rs:103-149` (cards/log from stage messages), `ui.rs:2811-2849` (stage list, first issue) | **Yes**, it renders the report above. |
| Plain `validate` events (CLI `main.rs:3348`, desktop `ui.rs:4181`, remaining phase) | `fomod_gate`, `engine.rs:445-457`: `?` returned before the emit loop | **Yes.** Only the error reached stderr. |
| `install` events (CLI `main.rs:3313`, desktop `ui.rs:4177`, remaining phase) | `fomod_gate` via `engine.rs:1252` | **Yes**, same function. |

**Status.** A real gap on every surface; fixed here.

## Behaviour after this slice

- **Engine.** `validate_fomod_configuration` now returns the bounded warnings together with
  the gate's outcome. The checks moved unchanged into `check_fomod_configuration`, which
  writes into a caller-owned `WarningCollector`, so every warning collected before a
  failure is returned. Outcomes and error texts are unchanged: a run that failed still fails
  with the same error and exit code, and a run that passed is untouched.
- **Report stage.** A failed FOMOD stage lists the warnings first, as `WARNING: ...` lines,
  then the error, as C# orders them. The stage has `hasWarnings: true`, the summary stays
  `Failed`, `errorCount` grows by one and `warningCount` by the number of warnings (Rust's
  existing counting for this stage; C# counts every message of a failed stage as an error).
  `Report::first_issue` (the desktop "Go to first issue") now points at the failed stage's
  first non-`WARNING:` message, so it still lands on the error.
- **Plain validate and install.** The `WARNING:` events are emitted first, then the error
  `FOMOD configuration check failed: ...` (install: `Installation blocked before any write:
  ...`). Nothing is written. C# prints no gate warnings on install at all; Rust already
  emitted them on a passing install and now does so on a failing one too.
- **Desktop.** The desktop uses the same report and events, so it shows the warning cards
  (`⚠️ <component>`) and log lines before the failed `❌ FOMOD Configuration` card.

## Out of scope

- Rust still stops at its first hard FOMOD issue, so warnings for components after that
  issue are not collected, and later issues are not listed. C# collects all of them.
- The C# error-count rule for a failed FOMOD stage (every message counts) is not copied.
- The C# install paths' silence about warnings is not copied; Rust keeps its events.
