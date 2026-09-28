# Remaining Markdown and draft contracts (F01/F02/F03)

Status: independently approved bounded sequence; source audit and one reference
conversion oracle, not implementation or Rust acceptance. F01/F02/F03 remain partial. Read after the approved
[source-normalization plan](source-normalization-plan.md); its implementation owner
owns `src/model.rs` until handoff. This plan changes no production or test source.

Reference: C# `d31ccbd3`, read-only checkout
`/home/brunner56/.codex/worktrees/5813/ModSync`. Paths below prefixed `Core/` mean
`src/ModSync.Core/` in that checkout. Current Rust source is the integration checkout
containing this document. Line anchors describe this inspection, not immutable IDs.

## What exists, and what the evidence does not establish

- `src/markdown.rs:29-245` already imports fenced structured guides, splits prose
  sections, resolves masters and extracts wizard content/phase flags.
  `src/markdown/extract.rs` implements default profile field extraction. Replacing
  this with a whole new parser is not the next task.
- `src/markdown.rs:247-364` reads one embedded YAML/TOML component and merges an
  explicit field set into prose. Legacy bullets are absent; the YAML diagnostic
  explicitly says so at line 317. Multiple comments and malformed structured
  metadata currently fail closed. Preserve that safety boundary.
- `src/model.rs:323` rejects Markdown export. Public conversion is already wired
  through `Guide::render` (`src/main.rs:1269-1324`), so an exporter needs a render
  branch and dedicated formatter, not another command or schema migration.
- `src/draft.rs:55-128` already generates drafts only when authored instructions
  are absent, adds download options, sanitizes, and stores `DraftReview` diagnostics.
  `mark_reviewed` rejects gaps, conditional drafts and rejected actions (`:22-41`).
  `src/main.rs:1293` opts into conversion drafting; file/install ingestion uses the
  separate shared model route. Both routes need product-path evidence.
- Historical [Markdown audit](markdown-csharp-audit.md) records 1,080 matching
  components and 35,640 projected field values across eight guides. Only 11,038
  reference values were nonempty. Its JSON projection excludes root wizard content,
  random GUID equality and Rust-only fields; Instructions/Options were empty.
  It is not authored-metadata, export, complete model, or installation proof.
- Historical [draft receipt](draft-check.json) compares 669 records supplied as
  already-parsed components. It does not establish that the current full Markdown
  ingestion followed by draft generation preserves those results. Do not repeat
  the obsolete claim that the draft parser itself is wholly missing.
- [Progress](PROGRESS.md), [acceptance](ACCEPTANCE.md) and [compatibility](../../COMPATIBILITY.md)
  retain broader gaps: custom import profiles, paste UI, archive/URL inference,
  unresolved-draft editing and actual full builds. This plan does not close those.

## Serial slice M1: public Markdown export

Owner: one fresh implementation agent, after Source normalization hands back
`src/model.rs`. Own a new `src/markdown/export.rs`, a small module bridge and
the narrow embedded-metadata merge seam in `src/markdown.rs:325-355`, and the
render branch in `src/model.rs`. Only touch
`src/main.rs` if observed public CLI format inference requires correction; in
particular verify `--output` alias inference as well as positional output.

Implement the existing public format, using C#
`Core/Services/ModComponentSerializationService.cs:4463-4478,4672-5050` as the
format authority:

1. Preserve component order. Emit preamble, `## Mod List`, separators, heading
   fallback, name text/link order, prose fields in reference order, category/tier
   defaults, language presentation, master names, and epilogue. Reuse normalized
   guide metadata precedence already used by TOML/YAML. The language helper at
   `:5051` and dependency original-name mapping require explicit source comparison.
2. Preserve authored NameFieldContent; otherwise render ordered registry URLs
   using the reference first-name/additional-link convention. Do not fetch URLs
   or populate a live download cache during export. Cache-dependent reference
   behavior must be measured with a fixed empty/controlled cache.
3. Insert the existing platform/widescreen warning content at the reference first
   qualifying component positions, respecting absent, blank and default content.
   Measure the known section-boundary interaction: reference import may stop at
   a later level-two heading following Mod List. Export/import losses caused by
   this reference behavior must be reported, not hidden by reordering components.
4. Emit `<!--<<ModSync>>` with one YAML mapping when instructions or options exist.
   Reuse the existing component serialization/resource compatibility transforms;
   no new envelope, marker version, required migration, or public command.
   Match lowercase action serialization and empty Heading behavior, including
   option instructions. Explicit Rust extension: also emit that same comment
   whenever `DraftReview` exists, even for diagnostics-only components with no
   instructions/options. Serialize and restore the existing `DraftReview` object,
   including `RequiresReview` and its complete `Result`, through the narrow
   importer merge seam. Preserve pending and explicitly reviewed states, all gaps,
   conditional notes and rejected counts without recomputing or auto-accepting
   them. Preserve this record for options as well. This is an optional field in
   the existing mapping, not a new mandatory schema or migration. C# may ignore
   the extension; C# round trips are not evidence of Rust review-state retention.
   The current importer allowlist omits `DraftReview`; without this change a
   reviewed draft reloads as pending through the warning fallback and cannot be
   accepted because its diagnostics are missing (`src/draft.rs:11-41`). Prove
   pending, reviewed, conditional/gapped/rejected and diagnostics-only Rust
   Markdown round trips using the actual CLI. Missing or malformed review records
   must never turn an unreviewed action into an executable accepted draft.
   Full component unknown-field preservation needs a measured field inventory;
   it must not be inferred from instruction counts.
5. For ordinary input, compare reference text with only declared line-ending
   normalization, then compare both parsers' recovered models. Exercise headings,
   multiline prose, ordered links, dependency names, nested options, flags and all
   wizard content. Record format losses individually (including prose-only GUID
   regeneration when reference omits a comment). Do not claim generic losslessness
   where the reference emitter does not promise it.
6. Fail before output replacement when content cannot be represented safely, such
   as an embedded comment terminator that would expose executable metadata as prose.
   Do not silently emit only a GUID on serialization failure even though reference
   `GenerateModSyncMetadata` has that fallback. Document any such strict deviation.

No validation-context warning generator is implied by ordinary CLI export: the
reference accepts an optional context. If a current public caller supplies one,
record that caller as a separate explicit requirement rather than fabricating data.

## Serial slice M2: legacy bullet metadata

Owner: a fresh implementation agent after M1, owning a new
`src/markdown/legacy.rs` and the small dispatch/merge seam in `src/markdown.rs`.
Policy: retain the approved uniform safe Rust Source normalization through
`Guide::from_value`, including the final component load at `src/markdown.rs:355`.
Do not fork normalization, bypass that final load, undo the structured-loader
fix, or alter execution roots. Preserve Choose Sources verbatim with the approved
case-insensitive action exclusion. This intentionally differs from C# legacy
parsing, whose top-level and option setters preserve raw Source lists
(`Core/Parsing/MarkdownParser.cs:1095-1103,1268-1274`). Record this as a specific
legacy compatibility difference, not equivalent stored-model behavior. Before
claiming downstream parity, compare direct production CLI stored-model output
and actual execution for the same authored legacy inputs in both implementations:
bare-relative paths, separators, placeholders, Choose references, nested options
and unsafe paths. Report stored-text and execution results separately; successful
Rust normalization does not establish that raw C# execution behaves identically.

Port the bounded grammar at `Core/Parsing/MarkdownParser.cs:1001-1293`:

- Component `- **GUID:**` before the Instructions section; exact section markers
  `#### Instructions` and `#### Options`.
- Numbered instruction GUID headers act as delimiters; instruction GUID payloads
  are obsolete and ignored. Action is case-insensitive; Overwrite is boolean;
  Source is a comma-split trimmed list; Destination is text. Preserve order.
- Option headings introduce records with GUID, Name, Description, Is Selected,
  Install State, Is Downloaded, Restrictions and nested instruction blocks.
  Retain the reference lexical distinctions between top-level and option fields.
  Do not invent Arguments, Dependencies or deeper nesting support absent from
  this grammar; detect actionable unsupported data instead of silently dropping it.
- Dispatch only recognized legacy syntax to this parser. Reference detection and
  fallback are at `:844-929`; failed YAML/TOML must not become an apparently valid
  empty legacy component. Preserve existing valid structured metadata behavior.
- Distinguish absent values from malformed ones. Explicit invalid action, invalid
  GUID/boolean, duplicate contradictory properties, truncated instruction, reversed
  section order and unrecognized actionable fields need diagnostic rejection or an
  explicit unresolved record that cannot execute. Reference silently defaults or
  drops several such values; report strict differences. Never inherit a default
  executable Action from malformed input merely to match the reference.
- Preserve component selection defaults and current prose/metadata merge precedence.
  Resolve Choose/option GUIDs without replacing their identities during source
  normalization. GUID generation for omitted valid option GUIDs is compared by
  structural mapping, not byte equality.

Acceptance includes exact authored fixtures for every grammar branch, mixed
prose plus metadata, CRLF, blank lines, multiple options/instructions, Source
commas, malformed records and valid-but-unsupported fields. Cross-read valid
reference/Rust exports through production conversion. Prove a generic safe
Copy/Choose install from legacy metadata and unchanged outside-root sentinel;
prove rejection occurs before game/mod mutation for malformed executable input.

## Serial slice M3: actual full-guide draft fidelity and remaining narrow fixes

Owner: fresh agent after M1/M2, initially evidence-only. Production changes, if
measured differences justify them, belong to `src/draft.rs`, extraction seams,
`src/direct_markdown.rs`, and existing CLI/review call sites. No replacement NLP
framework and no automatic acceptance of every generated draft.

1. Feed exact pinned `content/k1/full.md` and `content/k2/full.md` directly to both
   production CLIs, with and without direction parsing. Record source/corpus and
   binary hashes, argv, exit, stdout/stderr, output hashes and elapsed time. Compare
   ordered component fields, instructions, options and warnings; do not regenerate
   a component JSON input as a substitute for this end-to-end conversion path.
2. C# serialization can omit diagnostics or warnings. Report separately which
   fields its public output exposes. Use existing raw-object evidence only as
   supplementary historical diagnostics; do not claim current raw diagnostic
   equality from a lossy CLI projection or introduce a custom Rust executable.
3. Produce a per-component coverage inventory outside the repository for actual
   guide names: authored actions, drafted actions, gaps, conditions, rejected
   paths, option choices and zero-action components. Repository reports contain
   generic shapes, ordinals and aggregate counts only. Each unparsed or ambiguous
   instruction stays visible; no silent skip, best-effort execution or backfill.
4. Map measured discrepancies to existing ordered parser patterns and change the
   smallest responsible branch. Match reference parsing where correct; separately
   record reference drafts that are incomplete interpretations (for example a
   truncated folder or root placeholder). A matching wrong draft is not a correct
   guide install. Archive selection and installer namespace choices need actual
   archive evidence in their own bounded inference task, not prose guesswork.
5. Preserve review safety. C# `Core/Services/DirectMarkdownInstallPreflight.cs:71-105`
   counts recursively flagged options and accepts actions despite draft flags;
   Rust `src/direct_markdown.rs:42-47` uses top-level `requires_review`, blocking
   those drafts and coupling its drafted count to pending review. Reconcile
   recursive diagnostic counting separately from authorization; reviewed generated
   drafts still have provenance. Do not weaken engine review rejection at
   `src/engine.rs:1067` to get a green differential.
6. Existing `mark_reviewed` deliberately cannot accept unresolved gaps/conditions.
   A complete editing/resolution workflow needs a separate bounded spec: action
   edits, explicit condition/option decisions, retained provenance, invalidation
   after substantive edits and durable reload behavior. This is a remaining
   product requirement, not permission to set RequiresReview=false in bulk.
7. Prove install/validate automatic drafting and conversion opt-in independently
   with generic disposable inputs. Pending/ambiguous drafts must stop before writes;
   explicitly reviewed valid actions must retain review state through supported
   exports and run in order. Full-guide conversion does not authorize live installs.

## Common acceptance and stopping rules

No Rust tests, test edits, temporary Rust harnesses, all-target builds or test-running
CI while the freeze remains active. This planning pass runs no compilation,
production CLI, C# process, network operation, or live install. Later implementers
use the real production CLI and existing native UI where required. Production
`--lib --bin modsync` builds/Clippy in CLI-only and default feature modes and fmt
are allowed only in their authorized implementation slice; preserve frozen test
files and inline test regions byte-for-byte.

Run C# read-only and offline with disposable HOME/XDG/TMP and cache roots; capture
actual production commands. Compare GUID references by explicit component/option
mapping, ordered resources and absence/null/empty separately. No blanket ignore
list that hides new fields. Keep public guide source/raw outputs outside this
repository so published names never enter new fixtures or reports.

Each slice ends with source specification review, then independent quality review,
measured CLI receipts, and narrowly updated ACCEPTANCE/COMPATIBILITY/PROGRESS.
No domain promotion follows from implementing the exporter or parsing one fixture.
Custom profiles, no-component conversion semantics, unusual section boundaries,
Unicode extraction, duplicate master semantics, skipped-section diagnostics, paste
UI and unrepresented metadata fields remain explicit F02 follow-ups from the
existing audit. Archive/URL generation and editor/native review controls remain
F03 follow-ups. F29 requires separately authorized isolated full builds, verified
pre-step recovery, per-step resource comparisons and live-game proof.

The generic [legacy Source oracle](legacy-source-oracle/README.md) records the
pinned C# raw-Source conversion distinction used in the M2 plan review.
