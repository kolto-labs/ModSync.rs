# Install progress native and CLI evidence

Packaged 2026-09-27 for the install-progress slice. Reviewed source head is `ac9cf3ac45092bbeea3b650b180ab2df5da003af` (base `659027e`). This is bounded evidence for F10/F24/F26. It is not full C# parity, not F29, and not per-component checkpoint parity.

[receipt.json](receipt.json) lists every copied file with its original path, byte count and SHA256. Each hash was recomputed from the source file and again from the copy, and compared with the hash the original receipt claimed, where it had one. One claim did not match (see Gaps). Executables (about 95 MB each) and fixture trees are not copied; their hashes are in the receipt. Product source hashes per commit are in `receipt.json` → `sources`.

Copies keep their original bytes. `cli-ac9cf3a/help.stdout` has clap's trailing spaces, and the `*.log` files are ignored by `.gitignore`, so they were added with `git add -f`. Neither was normalized.

No Rust tests, inline tests or Rust harnesses were created, changed or run for any of this evidence. Only production builds, the real CLI and the real desktop were used. `AGENTS.md` was not touched.

## Packages

| Folder | Source | Binary SHA256 | What it shows |
|---|---|---|---|
| [cli-57f198f/](cli-57f198f/) | `57f198f` (Task 2a) | `16d2d2a2…5887` (re-hashed, matches) | Typed All/Base/Remaining streams, runtime failure, preflight failure with zero lines, `validate`/`conflicts` rejection, clap conflict with `--json` (exit 2), legacy `--json` byte-identical to the Task 1 baseline (639 bytes, `9ea418cf…`), dropped consumer on success and failure |
| [cli-ac9cf3a/](cli-ac9cf3a/) | clean `git archive ac9cf3a`, headless build | `b6097a79…9fa8` | Typed-mode legacy Event text on stderr (`cd8f0b0`); success/failure/preflight contract; clap conflict with `--json` and `--validation-report`; current `--help` text |
| [gui-r2-ac9cf3a/](gui-r2-ac9cf3a/) | `ac9cf3a` desktop build | `9429089d…dcce6` | 600×400 Stop Install and Continue; close request during install; Stop confirmation, cancel and exact restore; failure then restore of captured roots after the folders changed |
| [gui-r1-c28f4fc/](gui-r1-c28f4fc/) | `c28f4fc` desktop build, before review fixes F1–F7 | `35b3b1b7…a0d4` (receipt only) | Light success and widescreen skip; K1 failure, small-window failure and restore; K2 Stop, restore, retry and widescreen phase on the same page; close attempt at `f598129` |
| [scripts/](scripts/) | — | — | Fixture generator, launcher and input helpers used by both GUI rounds |

Pinned C# captures are separate: [../install-progress-reference/](../install-progress-reference/README.md), bound to `d31ccbd3`.

## CLI contract at ac9cf3a

Probe: [cli-ac9cf3a/probe.sh](cli-ac9cf3a/probe.sh), run on the host with disposable fixtures from `scripts/make_fixture.sh`.

| Case | Exit | Stdout | Stderr |
|---|---|---|---|
| `typed-ok` | 0 | 17 NDJSON lines; last is `completed`, `snapshotPending:false` | 7 legacy Event lines |
| `typed-fail` | 1 | 9 lines; no `completed`; last is `runningInstruction` for Beta step 1; no line for the failed step 2 | Event lines, then `Error: Install stopped. Restore before retrying; snapshot: …` |
| `typed-preflight` | 1 | 0 lines | `Error: Beta Patch / step 1 (Copy): Source matches no files` |
| `legacy-json` | 0 | 7 lines, keys `component`/`message`/`step` only | empty |
| `conflict-json`, `conflict-report` | 2 | empty | clap "cannot be used with" error |

After `typed-preflight`, the game and mod content manifests are unchanged and no snapshot exists. An empty `.modsync-*` state directory and two lock files were created next to the content roots. After `typed-fail`, a pending snapshot exists (`fail.recovery.ls`).

The `--help` text for the flag reads only "Emit newline-delimited typed installation progress". It does not yet state the contract above.

## GUI round 2 at ac9cf3a

Display: private Xvfb `:93`, 1280×800×24, 96 dpi, no window manager, scale 1:1. 600×400 via `xdotool windowsize`. Private HOME/XDG/dbus per fixture. Timestamped actions are in each `transcript.txt`. The original receipt is [gui-r2-ac9cf3a/receipt.md](gui-r2-ac9cf3a/receipt.md).

| Scenario | Captures | Observed |
|---|---|---|
| light-small | `10-small-active-stop-install`, `20-small-success-continue`, `21-success-large` | At 600×400, Stop Install shows while active and Continue after success |
| k2-close-request | `10-active`, `11-close-attempt`, `20-cancelled` | A real `WM_DELETE_WINDOW` opens the Stop confirmation at 67% (2 of 3). The install commits while the dialog is open. The result stays 100% complete. The file is named "cancelled", but it shows success |
| k2-cancel | `10-active`, `11-stop-confirm`, `20-cancelled`, `22-install-steps`, `30-restored` | Confirm at 10:27:29.303; elapsed freezes at 4 s inside Alpha step 2 (Run). 0 of 3 installed, snapshot active. Footer Restore: after-restore manifest equals before |
| k1-f1 | `20-g1-failed`, `32-folders-changed`, `40-restored-g1`, `41-folders-back-retry`, `g2-cli.out` | G1 fails at 33%. The folders are pointed at G2, which has its own pending snapshot. Footer Restore restores G1 exactly and leaves G2 pending. Retry stays disabled until the folders point back at G1 |

## GUI round 1 at c28f4fc

Kept for the paths round 2 did not repeat: widescreen phase reuse (`k2/51-wide-mid`, `k2/52-wide-success`, `light/23-wide-skipped`), K1 failure at 600×400 (`k1-fail/30-small-failed`), K2 retry after restore (`k2/41-retry-success`). This binary predates fixes F1–F7, so the footer layout, percent rounding and restore routing differ from `ac9cf3a`. In this round the K2 stop landed in the second Beta Patch, because the confirmation was clicked several seconds after the dialog opened.

## Gaps

- **Round-2 binary.** `9429089d…` matched `/tmp/modsync-2b-native/modsync` when Task 3 hashed it at about 11:12. By 11:14 another build had overwritten that path (`f233685f…`), so it cannot be re-checked now.
- **Round-1 binary.** `35b3b1b7…` comes from the original receipt only. The file was replaced before Task 3 started.
- **Hash mismatch.** The round-2 receipt claims `90e9d7d3…` for `r2/lib.sh`; the file on disk is `5af056fd…`. It was copied as found. The scenario claims rest on the transcripts and manifests, not on that hash.
- **Older script hash.** The round-1 receipt claims `caf0966c…` for `wm_delete.py`, which is an earlier version. The copy matches the round-2 claim.
- **Task 2a receipt error.** `typedStreams.failure.completedFacts` is 0, but `failure.stdout` holds one `componentCompleted` line. The raw stream is correct; the summary field is wrong.
- **Not run natively.** Nested Choose owner paths and directions; silent preflight with the desktop; empty Remaining; late cancellation beyond the close-request case.
- **Not covered.** DPI, keyboard, screen reader, and Windows/macOS were not tested. No full-build guides were used.
- **Open fixes.** Review findings #2, #9 and #10 were being fixed in the `src/ui.rs` working tree during packaging. None of this evidence covers those fixes.
