# Native archive-aware selection

The installer desktop and rooted CLI selection use `Engine::select_available`:
inspect initial eligibility, construct the proposed selection with the existing
selection graph, then inspect local
Extract inputs with the engine's source matching and archive decoder, propagate
unavailability through required dependencies (including option parents), and
apply the same action while refusing unavailable additions. Bulk selection is
additive and base-only, matching the installer desktop; it does not reset option
choices. Restriction conflicts retain the existing graph's rules and warnings.

Eligibility is a separate report, never guide metadata. Installer file-open and
protocol loading preserve authored all-false flags instead of turning them into
implicit saved selections before Select All. Saved checked items are
retained even if unavailable; they remain deselectable and ordinary validation
still refuses their missing inputs. Missing dependency GUIDs are also unavailable.
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
`--no-default-features`. The ten production CLI fixture groups passed; actual
output is retained in `validation.log`. No Rust tests or Rust harnesses ran.
Native GUI interaction remains unverified: the execution environment has no
Xvfb/display server or GUI automation tool. The GUI worker and controls compiled
and passed Clippy, which does not prove runtime interaction.

No workflow, dependency pin/version, TypeScript source, or Odyssey cache changed.
The hosted Production parity gates workflow is `disabled_manually`; no hosted
run was dispatched. The source was reviewed after reconciliation with the parent task; native GUI
interaction acceptance remains pending.
