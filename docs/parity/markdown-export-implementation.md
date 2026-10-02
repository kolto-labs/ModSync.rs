# Markdown export implementation (M1)

Status: source implementation; runtime and differential acceptance deferred by
the remaining-work tracker's current execution policy. This document does not
close F01, F02, F03 or the M1 acceptance checklist.

## Implemented path

`Guide::render("md")` and the `markdown` alias now use
`src/markdown/export.rs`. The existing save/conversion path renders the complete
document before atomic output replacement. Metadata serialization failures and
unsafe HTML comment contents return an error before that replacement.

The formatter follows the pinned C# `d31ccbd3`
`ModComponentSerializationService.GenerateModDocumentation` and
`GetNonEnglishFunctionalityText`: component order, preamble and epilogue spacing,
the Mod List heading and separators, heading fallback, authored NameFieldContent,
ordered registry URLs with first-name/additional-patch links, field order,
category/tier defaults, language presentation and dependency names. Original
dependency spellings use the GUID-keyed reference map when present, then the
Rust importer's parallel DependencyNames list, then component GUID lookup.

Normalized guide metadata uses the same nested-over-root and Pascal-over-camel
precedence as the existing structured exporters. Aspyr and widescreen warnings
are emitted before the first respective qualifying component. Blank warnings
are omitted; the reference default widescreen notice is suppressed. No cache,
network lookup or fabricated validation context is used.

Embedded comments use the existing `<!--<<ModSync>>` marker and a single YAML
component mapping. The component serialization and resource compatibility
transforms are shared with structured YAML export. Actions are lowercase,
including nested option instructions. An absent or blank Heading stays absent
inside YAML while the visible heading falls back to Name. Empty model fields and
default instruction overwrite flags are omitted where they can be recovered by
the existing loader.

## Review-state preservation and rejection

The same optional DraftReview field is emitted whenever it exists, including
components with diagnostics and no instructions/options. Options retain their
own records recursively. The Markdown merge allowlist now restores DraftReview
from embedded metadata without regenerating its Result or changing RequiresReview.

Export and embedded import validate records before model normalization can remove
null values. RequiresReview must be a boolean; Result must deserialize completely
as DraftResult and belong to the containing Guid. A reviewed record with gaps,
conditional drafts or rejected actions is rejected. Pending records retain all
those diagnostics. Missing records are not synthesized or accepted here; existing
draft warning and engine review checks continue to determine authorization.
Validation follows the supported component, component-array and thisMod/Components
guide envelopes, then recurses only through component Options. ResourceRegistry,
Files and HandlerMetadata dictionaries are not interpreted as review records, even
when they contain a key or filename named DraftReview.

Every generated YAML comment is checked for `-->`. Such content is rejected
instead of allowing it to terminate the metadata comment. Serialization errors
propagate; the reference's GUID-only fallback is intentionally not reproduced.

All emitted prose is checked for the importer's ASCII-insensitive ModSync marker:
component headings, names, link-derived text and fields, emitted warnings,
preamble and epilogue. An embedded marker is rejected even in a code fence or
ordinary HTML comment, because the component parser searches those regions too.
This prevents a prose-only component from acquiring executable metadata on reload.
The final document also rejects triple-backtick openings whose trimmed language
is toml, json, yaml, yml or xml, matching the structured import prepass. That check
covers generated YAML comments as well: the prepass does not respect HTML comment
boundaries. Every such opener is rejected regardless of current fence parity,
since another field can change that parity. These are deliberate strict export
restrictions, applied before any output replacement.

## Explicit boundaries and deferred acceptance

- The reference importer stops at a later level-two heading after Mod List.
  Warning content containing such a heading can exclude later components when
  reimported. The exporter preserves reference placement and component order.
- Prose-only components intentionally have no comment unless DraftReview exists;
  GUIDs can regenerate and non-prose metadata can be lost, as with the reference.
- The embedded Rust mapping retains model extensions and nested option data.
  Its YAML key order, default omission and scalar presentation are not claimed
  byte-identical to YamlDotNet. Meaningful instruction fields are retained even
  where the reference serializer conditionally omits them by action type.
- The existing importer merge remains an allowlist. Adding DraftReview does not
  establish arbitrary unknown-field retention, phase-flag fidelity, or complete
  structured-to-Markdown-to-structured fidelity. Root installationWarningContent
  has no output position in the reference formatter.
- Ordered registry output reads only the stored registry. A controlled-cache C#
  comparison and resource-field inventory remain required before claiming
  serialization parity.
- Prose text with unusual headings or newlines still needs the planned
  parser-boundary corpus; no blanket lossless claim is made for arbitrary
  Markdown prose. Executable metadata markers and supported structured guide
  fences are rejected as described above.
- Source inspection found that the CLI inferred formats only from positional
  output, despite writing to the `--output` alias. The main.rs owner handles
  that separate integration correction.

No Rust tests, fixtures, harnesses, production execution, Cargo commands, network
operations or C# processes were used for this slice. Only source inspection and
formatting of the new exporter were performed. After the execution deferral is
lifted, compare ordinary reference text with line-ending normalization only,
inventory field losses through both real parsers, and exercise pending/reviewed,
gapped/conditional/rejected, diagnostics-only and option review records through
the public CLI. These receipts remain necessary for M1 acceptance.

## Review corrections in the continuing implementation

Raw review-record validation now belongs to `draft::validate_review_records` and
runs from `Guide::from_value` before normalization for every structured format,
including Markdown fences. Null or malformed review records cannot disappear
during normalization. Traversal follows guide components and Options only.

Direct-Markdown preflight counts recursive draft provenance independently from
recursive pending-review authorization. Explicitly reviewed drafts still count
as generated content; an unresolved option fences its owning component. Full
guide corpus conversion and downstream execution remain deferred.
