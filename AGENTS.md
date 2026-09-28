# ModSync.rs

This is the Rust port. The reference C# repository's .NET commands do not apply here.

- Keep desktop and CLI installation behavior in `src/engine.rs`.
- Current parity workflow: Rust tests and temporary/custom Rust harnesses are frozen until full parity. Do not create, modify, compile, or run them; do not use `--all-targets`. This overrides older test commands in historical plans.
- Production checks: `cargo fmt --all -- --check`; `cargo build --locked --lib --bin modsync -j2` and `cargo clippy --locked --lib --bin modsync -j2 -- -D warnings`, each with and without `--no-default-features`.
- For cloud continuation, read `docs/handoffs/rust-parity-cloud.md` and `docs/parity/cloud/README.md`. No local machine cache, agent mailbox, or private game tree is a prerequisite for source work and controlled fixtures.
- Use isolated temporary directories for install tests. Never mutate a user's game or baseline for testing.
- Validation must not mutate game/mod trees or run external programs.
- Preserve guide order and stop on failure. Restore the verified snapshot before retrying; never backfill an earlier mod at the end.
- Reject unsafe/unsupported instructions. Do not silently claim feature parity from preserved metadata.
- Update `COMPATIBILITY.md` when a supported behavior or proof boundary changes.
- Do not publish releases or open upstream PRs without a user request.
