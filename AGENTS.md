# ModSync.rs

This is the Rust port. The reference C# repository's .NET commands do not apply here.

- Keep desktop and CLI installation behavior in `src/engine.rs`.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, and `cargo test --no-default-features` are the local checks.
- Use isolated temporary directories for install tests. Never mutate a user's game or baseline for testing.
- Validation must not mutate game/mod trees or run external programs.
- Preserve guide order and stop on failure. Restore the verified snapshot before retrying; never backfill an earlier mod at the end.
- Reject unsafe/unsupported instructions. Do not silently claim feature parity from preserved metadata.
- Update `COMPATIBILITY.md` when a supported behavior or proof boundary changes.
- Do not publish releases or open upstream PRs without a user request.
