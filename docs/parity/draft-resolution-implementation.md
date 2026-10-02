# Explicit generated-draft resolution

Source implementation, 2026-10-02. No behavioral or round-trip acceptance is
claimed. Filesystem tests, runtime probes and CI remain deferred.

The shared editor operation `resolveDraft` accepts a component GUID and a
`resolution` object containing `expectedIdentity`, `gapDecisions`,
`conditionDecisions`, `rejectedDecisions` and `reviewNote`. The identity comes
from `draft::review::identity` after the author has applied the instruction and
option edits. The `guide-draft-review INPUT COMPONENT` CLI emits the original
diagnostics and an `operations` template with empty decision fields; extract and
fill that operation array for `guide-edit`. Empty notes cannot approve a draft. Every original gap, conditional draft and rejected instruction
requires one nonempty explanation in original diagnostic order. A final review
note is mandatory. The shared editor validates the resulting whole guide before
returning it; its existing distinct-output save boundary remains in force.

The operation preserves the generated `OriginalResult`, stores the explicit
`Resolution`, and records `ReviewedIdentity`. Its effective `Result` has no
unresolved diagnostics. Notes document the author's decision; they do not infer
conditions, alter instructions, verify archive contents or prove the edits
faithfully implement prose. There is no bulk automatic acceptance operation.

Subsequent ordinary editor changes retain existing draft provenance and set
`RequiresReview` again. A changed resolved draft requires a new explicit
resolution; the older simple `mark_reviewed` path cannot reuse its cleared
effective diagnostics. Receipts are checked at installation review admission.
The fingerprint uses the shared ingestion normalization and includes authored
component data and nested options, excluding the outer selection flag and all
review records. Selecting a component
alone does not invalidate its receipt. Changing nested choices can invalidate an
ancestor receipt. Older uncomplicated reviewed records remain readable under
the existing diagnostics checks; new approvals gain an identity receipt.

Generated component identity changes through replacement are refused. Authoring
may still remove a component explicitly or insert a separately authored one;
review receipts are an accidental-staleness fence, not a signature or access
control against someone intentionally rewriting a guide.

The existing Markdown embedded metadata path carries the entire DraftReview
object. Native review usability, serialization normalization across every format,
restart/reload acceptance and actual execution remain unverified. Any changed
serialized definition that fails its receipt comparison remains pending review.
