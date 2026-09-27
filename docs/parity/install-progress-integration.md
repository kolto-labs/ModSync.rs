# Install progress integration checkpoint

Documentation checkpoint, 2026-09-27. Product source is reviewed through `ac9cf3a` on `codex/rust-port` (base `659027e7eef54bb489039e3062b36ddd232b5c7d`, pinned C# oracle `d31ccbd3bfcddb4705917eafa6483715c166de3b`). This slice advances F10, F24 and F26. It does **not** close full C# parity, F29 full-build equivalence, per-component checkpoint parity, automatic rollback, or accessibility/platform equivalence.

- Plan: [install-progress-plan.md](install-progress-plan.md)
- C# source audit: [install-progress-csharp-audit.md](install-progress-csharp-audit.md)
- Native C# reference captures: [install-progress-reference/](install-progress-reference/README.md)
- Native and CLI Rust evidence: [install-progress-native/](install-progress-native/README.md)

## Commits in the slice

| Task | Commit(s) | Scope |
|---|---|---|
| 1 shared contract | `065ffee` (supersedes `599df2c`) | `src/install_progress.rs`, `src/lib.rs`, `src/engine.rs`: `Progress`/`Phase`/`OwnerFrame`, `Engine::install_observed`, `Engine::install_remaining_observed` |
| 2a CLI adapter | `57f198f`, `cd8f0b0` | `src/main.rs`: `install --install-progress-json` NDJSON; typed mode sends legacy Event text to stderr |
| 2b desktop page | `dcd1373`, `733bffb`, `f598129`, `c28f4fc`, `ac9cf3a` | `src/ui.rs`, `src/ui/installing_page.rs` |
| 3 docs/evidence | this commit | docs only |

## Rulings

Copied verbatim from the SDD ledger `.superpowers/sdd/install-progress-plan/progress.md` (SHA256 `8955cbf242ae8b67ca5a020c8c1d3ae92c495e3e97f7bca4880290f9a8a54ad3` at copy time), in ledger order, so that scratch directory can be deleted. The source line number is given for each.

- (L11) Ruling: no Rust tests, inline tests, temporary Rust harnesses, or `cargo test` runs until full C# parity is complete; preserve existing test source.
- (L12) Ruling: `Event { component, step, message }` and legacy `--json` remain unchanged; install progress is additive and explicit.
- (L13) Ruling: shared engine emits operational facts; elapsed/ETA/rate/layout are presentation state.
- (L14) Ruling: current Rust has one whole-run recovery snapshot, so the page must not fabricate C# per-component checkpoint counts.
- (L15) Ruling: Base and verified Remaining use the same observed engine API and renderer.
- (L16) Ruling: cancellation follows Rust safe boundaries and pending-recovery semantics; do not promise “finish current mod.”
- (L17) Ruling: user-owned dirty `AGENTS.md` remains untouched and unstaged.
- (L18) Ruling: lock, verification, scan and non-mutating preflight are silent; they are never counted as install progress.
- (L19) Ruling: component totals count only ordered top-level guide components; nested Choose work carries a separate GUID-bound owner path and never increments the top-level count.
- (L20) Ruling: success exists only after `record_completion` and `recovery::finish`; a late cancellation flag cannot override the committed engine result.
- (L21) Ruling: terminal pending-recovery state comes from the worker's captured run roots, never current UI paths or error-string parsing.
- (L22) Ruling: one phase has one recovery snapshot; the UI never represents it as per-mod checkpoints.
- (L23) Ruling: C#'s zero-based pre-component “installed” callback and non-success 100%/Continue behavior are reference bugs and are explicitly rejected.
- (L24) Ruling: condition-skipped instructions retain their owner/path but do not visit nested Choose options; current-mod progress remains indeterminate.
- (L25) Ruling: dropped progress receivers do not alter engine execution; external-program output remains logged but cannot supply typed warning/error counts.
- (L26) Ruling: empty Remaining is rejected or route-skipped before dispatch and emits no fake engine progress or success.
- (L57) Approved ruling: emit nested owner/path instruction completions as they actually succeed; emit the owning Choose instruction completion only after all selected nested frames succeed. Do not publish premature Choose success. InstallingComponent marks top-level start. Snapshot/terminal use no active owner, exact suffix total; snapshot_pending true only after recovery::begin returns with pending verified, false only after recovery::finish succeeds. If begin fails after writing pending, terminal must query captured roots and report actual pending, not assume false.
- (L67) Ruling: observed installation is additive and no-op wrappers must preserve legacy cancellation boundaries. Remove every newly added cancellation poll, including extra snapshot-notification and post-run/finalization polls. Once the final existing execution checkpoint has passed, RecordingCompletion observers and late token changes cannot interrupt BASE's receipt/finish path. Original599df2c evidence remains separately bound; finalfix1 source/build/receipt will supersede cancellation-control-flow claims. No tests/harnesses.
- (L75) Ruling: split Task 2 into serialized 2a CLI adapter (`src/main.rs`) then 2b GUI adapter/renderer (`src/ui.rs`, new `src/ui/installing_page.rs`, and `src/wizard.rs` only if necessary). No shared projection file is added: shared facts are already approved in `src/install_progress.rs`, CLI serialization is adapter-only, and GUI timing/layout are presentation-only.
- (L81) Ruling: typed CLI mode routes legacy Event text to stderr so external-program output is never silently lost.
- (L82) Ruling: build via host-spawn (container lacks cc); separate CARGO_TARGET_DIR per concurrent builder.

## Open review findings at hand-off

From the whole-package review (`/tmp/compound-engineering-1000/ce-code-review/20260927-103346-384f6429/report.md`, range `659027e..ac9cf3a`):

| # | Status in this checkpoint |
|---|---|
| 1 — no COMPATIBILITY.md entry for the typed stream | Closed by this docs commit (COMPATIBILITY.md "Typed install progress and Installing page — 2026-09-27") |
| 5 — typed-stream success/failure contract undocumented | Documented here, in COMPATIBILITY.md and GUI-CLI.md. The `--install-progress-json` help text in `src/main.rs:343` still reads only "Emit newline-delimited typed installation progress."; the lead updates it separately |
| 2 — utility "Restore failed install" uses live paths | Open; source fix in progress in `src/ui.rs` by another worker, not bound by this checkpoint |
| 9 — stale Remaining success dead-ends | Open; same |
| 10 — Retry disabled after `Settings::new` failure | Open; same |
| 3 — frozen test helper waits for `Message::Done` | Recorded; waits for the user's test freeze to lift (see below) |

## Frozen-test drift (recorded, not fixed, not compiled)

The user's freeze forbids editing or running Rust tests. Source reading at `ac9cf3a` only; no test was compiled or run.

- `src/ui.rs:3221-3231` (`mod tests`, `fn finish_worker`) loops until `Message::Done`. Installs now end with `Message::InstallTerminal`, so every test that starts `Job::Install` and then calls `finish_worker` waits out its 15 s `recv_timeout` and panics. Source-read callers: `desktop_retained_and_cli_reconstructed_choices_verify_real_completion` (L3343/3345, L3412/3413), `desktop_invalid_retained_ids_and_failed_base_never_promote_or_mutate` (L3471/3473), `stale_install_completion_cannot_promote_effective_definitions` (L3724/3734). That is the review's count of three.
- A test that feeds `Done(Ok)` for `Job::Install` would now reach the `unreachable!` arm. `desktop_gates_reject_stale_validation_unknown_game_and_failed_worker` (L3645/3646) feeds only `Done(Err)`.
- Pre-existing, not caused by this slice: the test module matches `Message::ValidationReport` (L3910), which the `Message` enum did not have at `065ffee` either. The tests module therefore already failed to compile before this slice.
