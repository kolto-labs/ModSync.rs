# Expanded C# format oracle

Measured 2026-09-28 UTC by the controller while the independent fetch worker owned
production source/builds. Twelve Source shapes and a nested Option were converted
through pinned C# JSON/TOML/YAML/XML loaders. All four projected Source/Options
trees agree; all seven render/conversion commands exited zero.

`receipt.json` contains projected values and source binary hashes; each command
record preserves full argv/stdout/stderr/exit. Inputs and C# outputs are retained.
`probe.py` is a Python driver invoking existing production binaries, not a Rust
test or harness. C# ran with a read-only host and isolated network/disposable roots.

Non-JSON inputs were rendered by the old production Rust converter. That may
normalize scalar-versus-array shape, so this does not prove every authored shape
in each format. Bare empty/whitespace scalars and malformed non-string values
were omitted to keep the positive conversion matrix runnable; separate negative
cases remain required. No post-fix Rust or full-domain acceptance is claimed.
