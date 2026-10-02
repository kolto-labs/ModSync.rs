//! Bounded metadata observation for registered resources. Notifications invalidate
//! UI validation; they never validate instructions or mutate installation state.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, SystemTime},
};
const MAX_FILES: usize = 4096;
const MAX_PATH_BYTES: usize = 1024 * 1024;
const INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct WatchRequest {
    pub root: PathBuf,
    pub relative_files: Vec<PathBuf>,
    pub generation: u64,
}
#[derive(Clone, Debug)]
pub struct WatchEvent {
    pub generation: u64,
    /// Coalesced observations, not necessarily distinct resource identities.
    pub changes: usize,
    pub error: Option<String>,
}
struct Shared {
    stopped: AtomicBool,
    wake: (Mutex<()>, Condvar),
    event: Mutex<Option<WatchEvent>>,
}
pub struct Watcher {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl Watcher {
    /// Admission is lexical and bounded; filesystem work starts on the worker.
    pub fn start(mut request: WatchRequest) -> Result<Self> {
        ensure!(
            !request.relative_files.is_empty() && request.relative_files.len() <= MAX_FILES,
            "Resource watcher requires 1–4096 registered files"
        );
        ensure!(
            !request.root.as_os_str().is_empty(),
            "Resource watcher root is empty"
        );
        request.root = std::path::absolute(request.root)?;
        ensure!(
            !request
                .root
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir)),
            "Resource watcher root contains parent traversal"
        );
        let mut bytes = 0usize;
        let mut unique = BTreeSet::new();
        for path in request.relative_files {
            let text = path
                .to_str()
                .context("Watched resource path is not UTF-8")?;
            bytes = bytes
                .checked_add(text.len())
                .context("Resource watcher path size overflow")?;
            ensure!(
                bytes <= MAX_PATH_BYTES,
                "Resource watcher paths exceed 1 MiB"
            );
            let relative = crate::paths::relative(text, false)?;
            ensure!(
                !relative.as_os_str().is_empty(),
                "Watched resource path is empty"
            );
            ensure!(
                relative.components().count() <= 128,
                "Watched resource path exceeds depth bound"
            );
            unique.insert(relative);
        }
        request.relative_files = unique.into_iter().collect();
        let shared = Arc::new(Shared {
            stopped: AtomicBool::new(false),
            wake: (Mutex::new(()), Condvar::new()),
            event: Mutex::new(None),
        });
        let owned = shared.clone();
        let worker = std::thread::Builder::new()
            .name("modsync-resource-watcher".into())
            .spawn(move || run(request, owned))
            .context("Start resource watcher")?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    pub fn request_stop(&self) {
        // Pair cancellation and wake with the waiter's mutex to prevent a lost wake.
        let _guard = self.shared.wake.0.lock().unwrap_or_else(|p| p.into_inner());
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.wake.1.notify_all();
    }
    pub fn poll(&self) -> Option<WatchEvent> {
        self.shared.event.try_lock().ok()?.take()
    }
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
    /// UI callers wait for is_finished; this method refuses a blocking live join.
    pub fn join(&mut self) -> Result<()> {
        ensure!(self.is_finished(), "Resource watcher is still stopping");
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("Resource watcher panicked"))?;
        }
        Ok(())
    }
}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    readonly: bool,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}
fn ordinary(meta: &fs::Metadata, directory: bool) -> Result<()> {
    ensure!(
        !meta.file_type().is_symlink()
            && if directory {
                meta.is_dir()
            } else {
                meta.is_file()
            },
        "Watched resource path became a link or special file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                == 0,
            "Watched resource path is a reparse point"
        );
    }
    Ok(())
}
fn inventory(request: &WatchRequest, shared: &Shared) -> Result<Vec<Option<Stamp>>> {
    for ancestor in request.root.ancestors() {
        ensure!(
            !shared.stopped.load(Ordering::Acquire),
            "Resource watcher cancelled"
        );
        ordinary(&fs::symlink_metadata(ancestor)?, true)?;
    }
    let mut stamps = Vec::with_capacity(request.relative_files.len());
    for relative in &request.relative_files {
        ensure!(
            !shared.stopped.load(Ordering::Acquire),
            "Resource watcher cancelled"
        );
        let mut path = request.root.clone();
        let components: Vec<_> = relative.components().collect();
        let mut stamp = None;
        for (index, component) in components.iter().enumerate() {
            ensure!(
                !shared.stopped.load(Ordering::Acquire),
                "Resource watcher cancelled"
            );
            path.push(component.as_os_str());
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error).context("Observe registered resource metadata"),
            };
            let directory = index + 1 < components.len();
            ordinary(&metadata, directory)?;
            if !directory {
                #[cfg(unix)]
                use std::os::unix::fs::MetadataExt;
                stamp = Some(Stamp {
                    len: metadata.len(),
                    modified: metadata.modified().ok(),
                    readonly: metadata.permissions().readonly(),
                    #[cfg(unix)]
                    device: metadata.dev(),
                    #[cfg(unix)]
                    inode: metadata.ino(),
                });
            }
        }
        stamps.push(stamp);
    }
    Ok(stamps)
}
fn publish(shared: &Shared, event: WatchEvent) {
    let mut slot = shared.event.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(previous) = slot.as_mut() {
        previous.changes = previous.changes.saturating_add(event.changes);
        if event.error.is_some() {
            previous.error = event.error;
        }
    } else {
        *slot = Some(event);
    }
}
fn run(request: WatchRequest, shared: Arc<Shared>) {
    let result = (|| -> Result<()> {
        let mut previous = inventory(&request, &shared)?;
        loop {
            let guard = shared.wake.0.lock().unwrap_or_else(|p| p.into_inner());
            if shared.stopped.load(Ordering::Acquire) {
                return Ok(());
            }
            let (guard, _) = shared
                .wake
                .1
                .wait_timeout(guard, INTERVAL)
                .unwrap_or_else(|p| p.into_inner());
            drop(guard);
            if shared.stopped.load(Ordering::Acquire) {
                return Ok(());
            }
            let current = inventory(&request, &shared)?;
            let changes = previous
                .iter()
                .zip(&current)
                .filter(|(left, right)| left != right)
                .count();
            if changes > 0 {
                publish(
                    &shared,
                    WatchEvent {
                        generation: request.generation,
                        changes,
                        error: None,
                    },
                );
            }
            previous = current;
        }
    })();
    if let Err(error) = result
        && !shared.stopped.load(Ordering::Acquire)
    {
        publish(
            &shared,
            WatchEvent {
                generation: request.generation,
                changes: 0,
                error: Some(format!("{error:#}")),
            },
        );
    }
}
