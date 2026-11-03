# Native archive-aware selection

The installer desktop and rooted CLI selection use `Engine::select_available`:
inspect initial eligibility, construct the proposed selection with the existing
selection graph, then inspect local
Extract inputs with the engine's source matching and archive decoder, propagate
unavailability through required dependencies (including option parents), and
apply the same action while clearing every confirmed unavailable selection. Bulk selection is
additive and base-only, matching the installer desktop; it does not reset option
choices. Restriction conflicts retain the existing graph's rules and warnings.

Eligibility is a separate report, never guide metadata. Installer file-open and
protocol loading preserve authored all-false flags instead of turning them into
implicit saved selections before Select All. A successful inspection clears saved checked items and required dependents when
they are confirmed unavailable. Healthy option choices remain checked. These are
in-memory changes; inspection never rewrites the source guide. Failed or
cancelled scans publish no selection changes. Missing dependency GUIDs are also unavailable.
Optional branches are inspected independently; unchosen missing options do not
block a valid parent. Archive CRC/format, path, link and size rules are inherited
from the production archive reader. One inspection memoizes shared archive
inventories. Nested archive names produced by an earlier Extract are recognized;
the nested payload itself is validated when the execution engine opens its bytes.

The actual desktop controls use an owned cancellable worker, discard results if
inputs change, and join the worker on exit. Reasons appear beside disabled
checkboxes. Refresh archive availability repeats inspection after disk changes.
Deselecting does not require archives or configured roots. The Qt authoring app
has no installer Select All and is unchanged.

CLI inspection: `modsync selection --input guide.json --game-dir GAME
--source-dir MODS --json`. JSON includes `availability.unavailable` with GUIDs
and reasons. Omit `--source-dir` for graph-only selection. Complete-root ordinary
install/validate/conflicts use the same entry point. Download acquisition, direct
Markdown preparation, remaining-phase reconstruction, and rootless commands
retain their existing special handling rather than excluding downloads before
acquisition or treating generated inputs as missing. These are explicit scope
boundaries; no cross-route/full-install parity claim is made.

`check.py` runs the actual production CLI against generated temporary archives and
guides. Set `MODSYNC_BIN` to the built binary. It creates no Rust harness, touches
no real game, does not run installers, and checks that the guide and game remain
unchanged. Rust tests remain frozen under AGENTS.md. Native GUI interaction is a
separate acceptance requirement; source/build checks are not that proof.

## Verification

Integrated on top of `06db96b` (including its option-identity preservation).
Rust 1.95.0 passed `cargo fmt --all -- --check`, locked production `--lib --bin
modsync -j2` builds and strict Clippy (`-- -D warnings`), both with and without
`--no-default-features`. The production CLI fixture groups passed; actual
output is retained in `validation.log`. No Rust tests or Rust harnesses ran.
Native GUI interaction remains unverified: the execution environment has no
Xvfb/display server or GUI automation tool. The GUI worker and controls compiled
and passed Clippy, which does not prove runtime interaction.

No workflow, dependency pin/version, TypeScript source, or Odyssey cache changed.
The hosted Production parity gates workflow is `disabled_manually`; no hosted
run was dispatched. The source was reviewed after reconciliation with the parent task; native GUI
interaction acceptance remains pending.

## Confirmed unavailability versus incomplete inspection

Completed refreshes and `--use-file-selection` now enforce the same eligibility
rules as Select All. A saved checkmark cannot bypass an unavailable archive or
required dependency. Removing a bad chosen option does not remove healthy option
choices or a parent that remains viable without that option. Repairing archives
makes items selectable again; it does not silently reselect cleared items.

Directory inventory stamps are captured during the engine scan and checked before
and after availability inspection. Each opened archive is checked before/after
reading and again before publishing the report. Changed identity, size, timestamp,
or Unix change-time aborts inspection. Permission/open/read errors, decoder I/O
errors and cancellation also abort, rather than masquerading as definitive bad
archives. A stable invalid format, checksum, or truncated archive remains a
confirmed failure. On incomplete desktop scans, existing checkmarks are retained
and validation readiness is cleared; refresh retries with a fresh engine snapshot.

The CLI fixtures include saved unavailable dependency chains, mixed healthy/bad
saved options, permission failure and retry, plus a Linux archive-mutation check
synchronized to a real file-open event. This does not claim exhaustive filesystem
race coverage or native GUI interaction acceptance. Windows/macOS runtime checks
remain unperformed.

## Nested dependency and deselection follow-up

Explicit deselections now remove effective required dependents at every option
depth, transitively. The same final pass is used by individual, option, category,
and base deselection controls. Saved option flags under unchecked parents stay
intact as preferences, while the CLI selected-ID receipt excludes inactive
subtrees. This does not rewrite the source guide. Added production CLI fixtures
cover parent and option-subtree removal, restoring dormant option preferences,
and a missing nested archive with required dependents. Actual GUI category/base
button interaction, normalized reason categories, common-input receipts, and
cross-route comparisons remain open under issue #68. Native Windows/macOS
behavior remains unverified. The current Linux environment has no Xvfb/xdotool.

## Normalized reasons and common-input receipt (latest)

`selection --input GUIDE --source-dir MODS --game-dir GAME --json --receipt`
adds an explicit `receipt` object. Without `--receipt`, no full-inventory hashing
is performed. The desktop offers **Prepare selection receipt**, then **Copy
selection receipt**, using the same production engine export on its owned worker.
A new action or changed in-memory context invalidates the cached export. Receipts
are snapshots: refresh after changing disk files.

Schema version 1 uses these availability categories: `missing_archive`,
`invalid_archive` (including decoder/unsafe-member rejection), `invalid_instruction`,
`missing_dependency`, `unavailable_dependency`, `unavailable_ancestor`. The complete
excluded list additionally uses `not_selected` and `ancestor_unselected` for
usable inactive nodes; these describe selection state, not an inferred user intent.
Readable reasons remain available, including dependency chains. Compare stable
GUIDs/categories; absolute paths in human explanations can differ across hosts.

`guideDefinitionSha256` hashes the serialized loaded guide after recursively
clearing selection flags and sorting object keys. Guide/option order and metadata
remain significant. It is a semantic loaded-definition hash, not the original
file byte hash and not a claim about identical starting user choices or actions.
`modInventory` covers all regular files in the engine-scanned mod root, with
relative paths, byte sizes, and SHA-256; the engine's private peer-staging exclusion
still applies. `modInventorySha256` hashes that sorted inventory. Neither field
hashes the game tree. Detected game, case policy, and expanded-byte limit are
recorded separately. Export hashes before/after a fresh availability scan and
rejects observed changes, uncertain I/O, or now-unavailable effective selections.

The retained [Linux native proof](linux-native/proof.json) records actual clicks
on the production desktop: Select All, Prepare, Copy, and Deselect All. The copied
[GUI receipt](linux-native/gui-receipt.json) exactly matched the [CLI receipt](linux-native/cli.json)
for the same full guide and inventory. Selected IDs were `good` and `independent`;
`missing`, `broken`, `dependent`, and `transitive` were excluded with the expected
categories. Guide/mod/game input hashes remained unchanged, and deselection
removed the stale Copy control. Screenshots retain the selected and exported
states. This supersedes the earlier lack of display tooling: Xvfb dependencies
were privately extracted under `/tmp`, without hosted workflow runs.

Replay with a default-feature production binary and Xvfb, xdotool, xclip,
ImageMagick `import`, and tesseract available on PATH:

```sh
MODSYNC_BIN=/absolute/path/to/modsync MODSYNC_PROOF_OUT=/tmp/selection-native-proof \
  python3 docs/parity/selection-availability/native.py
```

The replay uses private home/settings/game/mod folders and actual desktop input;
no Rust test harness or synthetic GUI message injection. Display `:96` defaults
can be changed through `MODSYNC_DISPLAY`. Fixed click positions are calibrated to
the retained 1280x800 desktop on a 1280x900 private display. Production CLI fixtures
also verify individual file hashes, relocation-stable identities, changed file /
guide fingerprints, and choice-independent definition identity.

Still open: broader native group/option/wizard cases, inventory-change-during-export
runtime proof, native Windows/macOS, identical starting-choice/action evidence,
and deferred TypeScript comparison. Do not close #68 on this bounded result.

## Broader Linux controls and starting-input evidence

The native replay now exercises two scenarios. The default `base` scenario checks
Select All, Deselect All, group select/deselect in a group containing unavailable
items, and Select All with a search filter. A discovered group-toggle bug is
fixed: a fully selected eligible subset now exposes Deselect group, instead of
forever offering Select group because unavailable members are unchecked.

`MODSYNC_NATIVE_CASE=nested` covers a parent with a saved middle/leaf option chain,
a missing unchosen sibling option, a root requiring the leaf, and an independent
root. Actual checkboxes exercise parent deselection, subtree clearing/restoration,
a disabled missing option, and leaf deselection. Saved choices survive the
appropriate transitions; required dependents are removed when the effective
requirement is unchecked. Six resulting GUI receipts exactly match CLI receipts.
The base scenario compares four states. Each proof includes the exact authored
fixture guide, its SHA-256, original mod/game input hashes, GUI click history,
and the equivalent CLI flags. Thus starting selection flags are evidenced
separately from the choice-independent definition hash. No instruction prefilter
is generated or supplied to either route.

Retained proof directories are [linux-groups](linux-groups/proof.json) and
[linux-nested](linux-nested/proof.json). The original narrower proof remains as
historical evidence. Both scenarios use actual mouse/keyboard input and clipboard
export from the production binary; all source guide/mod/game inputs remain intact.

```sh
MODSYNC_NATIVE_CASE=nested MODSYNC_DISPLAY=:97 MODSYNC_BIN=/absolute/path/to/modsync \
  MODSYNC_PROOF_OUT=/tmp/selection-nested-proof \
  python3 docs/parity/selection-availability/native.py
```

The prior production fmt/build/Clippy requirements still apply. No Rust tests or
custom Rust harnesses were run. Remaining acceptance includes wizard navigation,
native Windows/macOS, receipt generation during concurrent disk changes, and the
deferred TypeScript comparison; issue #68 remains open.

## Concurrent receipt export: reproduced race and fix

At `5787e21`, a synchronized production CLI probe changed `a-state.txt` after it
had been hashed during the second inventory pass, while `zz-padding.bin` was open
for hashing. The command exited successfully and its receipt hash disagreed with
the current first file. Immediate per-file checks and equality between the two
recorded inventories missed this late mutation.

The export now retains all per-file stamps and rechecks them at the end of each
whole inventory pass. The retained `check.py` production CLI fixture uses Linux
inotify to observe the later file's second open, stops only its owned CLI process,
mutates the earlier file, and resumes it. The corrected command fails with
`Mod inventory changed while preparing receipt` and no partial JSON; guide/game
inputs remain unchanged. Another receipt-only inventory file permission failure
also produces no receipt, and a stable retry succeeds with the current file hash.
No Rust tests or custom Rust harnesses are used.

These are bounded Linux CLI results. A receipt remains a historical inspected
snapshot, not a filesystem lock or a promise that files cannot change after the
checks. Native GUI presentation of this export failure, wizard variants, native
Windows/macOS, and the deferred TypeScript comparison remain outstanding.
