# Basic validation without roots

Reference: pinned ModSync.Core `d31ccbd3`, ComponentValidation,
InstallationValidationPipeline, and ModBuildConverter. Source implementation:
`src/validation/basic.rs`, `src/validation.rs`, `src/archive.rs`, and the shared
CLI adapter in `src/main.rs`.

Ordinary all-phase validation with either effective root absent runs component
validation in the reference null-root context, followed by the failed FOMOD
configuration stage. It does not initialize Roots, Engine, patchers or scratch
directories. Profile and explicit selection policy are resolved first. Both
validation paths use the preserving loader, so an authored all-false guide is
not implicitly changed to all-selected. An empty effective selection is a CLI
setup error. Early JSON setup errors produce the validation report schema;
cancellation retains exit 130. A failed report returns exit 1.

The component stage checks base and immediate-option Extract sources, conditional
archive collection, original-spelling archive membership, literal destinations,
and collected diagnostic/count behavior. Exact-case root variables become empty
strings. Literal paths still receive actual read-only filesystem inspection.
Symlinks, devices, unsafe traversal, ambiguous case aliases and exhausted bounds
produce explicit diagnostics. Explicit empty directory entries match themselves;
only files imply parent directories. EXE archive handling never runs a program.

The terminal FOMOD summary is:

> Cannot check FOMOD configuration: mod directory is not set or does not exist.

It contributes one error and includes the reference recovery hint. This report
does not certify installation readiness.

## Bounds and deliberate differences

- Paths: 16 KiB; inspected path entries: 10,000; depth: 64.
- Unique archives: 64; compressed archive: 512 MiB.
- Expanded bytes: 256 MiB per archive, 1 GiB aggregate. A failed decode reserves
  its full allowance; cancellation never returns successful partial results.
- Shared archive inventory: 100,000 entries and 16 MiB original-name storage.
- Validation never creates a destination, rewrites a guide, or performs the
  reference's automatic-fix side effects. Duplicate-folder retry is a read-only
  probe with retained diagnostics.
- The model loses null-versus-empty Source lists, so its null-Source warning
  cannot be reconstructed. Unsafe-input exception counts can differ.
- Complete effective roots retain the existing Rust engine-based validation
  extension; the pinned basic C# command ignores supplied roots.
- Rootless JSON is one report; the existing rooted success event stream is
  unchanged. Full/dry/phased modes remain on their existing routes.

Independent source reviews found and corrected explicit-directory membership,
unnecessary original-name allocation in existing readers, all-false ingestion,
and the early JSON setup boundary. No tests, fixtures, runtime probes or CI runs
were used. Production compilation is recorded separately; F08/F26 remain partial.
