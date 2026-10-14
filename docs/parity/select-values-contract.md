# Several values after one `--select`

This contract covers how `validate` and `install` read the `--select` option when
several selections follow one flag: `install -i guide.toml -g game --select
category:Bugfix category:Immersion`. The C# CLI reads every value up to the next
option, so it installs both categories; Rust took one value per flag, so the same
command line stopped with a usage error and validated or installed nothing. The
reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Rust lines before this slice are at `b97c090`. Evidence:
[select-values-proof](select-values-proof/README.md). F26 (CLI) stays partial.

## Why this gap

It decides whether a command runs at all. A user or script that follows the C# CLI
(its options declare `--select` as a list, and its help says "category:Name" or
"tier:Name") and passes two filters after one flag got `error: unexpected argument`
(exit 2) from Rust, with no validation and no install. Matching C# is not less safe:
the values go through the same selection filters as before, and every selected
component is still validated before any write.

## Triage

**C# at `d31ccbd3`.** `validate` and `install` declare `--select` as
`IEnumerable<string> Select` (`CLI/ModBuildConverter.cs:582-583`, `:619-620`), as do
`convert` and `merge` (`-s`/`--select`, `:403-404`, `:503-504`). CommandLineParser
2.9.1 (`ModSync.Core.csproj:99`, default settings at `ModBuildConverter.cs:765`)
reads a sequence option's values up to the next token that names an option, so
`--select a b` gives `["a", "b"]`; it also reads the value after `--select=a`. It
refuses a second `--select` flag (`Option 'select' is defined multiple times.`),
and every parse error returns exit 1 after the help screen (`:777`).
`ApplySelectionFilters` (`:1370-1504`) then ORs the category values, ORs the tier values (a named tier
selects it and every higher-priority tier) and ANDs the two groups; a value without
`:` or with another type only logs a warning. `validate` uses it when `--select` is
given (`:3146-3155`); `install` when `--select` is given or `--use-file-selection`
is not (`:3430-3438`).

**Rust at `b97c090`.** `--select` was `#[arg(long)] select: Vec<String>` for
`validate`, `install` and `conflicts` (`InstallArgs`, `src/main.rs:744-746`), and for
the Rust-only `selection` (`SelectionArgs`, `:847-849`) and `fetch-downloads`
(`:444-446`). Clap reads one value per flag, so a second value was an unexpected
argument: exit 2 before anything ran. The repeated form `--select a --select b`
worked and gave the same selection C# gives `--select a b`
(`apply_selection`, `:922-954`, through the shared selection module).

**User-visible difference** (`install`; components `a`-`e` of the proof's guide):

| `--select` values | C# CLI | Rust before | Rust after |
| --- | --- | --- | --- |
| `category:Bugfix category:Immersion` | installs b, c, d, e | usage error, exit 2 | installs b, c, d, e |
| `"category:Graphics Improvement" "tier:1 - Essential"` | installs a | usage error | installs a |
| `"tier:custom tier" tier:Recommended` | installs a, b, e | usage error | installs a, b, e |
| `category:Bugfix` (one value) | installs b, c | installs b, c | installs b, c |
| `category:Bugfix --select category:Immersion` | usage error, exit 1 | installs b, c, d, e | installs b, c, d, e |

## Behaviour after this slice

- `--select` takes one or more values (`num_args = 1..`) for `validate`, `install`,
  `conflicts` (`src/main.rs:745-748`), `selection` (`:849-852`) and `fetch-downloads`
  (`:444-447`): the values after the flag, up to the next option, are selections, as
  in C#. They join the values of any repeated flag, in command-line order.
- The selection rules are unchanged: the values reach `apply_selection` exactly as a
  repeated flag delivered them before, so categories and tiers combine as in C#, and
  Rust's name/GUID selectors (an exact component name or GUID wins over selector
  syntax) keep working in a list.
- Unchanged: a repeated `--select` is still accepted; `--select` with no value is
  still a usage error, as in C# (exit 2 in Rust, 1 in C#); `convert` and `merge`
  still read one value per `-s`/`--select` (below).

Callers checked: the five structs' `select` fields are only read as slices: by
`apply_selection` (`src/main.rs:925-957`) for `validate`, `install`, `conflicts`,
`selection` and `fetch-downloads`, and by emptiness tests that pick the default
selection and the "No components match" message. The `direct_markdown` conflict
list (`src/main.rs:720` at `b97c090`) names the argument id, not its arity.
`ordered_selection_steps` orders the other selection flags by index and does not
look at `select`. The legacy convert merge normalizer (`conversion_value_flag`,
`src/cli_compat.rs:238-260`) reads one value after `convert`'s `-s`/`--select`,
which did not change.

## Proof

[select-values-proof](select-values-proof/README.md) runs 14 cases through the live C#
CLI and Rust: `validate --output json --full --dry-run`, `install` and Rust's plain
`validate` (42 checks).

| Binary | Result |
| --- | --- |
| this slice, live C# CLI | 34 match / 8 known difference / 0 fail |
| this slice, recorded reference | 34 match / 8 known difference / 0 fail |
| `b97c090` (pre-fix), recorded reference | 12 match / 7 known difference / 23 fail |

On `b97c090` 23 checks fail, all in the nine cases with two values after one flag,
where Rust stopped with a usage error: all three checks of `two_categories`,
`category_and_tier`, `two_tiers`, `values_then_flag`, `values_before_roots`,
`unknown_type_in_list` and `name_in_list`, which C# validates and installs, and the
report of `broken_in_list` and `no_match`, where C# prints a JSON report (passed and
failed). The cases with one value or no value matched before and after.

## Known differences

- **A repeated flag.** `--select a --select b` is refused by the C# CLI (exit 1, help
  screen) and accepted by Rust, which selects as for `--select a b`. Kept: it was the
  documented Rust form, and refusing it would break existing scripts.
- **`--select=a b`.** C# reads `b` as a second value; clap does not, so Rust stops
  with a usage error (exit 2) before anything runs. Fail closed; write `--select a b`
  or repeat the flag.
- **Names in the list.** C# ignores a value without a `:` (a warning); Rust selects
  the component of that exact name or GUID, as before this slice (COMPATIBILITY.md,
  shared selection rules). So `--select category:Bugfix a` also installs `a` in Rust.
- **A selected component that fails.** The C# install checks a missing Copy Source
  only when it reaches that component, so it installs the earlier ones first; Rust
  refuses before any write, as for every validation failure (`broken_in_list`).
- **`convert` and `merge`.** Their C# `-s`/`--select` is a list as well, but Rust's
  `convert` and `merge` take positional input and output paths, which a list would
  swallow (`convert -s category:A in.toml` would read `in.toml` as a selection).
  They keep one value per flag; `-s a b` there is a usage error or a wrong positional
  path, never a silent selection. Not probed.
- **Exit code of a parse error.** C# exits 1, Rust (clap) exits 2; both run nothing.
  The proof compares zero or not.

## Notes for users

Both forms work in Rust: `--select category:Bugfix category:Immersion` (as in the C#
CLI) and `--select category:Bugfix --select category:Immersion`. Quote a value with
spaces: `--select "tier:1 - Essential"`.
