//! Cancellable, bounded reads of verified archive metadata spools.
use anyhow::{Result, ensure};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) const INPUT_LIMIT: usize = 8 * 1024 * 1024;

pub(super) fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "FOMOD metadata inspection cancelled"
    );
    Ok(())
}

pub(super) fn read(path: &Path, cancelled: &AtomicBool) -> Result<String> {
    check(cancelled)?;
    let mut file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= INPUT_LIMIT as u64,
        "FOMOD metadata exceeds 8 MiB"
    );
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        check(cancelled)?;
        let count = file.read(&mut chunk)?;
        check(cancelled)?;
        if count == 0 {
            break;
        }
        ensure!(
            count <= INPUT_LIMIT.saturating_sub(bytes.len()),
            "FOMOD metadata exceeds 8 MiB"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    check(cancelled)?;
    Ok(String::from_utf8(bytes)?)
}
