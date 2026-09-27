# GUI and CLI parity acceptance

The target is the C# GUI's controls, layouts, text, conditional pages, interactions, and behavior, with GUI and CLI actions using one Rust implementation of each operation. Shared calls alone do not prove C# equivalence. Reference: C# commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`.

| Flow | Current shared implementation | Remaining evidence or correction |
|---|---|---|
| Base validation | `Engine::validate_report_with_options`; desktop selects full validation with dry run, CLI supplies flags | C# environment, dependency conflicts, archive validation, issue presentation; native comparison |
| Remaining-phase validation | `Engine::validate_report_remaining` enters the same report pipeline as base validation | Completed prefix survives every fresh stage engine; consumed-source regression verifies no replay or writes. Native remaining-phase rendering remains open |
| Installation | `phase::prepare` then `Engine::install_observed` / `install_remaining_observed` (old names wrap them with a no-op observer); CLI All/Base/Remaining, desktop All/Base/Retained Remaining | Equality begins after selection; CLI stable-input reconstruction differs from GUI retained choices; full-build output remains unproven |
| Install progress | Both frontends read the same `install_progress::Progress` facts. CLI serializes them with `--install-progress-json`; the desktop Installing page renders them | Contract below. Desktop elapsed/ETA/rate are local presentation only and never reach the CLI |
| Selection | `selection.rs` owns both reference policies: GUI helpers add using wizard numeric/`Optional` tier priorities, while CLI resets using `matches_category_tier` and CLI tier normalization | Regressions cover empty/numeric CLI selectors, numeric wizard selection, exact full-label wizard selection versus cumulative CLI selection, `Optional`, and exact colon-containing name/GUID precedence. Exact name/GUID selection remains a Rust CLI extension; native selection interaction parity remains open |
| Downloads | Shared download/provider modules with separate frontend orchestration | Queue, cancellation, progress, filename selection and host behavior through both entry points |
| Wizard presentation | `src/ui.rs`, currently egui | Exact C# page/control inventory and conditional routes, text, layout, keyboard/focus, accessibility, themes and native rendered comparisons |

Confirmed mismatch corrected: C# `ValidatePage.ValidateAsync` uses `ValidationPipelineOptions.WizardFull` (`FullValidation=true`, `DryRun=true`). Rust desktop previously omitted DryRun. Its worker now requests the shared full pipeline with dry run; a worker-level regression compares the entire result with the same engine entry point used by CLI and verifies no game writes.

This regression establishes shared Rust behavior only. Exact C# GUI equivalence remains unverified and incomplete. Native screenshots and driven interaction tests must accompany the control-by-control audit; passing state tests are insufficient.

Shared phase preparation and CLI phase dispatch are implemented and independently
reviewed through `3d106ef`. [Integration evidence](cli-phases-integration.md) records
the exact commands, report/error contracts, receipt verification, repeats/recovery,
normalized output archive and final-source checks (229 passing all-target tests; 199 passing
headless tests; one intentional ignored fixture in each). Whole-package final review
passed with nonblocking P3 notes; this bounded phase slice is approved. All validation
retains its existing pipeline-report failure timing.
Remaining conflicts now verify receipts in the shared engine before suffix analysis.

GUI/CLI All and Base preparation compare equal for identical post-selection guides;
stable Remaining preparation/reports also compare equal. GUI keeps successfully
installed effective Base definitions after guarded completion. CLI reconstructs
from current guide/choices: whole-payload external choices metadata can make even
suffix-only edits fail the completed-definition comparison. Reusing stable choices
is required when they affected Base. A fresh explicit Base can repeat ordinary Copy
operations; consumed Move inputs fail safely. Only explicit `--direct-markdown`
transformation is excluded from phased requests. The automated differential evidence
does not refresh the earlier native wizard captures or establish exact C# parity.

## Typed install progress contract — 2026-09-27

The CLI and the desktop Installing page consume one shared fact stream: `Engine::install_observed` / `install_remaining_observed` and `install_progress::Progress` (`065ffee`). Neither frontend infers component order, completion, recovery or cancellation. Full schema and compatibility notes: `COMPATIBILITY.md`, "Typed install progress and Installing page — 2026-09-27". Rulings: [install-progress-integration.md](install-progress-integration.md).

`modsync install --install-progress-json` writes `{"type":"installProgress","progress":{...}}` per stdout line. It conflicts with `--json` and `--validation-report`, and `validate`/`conflicts` reject it. In typed mode the legacy Event text goes to stderr (`cd8f0b0`). Stdout write errors are ignored on purpose: a gone consumer does not stop or alter the install.

How a consumer reads the result:

| Outcome | Exit | Stdout | Stderr |
|---|---|---|---|
| Success | 0 | Last line has `"phase":"completed"` and `snapshotPending:false` | Legacy Event text |
| Failure or cancel after the snapshot | nonzero | No `completed` line. The last line is the last fact that happened, e.g. an earlier `componentCompleted` or `installingComponent` | Legacy Event text, then `Error: …` with the real error and the restore-before-retry snapshot path |
| Preflight failure | nonzero | Zero lines | `Error: …`; no snapshot is created |

There is no failure envelope. `33de7f6` updated the flag's `--help` text (`src/main.rs`) to state this contract. That is a source-only change; the `ac9cf3a` probe's `help.stdout` predates it. The CLI has no cancellation input, so cancellation is exercised only through the desktop Stop Install path.

Evidence: [install-progress-native/](install-progress-native/README.md) binds the Task 2a CLI probes to `57f198f` and a typed-mode stderr/contract probe to a clean `ac9cf3a` build. This is shared-contract evidence only. No C# CLI install-stream comparison was made, and it proves nothing about C# GUI parity.
