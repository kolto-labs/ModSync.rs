# Missing-source recovery for Extract, Copy, Move and Patcher

This production source slice ports the pinned reference fallback routing in
`ModComponent.cs` (`TryResolveInstructionSourcesAsync`,
`TryAutoExtractMissingFilesAsync` and `TryRemapMoveFirstModSubfolderToVariant`).
Code: `src/engine/source_resolution.rs`, called from the Extract, Copy/Move and
Patcher arms of `Engine::execute`.

## Direct binding is unchanged

Every Extract, Copy, Move and Patcher instruction first binds its sources the way
it did before this slice. Copy, Move and Extract qualify every authored pattern,
then match the VFS. Patcher binds one literal file or directory. A hit returns
those sources with no snapshot, no recovery budget and no extra refusals. Explicit
Extract still unpacks each archive with the configured per-archive options,
writes members in order and runs configured FOMOD copies through the ordinary
Copy action.

Only a typed missing source (`MissingSource`) enters recovery. I/O, cancellation,
unsafe paths, malformed patterns and stale FOMOD choices stay errors. Rename,
Delete, Run, Execute and CleanList never recover.

## Recovery order

1. Qualify every authored source, destination, Patcher trust and Patcher argument.
2. Auto-extract: each missing source is matched against component
   `ResourceRegistry` entries whose `Files` value is exactly `true`. The registry
   KEY is the archive identity, as in the reference. Only a safe literal local
   archive key under the mod directory is admitted. URL keys and URL basename
   guesses are refused with an explicit-Extract route.
   - Every missing source is inspected before deciding. A later ambiguity, unsafe
     key or absent archive is reported even when an earlier source has no owner.
   - A source with no owner makes auto-extract unavailable. Nothing is extracted.
     The reference extracts the owned archives first and then reports failure.
     Rust stages nothing, so a failed recovery leaves no partial output.
   - A source declared by more than one archive is an error. The reference logs a
     warning and falls through.
3. If auto-extract ran: retry the direct match, then look for extracted
   descendants by basename. A literal basename must have one descendant. A
   wildcard may match several. No descendant at all is a missing source, so
   dry-run reports the component as skipped.
4. If auto-extract was unavailable: Move remaps the first mod subfolder to a
   variant directory (for example `Skyboxes` to `Skyboxes_1k`).
5. Last, a Move into the game directory counts as already satisfied when every
   source is a nonempty literal basename already present at the target.

A final miss is `MissingSourceFiles` for Copy, Move and Extract, which is the
existing dry-run skip cause. Patcher keeps its existing
`Missing external program/mod` error.

## Staging

Recovery copies the VFS maps into a shadow overlay and restores the originals
afterwards, including on unwind. Auto-extracted members, configured FOMOD copies
and the final action targets are all checked in the overlay. Writes publish only
after the whole binding succeeds. Checks:

- a staged file may not collide with a directory, and a file may not block a
  target parent;
- two derived writes to one target are refused, including FOMOD members that
  would overwrite each other during implicit extraction;
- recovered Copy/Move sources may not collide at one flattened target, or equal
  their target;
- a recovered Patcher file binds its parent directory, which must exist and hold
  a patcher configuration.

Configured FOMOD archives found by auto-extraction still require saved choices.
A FOMOD copy source must come from the archive being extracted.

## Bounds

| Bound | Value |
|---|---|
| Sources per instruction | 100,000 |
| Overlay entries (files plus directories) | 100,000 |
| Archive members across one recovery | 100,000 |
| Path depth | 128 components |
| One path or name | 16 KiB |
| All names visited | 16 MiB |
| Visits | 2,000,000 |
| Implicit expanded bytes | `--max-expanded-bytes` when set, else 8 GiB, shared across the recovery |

Cancellation is checked at every visit and inside archive reads.

## Variant matching

Variant candidates are direct children of the mod directory. A candidate matches
when its ordinal-folded name starts with the authored name and the next character
is `_`, `-` or a decimal digit. Decimal digits are ASCII plus BMP `Nd` code points
from the retained Unicode 15.0 `UnicodeData` file. If the remaining path has a
first directory segment, the candidate must contain it. The shortest candidate in
UTF-16 length wins. A tie in shortest length is refused.

## Known differences from the reference

- Extraction destination keeps the real Rust behavior: no destination means a
  folder named after the archive stem; an explicit destination gets no extra stem.
  The pinned VFS provider always adds the stem.
- A Patcher binding to a directory is a Rust extension. The reference helper
  expects a file. Any Patcher file binding now uses its parent directory as the
  patch base, in both dry-run and install. Before this slice only an `.exe`
  binding did; another file was treated as the base itself.
- Recovery wildcards are ordinal ASCII only and refuse `I`/`i` and newlines. The
  reference uses culture-sensitive, UTF-16 `Regex` IgnoreCase matching. This is
  not a claim of universal wildcard parity.
- The Move variant needs a leading `<<modDirectory>>` placeholder. The reference
  finds the placeholder anywhere in the source, ignoring case. It also takes the
  first shortest directory in enumeration order and reads the real disk; Rust
  refuses a tie and reads the VFS.
- Ambiguous ownership and absent declared archives are errors rather than
  warnings.
