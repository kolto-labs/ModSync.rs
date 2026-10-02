//! Owned acquisition deadline. Stopping wakes and joins its timer before return.
use anyhow::{Context, Result};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(super) struct Deadline {
    end: Instant,
    cancelled: Arc<AtomicBool>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    expired: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Deadline {
    pub(super) fn start(duration: Duration, cancelled: Arc<AtomicBool>) -> Result<Self> {
        let end = Instant::now()
            .checked_add(duration)
            .context("Acquisition timeout is too large")?;
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let expired = Arc::new(AtomicBool::new(false));
        let state = stop.clone();
        let timeout = expired.clone();
        let operation_cancelled = cancelled.clone();
        let worker = std::thread::Builder::new()
            .name("acquisition-deadline".into())
            .spawn(move || {
                let (lock, wake) = &*state;
                let mut stopped = lock.lock().unwrap_or_else(|error| error.into_inner());
                while !*stopped {
                    let remaining = end.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        timeout.store(true, Ordering::Release);
                        operation_cancelled.store(true, Ordering::Release);
                        break;
                    }
                    stopped = wake
                        .wait_timeout(stopped, remaining)
                        .unwrap_or_else(|error| error.into_inner())
                        .0;
                }
            })?;
        Ok(Self {
            end,
            cancelled,
            stop,
            expired,
            worker: Some(worker),
        })
    }
    pub(super) fn finish(mut self) -> bool {
        // Publication must enforce elapsed time even if the timer was descheduled.
        if Instant::now() >= self.end {
            self.expired.store(true, Ordering::Release);
            self.cancelled.store(true, Ordering::Release);
        }
        self.join();
        self.expired.load(Ordering::Acquire)
    }
    fn join(&mut self) {
        {
            let (lock, wake) = &*self.stop;
            *lock.lock().unwrap_or_else(|error| error.into_inner()) = true;
            wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        self.join();
    }
}
