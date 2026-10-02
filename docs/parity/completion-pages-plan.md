# Completion receipt and exact Base/Widescreen/Finished pages plan

Status: planned, not dispatched. The pinned C# oracle is `d31ccbd3bfcddb4705917eafa6483715c166de3b`, and the source-cited composition is in [completion-pages-csharp-audit.md](completion-pages-csharp-audit.md). This slice advances F24 and part of F10/F11 (completion evidence the user can read). It cannot close full C# GUI parity, checkpoint/rollback parity (F11), or F29 full-build equivalence.

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md` and all unrelated worktree state. Never edit `AGENTS.md`.
- The user has deferred creating, porting, expanding, modifying or running Rust tests of every kind until full parity is complete. Preserve existing test and inline `#[cfg(test)]` code byte-for-byte. Do not write a temporary Rust harness and call it a probe.
- Verify with production builds, the real CLI, the real native desktop, disposable fixtures, before/after manifests, source and binary hashes, and independent review.
- Install facts belong to the shared library. The GUI formats them. It must not count files, re-derive the completed components, or decide recovery state from editable paths or error text.
- The completion receipt format (`completion.json`) and `verify_completion` behavior must not change. Remaining installs depend on them. Additions are read-only.
- Keep strict guide order, fail-stop, pending-recovery guards and the verified Remaining prefix exactly as they are.
- Do not claim what Rust does not do. Rust makes one whole-run snapshot per install run (`recovery::begin`). `finish` removes `pending` after success (`src/recovery.rs:244-247`, called at `src/engine.rs:835`). `restore` refuses without `pending` (`src/recovery.rs:252-255`). The next `begin` deletes the old snapshot (`src/recovery.rs:221-223`). So after a successful phase **the user has no rollback path**. Copying the C# "you can always roll back" text, or showing a checkpoint counter, would state something false.
- Anchor every `src/ui.rs` edit by its match pattern or function name, never by line number. Lines drifted by about 3 at `dcd1373`, and the Installing-page worker is still editing. Line numbers below are orientation only.
- The Installing-page worker owns `src/ui.rs` and `src/ui/installing_page.rs` right now. This slice's UI task must not start until that worker's slice is approved and merged. Rebase on that result, and never write over it.

## Current Rust state (source)

- Route: `src/wizard.rs:5-23,195-214` already matches C# page order, dynamic widescreen insertion before Finished, optional notice, and titles/subtitles (`:26-66`, which equal the C# strings). Back is blocked on every completion page (`:68-80`).
- Gates: `src/ui.rs:408-415`. Installing is forward-ready only after `completed_base` is set and the phase did not fail. WidescreenInstalling needs `finished && !phase_failed`. Finished is `false`. Base and WidescreenComplete fall through to `true`.
- Footer: `src/ui.rs:541-590`. Finish appears only on Finished and calls `finish_wizard` (`:1646-1651`), which returns to setup and keeps the guide, directories and log. This matches C# `ExitWizardMode` closely enough.
- Renderer: one shared arm, `Page::BaseInstallComplete | Page::WidescreenComplete | Page::Finished =>` (about `src/ui.rs:703` at `dcd1373`), for all three pages. It shows a one-line count or phase sentence, `Keep the recovery data until you have checked your installation.`, three buttons (`Open game folder`, `Open mod workspace`, `Copy installation log`), and on Finished `Finish returns to the loaded project.` It has no header, statistics, cards, epilogue or per-page composition.
- Notice: the arm `Page::Preamble | Page::AspyrNotice | Page::WidescreenNotice =>` (about `src/ui.rs:651` at `dcd1373`) renders the raw metadata text as a bare label for all three pages. It has no `🖥️ Widescreen Mods` heading and no bordered scroll box with MinHeight100.
- Run state: `src/ui/installing_page.rs:27-56`. One `State` is shared by the Base and Remaining phases. `begin` (and `skip`, `:73-80`) replaces it, so **base elapsed time and counts are lost when the widescreen phase starts**. The Base-complete summary must be copied out at the base terminal.
- Receipt: `src/recovery.rs:147-178`. `completion.json` holds `{mods, game, components: [full ordered definitions], files: game-tree sha256 manifest}`. After success, `snapshot.json` still holds the before-run manifest of `mods/` and `game/` (`:217-236`). No typed reader exposes either file. `read_completion` is private and returns untyped `serde_json::Value` (`:155-161`).
- Rust defect R1: the completion arm (`Keep the recovery data…` line) tells the user to keep recovery data, but no action can use it after success (see Binding constraints). The fix is to replace that line with a true statement.
- Rust defect R2: `Copy installation log` in the completion arm copies the app-wide `self.logs`, which includes unrelated jobs from earlier. Rust keeps no log file, so C# `View Logs` has nothing to open.

## Shared facts and presentation

### Library (additive, no receipt or snapshot writes): `recovery::CompletionSummary`

Add to `src/recovery.rs` and export it from the existing module:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedComponent { pub guid: String, pub name: String }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFileDelta { pub added: usize, pub changed: usize, pub removed: usize }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionSummary {
    /// Every component in completion.json, in recorded order (base prefix + suffix).
    pub components: Vec<CompletedComponent>,
    /// Game files in the receipt manifest.
    pub game_files: usize,
    /// Receipt game manifest vs the retained snapshot.json game/ manifest of the
    /// run that wrote this receipt. None when the snapshot is absent or from other
    /// roots, when pending recovery exists, or when snapshot.completion equals the
    /// current receipt (the receipt predates that snapshot's run, e.g. after restore).
    pub game_delta_since_run_start: Option<GameFileDelta>,
}

pub fn completion_summary(roots: &Roots) -> Result<Option<CompletionSummary>>;
```

Rules:

- Check for `completion.json` **before** taking the lock, and return `Ok(None)` when it is missing. `Lock::acquire` creates the state directory and lock files (`src/recovery.rs:52,69-74`), and a query with no receipt must not create them. With a receipt present, take the recovery `Lock` and re-read.
- Error when the receipt roots differ from the requested roots.
- Return `game_delta_since_run_start: None` when `pending` exists, because then the receipt describes an earlier run.
- **Bind the delta to the run that wrote the receipt.** `restore` (`src/recovery.rs:249-293`) writes back the old `completion.json` and removes `pending`, but leaves `snapshot.json` in place. Example: base succeeds, a later run fails, the user restores. `snapshot.json` now belongs to the failed run while `completion.json` is the base receipt, so a naive diff credits the wrong run. No schema change is needed: `begin` already stores the receipt that existed at run start in `snapshot.json` as `completion: read_completion(roots)?` (`src/recovery.rs:227-232`). If `snapshot.completion` equals the current receipt, the receipt predates that snapshot's run: return `None` (shown as `N/A`). Only when they differ is the snapshot the run that wrote this receipt.
- Build the snapshot side from `snapshot.json` keys by `Path::strip_prefix("game")`, as `restore` does at `src/recovery.rs:278`. Ignore `mods/` keys. Never compare by string prefix.
- The only filesystem write allowed is the lock file. No receipt, snapshot or state writes.
- Hash nothing. The function compares two manifests already on disk. It does not prove current bytes still match; that stays the job of `verify_completion`.
- The components list comes only from the receipt. The GUI derives "this phase installed N" as `summary.components.len() - completed_prefix.len()`, where the prefix is the one the engine was actually given. The page must not count its own list.

**CLI surface (Task 2b, mandatory).** Expose the same summary with no new business logic, for example `modsync completion-summary --json` (spelling goes through Clap review). The GUI calls `completion_summary` only after the engine returned `Ok`, and by then `pending` has already been removed (`src/engine.rs:834-835`). So the pending→`None` rule, the restore→`None` rule and the roots-mismatch error can only be reached through a standalone query. Without the CLI they could not be verified.

### Worker terminal (GUI adapter)

After the engine returns `Ok` and **before** sending the terminal message, the install worker calls `recovery::completion_summary(&captured_roots)` and sends the result with it. The captured roots are the same ones used to read `pending` in the existing plan. A read error becomes a `None` summary with its message shown as "summary unavailable". It never turns a committed success into a failure.

### Presentation state (`src/ui/completion_pages.rs`, new)

`PhaseReceipt { phase: InstallPhase, installed_in_phase: usize, elapsed: Duration, summary: Option<CompletionSummary>, skipped: bool }`, copied at each accepted terminal. `base: Option<PhaseReceipt>` and `remaining: Option<PhaseReceipt>` are held on `App`. `reset_installation` clears both. Only a result accepted for the current run and input stamp fills them.

Presentation only: time formatting (`{m}:{ss}` at one minute or more, otherwise `{s}s`, matching C# `.axaml.cs:91-93`), card layout, copy, button feedback toasts, and page scroll.

## Page contracts

Titles and subtitles already match `src/wizard.rs`. Keep them.

**Base Installation Complete.** Follow C# composition and measurements from the audit (margin24×16; header card padding32×24, radius16; emoji64 / heading32 Bold / body16 at .8; four stat cards `*,16,*,16,*,16,*`, padding24×20, radius12, label12 at .6, value36 Bold; two info cards `*,16,*`; Quick Actions spanning full width, 2×2, button padding16×12; footer padding16×12, radius8). Values:

| C# card | Rust value (truthful) |
|---|---|
| MODS INSTALLED | `installed_in_phase` from the receipt (fixes D1) |
| CHECKPOINTS | relabel `RECOVERY SNAPSHOT`, value `Released`. Reason: Rust has one whole-run snapshot and restore is no longer offered after success, so a count would be false. |
| TIME ELAPSED | the base run's monotonic elapsed time, in C# format |
| FILES MODIFIED | relabel `GAME FILES CHANGED`: `added+changed+removed` from `game_delta_since_run_start`, otherwise `N/A` (same fallback string as C#). Reason: C# counts nothing, and mod-workspace changes are excluded, so the label names the tree it counts. |

"What's Next" items 1–2 use the C# copy exactly and are always shown, as in C#. Item 3 is replaced with truthful recovery copy (see Q2). Reason: Rust cannot roll back after success. Important Notes keeps all four C# bullets as plain text, including `• Join our community for support and discussion` with no link (`BaseInstallCompletePage.axaml:167`). Quick Actions: `Open Game Folder`, `Open Mod Folder` (C# labels, Rust behavior with a visible failure message, fixing D5), `Copy Log` in place of View Logs (Q3; reason: Rust has no log file), and `Recovery` in place of Manage Checkpoints (Q2; reason: no checkpoint history exists). Footer: keep the C# sentence up to `(if applicable)` and drop `, or 'Finish' to exit`, giving `Click 'Next' to continue with widescreen-specific mods (if applicable)`. Reason: Finish is not shown on this page (D3).

**Widescreen Support (notice).** Split `Page::WidescreenNotice` out of the shared `Page::Preamble | Page::AspyrNotice | Page::WidescreenNotice` arm into its own arm. Do not delete or change the Preamble/Aspyr arm. Heading `🖥️ Widescreen Mods` above a bordered box (MinHeight100, padding12, radius8) with an inner vertical scroll. The raw text stays raw, matching C# (D13 recorded; rendering Markdown is a separate decision, Q6).

**Widescreen Installation Complete.** C# centered stack (margin16×12, spacing12, theme default text sizes). Heading `✅ Widescreen Installation Complete!`. Body keeps the C# sentence `All widescreen mods have been successfully installed.` when the Remaining engine run committed. For a skipped phase (`installing_page.rs` `skip`, 0 selected), show `No widescreen mods were selected, so nothing was installed.` Reason: the C# sentence is false when nothing ran (D7). No count is added. Cancellation and failure can never reach this page: the gate already requires `finished && !phase_failed` (keeps the fix for D6).

**Installation Complete (Finished).** C# stack (margin16×12, spacing14; Next Steps card MaxWidth600, padding16×14; Support card MaxWidth500, padding14×12). The body keeps the C# sentence `All mods have been successfully installed.` except when widescreen was skipped, where it reads `Base mods were installed. No widescreen mods were selected.` Reason: the C# sentence is false after a skip (D7). Next Steps bullet 1 is the C# text. Bullet 2 uses the Q2 recovery copy (reason: no rollback after success). Bullet 3 becomes `Copy the activity log for installation details.` (reason: no log file). The Support card uses the C# text exactly, with no link. **Added content, not a correction:** show the guide `epilogueContent` in a bordered, scrollable box when it is nonblank (Q4). C# never shows it (D11). Finish behavior is unchanged.

The C# managed-deployment summary stays hidden. Rust has no managed install session in the engine install path (`src/deployment.rs` is used only by the CLI at `src/main.rs:452`), and C# hides it by default (`ManagedDeploymentEnabled` false). Record this difference; do not fake the summary.

Small window: follow the C# layout at 1280×800. At 600×400, whole-page vertical scroll must keep every value readable. If the four stat cards cannot fit, wrap them to 2×2 as a recorded deliberate difference (C# D12 is expected to clip; confirm natively before deciding).

## File ownership and task sequence (serialized)

| Task | Owned files | Responsibility |
|---|---|---|
| Task 1 | `src/recovery.rs` | Add the read-only `CompletionSummary`/`completion_summary`. Leave receipt schema, `record_completion`, `verify_completion`, `begin`, `finish` and `restore` byte-identical in behavior. |
| Task 2 (after the Installing-page slice lands) | `src/ui.rs` (worker terminal payload, `PhaseReceipt` capture, routing; replace the arm `Page::BaseInstallComplete \| Page::WidescreenComplete \| Page::Finished =>`; split `Page::WidescreenNotice` into its own arm and leave Preamble/Aspyr unchanged; anchor by pattern only), `src/ui/completion_pages.rs` (new) | Run-bound receipts, the four exact page renderers, action feedback. `src/ui/installing_page.rs` changes only when a terminal-snapshot accessor (elapsed/total) is needed, and that expansion must be reported first. |
| Task 2b (mandatory; may run in parallel with Task 2 after Task 1) | `src/main.rs` | Standalone CLI query that serializes the same `CompletionSummary`. It is the only route to the pending, restore and roots-mismatch rules. |
| Task 3 | `docs/parity/completion-pages-native/` (new), `docs/parity/GUI-INVENTORY.md` (row P1 base/final pages), `docs/parity/ACCEPTANCE.md` | Native/differential evidence and an honest status record. No source rewrites. |

`src/wizard.rs` needs no change. Titles, subtitles, back gates and insertion already match. Workers do not edit outside their ownership without first reporting the exact file they need to expand into.

## Probes without Rust tests

Use a temporary fixture root outside the repository and outside user game trees. Record fixture, game, mod and recovery-state manifests before and after each run.

1. Base-only guide (no widescreen): two Copy components, one overwriting an existing game file and one adding a file. Expect Base shows 2 installed, `GAME FILES CHANGED 2` (1 added + 1 changed, matching an external manifest diff), the time format, and `Next` leads to Finished. Diff `completion.json` against an external `jq` extraction of the component list.
2. Base plus widescreen with one wide component selected. Base shows only the base count. WidescreenComplete shows the C# sentence `All widescreen mods have been successfully installed.` with no count; the Task 2b CLI reports one component installed in the Remaining phase (`components.len()` minus the base prefix). The base elapsed time does not change after the Remaining phase starts (compare screenshots). Finished body sentence as specified.
3. Widescreen skipped (zero selected). No engine run and no receipt change (hash `completion.json` before and after). The skip copy appears on WidescreenComplete and Finished.
4. Failure on component two of the Remaining phase (a base receipt already exists) while `pending` exists. (If instead the first-ever Base run fails, there is no earlier receipt and the CLI reports none.) Installing stays blocked, and no completion page can be reached. The Task 2b CLI returns the previous receipt with delta `null`, and `completion.json` hashes the same as before the run.
4b. Restore binding: base succeeds, then a Remaining run fails, then `restore`. The CLI returns the base receipt with delta `null`: the snapshot's stored completion equals the current receipt, so the run-start snapshot is not credited. Compare with an external `jq` check of `snapshot.json` `.completion` against `completion.json`.
5. Cancellation in the widescreen phase. The WidescreenComplete route is unreachable (C# D6 contrast).
6. Roots mismatch: `state_dir` hashes only the game path (`src/recovery.rs:23-33`). Complete a base install, then keep the game path and change the mods path. The Task 2b CLI must return the roots-mismatch error. In the GUI, reloading or changing paths clears the phase receipts.
7. No receipt: run the Task 2b CLI on a fresh game path. It returns none, and a before/after manifest of the game's parent directory shows no state directory or lock file was created.
8. Folder buttons with the directory removed after completion: a visible message appears and nothing crashes.
9. Copy Log: clipboard hash equals the hash of the displayed run-scoped log.
10. Epilogue: a Markdown guide with post-widescreen notes shows them on Finished, byte-equal to the extracted metadata. Metadata `""` shows no box.
11. Receipt compatibility: run a Remaining install on a receipt written before Task 1, then after it. `verify_completion` accepts both, and `completion.json` bytes come out identical for identical inputs.

Native matrix at 1280×800 and 600×400, in Light plus focused K1/K2 samples: Base (with and without widescreen following), Notice (long content scroll), WidescreenComplete (installed and skipped), Finished (with and without epilogue), plus each button's feedback. Take pinned C# reference captures of the same four pages, bound to the verified DLL hashes in `validation-reference/receipt.json`. They are expected to show `0/0/0s/N/A`, and that is a recorded C# defect, not a target.

Every receipt records the Rust HEAD and dirty-state hash (`AGENTS.md` preserved), product source hashes, the executable SHA-256, the C# commit and binary hashes, argv, exit codes and output hashes, fixture manifests, `completion.json`/`snapshot.json` hashes, geometry, theme and DPI, the interaction transcript, screenshot and clipboard hashes, and an explicit statement that no Rust tests or harnesses were created, changed or run.

## Review gates

- **Task 1 spec review:** no receipt or snapshot writes (the lock file is the only write), missing receipt returns `None` before the lock, errors on mismatched roots, delta `None` when pending and when `snapshot.completion` equals the receipt, `game` keys taken by `Path::strip_prefix`, receipt order preserved, no change to `verify_completion` or the receipt schema. **Quality review:** no rehash, no panic on a malformed file, serde field names.
- **Task 2b spec review:** serializes only the library result, no extra logic, and probes 4, 4b, 6 and 7 pass through it.
- **Task 2 spec review:** receipts bound to run and stamp, and only accepted results fill them. Base receipt survives the Remaining run. No count or delta computed in the UI. Every C# measurement from the audit, or a recorded difference. Copy equals the C# strings except the listed changes, each with its reason. Copy is truthful: no rollback promise, no checkpoint count, and no "all mods installed" after a skip. Footer copy matches the visible buttons. The Preamble/Aspyr arm is unchanged. **Quality review:** stale receipt after reset or reload, action failure feedback, small-window scroll, accessibility labels on stat values.
- **Task 3 final review:** whole diff, all probe receipts, doc claims, user constraints. One fresh reviewer reads the combined diff before delivery.

Frozen inline tests in `src/ui.rs` that exhaustively construct `Message` or `App` state may stop compiling once the terminal message carries a summary. Keep additions compatible with existing constructors and defaults. Do not edit, hide or run those tests. Record any test-only compile drift with its exact location, for when the user lifts the freeze.

## Open questions (default in bold; the default applies unless the user decides otherwise)

1. Should `FILES MODIFIED` be computed? **Yes: game-tree added+changed+removed from the retained receipts, relabelled, falling back to `N/A`.** The alternative is always `N/A`, which matches C# output.
2. What replaces `Manage Checkpoints` and the "roll back" copy? **A `Recovery` button that opens a short read-only explanation: "This run finished. Its snapshot was released, and the next install replaces it. Restore is offered only when an install stops partway." Plus `Open recovery folder`.** Offering restore after success needs a new F11 contract and is out of scope.
3. What replaces `View Logs`? **`Copy Log` of the run-scoped installing-page log (base plus remaining), with no log file.** Persisting a log file belongs to a separate settings/logging slice.
4. Should the epilogue show on Finished even though C# never shows it? This is **added content, not a C# correction**. Default: **yes, recorded as a deliberate addition (D11)**, because the guide author wrote those post-install notes for this moment.
5. Should the community and sponsor text get links? **No. Keep the C# plain text, including "Join our community for support and discussion" (`BaseInstallCompletePage.axaml:167`), with no link.**
6. Should the notice render Markdown? **No. Keep raw text for parity.** Rendering belongs to a Markdown-display slice.
7. At 600×400, 2×2 stat wrap or exact 4-column? **Exact 4-column first. Switch to 2×2 only if native capture shows clipped values, and record it.**
