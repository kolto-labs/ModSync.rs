# FOMOD S1 independent source quality review

## Round 1: CHANGES REQUIRED

Source `f3f27d346c9a07dd102d23ed8992f72d19f3c8d2` (unchanged at `3fcb24e`), baseline
`2d0ecd4`. Independent read-only reviewer; no cargo, no tests.

| # | Severity | Location | Finding |
| --- | --- | --- | --- |
| 1 | blocking | `src/fomod_cli.rs` Unix `TerminalInput::new` | `F_SETFL O_NONBLOCK` on fd 0 mutates the open file description shared with stdout/stderr on a TTY: prompt writes can fail `WouldBlock` (spurious error, exit 1), and SIGTSTP/SIGHUP/SIGKILL leave the user's shell TTY non-blocking. "Scoped to stdin" rationale was wrong. |
| 2 | should-fix | `src/fomod_cli.rs` dispatch | Mode resolved eagerly; malformed settings or a missing choices file fail runs where every consumer already has usable saved choices. Contract says lazy. |
| 3 | should-fix | `src/fomod_cli.rs` + `src/main.rs` record | Warn/Skip emit identical-status edits, forcing a guide rewrite and repeated warnings on every `--record` run. |
| 4 | should-fix | `src/main.rs` report | `fomodFollowups` built from the pre-dispatch scan disagrees with post-dispatch exit code; duplicates the desktop projection with another predicate. |
| 5 | should-fix | `src/fomod_cli.rs` Windows supervisor | Unbounded silent `CancelSynchronousIo` retry loop can hang Ctrl-C with no message. |
| 6 | nit | `src/fomod_cli.rs` | `unwrap` on archive UTF-8 name depends on another module's invariant. |
| 7 | nit | `src/fomod.rs` | Scoped status writer duplicates `set_prompt_status`. |
| 8 | nit | `src/fomod_cli.rs` | stderr lock held across blocking read. |
| 9 | nit | `src/main.rs` record | FOMOD application failure also drops acquisition facts. |

Verified sound: Unix poll/read/EINTR handling and residual-line EOF, Windows
bindings and mandatory join, clone-validate-commit atomicity, `atomic_write` save,
top-level consumer binding, exit latching after recording.

Controller confirmed finding 1 by source inspection. Fixes dispatched to a fresh
implementer; a new independent QUALITY round follows.
