# GUI and CLI parity acceptance

The target is the C# GUI's controls, layouts, text, conditional pages, interactions, and behavior, with GUI and CLI actions using one Rust implementation of each operation. Shared calls alone do not prove C# equivalence. Reference: C# commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`.

| Flow | Current shared implementation | Remaining evidence or correction |
|---|---|---|
| Base validation | `Engine::validate_report_with_options`; desktop selects full validation with dry run, CLI supplies flags | C# environment, dependency conflicts, archive validation, issue presentation; native comparison |
| Remaining-phase validation | `Engine::validate_remaining` | Desktop currently receives logs instead of the same structured report; preserve completed-prefix semantics when unifying |
| Installation | `Engine::install` / `install_remaining` | Equivalent selection/preflight preparation and full-build output |
| Selection | Desktop uses `selection` helpers; CLI prepares selections in `main.rs` | Audit explicit selection, dependency/option behavior, category/tier filters; centralize matching rules without erasing intentional CLI options |
| Downloads | Shared download/provider modules with separate frontend orchestration | Queue, cancellation, progress, filename selection and host behavior through both entry points |
| Wizard presentation | `src/ui.rs`, currently egui | Exact C# page/control inventory and conditional routes, text, layout, keyboard/focus, accessibility, themes and native rendered comparisons |

Confirmed mismatch corrected: C# `ValidatePage.ValidateAsync` uses `ValidationPipelineOptions.WizardFull` (`FullValidation=true`, `DryRun=true`). Rust desktop previously omitted DryRun. Its worker now requests the shared full pipeline with dry run; a worker-level regression compares the entire result with the same engine entry point used by CLI and verifies no game writes.

This regression establishes shared Rust behavior only. Exact C# GUI equivalence remains unverified and incomplete. Native screenshots and driven interaction tests must accompany the control-by-control audit; passing state tests are insufficient.
