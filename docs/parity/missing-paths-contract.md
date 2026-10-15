# A missing guide, game folder or mod folder

This contract covers what `validate`, `install` and `conflicts` say when the guide
(`-i`), the game folder (`-g`) or the mod folder (`-s`) does not exist. The C# CLI
names it (`Input file not found: <path>`, `Game directory not found: <path>`, `Source
directory not found: <path>`); Rust said `No such file or directory (os error 2)` for
either folder and `Read <path>: No such file or directory (os error 2)` for the guide.
The reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Rust lines before this slice are at `b0483d5`. Evidence:
[missing-paths-proof](missing-paths-proof/README.md). Tracker item: F26 ("complete
verb/alias/option/preload/pipeline/stdout/stderr/schema/exit contract comparison");
F26 stays partial.

## Why this gap

A mistyped `-g` or `-s` is the most common mistake on these commands, and Rust's
message did not say which folder was wrong; with `--output json` a script reading the
report's `error` field got a different string. The exit codes were already the same
(1) and nothing was written on either side, so no decision changes; only the message
does. The fix refuses nothing new and loosens nothing.

Candidates looked at and skipped (all compared with the live C# CLI in this session):

- **Install effects of corpus-shaped steps.** Copy/Move with no `Overwrite` (both
  default to true), backslash and trailing-slash paths, `<<gameDirectory>>`, Move
  within the game folder, Copy from the game folder into the mod folder, Delete then
  Copy, `Move <<modDirectory>>/a/*`, a wildcard Copy into an existing subfolder, and an
  Extract into a folder that already holds files (default, `Overwrite = true`, a later
  Move with `Overwrite = false`, two components extracting the same archive): the
  installed files, decisions and report counts matched, except two cases already
  documented (Rust restores the snapshot after a failed step where C# leaves the partial
  install; Rust's dry run warns on a skipped existing file where C# only logs it,
  copy-move-self contract, "Dry-run messages").
- **`settings`.** `list`, `get`, `set` (string, number, JSON array, empty value),
  `--json`, a secret key and a missing key matched in output and exit code. An unknown
  `--action` exits 2 in Rust (usage error) and 1 in C#: Rust's usage errors exit 2 on
  every command, a documented choice (COMPATIBILITY.md, `--no-managed` and `--select`
  sections).
- **`convert` errors.** A missing input, an unknown `-f` and an output in a missing
  folder have the same exit codes; the wording differs and was left alone (`convert`
  round trips are covered elsewhere).

## Triage

**C# at `d31ccbd3`.** `validate` (`CLI/ModBuildConverter.cs:3014`) first checks the
guide with `File.Exists` and fails with `Input file not found: <path>`
(`:3030-3040`); with `--full`, `--dry-run` or `--dry-run-only` it then checks the game
folder and the mod folder with `Directory.Exists`, in that order, and fails with `Game
directory not found: <path>` or `Source directory not found: <path>` (`:3056-3078`).
With `--output json` the message is the report's `error` field
(`WriteValidateJsonError`, `:3008`), with `errorCount` and `componentCount` 0; in text
it is logged as `Error: <message>`. The path is printed as typed. `install`
(`:3279`) checks the guide (`:3318-3322`), the game folder (`:3324-3328`) and the mod
folder (`:3349-3353`) the same way. Exit code 1 in every case, nothing written. Plain
`validate` (no `--full`, `--dry-run`, `--dry-run-only`) does not look at `-s`/`-g` and
fails its FOMOD stage (`mod directory is not set or does not exist`). C# has no
`conflicts` verb.

**Rust at `b0483d5`.** The guide was read by `Guide::load_with_selection_default`
(`src/model.rs:169-174`), whose error is `Read <path>: <io error>`. The folders were
opened by `Roots::new` (`src/paths.rs:205-221`), which called `canonicalize()?` on the
mod folder, then the game folder, so a missing one gave the bare
`No such file or directory (os error 2)`; a file in place of a folder gave `Choose
existing mod and game directories`. Every caller of `Roots::new` showed that text: the
`validate`, `install` and `conflicts` dispatch (`src/main.rs:2975-3175`), the JSON
report's `error` field, `restore`, `completion-summary`, checkpoint and maintenance
commands, and the desktop app's Validate and Install steps (through
`engine::Settings::new`, `src/engine.rs:62-72`).

**User-visible difference** (fixture with `g.toml`, `mods`, `game`, a file `afile`):

```
Command                                                   C#                                   Rust before
validate -i g.toml -s mods -g nogame --output json --full error "Game directory not found: nogame"  error "No such file or directory (os error 2)"
validate -i g.toml -s nomods -g game --output json --full error "Source directory not found: nomods" same os error
validate -i nope.toml ... --output json --full            error "Input file not found: nope.toml"   error "Read nope.toml: No such file ..."
install -i g.toml -s nomods -g game                       Error: Source directory not found: nomods Error: No such file or directory (os error 2)
install -i g.toml -s afile -g game                        Error: Source directory not found: afile  Error: Choose existing mod and game directories
```

## Behaviour after this slice

- **The folders.** `Roots::new` (`src/paths.rs:206-217`) now checks the game folder,
  then the mod folder, before resolving them, and fails with `Game directory not found:
  <path>` or `Source directory not found: <path>`, the path as given (after the Aspyr
  `steamassets` check, which keeps a missing path as typed). A file in place of a
  folder gets the same message, as `Directory.Exists` is false for a file in C#.
- **The guide.** The `validate`/`install`/`conflicts` dispatch now checks the guide
  given with `-i` before anything else that reads it (`src/main.rs:3051-3055`, also
  for plain `validate`, whose rootless path loads the guide elsewhere) and again once
  a profile may have supplied it (`src/main.rs:3172-3176`), and fails with `Input file
  not found: <path>`; a folder given as the guide gets the same message.
- **Where it shows.** The text output (`Error: <message>`, exit 1), the JSON report's
  `error` field (`--output json`, `--validation-report`, `--json`), `conflicts`, and
  every other caller of `Roots::new` (`restore`, `completion-summary`, checkpoint and
  maintenance commands, the desktop Validate and Install steps). Exit codes and the
  order of checks (guide, game folder, mod folder) are as before and as in C#.
- **Unchanged.** `convert`, `merge`, `list` and the other guide readers keep `Read
  <path>: ...` (C#'s `merge` uses other words, `Existing file not found`); a folder
  that exists but cannot be resolved still gives the system's error.

## Proof

[missing-paths-proof](missing-paths-proof/README.md) runs 19 cases on both sides under
`unshare -Urnm` against the live C# CLI with a HoloPatcher stub, and two Rust-only
`conflicts` cases: the exit code, the message (the JSON `error` field or the first
`... not found: <path>` text) and whether `game` and `mods` are unchanged, 63 checks.

| Binary | missing-paths |
| --- | --- |
| this slice, live C# CLI | 62 match / 1 known difference / 0 fail |
| this slice, recorded references | 62 match / 1 known difference / 0 fail |
| `b0483d5` (pre-fix), recorded references | 44 match / 0 known difference / 19 fail |

On `b0483d5` every `message` check fails except the controls': the sixteen C# cases
with a missing path, the two `conflicts` cases, and the known `plain_mods_missing`
(Rust's message then was not a `not found` line, which equals C#'s none but is not the
documented Rust result). Exit codes and the unchanged trees match on both binaries.

## Known differences

- **Plain `validate` with a missing folder** (`plain_mods_missing/message`): C#'s plain
  `validate` ignores `-s`/`-g` and fails its FOMOD stage (`Cannot check FOMOD
  configuration: mod directory is not set or does not exist.`); Rust uses the folders
  (a documented extension, missing-archives contract) and now says `Source directory
  not found: nomods`. Both exit 1.
- **Not compared.** C#'s `install` defaults `-s` to the guide's folder; Rust does the
  same, so a missing default cannot occur. The desktop app's wording is not compared
  with C#'s wizard, which checks folders on its own pages.

## Notes for guide authors

Nothing changes for guides. Users: when `validate` or `install` stops with `Game
directory not found: <path>`, `Source directory not found: <path>` or `Input file not
found: <path>`, the named path does not exist (or is not a folder, or not a file);
fix the `-g`, `-s` or `-i` argument and run the command again. Nothing was written.
