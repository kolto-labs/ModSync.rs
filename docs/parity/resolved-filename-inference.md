# Resolved filename instruction inference

`archive_generation::resolved::generate` accepts a guide, component-bound lists
of resolved filenames, an optional existing local download root, archive options,
and cancellation. Provider resolution belongs to the caller. This helper does
not perform network requests, acquire downloads, save a guide, or mutate the
mod/game trees. Existing archive analysis uses the shared decoder's scratch
storage and admission limits.

The reference behavior is `AutoInstructionGenerator.GenerateInstructionsFromUrlsAsync`
(at the pinned C# source's URL pre-resolution section near line 1314),
`CreatePlaceholderInstructionObject` (near 1776), and `IsGameFile` (near 2293).
All filenames from every caller-supplied component group are considered. Known
archive extensions are zip, 7z, rar, exe, tar, and gz. Existing archives use
`generate_from_archive`; unavailable archives produce Extract placeholders.
Recognized loose game files produce Move drafts targeting Override. Unsupported
names produce explicit unresolved review diagnostics.

Inputs are limited to 4,096 component groups and 4,096 filenames, one MiB of
filename text, 1,024 bytes per component identity, and 255 bytes per basename.
Traversal, separators, wildcard/control characters, reserved filenames, links,
reparse points, and nonregular local files are rejected. Case-distinct filenames
that collide under the default path policy are rejected rather than silently
choosing one. Local roots have at most 128 ancestors, checked as ordinary
folders. Cancellation is checked between bounded work and passed to the decoder.

Inference operates on a candidate guide and publishes it only after the final
cancellation check. Existing authored instructions and options remain in place.
Local archive inference runs on an isolated instruction/option set, then merges
new steps using the existing equivalence policy. Generated option identities
are remapped before Choose steps are appended; existing compatible options are
reused. Inference does not consolidate or rewrite preexisting authored steps.

Every changed component and newly generated option receives a pending
`DraftReview` record with the standard `DraftResult` diagnostics. Existing
unresolved diagnostics are retained. Previous acceptance receipts are invalidated;
prior receipts, resolutions, and original diagnostics are appended to
`InferenceHistory` (maximum 32 generations and one MiB total review record), so
new resolution acts on current diagnostics without overwriting prior history. Unsupported filenames require
explicit diagnostic resolution. These are drafts, including when a local archive
was successfully inspected, and require the existing explicit review workflow.

This document records source implementation scope. No provider, archive corpus,
filesystem test, install, native-platform, or CI acceptance was run for this slice.

## Cancellable local matching

`archive_generation::generate_local_with_cancel` supplies the same retained
cancellation token to local directory scanning and archive decoding. The existing
`generate_local` signature remains a wrapper for compatibility. Local matching
retains its reference filename scoring and archive preference. Its scan is
limited to 100,000 directory entries, validates an ordinary nonlink root and
safe archive basenames, and opens candidate archives as nofollow regular files.
Each component is inferred on a clone. Ordinary inference failures remain per-
component reports; cancellation returns an error and publishes none of the
batch's guide changes. Final publication follows a cancellation check.

Namespace inference reads at most one MiB through bounded cancellable chunks,
then admits at most 4,096 lines of at most 16 KiB each. Archive inference admits
at most 100,000 entries, 4,096 options, and 16,384 top-level instructions. The
namespace parser checks cancellation per line and option generation checks it
per option. Existing options are reused only when serialized executable semantics
match exactly after excluding component identity and draft review receipts;
guards, metadata, selection, and exact instruction patterns participate.
