# Resource observation and readiness invalidation

The desktop observes metadata for the current component's registered files. Observation is a hint to rerun validation, not proof of file integrity.

`src/ui/resource_watch.rs` now clears validation, conflict results and install confirmation whenever its requested observation context changes, including suspension and resumption. Starting a fresh observer also clears readiness: its initial baseline cannot prove that files remained unchanged while observation was unavailable.

Update checks and saved update-summary reads retain observation. The shared Editor job remains conservatively suspended because it also covers deployment and standalone patcher operations; even read-only editor or log-save operations therefore clear prior readiness when they suspend observation.

Transition invalidation does not change the captured input identity of an owned install or download. When validation or conflict analysis is running, however, a context change or fresh baseline increments the observation epoch so the old result cannot restore readiness. Matching resource-change/error events retain the same epoch behavior.

After joining an already-finished observer, the UI drains its final event slot. This preserves errors or changes published between the initial poll and the finished check. Events from an obsolete context cannot apply to the replacement context; transition invalidation already clears its prior readiness.

Verification for this change is source review, production-file formatting and whitespace checking only. No tests, builds, runtime probes or native UI checks were performed. Metadata-preserving edits and changes wholly between polls remain outside this observer's detection scope; actual installation still requires the shared preflight.
