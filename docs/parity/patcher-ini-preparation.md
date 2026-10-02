# Patcher INI preparation and owned staging

Source implemented and independently reviewed on 2026-10-02. Runtime/backend/native acceptance is deferred.

The install engine copies every Patcher payload into a temporary owned directory before invoking a backend. The layout distinguishes a payload parent from a source already named `tslpatchdata`; copied spelling is retained. Normal and numeric namespace invocations use the complete staged payload. Named INI selection maps the actual source spelling into staging, copies the chosen staged file to `changes.ini`, and removes namespace files from both supported staged locations. Original extracted INIs are never normalized in place. The process working directory is also staged.

The pinned C# reference (`Instruction.cs` and `TSLPatcher/IniHelper.cs`) applies three case-insensitive multiline replacements: `PlaintextLog=0` becomes `PlaintextLog=1`, `LookupGameFolder=1` becomes `LookupGameFolder=0`, and any `ConfirmMessage` value becomes `N/A`. Rust performs these replacements only inside staging. BOM-marked UTF-8, UTF-16 and UTF-32 are decoded, and output is UTF-8. Malformed Unicode is explicitly rejected instead of using replacement characters. Simulation reuses the decoder but never stages, normalizes, or starts a backend.

Staging rejects links, special files and Windows reparse points. It copies at most 100,000 entries and 64 GiB of actual bytes, checking cancellation between entries and every 64 KiB read. INI inventory is bounded to 100,000 entries, 4,096 INIs, 8 MiB per file and 64 MiB aggregate input. All INIs are decoded and validated before publication. Each changed file is reread for concurrent changes and published through a sibling temporary file with original permissions. Read-only destination failures are explicit. No hostile same-user mutation isolation is claimed.

Whole-run recovery or the component Materializing intent precedes preparation. Failure runs no later instruction or component. The temporary owner lives through process settlement and bounded diagnostic collection. Successful preparation or exit zero does not establish backend parity; required fresh logs and error interpretation remain separate postconditions.

Source review corrected original-input mutation, missing Windows reparse checks, case-sensitive named-data lookup and root-level namespace precedence. No filesystem tests, custom harnesses, backend launches or CI were used for this slice.
