# FOMOD gate: collect every issue and warning

This contract closes the residual recorded in [fomod-gate-warnings.md](fomod-gate-warnings.md)
("Rust still stops at its first hard FOMOD issue"). The reference is the pinned C#
`d31ccbd3`; C# paths below are under `src/` there. Evidence:
[fomod-gate-collect-proof](fomod-gate-collect-proof/README.md). F15 stays partial.

## Triage at `d78aea2`

**C# behaviour.** `FomodConfigurationGate.Validate`
(`ModSync.Core/Services/Fomod/FomodConfigurationGate.cs:106-197`) walks every component in
scope and every registered archive, and never returns early. It collects:

| C# finding | Where | Kind |
| --- | --- | --- |
| Registered archive missing, prompt state recorded (`ArchiveMissing`) | `:117-130` | issue |
| Registered archive missing, no prompt state | `:131-141` | warning |
| Registered archive cannot be inspected (`ArchiveUnreadable`) | `:146-161` | issue |
| FOMOD archive whose status is `configured` but without archive-scoped instructions (R1) | `:169-185` | warning |
| FOMOD archive whose status is not `configured` | `:188-193` | issue |

Messages are formatted by `FormatIssueMessage` (`:262-290`). The validation pipeline
(`ModSync.Core/Services/Validation/InstallationValidationPipeline.cs:452-478`) adds every
warning as `WARNING: <component>: <message>` (`:457-460`), then, when any issue exists,
every issue as `ERROR: <component>: <message>` (`:470-476`) in one failed stage, and adds
every stage message to `ErrorCount` (`:163-167`). `InstallationService.cs:1127-1139` logs
every issue; the wizard (`InstallStartPage.axaml.cs:69-78`) shows the first issue plus
`(+N more unconfigured FOMOD archive(s))`.

**Rust at `d78aea2`.** `Engine::check_fomod_configuration` (`src/engine.rs:899`) returned
at the first in-gate issue: the missing-with-prompt-state `bail!` (`:993`), the
registered-archive inspection `?` (`:1015-1017`), and `configured_choices(...)?` in the
registered pass (`:1021`) and the Extract pass (`:1082`), which covers configured status
without saved choices, unparseable saved choices and stale choices. Warnings and issues
of later components were never collected, on every surface (report stage, plain
`validate`, `install`, and the desktop, which uses the same report and events).

**Decision.** The following become collected issues: missing archive with prompt state,
unreadable archive (registered pass, and Rust's Extract pass), and every
`configured_choices` failure (no or unparseable saved choices, saved choices that no longer
apply). After an issue the gate continues with the next archive. That archive's R1, mapper
fallback and no-route findings are skipped, because they require valid choices. These
still abort the gate immediately: cancellation (an inspection failure while the cancel flag
is set returns that error), unsafe or invalid registered paths and filenames, guide
structure errors (`collect_components`), and Extract source resolution
(`self.matches`, `self.files[..].path()`). An abort ends the list and is listed last,
after any issues collected before it.

## Behaviour after this slice

- **Engine.** `validate_fomod_configuration` (`src/engine.rs:910`) returns the warnings and
  a list of issues. An empty list means the gate passed. Issues are listed in gate order:
  the registered pass over the scope and then its hard dependencies, then the Extract pass.
  Issues with identical text are kept once, for example one unconfigured archive
  registered by two components (Rust's texts name the archive, not the component). When
  any FOMOD issue exists, the gate still fails before Rust's stricter checks (`ordered`,
  `check_component`), as before (`:1161`).
- **Report stage** (`src/validation.rs:324`). The warnings come first, as `WARNING:` lines.
  Every issue follows as its own message, with the same bare text as before (no `ERROR:`
  prefix). `errorCount` grows by the number of issues. The summary stays `Failed`. "Go to
  first issue" lands on the first issue.
- **Plain validate and install** (`fomod_gate`, `src/engine.rs:445`). All `WARNING:` events
  are emitted first.
  - **Single issue.** Its error is unchanged:
    `FOMOD configuration check failed: <issue>`. Install wraps it as
    `Installation blocked before any write: ...`.
  - **Several issues.** They are numbered in gate order in one line:
    `FOMOD configuration check failed: <n> issues: [1] <first>; [2] <second>; ...`.
    Each issue keeps its full error chain (`{:#}`).

  Exit codes are unchanged, and nothing is written.

## Out of scope

- Rust issue texts do not carry C#'s `ERROR: <component>:` prefix or wording; they stay
  as they were, so single-issue output is stable.
- C#'s error count for a failed FOMOD stage (every message, warnings included) and its
  summary `<n> unconfigured FOMOD archive(s).` are not copied.
- The issue list is not bounded: it holds at most one issue per (component, archive),
  like C#.
- After a component's archive has an issue, Rust's R1 warning, mapper fallbacks and
  no-route warning are not produced for that archive and component. C# does not check
  choices, so its R1 check depends only on status.

## Known limits (review of df051f7)

- Issues are de-duplicated by text, and the text names only the archive's file name.
  Two registered paths with the same file name (`a/x.zip` and `b/x.zip`) that fail the
  same way therefore show as one issue. The gate still fails; the second issue appears
  once the first is fixed. C# names the component, so it lists both.
- A failed inspection is not cached, so each further component that registers or
  extracts the same unreadable archive reads it again. C# also re-probes per entry.
- An Extract-pass inspection error keeps its pre-existing text, which does not name the
  archive. This keeps single-issue output byte-identical.
