# Download filename inventory and selection

## API and ownership

`download_plan::select_filenames_with_cancel(component, url, candidates, source,
cancelled)` is the additive bounded entry point. The existing `select_filenames`
signature delegates with a fresh false flag. Both apply the same admission bounds,
including the no-source, no-Extract, empty-URL and empty-candidate paths.

Ordinary `plan_with_cancel` and provider discovery each own one private selection
context. Its source inventory is captured lazily, once per invocation, only when
Extract-pattern filtering needs disk names. It is reused across that invocation's
resources. No component cache is keyed by a GUID; registry names and instruction
patterns remain specific to the actual component passed to selection.

The inventory is read-only filename evidence. It does not hash files, validate
archive bytes, create temporary files or modify either installation tree. Existing
archive verification and transfer publication remain responsible for integrity.
The snapshot can become stale after an external filesystem edit; it is not a
durable acceptance receipt or authority to publish a file.

## Reference selection policy

Pinned `ComponentValidationService.cs:1076–1225` collects Extract instructions
from the component and every immediate option, independently of selection and
instruction dependencies. ResourceRegistry Files keys and disk names contribute
both full relative names and basenames. A registry key contributes to availability
even if its value is false. Those semantics remain unchanged.

Pinned `DownloadCacheService.cs:2443–2549` uses explicit Files flags when present,
otherwise infers names from unsatisfied Extract patterns. Explicit filtering and
the no-Extract route preserve candidate duplicates. Inferred selection uses
ordinal-ignore-case uniqueness and preserves the first authored spelling and
candidate order. Inventory uniqueness uses the same retained Unicode 15 ordinal
profile as the shared FOMOD/name matcher; its newer-codepoint boundary remains
documented separately. URL lookup retains its existing ASCII comparison policy.
The existing `reference_paths::pattern_matches` wildcard/path policy is retained;
this change does not claim to repair every reference wildcard difference.

Ordinary missing or non-directory source roots contribute no disk names, matching
the former `source.is_dir()` behavior. Unsafe or unreadable paths fail
qualification. The existing peer staging directory predicate excludes unaccepted
peer copies. Its ordinary directory is qualified before descent is skipped.
Every visited path uses the shared ancestor check, rejecting symlinks, Windows
reparse points, nonordinary objects and inaccessible ancestors. Non-UTF-8 names
are unsupported. This is a snapshot under the existing cooperative-writer model,
not a native handle guarantee against arbitrary external namespace replacement.

## Bounds and cancellation

These are Rust admission limits, not .NET limits:

| Resource | Limit |
| --- | --- |
| Source path before cloning | 16 KiB |
| Inventory entries, including directories | 100,000 per capture |
| Relative inventory depth | 128 |
| Cumulative unique name insertions | 100,000 per context |
| Candidate, registry filename or Extract pattern | 4,096 UTF-8 bytes |
| Cumulative retained text | 16 MiB per context |
| Selection/ancestor/matching work steps | 2,000,000 per context |
| Cumulative processed/comparison text | 128 MiB per context |

Retained text counts logical bytes of original names, ordinal keys, source-path
copies, patterns and emitted results. Released intermediates still consume the
cumulative budget. Container capacity and allocator overhead are not included in
that logical-byte count; entry/name/work limits bound those separately. Temporary
ordinal and wildcard processing is charged conservatively before work, including
a fourfold scalar/path processing allowance. Bounds are checked before retained
copies, and no result is truncated or published partially.

Cancellation uses Acquire checks for each visited entry, ancestor qualification,
instruction, option, registry name, candidate and matching operation, with a
second check after the existing wildcard matcher. Filesystem operations and one
bounded regex match are not internally interrupted. A cancellation or local
qualification/budget failure is a typed error. Discovery propagates it instead
of falling back to an unnamed provider target. Provider HTTP or semantic discovery
errors retain their existing warning/fallback behavior.

The public selector also qualifies candidate/pattern/registry names before its
early-return branches; callers cannot bypass the bounds by omitting a root or
Extract instruction. Planning and discovery hold distinct contexts, so discovery
observes a fresh disk snapshot rather than reusing a persisted planning cache.

## Evidence boundary

This is production source implementation, awaiting independent SPEC and QUALITY
review and parent production compilation gates. No tests, fixtures, harnesses,
runtime or native filesystem probes, network transfers, builds or CI were run by
the implementing agent. It does not close F12, live provider acceptance, complete
wildcard parity, or whole application parity.
