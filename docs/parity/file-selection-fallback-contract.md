# File selection when the file selects nothing

This contract covers which components `validate` and `install` run with
`--use-file-selection` when the instruction file marks none of them `IsSelected`. The
reference is pinned C# `d31ccbd3`; C# paths are under `src/ModSync.Core/` there.
Evidence: [file-selection-fallback-proof](file-selection-fallback-proof/README.md).
F04 and F08 stay partial.

## Why this gap

The missing-destination comparison found it
([missing-destination-contract.md](missing-destination-contract.md), Known
differences). A guide saved with every component unselected, or written without any
`IsSelected` keys, is common: `IsSelected` defaults to false. With
`--use-file-selection`, which the C# help offers to match the GUI after Mod Selection,
C# validates and installs every component of such a guide. Rust refused it with `No
components are marked IsSelected in the instruction file.` (validate, exit 1) or
`FOMOD configuration check failed: Select at least one component` (install, exit 1).
That refusal was a reading of the C# CLI code, not a safety choice: validation is
read-only, and installing every component is what both programs already do without the
flag. Rust's own `selection` verb already applied the C# fallback.

The other open item from that comparison, a component with no instructions, was left
alone: Rust refuses to install it on purpose (COMPATIBILITY.md, "Components with no
executable instructions cannot be installed").

## Triage at `a0df4b7`

**C# behaviour.** `validate` and `install` load the guide with
`FileLoadingService.LoadFromFileAsync` (`CLI/ModBuildConverter.cs:3089`, `:3417`).

- For a Markdown extension (`md`, `markdown`, `mdown`, `mkdn`, `mkd`, `mdtxt`, `mdtext`,
  `text`; `FileLoadingService.cs:107-139`) it runs guide ingestion
  (`FileLoadingService.cs:60-70`), which keeps the authored `IsSelected` flags.
- Any other file goes through
  `ModComponentSerializationService.DeserializeModComponentFromString`
  (`FileLoadingService.cs:72`). After parsing (any format, or autodetection), it
  selects every top-level component when none is selected
  (`ModComponentSerializationService.cs:895-903`, logged as `All components are
  unselected, selecting all components automatically`). Options are not touched.

Then `validate --use-file-selection` keeps the components marked selected
(`ModBuildConverter.cs:3175-3194`) and fails with `No components are marked IsSelected
in the instruction file.` only when there are none, which after the fallback happens
only for Markdown. `install --use-file-selection` uses the flags as loaded
(`:3431-3434`). Without the flag both select every component (`:3196-3203`); with
`--select` the filters reset the selection.

**Rust at `a0df4b7`.** Both validate routes load with
`Guide::load_for_install_preserving_selection`, which skips the fallback: the rootless
report (`src/main.rs:1062`) and the rooted route shared by `validate`, `install` and
`conflicts` (`src/main.rs:3124`). Without the flag or a profile the CLI then selects
every component (`:1065`, `:3128-3134`), so only `--use-file-selection` saw the
authored all-false flags. `require_validation_selection` (`:953-967`) then refused
validate, and install stopped at the FOMOD gate. The preserving loader was chosen in
the basic-validation slice ([basic-validation-without-roots.md](basic-validation-without-roots.md))
on the reading that C# keeps authored all-false flags; the live C# CLI does not.
The ordinary loader (the `default_selection` block in `parse_with_yaml_budget`,
`src/model.rs:335-339`) already applies the
fallback, for every format including Markdown.

## Change

- `Guide::load_like_file_loader` (`src/model.rs`) loads as the C# file loader does:
  the fallback for every file except a Markdown extension, which keeps its flags. It
  reuses `load_for_install_with_selection_default` and the existing extension mapping,
  which is the C# one, so Markdown drafting is unchanged.
- `load_validation_guide` (`src/main.rs`) uses it for the rootless report and for the
  rooted validate/install/conflicts route when no profile is given. With `--profile`
  or `--active-profile` it keeps the preserving loader, since the profile's
  selections replace the loaded ones (unchanged).

Callers checked: `validate_without_roots` and the rooted dispatch are the only users
of the new helper. `selection` (`Guide::load_for_install`), `fetch-downloads`, the
profile, merge, guide-edit and peer-sharing verbs and the desktop loader are not
changed. `--direct-markdown` cannot be combined with `--use-file-selection`
(clap `conflicts_with_all`), and without the flag every component is selected anyway,
so its runs are unaffected. Selection flags after loading (`--select`, `--option`,
game rules) are applied as before; when they leave nothing selected the existing
messages still apply.

## Proof

[probe.py](file-selection-fallback-proof/probe.py) runs 20 cases against the live C#
CLI and the production binary, 58 checks: `validate --output json --dry-run`
(decision, counts with `componentCount`, stages with the archive-stage messages),
the rootless report, and `install` (decision and the game folder's files).

| Binary | Result |
| --- | --- |
| this slice, live reference (`--reference ... --record`) | 58 match / 0 fail |
| this slice, recorded reference | 58 match / 0 fail |
| `a0df4b7` (pre-fix), recorded reference | 28 match / 30 fail |

On `a0df4b7` the 30 failures are the 11 gap cases: TOML (one or two components, flags
omitted, a selected or unselected option under an unselected component), JSON and
YAML, the rootless report, and install. Each refused instead of validating or
installing every component. The nine controls (some selected, no flag, Markdown) match
on both builds.

## Known differences

- **Desktop.** The desktop file loaders (`src/ui/main_window.rs:179`, `src/ui.rs:3046`)
  use `Guide::load_for_install`, which already applies the fallback, so an all-false
  non-Markdown guide is selected as in C#. They also apply it to Markdown, where C#
  keeps the authored flags. Not changed or compared here.
- **Rust-only selection flags.** On an all-unselected non-Markdown guide,
  `--use-file-selection` with `--option`, `--add-category`, `--select-through-tier`,
  `--select-visible-search` or `--deselect` now starts from every component selected,
  the same state as a run without `--use-file-selection`, instead of from none. C# has
  no such flags. `--select` still resets the selection first.
- **Rust-only verbs.** `selection` applies the fallback to Markdown too (it uses the
  ordinary loader); C# has no such verb. `fetch-downloads` keeps the authored flags
  for its execution clone, so `--use-file-selection` on an all-unselected guide fetches
  nothing; C#'s `install --download` would fetch every component. `conflicts` now
  follows validate. Not probed.
- **Profiles.** With a profile, an all-unselected selection still fails before any
  write (COMPATIBILITY.md, profile-backed validation). C#'s `--profile` is a managed
  deployment option, not a selection source, so there is nothing to compare.
- **Fenced Markdown.** A Markdown file holding a fenced TOML guide loads in Rust and
  gives `No components loaded from instruction file.` in C#. That is a format
  difference outside this slice; the probe's Markdown cases use HTML-comment blocks,
  which both load.
- **JSON shape.** Rust also accepts an object such as `{"thisMod": [...]}`; C# accepts
  only a top-level array. The probe uses an array.
- **Install arguments.** C# install runs with `-y --skip-validation` (it asks for
  confirmation, and its environment check needs a packaged HoloPatcher), Rust with
  `--no-checkpoint`. The probe compares the exit decision and the game files outside
  `.modsync`.
- **Platform.** The reference ran on Linux under `unshare -rn` with
  `DOTNET_ROLL_FORWARD=Major` (the build targets .NET 9; the host has .NET 10).
  Nothing here depends on the platform.
