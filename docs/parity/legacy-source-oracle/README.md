# Legacy Source conversion reference

Pinned C# production CLI conversion of the retained generic legacy-bullet Markdown
returns exit0 and keeps `folder\\example.tga` as its raw Source. This confirms the
source-audit distinction from structured deserialization, which prefixes and
normalizes paths. The exact fixture, argv, stdout/stderr, output object and pinned
DLL hash are in receipt.json. Invocation uses a read-only-host, network-isolated
bubblewrap sandbox with disposable writable roots. No Rust fix or downstream
installation equivalence is established. The Markdown plan chooses explicit
uniform safe Rust normalization and requires separate execution comparison.
