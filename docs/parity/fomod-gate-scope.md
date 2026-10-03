# FOMOD residuals: gate scope, missing archives, folder sources, run choices

Contract for the tracker's two FOMOD residual items ("ownership, hard-dependency
scope, missing-archive warnings and folder sources named like archives" and "per-archive
embedded/run-choice preservation; the older replacement warning"). Reference is the
pinned C# `d31ccbd3`; paths below are under `src/ModSync.Core/` there. Evidence:
[fomod-gate-scope-proof](fomod-gate-scope-proof/README.md). F15 stays partial.

## Triage at `15b11cb`

| Item | C# | Rust at `15b11cb` | Status |
| --- | --- | --- | --- |
| Run-choice ownership (which component or option takes an entry) | No validate/install choices file; Fetch binds by component and archive | `ChoicesFile::update_guide_resolved` (`src/fomod.rs:1244`) applies an entry only to components/options that register the archive or may extract it; rooted CLI runs resolve physical members (`src/fomod/resolved_choices.rs:104`, called from `phase::prepare_with_roots`, `src/main.rs:3186`) | Closed in source by S2b and `a185ced`; only runtime proof was missing. Proved here. |
| Gate hard-dependency scope | `FomodConfigurationGate.Validate` seeds the selected components and expands them with `ExpandWithHardDependencies` (`Services/Fomod/FomodConfigurationGate.cs:107`, `:200-259`), selected or not | `validate_fomod_configuration` (`src/engine.rs`) walked `guide.ordered()?` and `check_component`, so an unselected dependency failed with "requires selected component" before any FOMOD finding, and its archives were never gated | **Open; fixed here** |
| Missing registered archive without prompt state | Soft warning per component: `WARNING: <component>: Registered archive '<f>' is not on disk under the mod directory. Re-download the archive (Fetch Downloads), then configure the FOMOD installer before validate/install.` (`FomodConfigurationGate.cs:131-141`, `Services/Validation/InstallationValidationPipeline.cs:457-460`) | One warning per archive with Rust-only wording and no component (`src/engine.rs`, missing branch); identical texts were de-duplicated | **Open (wording and attribution); fixed here.** The with-state hard error was already fail-closed and keeps its Rust text. |
| Folder source named like an archive (`Pack.zip/`) | n/a (no choices file on these verbs) | Rooted resolver treats a verified directory as a folder and takes only its real members (`resolved_choices.rs:104`); the S2b README residual (compared as a file name) predates `a185ced` | Closed in source; proved here |
| Per-archive embedded/run-choice preservation | n/a | `merge_relevant` (`src/fomod.rs:1282`) keeps other archives' entries | Closed by S2b (proof 254/254); re-proved here through a folder source |
| Older "replaces all embedded choices" warning | n/a | Stale text in `COMPATIBILITY.md` (S2a section), `PROGRESS.md`, and the ambiguous `--fomod-choices` help "overrides embedded choices for this run" (`src/main.rs:719`) | **Open (docs and help); fixed here** |

## Behavior after this slice

- **Gate scope.** The gate seeds the selected, non-completed components in install
  order (guide order when the order is invalid), with their chosen options, then adds
  hard dependencies transitively by GUID, selected or not, skipping completed and
  unknown GUIDs. A dependency outside the selection has only its registered archives
  gated, which is all the C# gate checks. FOMOD findings are reported first. Rust's
  stricter readiness checks (unselected dependency, restriction conflict, invalid
  order, unreviewed drafts) still fail the gate afterwards, so no run that failed
  before now passes: C# only warns for a missing dependency, Rust keeps rejecting it.
  Readiness checks for an option still run only after the gate's findings. Soft gate
  warnings collected before such a failure are kept
  ([fomod-gate-warnings.md](fomod-gate-warnings.md)).
- **Completed components (Rust-only divergence).** C# gates completed components
  too, because its gate runs on every selected component before resume. Rust skips
  completed seeds (as before this slice) and completed dependencies without walking
  through them. Under Rust's completed-prefix invariant, a dependency of a completed
  component is itself completed, so the difference is not reachable in practice.
- **Missing archive without state.** One warning per registering component, in C#'s
  wording, prefixed by the component name as C#'s report prints it. It is a counted
  FomodConfiguration warning in the report and a `WARNING:` event on plain
  `validate`/`install`; it never blocks. With recorded prompt state the absence is
  still a hard error ("missing from the mod directory after configuration state was
  recorded").
- **Run choices.** Unchanged: merged per archive, never saved. The help text now says so.

## Out of scope

The CLI selects every component unless `--use-file-selection`, `--select` or a profile
narrows it, so the dependency case needs a narrowed selection. Desktop reconfiguration,
the S3 restart/collision corpora and .NET casing-table exceptions remain open. The C#
"configured without archive-scoped instructions" warning is now settled by
[fomod-archive-scoped.md](fomod-archive-scoped.md). Its Rust form fires when the
installer maps no files for any choice. The Rust-only no-route warning is kept.
