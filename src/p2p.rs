use anyhow::{Context, Result, ensure};
use librqbit::{AddTorrent, Session};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedInfo {
    pub magnet: String,
    pub sha256: String,
    pub peer: Option<String>,
}

/// Validate an authored archive, announce it to the swarm, and seed until Ctrl+C.
pub fn seed(archive: &Path, trackers: Vec<String>) -> Result<SeedInfo> {
    ensure!(archive.is_file(), "Seed archive does not exist");
    crate::archive::verify(
        archive,
        archive,
        &crate::archive::Options::default(),
        &AtomicBool::new(false),
    )?;
    let sha256 = file_sha256(archive)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let magnet = runtime.block_on(async {
        let session = session(
            archive
                .parent()
                .context("Seed archive needs a parent folder")?
                .to_path_buf(),
        )
        .await?;
        let (torrent, _) = session
            .create_and_serve_torrent(
                archive,
                librqbit::CreateTorrentOptions {
                    trackers,
                    ..Default::default()
                },
            )
            .await?;
        let magnet = torrent.as_magnet().to_string();
        let peer = session
            .listen_addr()
            .map(|address| local_peer(address.port()).to_string());
        println!(
            "{}",
            serde_json::to_string(&SeedInfo {
                magnet: magnet.clone(),
                sha256: sha256.clone(),
                peer: peer.clone(),
            })?
        );
        tokio::signal::ctrl_c().await?;
        session.stop().await;
        Ok::<_, anyhow::Error>((magnet, peer))
    })?;
    Ok(SeedInfo {
        magnet: magnet.0,
        sha256,
        peer: magnet.1,
    })
}

/// Download one archive from the BitTorrent swarm and publish it only after
/// both the authored SHA-256 and archive integrity checks pass.
pub fn fetch(
    magnet: &str,
    destination: &Path,
    sha256: &str,
    peers: Vec<SocketAddr>,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
) -> Result<()> {
    ensure!(magnet.starts_with("magnet:?"), "A magnet URI is required");
    ensure!(!destination.exists(), "Download destination already exists");
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "A 64-digit SHA-256 is required"
    );
    let parent = destination
        .parent()
        .context("Choose a destination folder")?;
    let download = tempfile::Builder::new()
        .prefix(".modsync-torrent-")
        .tempdir_in(parent)?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let session = session(download.path().to_path_buf()).await?;
        let handle = session
            .add_torrent(
                AddTorrent::from_url(magnet),
                Some(librqbit::AddTorrentOptions {
                    initial_peers: (!peers.is_empty()).then_some(peers),
                    ..Default::default()
                }),
            )
            .await?
            .into_handle()
            .context("Torrent did not start")?;
        let result = tokio::select! {
            result = handle.wait_until_completed() => result,
            _ = wait_for_cancel(cancelled) => Err(anyhow::anyhow!("Download cancelled")),
        };
        session.stop().await;
        result?;
        Ok::<_, anyhow::Error>(())
    })?;
    publish(
        &only_file(download.path())?,
        destination,
        sha256,
        cancelled,
        options,
    )
}

async fn session(root: PathBuf) -> Result<std::sync::Arc<Session>> {
    Session::new_with_opts(
        root,
        librqbit::SessionOptions {
            listen: Some(librqbit::ListenerOptions {
                enable_upnp_port_forwarding: true,
                ipv4_only: true,
                ..Default::default()
            }),
            ipv4_only: true,
            ..Default::default()
        },
    )
    .await
}

fn local_peer(port: u16) -> SocketAddr {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.0.2.1:9")?;
            socket.local_addr()
        })
        .map(|address| SocketAddr::new(address.ip(), port))
        .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], port)))
}

async fn wait_for_cancel(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

fn only_file(root: &Path) -> Result<PathBuf> {
    let files: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_type().is_file() => Some(Ok(entry.into_path())),
            Ok(_) => None,
            Err(error) => Some(Err(error.into())),
        })
        .collect::<Result<_>>()?;
    ensure!(
        files.len() == 1,
        "ModSync torrents must contain exactly one archive"
    );
    Ok(files.into_iter().next().unwrap())
}

fn publish(
    source: &Path,
    destination: &Path,
    expected_sha256: &str,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
) -> Result<()> {
    let mut source = File::open(source)?;
    let mut temp = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        ensure!(!cancelled.load(Ordering::Relaxed), "Download cancelled");
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        temp.write_all(&buffer[..count])?;
    }
    ensure!(
        format!("{:x}", hash.finalize()).eq_ignore_ascii_case(expected_sha256),
        "SHA-256 mismatch; torrent discarded"
    );
    crate::archive::verify(temp.path(), destination, options, cancelled)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torrent_payload_must_be_one_archive() {
        let root = tempfile::tempdir().unwrap();
        assert!(only_file(root.path()).is_err());
        std::fs::write(root.path().join("one.zip"), b"one").unwrap();
        assert_eq!(only_file(root.path()).unwrap(), root.path().join("one.zip"));
        std::fs::write(root.path().join("two.zip"), b"two").unwrap();
        assert!(only_file(root.path()).is_err());
    }
}
