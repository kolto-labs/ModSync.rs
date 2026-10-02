//! Current-component registered-resource observation. No automatic validation.
use super::{App, Job};
use modsync::resource_watcher::{WatchRequest, Watcher};
use std::path::PathBuf;
#[derive(Default)]
pub(super) struct State {
    owner: Option<Watcher>,
    signature: String,
    requested_context: String,
    attempted: String,
    generation: u64,
    pub epoch: u64,
    pub status: String,
}
impl State {
    pub fn closed(&self) -> bool {
        self.owner.is_none()
    }
}
impl App {
    pub(super) fn poll_resource_watch(&mut self) {
        // Suspend during possible root mutations, including the shared Editor job.
        // Read-only update jobs keep observing; pauses invalidate readiness below.
        let enabled = self.desktop.watch_enabled()
            && !self.sharing.closing
            && (self.receiver.is_none()
                || matches!(
                    self.job,
                    Job::Validate | Job::Conflicts | Job::UpdateSummary | Job::Update
                ))
            && self.sharing_continuation.is_none();
        let mut overflow = false;
        let request = enabled
            .then(|| {
                self.guide
                    .as_ref()
                    .and_then(|guide| guide.components.get(self.selected))
            })
            .flatten()
            .and_then(|component| {
                let mut files = std::collections::BTreeSet::new();
                let mut bytes = 0usize;
                for resource in component.resource_registry.values() {
                    if let Some(entries) = resource
                        .get("Files")
                        .or_else(|| resource.get("files"))
                        .and_then(serde_json::Value::as_object)
                    {
                        for file in entries.keys() {
                            bytes = bytes.saturating_add(file.len());
                            if files.len() >= 4096 || bytes > 1024 * 1024 {
                                overflow = true;
                                return None;
                            }
                            files.insert(PathBuf::from(file));
                        }
                    }
                }
                if files.is_empty() || self.mods.trim().is_empty() {
                    None
                } else {
                    Some((
                        format!("{}\n{}\n{:?}", self.mods, component.guid, files),
                        files.into_iter().collect::<Vec<_>>(),
                    ))
                }
            });
        if overflow {
            self.resource_watch.status = "Observation unavailable: registered resource list exceeds 4096 files or 1 MiB of paths.".into();
        }
        let desired = request
            .as_ref()
            .map(|(signature, _)| signature.as_str())
            .unwrap_or("");
        let mut invalidate = self.resource_watch.requested_context != desired;
        self.resource_watch.requested_context = desired.to_owned();
        let mut observed_change = false;
        let mut events = Vec::with_capacity(2);
        if let Some(owner) = &self.resource_watch.owner {
            if self.resource_watch.signature != desired {
                owner.request_stop();
                // Returning to this same context before stop completes must
                // still create a new observer after the old one is joined.
                self.resource_watch.attempted.clear();
            }
            events.extend(owner.poll());
            if owner.is_finished() {
                let mut owner = self.resource_watch.owner.take().expect("finished watcher");
                if let Err(error) = owner.join() {
                    self.logs
                        .push(format!("Resource observer failed: {error:#}"));
                    invalidate = true;
                    observed_change =
                        self.resource_watch.signature == desired && !desired.is_empty();
                }
                // A final event can be published between the first poll and the
                // finished check. Joining the finished worker closes that race.
                events.extend(owner.poll());
            }
        }
        for event in events {
            if event.generation != self.resource_watch.generation
                || self.resource_watch.signature != desired
                || desired.is_empty()
            {
                continue;
            }
            if event.changes > 0 {
                self.logs.push(format!(
                    "Registered resource metadata changed ({} observations); run validation again.",
                    event.changes
                ));
                observed_change = true;
            }
            if let Some(error) = event.error {
                self.resource_watch.status = format!("Observation stopped: {error}");
                self.logs
                    .push(format!("Resource observation stopped: {error}"));
                observed_change = true;
            }
        }
        if desired.is_empty() {
            self.resource_watch.attempted.clear();
        }
        if self.resource_watch.owner.is_none()
            && desired != self.resource_watch.attempted
            && let Some((signature, files)) = request
        {
            // A new baseline cannot establish that earlier validation survived
            // the interval while the previous observer was stopping or absent.
            invalidate = true;
            self.resource_watch.generation = self.resource_watch.generation.wrapping_add(1);
            self.resource_watch.attempted = signature.clone();
            match Watcher::start(WatchRequest {
                root: PathBuf::from(&self.mods),
                relative_files: files,
                generation: self.resource_watch.generation,
            }) {
                Ok(owner) => {
                    self.resource_watch.status = "Observing current registered resource metadata; content integrity still requires validation.".into();
                    self.resource_watch.signature = signature;
                    self.resource_watch.owner = Some(owner);
                }
                Err(error) => {
                    self.resource_watch.status =
                        format!("Resource observation unavailable: {error:#}");
                    self.logs.push(self.resource_watch.status.clone());
                }
            }
        }
        if invalidate || observed_change {
            // Do not alter the captured identity of an owned install/download.
            // In-flight validation/conflict reads must nevertheless be rejected
            // when their observation context changes or is newly baselined.
            if observed_change
                || (self.receiver.is_some() && matches!(self.job, Job::Validate | Job::Conflicts))
            {
                self.resource_watch.epoch = self.resource_watch.epoch.wrapping_add(1);
                self.refresh_input_stamp();
            }
            self.invalidate_validation();
        }
    }
}
