# E1 patcher log postcondition: bounded source change

This slice closes the known exit-zero/no-log and ignored-cleanup-error success paths. It does not complete backend acceptance or establish full E1 parity.

## Implemented contract

Before spawning a patcher, remove `installlog.rtf` and `installlog.txt` only at the resolved invocation root. Only `NotFound` is ignored; a failed deletion aborts before execution. Successful cleanup prevents a log left at these paths by an earlier attempt from satisfying the postcondition. The contract assumes exclusive ownership of the invocation directory; it is not an authentication mechanism against another process concurrently writing there.

After the process terminates, select RTF before TXT. TXT is considered only when RTF is absent. Inspection errors, symlinks, directories, unreadable files, and non-UTF-8 content fail verification rather than falling through to a less preferred file. Both missing is failure. There is no recursive search or acceptance of logs in an unrelated parent, sibling, or game directory.

The selected log's full decoded content is copied into an engine event before stdout/stderr processing, exit-status checks, error-marker checks, or named-INI staging cleanup. A verification failure emits its contextual error. Consumers receive owned text, so a removed staging directory does not invalidate a successfully read log's diagnostic event. This is event delivery, not a claim that every consumer durably stores it. The alternate lower-precedence log is not retained. Cancellation that successfully kills and waits for the child follows the same diagnostic collection path before returning failure. Spawn, kill, or wait errors still fail immediately.

`Error:` (including no following space) and `[Error]` flag error lines; CR and LF both delimit lines. Exit failure and cancellation remain failures even when the selected log is clean. Validation does not perform log cleanup or subprocess execution.

The invocation-root mapping is unchanged: executable-file Source uses its parent; directory Source uses that directory; numeric namespace keeps that root; a named INI uses the temporary staging root. Existing source resolution recognizes `.exe` as executable-file Source. These are current invocation semantics, not verified contracts for every backend.

## Reference and local backend inspection

Pinned C# `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.Core/Instruction.cs`, `VerifyInstall`, selects RTF before TXT, throws if both are absent, and reads the selected content for error markers. Its preparation removes both expected log paths. Rust now rejects failure to remove a previous log instead of ignoring it.

The following local source snapshots were read without executing backend programs. Commit IDs identify checkout HEAD; they do not identify shipped binaries or certify a clean checkout.

| Local source | Observed source behavior |
| --- | --- |
| PyKotor HEAD `ca61ce8d53f5442f73c109e8518cd9a838b2f37c`, `Tools/HoloPatcher/src/holopatcher/core.py:630-679` | `get_log_file_path` returns `mod_path/installlog.txt`; log entries append as UTF-8. |
| KPatcher HEAD `c582c08b432507af0e02d7fd3ad5a6b5015fe0f5`, `src/KPatcher.Core/Patcher/ModInstaller.cs:99-166` | Installer log is beside the resolved `changes.ini`; PlaintextLog selects TXT or RTF. This can differ from the current invocation root. |
| OdyPatcher HEAD `859aba157591a27f3dfe71f3db8c0d398bdd5c34`, `src/session.rs:151-172` | Writes at `base_path`, selects TXT/RTF from settings, encodes Latin-1, and writes nothing at log level zero. Base-path resolution still needs end-to-end backend proof. |

A backend that writes elsewhere, disables logging, or emits non-UTF-8 bytes can now fail closed. No permissive fallback or undocumented no-log success exception was added. Version-bound path/encoding adapters require a confirmed binary/source identity and observed acceptance; the local source observations alone are insufficient.

## Remaining acceptance debt

No tests, fixtures, harnesses, backend runs, builds, Cargo commands, or CI were created or executed for this slice. Existing inline tests were not edited.

The E1 controlled production CLI matrix remains deferred: fresh TXT/RTF/both; absent/stale/unrelated/unreadable logs; both error markers; nonzero exit with clean log; cancellation; named-INI staging; subsequent-component suppression; pending recovery state; verified restore and ordered replay. Actual bundled backend/version log locations and encodings need explicit verification for executable-file, directory, numeric namespace, and named-INI invocations. Resource semantics remain E7 debt. Source inspection and emitted diagnostics do not establish live patcher correctness.
