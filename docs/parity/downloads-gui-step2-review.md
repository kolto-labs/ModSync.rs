# Downloads GUI step 2 review

Baseline: `d438528` (merged step 1). This is a bounded review of settings,
FOMOD queue/readiness and navigation. Full parity remains incomplete.

## Round 1

Independent SPEC review found three issues, corrected in the candidate:

- Backward navigation before DownloadsExplain hid live status/Stop.
- A narrow, tall Validate page skipped inline Downloads controls.
- A byte-identical guide replacement could retain old editor ownership.

The corrections retain download history visibility, constrain the direct Validate
layout shortcut to wide viewports, and include guide generation/input/root identity
in the acquisition context. SPEC approved source `src/ui.rs` SHA-256
`6ec16daae8d2eeed381a5296f25e2f68fff1d504adad7479eb5df6680e9af8dc`.
That approval does not cover subsequent changes or establish runtime behavior.

Independent QUALITY review requested changes:

- P1: saved-choice parsing or submitted-choice application errors could consume an
  unanswered FOMOD prompt and remove its readiness block. Preserve failed/pending
  ownership until recovery or explicit dismissal.
- P2: reading/parsing settings synchronously on every repaint could block the UI
  and Stop. Cache presentation with asynchronous refresh while retaining preserving
  read-modify-write for explicit edits.

Review also recommended routing FOMOD errors through the existing redactor. No
automatic leak of a stored API key was established.

The first remote production build found use of `Context::content_rect`, which is
unavailable in the pinned egui 0.32 dependency. Compilation is a separate gate;
neither source review nor historical probe counts substitute for it.

## Round 2 and final source review

The failed queue item now remains pending behind a Retry/Dismiss error modal,
including failures after submitting choices. Settings presentation uses one
asynchronous receiver and a cached snapshot; explicit saves still perform fresh
preserving read-modify-write. New error paths use the existing redactor. The pinned
egui API correction uses `screen_rect()`.

QUALITY approved those fixes. SPEC re-review then found that the asynchronous
initial load allowed Fetch before its peer disclosure/snapshot was ready. The
final correction renders static disclosure first and blocks Fetch until an idle
settings worker and a successful snapshot for the current directory exist.

Final SPEC approval binds:

```text
29e3f4694f35fc051f8e4fcec3741b7a2b0221ed6f369ce414939a25d78cee08  src/ui.rs
2e8c88e60e40471fda69a3971a333840b9b43d1ac37f918159be9470dc8988c2  src/fomod_ui.rs
```

The other four reviewed source files are:

```text
e0824e2782b1ceca635d5f1c4c75ff2aec6036cebe3767f05a09bda80079f263  src/profiles.rs
13a1e98f92511c1abf2b9d83c6606715c107e396dac76fa3b44109ad76eab94f  src/ui/downloads_panel.rs
6506318221fbaa673706c92da5d58119f064c739d85af79a06ca4a32dadc890e  src/ui/validation_page.rs
fb8fd6ebc6dd3a62609d0fee178a5b1a11006b49112e099566e173fdc9dbecd7  src/wizard.rs
```

At this review checkpoint, production gates and native evidence were still pending.
Source review is not runtime proof. Rust tests/custom Rust harnesses remain frozen.

Strict Clippy subsequently identified a `never_loop` after both queue branches
became terminal. Replacing `while let` with `if let` and removing its two `break`s
preserves single-item advancement. The final `src/ui.rs` hash is
`bb27fe97352d248df8363f6978d0ab5268b437e8017ff7c32bf1463358c02147`.
Independent QUALITY re-review approved that equivalent cleanup at the final hash.

## Production verification

All five final production gates passed on Linux aarch64 with Rust 1.95.0:
formatting, lib/bin build and strict Clippy without default features, then lib/bin
build and strict Clippy with desktop features. The
[gate receipt](downloads-gui-step2/gates/build-receipt.json) hashes every source
file and the binary; those source hashes match the local candidate. No tests or
custom Rust harnesses were compiled or run. The
[freeze receipt](downloads-gui-step2/freeze.json) confirms 53 frozen files/regions.

Native acceptance is recorded separately below, after the retained probes and
their screenshots/actions were reviewed.

## Upload-cap preservation follow-up

The controller identified a possible implicit settings edit, and independent
QUALITY review confirmed it against pinned egui 0.32.3: `DragValue::range` clamps
an existing value and marks its response changed even without user input. Opening
the controls with a stored cap of 0 or 1 could therefore save 16 without an edit.
Adding `clamp_existing_to_range(false)` preserves the authored value while keeping
the range for explicit edits. QUALITY re-review approved final `src/ui.rs` hash
`e8e6c5a1895dabeeadafd429928d9c4a2077655c619c8a53d66f2d20aa241e87`.
The other five source hashes are unchanged. All five production gates passed
again for these final bytes. The native fixture now starts with a saved cap of 1
and checks byte-identical settings immediately after opening the controls.

## Final bounded acceptance

The final source passed all five production gates and **46/46 native assertions**
in five isolated scenarios. The [controller index](downloads-gui-step2/native-index.json)
binds each completed receipt to the same source, production binary and probe script;
all screenshot hashes were checked. The controller visually reviewed actual
600x400 modal controls and page-6 navigation, the missing-archive inspection error,
and malformed-settings disclosure with disabled Fetch. Runtime validation after
queue resolution supplies the positive control for readiness checks.

Probe calibration failures were resolved without treating them as product passes:
ambiguous prose/button OCR, focused labels, quoted TSV text, X server reset timing,
and a navigation assertion that previously matched old log text. The final driver
uses real page numbers, preserves screenshots, and records calibrated actions.
Only final, completed scenarios contribute to the 46 assertions.

This accepts the bounded step-2 slice. Remaining native cases and the wider step-3
matrix are listed in [the evidence README](downloads-gui-step2/README.md). No full
Rust parity, persistent-seeding, cross-platform or F29 claim is made.
