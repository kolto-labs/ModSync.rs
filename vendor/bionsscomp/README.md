# Vendored bionsscomp production snapshot

- Canonical repository: https://github.com/kolto-labs/odynsscomp
- Original source pin: `00c53a90ba1c2ed9085f896e37d0c053b4fecc52`.
- Reviewed snapshot inventory: `PROVENANCE.json`, SHA-256 `5832ae40762a74f46f091a08068f90f1e0da567d06e02cc81d1f268dc2b633da`.
- Production Rust closure: 12 files; every reviewed-source and vendored-file SHA-256 is recorded in that inventory.

This snapshot includes the locally reviewed strict virtual NSS compilation work. It permits one reproducible ModSync batch without publishing dependency revisions first. It does not claim the modified source is already published at the original pin.

## Production packaging

The source-module closure is retained. Trailing `#[cfg(test)]` modules were excluded only after checking that the entire suffix consists of test modules; production prefixes differ from the reviewed local sources only by rustfmt normalization of trailing end-of-file whitespace after stripping. Per-file production hashes record the formatted bytes; original reviewed hashes remain unchanged. The isolated source checkouts retain their original tests unchanged. No tests, fixtures, custom harnesses, binaries, target trees, repository metadata or game data were copied.

Automatic tests, examples and benches, plus library test/doctest targets, are disabled. `UPSTREAM-MANIFEST.toml.orig` preserves the original pinned manifest; `REVIEWED-MANIFEST.toml.orig` records the reviewed manifest before production packaging. `UPSTREAM-README.md` preserves original project documentation.

Workspace-inherited package metadata is flattened into this standalone manifest. `UPSTREAM-WORKSPACE.toml.orig` retains the source of those fields. The original test-only kotor-ncs-isa dependency (whose old repository URL is unavailable) is omitted from this production-only manifest. The compiler library remains std-only. Its separate command-line frontend is not required by the dependency and is not included.

Policy version1 adds an explicit memory namespace/header, immutable source limits, cooperative cancellation/deadline/work guards, bounded recursion/token/AST/output handling and retained diagnostics. Legacy filesystem APIs remain available; callers must explicitly select the strict API.

## License provenance

LICENSE and NOTICE are retained verbatim. The pinned Cargo metadata and LICENSE identify AGPL-3.0-or-later; NOTICE still contains an older MIT claim. This inconsistency is recorded rather than silently changing either legal text. This snapshot does not resolve distribution-license compatibility.

## Verification boundary

The reviewed source library and production CLI builds passed before packaging. Compiler production-CLI checks covered the certified three-script closure using representative numeric tokens, unsafe/missing includes, cancellation, deadlines and resource limits. This does not establish actual installer/game results or original-compiler byte equality. Final ModSync locked production integration is recorded separately outside this vendored source.

Policy version2 admits individual main/include inputs up to128KiB, covering the observed original100,960-byte K1 include. The complete namespace remains limited to1MiB. Expression segments permit192 tokens; token, AST, recursion, work and output ceilings are unchanged. This changes admission capacity, not filesystem access or the compiler search path.

The parser uses precedence climbing so unused binary precedence levels do not multiply nested expression stack frames. The recursion ceiling remains 256. Production CLI verification compiled all seven original K1 Bastila main scripts with representative numeric substitutions and explicit original includes/header, with zero warnings. Five previously successful outputs retained identical NCS hashes. The two previously rejected large scripts now compile; no original-compiler or in-game equivalence is established.
