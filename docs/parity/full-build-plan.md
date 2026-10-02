# F29 full K1/K2 build-equivalence plan

Status: ready for dispatch only after full C# feature parity is declared and the active serialized UI/engine lanes are complete. This plan creates no Rust test and no harness in the current lane. Pinned C# oracle: `d31ccbd3bfcddb4705917eafa6483715c166de3b`; audit and feasibility basis: [full-build-audit.md](full-build-audit.md).

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md`, all unrelated worktree state, the pinned C# worktree, guide checkout, archive cache, manual trees, ledgers, rollback snapshots, game baselines, and every live game install. Current project precedent requires isolated state and no mutation of game/mod trees during validation (`AGENTS.md:9-13`).
- Do not create, modify, compile, or run Rust tests or an acceptance harness until the acceptance owner has recorded full C# parity. Before that boundary, work is limited to read-only inventory, review, and documentation.
- Do not build in the documentation lane. When later authorized, Rust commands run on the host with a lane-owned `CARGO_TARGET_DIR` and `-j 2`; the four ordinary local gates remain those named in `AGENTS.md:6-7` but are not F29 proof.
- Every graded path is non-interactive after the run bundle is signed. Pre-resolve selections, FOMOD choices, archive acquisition, patcher namespaces, trust, and overwrite policy; a prompt, credential gate, CAPTCHA/MFA boundary, or native file picker blocks the run instead of asking the user or guessing.
- Use no live download during a graded install. Acquisition finishes first into a content-addressed, read-only bundle; the C# runbook confirms payloads come from third-party hosts rather than `mod-builds` itself (`docs/local_desktop_agent_runbook.md:204-210`).
- Never use `--best-effort`, concurrent install, skipped validation, missing-source continuation, component-failure continuation, or an unreviewed draft in a graded run. The shipped C# helper uses several of those permissive flags and is discovery-only (`scripts/agents/cli_full_build_pipeline.sh:242-256`).
- Preserve guide order and stop on first failure. Restore an independently verified pristine clone before retrying; never backfill an earlier mod at the end (`AGENTS.md:10-12`).
- Never seed from or write to a real Steam/GOG install. Never hardlink writable game files. Use independent disposable copies from a read-only verified baseline and prove baseline/live-install manifests unchanged. The existing K2 workflow demonstrates this isolation (`MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-22`; `INSTALLATION_PROGRESS_K2_2026-07-30.md:79-97`).
- The fake template is limited to routing, acquisition, and rejection probes because the current helper explicitly says game parsers/detectors reject its plain-text files and it writes literal fake executable/TLK bytes (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:2-8`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:19-34`). Even the format-aware mock helper remains synthetic and test-assembly-generated (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_mock_kotor_install.sh:1-6`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_mock_kotor_install.sh:65-79`). Full validation, patch/install, and game smoke require real game bytes in disposable clones.
- Pin exact guide, archive, selection, FOMOD, platform, extraction-tool, patcher-backend, environment, and baseline hashes. Same filenames or the same guide commit are insufficient: the guide checkout is dirty and historical archives include renamed byte-identical twins (`docs/parity/full-build-audit.md:19-29`, `docs/parity/full-build-audit.md:64-80`).
- The comparator is external to both products. Product receipts/checkpoints corroborate but do not grade their own implementation; Rust's completion manifest is product-authored (`src/recovery.rs:163-177`).
- Raw patch-byte equality is the default. A semantic exception requires a predeclared format-specific comparator, a byte-difference receipt, independent review, and explicit acceptance-owner approval; it never emerges from a failing run.
- No source file is shared between simultaneous workers. `src/ui.rs` remains serialized in the existing order: install-progress, completion-pages, then mod-selection. `src/engine.rs` is separately serialized. F29 source work waits for both queues and obtains explicit ownership before editing either file.
- No feature row, `COMPATIBILITY.md` boundary, or F29 status changes until all review and execution gates below pass. Existing evidence explicitly leaves F29 unverified (`docs/parity/ACCEPTANCE.md:5-10`, `docs/parity/ACCEPTANCE.md:64`).

## Acceptance topology

One immutable run bundle feeds three isolated execution lanes and two presentation adapters:

```text
guide bytes + archive hashes + selections + pristine baseline + backend hash
                                |
                         immutable run spec
                    /-----------|------------\
               C# shared     Rust shared     manual replay/oracle
                    \-----------|------------/
                       external step manifests
                                |
             tree diff + patch-byte diff + reviewed dispositions
                                |
                     disposable-clone game smoke
```

The canonical behavioral comparison is C# shared/CLI versus Rust shared/CLI. Native C# and Rust GUI runs must import or emit the same prepared run-spec projection and use the same shared operations; GUI layout/progress evidence is graded separately because shared calls alone do not prove GUI parity (`docs/parity/GUI-CLI.md:1-13`, `docs/parity/GUI-INVENTORY.md:168-175`). Manual evidence is a third oracle, not a tiebreaker that may overrule two differing automated outputs without a guide/ledger referent.

## Run-bundle contract

Task 1 produces a read-only directory whose root hash covers:

- `run-spec.json`: schema version, K1/K2, game distribution/layout, platform policy, locale/timezone, tool versions, Rust/C# commits and source-content hashes;
- `guides/`: exact Markdown and TOML files with SHA-256 and the dirty corpus description;
- `archives.tsv`: one selected resource per row with component GUID, source URL identity, local basename, size, SHA-256, archive type, payload-list hash, and acquisition disposition;
- `selection.json`: ordered component GUIDs, option GUIDs, FOMOD plugin choices, tier/category/full-build preset, widescreen decision, and every explicit skip with guide/ledger referent;
- `baseline-manifest.jsonl`: immutable real-game seed manifest and a separate identity record for each discovered live install;
- `patcher.json`: exact backend, executable hash, version receipt, argv policy, namespace/INI selection, and external-program allowlist;
- `manual-map.jsonl`: guide component/GUID to manual ledger step, input hash, choice, operation, expected changed paths, and reconciliation status;
- `normalization.json`: empty by default; only pre-reviewed harness/product metadata exclusions, never payload extensions or paths chosen after a diff.

The four local guide files currently expose 189 K1 and 145 K2 TOML components, while the cache exposes 454 archive/installer candidates; these are planning-scale estimates only (`docs/parity/full-build-audit.md:19-29`, `docs/parity/full-build-audit.md:64-80`). Task 1 is complete only when every selected resource maps to one verified content hash and every alternative/platform skip is explicit.

## Manifest and byte-comparison contract

The same standalone manifest command runs outside the game tree for all lanes. It emits deterministically sorted JSON Lines with reversible relative-path encoding, entry kind, size, mode/executable bit, and SHA-256. Symlinks and special files invalidate the run unless the baseline specification explicitly requires and safely represents them. This extends the safety properties of Rust's internal manifest, which already uses relative paths/SHA-256 and rejects symlinks/special files (`src/recovery.rs:83-131`), without trusting a product-specific implementation.

Capture:

1. baseline before clone;
2. each fresh clone before execution;
3. `step N-1 -> step N` and `baseline -> step N` after every top-level component;
4. final tree before game smoke;
5. final tree after game smoke;
6. baseline and all discovered live installs after every lane.

The step boundary comes from typed shared progress only after component success. Rust's observer emits component completion after `component` returns (`src/engine.rs:969-993`); C# invokes each ordered component and records success/checkpoint afterward (`src/ModSync.Core/Services/InstallationService.cs:1226-1284`). If either frontend cannot expose an unambiguous machine-readable boundary, add that fact to the shared library after its serialized owner lane; do not parse display prose.

For every step compare C#↔Rust, C#↔manual, and Rust↔manual. Report the first divergent component and every changed path with add/remove/modify/type/mode, hashes, sizes, and later-overwrite status. The patch-byte set is the union of paths changed during external/patcher components and paths named by patcher provenance. Store baseline/C#/Rust/manual bytes or content-addressed references plus `cmp`/binary-diff receipts for mismatches. A zero final diff does not waive a nonzero step diff (`docs/parity/ACCEPTANCE.md:64`).

## Task sequence and serialized ownership

Tasks are ordered. Read-only review can overlap; mutable work cannot.

| Task | Owned files/state | Responsibility | Start gate |
|---|---|---|---|
| 0. Full-parity declaration | parity ledger/reviews only | Acceptance owner explicitly records full C# feature parity for F01-F28, permitting creation/execution of the F29 harness | All feature owners and independent final review approve; F29 remains unverified |
| 1. Freeze inputs | new external evidence directory only; no source | Hash guides/cache/baselines/tools; create run spec, archive ledger, selection/FOMOD manifest, manual map; identify live installs read-only | Task 0 |
| 2. Comparator probe | existing `scripts/diff_kotor_installs.py`, `scripts/agents/incremental_diff_gate.py`, thin external wrappers/receipts; no Rust tests | Extend the existing format-aware diff path with deterministic manifests, clone-isolation proof, baseline/live guards, raw patch-byte receipts, and run-spec identity | Task 1; independent script/security review |
| 3. Shared fact gaps | `src/engine.rs` **or** pinned C# shared service, never both concurrently | Only if Task 2 proves a missing machine-readable run-spec or component boundary; additive factual API, no presentation | Existing `src/engine.rs` queue complete; explicit serialized ownership |
| 4. CLI adapters | `src/main.rs` and C# CLI adapter in separate turns | Import/export run spec; disable network/permissive flags; emit ordered facts without new business rules | Task 3 approved or formally unnecessary |
| 5. Paired K1 lanes | independent disposable C# and Rust K1 clones, K1 manual tree/ledger read-only, fresh dispute clones, and external receipts | Run C# K1, then Rust K1, compare immediately, reconcile/replay manual differences, and pass K1 Gate D; do not start K2 with an unresolved K1 row | Tasks 1-4; archive coverage 100% |
| 6. Paired K2 lanes | independent disposable C# and Rust K2 clones, K2 manual tree/ledger read-only, fresh dispute clones, and external receipts | Run C# K2, then Rust K2, compare immediately, reconcile/replay manual differences, and pass K2 Gate D | K1 Gate D passes; no shared mutable tree/process |
| 7. Cross-game evidence audit | immutable K1/K2 receipts and manual maps only | Check that dispositions, exclusions, backend policy, and manual mappings are consistent across both completed game gates; no new install or oracle edit | K1 and K2 Gates D pass |
| 8. Native GUI lanes | `src/ui.rs` only after install-progress → completion-pages → mod-selection; C# GUI separately | Prove prepared run-spec equality, execute full K1/K2 disposable native runs, capture route/progress/completion | Serialized UI queue complete; Tasks 5-7 clean |
| 9. Game smoke | disposable final clones, run-local profiles/saves | Main menu + fixed save + patch-sensitive scene/dialogue; pre/post manifests and media/log evidence | Zero unexplained payload/patch diff |
| 10. Closeout | `docs/parity/ACCEPTANCE.md`, `COMPATIBILITY.md`, `GUI-CLI.md`, evidence index | Independent review, status update, exact residual limits | All prior gates pass |

Workers state exact file and state ownership before beginning. They do not reset, clean, repair, or reuse a failed tree. C# and Rust full installs are serialized per game if they share archive storage bandwidth or external patcher state; read-only hash comparison may parallelize only after run outputs are immutable.

## Task 1: archive acquisition and run-spec freeze

1. Inventory the top-level cache and selected guide resources without following extracted-directory state. The observed root and current extension counts are recorded in `docs/parity/full-build-audit.md:64-80`.
2. Hash every archive candidate once into content-addressed storage or a read-only index. Record duplicates by hash without choosing by basename. The documented K2 renamed-archive case proves filenames alone are unsafe (`MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:39-58`).
3. Resolve every selected guide resource to exactly one hash. Inspect archive payload lists and mod readmes when filenames collide. Reject zero-byte/corrupt inputs; a historical zero-byte archive blocked a complete K1 batch (`MANUAL_INSTALL_PROGRESS_2026-07-30.md:231-235`).
4. Acquire only unresolved resources. Record URL, provider, timestamp, final size/hash, redirect chain where available, and any authentication/manual-download boundary. Never bypass CAPTCHA/MFA or place credentials in receipts.
5. Fresh-extract each selected archive into a run-owned staging tree. Do not accept previously extracted folders because historical `Move` operations consumed their content (`MANUAL_INSTALL_PROGRESS_2026-07-30.md:64-68`).
6. Freeze identical K1/K2 selection and option manifests for C#, Rust, and manual reconciliation. “Select all” still requires explicit resolution of mutually exclusive alternatives, platform-only components, widescreen, FOMOD groups, and known note-only steps; K2's historical skip table shows why (`MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:81-95`).

Gate A: selected archive coverage is 100%; each row has one content hash; extraction succeeds in isolation; there are zero unresolved FOMOD choices, drafts, filename ambiguities, missing/corrupt files, or implicit platform skips. Independent reviewer signs the run-spec root hash before execution.

## Task 2: safe baseline and comparator probe

Start from the existing comparison assets, not a second unrelated diff stack. `diff_manual_vs_auto.sh` already binds manual/automated trees and emits all-file JSON/Markdown comparison output (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/diff_manual_vs_auto.sh:15-54`). `incremental_diff_gate.py` already combines automated checkpoint scope with manual-ledger paths and applies format-aware TLK/2DA/container/raw diagnostics (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_diff_gate.py:1-40`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_diff_gate.py:302-369`). Extend their schemas/adapters only after review; preserve old CLI/output behavior for historical lanes.

1. Identify pristine, legally obtained K1 and K2 baselines read-only. Record distribution, platform layout, full manifest, and sentinel hashes/counts. K2/Aspyr uses `steamassets` with lowercase content folders (`MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-22`).
2. Discover live installs and record read-only guard manifests; do not expose them inside an automation VM/container if patchers can auto-detect installations.
3. Create three independent clones per game. Try reflink only in a throwaway probe: mutate one cloned byte and prove baseline/sibling hashes unchanged. Otherwise use full copies. Never use hardlinks.
4. Give each process a run-local HOME/XDG/TMP/Wine prefix/config/log/save root. Make archive bundle and baseline read-only. Pass explicit game/mod destinations to every backend.
5. Exercise the external comparator against tiny non-Rust fixtures: empty directory, case-only names, Unicode names, executable modes, add/remove/modify, large file, and deliberate symlink/special-file rejection. This is a direct script probe, not a Rust test.
6. Deliberately point a probe at a sentinel outside the disposable root and prove the wrapper rejects it before launch. Manifest all live guards afterward.
7. Preserve format-aware diagnostics but always emit raw SHA-256 for every patch-byte-set member. A TLK/2DA/container semantic match may classify a raw mismatch; it cannot erase it or satisfy the raw-byte gate without a preapproved exception.

Gate B: clone isolation, path containment, deterministic manifesting, mismatch reporting, and unchanged baseline/live guards are independently reviewed. No product install begins on a failed or merely assumed clone.

## Tasks 3-4: shared library API versus presentation

Required shared facts are immutable run ID/spec hash, ordered selected component and option GUIDs, archive hashes, phase/component boundary, patcher/backend identity, actual result, and output receipt path. They belong in the shared engine/service when absent. Selection, dependency resolution, guide order, external-process policy, install mutation, and completion remain shared-library behavior; CLI and GUI only import/export, render, and retain facts. Rust installation already belongs in `src/engine.rs` and both adapters consume it (`src/engine.rs:786-861`, `docs/parity/GUI-CLI.md:5-13`).

No adapter may infer success from progress, parse logs into boundaries, reorder components, resolve archives independently, or substitute a local selection. Existing typed install progress and terminal result remain authoritative (`docs/parity/install-progress-plan.md:68-77`, `docs/parity/install-progress-plan.md:100-116`).

Gate C uses direct production CLI probes, not Rust tests:

- `prepare/plan` output is byte-identical across repeated calls and C#/Rust projections match after schema normalization;
- legacy CLI JSON remains unchanged when acceptance flags are absent;
- network-disabled strict install rejects missing input before mutation;
- permissive/concurrent flags conflict with acceptance mode;
- component completion appears only after successful mutation and no completion follows failure.

Independent API-contract, correctness, reliability, and safety review is mandatory before full builds.

## Tasks 5-7: paired graded runs and cross-game audit

For each game and implementation:

1. verify bundle/spec/baseline/backend hashes;
2. create a fresh clone and prove its manifest equals the baseline;
3. run full validation with network disabled and archive bundle read-only;
4. start strict serial installation with external processes explicitly allowlisted;
5. at each authoritative component-success boundary, capture external manifests and patch-byte provenance;
6. on any error/cancel/missing input, stop, retain the failed tree/receipt, and mark the run invalid;
7. retry only from a new verified clone with a new run ID after the defect or input changes and review approves it;
8. capture final product receipt plus external manifest, then prove baseline/live guards unchanged.

Compare C# and Rust immediately after each same-game pair. Do not continue to GUI/game claims while an earlier component differs. Reconcile the first mismatch against the guide and manual ledger, then classify it as input drift, selection drift, product defect, backend nondeterminism, manual-oracle drift, comparator defect, or unresolved. Only input/comparator corrections permit rerunning without a product parity change.

The fixed execution order is K1 C# → K1 Rust → K1 diff/reconciliation → K1 Gate D, then K2 C# → K2 Rust → K2 diff/reconciliation → K2 Gate D. An unchanged checkpoint is not completion: the existing incremental loop admits that it can mean either fully installed or stalled (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_install_diff_loop.sh:165-171`). Each lane must emit an explicit terminal receipt proving the expected ordered component GUID/count, with no skipped, failed, blocked, or merely unattempted selected component.

Within each paired game task, the existing manual tree and ledger stay read-only. Their documented completion makes them high-value evidence (`MANUAL_INSTALL_PROGRESS_2026-07-30.md:3-36`; `MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-37`), but their documented platform skips and historical choices prevent automatic golden status. Reconcile and replay disputed steps into a new same-game baseline clone using the exact frozen archive/options/backend before that game's Gate D; append a new external receipt rather than editing the historical ledger. Task 7 then audits the two already-closed game evidence sets for cross-game policy drift and cannot defer an unresolved K1/K2 manual row.

Gate D per game:

- all selected components accounted for in authored order;
- zero permissive skips, unresolved drafts, or late backfills;
- zero unexplained per-step or final C#↔Rust payload differences;
- patch-byte gate passes under the predeclared backend policy;
- manual map has no unresolved row and every accepted difference names a guide/ledger referent plus acceptance owner;
- baseline, bundle, and live-install guards unchanged.

## Task 8: native GUI acceptance

After the current `src/ui.rs` sequence completes, run the C# desktop using its preload arguments (`docs/local_desktop_agent_runbook.md:96-107`) and Rust desktop with equivalent run-spec preload/import. For K1 and K2 record controlled-size native video/screenshots, accessibility/keyboard evidence, exact prepared-spec hash, validation result, install progress, completion/recovery state, and final manifest.

The GUI full runs use the same immutable bundle and strict mode as CLI. “Fetch Downloads” is not used in the graded run because the runbook confirms it performs real third-party downloads (`docs/local_desktop_agent_runbook.md:182-210`). Native evidence must show GUI and CLI prepared the same ordered GUID/option/archive/backend facts; matching screenshots without this receipt do not prove installation parity (`docs/parity/GUI-INVENTORY.md:168-175`).

Gate E: both games complete in both GUIs, GUI prepared-projection hashes exactly match their shared/CLI run-spec projections, output trees satisfy Gate D, and route/progress/completion evidence passes the existing GUI inventory.

## Task 9: disposable game smoke

Smoke only byte-clean final clones. Use a run-local launcher/profile/save path and verify the executable resolves inside the disposable clone. Record executable hash and process command/cwd before launch. For each game:

- reach the main menu;
- load a fixed save compatible with the pinned baseline/build;
- traverse one area or dialogue whose resources were changed by a patcher and one loose-file visual/audio asset where automation can observe it;
- save or exit without writing outside the run root;
- capture logs, screenshots/video, crash status, and before/after manifests.

This is bounded smoke evidence, not a claim that hundreds of mods were exhaustively played. A successful launch cannot excuse any tree/patch mismatch; conversely, byte equality without launch cannot satisfy the F29 game-evidence row (`docs/parity/ACCEPTANCE.md:64`, `COMPATIBILITY.md:149`).

Gate F: both C# and Rust K1/K2 disposable outputs reach the prescribed checkpoints with no crash or out-of-root write, and post-smoke differences are limited to predeclared run-local save/config/log paths.

## Review and closeout gates

1. **Input review:** archive/resource mapping, choices, manual map, baseline identity, patcher hash.
2. **Harness review:** deterministic manifests, safe clone/path guards, receipt schema, deliberate-failure probes.
3. **Shared API review:** no presentation business logic, backward-compatible CLI, factual boundaries.
4. **First-difference review:** stop at the earliest mismatch; correctness owner and independent skeptic rederive from receipts.
5. **Native review:** GUI projection equality plus interaction/accessibility evidence.
6. **Game review:** bounded smoke evidence and out-of-root guard.
7. **Final F29 review:** one acceptance owner verifies every gate and updates ledgers. A build, test count, source presence, screenshot, completion page, or product-authored receipt alone is never sufficient (`docs/parity/ACCEPTANCE.md:5-10`, `COMPATIBILITY.md:149`).

The final evidence index must include run-spec root hash; all commits/content hashes; archive/baseline/backend manifests; exact commands/environment; ordered events; per-step/final/patch diffs; manual dispositions; native media/accessibility receipts; game-smoke receipts; baseline/live guard manifests; reviewer verdicts; and rerun lineage. Failed and superseded runs remain retained and visibly invalidated.

## Open questions and defaults

1. **Which guide bytes are authoritative?** Default: hash the current four files into a candidate bundle, but do not grade until the guide owner either commits/reverts the dirty K2/TOML state or signs those exact hashes. The current corpus commit alone is insufficient (`docs/parity/full-build-audit.md:19-29`).
2. **Which full-build selection?** Default: the guide's recommended full build with one explicit choice for every alternative, plus platform/widescreen decisions copied into `selection.json`; never infer “all” for mutually exclusive entries. Historical K2 skips are evidence, not automatic current defaults (`MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:81-95`).
3. **Which platform/distribution?** Default: reproduce the existing K1 Linux/Steam-style and K2 Linux/Aspyr manual baselines first, then add Windows/GOG/Steam lanes if product support claims require them. Do not compare flat K1 and `steamassets` K2 layouts as though interchangeable (`MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-22`).
4. **Which patcher backend?** Default: one byte-identical HoloPatcher executable for C# and Rust if both adapters support it; otherwise stop as unresolved. Do not compare HoloPatcher output to OdyPatcher output and call serialization differences ModSync parity (`src/engine.rs:1700-1713`).
5. **Can historical manual trees close the manual lane?** Default: no. Reconcile all rows; replay disputed or drifted steps in fresh clones. Existing trees remain immutable evidence (`docs/parity/full-build-audit.md:82-88`).
6. **Are semantic patch comparisons allowed?** Default: no; raw bytes gate. A format-specific semantic exception requires preapproval and does not erase the raw mismatch.
7. **How are per-step snapshots captured without product changes?** Default: consume authoritative typed component-success events and run an external read-only manifest command synchronously. If either product cannot pause/capture reliably, add an additive shared acceptance observer under serialized ownership; never parse UI text.
8. **May C# checkpoints be the manifest oracle?** Default: no. They are corroborating evidence because their creation/promotion is inside the product path (`src/ModSync.Core/Services/InstallationService.cs:1252-1284`).
9. **Can tasks parallelize?** Default: only immutable hash/diff/review work. Installs sharing archive I/O, patcher state, Wine prefix, baseline, or game tree are serialized. `src/ui.rs` and `src/engine.rs` always follow their named ownership queues.
10. **When does F29 change from unverified?** Default: only after Gates A-F pass for both games and the final acceptance owner signs the evidence index. Until then retain `unverified` (`docs/parity/ACCEPTANCE.md:5-10`, `docs/parity/ACCEPTANCE.md:64`).

## Plan claim blocks

claim: The acceptance harness can safely use the C# fake template for every full-guide step.
referent: `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:2-8,19-34`; `docs/local_desktop_agent_runbook.md:59-77`
checked: pinned source inspection
verdict: does not hold
confidence: high; the template contains fake executable and TLK bytes

claim: The shipped concurrent best-effort pipeline is an acceptable graded command.
referent: `scripts/agents/cli_full_build_pipeline.sh:242-256`; `AGENTS.md:10-12`
checked: script and project-order contract
verdict: does not hold
confidence: high; it permits precisely the skips/reordering that F29 must detect

claim: Existing local inputs are sufficient to begin a hash-and-coverage freeze without downloading everything again.
referent: `docs/parity/full-build-audit.md:64-80`; `MANUAL_INSTALL_PROGRESS_2026-07-30.md:47-50`; `MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:39-58`
checked: bounded top-level cache inventory plus historical archive records
verdict: holds
confidence: medium; exact selected-resource coverage remains Gate A

claim: Final C#/Rust tree equality by itself closes F29.
referent: `docs/parity/ACCEPTANCE.md:64`; `docs/parity/GUI-INVENTORY.md:168-175`; `COMPATIBILITY.md:149`
checked: acceptance ledger and existing proof boundaries
verdict: does not hold
confidence: high; step, manual, GUI, and game evidence are separately required
