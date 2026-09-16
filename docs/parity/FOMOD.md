# FOMOD parity contract — investigation

Reference: d31ccbd3bfcddb4705917eafa6483715c166de3b. Source hashes and isolated executable probes are in `fomod-selection-oracle/`, `fomod-unmodeled-dependencies/`, `fomod-rename-oracle/`, and `fomod-conditional-skip-oracle/`. Rust rejects unconfigured FOMOD archives at extraction, but configured archives install selected bytes through the shared engine and CLI/desktop choice flows. F15 remains partial: full corpus, persistence/recovery and all native interface behavior remain unproven.

## Required implementation and acceptance

1. Discover ModuleConfig.xml and optional info.xml in archives, including nested roots; preserve archive identity and safe paths. Cover missing/corrupt archives and cancellation without game writes.
2. Parse the reference model: required files, file/folder destinations and priorities, ordered steps/groups/plugins, descriptions/images, plugin types, condition flags, visibility and conditional installs. Reject unsupported executable semantics explicitly rather than silently treating metadata preservation as support.
3. Share a selection session between desktop and CLI. Implement all five group modes, defaults, required/recommended plugin handling, visible steps and validation. Prove the same choices produce the same selected options in both interfaces.
4. Load the version/archives/selections choices-file contract, case-insensitive archive/step/group/plugin matching, and actionable missing/invalid-choice errors. Cover empty, duplicate and unknown selections rather than relying on permissive reference behavior.
5. Map configured choices to shared engine instructions with archive-relative sources, correct destination naming, required files and conditional copies. Isolated selected-byte installs and safe-path checks exist; priority/conflict behavior with corpus inputs remains unproven.
6. Persist archive-scoped choices and apply them during validate/install. Existing flows do this, but restart/recovery and broader guide-merging equivalence remain open.
7. Integrate post-download prompts, CLI choices/TTY workflow, desktop wizard, cancellation/reconfiguration and recovery. CLI and desktop flows exist; full failure/restore and corpus acceptance remain required.

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

`FomodToComponentMapper.EvaluateDependency` treats file/game dependencies as satisfied with warnings. Conditional mapping supports only restricted flat flag patterns and skips unsupported patterns. Direct network-disabled C# mapper probes now cover those fallback rules and file-rename fallback; they do not establish full archive or UI behavior. Visibility, conflicting flags, priority, conditional mapping across corpus fixtures, NotUsable plugins, duplicate names, archive path layout and choices-file handling still need broader acceptance before F15 can advance beyond partial.

## Rust selection primitive

GroupSelection now provides defaults, cardinality validation and atomic selection changes. Tests compare against the saved five-mode C# result. The required-sibling conflict is resolved by rejecting any exclusive selection that would deselect a required sibling; conflicting required defaults fail validation. This is an explicit correctness divergence. The engine permits only configured FOMOD archives and derives their copies from the current archive, preventing stale extraction sources.
