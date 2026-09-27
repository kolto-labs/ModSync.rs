# Wizard integration evidence — bounded package approved

Prior checked implementation: `ff66794b83484ed1a40786a81960456540bbac4e`.
Final-fix source: `0a4f9dd433c7360393a095f4bb7bdef470e1f896`.
Scoped final re-review approved that correction and the bounded functional package.
Package base: `6b8fa9c`. Reference: C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`
in `/home/brunner56/.codex/worktrees/5813/ModSync`.

This is acceptance of the bounded wizard integration package, not full C# GUI
parity. Task-scoped review approved Task 2 after its first FOMOD fix. The completed
whole-package review rejected external choices applied before base installation only
to a dispatch clone, leaving the resident guide inconsistent with the base receipt.
The final fix retains exact effective base definitions on successful completion,
with context checks rejecting stale completion after reset, guide or root changes.
It never rereads edited external choices to reconstruct historical definitions.
The final whole-package reviewer's scoped re-review of `ff66794..0a4f9dd`
returned **Original P1: ADDRESSED**, **Spec: PASS**, **Quality: PASS**, with no new
actionable breakage. Its full report is retained in `wizard-final-review.md`.

## Ledger rulings, preserved verbatim

Ruling: use the established feature checkout and existing push authorization — avoids disturbing dirty AGENTS.md — cost if wrong is relocating commits.

Ruling: extract task briefs directly from the plan — bundled task-brief executes a non-executable helper — cost if wrong is missing helper metadata.

Ruling: match the active embedded WizardHostControl instead of the older dialog — native launch and MainWindow.EnterWizardMode establish the actual route — cost if wrong is revising the shell and initial route.

Ruling: preserve failure blocking and refresh Next from current worker state — C# has unsafe or stale completion state here and the user requires working shared logic — cost if wrong is a documented behavior difference from C# defects.

The publication ruling records existing authorization for a normal push to
`origin/main`; no pull request or release is part of this package. User-owned
`AGENTS.md` remains untouched. The current execution request requires delegated,
hands-off implementation; final bookkeeping and delivery change no product source.

## Implementation and review history

- Task 1: typed route, conditional metadata predicates, delayed widescreen insertion,
  sequential navigation and fresh-guide reset; commits `6b8fa9c..7cf5389`, scoped
  spec/quality review approved.
- Reference evidence `bc71ceb`: native active-host captures and Copy-install hashes;
  successful C# installation left Continue disabled. `b757639` records the active
  host correction and shared-loader metadata gap.
- Task 2: interrupted implementation resumed, existing source preserved, native
  evidence reconstructed; `d4fdb45` wires real desktop pages to shared workers.
  Replaced substitute Welcome content with the reference hierarchy and documented
  truthful manual-download copy. Self-review fixed per-repaint FOMOD payload reads
  and stale validation completion after inputs change.
- Scoped review found post-base FOMOD configuration unavailable and choices updates
  mutating completed definitions. `ff66794` adds shared remaining-only choices,
  saved-session and prompt-status handling; editor and external-file paths pass
  focused tests. Recovery receipt verification remains unchanged. Scoped re-review
  approved this correction; final package review then found the external-before-base
  definition loss. The final fix adds retained effective worker definitions and tests
  unchanged external choices, editor changes, external suffix changes and stale
  completion. Both post-base configuration paths remain covered.

## Fresh final-tree verification at ff66794

Parent executed these checks against the final tree; bookkeeping independently
counted successful test-result records in the captured logs, rather than rerunning
the suites:

| Command | Result | Captured output |
|---|---|---|
| `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo test --all-targets --all-features` | 214 passed, 0 failed, 1 ignored | `/tmp/modsync-wizard-integration-tests.log` |
| `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo test --no-default-features` | 188 passed, 0 failed, 1 ignored | `/tmp/modsync-wizard-integration-headless.log` |

The prior focused fix evidence is committed in
`wizard-native/fomod-fix-checks.txt`: 26 desktop tests, 26 shared FOMOD tests,
26 headless shared FOMOD tests, one CLI FOMOD integration test, strict all-target
Clippy, formatting and diff checks. Those outputs prove their covered contracts;
the final reviewer finding is not negated by passing suites.

## Verification after the final P1 correction

The implementer ran the required checks again because install-completion context
and phase-transition behavior changed. Captured output is committed in
`wizard-final-fix-checks.txt`, covering UI source SHA256
`f5febe34e756bac0a341670f3de41c842c7a16a5b7d019273f8c2c6201b7d317`:

| Command | Result |
|---|---|
| `cargo test --all-targets --all-features` | 215 passed, 0 failed, 1 ignored |
| `cargo test --no-default-features` | 188 passed, 0 failed, 1 ignored |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed |

The focused desktop suite also passed all 27 tests. Its expanded FOMOD regression
covers post-base editor/file configuration and external choices supplied before
base installation then left unchanged, edited through the editor, or edited in the
external file. Every case compares the resident completed definition to the actual
base receipt, requires source consumption, and verifies remaining installation
without base-byte changes. A separate stale-completion regression changes the guide
or resets context while the worker runs and rejects promotion into the current
project. These checks do not update the historical native receipt or establish new
native FOMOD behavior. The scoped reviewer independently parsed these check totals
and verified the UI source hash; no passing suites were rerun for final bookkeeping.

## Native proof is bound to pre-fix d4fdb45

`wizard-native/receipt.json` and its 22 captures bind the earlier `d4fdb45` source,
exact fixture, running executable, copied activity and intermediate/final hashes.
It is not native evidence for `ff66794` or any later correction. The executable
was hashed through the live `/proc/<pid>/exe`; parent independently verified that
hash and the installed fixture files. The base Move consumed its source before
remaining validation/install; base bytes remained unchanged and the remaining Copy
produced the expected bytes. The route reached Finish, retained the loaded project,
then reentered Welcome. The private native process and Xvfb were closed.

A previous run had its executable path replaced by a no-default build before the
hash was recorded; the entire native route was repeated with live executable
hashing. The committed receipt uses the repeated run, not that ambiguous binding.

## Preserved acceptance boundaries

- Full exact C# GUI parity remains the objective. Route/state tests and a two-file
  native fixture are functional integration evidence, not full rendering,
  accessibility, gameplay or full-mod-build proof.
- Active host is `MainWindow` → `Controls/WizardHostControl`; landing load is
  separate, Welcome starts the wizard. Shell requests 1280×800 and minimum 600×400,
  with a 320px download sidebar/footer from Download Process. It has no step-list
  sidebar or modal wizard Cancel action. The minimum-size capture retains
  navigation but leaves very little content height.
- Engine owns installation, validation, downloads and selection rules. UI owns
  route presentation and dispatch. No fabricated progress, validation or completion
  metrics are permitted. Base validation covers all selected components; base
  install excludes widescreen. Remaining operations verify completed definitions
  and outputs and must not replay consumed base sources.
- Failed installs must stop and require verified recovery before retry where a
  pending snapshot exists. Accessible Stop, close confirmation, explicit install
  confirmation and worker-driven Continue are documented safety differences from
  active-host defects or missing controls.
- Exact validation cards/report wording, rich authored markup, theme/chrome
  equivalence, keyboard/focus/accessibility and supported-platform coverage remain
  open. Native failure, cancellation, restore/retry, network downloads and FOMOD
  interaction were not established by the recorded native run.
- Guide-wide automatic provider acquisition/Fetch Downloads remains incomplete;
  Welcome explicitly says so instead of promising unsupported background downloads.
- Metadata-free C# JSON starts with the default `MainConfig.PreambleContent`, while
  TOML metadata parsing clears absent content. Route tests cover supplied effective
  metadata, not this shared-loader/default-state difference.
- FOMOD repaint stamps use metadata, including Unix ctime/inode. Non-Unix same-size
  edits preserving timestamps remain an invalidation limitation. Exact receipt
  verification must not be weakened to conceal choices-state inconsistencies.
- CLI currently has no remaining-phase command. Its full-install choices path is
  an empty-completed-prefix wrapper around the same shared choices implementation;
  adding a CLI remaining workflow remains a separate full-parity obligation.
- Disposable fixtures only; no user's game is mutated. Preserve dirty user files,
  existing processes and shared state. Delivery uses only the authorized normal
  push; no force-push, pull request or release.

## Completion and continuation

Task 1 route model, Task 2 desktop integration and both FOMOD correction waves, and
Task 3 final whole-package/scoped re-review are complete for this bounded package.
Final review status: **approved for integration; full exact GUI parity open**.

All four ledger Ruling lines appear verbatim above. The next shared GUI/CLI phase
audit is preserved in the separate `docs/parity/cli-phases-plan.md`: shared phase
preparation, `--phase all|base|remaining`, retained desktop versus reconstructed CLI
completion context, unchanged receipt authority, executable CLI/differential tests,
and explicit non-goals. That next-plan file remains outside this package's commit.
The package scratch directory may be removed after verified delivery; no other
worktree cleanup or user-file changes are authorized by this completion.
