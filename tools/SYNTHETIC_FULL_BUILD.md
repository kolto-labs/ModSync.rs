# Full Markdown parser and synthetic engine proof

`prove_synthetic_full_build.py` runs the production CLI against the actual public
`kolto-labs/mod-builds` `content/k1/full.md` and `content/k2/full.md`. The three
manual `synthetic-full-cli-{linux,windows,macos}.yml` workflows build only the
production library and CLI, record both source commits, and retain evidence even
on failure. No Rust tests or custom Rust harnesses are built or run.

Every run retains plain and drafted conversion output, a recursive owner/action
coverage ledger, review diagnostics and omissions. The original drafted guide
must fail at the unreviewed-draft gate without changing either synthetic content
root. The probe never marks authored instructions reviewed and never downloads
mods, executes patchers or installs original guide resources.

The independent engine fixture projects each observed Copy/Move/Rename/Delete/
Extract action family onto fabricated UTF-8 text or ZIP payloads. Every projected
action is an independent synthetic component, ordered by original owner traversal
and instruction ordinal. Authored paths, byte contents, dependencies, conditions,
option selection and overwrite policy are replaced. This is explicitly **not**
execution of the full guide's actual instructions. All other action families and
owners without generated instructions remain visible in `coverage.json`.

The baseline contains only `SYNTHETIC_BASELINE.txt` and empty Override/Modules
folders. It is not a valid game installation. Validation must leave both content
roots unchanged. Installation must produce exactly the independently planned
SHA256 manifests, and the completion receipt must retain projected
component order. Logs, argv/exit status, converted guides and manifests are
artifacts; neither a passing job nor a synthetic resource file establishes game
compatibility, actual patch semantics, complete prose coverage or full parity.

Local invocation after a production build:

```sh
python3 tools/prove_synthetic_full_build.py --binary target/debug/modsync --guides ../mod-builds --game k1 --output /tmp/fresh-k1-proof
```

`--prepare-only` writes the synthetic roots, projection, coverage, expected final
manifests and pre-install manifests for a native GUI driver. It performs real
plain/draft conversion but no validation or install. `--verify-only` requires a
previously prepared and installed output, checks current file hashes and queries the
production completion receipt for ordered component identity. Receipt inspection
does not rehash current files; the separate manifest comparison provides that check. These modes retain
the same proof limits. Use a fresh output for every preparation or CLI run.

Linux production CLI measurements at the pinned fork revision
`21b2f0499d28ecd4ee2092701dee5792ba5da4b9`: K1 parsed 186 roots/187 owners,
68 review-required owners, 62 projected instructions and 10 omitted authored
actions; TSL parsed 145 roots/148 owners, 62 review-required owners, 48 projected
instructions and 8 omitted authored actions. Both original draft installations
were refused before content writes. Synthetic final hashes and receipt order checks
passed. Windows/macOS execution remains pending until their workflows run.

The separate `synthetic-gui-{linux,windows,macos}.yml` workflows use
`prove_synthetic_gui.py` to launch the production egui desktop. Native mouse
clicks decline first-run telemetry, pause sharing for edits, validate, review
installation and confirm creation of the recovery snapshot. Screenshot OCR
locates visible controls; no application messages, reflection or fabricated
success events are injected. Validation must change no content bytes. The driver
observes the installation terminal, then verifies final hashes and ordered
durable completion identity. Screenshots, native input coordinates, observed
labels, logs and failure evidence are retained. Linux uses Xvfb; Windows/macOS
require native input and screen capture permissions on the hosted runner.
Missing permissions or invisible controls fail the lane.

The Linux GUI dispatch defaults to GitHub-hosted Ubuntu. Its optional `session`
runner requires an explicitly registered ephemeral runner labelled
`modsync-session`, an existing production target cache, and wrapper-provided
`GUIVENV`/`ENV_GUI_RUNTIME` paths. It reuses installed dependencies while still
building the checked-out production source and executing native Xvfb clicks.

Local Linux native clickthrough measured both K1 (62 projected components) and
TSL (48 projected components): seven mouse clicks per guide, successful visible
validation and installation terminals, unchanged validation snapshots, exact
final content hashes and ordered durable completion receipts. This exposed and
verified a fix for the confirmation modal dismissing itself on its opening
footer click. Native Windows/macOS execution remains pending.

For a native desktop after installing the workflow Python packages:

```sh
python3 tools/prove_synthetic_gui.py --binary target/debug/modsync --guides ../mod-builds --game k1 --output /tmp/fresh-native-gui-proof
```
