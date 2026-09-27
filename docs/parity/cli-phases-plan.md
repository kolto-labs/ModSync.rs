# Shared GUI/CLI phase preparation plan

Status: provisional implementation plan against commit `0a4f9dd` and `cli-phases-preflight.md`. No implementation or runtime proof is claimed.

## Goal and global constraints

Add `--phase all|base|remaining` to the existing CLI `validate`, `conflicts`, and `install` commands, defaulting to `all`. Both frontends prepare phase inputs through one library function, then call existing `Engine` methods.

- No second installer, automatic coordinator, public completed-ID list, receipt reader/editor, or new recovery format.
- GUI keeps C# additive selection; CLI keeps reset/filter/name/GUID/profile selection. Shared equality starts at identical post-selection guides.
- Preserve receipt verification of roots, serialized definitions, and game hashes. Never trust receipt definitions as preparation input or weaken comparison.
- Preserve the committed `0a4f9dd` desktop lifecycle: effective base definitions are pending only during work, overlaid into the loaded guide only after guarded success, then represented by completed IDs.
- Keep install order fail closed. No replay, late backfill, best-effort continuation, or implicit phase change.
- Preserve existing human, JSONL, and validation-report output contracts.
- Reject phased Direct Markdown until separate-invocation definition stability is proven.
- Use temporary fixtures only; do not touch real game/mod installations.
- No GUI redesign, wizard-document changes, visual-parity claim, full-build claim, push, or public PR.

## Load-bearing engine prerequisite

`Engine::analyze_conflicts_remaining` currently sets a prefix and calls `analyze_conflicts`, which checks recovery cleanliness but not completion receipt roots, definitions, or game hashes. Before CLI remaining conflicts can be exposed, add the existing `recovery::verify_completion(roots, guide, completed_count)` check to the shared conflict-analysis execution path. Zero-prefix ordinary analysis must retain current behavior. Do not duplicate this check in CLI preparation.

Regression: valid remaining conflict analysis reports suffix writes only, changes neither tree/snapshot nor `pending`; missing/corrupt receipt, changed completed definition, and tampered completed output all reject with identical pre/post manifests and no `pending` creation.

## Shared preparation contract

Provisional API; final names may follow implementation conventions:

```rust
pub enum InstallPhase { All, Base, Remaining }
pub enum CompletedContext<'a> {
    None,
    Retained { ids: &'a [String] },
    Reconstruct,
}
pub struct PreparedPhase {
    pub guide: Guide,
    pub completed_ids: Vec<String>,
}
pub fn prepare_install_phase(
    selected_guide: &Guide,
    phase: InstallPhase,
    external_choices: Option<&ChoicesFile>,
    completed: CompletedContext<'_>,
) -> Result<PreparedPhase>;
```

| Phase | Valid context | Required result |
|---|---|---|
| `All` | `None` | Whole selected guide, choices applied, no completed IDs |
| `Base` | `None` | Top-level `WidescreenOnly=true` deselected after choices preparation; nonempty base; no completed IDs |
| GUI `Remaining` | `Retained { ids }` | Exact completed definitions already in selected guide; choices applied only outside validated prefix |
| CLI `Remaining` | `Reconstruct` | Candidate base reconstructed from same guide and stable choices; returned prefix judged by unchanged receipt verifier |

Reject all other pairs. Retained/reconstructed prefixes must be nonempty, unique, selected, present, and an exact ordered prefix. Remaining suffix must be nonempty. Never insert/select components to manufacture a prefix or reinterpret invalid `Remaining` as `All`.

## External FOMOD choices across CLI processes

First slice supports only definition-equivalent reconstruction inputs. Current choices metadata embeds the whole external payload into each unfinished component. Therefore a suffix-only file edit can still change completed-base serialized metadata and correctly fail verification.

- Same choices used for Base must be supplied for Remaining when they affected base definitions.
- Changed completed choices reject with `Completed component definitions differ from receipt`.
- Suffix-only external edits may also reject today because of whole-payload metadata. This is a known CLI limitation, not GUI parity.
- Missing/corrupt supplied file fails preparation. Omitting a choices-dependent file yields definition mismatch.
- Supporting suffix-only edits later requires evidenced per-component choice projection, including nested options, wildcard/unregistered sources, and receipt compatibility. It is outside this slice.

GUI retained context may allow suffix edits because it owns the historical prefix. That does not prove separate CLI equivalence.

## Preflight interface table

| Adapter / request | Phase/context | Engine call | Required boundary |
|---|---|---|---|
| GUI pre-base validation | `All/None` | full report + dry run | Includes selected widescreen; failure blocks Next |
| GUI base conflicts/install | `Base/None` | ordinary conflict/install | Nonempty base; retain definitions only after success |
| GUI remaining operations | `Remaining/Retained` | matching remaining API | Valid nonempty prefix/suffix; guarded lifecycle and receipt checks |
| CLI `--phase all` | `All/None` | existing ordinary API | Default preserves current behavior |
| CLI `--phase base` | `Base/None` | existing ordinary API | No wide events/writes; empty base rejects |
| CLI `--phase remaining` | `Remaining/Reconstruct` | matching remaining API | Stable reconstructed prefix, nonempty suffix, clean valid receipt |

## CLI flags, reports, and lifecycle

- `--phase all|base|remaining`, default `all`, on shared `InstallArgs`.
- Remaining report mode uses fixed `validate_report_remaining` (full + dry run). Permit redundant `--full`/`--dry-run`; reject `--dry-run-only`.
- Reject validation-report shaping flags on phased install/conflicts instead of suppressing events without a report. Preserve existing All compatibility.
- Report preparation/prefix errors: one ErrorReport JSON document on stdout, empty stderr, exit 1.
- Report receipt/pipeline errors: one `Report` with `success=false` and `inputPath`, empty stderr, exit 1.
- Normal/JSONL errors: nonzero with stderr diagnostic and no invented success event.
- Empty base, empty prefix, and empty suffix reject before work. GUI retains its separate empty-widescreen presentation transition.
- Second Remaining after successful suffix rejects as exhausted; it is not resume and replays nothing.
- Repeated Base must fail safely with no mutation.
- Failed/cancelled suffix with pending snapshot blocks remaining validation/conflicts/install until existing restore succeeds; retry then runs suffix in order.

## Within-task decisions

| Decision | Ruling | Costs / limits |
|---|---|---|
| Public phase surface | One enum flag on existing commands | Explicit multi-command flow; no automatic orchestration |
| Completed IDs | Internal prepared result only | Separate CLI must reconstruct; callers cannot override |
| Desktop history | Reuse IDs plus definitions already overlaid in loaded guide | Valid only for current GUI lifecycle; no second store |
| CLI history | Reconstruct candidate from stable guide/choices, then verify receipt | Definition-equivalent inputs required |
| Suffix-only changed choices | Reject when whole-payload metadata changes base definition | CLI limitation until per-component projection exists |
| Remaining conflicts | Add receipt verification in shared engine path | Small engine change plus focused coverage required first |
| Empty/exhausted phase | Explicit no-work error | CLI differs from GUI empty-phase presentation |
| Report flags | Fixed remaining full+dry report; reject incompatible flags | Some redundant flags accepted; dry-run-only unavailable |
| Failure recovery | Existing pending/restore/retry contract | User runs restore explicitly; no coordinator |

## Tasks

### Task 1 — verify remaining conflicts and extract shared preparation

Own `src/engine.rs`, the new/small shared phase-preparation module plus `src/lib.rs`, and focused engine/preparation tests. Add remaining-conflict receipt verification first. Implement phase/context validation, Base filtering, retained lifecycle0a4f9dd handling, stable-input reconstruction, and whole-payload choice limitations. Do not edit CLI dispatch or wizard documentation.

### Task 2 — wire CLI phases and executable regressions

After Task 1, own `src/main.rs` and CLI-focused tests. Add the phase enum flag, compatibility checks, exact error/output shapes, dispatch to existing engine APIs, happy path, forged receipts, empty/exhausted/repeated phases, choices reconstruction limits, and failure → blocked retry → restore → success.

### Task 3 — migrate desktop adapter and differential tests

After the shared API is stable, own only the bounded `src/ui.rs` preparation calls and focused differential tests. Preserve the committed guarded success overlay and selection/navigation behavior. Compare identical post-selection guides for All/Base/stable Remaining; record GUI-retained versus CLI-reconstructed divergence for changed choices.

### Task 4 — integration review and durable status

Review Tasks 1–3, run focused then required checks once, resolve findings, and update `GUI-CLI.md`/`ACCEPTANCE.md` with exact evidence and limitations. No exact GUI parity or full-build claim.

## Exact executable sequence and assertions

Fixture: unchanged selected guide where Base `Move`s/consumes `mods/base.txt`; selected Wide is `WidescreenOnly=true`, depends on Base, and writes `game/Override/wide.txt`.

```bash
modsync validate --input guide.json --game-dir game --source-dir mods --use-file-selection --phase all --validation-report --full --dry-run
modsync install --input guide.json --game-dir game --source-dir mods --use-file-selection --phase base --json
modsync validate --input guide.json --game-dir game --source-dir mods --use-file-selection --phase remaining --validation-report
modsync conflicts --input guide.json --game-dir game --source-dir mods --use-file-selection --phase remaining
modsync install --input guide.json --game-dir game --source-dir mods --use-file-selection --phase remaining --json
```

Assert All reports Base+Wide; Base emits/writes Base only and consumes source; remaining validation reports Wide only and writes nothing; conflicts reports suffix only and writes nothing; Remaining install emits Wide only, preserves exact base bytes, writes exact wide bytes, records ordered Base+Wide definitions, and leaves no pending marker.

Fail-closed executable cases with pre/post manifests: missing/corrupt receipt, wrong roots, changed definition, tampered base output, invalid prefix, empty base/prefix/suffix, exhausted/repeated phases, missing/corrupt/omitted/changed choices including suffix-only whole-payload change, incompatible report flags, and failed suffix → all remaining commands blocked → restore → successful retry.

## Shared frontend differential tests

| Case | Expected evidence |
|---|---|
| Identical post-selection `All/None` | Equal prepared guides, no IDs |
| Identical post-selection `Base/None` | Equal wide exclusion and ordered base definitions |
| Stable choices, GUI `Remaining/Retained` vs CLI `Remaining/Reconstruct` | Equal prepared guides/IDs; both verify |
| Changed completed choice | GUI retains history; CLI reconstructs mismatch and rejects; neither replays |
| Suffix-only external edit with whole-payload metadata | GUI may retain; CLI rejects; documented non-parity limitation |
| Different selection policy outputs | No equality assertion until selected guides match |

## Evidence required and still unproven

Required receipts: exact commands/exits/stdout/stderr; guide/choices hashes; before/base/remaining manifests; completion receipt hashes/ordered IDs; event sequences; snapshot/pending state around no-write operations, rejection, failure, restore, retry; serialized prepared-guide equality/difference.

Still unproven:

- Shared API and CLI phase parsing do not exist.
- Remaining conflict receipt verification is absent.
- No executable CLI phase regression or stable-input reconstruction proof exists.
- Separate-process suffix-only choice edits are unsupported with current metadata.
- Direct-Markdown phases, restart coordination, full-build parity, exact GUI parity, accessibility, and cross-platform behavior remain open.
