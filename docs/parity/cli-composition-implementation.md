# F26 conversion, merge, validation and install composition

Source-only implementation against pinned C# `ModBuildConverter.cs` and
`AutoInstructionGenerator.cs`. No CLI commands, downloads, installs, tests or CI
were executed for this slice. Earlier production gate receipts do not cover it.

## Conversion and merge

- Shared processing accepts `--download`, `--source-path`, `--concurrent`, an
  explicit Nexus key/settings directory, `--spoiler-free`, and `--auto`.
- The spoiler overlay applies only five spoiler metadata fields. Ordinary guide
  fields are projected out with informational receipts. Ambiguous or malformed
  overlay edits reject the whole candidate.
- Acquisition completes before inference/output and requires terminal success
  plus resolved FOMOD configuration. Archives already downloaded remain available
  on failure; the input guide is not saved by the prephase.
- URL generation resolves all enabled filenames through metadata discovery,
  without acquiring archive bodies. Incomplete discovery blocks publication.
  Existing local archives use shared inference; missing archives and recognized
  loose files produce review-required drafts. Unsupported extensions retain
  explicit review diagnostics. This is not an assertion of executable readiness.
- Local archive inference has additive cancellation support and batch publication;
  the legacy library entry point retains its signature. Both convert and merge now
  accept `--auto-generate-local`, requiring `--source-path`, through the same
  cancellable processing path.
- Both authoring verbs accept repeatable `--select` / `-s`. The shared CLI selector
  applies category/tier or explicit name/GUID selection after inference and before
  rendering. Warnings go to stderr. Omitting the selector preserves existing Rust
  conversion selection behavior; it does not introduce another reset to all-selected.
- Merge supports named existing/incoming/output arguments and optional stdout
  output. Existing positional invocation and shared merge decisions remain.
- `convert --merge` / `convert -m` normalizes to that same merge parser and
  dispatcher. It requires named `--existing` / `-e` and `--incoming` / `-n`,
  preserving output, format, field preferences and shared processing flags.
  Option values are not interpreted as routing flags. Duplicate merge switches,
  positional conversion inputs, `--input`, `--stdin`, and conversion-only draft
  controls are rejected explicitly instead of ignored. Legacy GUI preload routing
  remains separate. Ordinary convert invocations are unchanged.
- Conversion/merge retain the cancellation token through generation, rendering,
  and the publication boundary. Download composition requires a distinct output
  file when an input file exists, using canonical source-alias checks.

## Install

`--download` uses the prepared selected guide before constructing the engine.
Remaining-phase acquisition excludes completed-prefix IDs and verifies their
full definitions remain unchanged. Checkpoint resume requires acquisition to
finish separately before reviewing a resume boundary. The default source path
for installation without a profile is the absolute input guide parent, matching
the pinned reference. Profile path precedence is unchanged. Whole-run recovery
remains default and per-component checkpoints require explicit opt-in.

## Validation

Text/JSON output selection and errors-only text filtering are supported. Full,
dry-run and dry-run-only options route through the shared validation pipeline
without requiring a separate report selector. Incompatible legacy JSON-event
selectors are rejected. Missing roots produce a structured setup failure for
JSON output; reference stage-level equivalence and root-free success are not
claimed.

## Evidence boundary

Independent source review found and corrected completed-prefix mutation risk and
lost cancellation authority after acquisition. Native behavior, invocation/output
comparisons, production compilation and full parity acceptance remain open.

Validation `--output` now accepts case-insensitive `text`/`json` spellings, matching
the pinned reference's ordinal-ignore-case comparison. This changes only argument
value matching; enum dispatch, option conflicts, and report/exit behavior remain
unchanged. Source-only correction; no invocation, test, or runtime probe performed.

The shared install/validate/resume backend selector now matches recognized patcher names without regard to case, including `HoloPatcher`. Existing aliases and unknown-backend rejection remain. This follows the pinned reference backend comparisons; invocation acceptance remains deferred.
