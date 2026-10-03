# Runtime revalidation of `main`, 2026-10-03 (second pass)

The [first pass](runtime-revalidation-2026-10-03.md) ran on `73e8819`. Since then
`main` gained ordinal path identity (engine VFS, managed capture, deployment,
conflicts, checkpoint tree aliases; `8276344`, `bc81a40`) and typed YAML scalar
keys (`bf7e793`). Both had been reviewed only as source. This pass reruns the same
production proofs on `main` `25296da`, adds the native GUI probes that the first
pass skipped, and repeats the public-pin C#/Rust differential.

No Rust tests or custom Rust harnesses were created, changed or run (AGENTS.md
freeze). Every network fixture ran in its own `unshare -Urn` (loopback only, no
routes), with the probe's guard variable set from `readlink /proc/self/ns/net`.
Receipts went to a scratch `MODSYNC_PROOF_OUT`; the retained receipts in this
repository were not overwritten.

## Binding

- Source: `25296da` plus the Windows-only lint fix below. The fix is `cfg(windows)`
  code, so the Linux binary is unchanged.
- Production binary `target/debug/modsync` (default features, rebuilt last):
  SHA-256 `730705dd6438963c37f9ee1ced95a4e9afa6c40bcebe17cba106a09a0fc5a4e0`.
- Rust 1.95.0, Linux x86_64 container, Xvfb, xdotool, ImageMagick and tesseract
  5.3.4 (installed for this pass).
- C# reference: `d31ccbd3`, freshly built with the apt .NET 10 SDK. `global.json`
  roll-forward was relaxed only in the scratch checkout, and the DLL ran with
  `DOTNET_ROLL_FORWARD=Major`. DLL SHA-256
  `6e7de00d37bf9e0dabc63a303f31ec024bd1af36e50a3865988fe3d48f136824`.
- Guides: mod-builds `336b36d9`. K1/K2 `full.md` hashes match
  [the cloud bootstrap](cloud/README.md).

## Gates

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo build --locked --lib --bin modsync -j3` (default and `--no-default-features`) | pass |
| `cargo clippy ... -- -D warnings` (Linux, both modes) | pass |
| `cargo clippy --target x86_64-pc-windows-gnu ... -- -D warnings` (both modes) | **3 errors on `25296da`**; pass after the fix |

The three Windows lints already existed at `73e8819`; they came in with `f68dcfc`
and `6f47e45`, and the first pass did not run windows-gnu Clippy. The fix changes
no behavior:

- `src/maintenance/platform/windows.rs:397`: `length % 2 == 0` became
  `length.is_multiple_of(2)`.
- `src/checkpoint/persistence.rs:128-130`: added a scoped
  `#[allow(clippy::permissions_set_readonly_false)]` with a comment. This code is
  `cfg(windows)` and clears `FILE_ATTRIBUTE_READONLY`, so there are no Unix mode
  bits for the lint to protect.
- `src/checkpoint/persistence.rs:276-280`: the nested `if` became a let-chain.

## Per-suite results

| Suite | As shipped on `25296da` | After fixes / updates |
| --- | --- | --- |
| `fomod-s1-proof/probe.py` | 339/339 | — |
| `fomod-s1-proof/inherited_controller.py` | 41/41 | — |
| `fomod-s2a-proof/probe.py` | 224/224 | — |
| `fomod-s2b-proof/probe.py` | 254/254 | — |
| `nexus-h1a-proof/probe.py` | 138/138 | — |
| `fetch-shared-url-proof/probe.py` | 382/382 | — |
| `deployment-csharp-differential.sh` (C# DLL + Rust) | 8/8, `TOTAL fail=0` | — |
| `downloads-gui/probe.py` (step 1) | 9/12, then stopped (click missed) | 46/46 |
| `downloads-gui-step2/probe.py`: serial, suppressed, vanished-archive, malformed | 8, then stopped (serial) | 37/37 |
| `downloads-gui-step2/probe.py`: broken-choices | stops before its first assertion | not updated (see below) |
| `downloads-gui-step3/probe.py` (install-gate) | 0/1, then stopped | 9/9 |
| `downloads-gui-step3/record_probe.py` | 4/4, then stopped | 14/14 (1 pending) |
| `downloads-gui-step3/window_close_probe.py` | 0, then stopped | 6/6 |
| `downloads-gui-step3/origin_probe.py`, `peer_probe.py` | not runnable (IPv6) | — |
| K1/K2 public-pin conversion differential | matches the first pass | — |

## The new ordinal and YAML code at runtime

- **Retained suites.** None of them failed. Deployment, conflicts, checkpoints,
  FOMOD and fetch all run through the new ordinal keys.
- **Supplementary deployment check.** This was a scratch Rust CLI run, not
  retained. The game held `Override/ä.txt` = ORIG and the component was staged as
  `OVERRIDE/Ä.TXT`:
  - Deploy reused the existing on-disk spelling and backed up ORIG.
  - A second component staging `Override/Ä.txt` got the conflict warning naming
    the first owner.
  - A component staging both `ä.txt` and `Ä.txt` was refused ("differ only by
    case").
  - Purge restored ORIG.
  - In the C# direction, C# deployed `Ä.TXT`, and Rust read that manifest and
    uninstalled it, leaving `ä.txt` = ORIG. C# on Linux created a separate
    `Ä.TXT` next to `ä.txt`, which is the known Linux C# case behavior.
- **Typed YAML keys, both tools on the same inputs.**
  - Both tools load `!!bool true`, `!!int 7` and `!!float 1.5` keys. Rust keeps
    them as text metadata keys `True`, `7` and `1.5`.
  - Both tools reject `!!int abc`, `!!null` keys and complex keys.
  - Pre-existing: C# `convert` drops `WidescreenOnly` in every format (TOML and
    YAML alike); Rust keeps it typed, as the first pass fixed.

## Failures and their causes

No product regressions were found. Every GUI failure traces to one of two causes:

- **Layout drift.** The always-visible completed-sharing strip and the FOMOD
  fallback warnings moved controls, and OCR then matched the word "Next" in prose.
- **Intentional, documented UI changes from `f68dcfc`.** That commit landed before
  `73e8819`; `src/ui*` has not changed since `73e8819`.

The retained probes were updated as follows:

1. **Step 1: Fetch button moved** from y=261 to y=530. Coordinates were
   recalibrated (`FETCH`/`STOP`). After that, 43/46 passed. The remaining three
   checks failed on new files in the settings tree. These files are documented
   state, and the probe now excludes them:
   - the dated `settings/Logs/kotormodsync_*.log` ([first pass](runtime-revalidation-2026-10-03.md));
   - the empty sibling `.modsync-lock-<hash>` ([acquisition root coordination](acquisition-root-coordination.md));
   - the empty `settings/peer-sharing-v1/{owner,transaction}.lock` ([persistent sharing plan](persistent-sharing-plan.md)).
2. **Step 2: calibration and alias reading.**
   - `CALIBRATED` gained or updated entries for Peer mode Off, Navigate to
     Validate, Beta, Apply and Cancel at 600x400. The Run Validation point moved
     from (96,280) to (96,395).
   - The fixture authors `peerSharing: false` and `PeerSharing: true`. Contradictory
     case aliases now read fail-closed ([persistent sharing plan](persistent-sharing-plan.md):
     "contradictory case aliases cannot restore sharing"; `src/p2p.rs`). The
     checkbox therefore starts unchecked. The probe now switches sharing On (and
     checks that the alias collapses) and then Off through the real control.
3. **Step 3 install-gate:** same navigation and Run Validation recalibration.
4. **Step 3 record:**
   - Editor controls now wait for completed-sharing cleanup
     ([persistent sharing desktop](persistent-sharing-desktop.md)), so the probe
     clicks the page's "Pause sharing to edit".
   - The editor's "Save guide as" became a typed "Save to" path plus
     "Save validated guide" ([production gates 2026-10-02](production-gates-2026-10-02.md),
     editor draft validation).
   - Typing into that field did not register on private Xvfb. The saved-guide
     checks therefore stay pending, exactly as in the retained receipt, whose
     native dialog never surfaced either. The 14 CLI and acquisition checks pass.
5. **Step 3 window-close:**
   - Closing now shows one "Stop work and close?" confirmation for downloads and
     sharing ([persistent sharing plan](persistent-sharing-plan.md): "Exit
     confirmation mentions both downloads and sharing"). Confirming cancels, joins
     cleanup and closes the window.
   - The probe now asserts that the dialog names downloads and sharing, that the
     process exits 0, and that there is no published or partial archive, no guide
     save and no game change.
   - It drops two checks that contradict the new contract: the window stays open,
     and the "no install rollback" wording.

**Open item, not updated: the step-2 broken-choices scenario.**

- When a component's saved `FomodChoices` cannot be parsed, Fetch now reports it
  as an inspection error. The text is "Saved FOMOD choices need repair" plus the
  repair hint (`src/download_orchestrator.rs` FOMOD follow-ups; COMPATIBILITY
  "Saved `FomodChoices` that cannot be parsed ... Fetch, validate and install
  instead name a repair hint").
- The desktop shows this only as an Activity line, "FOMOD inspection failed for …".
  The blocking "FOMOD choices need attention" modal and its Retry/Dismiss flow no
  longer appear for this case. Other archives' editors still open.
- The step-2 acceptance described a blocking modal. The probe was not rewritten,
  because whether the desktop should surface this as a blocking prompt is a
  product decision, not a stale expectation.
- This behavior existed before `73e8819`, so it is not a regression from the
  ordinal or YAML work.

## C#/Rust differential on the public guides

The [first pass's comparison scripts](runtime-revalidation-2026-10-03/) were run
unchanged. Both tools ran `convert -i content/k{1,2}/full.md -f json`, with and
without `--parse-directions`.

- **Shape:** K1 186 roots (187 drafted nodes), K2 145 (148), with identical
  Name/Author by position. The Rust-only fields are as before: `NameFieldContent`,
  `UnresolvedMasters` (two per guide), and `DraftReview` (68 for K1, 62 for K2).
- **Instructions:** the only difference is still C#'s omission of `Overwrite` on
  Patcher/Extract/CleanList (K1: 8/1/2, K2: 6/1/2).
- **Cross-format interchange:** each tool's drafted guide was exported to TOML,
  YAML and XML and reloaded by both tools, for both guides. All 24 loads succeed.
  Rust's own reloads differ from its source only in the two categories both tools
  share: `\` Source separators, and a comma splitting `Language` (K1 88, K2 92 in
  every format). YAML reloads therefore match TOML and XML after the typed-key
  change.

## Not runnable here

- **IPv6 is missing.** `downloads-gui-step3/origin_probe.py` and `peer_probe.py`
  seed through `torrent-seed`, and librqbit binds `[::]:0`. This kernel returns
  `Address family not supported` (os error 97), which is the same limit that
  blocks `origin-loopback-proof`.
- **Workstation paths.** `fetch-controller-{pre-fix,review}`,
  `legacy-source-oracle`, `source-normalization-{proof,format-oracle,scalar-oracle}`
  hard-code `/home/brunner56/...` binaries, outputs or a C# DLL. As
  [the cloud bootstrap](cloud/README.md) notes, they are not portable as shipped.
- **Not part of the first pass's set:** `fomod-s1-proof/gates.py` (a gate driver)
  and `fetch-downloads/run_*.py`.

## Limits

These runs prove the named CLI contracts on generic fixtures, the GUI flows
listed above on a private Linux X11 display, and the conversion equivalence above.
They do not prove the following:

- installs of real mods;
- live providers;
- Windows/macOS runtime (only Clippy ran for windows-gnu);
- the step-2 malformed-choices desktop prompt;
- the F29 full build.
