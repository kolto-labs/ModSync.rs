# Desktop patcher preference binding

Desktop preference loading now obtains the typed desktop report and engine-specific patcher preferences from the same raw settings snapshot. The shared decoder is also used by `Store::patcher_preferences`: HoloPatcher uses `holopatcherExecutablePath`, KPatcher uses `kpatcherExecutablePath`, and OdyPatcher uses `odyPatcherExecutablePath`.

Any invalid typed desktop field, unsupported patcher engine, or invalid type in a recognized patcher path key resets both loaded projections to defaults for the session. Recognizing the HoloPatcher and OdyPatcher path extensions for this validation is a deliberate Rust contract; this does not claim an unchanged C# settings schema.

The additive paired save API returns the exact decoded settings and patcher preferences it persisted under the settings lock. It validates the merged object before writing, preserves unrelated keys and engine-specific path extensions, and preserves a separately validated Nexus key. Existing save API signatures remain available. The desktop applies the selected engine and its matching executable from the paired receipt, and labels the general preferences field specifically “KPatcher executable”. Dedicated patcher controls continue to own other engine path edits.

Source-only implementation: targeted formatting and diff checks performed. No tests, builds, runtime, or native UI acceptance performed by this worker.
