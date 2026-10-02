//! Authenticated per-user single-instance delivery. An acknowledgement means the
//! shared parser accepted the request and the bounded desktop queue retained it.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender},
    },
    thread::JoinHandle,
    time::Duration,
};

pub const ACTIVATE_MESSAGE: &str = "__MODSYNC_ACTIVATE__";
const MESSAGE_LIMIT: usize = 8192;
const FRAME_LIMIT: usize = 64 * 1024;
const QUEUE_LIMIT: usize = 64;
const RECENT_LIMIT: usize = 256;
const DEADLINE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(20);

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(all(test, unix))]
use std::path::PathBuf;

pub struct Guard {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    id: String,
    message: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack {
    version: u8,
    id: String,
    accepted: bool,
    reason: String,
}
impl Request {
    fn new(protocol: Option<&str>) -> Result<Self> {
        let message = protocol.unwrap_or(ACTIVATE_MESSAGE).trim().to_owned();
        validate_message(&message)?;
        Ok(Self {
            version: 1,
            id: uuid::Uuid::new_v4().to_string(),
            message,
        })
    }
}
fn validate_message(message: &str) -> Result<()> {
    ensure!(
        !message.is_empty() && message.len() <= MESSAGE_LIMIT,
        "Protocol request must contain 1 to 8192 bytes"
    );
    if message != ACTIVATE_MESSAGE {
        crate::protocol::parse(message)?;
    }
    Ok(())
}
fn admit(
    request: Request,
    sender: &SyncSender<String>,
    recent: &mut VecDeque<(String, String)>,
) -> Ack {
    let result = (|| -> Result<()> {
        ensure!(request.version == 1, "Unsupported handoff version");
        uuid::Uuid::parse_str(&request.id)?;
        validate_message(&request.message)?;
        if let Some((_, old)) = recent.iter().find(|(id, _)| id == &request.id) {
            ensure!(
                old == &request.message,
                "Request ID was reused for different content"
            );
            return Ok(());
        }
        sender
            .try_send(request.message.clone())
            .map_err(|_| anyhow::anyhow!("Primary request queue is full or closed"))?;
        recent.push_back((request.id.clone(), request.message));
        if recent.len() > RECENT_LIMIT {
            recent.pop_front();
        }
        Ok(())
    })();
    Ack {
        version: 1,
        id: request.id,
        accepted: result.is_ok(),
        reason: result
            .err()
            .map_or_else(String::new, |error| error.to_string()),
    }
}
fn check_ack(request: &Request, ack: Ack) -> Result<()> {
    ensure!(
        ack.version == 1 && ack.id == request.id,
        "Primary acknowledgement identity differs"
    );
    ensure!(ack.accepted, "Primary rejected request: {}", ack.reason);
    Ok(())
}

pub fn claim_or_forward(protocol: Option<&str>) -> Result<Option<(Receiver<String>, Guard)>> {
    claim_or_forward_for_channel(protocol, "stable")
}

/// Separate installed release channels while retaining the historic stable namespace.
pub fn claim_or_forward_for_channel(
    protocol: Option<&str>,
    channel: &str,
) -> Result<Option<(Receiver<String>, Guard)>> {
    ensure!(
        !channel.is_empty()
            && channel.len() <= 128
            && channel
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
        "Invalid single-instance channel"
    );
    let namespace = if channel == "stable" {
        "stable".into()
    } else {
        use sha2::{Digest, Sha256};
        format!("channel-{:x}", Sha256::digest(channel.as_bytes()))[..32].to_owned()
    };
    #[cfg(unix)]
    {
        unix::claim_or_forward(protocol, &namespace)
    }
    #[cfg(windows)]
    {
        windows::claim_or_forward(protocol, namespace)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (protocol, namespace);
        anyhow::bail!("Single-instance transport is unsupported on this platform")
    }
}

#[cfg(all(test, unix))]
fn claim_at(path: PathBuf, protocol: Option<&str>) -> Result<Option<(Receiver<String>, Guard)>> {
    unix::claim_at(path, protocol)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn secondary_protocol_launch_reaches_the_primary_receiver() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("handoff.sock");
        let (receiver, _guard) = claim_at(path.clone(), None).unwrap().unwrap();
        assert!(claim_at(path.clone(), None).unwrap().is_none());
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            ACTIVATE_MESSAGE
        );
        assert!(
            claim_at(path, Some("nxm://kotor/mods/1/files/2"))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            "nxm://kotor/mods/1/files/2"
        );
    }
}
