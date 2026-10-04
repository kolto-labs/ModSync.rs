# FOMOD gate warnings after a hard failure: proof

The contract and triage are in [fomod-gate-warnings.md](../fomod-gate-warnings.md).
The base is `438bb73`.

`probe.py` follows the [archive-scoped probe](../fomod-archive-scoped-proof/README.md)
conventions (it reuses its scaffolding and fixtures):

- a `FOMOD_PARENT_NETNS` guard and a network namespace with no routes;
- private HOME, XDG, mods, game and scratch directories, and generic zip fixtures;
- production verbs only: `validate`, both plain and with `--validation-report`, and `install`;
- every validate leaves the guide, mods and game byte-identical, and every install
  leaves the guide byte-identical and the game empty;
- output goes to `target/cloud-proof/fomod-gate-warnings`, one JSON per run plus a
  receipt that binds the binary SHA-256, the git HEAD and porcelain, and the source hashes.

| Case | Checks |
| --- | --- |
| Rust-only no-route warning for an unselected configured dependency, then "requires selected component" (`--use-file-selection`) | Report exits 1; the FOMOD stage is failed with `hasWarnings`, summary `Failed`, messages exactly `[route warning, error]`; `errorCount` 1, `warningCount` 1. Plain validate prints the `WARNING:` event once and the unchanged `FOMOD configuration check failed: ...` error. Install exits 1, prints the event and `Installation blocked before any write`, and writes nothing. |
| C#'s missing-registered-archive warning, then the same order error | The same three surfaces. |
| C#'s "configured but has no matching archive-scoped instructions" warning, then the same order error | The same three surfaces. |
| Both warnings, then a restriction conflict with a selected component | Report: both warnings first, the conflict last, counts 1/2. Plain validate: both events once, the conflict on stderr. |
| Missing-archive warning on one component, then configured status without saved choices on a later one (an in-gate issue) | Report: the warning, then the issue. Plain validate: the event and the issue. |
| Controls | The order error without any warning keeps exactly `[error]`, no `hasWarnings`, no event. A warning-only gate still passes with `(1 warning(s))`. An issue-only gate keeps one message. |

## Results (cloud Linux, default-feature debug build)

- **This change:** 55/55 checks and 17 recorded production invocations.
- **Pre-fix:** the same probe against the `438bb73` binary
  (`FOMOD_GATE_WARNINGS_ALLOW_UNBOUND=1 MODSYNC_BINARY=<copy>`, output
  `target/cloud-proof/fomod-gate-warnings-pre438bb73`) gives 38/55, with 17 failures:
  for each of the three order cases `*_report_stage`, `*_report_counts`,
  `*_plain_events` and `*_install_events` (no warning anywhere), plus
  `two_then_restriction_report`, `two_then_restriction_counts`,
  `two_then_restriction_plain`, `missing_then_issue_report` and
  `missing_then_issue_plain`. The exit codes, the error texts (`*_plain_error`,
  `*_install_error`), the untouched trees and all controls pass on both builds.
- **Regressions,** each in its own namespace: S1 339/339, inherited 41/41,
  S2a 224/224, S2b 254/254, fomod-gate-scope 74/74 and fomod-archive-scoped 37/37.
  The gate-scope check `dep_configured_order_error` now compares only the stage's
  non-`WARNING:` messages, because its fixture (an unselected configured dependency with
  no route) is exactly case 1 here and its route warning is now kept. It passes on
  `438bb73` and on this change.
- **Gates,** all exit 0:
  - `cargo fmt --all -- --check`;
  - `cargo clippy --locked --lib --bin modsync -j4 -- -D warnings`, with and without
    `--no-default-features`;
  - the same clippy for `--target x86_64-pc-windows-gnu`.

  No Rust test or harness was created or run.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j2
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-gate-warnings-proof/probe.py
```

Set `MODSYNC_PROOF_KEEP=1` to keep the temporary fixture root.
