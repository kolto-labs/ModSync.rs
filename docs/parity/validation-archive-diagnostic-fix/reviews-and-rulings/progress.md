# Validation page plan progress

Updated: 2026-09-27
Plan BASE: 915ec69a21d90fac0395932d54e1622f8c049a18
Task 1 approved HEAD: 9d77f4ba9955ea505d3eb7969afdf5b5aee73b72
C# oracle: d31ccbd3bfcddb4705917eafa6483715c166de3b
Branch: codex/rust-port

## Latest user ruling

Defer ALL creating, porting or expanding Rust test files or inline test modules until full C# feature parity is complete. Explicit user ordering overrides earlier skill/AGENTS/test-writing requirements. Preserve existing tests and recorded evidence. Cost: automated regression coverage of subsequent changes is deferred. Current gates are builds, direct disposable CLI probes, actual native GUI probes and independent specification/quality reviews. Unchanged existing tests may run with compiler ownership permission.

- [x] Source audit and bounded plan/task briefs.
- [x] Task1 shared mapper and observed pipeline, approved915ec69..9d77f4b.
- [x] Task1 independent specification and quality reviews; fix1 P2 addressed in task-1-fix1-review.md.
- [x] phase_task_review audit: missing-source shared backend and exact full-CLI measurement, source-bound at9d77f4b.
- [x] Mandatory bounded Task1b defined before UI; ready brief: task-1b-brief.md, BASE9d77f4b.
- [ ] Task1b production implementation/direct disposable probes, then independent spec and quality approval.
- [ ] Task2 dedicated renderer/lifecycle; tests deferred per ruling.
- [ ] Task2 specification review then quality review.
- [ ] Task3 native comparison/docs and final reviews.

Prior sourceworker is inactive. Audit production build and probe are complete; compiler slot FREE/released. Parent grants compiler ownership to the fresh implementer; workers must recheck before any compiler process. User-owned AGENTS.md preserved. Native proof claims must use actual source/binary-bound receipts, not old screenshots or passing tests.

Next action: dispatch fresh Task1b implementer with task-1b-brief.md, latest ruling, Global Constraints, complete ownership and evidence. Task2 MUST wait for Task1b implementation and independent specification/quality approval. Implementers commit owned product/evidence changes for review; no Rust test additions/ports/expansions of any kind (including inline/temporary code or test harnesses), no nested subagents. Publication follows final reviews through designated integrator. Audit/planning worker does not commit.

Task1 historical evidence remains bound to original hashes. Fix review is .superpowers/sdd/validation-page-plan/task-1-fix1-review.md; broader backend semantics and native parity remain open. Full exact C# feature-parity objective remains open beyond this validation page.

## Task 1b implementer handoff (2026-09-27)

- Production commit `1cb3f4b` on BASE `9d77f4ba9955ea505d3eb7969afdf5b5aee73b72`; only `src/engine.rs` and `src/validation.rs` committed. Report: `task-1b-report.md`; full source diff: `task-1b-review.diff`.
- Shared declared conflict stage, archive/destination stage, and typed Copy/Move/Extract dry-run availability are implemented. Exact native guide directly produces **0/0/6 -> 0/1/5 -> 0/0/6**, success/exit0 for the missing-source warning case. The previous measured Rust baseline was **3/0/3**; the extrapolated full **2/0/4** remains superseded.
- Default and no-default-feature production builds and strict production Clippy both pass, plus format. Initial and final build/source/binary receipts remain separately bound in `docs/parity/validation-missing-source-fix/`; initial 80 CLI probes and final 14 matcher-refresh probes are documented there. All 68 report calls preserve checkpoint manifests. No Rust tests/harnesses were created, modified or run.
- Strict missing-source installs reject before snapshots/content writes; remaining phase preserves consumed-base definitions/receipt without replay. Root/definition/game mismatches and pending recovery remain hard failures. Owned late runtime failure -> pending retry block -> verified restore is directly recorded. Lock files/empty state directories are reported separately from root mutation.
- Compiler ownership released after final verification. Independent specification and quality review are **pending**; Task 1b is not marked approved/complete and Task 2 remains gated. Native renderer/callback/cancellation interaction evidence remains open. COMPATIBILITY.md delta/limits are provided in the report for Task 3 integrator ownership.

### Task 1b fix round 1 (2026-09-27)

- Independent P1 selected-option FOMOD regression fixed in source-only commit `88b2dd9`, BASE `1cb3f4be432b9c5435af442517990fedb2a97482`; only engine.rs changed. The executor and independent gate share `chosen_options`, instruction applicability and component checks. Completed subtrees and unreachable/unselected choices are excluded; owning option configuration/registered-state gates apply.
- Exact review fixture now fails FomodConfiguration independently without dry-run (1/0/4) and with dry-run (2/0/4). Configured-owner, unselected, unreachable and instruction-excluded variants pass. Reached component constraints and prior registered state stay hard failures; generic Extract absence retains archive/dry-run semantics.
- Both production builds, strict production Clippy variants and format pass. 39 new real CLI invocations / 37 unchanged report manifests are separately source/binary-bound under `docs/parity/validation-missing-source-fix/fix1/`. Consumed-base options are excluded in remaining reports; configured suffix passes and root/definition/game mismatch guards remain hard. No Rust tests/harnesses created/modified/run.
- Fix report appended to `task-1b-report.md`; fix-only diff is `task-1b-fix1-review.diff`. Compiler free. Scoped independent re-review **pending**; no Task 1b approval or Task 2 dispatch claimed.

## Authoritative missing-source correction and handoff

- Audit: `docs/parity/validation-missing-source-audit.md`; evidence: `docs/parity/validation-missing-source-evidence/current/{build-receipt.json,receipt.json,stdout.json,stderr.txt,tree-before.json,tree-after.json}`.
- Exact native C# guide hash `3c184ea3922d6454cc3b18dc26e3a8e7250fc164ca9075ab83723ef78e0c873b` matches the disposable Rust copy. C# native missing-source result0errors/1warning/5passed.
- Production `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo build -j2` at9d77f4b exited0; HEAD/source hashes unchanged, production diff empty. Copied binary SHA256 `93f0ae8ec686e8858c20976dccf290e236ddd704d54c37a6f89a37f6f70cd984`.
- Actual full `validate --validation-report --full --dry-run --use-file-selection` measured **3 errors / 0 warnings / 3 passed**, exit1, empty stderr. Failing stages: Conflicts, ComponentValidation, DryRun. Before/after manifests identical; no install invoked by audit.
- Historical partial `--dry-run` **2/0/1** remains valid for its actual flags. Earlier full **2/0/4** was an unsupported extrapolation, not measurement. `task-1-report.md` now contains a correction annotation retaining the historical sentence/logs and pointing here/current receipt.
- Task1b must fix the shared three-stage cause and produce source-missing0/1/5, restored0/0/6 while strict actual install rejects absence before writes. It preserves standalone simulated conflicts, known FOMOD-state failures, completion receipt/recovery safeguards and truthful observed-progress boundaries. No renderer compensation or second installer.
- Production-only acceptance uses builds and real CLI/native disposable probes; no new/expanded/ported Rust test files, modules, temporary tests or Rust test harnesses until overall full feature parity. Existing contradictory report assertion is preserved/documented, not used to preserve the defect.

## Task 2 start (2026-09-27)

Parent approved Task1b `9d77f4b..88b2dd9ea8331281c2a14bc6cb1dbaabcdb2a662` including fix1 selected-option FOMOD review resolution. Task2 starts at that HEAD. Own src/ui.rs and focused src/ui/validation_page.rs, preserve tests unchanged. Compiler assigned to Task2, target /tmp/modsync-rs-target, jobs2. No Rust test creation/modification/runs, subagents or publication. Existing inline worker test directly matching untagged report messages may require later adaptation to run-ID messages; its source remains frozen.

## Task 2 bounded implementation handoff (2026-09-27)

Task1b approved before start as above. Production source spans88b2dd9..47c3e675b93abcbd8660ad84a0b1fdd74b4aad95; initial556b165, fix1 7e804fa, fix2 2856543, utility route5548736, responsive47c3e67. Independent source reviewer approved through47c3e67. Final production default/headless builds, strict production Clippy and format pass; frozen test tail unchanged and no Rust tests created/modified/run. Native private-Xvfb proof includes final clean0/0/6→warning0/1/5→Environmentfailure1/0/0→repaired0/0/6/Next, real independent clipboard, real600×400 Stop/cancellation, setup/nonvalidation clears and theme hover. Separate5548736 binding proves stale rejection and utility routing. Exact remaining native cells and typography/layout differences are explicit in task-2-report.md. Final source/binary/fixture receipt: /tmp/modsync-rust-validation-task2/fix3/receipt.json. Core fixture manifests restored, owned native processes stopped, compiler free. Final evidence review and durable Task3 packaging delegated to parent; broader exact C# parity remains open.

### Final package F1 correction

099834c108696cec3f413a003e13bb869d9ea73a on63c8bc6 normalizes shared failed archive outcomes with idempotent ERROR: severity; rawschema unchanged, messagevalue correction explicit. Focused defaultbuild/strictproductionClippy/fmt pass. Direct CLI missingExtract1/1/4, corruptExtract3/0/3, missingCopy0/1/5 and strictinstallation rejection preserve game/mod content. Actual private-native componentcards, auto/manual firstissue after scroll-away, and independent clipboard reports verified for both Extract cases. Receipt /tmp/modsync-validation-archive-fix/receipt.json; report task-2-archive-fix-report.md; ownedprocesses exited, compilerfree. No Rusttests changed/run. Scoped final review pending; fullgoal remainsopen.
