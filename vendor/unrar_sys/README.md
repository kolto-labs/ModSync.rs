# Local unrar_sys production build correction

## Source provenance

- Crate: `unrar_sys` 0.5.8 from the existing crates.io registry cache.
- Original crates.io package SHA-256 (Cargo.lock checksum):
  `8b77675b883cfbe6bf41e6b7a5cd6008e0a83ba497de3d96e41a064bbeead765`.
- Packaged VCS revision: `8fe139781b92d0678afecc568496f5805401a6c4`, repository
  `https://github.com/muja/unrar.rs`, subdirectory `unrar_sys`.
- Bundled UnRAR version: 7.1, May 12, 2024, from `vendor/unrar/version.hpp`.
- This local patch retains the upstream FFI and actual UnRAR implementation.
  It does not introduce replacement OS-version symbols or change decoding logic.

## Exact production patch

1. `build.rs` uses Cargo's `CARGO_CFG_TARGET_OS` and
   `CARGO_CFG_TARGET_ENV`, rather than build-host `cfg!`/`#[cfg]`, to choose
   Windows system libraries and append `isnt.cpp`. Windows links `powrprof`
   and `shell32`; GNU Windows and non-Windows retain upstream `pthread`
   linkage. The real `isnt.cpp` supplies `WinNT` and `IsWindows11OrGreater`.
2. `os.hpp` spells `powrprof.h` and `wbemidl.h` in lower case; `isnt.cpp`
   likewise spells `wbemidl.h` in lower case. These real MinGW headers work
   on case-sensitive build hosts without external header aliases. Windows
   SDK builds retain the same includes on their case-insensitive filesystem.
3. The Cargo manifest retains upstream package/dependency/features definitions,
   explicitly disabling automatic tests, examples and benches. Root Cargo.toml
   selects this directory through `[patch.crates-io]`.

## Production-only inventory

The copy contains the build-script translation units (including Windows
`isnt.cpp`) and their recursive quoted-include closure: 139 C++/header files.
It excludes examples, package tests, fixtures, scripts, upstream build projects,
registry bookkeeping, and upgrade documentation. The upstream absent
`rarres.hpp` include remains under unused `USE_RC`, unchanged from the package.

Before copying, the Rust `src/lib.rs` trailing test module was excluded; its
production prefix is unchanged. Three entirely disabled `#if 0` self-test blocks
were excluded from `crc.cpp`, `rijndael.cpp` and `crypt5.cpp`. The actual archive
integrity-check operations remain production functionality. No test code was
created or run as part of this patch. Other C++ source bytes are retained except
for the documented include spellings and the license notice in `os.hpp`.

## Licenses

The Rust binding crate declares `MIT OR Apache-2.0`. Those license texts are
copied from the installed sibling `unrar` 0.5.8 package of the same upstream
repository; the `unrar_sys` crate archive itself omitted root license files.
They apply to the bindings, not to the separate UnRAR implementation.

The complete upstream UnRAR license and acknowledgments are retained verbatim
in `vendor/unrar/license.txt` and `vendor/unrar/acknow.txt`. The required full
paragraph 2 also appears in a source comment in `os.hpp`.

## Evidence boundary

This correction was established by source inspection of the existing Windows
GNU production linker failure. Compilation/linking and native archive behavior
are separate acceptance gates; this patch does not claim either succeeded.
