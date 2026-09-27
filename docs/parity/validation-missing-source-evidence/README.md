# Missing-source full-report measurement

The authoritative observation is in [`current/receipt.json`](current/receipt.json): Rust commit `9d77f4ba9955ea505d3eb7969afdf5b5aee73b72`, production `cargo build -j2` exit 0, unchanged source hashes/HEAD and no production diff, then a private copied executable against a disposable clone of the exact pinned native Copy guide.

Result: **3 errors / 0 warnings / 3 passed**, exit 1, empty stderr. Failed stages are Conflicts, ComponentValidation, and DryRun. Before/after fixture manifests match; no installation was invoked. The guide and removed-source hashes match the pinned C# fixture. Current binary SHA256 is `93f0ae8ec686e8858c20976dccf290e236ddd704d54c37a6f89a37f6f70cd984`.

Files at this directory's top level preserve the first measurement using an already-built copied executable without a new source-bound build. It measured the same counts and binary hash. Do not use its mtime as a commit receipt; the explicit build and rerun in `current/` resolve that provenance limitation.

The previously stated full-report total 2 errors / 0 warnings / 4 passed was an inference from a non-full CLI report, not a measurement. These captured full-report bytes supersede it. See the [source audit and proposed backend task](../validation-missing-source-audit.md).

The executable and disposable fixtures remain under the `/tmp/` paths in the receipts. Raw stdout/stderr, source/build receipts and path/hash manifests are kept here; large executable copies are not added to the repository. No Rust test files were created or changed.
