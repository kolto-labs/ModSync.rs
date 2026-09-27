# Task 1b independent specification and quality review

Reviewed BASE `9d77f4ba9955ea505d3eb7969afdf5b5aee73b72` through HEAD `1cb3f4be432b9c5435af442517990fedb2a97482` on 2026-09-27. C# oracle is pinned at `d31ccbd3bfcddb4705917eafa6483715c166de3b`.

Specification verdict: **CHANGES REQUIRED**. Quality verdict: **CHANGES REQUIRED**, for the same independently reproduced protection regression below. Task 2 should remain gated until the fix receives scoped review. No production/test files changed, no Rust tests or harnesses created/run, no rebuilds, no subagents. Only this review file was written in the repository. Two novel real-CLI invocations were justified by the concrete omitted-dry-run/options defect; fixtures and outputs are private disposable files.

## P1 — Preserve selected-option FOMOD gates when removing strict component simulation

Changed location: `src/engine.rs:318-319` (ComponentValidation now calls `validate_archives`), with new archive-only processing at `src/engine.rs:522-532`. The independent FOMOD traversal at `src/engine.rs:669-706` still only checks each top-level component's own instructions.

A selected parent can contain an applicable Choose targeting a selected immediate option whose Extract archive contains a valid FOMOD configuration but has no FomodChoices. Previously the component stage's strict simulation reached that option and rejected it. The replacement archive pass includes option instructions and unpacks the archive, but it does not inspect/validate FOMOD configuration. The independent FomodConfiguration stage never visits that option. Consequently `validate --use-file-selection --validation-report --full` now returns success, **0 errors / 0 warnings / 5 passed**, including `All detected FOMOD archives are configured.` The identical guide with `--dry-run` correctly fails **1/0/5** with `FOMOD choices are unsupported without explicit FomodChoices configuration`; the FOMOD stage itself still claims success. Actual install remains strict, so this is false validation/independent-gate success, not evidence of unsafe live installation.

This violates brief requirement 4: unconfigured FOMOD must remain an independent hard failure. It is an immediate selected-option regression, not a demand to implement the deferred general nested-option parity work. Restore the independent FOMOD check for options actually reached through applicable Choose instructions, preserving option selection, instruction dependencies/restrictions, completed-prefix exclusions, owning option configuration metadata, and registered prior-state protection. Reuse/factor the existing execution traversal policy rather than introducing a second selection policy or restoring whole-install simulation to ComponentValidation. Do not inspect every unselected/unreachable option as if it would execute. Generic absent Extract inputs without known configuration state must retain the intended archive/dry-run distinction.

Acceptance can use the real production CLI with this owned fixture: full report must fail its FomodConfiguration stage both with and without dry-run; configured, unselected, and dependency/restriction-excluded variants must retain applicable behavior. No new Rust tests are required or authorized.

## Other reviewed behavior and evidence boundary

The declared Conflicts stage uses component declarations and selected/completed IDs; standalone simulated-write conflict methods/JSON remain separate and strict. MissingSourceFiles is a private typed cause created only at Copy/Move/Extract resolution; Rename and FOMOD-generated member misses retain hard errors. Dry-run stops the affected component, retains earlier virtual state, and replaces provisional component issues with the specified single warning. Source syntax is checked before classifying ordinary absence. Game/kotor roots use the same source boundary. Existing install preflight, receipt verification, snapshot/pending creation order and recovery lifecycle are unchanged by the diff.

The new archive stage performs actual read-only archive unpack/member checks, includes immediate option instructions as the C# collector does, and checks directory destinations independently from loose Copy/Move source availability. Temporary unpack content is outside game/mod roots. Existing observer starts/completions still enclose actual work with the same stage ordering and totals; native callback timing is source-reviewed only, not newly driven GUI proof.

I verified all final production source hashes against final-build-receipt.json. Its build/fmt/production-clippy records report exit 0 for default and headless builds; I did not rerun them. Final receipts are separately bound to the final copied binary; the original 80 probes remain bound to the pre-hoist implementation, and the final set contains 14 invocations. Readback of exact-guide receipt stdout confirms:

- final-exact-present: 0/0/6; exit 0; before == after: True.
- final-exact-missing: 0/1/5; exit 0; before == after: True.
- final-exact-restored: 0/0/6; exit 0; before == after: True.

The exact missing-copy result and unchanged-manifest receipts establish the bounded Copy correction, not full C# feature parity. Registry auto-extraction, Move subfolder remapping, existing-target-basename fallback, mixed-source aggregation, archive heuristics/AttemptFixes differences and general nested-option parity remain open as named in task-1b-report.md and the binding audit. Cancellation remains source-reviewed without direct CLI exercise. UI/rendering/clipboard/navigation and full-project acceptance remain outstanding. The stale existing Rust assertion remains untouched as ordered.

## Reproduction fixture and artifact-bound receipt

Fixture: `/tmp/modsync-review-option-fomod-804s9034`. It copies only the prior owned unconfigured-FOMOD fixture's game/mod bytes, then places its Extract instruction into a selected immediate option reached by Choose. Both invocations use the already-built final desktop binary. Game/mod file manifests were checked immediately before and after both invocations and matched. This is checkpoint evidence, not syscall tracing. No installation or external program was run.

Exact guide:

```json
[{"Guid": "11111111-1111-1111-1111-111111111111", "Name": "Fixture", "Author": "Wizard", "Category": [], "Language": [], "ResourceRegistry": {}, "ExcludedDownloads": [], "Dependencies": [], "Restrictions": [], "InstallBefore": [], "InstallAfter": [], "IsSelected": true, "Instructions": [{"Action": "Choose", "Source": ["22222222-2222-2222-2222-222222222222"]}], "Options": [{"Guid": "22222222-2222-2222-2222-222222222222", "Name": "Selected FOMOD option", "Author": "Wizard", "Category": [], "Language": [], "ResourceRegistry": {}, "ExcludedDownloads": [], "Dependencies": [], "Restrictions": [], "InstallBefore": [], "InstallAfter": [], "IsSelected": true, "Instructions": [{"Action": "Extract", "Source": ["<<modDirectory>>/package.zip"], "Destination": "", "Overwrite": true}], "Options": [], "WidescreenOnly": false}], "WidescreenOnly": false}]
```

The following receipt embeds exact argv, raw stdout/stderr, exits and file hashes so the finding remains reviewable even after disposable files are cleaned:

```json
{
  "reviewed_head": "1cb3f4be432b9c5435af442517990fedb2a97482",
  "binary": {
    "path": "/tmp/modsync-task1b-proof-whg3rfzj/modsync-desktop-final",
    "sha256": "e0579401e4f76538f8c7aea694aefd552264e608b83210f66091b3e344d2b09f"
  },
  "production_source_sha256": {
    "src/engine.rs": "e21ddde7612b10c64dd32147bad6b7234ffa1cb2568d92cb4c5939f10726143a",
    "src/validation.rs": "749160e66d39335bc426d34d9b102cc85000a5cc9165d92d89d21ba22b43405d"
  },
  "fixture_files": {
    "review-results.json": "cec041f4a1a4a0f4d0910291c4af7bfc74121496edfac0ddc3fa172625599d0d",
    "guide.json": "20e28f2689dc8ac5e6dc07031bd982cbedb3fd4c7d1c9131f89a42614e0b7792",
    "mods/package.zip": "062caf1a1893f2a5dada67ab5018d5f84836e34775cefd951350ae80ede43f6d",
    "game/swkotor.exe": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  },
  "game_mod_manifests_unchanged_during_both_runs": true,
  "runs": [
    {
      "argv": [
        "/tmp/modsync-task1b-proof-whg3rfzj/modsync-desktop-final",
        "validate",
        "--input",
        "/tmp/modsync-review-option-fomod-804s9034/guide.json",
        "--game-dir",
        "/tmp/modsync-review-option-fomod-804s9034/game",
        "--source-dir",
        "/tmp/modsync-review-option-fomod-804s9034/mods",
        "--use-file-selection",
        "--validation-report",
        "--full"
      ],
      "exit": 0,
      "stdout": "{\n  \"success\": true,\n  \"exitCode\": 0,\n  \"errorCount\": 0,\n  \"warningCount\": 0,\n  \"passedCount\": 5,\n  \"componentCount\": 1,\n  \"inputPath\": \"/tmp/modsync-review-option-fomod-804s9034/guide.json\",\n  \"stages\": [\n    {\n      \"stage\": \"Environment\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"Passed\"\n    },\n    {\n      \"stage\": \"Conflicts\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"No dependency or restriction conflicts.\"\n    },\n    {\n      \"stage\": \"InstallOrder\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"Install order is correct.\"\n    },\n    {\n      \"stage\": \"ComponentValidation\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"All components passed archive validation.\",\n      \"messages\": [\n        \"OK: Fixture\"\n      ]\n    },\n    {\n      \"stage\": \"FomodConfiguration\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"All detected FOMOD archives are configured.\"\n    }\n  ]\n}\n",
      "stderr": ""
    },
    {
      "argv": [
        "/tmp/modsync-task1b-proof-whg3rfzj/modsync-desktop-final",
        "validate",
        "--input",
        "/tmp/modsync-review-option-fomod-804s9034/guide.json",
        "--game-dir",
        "/tmp/modsync-review-option-fomod-804s9034/game",
        "--source-dir",
        "/tmp/modsync-review-option-fomod-804s9034/mods",
        "--use-file-selection",
        "--validation-report",
        "--full",
        "--dry-run"
      ],
      "exit": 1,
      "stdout": "{\n  \"success\": false,\n  \"exitCode\": 1,\n  \"errorCount\": 1,\n  \"warningCount\": 0,\n  \"passedCount\": 5,\n  \"componentCount\": 1,\n  \"inputPath\": \"/tmp/modsync-review-option-fomod-804s9034/guide.json\",\n  \"stages\": [\n    {\n      \"stage\": \"Environment\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"Passed\"\n    },\n    {\n      \"stage\": \"Conflicts\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"No dependency or restriction conflicts.\"\n    },\n    {\n      \"stage\": \"InstallOrder\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"Install order is correct.\"\n    },\n    {\n      \"stage\": \"ComponentValidation\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"All components passed archive validation.\",\n      \"messages\": [\n        \"OK: Fixture\"\n      ]\n    },\n    {\n      \"stage\": \"FomodConfiguration\",\n      \"passed\": true,\n      \"hasWarnings\": false,\n      \"summary\": \"All detected FOMOD archives are configured.\"\n    },\n    {\n      \"stage\": \"DryRun\",\n      \"passed\": false,\n      \"hasWarnings\": false,\n      \"summary\": \"\u2717 Validation failed with 1 error(s) and 0 warning(s).\\n\\nThe following issues must be resolved before installation:\\n\",\n      \"messages\": [\n        \"ERROR: [EngineSimulation] Fixture / step 1 (Choose): Selected FOMOD option / step 1 (Extract): FOMOD choices are unsupported without explicit FomodChoices configuration\"\n      ]\n    }\n  ],\n  \"dryRun\": {\n    \"isValid\": false,\n    \"hasWarnings\": false,\n    \"issues\": [\n      {\n        \"severity\": \"Error\",\n        \"category\": \"EngineSimulation\",\n        \"message\": \"Fixture / step 1 (Choose): Selected FOMOD option / step 1 (Extract): FOMOD choices are unsupported without explicit FomodChoices configuration\",\n        \"componentName\": \"Selected FOMOD option\",\n        \"componentGuid\": \"22222222-2222-2222-2222-222222222222\",\n        \"instructionIndex\": 1\n      }\n    ]\n  }\n}\n",
      "stderr": ""
    }
  ]
}
```
