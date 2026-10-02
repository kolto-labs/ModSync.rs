# Shared GUI tools implementation — 2026-10-02

Working-source continuation after the T4 production gate. This document records
source contracts, not runtime or native acceptance.

## Editor

`editor::apply_operations` takes an immutable guide and ordered typed operations.
It applies them to a clone, reports an operation index for target/bounds/identity
errors, and validates the final guide before returning it. A rejected transaction
cannot alter its input. Component replacement carries all metadata, reference,
resource, selection and nested option fields. Instruction replacement carries
all source/destination/action/argument/overwrite/reference fields. Insert/remove/
move address recursive components by GUID and instructions by current index.
Moves remove and insert, preserving the relative order of other children.

Final validation includes names, component identity, all four reference lists,
instruction conditions, direct Choose targets, dependency/order cycles across
definitions, and centralized draft-review record validation. It permits Unset
instructions during authoring. Installer preflight remains responsible for
executable safety and accepting generated drafts.

`guide-edit INPUT OPERATIONS OUTPUT` reads a bounded JSON operation list, retains
authored selection, and atomically saves a distinct output only after validation.
No runtime command has been executed for this slice.

`guide-import-preview` parses a local document or a document already fetched by
the existing protocol command. Append and single-component replacement produce
an immutable operation preview bound to the input guide's SHA-256 identity.
`guide-import-apply` refuses an obsolete preview. Import does not implicitly merge
document-level metadata; its preview explicitly warns when that metadata exists.
`--regex-profile` supports raw named-capture and individual section/field profiles,
plus reset to the default Markdown parser. Custom previews retain UTF-8 capture
offsets and unmatched line numbers. Name/heading, author, prose, category/tier,
dependency names/GUIDs, warnings, language and HTTP(S) resource links map to the
model before the same transaction validation. Ambiguous or missing dependencies
are rejected. Patterns use Rust regex syntax; unsupported .NET lookarounds,
backreferences and duplicate named captures produce explicit errors. This is a
documented syntax difference, not full .NET regex compatibility. Default guide
parsing remains the existing production parser. Input, pattern, component,
capture and serialized-preview sizes are bounded; oversized previews are rejected
before publication, matching the Apply command's 8 MiB input limit.

`guide-resources` enumerates exact registered filenames, including disabled
entries. It checks root containment and uses the shared regular-file opener and
non-spooling archive inventory. Missing/unsafe/undecodable files remain visible
as per-resource errors; cancellation stops the operation. Archive members use the
inventory's normalized lowercase spelling and are relative to an eventual
extraction directory. They are never presented as children of an archive's
filesystem path. The CLI defaults to a 256 MiB expanded-byte limit per archive.
Variable projection checks existing ancestors and rejects traversal/symlinks.

Legacy helper signatures remain intact while the desktop transaction adapter is
pending. Their continued presence is not a claim that the native editor has
already switched to these operations.

## Conflict presentation

`conflicts::Presentation::from_report` consumes an existing shared report. It
retains ordered writers, GUIDs, zero-based instruction indices, winner flags,
component counts, limitations and original report JSON. Path rows sort
deterministically; count rows sort by descending count, name, then GUID. This
projection does no simulation, filesystem access or mutation.

## Update result storage

`StoredSummary` wraps the unchanged provider Summary with schema version 1,
RFC3339 completion time, and SHA-256 identities before and after metadata updates.
The result identity allows a saved checked guide to recognize its last result.
Only the original input identity authorizes applying a fresh result to live
state. Neither identity authorizes loading stored metadata into a guide.

`update-check` now also writes `update-check-snapshot.json` atomically. The legacy
summary file remains available to existing callers. `update-summary` returns one
of absent, valid, identity mismatch, unsupported version, or corrupt. Loading is
bounded to 8 MiB through the regular-file opener and never changes guide state.

Independent source review covered these library/CLI contracts and its concrete
findings were corrected. Desktop adapters remain pending. The combined production
gates are in progress; earlier receipts do not cover this source. Frozen tests
were neither changed nor run.
