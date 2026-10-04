# Execute and Run Arguments splitting

This contract covers how an `Execute` or `Run` step's `Arguments` text becomes the
program's argument list. The reference is pinned C# `d31ccbd3`; C# paths below are
under `src/ModSync.Core/` there. Evidence:
[execute-arguments-proof](execute-arguments-proof/README.md). F05 (F05-EXECUTE,
F05-RUN) and F09 stay partial.

## Triage at `0cc50a7`

**C# behaviour.** `ModComponent` runs Execute and Run through
`Instruction.ExecuteProgramAsync` (`ModComponent.cs:1646`, `Instruction.cs:1458-1530`).
It passes `UtilityHelper.ReplaceCustomVariables(Arguments)` (`Instruction.cs:1503-1506`)
to `PlatformAgnosticMethods.ExecuteProcessAsync`, which puts the string into
`ProcessStartInfo.Arguments` with `UseShellExecute = false`
(`Utility/PlatformAgnosticMethods.cs:286-357`, `:359-376`). `ReplaceCustomVariables`
(`Utility/Utility.cs:20-38`) replaces `<<modDirectory>>`, `<<kotorDirectory>>` and
`<<gameDirectory>>` as plain, case-sensitive text before anything is split. No shell
runs. .NET splits `ProcessStartInfo.Arguments` with the Windows command-line rules on
every platform (on Linux and macOS in `Process.Unix`, `ParseArgumentsIntoList`): only
space and tab separate arguments; double quotes group; inside quotes `""` is a
literal quote; `\` is literal unless a run of backslashes precedes a `"`; single
quotes, `#`, `$`, backquotes and newlines are ordinary characters; an unclosed quote
runs to the end. On Windows the child's C runtime splits the same string with the same
rules. So a guide's Arguments mean the same thing on every host.

**Rust at `0cc50a7`.** `process_arguments::expand` (`src/engine/process_arguments.rs:6-11`
at `0cc50a7`) split the text with `shlex::split` (POSIX shell rules) on every
non-Windows host and with `windows_split` (CRT rules) only on Windows, and substituted
the roots into each token afterwards. Its two callers are the pre-install check
(`src/engine.rs:1405`, run before any write) and the Execute/Run step itself
(`Engine::process`, `src/engine.rs:2625`), including managed installs. On Linux and
macOS a guide therefore ran its program with different arguments from C#:

| Arguments | C# (every host) and Rust after this slice | Rust before (Linux/macOS) |
| --- | --- | --- |
| `sub\file.txt` | `sub\file.txt` | `subfile.txt` |
| `<<kotorDirectory>>\Override` | `<game>\Override` | `<game>Override` |
| `a\ b` | `a\`, `b` | `a b` |
| `'a b'` | `'a`, `b'` | `a b` |
| `x #y z` | `x`, `#y`, `z` | `x` (the rest is a comment) |
| `"a""b"` | `a"b` | `ab` |
| `a` newline `b` | one argument `a\nb` | `a`, `b` |

**Status.** A visible difference: the same guide passed other arguments to its
program on Linux and macOS than on Windows and than C# anywhere. Windows-style paths
lost their separators, so an installer or tool given `<<kotorDirectory>>\Override`
received a path that does not exist, and `#` silently dropped the rest of the line.
Validation could not show it (both splitters accept these texts); only the program's
behaviour differed.

## Behaviour after this slice

- `expand` uses `windows_split` on every host (`src/engine/process_arguments.rs:10-11`);
  the `cfg` split is gone and `windows_split` (`:53`) is no longer Windows-only. The
  rules above now hold everywhere. Roots are still substituted after splitting, so a
  root with spaces stays one argument (deliberate, below).
- An unclosed double quote is still refused before any program runs (`Unclosed quote
  in Arguments`), as before on every host.

Callers checked: `expand` has two callers, the pre-install check and
`Engine::process`; both now split the same way, so a guide that passes the check
cannot fail a later re-parse mid-install. `windows_split` is also used by the Windows
standalone HoloPatcher (`src/patcher/standalone/windows.rs:313`), unchanged. Patcher
`Arguments` (an INI name or namespace index) never went through `expand`. The Unix
standalone HoloPatcher's `--args` text (`src/patcher/standalone.rs:209`) is a separate
command-line tool, not a guide step, and keeps `shlex`.

## Edge cases compared with the live reference

Fixture: `game/{chitin.key, Override/o.txt}` and `mods/args.sh`, which writes each
argument it receives, as `<arg>` on its own line, to `mods/args.txt`. The guide has one
Execute of `<<modDirectory>>/args.sh` with the case's Arguments. Each case is checked
five ways, all with `--allow-processes`: the validation pipeline (overall decision,
dry-run decision, `DryRunSkipped`), plain `validate`, `install`, `install
--component-checkpoints` and `install --component-checkpoints --managed` (the last four
against the reference install, with the final tree, the fixture folder written as
`<ROOT>`). "Before" is `0cc50a7`; it failed the three installing checks of each case
marked "differed".

| Case | Arguments | Result (both after this slice) | Before |
| --- | --- | --- | --- |
| `plain` | `a b` | `a`, `b` | same |
| `no_arguments` | empty | no arguments | same |
| `double_quoted` | `"a b" c` | `a b`, `c` | same |
| `empty_quoted` | `"" x` | empty, `x` | same |
| `tab_separated` | `a` tab `b` | `a`, `b` | same |
| `escaped_double_quote` | `a\"b` | `a"b` | same |
| `doubled_quote_in_quotes` | `"a""b"` | `a"b` | differed (`ab`) |
| `trailing_backslash_in_quotes` | `"C:\dir\\" x` | `C:\dir\`, `x` | same |
| `dollar_literal` | `` $HOME `x` `` | `$HOME`, `` `x` `` | same |
| `single_quoted` | `'a b'` | `'a`, `b'` | differed |
| `backslash_path` | `sub\file.txt` | `sub\file.txt` | differed |
| `backslash_space` | `a\ b` | `a\`, `b` | differed |
| `hash_word` | `x #y z` | `x`, `#y`, `z` | differed |
| `newline` | `a` newline `b` | one argument | differed |
| `placeholder_backslash` | `<<kotorDirectory>>\Override` | `<ROOT>/game\Override` | differed |
| `placeholder_forward` | `<<kotorDirectory>>/Override <<modDirectory>>` | two root paths | same |
| `windows_flag_value` | `--dir="C:\Program Files\x" -q` | `--dir=C:\Program Files\x`, `-q` | same |
| `unclosed_quote`, `quoted_placeholder_backslash` | `"a b`, `"<<kotorDirectory>>\Override\"` | known difference | same |
| `unknown_placeholder`, `lowercase_placeholder`, `root_with_space` | see below | known difference | same |

## Known differences

- **Unclosed quote (deliberate, fail-closed).** C# closes an unclosed double quote at
  the end of the text and runs the program (`"a b` gives `a b`); Rust refuses the step
  before any program runs, in the pipeline, `validate` and `install`. This includes the
  common Windows mistake `"<<kotorDirectory>>\Override\"`, where `\"` is an escaped
  quote: C# passes `<game>\Override"` with a trailing quote character. Probed
  (`unclosed_quote`, `quoted_placeholder_backslash`; five checks each).
- **Unknown placeholder (deliberate).** C# passes `<<foo>>` through as text; Rust
  refuses it (`Unknown Arguments variable <<foo>>`). Probed (five checks).
- **Placeholder case (unchanged).** C# replaces placeholders case-sensitively, so
  `<<kotordirectory>>` reaches the program as text; Rust matches the three names
  case-insensitively and passes the game folder. Probed (three installing checks; plain
  `validate` matches).
- **Root with a space (deliberate).** C# substitutes roots before splitting, so a game
  folder `.../game dir` given as `<<kotorDirectory>>` arrives as two arguments; Rust
  splits first and passes one. Probed (three installing checks; plain `validate`
  matches).
- **Pipeline.** The C# dry run does not inspect Arguments; Rust's validation pipeline
  splits them, so it reports the refusals above as dry-run errors. No other
  difference.
- **Standalone HoloPatcher `--args` on Unix (not a guide step, unchanged).** It keeps
  POSIX shell splitting; see [standalone-holopatcher.md](standalone-holopatcher.md).

## Results

| Binary | Execute arguments proof (110 checks) |
| --- | --- |
| this slice, live reference (`--csharp ... --record`) | 89 match / 21 known difference / 0 fail |
| this slice, recorded reference | 89 match / 21 known difference / 0 fail |
| `0cc50a7` (pre-fix), recorded reference | 68 match / 21 known difference / 21 fail |

The 21 failures on `0cc50a7` are the three installing checks (`install`,
`--component-checkpoints`, `--managed`) of the seven cases marked "differed". Every
regression probe stays at its baseline (see the proof README).

## Guide-author note

Write Arguments as you would on a Windows command line; they are split the same way on
every host. Separate arguments with spaces, group with double quotes, and write
backslashes as they are (`<<kotorDirectory>>\Override` keeps its `\`). Single quotes
and `#` are ordinary characters. Do not end a quoted argument with a single backslash
(`"...\Override\"`): `\"` is an escaped quote, so the quote never closes and the step
is refused. Drop that backslash or double it (`"...\Override\\"`). Write the
placeholders exactly as `<<modDirectory>>`, `<<kotorDirectory>>` or
`<<gameDirectory>>`.
