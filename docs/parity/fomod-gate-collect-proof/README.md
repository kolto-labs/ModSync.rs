# FOMOD gate collect-all: proof

The contract and triage are in [fomod-gate-collect.md](../fomod-gate-collect.md).
The base is `d78aea2`.

`probe.py` follows the [gate-warnings probe](../fomod-gate-warnings-proof/README.md)
conventions and reuses its scaffolding:

- a `FOMOD_PARENT_NETNS` guard and a network namespace with no routes;
- private HOME, XDG, mods, game and scratch directories, and generic zip fixtures;
- only production verbs: `validate`, plain and with `--validation-report`, and `install`;
- every validate leaves the guide, mods and game byte-identical; every install leaves
  the guide byte-identical and the game empty;
- output goes to `target/cloud-proof/fomod-gate-collect`: one JSON per run plus a receipt
  that binds the binary SHA-256, the git HEAD and porcelain, and the source hashes.

| Case | Checks |
| --- | --- |
| `mixed`: six components. Their findings, in guide order: missing archive (warning), configured without saved choices (issue), missing archive with prompt state (issue), R1 (warning), stale saved choices (issue), missing archive (warning). | **Report.** The stage fails with `hasWarnings` and summary `Failed`. The messages are the 3 warnings in gate order, then the 3 issues in gate order; `errorCount` is 3 and `warningCount` 3. **Plain.** Each event appears once, then `FOMOD configuration check failed: 3 issues: [1] ...; [2] ...; [3] ...` with each issue in its slot. **Install.** The same events, `Installation blocked before any write` with the same numbered list, and an empty game. |
| `issues_then_warnings`: two issues, then warnings only on later components | The same three surfaces. |
| `unreadable_then_issue`: an unreadable registered archive (C# `ArchiveUnreadable`), then configured without saved choices | The same three surfaces, with 2 issues and no warning. |
| `issues_before_order`: two issues and an unselected hard dependency (`--use-file-selection`) | Report and plain list both issues. Rust's order error is not reached, as before. |
| Controls | **Single issue after a warning.** The text is exactly the old `FOMOD configuration check failed: <issue>`, with no count or numbering, on report, plain and install, and `errorCount` is 1. **Shared archive.** One unconfigured archive registered by two components is listed once. **Warnings only.** The gate passes with `(2 warning(s))`. |

## Results (cloud Linux, default-feature debug build)

- **This change:** 67/67 checks over 17 production invocations.
- **Pre-fix:** the same probe against the `d78aea2` binary gives 45/67, with 22 failures.
  It was run with `FOMOD_GATE_COLLECT_ALLOW_UNBOUND=1 MODSYNC_BINARY=<copy>` and output
  to `target/cloud-proof/fomod-gate-collect-pred78aea2`. The failing checks:
  - **`mixed`:** `_report_warnings`, `_report_errors`, `_report_counts`, `_plain_events`,
    `_plain_error`, `_install_events` and `_install_error`;
  - **`issues_then_warnings`:** `_report_stage` and the same six;
  - **`unreadable_then_issue`:** `_report_errors`, `_report_counts`, `_plain_error` and
    `_install_error`;
  - **`issues_before_order`:** `_report_errors`, `_report_counts` and `_plain_error`.

  Exits, untouched trees and all controls pass on both builds.
- **Regressions,** each in its own namespace, are all unchanged with no probe edits:

  | Probe | Result |
  | --- | --- |
  | S1 | 339/339 |
  | inherited | 41/41 |
  | S2a | 224/224 |
  | S2b | 254/254 |
  | fomod-gate-scope | 74/74 |
  | fomod-archive-scoped | 37/37 |
  | fomod-gate-warnings | 55/55 |
- **Gates,** all exit 0:
  - `cargo fmt --all -- --check`;
  - `cargo clippy --locked --lib --bin modsync -j4 -- -D warnings`, with and without
    `--no-default-features`;
  - the same clippy for `--target x86_64-pc-windows-gnu`.

  No Rust test or harness was created or run.

## Reproduction

```bash
cargo build --locked --lib --bin modsync -j2
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-gate-collect-proof/probe.py
```

Set `MODSYNC_PROOF_KEEP=1` to keep the temporary fixture root.
