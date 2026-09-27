# Task1 fixround1 — final human report whitespace

Base:4c1a68dd5ef5805591171bd9d590c653913994cc. Read task-1-review.md P2 verbatim. Pinned C# BuildValidationReportText returns report.ToString().TrimEnd(); the previous Rust formatter retained trailing whitespace and two goldens incorrectly expected a terminal newline.

Scope:src/validation/presentation.rs final formatting boundary plus tests/validation_presentation.rs literal goldens. No engine, transport, warning-copy or backend classification changes. Original missing-source parity gap remains open.

Expected red: `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo test --test validation_presentation -j 2` failed3 updated/new exact-output tests and passed4; /tmp/task1-fix1-red.log. Failure demonstrates the original trailing-whitespace defect, including the pre-existing no-cards/no-log golden.

Correction: final text.trim_end().to_owned(), preserving all internal text/blank lines/whitespace. Exact-output tests cover full log ending with mixed ASCII/Unicode whitespace; cards without log; no cards/no log; and whitespace-only log (retains log section heading but trims following whitespace).

Focused checks passed:
- `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo test --test validation_presentation -j 2`:7 passed,0 failed; /tmp/task1-fix1-tests.log.
- `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo clippy --lib --test validation_presentation -j 2 -- -D warnings`:passed; /tmp/task1-fix1-clippy.log.
- `cargo fmt --all -- --check` and `git diff --check`:passed.

Commit:9d77f4ba9955ea505d3eb7969afdf5b5aee73b72 (only owned formatter/test files). Full4c1a68d..HEAD delta:task-1-fix1-review.diff. Ready for independent re-review; Task1 acceptance remains pending. Existing full240/all-targets and210/no-default suites were not repeated per parent instruction for this isolated formatter correction. Compiler slot RELEASED; no subagents or push.
