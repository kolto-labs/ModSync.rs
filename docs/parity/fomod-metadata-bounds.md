# FOMOD metadata inspection bounds

Production source qualification, 2026-10-02. This is not a live archive, installation, or cancellation timing receipt.

Verified archive spools are read without executing installers or touching game paths. Both `ModuleConfig.xml` and optional `info.xml` have an 8 MiB input limit. Reads use 64 KiB chunks and check the caller's cancellation flag before and after every read. The file length is an early hint; bytes actually read enforce the limit even if the spool changes. Input must be UTF-8.

The shared XML reader checks cancellation at each event, rejects DTDs, retains the existing depth limit of 128 stack entries, and permits at most 100,000 nodes and 100,000 attributes. A cumulative 32 MiB logical string budget covers node and attribute names, decoded attribute/text/reference values, and mixed text copied into ancestor nodes. It is a logical byte bound, not an allocator capacity or whole archive bound. Decoder temporaries remain subject to the input bound. The semantic projection has cancellation checks at its boundaries; it does not promise cooperative cancellation within every mapping iteration.

Existing public parser wrappers remain available; additive cancellation variants take the real caller flag. Archive inspection and extraction use that flag. Existing namespace-local names, attribute lookup, authored whitespace, child ordering, and mixed text mapping remain intact. These limits are explicit Rust policy, not claims of matching undocumented C# allocation limits.

Archive integrity, path confinement, extraction ownership, and configured-choice admission still belong to their existing boundaries. A successful metadata parse does not authorize an installation.
