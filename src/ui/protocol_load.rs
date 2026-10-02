//! A protocol document belongs to its worker, cancellation token and UI inputs.
use super::{App, Job, Message, SharingContinuation};
use anyhow::{Result, ensure};
use modsync::Guide;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};

#[derive(Default)]
pub(super) struct State {
    run: u64,
    worker: Option<Worker>,
}

struct Worker {
    run: u64,
    context: String,
    cancelled: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    tail_failed: bool,
}

impl Worker {
    fn join(&mut self) -> bool {
        if let Some(handle) = self.handle.take() {
            self.tail_failed |= handle.join().is_err();
        }
        self.tail_failed
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.cancelled.store(true, Ordering::Release);
            self.join();
        }
    }
}

pub(super) struct Finished {
    run: u64,
    result: Result<Guide>,
}

#[derive(Clone)]
pub(super) struct Ready {
    run: u64,
    context: String,
    cancelled: Arc<AtomicBool>,
    guide: Guide,
}

impl State {
    pub(super) fn closed(&self) -> bool {
        self.worker.is_none()
    }

    fn start(
        &mut self,
        raw: String,
        context: String,
        cancelled: Arc<AtomicBool>,
        sender: mpsc::Sender<Message>,
    ) -> Result<()> {
        ensure!(self.closed(), "A protocol document is already loading");
        let run = self.run.checked_add(1).ok_or_else(|| {
            anyhow::anyhow!("Protocol load limit reached; restart the application")
        })?;
        let flag = cancelled.clone();
        let handle = std::thread::Builder::new()
            .name("protocol-guide-load".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    modsync::protocol::fetch_modsync_guide_with_cancel(&raw, &flag)
                        .map(|(_, guide)| guide)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Protocol worker stopped unexpectedly")));
                // This is the final operation. The UI joins before consuming it.
                let _ = sender.send(Message::ProtocolTerminal(Box::new(Finished {
                    run,
                    result,
                })));
            })?;
        self.run = run;
        self.worker = Some(Worker {
            run,
            context,
            cancelled,
            handle: Some(handle),
            tail_failed: false,
        });
        Ok(())
    }
}

impl App {
    pub(super) fn open_pending_protocol(&mut self) {
        if self.receiver.is_some()
            || !self.protocol_load.closed()
            || self.desktop.busy()
            || self.sharing.closing
            || self.sharing_continuation.is_some()
            || self.fomod_pending()
            || self.peer_view.saving
        {
            return;
        }
        let Some(raw) = self.pending_protocol.pop_front() else {
            return;
        };
        if modsync::single_instance::validate_protocol_request(&raw).is_err() {
            self.logs.push("Protocol link rejected.".into());
            return;
        }
        self.refresh_input_stamp();
        let (tx, rx) = mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        match self
            .protocol_load
            .start(raw, self.input_stamp.clone(), flag.clone(), tx)
        {
            Ok(()) => {
                self.cancelled = flag;
                self.receiver = Some(rx);
                self.job = Job::ProtocolOpen;
                self.logs.push("Downloading ModSync build link…".into());
            }
            Err(_) => self
                .logs
                .push("Could not start protocol guide worker.".into()),
        }
    }

    pub(super) fn protocol_terminal(&mut self, finished: Finished) {
        if self
            .protocol_load
            .worker
            .as_ref()
            .is_none_or(|worker| worker.run != finished.run)
        {
            return;
        }
        let mut worker = self
            .protocol_load
            .worker
            .take()
            .expect("Matching protocol worker");
        let tail_failed = worker.join();
        // A foreign terminal never releases another job's channel or token.
        if self.job != Job::ProtocolOpen {
            return;
        }
        self.receiver = None;
        self.refresh_input_stamp();
        if worker.cancelled.load(Ordering::Acquire)
            || !Arc::ptr_eq(&worker.cancelled, &self.cancelled)
            || self.sharing.closing
            || worker.context != self.input_stamp
        {
            self.logs
                .push("Protocol load cancelled or inputs changed; result discarded.".into());
            return;
        }
        if tail_failed {
            self.logs
                .push("Protocol worker stopped after reporting its result.".into());
        }
        match finished.result {
            Ok(guide) => self.apply_protocol_ready(Ready {
                run: worker.run,
                context: worker.context.clone(),
                cancelled: worker.cancelled.clone(),
                guide,
            }),
            // The URL may contain a credential in any path/query position.
            Err(_) => self.logs.push("Protocol guide could not be loaded.".into()),
        }
    }

    pub(super) fn poll_protocol_worker(&mut self) {
        if !self
            .protocol_load
            .worker
            .as_ref()
            .is_some_and(|worker| worker.handle.as_ref().is_some_and(JoinHandle::is_finished))
        {
            return;
        }
        self.protocol_load
            .worker
            .as_mut()
            .expect("Finished protocol worker")
            .join();
        // Joining makes this drain stable: Empty can now mean no terminal.
        if self.job == Job::ProtocolOpen {
            let pending: Vec<_> = self
                .receiver
                .as_ref()
                .map(|receiver| receiver.try_iter().collect())
                .unwrap_or_default();
            for message in pending {
                self.handle_message(message);
            }
        }
        if self.protocol_load.worker.take().is_some() && self.job == Job::ProtocolOpen {
            self.receiver = None;
            self.logs.push("Protocol worker returned no result.".into());
        }
    }

    pub(super) fn apply_protocol_ready(&mut self, ready: Ready) {
        self.refresh_input_stamp();
        if ready.run != self.protocol_load.run
            || ready.context != self.input_stamp
            || ready.cancelled.load(Ordering::Acquire)
            || !Arc::ptr_eq(&ready.cancelled, &self.cancelled)
            || self.sharing.closing
        {
            self.logs
                .push("Protocol load cancelled or inputs changed; result discarded.".into());
            return;
        }
        if !self.sharing_before(SharingContinuation::Protocol(Box::new(ready.clone()))) {
            return;
        }
        self.apply_protocol_guide(ready.guide);
    }

    /// Mutates the guide only after production callers accepted its bound result.
    pub(super) fn apply_protocol_guide(&mut self, guide: Guide) {
        self.reset_installation();
        self.logs.push(format!(
            "Loaded {} components from modsync link",
            guide.components.len()
        ));
        self.loaded_input = None;
        self.downloads.unsaved_records = false;
        self.wizard = (!guide.components.is_empty()).then(|| modsync::wizard::Wizard::new(&guide));
        self.utility = false;
        self.guide = Some(guide);
        self.selected = 0;
        self.page = 0;
        self.start_pending_nxm_if_ready();
    }
}
