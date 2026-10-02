# Desktop preferences and telemetry lifecycle

This source slice connects the typed shared preferences and telemetry lifecycle to the native desktop. The coordinator is `src/ui/desktop_settings.rs`; shared settings and exporter behavior remain in their library modules.

## Configuration and consent

- Startup reads typed preferences on an owned background operation, preserving the distinction between an implicit default settings directory and an explicitly selected directory. Explicit launch game/mod directories take precedence over saved defaults. The existing constructor key lookup and test-facing preference helpers remain compatible.
- Theme, spoiler presentation, patcher selection, managed-profile defaults, debug logging, and physical-monitor download filtering have desktop consumers. Managed deployment explicitly enables component checkpoints; otherwise whole-run recovery remains the default.
- Unknown monitor dimensions leave the resolution filter unset and produce a visible notice. Dimensions use native pixels per point, independent of UI zoom.
- Guide files use UTF-8. The legacy encoding field is retained. Rust never automatically elevates privileges and Move already copies then deletes. Parallel I/O and automatic-fix preferences are preserved without claiming desktop consumers. Registered-resource observation is described below. FOMOD continues using its verified prompt flow.
- General preferences use the shared writer that preserves the current stored Nexus key under its settings lock. The separate key action validates through the cancellable shared API before saving. Probe launches do not load or store ordinary credentials through this flow.

Consent controls are unavailable until the initial read settles. Every consent or endpoint change immediately gates UI events and requests cancellation of the existing exporter. Persistence runs away from repaint. Only the latest successfully saved configuration can be submitted to the owner; the UI considers export ready only when `exporter_ready()` and the matching lifecycle receipt agree. Rapid edits replace queued configurations. A settings-directory change stops the previous owner before constructing the replacement.

The desktop uses the shared OTLP recorder and exporter, including collection-category filters and signed transport. There is no detached legacy exporter per event. Endpoint diagnostics use the shared redacted display. Invalid endpoints, missing signing material, and lifecycle failures remain visible rather than being labelled ready.

## Ownership and shutdown

Settings reads, saves, key validation, and diagnostics each have a retained result receiver. Applying startup or saved preferences waits for the sharing mutation barrier. Held results retry that barrier after sharing startup; explicit sharing User/Settings pauses remain preserved by the conditional resume API.

Normal close cancels key validation and ongoing application work, waits for settings publication and sharing cleanup, requests telemetry shutdown, and waits for the stopped receipt before allowing the window to close. An already published save is reported as such; cancellation does not claim to undo it. A stale settings-directory result never changes the new directory's preferences.

The privacy window exposes collection options, consent, the exporter receipt and error, a human-readable collection summary, diagnostics, and the persistent log-folder action. Diagnostics explicitly warn that local paths and recent logs need review before sharing.

## Platform routing

Primary-instance selection uses the packaged channel identity so stable and beta instances have separate handoff endpoints. On macOS, the native Apple Event receiver is retained through the window lifetime and feeds the existing parsed protocol queue. The handler remains a source implementation: Linux production gates do not establish macOS compilation or native URL delivery.

## Evidence boundary

This slice was implemented and statically reviewed without tests, fixtures, runtime probes, native desktop execution, network delivery, or release acceptance. Production build and lint gates belong to the parent integration owner. The existing frozen inline test regions and test-facing helpers remain unchanged.

## Registered resource observation

The later F17 adapter retains the shared bounded metadata watcher for the current component's registered resource filenames. It starts only after typed settings load, changes generation with component/root/file-list changes, and joins a stopped worker before replacing it. Disabling observation or closing requests stop and waits for the finished handle; application shutdown drains resulting logs afterward. Oversized lists and observer errors remain visible.

Current-generation changes invalidate validation and update the input stamp so an older in-flight validation result cannot re-authorize installation. Own mutation workers suspend observation because their expected writes already invalidate readiness at dispatch. Observation reads metadata only, uses exact registered relative spelling, and does not establish content integrity or automatically run validation. The accepted Rust preference now has an explicit consumer even though the pinned reference did not consistently consult its stored flag.

The case-insensitive path preference is captured in the engine factory and input stamp. Unsupported exact-case managed/checkpoint/platform combinations fail through the shared engine policy rather than being silently accepted.
