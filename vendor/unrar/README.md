# Local unrar production build correction

## Source provenance

- Crate: `unrar` 0.5.8 from the existing crates.io registry cache.
- Original crates.io package SHA-256 (Cargo.lock checksum):
  `92ec61343a630d2b50d13216dea5125e157d3fc180a7d3f447d22fe146b648fc`.
- Packaged VCS revision: `8fe139781b92d0678afecc568496f5805401a6c4`, repository
  `https://github.com/muja/unrar.rs`, repository root.
- It depends on `unrar_sys` 0.5, which root Cargo.toml redirects to
  `vendor/unrar_sys`.

## Exact production patch

1. `src/open_archive.rs`: `Internal::<M>::callback`, the only Rust function
   handed to native code (through `native::RARSetCallback` in `read_header`
   and `process_file_raw`), is declared `extern "system" fn` instead of
   `extern "C" fn`. This matches the vendored `unrar_sys::Callback` type,
   which follows UnRAR's `dll.hpp` `CALLBACK` (`__stdcall` on Windows).
   `"system"` is stdcall on 32-bit x86 Windows and identical to `"C"` on every
   other target. Leaving the callback `"C"` would fail to type-check against
   the corrected `unrar_sys`, and forcing it through would corrupt the stack
   on `i686-pc-windows-msvc`.
2. The Cargo manifest retains upstream package/dependency/features
   definitions (automatic tests, examples and benches are already disabled
   upstream). The explicit `[[example]]` and `[[test]]` targets and the
   `tempfile` dev-dependency were removed, since those files are not copied.
   Root Cargo.toml selects this directory through `[patch.crates-io]`.

## Production-only inventory

The copy contains `src/` (lib, archive, error, open_archive, pathed) and the
license files. It excludes examples, integration tests, test archives under
`data/`, the upstream CI workflow, changelog, `Cargo.toml.orig`, the package
`Cargo.lock`, and registry bookkeeping. This README replaces the upstream one.

The trailing `#[cfg(test)]` modules of `src/archive.rs` and
`src/open_archive.rs` were excluded; the production prefix of each file is
unchanged apart from patch item 1. No test code was created or run as part
of this patch.

## Licenses

The crate declares `MIT OR Apache-2.0`; both license texts are copied
verbatim from the package.

## Evidence boundary

This correction was established by source inspection of the
`i686-pc-windows-msvc` linker failure (unresolved stdcall-decorated
`_RAR*` symbols). Host builds confirm it compiles; the i686 MSVC link is a
separate acceptance gate this patch does not claim succeeded.
