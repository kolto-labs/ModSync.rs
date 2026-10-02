# F29 full-build equivalence audit

Status: planning audit only. No build, install, game launch, Rust test, or acceptance harness was run for this document. F29 remains **unverified** (`docs/parity/ACCEPTANCE.md:5-10`, `docs/parity/ACCEPTANCE.md:64`). The Rust checkout examined here is `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`; the pinned C# oracle is `d31ccbd3bfcddb4705917eafa6483715c166de3b` (`docs/parity/GUI-CLI.md:1-3`).

## Scope and proof boundary

F29 asks for full K1 and K2 equivalence to both the pinned C# implementation and manual installs, not another fixture-scale installer check. The existing ledger requires isolated C#/Rust runs with identical inputs and selections, per-step hashes, final file and patch semantics, GUI/CLI equivalence, and game smoke evidence (`docs/parity/ACCEPTANCE.md:64`). Existing round trips cover 189 K1 and 145 K2 component models but explicitly do not prove installation (`docs/parity/ACCEPTANCE.md:96`); the compatibility ledger likewise says no full K1/K2 build, manual-reference comparison, launch, or gameplay validation has been performed for the port (`COMPATIBILITY.md:149`).

This audit therefore distinguishes five claims which must not be collapsed:

1. the same guide and archive bytes were supplied;
2. the same components, options, platform decisions, and patcher backend were selected;
3. C# and Rust produced the same ordered mutations and final game-tree bytes;
4. those bytes agree with a guide-following manual install, or every difference has a reviewed disposition;
5. each frontend prepared the same shared run and reported the real outcome.

Shared engine calls alone do not prove C# equivalence (`docs/parity/GUI-CLI.md:1-3`), and a native disposable install is separate from full behavioral parity (`docs/parity/GUI-INVENTORY.md:168-175`).

## Pinned inputs examined

| Input | Observed state | Consequence |
|---|---|---|
| Rust port | Git `HEAD` was `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`; the worktree already contained unrelated modified/untracked lanes | The acceptance run must record a commit plus hashes of every in-scope dirty input; it must not treat `HEAD` as a content identity. Current unrelated state is protected by the binding constraints in `docs/parity/install-progress-plan.md:5-14`. |
| C# oracle | `/home/brunner56/.codex/worktrees/5813/ModSync` resolved to `d31ccbd3bfcddb4705917eafa6483715c166de3b` | All C# source citations below are relative to that pinned worktree. |
| Real guide checkout | `/run/media/brunner56/MyBook/Workspaces/ModSync/mod-builds` resolved to Git `336b36d911b879b0254da5d08a6cce72c28607e5`, but `content/k2/full.md` was modified and `TOMLs/` plus `content/extra.md` were untracked | The guide corpus is not reproducibly identified by its Git commit alone. The harness must hash and copy the four exact guide files into an immutable run bundle before either implementation reads them. |
| K1 guide | `content/k1/full.md` had 3,460 lines; `TOMLs/KOTOR1_Full.toml` had 5,012 lines and 189 `[[thisMod]]` records | The current ledger's 189-component serialization statement is consistent with the local TOML count (`docs/parity/ACCEPTANCE.md:96`) but is not an install result. |
| K2 guide | `content/k2/full.md` had 2,841 lines; `TOMLs/KOTOR2_Full.toml` had 4,028 lines and 145 `[[thisMod]]` records | The current ledger's 145-component serialization statement is consistent with the local TOML count (`docs/parity/ACCEPTANCE.md:96`) but is not an install result. |

The file counts and dirty-state observations above came from the read-only commands `git -C <root> rev-parse HEAD`, `git -C <root> status --short`, `wc -l <four-guide-files>`, and `rg -c '^\[\[thisMod\]\]$' <two-tomls>` on 2026-09-27. They are feasibility observations, not durable acceptance receipts; Task 1 in the plan creates the durable hashes.

## C# execution oracle

The C# full-install entry point filters selected components, validates the mod directory and FOMOD configuration, initializes the coordinator, and receives an ordered selected list (`src/ModSync.Core/Services/InstallationService.cs:1180-1221`). It checks cancellation before each component, reports progress in that order, skips coordinator states already completed/blocked, and invokes `component.InstallAsync` for each active component (`src/ModSync.Core/Services/InstallationService.cs:1226-1249`). Successful components may create and promote checkpoints (`src/ModSync.Core/Services/InstallationService.cs:1252-1284`); missing sources or component failures can instead be skipped under permissive flags (`src/ModSync.Core/Services/InstallationService.cs:1286-1317`). An equivalence run must therefore prohibit `--best-effort`, `--continue-on-mod-failure`, missing-source continuation, and any other setting that converts an incomplete build into a terminal run.

`ModComponent.InstallAsync` captures the managed-install pre-index, executes the authored instruction list against the real filesystem, records telemetry, optionally deploys staged files and patcher provenance, and marks the component complete only on success (`src/ModSync.Core/ModComponent.cs:841-918`). C# managed deployment and ordinary direct writes are different modes; one pinned mode must be used in all three comparison lanes rather than comparing a staged deployment with a direct Rust install (`src/ModSync.Core/ModComponent.cs:856-895`).

The shipped C# helper is useful discovery material but is not the F29 oracle command. It merges Markdown and TOML while preferring existing order/instructions/options/links (`scripts/agents/cli_full_build_pipeline.sh:181-201`), and its install mode explicitly uses concurrent best-effort execution, skips validation, and allows a 72-hour download window (`scripts/agents/cli_full_build_pipeline.sh:242-256`). Those flags defeat fail-stop equivalence and must not appear in an acceptance run.

The desktop runbook proves that the C# GUI accepts preloaded instruction/game/mod paths and describes the actual wizard path (`docs/local_desktop_agent_runbook.md:79-128`). Its full-build recipe selects all, fetches downloads, validates, and proceeds only after inspecting validation (`docs/local_desktop_agent_runbook.md:182-203`). It also warns that `mod-builds` contains metadata rather than payload archives and that downloads come from third parties (`docs/local_desktop_agent_runbook.md:204-210`). This makes archive-byte pinning a prerequisite, not a detail to defer to the GUI.

## Rust execution target

Rust `install_observed` takes both game and mod locks, requires clean recovery state, scans, verifies any completion prefix, performs a full silent preflight, freezes the ordered remaining list, then creates the recovery snapshot (`src/engine.rs:793-820`). It executes that list, records completion, finalizes recovery, and emits success only after finalization (`src/engine.rs:821-838`). The ordered executor rejects selected components with no instructions, processes components serially, and emits component completion only after every instruction returns success (`src/engine.rs:942-995`). Within a component it checks cancellation, evaluates instruction conditions, executes in authored order, and stops on the first error (`src/engine.rs:997-1053`). These are the correct fail-stop semantics for the Rust lane.

Rust external actions require explicit process trust; validation only checks their inputs and cannot simulate their effects (`src/engine.rs:1019-1036`, `src/engine.rs:1549-1564`). Patcher execution stages named INI input when necessary, runs the selected backend against an explicit destination, rejects unsuccessful processes and patcher logs containing errors, then rescans the tree (`src/engine.rs:1567-1695`). Backend arguments differ deliberately: KPatcher gains `--console` on non-Windows, while OdyPatcher gains `--cli -y` (`src/engine.rs:1700-1713`). The run specification must pin the same backend and backend binary hash for C# and Rust, or patch-byte differences will not isolate ModSync behavior.

Rust currently creates one whole-run recovery snapshot and writes a completion manifest of the final game tree (`src/recovery.rs:14-21`, `src/recovery.rs:163-177`). Its internal tree manifest is relative-path to SHA-256 and refuses symlinks/special files (`src/recovery.rs:83-131`). That implementation is useful corroboration, but the acceptance comparator must be external and identical for C#, Rust, and manual trees; a product-authored receipt cannot grade its own output.

## Disposable tree audit

The current C# helper `create_template_kotor_install.sh` explicitly says its plain-text files are rejected by game-format parsers/detectors and writes literal fake executable/TLK bytes (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:2-8`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:19-34`). It is suitable for route, acquisition, rejection, and simple loose-file probes, but not for full acceptance: real patchers and full guides modify or inspect executable, TLK, module, KEY/BIF, movie, and platform-layout content. The pinned runbook's “minimum expected structure” wording is limited/stale relative to the current helper warning (`docs/local_desktop_agent_runbook.md:59-77`).

The full harness therefore needs two seed classes:

- **synthetic template:** disposable fake tree for frontend routing, source discovery, and rejection checks only (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:2-8`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:19-34`);
- **real immutable baseline:** a verified pristine K1 tree and a verified pristine K2/Aspyr tree, copied into separate run-specific C#, Rust, and manual targets before every attempt.

The newer `create_mock_kotor_install.sh` is stronger for format-aware synthetic probes, but its own header says it generates layouts without copying game data and drives the C# test assembly (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_mock_kotor_install.sh:1-6`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_mock_kotor_install.sh:65-79`). It remains synthetic and cannot replace the real immutable baseline for game smoke or full patch-byte acceptance.

The existing K2 manual record demonstrates the safe pattern: it seeded with `rsync -a` from a pristine oracle rather than hardlinks or the live Steam tree, verified an empty override plus fixed TLK/module facts, and deliberately avoided a different live modded install (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-22`). The C# K2 automated record likewise reset a disposable tree from the oracle and says the real Steam install was never touched (`/run/media/brunner56/MyBook/Workspaces/ModSync/INSTALLATION_PROGRESS_K2_2026-07-30.md:79-97`).

Hardlinks are forbidden for acceptance seeds: a patcher that edits a file in place can mutate the supposed baseline or sibling lanes. Every lane gets an independent reflink only if a write-isolation probe proves copy-on-write, otherwise a full byte copy. Before and after each run, the harness manifests the source baseline and every discovered live install; any changed baseline/live-install byte is an immediate invalidation, not a warning.

## Local archive/cache feasibility

A bounded read-only inventory on 2026-09-27 examined `/run/media/brunner56/MyBook/kotor_mod_archives` with:

```text
timeout 45s sh -c 'ls -1pU /run/media/brunner56/MyBook/kotor_mod_archives |
  awk ...'
FILES=458 DIRS=410 ZIP=159 7Z=161 RAR=132 EXE=2 OTHER_FILES=4
```

Thus the root presently exposes **454 archive/installer candidates** by extension (`159 + 161 + 132 + 2`), alongside 410 extracted directories. The two top-level executable candidates visible in the read-only listing are `tslrcm2022.exe` and `3C-FD Tool v4.1.2.exe`. The exact set meant by “already on disk” is every regular file at that root whose case-insensitive extension is `.zip`, `.7z`, `.rar`, or `.exe`; Task 1 records every basename, size, and SHA-256 rather than copying hundreds of mutable names into this planning snapshot.

This count makes a mostly local acquisition pass feasible, but it proves neither guide coverage nor correctness. The cache contains archive/extracted twins, variants, compatibility patches, and unrelated files. The K2 manual record gives a concrete identity trap: two differently named 7z files were byte-identical and represented the required mod, while a similarly named K1 derivative was a different file (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:39-58`). Archive resolution must therefore bind each selected resource to content hash plus inspected payload identity, not filename alone.

Historical manual work also warns that extracted staging directories can be incomplete because earlier `Move` instructions consumed files; fresh extraction from original archives is mandatory (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_2026-07-30.md:47-50`, `/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_2026-07-30.md:64-68`). Existing extracted directories may help triage but are not accepted inputs.

Acquisition is incomplete until the selected-resource ledger has exactly one content hash for every required archive, records intentional platform/alternative skips, and contains no missing, ambiguous, zero-byte, or corrupt payload. A historical K1 run was blocked globally by a zero-byte `.rar` left by a failed download, demonstrating why downloader success or filename presence is insufficient (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_2026-07-30.md:231-235`).

## Existing manual evidence

The existing K1 manual build is valuable but cannot silently become the current F29 expected tree. Its status header says the ledger contains 201 guide steps, the final override has 5,808 files, Windows-only widescreen and note-only steps were intentionally skipped, and the durable manual oracle is `/run/media/brunner56/MyBook/modbuild_oracles/K1_manual_oracle` (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_2026-07-30.md:3-26`). It also says the ledger, not the older narrative, is the source of truth for order and choices (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_2026-07-30.md:28-36`).

The existing K2 manual build is similarly substantial: it records 161 unique steps, 150 passed/11 intentionally skipped/0 blocked, a final 6,522-file override, and per-step rollback snapshots (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-37`, `/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:74-95`). It used the Aspyr `steamassets` layout (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-14`), which must be part of the run specification rather than normalized away.

Both manual builds predate the exact future acceptance bundle and contain explicit platform decisions, substitutions, skipped alternatives, and later overwrites. They are evidence sources and comparison candidates, not automatic zero-diff golden trees. Before use, the harness must map every manual ledger step to the pinned guide component/GUID, archive SHA, selection, option, backend, baseline, and platform rule. Any unmapped or differently selected step makes the three-way full-equivalence claim unresolved. The cheaper safe default is to preserve the old trees read-only and replay only disputed/manual steps into a fresh disposable clone; never “repair” an old oracle in place.

## Existing comparison scripts

The canonical C# checkout already contains useful pieces that should be extended rather than replaced. `scripts/agents/diff_manual_vs_auto.sh` selects the K1/K2 manual tree, automated tree, ledger, and log, then runs `scripts/diff_kotor_installs.py` with full hashing and JSON/Markdown output (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/diff_manual_vs_auto.sh:15-28`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/diff_manual_vs_auto.sh:36-54`). It does not pin a baseline, guide/archive/selection hashes, C# commit, or Rust tree, so it is a comparator entry point rather than a complete F29 harness.

`scripts/agents/incremental_install_diff_loop.sh` has the right operational instincts: it declares finished manual trees read-only, requires fresh vanilla automated trees, installs bounded chunks, runs a scoped divergence gate, and stops rather than continuing after a failure or real divergence (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_install_diff_loop.sh:1-14`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_install_diff_loop.sh:124-175`). Its present lanes are not full-guide evidence: K1 removes CineMalak and K2 removes HD Cockpit Skyboxes because of known preparation gaps (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_install_diff_loop.sh:39-75`). It also drives the C# DLL only, so F29 must add an equivalent Rust lane rather than infer Rust parity from these outputs.

The existing incremental gate forms scope from checkpoint writes plus matching manual-ledger paths and invokes the shared tree-diff tool (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_diff_gate.py:1-16`). It treats hazards and missing/extra in-scope paths as failures, and diagnoses content with TLK common-range, 2DA cell, container-resource, or raw-byte rules (`/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_diff_gate.py:18-40`, `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_diff_gate.py:302-369`). F29 should reuse those diagnostics, but add an unconditional raw SHA-256 patch-byte receipt: semantic agreement explains a byte mismatch; it does not silently satisfy the requested patch-byte comparison.

## Required comparison model

The comparator consumes an immutable `run-spec.json` and produces external receipts. The run specification contains:

- Rust commit/content hash and C# commit `d31ccbd3bfcddb4705917eafa6483715c166de3b`;
- exact guide Markdown/TOML bytes and corpus commit/dirty description;
- game identity, platform/layout, pristine-baseline manifest hash, and clone method;
- archive basename, size, SHA-256, selected resource mapping, and extraction tool/version;
- ordered component GUIDs, selected options/FOMOD choices, tiers/categories, platform skips, and widescreen phase;
- patcher backend executable SHA-256, arguments/namespace, locale, environment, and external-program allowlist;
- frontend/mode flags, with concurrency, best-effort, continuation, and live downloading disabled.

The external tree manifest has one record per directory, regular file, and symlink rejection: raw relative-path bytes or an explicitly reversible encoding, entry kind, size, mode/executable bit where relevant, and SHA-256 for files. It excludes only harness-owned output outside the game root. Product recovery/checkpoint/log files inside or adjacent to the game tree are recorded separately and may be excluded from payload equality only by a named normalization rule reviewed before execution.

For each lane, compute `baseline -> step N` and `step N-1 -> step N` manifests at every top-level component boundary. Then compare C# to Rust and both to the reconciled manual step. A mismatch report must name the first divergent component, path, change kind, expected/actual size and hashes, and whether later steps overwrite the difference. Final equality cannot hide an earlier wrong mutation that was later overwritten.

The **patch-byte set** is the union of every file added, removed, or changed during a component containing `Patcher`, `Execute`, or `Run`, plus any path reported by patcher provenance/logs. For each such path, retain baseline, C#, Rust, and manual SHA-256 and—when unequal—a binary diff receipt. Raw byte equality between C# and Rust is the default gate. If different patcher engines can legitimately serialize semantically equivalent resource bytes differently, that is not silently accepted: rerun with one identical backend first; any remaining proposed semantic comparator is a separately reviewed exception and F29 remains open until the acceptance owner decides whether byte equivalence is required for that format.

## Frontend and game evidence

The canonical full-build comparison is a shared-library/CLI operation lane, because it can pin a machine-readable run specification and capture ordered output. The GUI lane must not reimplement selection, ordering, acquisition, or comparison. It imports the same run specification, proves its prepared component/option/archive/backend projection is byte-identical to the CLI projection, performs at least one full K1 and one full K2 native disposable install, and records native route/progress/completion evidence. GUI/CLI must consume the same shared operation facts (`docs/parity/GUI-CLI.md:5-13`), while visual and interaction evidence remains a distinct gate (`docs/parity/GUI-INVENTORY.md:159-175`).

A process exit, completion page, screenshot, or receipt is not game proof. After zero-diff payload comparison, launch each disposable C# and Rust clone—not any real install—with a run-local HOME/Wine prefix/config/save directory, verify process ownership and target path, reach the main menu, load a fixed smoke save for that exact game/build where legally and technically available, traverse one patch-sensitive area/dialogue, exit cleanly, and retain logs/screenshots/video plus post-smoke manifests. K1 and K2 need separate scripts and acceptance owners because their layouts and platform skips differ (`/run/media/brunner56/MyBook/Workspaces/ModSync/MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-22`).

## Feasibility verdict

**Feasible, expensive, and not currently executable as an acceptance claim.** Local evidence includes 454 top-level archive/installer candidates, complete historical K1/K2 manual ledgers, final manual trees, per-step K2 snapshots, real guide files, and pinned C#/Rust sources. The blockers are proof preparation rather than an obvious absence of data: the guide checkout is dirty, archive-to-resource coverage is not hashed, historical manual choices are not yet reconciled to one run specification, pristine baseline identity needs durable receipts, and full parity must precede creation or execution of the harness under the user's rule.

F29 closes only when all four K1/K2 C#↔Rust full runs (CLI/shared lane and native GUI lane) finish without permissive continuation, every selected step is accounted for, final and per-step payload comparisons have no unexplained difference, patch bytes pass the selected policy, manual reconciliation has no unresolved mapping, live installs/baselines remain unchanged, and both games have disposable-clone smoke evidence. Until then the exact ledger status is **unverified**, not partial success inferred from archive counts or historical completed builds (`docs/parity/ACCEPTANCE.md:5-10`, `docs/parity/ACCEPTANCE.md:64`).

## Audit claim blocks

claim: The fake template is insufficient for F29 full-build execution.
referent: `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/create_template_kotor_install.sh:2-8,19-34`; `docs/local_desktop_agent_runbook.md:59-77`
checked: source read at pinned C# commit; no program run
verdict: holds
confidence: high; only evidence that the full guides and all patchers require no real game bytes would change it

claim: The local cache makes a predominantly offline acquisition pass plausible, but does not establish selected-guide coverage.
referent: read-only `ls -1pU ... | awk` probe recorded above; `docs/local_desktop_agent_runbook.md:204-210`; `MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:39-58`
checked: top-level directory inventory only; no payload hash or extraction pass
verdict: holds
confidence: high for the inventory and medium for acquisition feasibility; a full resource-to-hash ledger would raise it

claim: Existing manual K1/K2 outputs can be used unchanged as zero-diff golden trees for the future pinned run.
referent: `MANUAL_INSTALL_PROGRESS_2026-07-30.md:12-36`; `MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:12-37`; `MANUAL_INSTALL_PROGRESS_K2_2026-07-30.md:81-95`
checked: manual status headers and documented skips/choices
verdict: does not hold
confidence: high; a complete run-spec reconciliation with zero selection/input drift would change it

claim: The shipped C# full-build helper is itself an F29 acceptance harness.
referent: `scripts/agents/cli_full_build_pipeline.sh:181-201`; `scripts/agents/cli_full_build_pipeline.sh:242-256`
checked: source inspection
verdict: does not hold
confidence: high; it merges inputs and uses permissive/concurrent install flags

claim: A final tree hash alone proves ordered installation equivalence.
referent: `docs/parity/ACCEPTANCE.md:64`; `src/ModSync.Core/Services/InstallationService.cs:1226-1317`; `src/engine.rs:942-1053`
checked: ledger requirement and both ordered executors
verdict: does not hold
confidence: high; later overwrites can erase an earlier divergent mutation

claim: The existing incremental C# lanes already constitute complete K1/K2 F29 evidence.
referent: `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_install_diff_loop.sh:39-75`; `/run/media/brunner56/MyBook/Workspaces/ModSync/scripts/agents/incremental_diff_gate.py:1-40`
checked: script source inspection
verdict: does not hold
confidence: high; the current lanes exclude one guide section per game and contain no Rust execution lane
