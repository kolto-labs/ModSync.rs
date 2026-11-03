# Desktop patcher preference binding

Desktop preference loading now obtains the typed desktop report and engine-specific patcher preferences from the same raw settings snapshot. The shared decoder is also used by `Store::patcher_preferences`: HoloPatcher uses `holopatcherExecutablePath`, KPatcher uses `kpatcherExecutablePath`, and OdyPatcher uses `odyPatcherExecutablePath`.

Any invalid typed desktop field, unsupported patcher engine, or invalid type in a recognized patcher path key resets both loaded projections to defaults for the session. Recognizing the HoloPatcher and OdyPatcher path extensions for this validation is a deliberate Rust contract; this does not claim an unchanged C# settings schema.

The additive paired save API returns the exact decoded settings and patcher preferences it persisted under the settings lock. It validates the merged object before writing, preserves unrelated keys and engine-specific path extensions, and preserves a separately validated Nexus key. Existing save API signatures remain available. The desktop applies the selected engine and its matching executable from the paired receipt, and labels the general preferences field specifically “KPatcher executable”. Dedicated patcher controls continue to own other engine path edits.

Original implementation validation: targeted formatting and diff checks performed. No tests, builds, runtime, or native UI acceptance performed by this worker.

## Default and dedicated save correction (2026-11-03)

`DesktopSettings::default` now names OdyPatcher and the invalid-settings paired
fallback uses `PatcherEngine::default()`. Valid explicit choices are unchanged.
The dedicated “Save patcher setting” action uses an owned settings worker and
`Store::save_patcher_preferences_with_desktop`, which merges only `patcherEngine`
and the selected backend's own executable key, then validates and returns both
projections while still holding the settings lock. It retains other backend paths,
unrelated settings, and implicit legacy provenance. Blank paths remove only the
selected override. Malformed input is refused before persistence. The general
preferences field remains specifically KPatcher; this change does not add typed
path fields or alter the pinned patcher dependency.

No Rust tests were added or run: AGENTS.md freezes those during parity work.
Native GUI interaction acceptance is still pending.

Verification for this correction passed with Rust 1.95.0: `cargo fmt --all --
--check`, locked production lib/`modsync` builds and strict Clippy (`-D warnings`),
each with and without default features and `-j2`. Ten actual CLI `settings
--action inspect --json` cases in isolated temporary directories passed: missing
file, empty object, all three explicit backends, case-insensitive engine key,
invalid JSON, invalid typed field, unsupported engine, and invalid backend path
type. Each inspection also preserved the input bytes (or file absence). These
checks cover default/fallback decoding, not native GUI save interaction.
