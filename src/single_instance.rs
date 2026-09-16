use anyhow::Result;
use std::sync::mpsc::Receiver;

#[cfg(unix)]
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::mpsc,
};

#[cfg(unix)]
pub struct Guard(PathBuf);

#[cfg(unix)]
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Claims the per-user Unix socket or forwards one protocol URL to the primary.
/// A stale socket is removed only after a connection attempt proves it has no listener.
#[cfg(unix)]
pub fn claim_or_forward(protocol: Option<&str>) -> Result<Option<(Receiver<String>, Guard)>> {
    let user = std::env::var("USER")
        .unwrap_or_else(|_| "default".into())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    claim_at(directory.join(format!("modsync-rs-{user}.sock")), protocol)
}

#[cfg(unix)]
fn claim_at(path: PathBuf, protocol: Option<&str>) -> Result<Option<(Receiver<String>, Guard)>> {
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            let Some(protocol) = protocol else {
                anyhow::bail!("ModSync.rs is already running");
            };
            if forward(&path, protocol).is_ok() {
                return Ok(None);
            }
            std::fs::remove_file(&path)?;
            UnixListener::bind(&path)?
        }
        Err(error) => return Err(error.into()),
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut message = String::new();
            if Read::by_ref(&mut stream)
                .take(8193)
                .read_to_string(&mut message)
                .is_ok()
                && message.len() <= 8192
                && !message.trim().is_empty()
            {
                let _ = tx.send(message.trim().into());
            }
        }
    });
    Ok(Some((rx, Guard(path))))
}

#[cfg(unix)]
fn forward(path: &std::path::Path, protocol: &str) -> Result<()> {
    let mut stream = UnixStream::connect(path)?;
    stream.write_all(protocol.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
pub struct Guard;

#[cfg(not(unix))]
pub fn claim_or_forward(_protocol: Option<&str>) -> Result<Option<(Receiver<String>, Guard)>> {
    // ponytail: Windows needs named-pipe parity; add it when validating that target.
    Ok(Some((std::sync::mpsc::channel().1, Guard)))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn secondary_protocol_launch_reaches_the_primary_receiver() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("handoff.sock");
        let (receiver, _guard) = claim_at(path.clone(), None).unwrap().unwrap();
        assert!(claim_at(path.clone(), None).is_err());
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
