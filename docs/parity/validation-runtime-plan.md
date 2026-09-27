# Native validation comparison plan

## Final corrective review checkpoint

The bounded package review found one P2 archive diagnostic/card/first-issue issue. Shared source correction `099834c` is now independently approved; [separate archive-fix evidence and reviews](validation-archive-diagnostic-fix/README.md) preserve the original finding and its closure. Failed raw message values gain an idempotent `ERROR:` prefix; JSON schema, stage counters and missing-Copy warning behavior remain unchanged. Earlier47c3e67 native evidence retains its binding. Full exact C# parity and the listed unproven native cells remain open; historical pending statements below describe earlier checkpoints. No new Rust tests or test adaptations were made.

## Current execution handoff — 2026-09-27

Pinned C# native reference already exists at [validation-reference/README.md](validation-reference/README.md). Shared Rust backend is approved through `88b2dd9`; renderer source is through `47c3e67`. Do not infer compiler availability from historical slot-release notes. This documentation task performs no builds, tests, launches or interaction with worker-owned sessions.

GUI worker evidence has separate bindings: `/tmp/modsync-rust-validation-task2/` root receipt is `556b165`; `final/` is `5548736`; `fix3/` is `47c3e67`. The parent reports independent reviewer confirmation of the latest 600×400 actual validation run, Stop confirmation and cancelled state with Next blocked. Its copied binary hash begins `5a77c2c2`; the full binding must come from the final receipt. Earlier stale/cancel evidence must remain attributed to `5548736`, even though the responsive-only delta is source-reviewed. **Final bounded matrix and complete binary binding are now archived in [validation-native/README.md](validation-native/README.md); integrated acceptance awaits final review.**

Actual missing Copy is **0 errors / 1 warning / 5 passed**, independently distinct from hard Environment failures. Rust pending recovery and C# missing patcher are different diagnoses despite the same **1/0/0** stage shape. C# clipboard readback remains unresolved; a successful Rust clipboard capture alone cannot prove bilateral copied-report parity. See [integration rulings](validation-page-integration.md).

Read-only discovery on 2026-09-27 found the pinned C# checkout at
`/home/brunner56/.codex/worktrees/5813/ModSync`, commit
`d31ccbd3bfcddb4705917eafa6483715c166de3b`, clean. Rust was at
`915ec69a21d90fac0395932d54e1622f8c049a18` with user-owned AGENTS.md changes.
The Rust binary at that initial discovery checkpoint `/tmp/modsync-rs-target/debug/modsync` hashes to
`641e4cc8520179566d0494d51a6c5dddd34af824e4b115ab410aa26852f8af6e`.
It differs from the historical wizard-native receipt; rebuild and bind the
executed binary after the current source work before claiming current evidence.

## Historical discovery prerequisites (revalidate before reuse)

- Xvfb, xdotool, xwininfo, ImageMagick import, ffmpeg, xclip, cargo and .NET
  SDKs 9.0.120/10.0.111 are installed. Xephyr/wmctrl were not found.
- The main desktop is Wayland with DISPLAY=:0. An unrelated C# manual run
  (PID 1199730 at inspection) and active input automation use :94. Leave those
  processes, :0, :94, :100 and their fixtures untouched.
- Historical :913/:915 servers and `/tmp/modsync-gui-reference-build` are gone.
  At discovery no pinned C# bin directory existed and both GUI submodules needed initialization. The successful later pinned build is recorded below; do not repeat initialization or build without ownership.
- About 13 GiB RAM was available and the Rust filesystem had 167 GiB free at
  inspection. Recheck before compiling; use one compiler owner and limited jobs.
- `vendor/bin/HoloPatcher_linux` exists. The GUI helper accepts the build output
  directory as its argument. A missing patcher is an environment failure, not
  the intended missing-source failure fixture.
- Local desktop runbook, desktop skill, KB index/action parity and
  gui-validation-surfaces.md were read. The private context archive exists;
  this scoped runtime task needs no private conversation material.

## Reproduction route (requires fresh parent ownership; not a pending build order)

Run sequentially from the pinned C# checkout after recording status and HEAD:

```bash
git submodule update --init src/AvRichTextBox src/RtfDomParserAvalonia
dotnet build src/ModSync.GUI/ModSync.csproj -c Debug -f net9.0 \
  --artifacts-path /tmp/modsync-validation-reference-artifacts -m:2
./scripts/agents/ensure_linux_holopatcher.sh \
  /tmp/modsync-validation-reference-artifacts/bin/ModSync/debug_net9.0
```

Use a unique artifacts path if that path already belongs to another run. Preserve
full build output and hash DLLs/resources. After the C# build slot is released,
build Rust with `CARGO_TARGET_DIR=/tmp/modsync-validation-rust-target cargo build
--bin modsync -j 2` and record commit, dirty diff and source hashes.

Allocate a new Xvfb with `-displayfd` rather than assuming a free display number;
retain its PID and emitted display number. Use `-screen 0 1280x900x24 -nolisten
tcp`, software OpenGL, unset WAYLAND_DISPLAY, and WINIT_UNIX_BACKEND=x11. Run
apps sequentially, using independent private XDG config/data/cache/runtime
directories and explicit `--allow-multiple-instances`. Keep the same fixture
absolute path for both app runs, resetting only the owned disposable fixture.
Do not overwrite HOME or operate the user's clipboard on :0.

C# launch, substituting the owned absolute paths:

```bash
dotnet /tmp/modsync-validation-reference-artifacts/bin/ModSync/debug_net9.0/ModSync.dll \
  --instructionFile=/tmp/modsync-validation-fixture/guide.json \
  --kotorPath=/tmp/modsync-validation-fixture/game \
  --modDirectory=/tmp/modsync-validation-fixture/mods --allow-multiple-instances
```

Rust launch:

```bash
/tmp/modsync-validation-rust-target/debug/modsync gui \
  --input /tmp/modsync-validation-fixture/guide.json \
  --game-dir /tmp/modsync-validation-fixture/game \
  --source-dir /tmp/modsync-validation-fixture/mods \
  --settings-dir /tmp/modsync-validation-fixture/rust-settings \
  --allow-multiple-instances
```

These are launch shapes; prepend the isolated display/XDG environment. Bind
window IDs to the owned PIDs, use xdotool windowsize to set both client windows
to 1280x800 and verify geometry with xwininfo. Explicitly select Light in both
apps and record scale 1.0. Capture client windows, not differently sized root
desktops. Hash `/proc/<pid>/exe` (Rust), the C# DLL actually passed to dotnet,
fixture files, and screenshots. Do not rebuild while capturing.

## Matched fixtures and behavior acceptance

Start with the identical selected Copy-only JSON and sample.tga from
`validation-pipeline-oracle`. Create a zero-byte swkotor.exe marker and an empty
Override directory. This establishes only a synthetic detected game. Hash all
game/mod files before and after every validation; validation must not mutate
them. Do not install or fetch downloads in this validation-only run.

1. Drive normal wizard controls to ValidatePage. Capture idle controls, stage
   area, summary, navigation and log state. Record selection and page index.
2. Run clean validation. Capture completion state and copy the report through
   the native Copy report control, then read the private X clipboard using
   `xclip -selection clipboard -o`. Assert stage order, summary counters,
   result cards, complete log, enabled continuation and collapsed clean log.
3. Remove sample.tga only in the owned fixture. Rerun validation. The actual
   pinned native run established this is warning-only (0 errors/1 warning/5
   passed): C# skips unavailable source in dry run. For a true failure remove
   only the private build's HoloPatcher symlink: 1 error/0 warnings/0 passed.
   Next remains clickable but a critical-error modal blocks advancement. Restore
   the owned symlink through the helper before the clean rerun.
4. For a long list, use a deterministic multi-component Copy fixture with a
   missing source. Scroll away from the issue, collapse the log and click
   Go to first issue. Capture the before/after scroll positions, highlighted
   warning card and expanded log at the first warning. Missing Copy is already a deterministic warning fixture in both apps after Task 1b; use a separately grounded hard-error fixture for error-first behavior. Multiple missing Copy components alone do not establish mixed error/warning coverage.
5. Restore source bytes and rerun. Verify stale errors/cards/highlights are
   cleared, scroll positions reset, counters/report replaced and continuation
   re-enabled. Keyboard activation and focus traversal require explicit native
   observations, separately from mouse behavior.

Store per-app action transcripts, clipboard reports, geometry/theme evidence,
terminal logs and before/after tree manifests alongside screenshots and a new
receipt. Compare report structure and semantic content; normalize only owned
root paths and genuinely variable durations/timestamps with the exact rule
recorded. A screenshot cannot prove clipboard contents, disabled navigation,
nonmutation, stage execution, rerun cleanup or focus behavior.

## Existing evidence limits

Fresh bounded C# execution is now recorded in `validation-reference/README.md`
and `receipt.json`. Clipboard readback remains unresolved even after a private
DBus/X11 relaunch; no reconstructed report is substituted for native copy.

Build prerequisite execution: upstream submodule URLs returned Repository not
found. Initialization succeeded using command-scoped local URL overrides from
`/home/brunner56/modsync-manual/source/ModSync/src/{AvRichTextBox,RtfDomParserAvalonia}`.
Those repositories supplied exactly the recorded gitlink commits
`94f175a709121de77930ada0241527cfec1365a8` and
`54a5b791e7c4ab28058648b7fc58f55549b14b05`; their working trees and running app
were not changed. Pinned C# source status remained clean. The actual build uses
`/tmp/modsync-validation-reference-20260927-5813/artifacts` and writes
`/tmp/modsync-validation-reference-20260927-5813/build.log`.

wizard-reference used 1280x900 Xvfb with default 1280x800 main window; the older
Rust wizard-native root capture used 1280x800. Their fixtures also differ.
The C# reference receipt explicitly did not establish Rust equivalence; the Rust
receipt omitted native failure/cancellation/theme/accessibility proof. Reusing
either receipt cannot satisfy current matched ValidatePage acceptance. All
themes, other platforms, real full builds and gameplay remain separate work.
