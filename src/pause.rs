//! Pausing a running download, check or install.
//!
//! A pause flag is shared like the cancel flag (`Arc<AtomicBool>`). Work that
//! sees it set holds at its next safe boundary: between download reads or
//! downloads, and between components during checks and installs, never in the
//! middle of a component. Clearing the flag continues from that boundary in the
//! same process. Cancellation always wins: a held run stops waiting as soon as
//! it is cancelled, and then stops as cancellation already does, so recovery
//! and restore apply unchanged.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const POLL: Duration = Duration::from_millis(100);

/// True while `paused` is set and the run is not cancelled.
pub fn is_holding(paused: &AtomicBool, cancelled: &AtomicBool) -> bool {
    paused.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire)
}

/// Wait while `paused` is set, until it is cleared or `cancelled` is set.
/// Returns true when it waited.
pub fn hold(paused: &AtomicBool, cancelled: &AtomicBool) -> bool {
    let mut waited = false;
    while is_holding(paused, cancelled) {
        waited = true;
        std::thread::sleep(POLL);
    }
    waited
}

/// A pause flag that follows whether `path` exists, checked a few times a
/// second until every other holder of the flag is gone. For command-line
/// callers: create the file to pause and delete it to resume.
pub fn follow_file(path: PathBuf) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(path.exists()));
    let watched = Arc::clone(&flag);
    std::thread::spawn(move || {
        while Arc::strong_count(&watched) > 1 {
            watched.store(path.exists(), Ordering::Release);
            std::thread::sleep(Duration::from_millis(250));
        }
    });
    flag
}
