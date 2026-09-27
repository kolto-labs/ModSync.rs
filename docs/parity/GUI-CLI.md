# GUI and CLI parity acceptance

The target is the C# GUI's controls, layouts, text, conditional pages, interactions, and behavior, with GUI and CLI actions using one Rust implementation of each operation. Shared calls alone do not prove C# equivalence. Reference: C# commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`.

| Flow | Current shared implementation | Remaining evidence or correction |
|---|---|---|
| Base validation | `Engine::validate_report_with_options`; desktop selects full validation with dry run, CLI supplies flags | C# environment, dependency conflicts, archive validation, issue presentation; native comparison |
| Remaining-phase validation | `Engine::validate_report_remaining` enters the same report pipeline as base validation | Completed prefix survives every fresh stage engine; consumed-source regression verifies no replay or writes. Native remaining-phase rendering remains open |
| Installation | `phase::prepare` then `Engine::install` / `install_remaining`; CLI All/Base/Remaining, desktop All/Base/Retained Remaining | Equality begins after selection; CLI stable-input reconstruction differs from GUI retained choices; full-build output remains unproven |
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
