# Instruction Source normalization follow-up

Status: implemented at62b6f70, evidence0f48868, independently SPEC and QUALITY approved for this bounded slice.
[Plan review](source-normalization-plan-review.md) records the corrections and approval.
F01 and F05 remain **partial**. This is one loader-field fix, not domain completion.

## Observed defect and reference

`Guide::from_value` calls `normalize_component` (`src/model.rs:223,560`) before
deduplication. Stored Sources retain authored text; `source_key` (`:431-463`)
normalizes only comparisons. A bare-relative Copy Source therefore compares as
normalized but fails Rust validation because execution requires a placeholder.

Pinned C# is `d31ccbd3`, read-only checkout
`/home/brunner56/.codex/worktrees/5813/ModSync`. Under `src/ModSync.Core/`:

- `Services/ModComponentSerializationService.cs:2140-2145,2349-2354` excludes
  Choose case-insensitively and normalizes other Sources.
- `Utility/Serializer.cs:111-179,237-249` formats and prefixes path strings,
  recognizes placeholders case-insensitively, and filters empty array items.
- `FileSystemUtils/PathHelper.cs:823-847` removes leading backslashes, converts
  separators, collapses repetitions except after a colon, and trims trailing ones.

## Bounded implementation contract

1. Own `src/model.rs` only, plus new direct-CLI evidence. Extract/reuse one
   `fix` plus prefix helper from `source_key`; apply it to stored non-Choose
   instruction Sources inside the existing `normalize_component` instruction walk.
   Preserve its existing recursive Options traversal (`:638-640`). Do not change
   Destination, arguments, execution path resolution, Markdown parsing, or exports.
2. Skip Choose **case-insensitively** before action canonicalization. Preserve
   GUID Sources verbatim, including mixed-case action spellings. Keep all existing
   deduplication algorithm, Choose-reference remapping, and format exclusions.
   Dropping empty array items intentionally makes their normalized comparisons equal.
3. For nonempty string Sources, normalize and prefix as the current `source_key`
   does. For arrays, drop empty/whitespace text items and normalize surviving
   strings. Preserve existing errors for unsupported non-string shapes; do not
   silently discard malformed values. Missing/null Source retains the empty default.
   Preserve bare empty/whitespace strings for downstream validation, deliberately
   avoiding C#'s whole-conversion exception for those inputs.
4. Recognition of `<<modDirectory>>`, `<<kotorDirectory>>`, and
   `<<gameDirectory>>` is case-insensitive; preserve authored placeholder casing.
   Otherwise prefix `<<modDirectory>>` and normalize separators. Use this same
   helper for equality and stored Sources. It must be idempotent because equality
   may normalize already-normalized text. Keep normalization before deduplication.
5. Keep `Roots::relative/resolve/check` unchanged. Traversal, unknown placeholders,
   drive letters and symlinks must remain rejected at execution. Leading `/` is
   absorbed into a mods-relative path by C# normalization; it must never access
   the host's absolute path. Prove containment with an inside-root positive fixture
   and a distinct outside-root sentinel, not merely a missing-file error.
6. Structured loaders share `Guide::from_value`. Markdown embedded YAML uses that
   path, as directly measured. Embedded TOML sharing it is source-derived and must
   be probed. Rust fenced JSON is a separate Rust-only route. C# legacy-bullet
   metadata is supported separately while Rust rejects it; that remains open and
   is explicitly outside this slice.

## Existing evidence and its limits — 2026-09-28 UTC

| Evidence | Measured coverage | What remains unmeasured |
|---|---|---|
| [JSON baseline](source-normalization-probe/receipt.json) | Seven cases; five differing separator/prefix shapes, game placeholder and Choose agree | Post-fix results |
| [Other structured formats](source-normalization-formats/receipt.json) | Same five differences in TOML/YAML/XML | Expanded edge matrix in each format |
| [Twelve-shape C# matrix](source-normalization-format-oracle/receipt.json) | C# normalized Source/Options agree across four rendered formats | Authored scalar shape in each format, negative cases, post-fix Rust |
| [Authored scalar oracle](source-normalization-scalar-oracle/README.md) | Sixteen direct C# calls: relative/empty/whitespace/numeric scalar across four authored formats; numeric JSON/TOML/YAML silently empties Source | Post-fix Rust; XML numeric text is not numeric token equivalence |
| [Expanded probes](source-normalization-expanded/receipt.json) | JSON strings/arrays/empty/null/options, embedded YAML, Rust-only fenced JSON, small Copy and path probes | Embedded TOML, full validation equivalence, all post-fix behavior |

The expanded C# binary hash is
`89d9b2361aee4364f95151b41db59a4133122a6ad41bc2703e232cf10a4f5a6b`;
fixed Rust baseline hash is
`c3e2f60f1fbae36b2efc666173052c47d7e1ebf87326cc4c8490e1a4b67934cf`
(source `714e59b`, bound in the Switch desktop receipt). The C# runs used bubblewrap
with isolated network, read-only host, mounted `/dev` and `/proc`, and disposable
HOME/XDG/TMPDIR roots. Exact captured commands accompany the expanded receipt.

Rust rejects bare `x/foo.tga`. C# Copy execution for the **same Source shape**
succeeds, and Rust Copy using C#-normalized Source writes identical bytes
(`5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03`).
The input files have different component identifiers; they are not identical guides.
C# used `--skip-validation` after an environment prerequisite failure. That flag
bypasses the full pre-installation validation pipeline (`ModBuildConverter.cs:3576`),
so these runs prove Copy behavior and bytes, **not validation equivalence**.

C# bare empty/whitespace Source throws during conversion; its array branch filters
those strings. Rust currently accepts both shapes. Mixed-case placeholders are
preserved by both converters but fail the measured install in both engines at
different points. Do not infer supported execution from matching serialization.
Rust's post-prefix traversal, unknown-placeholder and drive-letter probes reject;
the absolute-looking case currently proves a missing mods-relative file only.

## Direct acceptance after implementation

No Rust tests or temporary/custom Rust harnesses may be created, changed or run.
Use real production CLIs, disposable generic fixtures, pinned C# read-only sandbox,
and exact argv/exit/stdout/stderr plus source/binary hashes in retained receipts.

- Compare actual normalized Sources across JSON/TOML/YAML/XML and common Markdown
  embedded YAML and TOML. Include nested Options, mixed-case Choose actions, each
  supported placeholder, mixed case, separator shapes, strings/arrays, whitespace,
  empty/null/missing, malformed values, traversal, unknown placeholders and drives.
  Record format-specific differences rather than assuming JSON proves all formats.
- Verify normalization idempotency by load/save/load. Re-run existing loader-dedup
  direct probes and check Choose references, option order and format exclusions.
- Execute a safe Copy from the same authored guide in Rust and C#, compare output
  bytes, and separately label any skipped C# validation. Verify inside-root absolute
  reinterpretation and outside sentinel preservation; include symlink rejection.
- Build and strict-Clippy production `--lib --bin modsync` in CLI-only and default
  modes, run fmt, and confirm all existing Rust test sources remain byte-identical.
- Spec review, then independent quality review. Update COMPATIBILITY.md and progress
  with measured behavior and remaining gaps. Commit only owned paths; no push while
  the test-freeze/CI conflict remains unresolved.

No whole-format export, broad Markdown parser, complete F05 action, full-build,
GUI, live-game, or cross-platform acceptance follows from this bounded change.
