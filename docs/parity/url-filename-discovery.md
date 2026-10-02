# URL filename discovery for authoring

`download_orchestrator::filenames::resolve(&Guide, Option<String>, &AtomicBool)`
returns a `Report` with `resolved: Vec<archive_generation::resolved::ResolvedFiles>`,
fixed-code diagnostics, and cancellation state. It visits every top-level
component regardless of install selection, matching the conversion workflow.
Nested options are not independently treated as download consumers here.

Nonempty authored Files dictionaries take priority. Every entry except explicit
false is retained; all-disabled dictionaries intentionally produce no files.
Malformed dictionaries and unsafe names fail that resource visibly. Empty/missing
Files invokes provider discovery: DeadlyStream's bounded page resolver, MEGA node
attributes, Nexus explicit file metadata or MAIN/UPDATE/MISCELLANEOUS file lists,
and Direct metadata HEAD with the existing headers-only Range fallback. GameFront
is explicitly unsupported. Nexus metadata does not require premium download
eligibility. Existing API credential scoping, redirect rules and request deadlines
remain owned by provider readers; no archive acquisition or local source walk is
used. This operation never writes files or reads archive payload bodies.

The report never includes provider errors, resource URLs, query strings, fragments,
keys, cookies or API response bodies. Diagnostics identify a bounded authored
component GUID and resource ordinal. Failure/cancellation remains visible alongside
successful resources. Callers must reject incomplete discovery or retain a pending
draft diagnostic; the resolved subset alone is not a complete generation result.

All outputs use safe single filenames without globs. Exact duplicates per component
are removed in first-discovery order; case aliases produce a diagnostic rather
than silently choosing between them as a successful result. Bounds are 8 MiB input,
4096 components/resources, 4096 filenames, 1 MiB total filename bytes and 255
bytes per name. Component identities must be nonempty, unique and at most128 bytes.
Provider-specific smaller response limits also apply. Cancellation is checked
between resources and names and by cancellable provider readers; blocking network
calls retain their existing finite request deadlines.

Pinned C# `AutoInstructionGenerator.GenerateInstructionsFromUrlsAsync` resolves
all filenames before deciding whether to inspect local archives or create
placeholders. This API supplies that filename seam only. Actual generated
instructions and review requirements belong to `archive_generation::resolved`.

Evidence: production source inspection and targeted formatting only. No network
requests, tests, builds, runtime execution or CI activity were performed.
