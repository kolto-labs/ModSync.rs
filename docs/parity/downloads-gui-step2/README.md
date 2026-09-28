# Downloads GUI step 2: retained native evidence

The bounded step-2 slice is accepted: independent source/spec and quality review,
all five production gates, and **46/46 native assertions across five independent
scenarios**. This does not complete the full downloads acceptance plan or Rust parity.

Final `src/ui.rs` SHA-256:
`e8e6c5a1895dabeeadafd429928d9c4a2077655c619c8a53d66f2d20aa241e87`.
Production binary SHA-256:
`70fe6d1105d393d29c007b9bfe46d7ed4fbeab038764455f775f749845a6295c`.

[Controller acceptance and receipt index](native-index.json) binds all five receipts.
The controller verified every source, binary, probe-script and screenshot hash,
checked successful scenario completion, and visually reviewed the small dialog,
actual small-window page navigation, inspection error and malformed-settings state.
[Independent source review](../downloads-gui-step2-review.md),
[production gates](gates/build-receipt.json), and
[53 unchanged frozen files/regions](freeze.json) retain their separate boundaries.

## What ran

Linux aarch64, Rust 1.95.0, the real production desktop binary, private Xvfb,
xdotool pointer/keyboard automation and screenshot OCR. Every scenario had a fresh
user/network namespace with loopback only, private HOME/XDG/settings/game/mod
roots, and a recording loopback TLS origin. Public proxy/tracker lists and telemetry
were disabled. At most three independent scenarios ran concurrently, on distinct
displays and disposable roots. No Rust tests, custom Rust harnesses, fabricated GUI
messages, real hosts or real game installations were used.

| Scenario | Assertions | Evidence |
| --- | ---: | --- |
| Serial choices and settings | 27 | [receipt](native/serial/receipt.json) |
| Configured/warned suppression | 2 | [receipt](native/suppressed/receipt.json) |
| Malformed saved choices | 9 | [receipt](native/broken-choices/receipt.json) |
| Archive disappears before editor inspection | 3 | [receipt](native/vanished-archive/receipt.json) |
| Malformed settings and repair | 5 | [receipt](native/malformed/receipt.json) |

The serial scenario opens settings with an authored upload cap of 1 and verifies
byte-identical settings before any edit. Actual Off radio and sharing-checkbox
edits remove conflicting case variants while preserving the cap, unknown nested
settings and a synthetic secret. The saved settings survive restart.

It then downloads two exact fixture archives, observes one editor at a time,
attempts the real Validate control while the first dialog is unresolved, selects
Beta and applies choices at 600x400, and dismisses the second editor. Repainting
never relaunches it. After resolution, validation really executes and reports the
second archive's missing choices; Next remains blocked. Small-window Back/Next is
checked against the actual page numbers, not the word “Validation” in old logs.
Authored guide bytes and game bytes remain unchanged, with no install receipts.

Malformed saved choices retain a blocking error. Retry neither redownloads nor
changes authored choices. The existing saved-choice reader encounters the malformed
record for both archives, so two explicit dismissals are required. Validation then
runs and reports the malformed choices. A separate case removes a disposable
archive after shared inspection but before its queued editor opens: inspection
fails, Cancel returns no choice outcome, and validation subsequently runs. Finally,
malformed settings keep Fetch disabled with zero origin requests and no file changes;
an operator repair plus the real Refresh control restores acquisition.

## Reproduction

Build the production binary first, using the production-only gates. Each of the five
retained runs used the following shape with its scenario name and a unique display:

```bash
DOWNLOADS_GUI_PARENT_NETNS="$(readlink /proc/self/ns/net)" \
  MODSYNC_BINARY=/absolute/path/to/modsync \
  MODSYNC_PROOF_OUT=/absolute/path/to/evidence \
  MODSYNC_CASES=serial MODSYNC_DISPLAY=:98 \
  unshare -Urn python3 docs/parity/downloads-gui-step2/probe.py
```

`MODSYNC_CASES` accepts comma-separated names from the table's receipt directory
names; omission runs all five. Dependencies are Python 3, Xvfb, xdotool, ImageMagick,
Tesseract, OpenSSL, iproute2 and normal desktop runtime dependencies. Tesseract is
limited to one thread. The driver records native clicks and their calibration;
focused controls that OCR omits use positions established from retained screenshots.
It refuses an occupied display and verifies its private X server is alive.

Raw receipts contain `accepted: false` because the probe does not approve itself.
Their `completed: true`, assertions and source identities are measurements. The
controller's bounded acceptance is separately recorded in `native-index.json`.
Only completed final-source scenarios are retained here. Earlier calibration
attempts are not included in the 46 assertions.

## Still open

- Real stale-editor outcome after guide/root replacement, and completed-base choice
  preservation, remain source-reviewed rather than newly proved natively.
- Direct Install-button interaction while a FOMOD dialog is unresolved is
  source-reviewed; blocked validation, fresh-validation requirements and blocked
  wizard advancement have native evidence.
- The broader downloads step-3 matrix remains open: Peer races/gates, cancellation
  mid-race, window-manager close, completion-page status, GUI Save/record diffs and
  the remaining context/transport combinations. Historical step-1 46/46 and shared
  download 381/381 results were not rerun or relabeled as current-source results.
- Persistent sharing, other native platforms and F29 full-install/live-game parity
  remain outside this bounded acceptance.
