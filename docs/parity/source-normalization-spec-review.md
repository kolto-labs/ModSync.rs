# Source normalization: independent specification review

**SPEC APPROVED** for source `62b6f707147b734714e4dda7f46118beff3d6140` and
evidence `0f48868`. Fresh reviewer inspected source/diff before worker receipts.
No edits, builds, tests, Rust harnesses, network or Git mutations during review.

One shared helper serves stored Sources and equality. Case-insensitive Choose
exemption precedes canonicalization; bare blanks remain, array blanks are filtered,
malformed values retain existing errors, Options recurse, placeholder case survives
and rewriting is idempotent. Existing selection defaults, deduplication structure
and Markdown exclusions remain; paths.rs and tests are unchanged.

Retained production scenarios cover four structured formats, authored scalars,
common embedded YAML/TOML, separate Rust fenced JSON, inherited and filtered-array
deduplication, Choose, malformed values and deep options. Independent comparison
of supplemental projections confirms documented differences: C# drops grandchildren
and has an XML empty-array dictionary-string path. These are not hidden matches.

Both actual Copy guides independently hash
`03d4f6ef9fca4907a527891abc05b2cf3c674aaf1b40e82189fd856862282e36`; recorded
payloads match `4edc2ebc3b8f737678840127749239b1e288264249da9590249e64b421ee7341`.
Inside-root positive/outside sentinel and corrected independent unsafe-path probes
reach their intended behaviors. C# full validation fails missing HoloPatcher;
its successful Copy uses --skip-validation and does not prove validation parity.
Rust positive/blank-source validation leaves file manifests unchanged.

All retained SHA256SUMS entries independently verified. Model source hash
`b54f38f1b2c5103a9b92e855e4ed6edee7e1aba558717c7a4939930a37579354` matches
gates. Build/freeze/runtime results were inspected from receipts, not rerun by
reviewer. Five production gates pass and53 frozen files/regions remain unchanged.

Confidence high within [bounded evidence](source-normalization-proof/README.md).
No whole F01/F05, export, all-actions, GUI, full-build, live-game or cross-platform
acceptance. Independent quality review remains separate.
