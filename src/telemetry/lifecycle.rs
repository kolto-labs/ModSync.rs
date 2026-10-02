//! Serialized desktop telemetry configuration and shutdown. The UI polls receipts;
//! configuration, log initialization, exporter flush and join run on the owner thread.
use super::{Config, Export, global};
use anyhow::{Context, Result, ensure};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub requested: u64,
    pub completed: u64,
    pub running: bool,
    pub stopping: bool,
    pub stopped: bool,
    pub export: Option<Export>,
    pub error: Option<String>,
    /// Configuration outcome separately from persistent-log initialization.
    pub configuration_error: Option<String>,
}

struct State {
    pending: Option<(u64, Config)>,
    shutdown: Option<(u64, bool, bool)>,
    status: Status,
    logs: VecDeque<String>,
    log_bytes: usize,
}

const MAX_LOG_LINES: usize = 512;
const MAX_LOG_BYTES: usize = 1024 * 1024;

/// One controller per application. A later configuration replaces a pending one;
/// shutdown takes priority and permanently rejects further configuration.
pub struct Lifecycle {
    shared: Arc<(Mutex<State>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl Lifecycle {
    pub fn start(
        directory: PathBuf,
        logging: crate::logging::Options,
        config: Config,
        total: usize,
        selected: usize,
    ) -> Result<Self> {
        let shared = Arc::new((
            Mutex::new(State {
                pending: Some((1, config)),
                shutdown: None,
                logs: VecDeque::new(),
                log_bytes: 0,
                status: Status {
                    requested: 1,
                    ..Default::default()
                },
            }),
            Condvar::new(),
        ));
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("modsync-telemetry-owner".into())
            .spawn(move || run(worker_shared, directory, logging, total, selected))
            .context("Start telemetry lifecycle worker")?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub fn configure(&self, config: Config) -> Result<u64> {
        let (mutex, wake) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
        ensure!(
            !state.status.stopping && !state.status.stopped,
            "Telemetry is shutting down"
        );
        state.status.requested = state
            .status
            .requested
            .checked_add(1)
            .context("Telemetry request identity exhausted")?;
        let ticket = state.status.requested;
        // Cancellation is immediate best effort. The matching completed receipt
        // establishes that the old exporter and its connections have been joined.
        if !config.exporter_ready() {
            global().request_stop();
        }
        state.pending = Some((ticket, config));
        wake.notify_one();
        Ok(ticket)
    }

    pub fn status(&self) -> Status {
        self.shared
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .status
            .clone()
    }

    /// Admit a whole batch without disk I/O or waiting for the controller lock.
    /// Rejection retains ownership of log history in the UI; accepted lines are
    /// drained before the stopped receipt. Individual lines are never truncated.
    pub fn log_lines(&self, lines: Vec<String>) -> Result<()> {
        let bytes = lines
            .iter()
            .try_fold(0_usize, |sum, line| sum.checked_add(line.len()))
            .context("Log batch size overflow")?;
        ensure!(
            lines.len() <= MAX_LOG_LINES && bytes <= MAX_LOG_BYTES,
            "Log batch exceeds queue bound"
        );
        let (mutex, wake) = &*self.shared;
        let mut state = mutex
            .try_lock()
            .map_err(|_| anyhow::anyhow!("Log owner is busy"))?;
        ensure!(
            !state.status.stopping && !state.status.stopped,
            "Log owner is stopping"
        );
        ensure!(
            state.logs.len() + lines.len() <= MAX_LOG_LINES
                && state.log_bytes + bytes <= MAX_LOG_BYTES,
            "Log queue is full"
        );
        state.log_bytes += bytes;
        state.logs.extend(lines);
        wake.notify_one();
        Ok(())
    }

    pub fn shutdown(&self, completed: bool, flush: bool) -> Result<u64> {
        let (mutex, wake) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
        if state.status.stopping || state.status.stopped {
            return Ok(state.status.requested);
        }
        state.status.requested = state
            .status
            .requested
            .checked_add(1)
            .context("Telemetry request identity exhausted")?;
        let ticket = state.status.requested;
        state.status.stopping = true;
        state.pending = None;
        state.shutdown = Some((ticket, completed, flush));
        if !flush {
            global().request_stop();
        }
        wake.notify_one();
        Ok(ticket)
    }
}

impl Drop for Lifecycle {
    fn drop(&mut self) {
        // Normal GUI close requests shutdown and polls stopped before dropping.
        // An abnormal owner drop still joins; it never detaches a sending thread.
        let _ = self.shutdown(false, false);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(
    shared: Arc<(Mutex<State>, Condvar)>,
    directory: PathBuf,
    logging: crate::logging::Options,
    total: usize,
    selected: usize,
) {
    let started = Instant::now();
    let log_error = crate::logging::init(&directory, logging)
        .err()
        .map(|e| e.to_string());
    crate::logging::install_panic_hook();
    let mut session_started = false;
    loop {
        let (mutex, wake) = &*shared;
        let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
        while state.pending.is_none() && state.shutdown.is_none() && state.logs.is_empty() {
            state = wake.wait(state).unwrap_or_else(|p| p.into_inner());
        }
        if let Some((ticket, completed, flush)) = state.shutdown.take() {
            let logs = std::mem::take(&mut state.logs);
            state.log_bytes = 0;
            drop(state);
            for line in logs {
                crate::logging::info(line);
            }
            if session_started && flush {
                global().record_session_end(started.elapsed(), completed);
            }
            let result = global().shutdown(flush, Duration::from_secs(2));
            let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
            state.status.completed = ticket;
            state.status.running = false;
            state.status.stopped = true;
            state.status.export = Some(result);
            state.status.error = log_error;
            state.status.configuration_error = None;
            return;
        }
        if state.pending.is_none() {
            let count = state.logs.len().min(64);
            let logs: Vec<_> = state.logs.drain(..count).collect();
            state.log_bytes -= logs.iter().map(String::len).sum::<usize>();
            drop(state);
            for line in logs {
                crate::logging::info(line);
            }
            continue;
        }
        let (ticket, config) = state.pending.take().expect("pending configuration");
        drop(state);
        let running = global().configure(&config);
        let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
        // A pending revocation can arrive while configure is joining or starting.
        // Cancel the resulting worker before exposing the stale receipt.
        let revoke_pending = state
            .pending
            .as_ref()
            .is_some_and(|(_, config)| !config.exporter_ready())
            || state.shutdown.is_some_and(|(_, _, flush)| !flush);
        if revoke_pending {
            global().request_stop();
        }
        if running && !revoke_pending && !session_started {
            global().record_session_start(total, selected);
            session_started = true;
        }
        state.status.completed = ticket;
        state.status.running = running && !revoke_pending;
        state.status.configuration_error = {
            (config.exporter_ready() && !running).then(|| "Telemetry exporter did not start".into())
        };
        state.status.error = log_error
            .clone()
            .or_else(|| state.status.configuration_error.clone());
    }
}
