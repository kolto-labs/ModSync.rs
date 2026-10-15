# Oracle checkpoint comparison

`oracle_state.py` is a bounded prerequisite for per-mod easy-builder evidence.
It reconstructs recorded file states and compares SHA-256 content. It does **not**
install anything, execute patchers, prove guide order, or prove easy-builder
capture/replay, rollback correctness, runtime behavior, or full installation parity.

## Commands and exit status

Existing positional subcommands remain supported (run from the repository root):

```sh
python3 -I tools/oracle_state.py verify ROOT k1_manual
python3 -I tools/oracle_state.py dump ROOT k1_manual 12
python3 -I tools/oracle_state.py diff ROOT k1_manual 12 OTHER.sha256
python3 -I tools/oracle_state.py tree ROOT k1_manual 12 GAME_DIRECTORY
python3 -I tools/oracle_state.py trees DIRECTORY_A DIRECTORY_B
python3 -I tools/oracle_state.py trees DIRECTORY_A DIRECTORY_B --exclude ./backup --exclude installlog.txt
```

- `0`: comparison matches, dump succeeds, or all available full anchors reproduce.
- `1`: file-path/content differences or anchor mismatches.
- `2`: invalid arguments, invalid/missing input, unsafe file types, read/walk errors,
  observed concurrent modification, missing checkpoints/gaps, or zero full anchors
  for `verify`. Ordinary argparse `--help` exits `0` without reading input trees.

`dump` emits only SHA-256 records on stdout. Scope/exclusion notices go to stderr;
retain **both streams and the exit status** in a future Actions evidence artifact.
Reject a nonzero status before continuing a lane. Do not treat a dump as verification.

## Manifest layout and strict input contract

`ROOT/manifests/k1_seed.sha256` seeds `k1_manual`;
`ROOT/manifests/tsl_seed_src.sha256` seeds `tsl_manual`. No other lanes are accepted.
Each lane uses `ROOT/manifests/LANE/step_NNN.sha256` for changed/added files,
optional `step_NNN.deleted` for removed paths, optional `step_NNN.added` for
informational added paths, and optional `step_NNN_full.sha256` as a full anchor.

- Each SHA-256 line is exactly `64-hex-digits`, two spaces, then `./relative/path`.
  Uppercase hex is accepted and normalized to lowercase. UTF-8 decoding is strict;
  LF and CRLF line endings are accepted.
  Empty lines are allowed; duplicate paths (even with identical hashes) are errors.
- Paths are case-sensitive and must start with `./`. Absolute paths, empty path
  components, `.`/`..` components, backslashes, and control characters are rejected.
  Spaces within a filename are preserved; no escaping or glob expansion is performed.
- `.deleted` and `.added` contain one normalized path per line, with the same
  duplicate/path checks. Missing sidecars mean no deletions/no added-path annotation.
  Added paths must occur in that step's delta. Deletion of an absent path is a no-op;
  deletions apply before delta updates. A delta is authoritative for added content.
- Step numbers are `000` through `999`. Normally the first delta is `step_001`.
  Optional `step_000` is applied to the seed as a recorded base delta, not substituted
  for the seed. Requesting checkpoint zero requires an actual `step_000.sha256`.
- Every delta from the first step through the requested checkpoint must exist,
  including the requested step itself; empty deltas may represent no file changes.
  Recognized sidecars/full anchors without a corresponding delta are rejected even
  beyond the requested checkpoint. Unrelated filenames are not checkpoints.
- `verify` reconstructs through the last delta, rejects gaps, and checks every
  available full anchor. Zero anchors is an error, not success. Anchors are not
  required at every step: success only covers the anchors that actually exist.
  `diff`, `tree`, and `dump` validate encountered anchors' syntax but do not establish
  their agreement; run `verify` separately when anchor consistency is required.

## Exact by default; exclusions must be explicit

There are **no implicit exclusions**, including `backup`, `tslpatchdata`, or
`installlog.txt`/`installlog.rtf`, anywhere in a tree. Old results that ignored these
names may now differ. Historical manifests that omitted files cannot prove exact
equality to a tree containing those files.

Every subcommand supports repeatable `--exclude RELATIVE_PATH` after the subcommand.
An exclusion matches exactly that root-relative file or subtree, case-sensitively;
it does not match the same basename in other directories. A leading `./` and a
single trailing `/` are optional. Root-wide, absolute, traversal, and glob-style
matching are not supported (glob characters are literal filename characters).
The normalized rules and excluded file counts for each compared state are reported
on stderr. Filtered equality is **not exact equality**, even when zero files match
the exclusion rules. Empty valid states can match; equality is not evidence that
an install ran. Exclusions may also remove all compared files.

Filtering occurs **after** parsing/hashing: excluded inputs are still validated,
read, and hashed. Exclusions cannot hide malformed records, symlinks, special files,
unreadable files, or observed changes, and do not make scans cheaper.

## Fresh scans and boundaries

Scans hash every regular file afresh; no size/mtime cache is read or written.
Old adjacent `*.scan-cache.json` files are left untouched and ignored as caches.
If such a file is inside a scanned tree, it is ordinary compared content.
Missing roots and non-directory roots fail. Symlink roots, symlink ancestor
directories, symlink entries, and non-regular files are rejected; walk/read errors
propagate. Manifest inputs likewise must be regular files under real directories.

File identity, mode, size, mtime and ctime are compared before opening, on the open
descriptor before/after reading, and on the path afterward. Nonblocking/no-follow
opens avoid following a replaced final symlink or blocking on a replaced FIFO.
Scans recheck observed file/directory metadata before returning to detect additional
observable changes. This implementation requires POSIX `O_NOFOLLOW`/`O_NONBLOCK`
(the intended runner is Linux); it is not a native Windows scanner.

Python 3.9 or newer is required. These checks are **not an atomic snapshot or a
hostile-filesystem security boundary**.
Ancestors can race with path-based traversal, changes can occur after checks, and
metadata behavior depends on the filesystem. Freeze writers or use immutable,
trusted snapshots. Sequential tree scans do not establish simultaneous equality;
multiple manifest reads likewise need stable inputs. File metadata, permissions,
empty directories, hardlink topology, and semantic resource equivalence are not
compared; exact means regular-file paths and SHA-256 contents only.

Future Actions lanes must supply their own authorized manifests, checkpoints, and
trees and preserve provenance, scope, command, output, and exit code after **each**
mod. No cloud credentials, private game trees, downloads, or local caches are assumed
or provisioned here. This change does not wire Actions or establish per-mod proof.

Compatibility changes are recorded here because this bounded change owns only
this document and `oracle_state.py`; the repository-wide `COMPATIBILITY.md` is not
modified. Validation during the parity freeze is limited to production `--help`,
in-memory syntax compilation, and source review: no tests or custom harnesses.
