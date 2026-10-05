# `install --no-managed`

This contract covers the C# CLI's `install --no-managed` option: `install -i
guide.toml -g game -s mods --no-managed`. The C# CLI installs classic (without
managed deployment) for that run, even when settings.json turns managed deployment on.
Rust had no such option, so the same command line stopped with a usage error and
installed nothing. The reference is pinned C# `d31ccbd3`; C# paths are under
`src/ModSync.Core/` there. Rust lines before this slice are at `bef5735`. Evidence:
[no-managed-proof](no-managed-proof/README.md). F26 (CLI) stays partial.

## Why this gap

It decides whether a command runs at all. The C# CLI reference
(`docs/knowledgebase/core-cli-reference.md:95` in the reference repository) and its
managed-deployment guide list `--no-managed` beside `--managed` and `--profile` as the
way to force a classic install for one run. A user or script that passes it got
`error: unexpected argument '--no-managed' found` (exit 2) from Rust, with no
validation and no install. Matching C# is not less safe: the flag only asks for the
install Rust already does, and every selected component is still validated before
any write.

## Triage

**C# at `d31ccbd3`.** `install` declares `[Option("no-managed")] bool NoManaged`
(`CLI/ModBuildConverter.cs:673-674`), beside `--managed` (`:670-671`) and `--profile`
(`:676-677`); `validate` has neither. After validation the install loads
settings.json (`ModSyncSettings.Load()`, `:3608`) and calls
`ManagedInstallCliOverrides.Apply` (`:3609-3618`,
`Services/Installation/ManagedInstallCliOverrides.cs:24-61`): both flags give `Cannot
combine --managed and --no-managed.` (`:35-38`), exit 1, nothing installed;
`--no-managed` sets `ManagedDeploymentEnabled = false` for this run (`:44-47`, not
saved). `ResolveManagedOverride` (`:68-86`) passes `false` to
`InstallationService.InstallAllSelectedComponentsAsync` (`CLI/ModBuildConverter.cs:3620,
3629-3639`), so the install is classic. Without either flag, settings.json's
`managedDeploymentEnabled` decides; when it is true the install needs the active
profile on disk (`Managed deployment requires profile 'p', but it was not found on
disk.`, seen live). `validate --no-managed` is an unknown option: help screen, exit 1.

**Rust at `bef5735`.** `InstallArgs` (`src/main.rs:665-803`), shared by `validate`,
`conflicts`, `install` and `checkpoint resume`, has `--managed` (`:708-710`), which
needs `--component-checkpoints` or a checkpoint resume (`:3103-3106`), but no
`--no-managed`, so clap stopped with a usage error (exit 2) before anything ran. The
CLI install never reads settings.json's `managedDeploymentEnabled` (only the desktop
does, `src/ui/desktop_settings.rs:173`): without `--managed` it always installs
classic.

**User-visible difference** (`install -i G -s mods -g game --no-checkpoint`; guide
`a`, `b`):

| Arguments | C# CLI | Rust before | Rust after |
| --- | --- | --- | --- |
| `--no-managed` | installs a, b | usage error, exit 2 | installs a, b |
| `--no-managed`, settings.json enables managed | installs a, b (classic) | usage error | installs a, b |
| `--managed --no-managed` | `Cannot combine ...`, exit 1 | usage error | `Cannot combine ...`, exit 1 |
| `validate ... --no-managed` | usage error, exit 1 | usage error, exit 2 | refused, exit 1 |

## Behaviour after this slice

- `install` (and `checkpoint resume`, which takes the install options) accepts
  `--no-managed` (`src/main.rs:711-713`) and installs as without `--managed`: classic,
  with the same validation, recovery snapshot and stop on failure.
- `--managed` with `--no-managed` stops before validation with C#'s message `Cannot
  combine --managed and --no-managed.` (`src/main.rs:3016-3019`), exit 1, nothing
  written.
- `validate` and `conflicts` refuse `--no-managed` with `--no-managed is only
  supported by install` (`src/main.rs:3020-3023`), exit 1, with or without roots: the
  check runs before the rootless branch.
- Unchanged: `--managed` alone, and the CLI's classic default when neither flag is
  given.

Callers checked: `InstallArgs` is built only by clap, for `validate`, `conflicts`,
`install` and the flattened `checkpoint resume` options (`src/checkpoint_cli.rs:19`,
`into_resume` at `:166-168`); no code constructs it by hand. The new field is read only
by the two checks above. The rootless validation list of unsupported options
(`validate_without_roots`, `src/main.rs:1037-1127`) is never reached with
`--no-managed`, because the dispatch checks run first.

## Proof

[no-managed-proof](no-managed-proof/README.md) runs 9 cases through the live C# CLI and
Rust (17 checks).

| Binary | Result |
| --- | --- |
| this slice, live C# CLI | 14 match / 3 known difference / 0 fail |
| this slice, recorded reference | 14 match / 3 known difference / 0 fail |
| `bef5735` (pre-fix), recorded reference | 7 match / 3 known difference / 7 fail |

On `bef5735` 7 checks fail: both checks of `no_managed`, `no_managed_first` and
`settings_no_managed`, which C# installs and Rust refused with a usage error, and the
message of `both_flags`. The other cases matched before: Rust refused them too, only
with a usage error, and wrote nothing.

## Known differences

- **settings.json's managed toggle.** Without `--no-managed`, C# follows
  `managedDeploymentEnabled` into a managed install (here it fails: the active profile
  `p` does not exist). The Rust CLI does not read that setting and installs classic, as
  before this slice; use `--managed` with `--component-checkpoints` for a Rust managed
  install (`settings_only`).
- **A selected component that fails.** The C# install checks a missing Copy Source
  only when it reaches that component, so it installs the earlier ones first; Rust
  refuses before any write, as for every validation failure (`no_managed_broken`).
- **When both flags are refused.** C# validates first and refuses before the install;
  Rust refuses before validation. Both exit 1 with the same message and write nothing.
  With `--no-checkpoint` as well, Rust's parser refuses `--managed` beside it first
  (usage error, exit 2), as before this slice; the probe leaves `--no-checkpoint` out
  of that case.
- **The message for `validate --no-managed`.** C# shows its help screen; Rust names
  the option. Both exit 1 and validate nothing.

## Notes for users

`install --no-managed` now works as in the C# CLI. The Rust CLI already installs
without managed deployment unless you pass `--managed`, so the flag changes nothing
else; it cannot be combined with `--managed`.
