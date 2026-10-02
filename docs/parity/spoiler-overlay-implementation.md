# Spoiler-free authoring overlay

Source implementation: `src/markdown/spoiler.rs`, exposed by `modsync::spoiler`.

The pinned reference `src/ModSync.Core/CLI/ModBuildConverter.cs` parses a separate
Markdown document after `## Mod List`, matches `###` component names without
case sensitivity, and assigns DescriptionSpoilerFree, DirectionsSpoilerFree,
DownloadInstructionsSpoilerFree, UsageWarningSpoilerFree, and ScreenshotsSpoilerFree.
Its convert and merge paths call the importer independently of instruction inference.

## Shared API

- `apply_markdown(&Guide, &str) -> Result<Overlay>` accepts reference headings,
  bold field markers, multiline text, and the Installation Instructions section
  inside note/warning/tip admonitions. CRLF and CR are normalized. Document
  preamble before Mod List is ignored as in the reference.
- `apply_guide(&Guide, &Guide) -> Result<Overlay>` accepts a parsed overlay guide:
  Description and Directions plus text metadata for the remaining supported
  fields, or explicit SpoilerFree field names. Populated unrelated authoring
  fields, including GUID and selection, are projected out with informational
  `ignored_fields` records. Unknown fields that claim a SpoilerFree role remain errors.
- `Overlay` returns the candidate guide, complete flag, applied component/field
  counts, component-addressed blocking diagnostics, and informational ignored fields.

The operation is atomic. Any unmatched, duplicate, ambiguous, empty, unsupported,
non-text, or unassigned content prevents all assignments and returns an unchanged
clone with `complete=false`. The CLI must display diagnostics and refuse to save
that candidate as a successful overlay. Ordinary Markdown metadata labels such as
Name, Author, Category and Tier are recognized and reported in `ignored_fields`;
they do not block application. Components with no overlay values are likewise
reported as omissions and do not change the target. Input/target/result guide payloads are
limited to 8 MiB and inventories to 10,000 top-level components. Diagnostic text
is bounded; exceeding 10,000 diagnostics explicitly rejects the overlay.

Name matching uses the existing Unicode simple-uppercase comparison key, without
trimming target names or fuzzy matching. Options are not searched: reference
matching is top-level. Only the five canonical metadata keys are changed. Base
instructions, options, resources, selection, names and unrelated metadata remain
unchanged. Existing draft review records remain present; authored identity changes
are still subject to the shared draft identity validation.

## Deliberate stricter behavior

The reference silently overwrites duplicate source headings/fields, skips
unmatched components and unsupported labels, and skips blank values. This API
reports malformed or ambiguous actual overlays and refuses partial application.
Ordinary authoring fields collected by the reference generic parser are accepted
as a projection rather than rejected: its documented Name and Author example is
a supported input. Arbitrary unrecognized Markdown labels still require correction.
It also accepts spaced
labels such as Download Instructions and explicit SpoilerFree labels. It does not
interpret arbitrary HTML/Markdown as executable content, fetch URLs, or inspect
archives. The caller owns bounded file reading, cancellation around I/O, output
format selection and publication.

## Evidence boundary

Source inspection and targeted formatting only. No tests, custom harnesses,
program execution, build, CI run, or actual document conversion was performed.
Native/end-to-end CLI acceptance remains pending.
