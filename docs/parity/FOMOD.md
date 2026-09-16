# FOMOD parity contract — investigation

Reference: d31ccbd3bfcddb4705917eafa6483715c166de3b. Source hashes and an isolated executable selection probe are in `fomod-selection-oracle/`. Rust currently rejects FOMOD archives at extraction; F15 remains missing.

## Required implementation and acceptance

1. Discover ModuleConfig.xml and optional info.xml in archives, including nested roots; preserve archive identity and safe paths. Cover missing/corrupt archives and cancellation without game writes.
2. Parse the reference model: required files, file/folder destinations and priorities, ordered steps/groups/plugins, descriptions/images, plugin types, condition flags, visibility and conditional installs. Reject unsupported executable semantics explicitly rather than silently treating metadata preservation as support.
3. Share a selection session between desktop and CLI. Implement all five group modes, defaults, required/recommended plugin handling, visible steps and validation. Prove the same choices produce the same selected options in both interfaces.
4. Load the version/archives/selections choices-file contract, case-insensitive archive/step/group/plugin matching, and actionable missing/invalid-choice errors. Cover empty, duplicate and unknown selections rather than relying on permissive reference behavior.
5. Map configured choices to shared engine instructions with archive-relative sources, correct destination naming, required files and conditional copies. Prove priority/conflict behavior with real bytes and ordered execution; unsafe paths must fail before writes.
6. Merge configured instructions/options into the owning guide component and persist archive-scoped prompt/configuration state. Validate/install must check the configured state and archive-scoped instructions; a stale status alone is insufficient proof.
7. Integrate post-download prompts, CLI choices/TTY workflow, desktop wizard, cancellation/reconfiguration and recovery. Only then replace the current extraction refusal for verified configured archives. Prove failure stops in order and restore precedes retry.

## Measured reference behavior

The service probe parses XML, maps options, creates presenter sessions and exercises selection/validation without writing game files.

| Group | Default optional selections | Result after clearing first and selecting second |
|---|---|---|
| SelectAny | neither | valid |
| SelectExactlyOne | first | valid; empty intermediate state rejected |
| SelectAtMostOne | neither | valid |
| SelectAtLeastOne | first | valid; empty intermediate state rejected |
| SelectAll | both | invalid because first is cleared |

Required-plugin conflict: direct deselection returns false, but selecting an optional sibling in SelectExactlyOne clears the required plugin and validation returns no error. This is reproduced behavior, not an endorsed requirement. Resolve this conflict in the Rust session contract before claiming selection parity; do not silently copy a required-selection bypass.

## Source-grounded limitations requiring separate probes

`FomodToComponentMapper.EvaluateDependency` treats file/game dependencies as satisfied with warnings. Conditional mapping supports only restricted flat flag patterns and skips unsupported patterns. These are source findings, not full live reproductions. Visibility, conflicting flags, priority, conditional mapping, NotUsable plugins, duplicate names, archive path layout and choices-file handling all need independent acceptance before F15 can advance beyond the initial selection investigation.
