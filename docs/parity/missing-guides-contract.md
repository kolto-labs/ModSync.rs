# A missing guide for `convert` or `merge`

This contract covers what `convert` and `merge` say when a guide they are given does
not exist or is a folder. The C# CLI names it before reading anything (`Input file not
found: <path>` for `convert`, `Existing file not found: <path>` or `Incoming file not
found: <path>` for `merge`); Rust said `Read <path>: No such file or directory (os
error 2)`, or `Read <path>: Archive source must be a regular file, not a symlink or
special file` for a folder. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Rust lines before this slice are at `5796295`. Evidence:
[missing-guides-proof](missing-guides-proof/README.md). Tracker item: F26 ("complete
verb/alias/option/preload/pipeline/stdout/stderr/schema/exit contract comparison");
F26 stays partial.

## Why this gap

The previous slice ([missing-paths-contract.md](missing-paths-contract.md)) gave
`validate`, `install` and `conflicts` C#'s words for a missing guide and left
`convert` and `merge` with `Read <path>: ...`. A mistyped guide path is the most common
mistake on these two commands, and for `merge` Rust's message did not say which of the
two guides was wrong in C#'s terms (existing or incoming). Exit codes were already the
same (1) and nothing was written on either side, so no decision changes; only the
message does. The fix refuses nothing new and loosens nothing.

Candidates looked at and skipped (records first, then the live C# CLI in this session):

- **`merge` writes to stdout without an output path.** The audit row
  (`cli-table-audit.md`, `merge` `-o/--output`) says Rust requires a positional
  output; the current binary already prints the merged guide when no output is given,
  as C# does. A stale record, not a gap.
- **`convert -o` into a missing folder.** C# creates the folder
  (`CLI/ModBuildConverter.cs:2304-2318`); Rust also writes `newdir/sub/out.toml`
  (exit 0). Matched.
- **`convert -f bogus`.** Both exit 1 with different wording; C#'s comes from its
  serializer's exception text, and the format list is a separate contract. Left.
- **Missing C# flags (`--concurrent`, `--spoiler-free`, `-a/--auto`, `--plaintext`,
  `-v`, `--non-interactive`, `set-nexus-api-key`, `install-python-deps`).** Most were
  since added (tracker F26: legacy preload, spoiler overlay, standalone HoloPatcher);
  the rest change output style or need network or Python, which no bounded
  user-visible fix can prove here. Left.
- **`--help`/`--version` exit codes and the unknown-verb URL attempt.** Documented
  deliberate differences (Clap owns help; bare URLs route to the GUI handoff).

## Triage

**C# at `d31ccbd3`.** `convert` (`RunConvertAsync`, `CLI/ModBuildConverter.cs:1815`)
hands `--merge` to `RunMergeAsync` (`:1905-1944`), then requires `--input` or
`--stdin` (`:1946-1958`), refuses both together (`:1960-1964`) and, without
`--stdin`, fails with `Input file not found: <path>` when `File.Exists` is false
(`:1966-1970`). `merge` (`RunMergeAsync`, `:2384`) checks `--existing`, then
`--incoming`, the same way, before reading either (`:2473-2483`): `Existing file not
found: <path>`, `Incoming file not found: <path>`. The path is printed as typed;
`File.Exists` is false for a folder. Exit code 1, nothing written. The message is
logged on stdout as `[<time>] [Error] <message>`.

**Rust at `5796295`.** `convert` read its input with `Guide::load`
(`src/main.rs:2314`), and `merge` read the existing guide, then the incoming one
(`src/main.rs:2238-2239`); `Guide::load` (`src/model.rs:165-170`) fails with `Read
<path>: <error>`. `convert --merge` is rewritten to `merge` before parsing
(`src/cli_compat.rs:156-167`), so it read the guides the same way. A folder was
refused by the regular-file check inside the read. Exit code 1, nothing written,
`Error: <message>` on stderr.

**User-visible difference** (fixture with `g.toml` and a folder `adir`):

```
Command                                  C#                                     Rust before
convert -i nope.toml                     Input file not found: nope.toml        Read nope.toml: No such file or directory (os error 2)
convert -i adir -o out.toml              Input file not found: adir             Read adir: Archive source must be a regular file, ...
merge -e g.toml -n nope.toml             Incoming file not found: nope.toml     Read nope.toml: No such file or directory (os error 2)
merge -e nope.toml -n nope2.toml         Existing file not found: nope.toml    Read nope.toml: ...
convert --merge -e nope.toml -n g.toml   Existing file not found: nope.toml    Read nope.toml: ...
```

## Behaviour after this slice

- **`convert`.** Without `--stdin`, the input (`-i` or positional) must be a file
  before it is read (`src/main.rs:2325`); otherwise `Input file not found: <path>`, the
  path as given. `--stdin` is unchanged.
- **`merge` and `convert --merge`.** The existing guide, then the incoming guide, must
  be files before either is read (`src/main.rs:2238-2248`); otherwise `Existing file
  not found: <path>` or `Incoming file not found: <path>`. The positional forms get
  the same checks.
- **Unchanged.** Exit codes (1), the order of checks, and the absence of any write. A
  symlink to a file passes the new check (as `File.Exists` does) and is still refused
  by the read, as before (`Read <path>: ...`; a documented Rust refusal). Other guide
  readers (`merge-preview`, `merge-apply`, `list`, `download-plan`,
  `fetch-downloads`, `selection`) keep `Read <path>: ...`; C# has no such verbs.

## Proof

[missing-guides-proof](missing-guides-proof/README.md) runs 13 cases on both sides
under `unshare -Urnm` against the live C# CLI, and two Rust-only positional cases: the
exit code, the message (the first `... file not found: <path>` text in the output) and
the names written into the fixture, 45 checks.

| Binary | missing-guides |
| --- | --- |
| this slice, live C# CLI | 45 match / 0 known difference / 0 fail |
| this slice, recorded references | 45 match / 0 known difference / 0 fail |
| `5796295` (pre-fix), live C# CLI (`--record`) | 32 match / 0 known difference / 13 fail |
| `5796295` (pre-fix), recorded references | 32 match / 0 known difference / 13 fail |

On `5796295` every `message` check fails except the two controls'. Exit codes and the
written names match on both binaries.

## Known differences

- **Stream and prefix.** C# logs `[<time>] [Error] Input file not found: <path>` on
  stdout; Rust prints `Error: Input file not found: <path>` on stderr, as for every
  Rust error (cli-table-audit.md, "Parser-wide flags and exit behavior"). The probe
  compares the message text only.
- **Not compared.** `convert --stdin` (no file to check) and the desktop app's merge
  and import dialogs, which pick existing files.

## Notes for guide authors

Nothing changes for guides. Users: when `convert` or `merge` stops with `Input file not
found`, `Existing file not found` or `Incoming file not found`, the named guide does not
exist (or is a folder); fix that path and run the command again. Nothing was written.
