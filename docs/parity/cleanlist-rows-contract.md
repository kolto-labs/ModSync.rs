# CleanList rows and listed names

This contract covers the remaining F07-D CleanList cases of the
[actions audit](engine-actions-archives-audit.md): row grammar (blank rows, CR/LF,
whitespace, empty fields, BOM, rows without a comma), row matching against the
selected component names, comments, CSV quoting, and the file names a row lists. The
reference is pinned C# `d31ccbd3`; C# paths below are under `src/ModSync.Core/` there.
Evidence: [cleanlist-rows-proof](cleanlist-rows-proof/README.md). F07 stays partial
(F07-E texture priority and F07-F compatible-extension lists remain).

## Triage at `f9eab15`

**C# behaviour.** `Instruction.ExecuteCleanListAsync` (`Instruction.cs:682-830`)
splits the list on CR and LF (`:732`), splits each row on every comma with no CSV
quoting (`:745`), trims the first field as the mod name (`:752`) and each other field
as a file name (`:757`), and matches the name with `IsModSelected`
(`ModComponent.cs:1673-1778`). For each file name it deletes
`Path.Combine(targetDirectory.FullName, fileName)` when
`_fileSystemProvider.FileExists` is true (`:780-782`; `RealFileSystemProvider.cs:27`,
`File.Exists`), and otherwise logs `File not found (skipping)` and goes on (`:801`).
`File.Exists` returns false for a path no file can have, so a quoted field
(`"a.tga"`), a wildcard (`*.tga`, `?.tga`) or a name with `<`, `>`, `|` or `:` is
skipped. `[` and `]` are ordinary characters: `c[1].tga` deletes the file of that name.

**Rust at `f9eab15`.** The CleanList arm (`src/engine.rs:2523-2600`) parsed every
listed name of a matched row with the guide path parser, `relative(raw, false)?`
(`src/engine.rs:2576` at `f9eab15`; `src/paths.rs:98-122`). That parser refuses `"`,
`<`, `>`, `|`, `:`, wildcards (`*`, `?`, `[`, `]`), `..`, absolute paths, trailing
dots or spaces and device names, and the `?` failed the whole step. So a CleanList
whose matched row lists one such name failed the dry run, plain `validate` and
`install`, where C# skipped that name (or, for brackets, deleted the file) and
succeeded.

**Status.** A visible difference: the install refused (`Unsafe path: "a.tga"`,
`Wildcards are not allowed here: c[1].tga`) and the Validate step reported an error.
A quoted mod name with a comma (`"Mod, by Author",a.tga`), the usual CSV way to write
it, leaves a field ending in `"` and hit this. Row grammar and row matching already
matched C#.

## Behaviour after this slice

- `cleanlist::unnameable` (`src/engine/cleanlist.rs:61-68`) reports a listed name
  that no Windows file name can spell: one holding `"`, `<`, `>`, `|`, `*`, `?`, NUL,
  or a `:` anywhere but after a leading drive letter. The CleanList arm skips such a
  name as missing (`src/engine.rs:2575-2582`); nothing is deleted for it and the step
  goes on, as in C#.
- Every other name is parsed with `relative(raw, true)` (`src/engine.rs:2583`), so `[`
  and `]` are literal characters. The CleanList arm never expands wildcards: the
  path is looked up as one file, as `File.Exists` does.
- Unchanged and still refused, failing the step before anything is deleted: `..`
  parts, absolute and drive-qualified names (`/x`, `C:\x`, `a:b.tga`), trailing dots or
  spaces in a part, and Windows device names (see Known differences).
- Only the CleanList arm changes. `relative` and its other callers are untouched, and
  `unnameable` has no other caller.

## Edge cases compared with the live reference

Fixture: `game/{chitin.key, x.txt}`, `game/Override/{a.tga, b.2da, c[1].tga, d.tga,
sub/x.tga}`; the guide's one component, `CleanList Row Probe`, runs
`CleanList [<<modDirectory>>/l.csv] -> <<kotorDirectory>>/Override`. `MD` is
`Mandatory Deletions`. Each case is the text of `l.csv`.

| Case | List | Result |
| --- | --- | --- |
| `control` | `MD,a.tga` | `a.tga` deleted (both) |
| `blank_rows` | blank, space and tab rows, CRLF | `a.tga` deleted (both) |
| `cr_only` | two rows split by CR | `a.tga`, `b.2da` deleted (both) |
| `no_final_newline` | `MD,a.tga` without a newline | `a.tga` deleted (both) |
| `padded_fields` | `  MD  ,  a.tga  ,\t b.2da\t` | both deleted (both) |
| `empty_fields` | `MD,,  ,a.tga,` | `a.tga` deleted (both) |
| `bom` | UTF-8 BOM first | `a.tga` deleted (both) |
| `duplicate_field` | `MD,a.tga,a.tga` | `a.tga` deleted (both) |
| `no_comma_row` | `MD` then `MD,a.tga` | warning, `a.tga` deleted (both) |
| `semicolon_row` | `MD;a.tga` then `MD,b.2da` | `b.2da` deleted (both) |
| `mandatory_lower` | `mandatory deletions,a.tga` | deleted (both) |
| `mandatory_suffix` | `MD (K1 only),a.tga` | deleted (both) |
| `empty_name` | `,a.tga` | nothing (both) |
| `stop_words_name` | `The Mod,a.tga` | nothing (both) |
| `unselected_name` | `Some Other Thing,a.tga` | nothing (both) |
| `selected_name`, `selected_name_case` | the component name, any case | deleted (both) |
| `name_substring` | `Row Probe,a.tga` | deleted (both) |
| `token_overlap` | `Probes for Rows Pack,a.tga` | deleted (both) |
| `single_token` | `Probe Extra Words,a.tga` | nothing (both) |
| `comment_mandatory` | `# MD,a.tga` then `MD,b.2da` | `b.2da` only (both) |
| `comment_selected` | `#CleanList Row Probe,a.tga` | deleted (both) |
| `quoted_name` | `"MD",a.tga` | nothing (both) |
| `quoted_selected_name` | `"CleanList Row Probe",a.tga` | deleted (both) |
| `quoted_file` | `MD,"a.tga",b.2da` | `b.2da` only (Rust before: refused) |
| `quoted_row` | `"MD,a.tga",b.2da` | nothing (both) |
| `quoted_name_with_comma` | `"CleanList Row Probe, by Someone",a.tga` | `a.tga` deleted (before: refused) |
| `quoted_files_with_comma` | `MD,"a.tga,b.2da",d.tga` | `d.tga` only (before: refused) |
| `bracket_literal` | `MD,c[1].tga` | `c[1].tga` deleted (before: refused) |
| `bracket_missing` | `MD,c[2].tga,a.tga` | `a.tga` deleted (before: refused) |
| `star`, `question` | `MD,*.tga,b.2da`, `MD,?.tga,b.2da` | `b.2da` only (before: refused) |
| `star_in_subdir` | `MD,sub/*.tga,b.2da` | `b.2da` only (before: refused) |
| `colon` | `MD,ab:c.tga,sub/x:y.tga,b.2da` | `b.2da` only (before: refused) |
| `angle_pipe` | `MD,a<b.tga,a>b.tga,a\|b.tga,b.2da` | `b.2da` only (before: refused) |
| `subdir_slash` | `MD,sub/x.tga` | deleted (both) |
| `dot_part` | `MD,./a.tga` | deleted (both) |
| `directory_name` | `MD,sub,a.tga` | `a.tga` only; the folder stays (both) |
| `missing_file` | `MD,none.tga,a.tga` | `a.tga` deleted (both) |
| `backslash_subdir` | `MD,sub\x.tga,a.tga` | known difference |
| `case_variant` | `MD,A.TGA,b.2da` | known difference |
| `dotdot` | `MD,../x.txt,a.tga` | known difference |
| `absolute` | `MD,<game>/x.txt,a.tga` | known difference |
| `drive_relative` | `MD,a:b.tga,b.2da` | known difference |
| `drive_absolute` | `MD,C:\Windows\x.txt,b.2da` | known difference |
| `trailing_dot` | `MD,a.tga.,b.2da` | known difference |
| `reserved_name` | `MD,nul.tga,b.2da` | known difference |

Each case runs three ways (pipeline dry run, plain validate, install); 47 cases, 141
checks. The archive/destination stage passed in both for every case.

## Known differences

- **`..` and absolute names (fail-closed, deliberate).** C# `Path.Combine` follows
  `..` and takes a rooted field as it is: `../x.txt` and `<game>/x.txt` deleted
  `game/x.txt`, outside the target. Rust refuses the step before deleting anything
  (`dotdot`, `absolute`; all three checks). A cleanup list must not reach outside its
  target folder.
- **Drive-qualified names (fail-closed).** `a:b.tga` and `C:\Windows\x.txt` are
  missing files to C# on Linux, which skips them; on Windows they leave the target
  (`C:\Windows\x.txt` would be deleted). Rust refuses them as absolute
  (`drive_relative`, `drive_absolute`).
- **Trailing dots and device names (fail-closed).** C# on Linux skips `a.tga.` and
  `nul.tga` as missing; on Windows Win32 trims the dot, so `a.tga.` deletes `a.tga`,
  and device names are not files. The meaning depends on the platform, so Rust
  refuses them on every host, as it does for guide paths (`trailing_dot`,
  `reserved_name`).
- **Backslash separators (C# Linux platform accident, not copied).** On Linux C# takes
  `sub\x.tga` as one name and skips it; on Windows `\` is a separator and
  `Override/sub/x.tga` is deleted. Rust treats `\` as a separator on every host and
  deletes it (`backslash_subdir`, install tree).
- **Case (C# Linux platform accident, not copied).** `File.Exists` is case-sensitive on
  Linux, so C# skips `A.TGA`; on Windows it deletes `a.tga`. Rust's lookup is
  case-insensitive on every host and deletes it (`case_variant`, install tree).
- **Unnameable names on Linux.** A Linux file system can hold a file named `a"b.tga`
  or `a*.tga`, which C# on Linux would delete. Rust skips such names on every host, as
  C# on Windows must; the probe has no such file.

## Results

| Binary | CleanList rows proof |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 121 match / 20 known difference / 0 fail (141 checks) |
| this slice, recorded reference | 121 match / 20 known difference / 0 fail |
| `f9eab15` (pre-fix) | 91 match / 20 known difference / 30 fail |

On `f9eab15` the 30 failures are the ten cases marked "before: refused" above, all
three checks each: the dry run reported an error, and plain `validate` and `install`
refused.

## Guide-author note

A CleanList list is plain comma-separated text: quotes are kept as part of a field,
and a mod name or file name cannot contain a comma. A field that cannot be a file name
(quotes, `*`, `?`, `<`, `>`, `|`, `:`) is skipped in C# and now in Rust; brackets are
part of the name. List file names relative to the CleanList's Destination, with `/`,
and without `..`, drive letters, absolute paths or trailing dots: Rust refuses the step
for those.
