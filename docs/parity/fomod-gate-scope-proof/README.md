# FOMOD residuals proof: gate scope, missing archives, folder sources, run choices

Contract and triage: [fomod-gate-scope.md](../fomod-gate-scope.md). Base `15b11cb`.

`probe.py` follows the S2b probe conventions: a `FOMOD_PARENT_NETNS` guard, a network
namespace with no routes (no transfers are needed), private HOME/XDG/mods/game/scratch
directories, generic zip fixtures, and production verbs only (`validate` plain and
`--validation-report`, `install`, `--help`). Every validate leaves the guide, mods and
game byte-identical; every install leaves the guide byte-identical. Output goes to
`target/cloud-proof/fomod-gate-scope`, one JSON per run plus a receipt binding the
binary SHA-256, git HEAD/porcelain and source hashes.

| Case | Checks |
| --- | --- |
| Hard-dependency scope (`--use-file-selection`) | An unselected dependency's unconfigured FOMOD archive fails the report's FOMOD stage, plain validate and install with "has no explicit FomodChoices configuration", not "requires selected component"; install writes nothing. Also transitively (consumer → dependency → deep dependency) and for a stale saved choice. Controls: a configured or non-FOMOD dependency, an unknown dependency GUID and a selected restriction conflict still fail; a selected dependency is gated and a valid chain installs `b`, `notes`. |
| Missing registered archive | Without state: report passes with exactly one `WARNING: Example Consumer: Registered archive 'gone.zip' is not on disk under the mod directory. Re-download the archive (Fetch Downloads), then configure the FOMOD installer before validate/install.`, the summary counts 1 warning, plain validate prints the event once and install proceeds. Two registrants give two warnings. Controls: recorded state fails closed; a missing non-archive name is not gated. |
| Folder named like an archive | `<<modDirectory>>/Pack.zip` (a directory holding `inner.zip`) fails without choices, installs `b` with a run entry for `inner.zip`, and passes report/plain validate; an entry for `Pack.zip` configures nothing. An unrelated `Other.zip/` folder holding another archive does not take the entry, so its unparseable `FomodChoices` do not fail the run; the same unparseable choices on `Pack.zip`'s component fail with the repair hint. |
| Per-archive preservation | Embedded `inner.zip`/`second.zip` = Alpha gives `a`,`c`; a run entry `INNER.ZIP` gives `b`,`c`; `second.zip` gives `a`,`d`; the guide is never written. |
| Help | `install --help` and `validate --help` say the file is merged per archive and other archives keep theirs. |

## Results (cloud Linux, default-feature debug build)

- This change: **74/74 checks, 29 recorded production invocations plus two `--help` runs**.
- Pre-fix: the same probe against the `15b11cb` binary
  (`FOMOD_GATE_SCOPE_ALLOW_UNBOUND=1 MODSYNC_BINARY=<copy>`, output
  `target/cloud-proof/fomod-gate-scope-pre15b11cb`) gives **62/74; 12 fail**:
  `dep_report_fomod_names_archive`, `dep_report_fomod_not_order_error`,
  `dep_plain_gate_names_archive`, `dep_install_gate_names_archive`,
  `dep_transitive_names_archive`, `dep_stale_names_archive` (each reported "requires
  selected component" instead), `missing_report_stage`, `missing_plain_event`,
  `missing_install_event`, `missing_two_messages` (old wording, one deduplicated
  warning), and both `help_*_per_archive`. The folder, ownership, preservation and
  control checks pass at both builds: those behaviors were already in source.
- Regressions, each in its own namespace: S1 `probe.py` **339/339**,
  `inherited_controller.py` **41/41**, S2a **224/224**, S2b **254/254**.
- Gates, all exit 0: `cargo fmt --all -- --check`; `cargo clippy --locked --lib --bin
  modsync -j4 -- -D warnings` with and without `--no-default-features`; the same for
  `--target x86_64-pc-windows-gnu`. No Rust test or harness was created or run.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j2
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-gate-scope-proof/probe.py
```

Set `MODSYNC_PROOF_KEEP=1` to keep the temporary fixture root.
