# Vendored odypatcher production snapshot

- Canonical repository: https://github.com/kolto-labs/OdyPatcher
- Original source pin: `9b9ee8d1d25e2e97c87df4521a13965ced1e0b3f`.
- Reviewed snapshot inventory: `PROVENANCE.json`, SHA-256 `6cb7c8645f3add0d51402938f4012173d0bd082f52fb2874d3b6c5ba9a7b3ed2`.
- Production Rust closure: 43 files; every reviewed-source and vendored-file SHA-256 is recorded in that inventory.

This snapshot includes the locally reviewed strict virtual NSS compilation work. It permits one reproducible ModSync batch without publishing dependency revisions first. It does not claim the modified source is already published at the original pin.

## Production packaging

The source-module closure is retained. Trailing `#[cfg(test)]` modules were excluded only after checking that the entire suffix consists of test modules; production prefixes differ from the reviewed local sources only by rustfmt normalization of trailing end-of-file whitespace after stripping. Per-file production hashes record the formatted bytes; original reviewed hashes remain unchanged. The isolated source checkouts retain their original tests unchanged. No tests, fixtures, custom harnesses, binaries, target trees, repository metadata or game data were copied.

Automatic tests, examples and benches, plus library test/doctest targets, are disabled. `UPSTREAM-MANIFEST.toml.orig` preserves the original pinned manifest; `REVIEWED-MANIFEST.toml.orig` records the reviewed manifest before production packaging. `UPSTREAM-README.md` preserves original project documentation.

The compiler dependency points to `../bionsscomp`. The existing optional nss-compiler feature also enables exact rustix1.1.4 on Linux for safe descriptor-relative staged reads. Remaining production dependencies/features are retained.

The optional strict installer policy loads exactly certified staged bytes, substitutes actual predecessor memory, compiles the explicit in-memory closure, preserves warnings and stops on the first failed operation. Strict outcomes additionally identify the certified raw main, transformed UTF-8 main and exact compiled NCS bytes by SHA-256. Legacy callers are unchanged unless the strict policy is selected.

## License provenance

The upstream AGPL-3.0-or-later LICENSE is retained verbatim. No separate NOTICE file exists at the pinned source root. Existing licensing obligations are not changed by using a path dependency.

## Verification boundary

The reviewed source library and production CLI builds passed before packaging. Compiler production-CLI checks covered the certified three-script closure using representative numeric tokens, unsafe/missing includes, cancellation, deadlines and resource limits. This does not establish actual installer/game results or original-compiler byte equality. Final ModSync locked production integration is recorded separately outside this vendored source.
