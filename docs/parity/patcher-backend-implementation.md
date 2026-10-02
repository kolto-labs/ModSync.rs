# Packaged patcher discovery and Python invocation

## Source implementation

`src/patcher.rs` resolves a typed `Invocation::Native` or `Invocation::Python`.
The existing `resolve` signature remains a native-only compatibility wrapper;
engine execution uses `resolve_invocation`. An explicit configured executable
wins and an invalid configured path fails with its own diagnostic.

KPatcher and OdyPatcher search nonempty PATH directories in order, then the
application and resource directories, retaining the reference filename variants.
HoloPatcher searches resource directories for its platform binary (including the
non-Windows nested app executable) before considering any Python distribution.
Regular executable files are required, including Unix execute permissions.

Application resources normally live beside the application under `Resources`.
For an application inside `.app/Contents/MacOS`, discovery first preserves the
reference `.app/Resources` convention, additionally supports conventional
`.app/Contents/Resources`, and finally checks the application-directory resources.
All native candidates are considered before Python candidates.

## Installed Python resource contract

The Rust fallback requires these ordinary source files under one resource root:

- `PyKotor/Tools/HoloPatcher/src/holopatcher/__main__.py`
- `PyKotor/Tools/HoloPatcher/src/holopatcher/__init__.py`
- `PyKotor/Libraries/PyKotor/src/pykotor/__init__.py`
- `PyKotor/Libraries/Utility/src/utility/__init__.py`

An absent distribution means no fallback; an incomplete distribution produces an
actionable error. The interpreter search is deterministic: `Resources/Python/bin`,
then `Resources/Python`, then nonempty PATH directories. Windows searches
`python.exe`; other platforms search `python3`, then `python`. Interpreter symlinks
retain their invoked absolute path so virtual environments retain their own
`pyvenv.cfg` and installed packages.

The actual child receives `-I -B -c` and fixed bootstrap source. Every path and
patcher argument is a separate argv item, including paths with spaces. The
bootstrap requires Python 3.11 or newer, adds only the three explicit source
roots, checks installed `loggerplus`, `ply`, `pykotor`, and `utility` imports, then
runs the HoloPatcher entrypoint as `__main__`. Missing dependencies fail before
HoloPatcher execution. There is no pip invocation, package installation, runtime
download, shell argument interpolation, or interpreter probe during validation.
An incompatible first discovered interpreter fails clearly at actual execution;
discovery does not run arbitrary candidate interpreters to choose another one.

Native/Python preference uses the existing engine selection. No new public
preference setting is introduced: the reference's optional Python-first argument
has no observed product caller enabling it.

## Engine integration and failure boundaries

Pure validation and non-patcher installation do not require implicit bundled
Python resources to be complete. Implicit discovery errors are retained until an
applicable Patcher instruction reaches installation preflight. Preflight checks
the typed invocation before the recovery snapshot without spawning a process.
Installed dependency imports and interpreter version are checked by the actual
child; source discovery alone does not establish runnable backend availability.

The existing install argument construction, named-INI staging, working directory,
owned process lifetime, cancellation, simulation and fresh-log verification remain
the same for both invocation types. Zero exit without a fresh accepted install
log still fails. Python startup/import failures cannot become successful patches.

## Reference evidence and remaining acceptance

Source inspected at pinned C# commit `d31ccbd3`:

- `src/ModSync.Core/Services/InstallationService.cs`: native search, Python source
  lookup, native-first preference, Python.Included initialization, explicit
  `loggerplus`/`ply` imports, and execution of `holopatcher/__main__.py`.
- `src/ModSync.Core/Utility/Utility.cs`: resource-directory convention.
- `src/ModSync.Core/ModSync.Core.csproj`: Python.Included package dependency.
- `src/ModSync.GUI/ModSync.csproj`: copying `vendor/PyKotor/**/*.py` into resources.

The pinned checkout lacks the referenced `vendor/PyKotor` source distribution.
The library source roots above are the explicit Rust packaging contract, not a
claim that every reference package asset was available for inspection. The C#
implementation can initialize/install Python dependencies; Rust deliberately
requires an already installed interpreter and dependencies. No complete Python
bundle is added or certified by this source slice.

No tests, runtime probes, custom harnesses, package executions or Cargo commands
were run for this slice. Source formatting and diff review are the current
checks. Controlled production discovery fixtures, actual Python-only package
execution, dependency completeness, supported Windows/macOS/Linux bundles, and
real backend patch semantics remain acceptance work. E6 package delivery and E7
runtime parity are not established by this implementation.
