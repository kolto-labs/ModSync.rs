# Instruction Source normalization follow-up

Status: proposed; not implemented or independently reviewed. F01/F05 remain partial.

## Observed gap

At the integrated wave 2 source, `Guide::from_value` calls `normalize_component`
for each top-level component and recursively for options (`src/model.rs:223`,
`:560`). The loader retains authored instruction Source strings. The deduplication
comparison alone applies `source_key` (`src/model.rs:431`). Consequently an option
can compare equal under normalized paths while validation receives the original
path, including a leading backslash that the pinned C# loader rewrites.

C# reference is the read-only checkout at
`/home/brunner56/.codex/worktrees/5813/ModSync`, pinned to `d31ccbd3`.
`Services/ModComponentSerializationService.cs:2140-2145,2349-2354` excludes Choose
and invokes `Utility/Serializer.cs:111-179` for all other Source values.
`FileSystemUtils/PathHelper.cs:823-847` trims leading backslashes, converts
separators, collapses repeated separators except after a colon, and removes
trailing separators. `Utility/Serializer.cs:237-249` prefixes relative inputs
with `<<modDirectory>>` and preserves recognized placeholders case-insensitively.
These references are under `src/ModSync.Core/`.

## Proposed implementation boundary

1. Extract one source-normalization helper from `source_key`; apply it when
   normalizing non-Choose instruction Source values before deserialization.
   Cover both string and array shapes and recursive Options. Keep Choose GUID
   strings intact. Do not alter Destination or executable arguments in this slice.
2. Verify the route used by every format. Structured loaders use C#'s serializer;
   Markdown reaches Rust's same `from_value` entry point, but its C# ingestion
   route differs. Do not apply the structured-loader policy to Markdown merely
   because the Rust entry point is shared; measure it first and pass explicit
   format context if required.
3. Preserve the engine's traversal, absolute-path and unknown-placeholder
   rejection. Matching C# formatting is not permission to weaken the sandbox.
   Separate C# quirks from safe supported behavior in COMPATIBILITY.md.
4. Use the same helper for equality and stored Sources, avoiding divergent
   comparisons. Retain dedup/Choose-reference behavior already approved in wave 2.

## Required direct evidence before acceptance

Use production CLIs with disposable guides and the pinned C# DLL. No Rust tests
or temporary Rust harnesses may be created, modified, or run under the current
freeze. Capture exact normalized Source values and validate results for JSON,
TOML, YAML, XML and Markdown, including nested Options, Choose, single-string
and array Sources, leading/repeated/trailing separators, each supported
placeholder, mixed case, whitespace/empty strings, traversal and absolute paths.
Compare the saved normalized guide and actual installed bytes for a safe Copy
case. Re-run loader-dedup direct probes and confirm existing Rust test sources
remain byte-identical. Record source/binary hashes and any intentional deviation.

No new normalization behavior or full-install proof is claimed by this plan.
