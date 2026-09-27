# CLI phase plan preflight

Reviewed `cli-phases-plan.md` against current commit `0a4f9dd`, including the retained-effective-definition desktop correction. Read-only investigation; no builds, tests, production edits, or commits. This is a next-plan review, not another wizard acceptance review.

**Verdict: revise before implementation.** A single shared preparation function plus existing engine dispatch is the right-sized architecture. Two asserted contracts do not hold in the current code; three smaller lifecycle contracts need to be made explicit. No new coordinator is needed.

## 1. Remaining conflicts do not currently verify completion

**Plan referent:** architectural boundary and interface table (`cli-phases-plan.md:24`, `:110`), fail-closed cases (`:173-183`). The plan promises missing/corrupt/stale receipts and modified completed game bytes reject all remaining commands.

**Checked:** `Engine::analyze_conflicts_remaining` (`src/engine.rs:541-547`) only calls `set_completed_prefix` then `analyze_conflicts`. The latter (`:469-473`) calls `ensure_clean` and `run`; `run` skips completed IDs but does not verify their receipt. Prefix validation checks IDs/order, not receipt roots, definitions, or game hashes. The existing comment claiming the same verified-completion contract as validation is stronger than its implementation.

**Consequence:** delegating CLI remaining conflicts to that API exactly as planned can report a successful suffix analysis with no completion receipt or with tampered base output. The plan's fail-closed matrix cannot pass unchanged engine code.

**Smallest correction:** add the existing `recovery::verify_completion` call to the shared conflict-analysis execution path before suffix simulation (with completed count, so ordinary zero-prefix analysis keeps its behavior). Keep the verifier itself and receipt format unchanged. Add missing/corrupt/definition/hash rejection coverage for remaining conflicts; do not recreate verification in CLI preparation. This is a bounded shared-engine prerequisite to the CLI adapter, not a new coordinator.

## 2. Suffix-only external-choice changes cannot survive current CLI reconstruction

**Plan referent:** remaining reconstruction and its final sentence (`:86-93`), external FOMOD case 3 (`:191`), differential suffix-change row (`:207`).

**Checked:** `ChoicesFile::apply_to_guide` delegates to `apply_to_remaining(..., &[])` (`src/fomod.rs:715-724`). `update_guide` embeds the entire supplied archives vector into every unfinished component (`:880-893`), regardless of which archive that component installs. During Base there is no completed prefix, so the exact base receipt includes widescreen choices too. `verify_completion` compares entire serialized definitions (`src/recovery.rs:180-205`).

**Concrete counterexample:** choices contains base.zip and wide.zip. Install Base and record its definitions. Change only wide.zip from Default to Alternate. CLI Reconstruct applies the changed whole choices file to base again, so the base FomodChoices metadata differs even though its installed files and effective base.zip selection do not. The unchanged verifier correctly rejects. Desktop Retained succeeds because it owns historical effective definitions. Those two contexts cannot satisfy the plan's equality promise for this input.

**Smallest correction for this slice:** keep the fail-closed reconstruction and explicitly require the external choices content used to reconstruct base to remain definition-equivalent. Change case 3 and the differential row to expect CLI rejection when suffix edits also change metadata embedded in base; retain GUI support for suffix edits. Remove the unconditional claim that suffix-only external changes remain allowed in CLI. Omitted/changed irrelevant choices may also cause mismatch because the receipt is definition-based.

If suffix-only changes must succeed across separate CLI processes now, first specify a separate shared per-component choices-projection contract, including archive matching for nested options, wildcard/unregistered instruction sources, and existing receipt compatibility. That is additional shared FOMOD work; it must not be hidden inside the CLI adapter or bypass receipt comparison. Reading historical receipt definitions as authoritative preparation input would contradict this plan's stated boundary.

## 3. Completed context representation must follow the committed desktop lifecycle

**Plan referent:** provisional `Retained(&[Component])` API and steps 1-3 (`:32-61`, `:119-121`).

**Checked:** after `0a4f9dd`, `pending_base: Vec<Component>` holds effective dispatch definitions only while base work runs. `installed_phase` verifies the dispatch guide/root context, overlays those definitions into the loaded guide, sets completed IDs, and clears `pending_base` (`src/ui.rs:1873-1904`). Later remaining dispatch therefore has a loaded guide containing retained effective definitions plus IDs, not a persistent separate vector.

**Smallest correction:** freeze the plan against that committed shape. Prefer `Retained { ids: &[String] }` whose exact definitions already live in the selected guide, or explicitly derive a temporary component slice from that guide and the retained IDs. Do not add a second long-lived completed-definition store. Preserve the current install-context guard and success-only overlay; phase preparation must not promote pending definitions into completed state. Specify missing/duplicate/mismatched retained IDs as errors and never silently insert missing completed components or change current selection to manufacture a prefix.

Also define the valid phase/context matrix: All/Base accept None; Remaining accepts Retained/Reconstruct; reject invalid pairs. The proposed enum currently makes invalid combinations representable without defining their behavior.

## 4. Empty, exhausted, repeated, and failed phases need explicit command outcomes

**Plan referent:** public `--phase` contract and exact command lifecycle (`:9-16`, `:127-183`).

**Checked:** `Guide::ordered` rejects an empty selected set (`src/model.rs:320`); `Engine::set_completed_prefix` rejects a completed count equal to the selected count (`src/engine.rs:557-559`) but accepts an empty prefix when work exists. `verify_completion` returns immediately for count zero (`src/recovery.rs:181-183`). The GUI separately skips an empty widescreen phase without calling the remaining engine. Its behavior cannot be inferred from the CLI command name.

**Smallest correction:** state and test these CLI rules:

- Base with no selected base components rejects during shared preparation.
- Remaining requires a nonempty reconstructed/retained base and a nonempty suffix; reject an empty prefix rather than accidentally bypassing receipt verification. Do not silently reinterpret Remaining as All.
- No selected widescreen suffix is a clear no-work rejection for this slice, using the existing engine semantics; GUI keeps its explicit empty-phase presentation transition.
- A second Remaining invocation after successful suffix install rejects against the now-full receipt; it is not a resume command and must not replay anything.
- Following suffix failure/cancellation with a pending snapshot, remaining validation/conflicts/install reject until the existing restore operation succeeds. Then retry the same phase in order. Add one executable failure → blocked retry → restore → successful suffix sequence, preserving consumed base source/output.

These are adapter/preparation contracts around existing engine/recovery operations, not justification for another workflow coordinator.

## 5. Finish the report/flag and error-location matrix

**Plan referent:** `:113`, external choices case 5 (`:193`), preserve existing output contract (`:123`).

**Checked:** current shared `InstallArgs` has `--full`, `--dry-run`, `--dry-run-only`, and `--validation-report` (`src/main.rs:340-351`). Remaining reports use a fixed full+dry-run method; prefix errors return `Err` before that report exists, while receipt failures become an unsuccessful Environment stage. Existing CLI report mode wraps setup failures in ErrorReport JSON and prints failed pipeline reports with exit 1 and no extra stderr (`src/main.rs:1034-1065`). Missing external files fail file read before receipt comparison (`:1000-1003`).

**Smallest correction:** explicitly permit redundant `--full` and `--dry-run` with remaining report mode, reject `--dry-run-only`, and define how report flags apply to install/conflicts (do not silently suppress install events with a report flag while emitting no report). Preserve existing All behavior if compatibility requires it; document any rejection limited to new phased modes.

In report mode, assert one JSON document on stdout and empty stderr for preparation/prefix errors (ErrorReport) and receipt/pipeline errors (Report.success=false). Preserve input_path population. For normal/JSONL modes, specify nonzero failure and stderr diagnostics without invented success events. Correct external case 5: a supplied missing file yields a read/preparation error; omitting the flag may yield a completed-definition mismatch. Both reject before mutations, but the error referent differs.

## What holds

- Applying frontend profile/selection rules before shared preparation preserves the intentional C#-derived GUI additive versus CLI reset/filter distinction. Identical post-selection inputs should share projection and choices logic.
- All/Base use existing ordinary operations; Remaining uses existing prefix-aware operations after the conflict verification prerequisite above. No new installer loop, scheduling object, coordinator, recovery format, or trusted receipt reader is required.
- Reconstructing candidates and letting unchanged receipt verification judge them is a sound fail-closed boundary when its definition-equivalence limitations are stated honestly.
- Direct-Markdown rejection is a reasonable bounded scope until prepared definitions across separate invocations are proven stable.
- The Move-consumption fixture, explicit command sequence, manifests, ordered events, and no-write checks are appropriate acceptance evidence. None were executed by this preflight.

## Minimal implementation order after plan correction

1. Freeze the retained-context API against 0a4f9dd and settle suffix-choice/empty-phase/report contracts above.
2. Correct shared remaining-conflict receipt verification and add focused engine coverage.
3. Extract pure shared preparation; migrate desktop dispatch without changing worker completion/context ownership.
4. Add the CLI phase argument and dispatch through existing engine methods; keep current output adapters.
5. Run executable happy path, receipt failures, FOMOD reconstruction limits, empty/repeated phase and restore/retry lifecycle, plus differential post-selection preparation checks.

Full exact GUI acceptance remains open and is not a deliverable of this CLI phase plan.
