use anyhow::{Context, Result, ensure};
use librqbit::{AddTorrent, Session};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream, UdpSocket},
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedInfo {
    pub magnet: String,
    pub sha256: String,
    pub peer: Option<String>,
    /// Base64 of the bencoded info dictionary, for the guide's `TorrentInfo`.
    /// Printed only by the proxied mint; the legacy line is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torrent_info: Option<String>,
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
    let cancelled = stop_signal_flag()?;
    // librqbit opens served files for writing, so it gets a verified copy
    // under the same name, never the user's file.
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .context("Seed archive needs a UTF-8 file name")?;
    let copy = ShareCopy::create(archive, name, &default_share_root(), &sha256)?
        .context("Seed archive changed while it was copied")?;
    ensure!(!cancelled.load(Ordering::Relaxed), "Seeding cancelled");
    let runtime = tokio::runtime::Runtime::new()?;
    let magnet = runtime.block_on(async {
        let session = session(copy.dir.path().to_path_buf()).await?;
        let (torrent, _) = session
            .create_and_serve_torrent(
                &copy.path,
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
                torrent_info: None,
            })?
        );
        wait_for_cancel(&cancelled).await;
        session.stop().await;
        Ok::<_, anyhow::Error>((magnet, peer))
    })?;
    Ok(SeedInfo {
        magnet: magnet.0,
        sha256,
        peer: magnet.1,
        torrent_info: None,
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
    fetch_direct(magnet, None, destination, sha256, peers, cancelled, options)
}

/// The direct (`--no-proxy`) download. With `torrent` it adds the torrent from
/// its authored metadata instead of the magnet.
fn fetch_direct(
    magnet: &str,
    torrent: Option<&CheckedInfo>,
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
        let add = match torrent {
            Some(torrent) => AddTorrent::from_bytes(torrent.torrent.clone()),
            None => AddTorrent::from_url(magnet),
        };
        let trackers = match torrent {
            Some(_) => Some(sanitize_magnet(magnet, &[])?.all_trackers),
            None => None,
        };
        let handle = session
            .add_torrent(
                add,
                Some(librqbit::AddTorrentOptions {
                    initial_peers: (!peers.is_empty()).then_some(peers),
                    trackers,
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
    direct_session(root, None, None).await
}

/// Peer timeouts for ModSync's own sharing sessions. librqbit drops a peer
/// that sends nothing for 10 s (`peer_connection.rs`, `read_message` with the
/// default `read_write_timeout`), while its keep-alive is 120 s. A capped
/// seeder serving a full request queue hears nothing for longer than 10 s,
/// dropped the leecher, and an outbound-only seeder then redials only after
/// minutes. Wait longer than a standard keep-alive, and send ours sooner.
fn sharing_peer_options() -> librqbit::PeerConnectionOptions {
    librqbit::PeerConnectionOptions {
        connect_timeout: None,
        read_write_timeout: Some(Duration::from_secs(180)),
        keep_alive_interval: Some(Duration::from_secs(30)),
    }
}

/// Today's direct session (listener, UPnP, DHT, LSD), with an upload cap.
async fn direct_session(
    root: PathBuf,
    upload_bps: Option<NonZeroU32>,
    peer_opts: Option<librqbit::PeerConnectionOptions>,
) -> Result<std::sync::Arc<Session>> {
    Session::new_with_opts(
        root,
        librqbit::SessionOptions {
            connect: peer_opts.map(|peer_opts| librqbit::ConnectionOptions {
                peer_opts: Some(peer_opts),
                ..Default::default()
            }),
            listen: Some(librqbit::ListenerOptions {
                enable_upnp_port_forwarding: true,
                ipv4_only: true,
                ..Default::default()
            }),
            ipv4_only: true,
            ratelimits: librqbit::limits::LimitsConfig {
                upload_bps,
                download_bps: None,
            },
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

fn file_sha256_cancelled(path: &Path, cancelled: &AtomicBool) -> Result<String> {
    let mut file = crate::paths::open_regular_read(path)?;
    let mut hash = Sha256::new();
    let mut block = [0u8; 64 * 1024];
    loop {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Sharing preparation cancelled"
        );
        let count = file.read(&mut block)?;
        if count == 0 {
            return Ok(format!("{:x}", hash.finalize()));
        }
        hash.update(&block[..count]);
    }
}

// ---------------------------------------------------------------------------
// Proxied peer acquisition (plan task P1).
//
// Everything below runs BitTorrent only through a health-checked SOCKS5 proxy.
// A proxied session never has a listener (so no uTP socket and no UPnP), never
// runs DHT or local service discovery, and never announces to UDP trackers.
// Without a working proxy no session is built at all, so no peer socket opens.
// The legacy `fetch` and `seed` above keep their direct behavior; the CLI only
// reaches them through the explicit `--no-proxy` flag.
// ---------------------------------------------------------------------------

/// One-line disclosure for Settings and About.
pub const SHARING_DISCLOSURE: &str = "ModSync shares mod files with other users over BitTorrent, through a public proxy. Turn this off in Settings.";

/// Default public SOCKS5 list. It is fetched as data at run time.
pub const DEFAULT_PROXY_LIST_URL: &str =
    "https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/socks5.txt";

/// Default tracker lists: the HTTP and HTTPS subsets of ngosang/trackerslist.
pub const DEFAULT_TRACKER_LIST_URLS: [&str; 2] = [
    "https://raw.githubusercontent.com/ngosang/trackerslist/master/trackers_all_http.txt",
    "https://raw.githubusercontent.com/ngosang/trackerslist/master/trackers_all_https.txt",
];

/// Default upload cap in KiB/s (Wizard's earlier `MaxSendKbps = 100`).
pub const DEFAULT_UPLOAD_KIBPS: u32 = 100;

/// librqbit uploads 16 KiB blocks, and its rate limiter rejects a block larger
/// than one second of quota. A lower cap would drop every upload request.
pub const MIN_UPLOAD_BPS: u32 = 16 * 1024;

/// Most proxies tried for one target before the peer step gives up.
pub const DEFAULT_MAX_PROXY_ATTEMPTS: usize = 5;

const LIST_BYTES_LIMIT: u64 = 4 * 1024 * 1024;

/// How peer transfers relate to host downloads (`peerMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PeerMode {
    Off,
    Race,
    Fallback,
}

/// Peer settings as stored in `settings.json`. Sharing is on by default.
#[derive(Clone, Debug)]
pub struct PeerSettings {
    /// `peerSharing`: the off-switch for every peer socket. Default on.
    pub sharing: bool,
    /// `peerKeepSharing`: share finished, verified downloads. Default on.
    pub keep_sharing: bool,
    /// `peerUploadWhileDownloading`: upload pieces during a peer download.
    /// Default on.
    pub upload_while_downloading: bool,
    /// `peerUploadKiBps`: upload cap. Default 100 KiB/s.
    pub upload_kibps: u32,
    /// `peerMode`. Default `Race`.
    pub mode: PeerMode,
    /// `peerProxies`: explicit `socks5://ip:port` endpoints.
    pub proxies: Vec<String>,
    /// `peerProxyListUrl`: public SOCKS5 list. `None` or empty disables it.
    pub proxy_list_url: Option<String>,
    /// `peerTrackerListUrl`: tracker list URLs (string or array).
    pub tracker_list_urls: Vec<String>,
    /// `peerExtraPeers`: user-entered peers, dialed only through the proxy.
    pub extra_peers: Vec<String>,
    /// `peerTrackers`: user-entered trackers. Only http(s) ones are used.
    pub trackers: Vec<String>,
}

impl Default for PeerSettings {
    fn default() -> Self {
        Self {
            sharing: true,
            keep_sharing: true,
            upload_while_downloading: true,
            upload_kibps: DEFAULT_UPLOAD_KIBPS,
            mode: PeerMode::Race,
            proxies: Vec::new(),
            proxy_list_url: Some(DEFAULT_PROXY_LIST_URL.to_owned()),
            tracker_list_urls: DEFAULT_TRACKER_LIST_URLS.map(str::to_owned).to_vec(),
            extra_peers: Vec::new(),
            trackers: Vec::new(),
        }
    }
}

impl PeerSettings {
    /// Completed-sharing bootstrap, gated before any list request. Each request
    /// has the existing 20-second bound; cancellation is checked between them.
    pub fn sharing_policy(
        &self,
        cache_dir: PathBuf,
        cancelled: &AtomicBool,
    ) -> Result<Option<PeerPolicy>> {
        if !self.sharing || !self.keep_sharing || self.mode == PeerMode::Off {
            return Ok(None);
        }
        let mut configured = self.clone();
        configured.tracker_list_urls.clear();
        for (index, url) in self.tracker_list_urls.iter().enumerate() {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Sharing bootstrap cancelled"
            );
            if let Ok(text) =
                fetch_list(url, &format!("peer-tracker-list-{index}"), Some(&cache_dir))
            {
                configured.trackers.extend(
                    text.lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(str::to_owned),
                );
            }
        }
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Sharing bootstrap cancelled"
        );
        Ok(configured.policy(Some(cache_dir)))
    }
    /// Read the `peer*` keys from a `settings.json` value. Missing or malformed
    /// keys keep their defaults.
    pub fn from_settings(settings: &serde_json::Value) -> Self {
        let mut out = Self::default();
        let Some(map) = settings.as_object() else {
            return out;
        };
        let get = |key: &str| {
            map.iter()
                .rev()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v)
        };
        let strings = |value: &serde_json::Value| -> Vec<String> {
            match value {
                serde_json::Value::String(s) => s
                    .split([',', '\n'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                serde_json::Value::Array(items) => items
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                _ => Vec::new(),
            }
        };
        // A case alias cannot turn an explicit off-switch back on. Malformed
        // values for either safety switch also fail closed.
        for (key, enabled) in [
            ("peerSharing", &mut out.sharing),
            ("peerKeepSharing", &mut out.keep_sharing),
        ] {
            for value in map
                .iter()
                .filter(|(stored, _)| stored.eq_ignore_ascii_case(key))
                .map(|(_, value)| value)
            {
                if value.as_bool() != Some(true) {
                    *enabled = false;
                }
            }
        }
        if let Some(v) = get("peerUploadWhileDownloading").and_then(|v| v.as_bool()) {
            out.upload_while_downloading = v;
        }
        if let Some(v) = get("peerUploadKiBps").and_then(|v| v.as_u64()) {
            out.upload_kibps = u32::try_from(v).unwrap_or(u32::MAX);
        }
        let modes: Vec<_> = map
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case("peerMode"))
            .map(|(_, value)| value.as_str().map(str::to_ascii_lowercase))
            .collect();
        if modes
            .iter()
            .any(|mode| !matches!(mode.as_deref(), Some("race" | "fallback")))
        {
            out.mode = PeerMode::Off;
        } else if modes.iter().any(|mode| mode.as_deref() == Some("fallback")) {
            out.mode = PeerMode::Fallback;
        } else if !modes.is_empty() {
            out.mode = PeerMode::Race;
        }
        if let Some(v) = get("peerProxies") {
            out.proxies = strings(v);
        }
        match get("peerProxyListUrl") {
            Some(serde_json::Value::Null) => out.proxy_list_url = None,
            Some(serde_json::Value::String(s)) => {
                out.proxy_list_url = (!s.trim().is_empty()).then(|| s.trim().to_owned())
            }
            _ => {}
        }
        match get("peerTrackerListUrl") {
            Some(serde_json::Value::Null) => out.tracker_list_urls.clear(),
            Some(v) => out.tracker_list_urls = strings(v),
            None => {}
        }
        if let Some(v) = get("peerExtraPeers") {
            out.extra_peers = strings(v);
        }
        if let Some(v) = get("peerTrackers") {
            out.trackers = strings(v);
        }
        out
    }

    /// The effective mode: `Off` when sharing is turned off.
    pub fn effective_mode(&self) -> PeerMode {
        if self.sharing {
            self.mode
        } else {
            PeerMode::Off
        }
    }

    /// Upload cap in bytes per second, clamped to what librqbit can honor.
    pub fn upload_bps(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(self.upload_kibps.saturating_mul(1024).max(MIN_UPLOAD_BPS))
    }

    /// Build the policy for the acquisition path. Returns `None` when sharing
    /// is off: the caller must then open no peer socket and fetch no list.
    /// Tracker lists are fetched here, as data, over a direct HTTPS request.
    pub fn policy(&self, cache_dir: Option<PathBuf>) -> Option<PeerPolicy> {
        if !self.sharing || self.mode == PeerMode::Off {
            return None;
        }
        let share_root = cache_dir
            .as_ref()
            .map(|dir| dir.join("share"))
            .unwrap_or_else(default_share_root);
        let mut trackers = self.trackers.clone();
        trackers.extend(fetch_tracker_lists(
            &self.tracker_list_urls,
            cache_dir.as_deref(),
        ));
        Some(PeerPolicy {
            proxy: ProxyPool::new(
                self.proxies
                    .iter()
                    .filter_map(|p| parse_proxy_line(p))
                    .collect(),
                self.proxy_list_url.clone(),
                cache_dir,
            ),
            upload_while_downloading: self.upload_while_downloading,
            keep_sharing: self.keep_sharing,
            upload_bps: self.upload_bps(),
            trackers: http_trackers(trackers),
            extra_peers: self
                .extra_peers
                .iter()
                .filter_map(|p| p.parse().ok())
                .collect(),
            stall_timeout: Duration::from_secs(120),
            proxy_recheck_interval: Duration::from_secs(15),
            share_root,
        })
    }
}

/// Why a peer step did not run. The orchestrator can match on this with
/// `anyhow::Error::downcast_ref` instead of reading error text. Each case
/// means Peer failed closed and Origin should run as normal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerUnavailable {
    /// No proxy passed the health check; no peer socket was opened.
    NoWorkingProxy,
    /// Neither an http(s) tracker nor a user peer is known; nothing to dial.
    NoPeerSource,
    /// No bytes or metadata arrived within the stall timeout.
    Stalled,
}

impl std::fmt::Display for PeerUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoWorkingProxy => "No working SOCKS5 proxy; peer download skipped",
            Self::NoPeerSource => "No HTTP(S) tracker or peer to ask; peer download skipped",
            Self::Stalled => "No peer sent data in time; peer download stopped",
        })
    }
}

impl std::error::Error for PeerUnavailable {}

/// A SOCKS5 endpoint that passed a health check. Only `ProxyPool` creates it,
/// so a proxied session cannot be built without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthyProxy(SocketAddr);

impl HealthyProxy {
    pub fn address(&self) -> SocketAddr {
        self.0
    }
    fn url(&self) -> String {
        format!("socks5://{}", self.0)
    }
}

/// Where a health check asks the proxy to connect: a tracker or a user peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthTarget {
    pub host: String,
    pub port: u16,
}

impl HealthTarget {
    /// Targets taken from http(s) trackers, then from user peers.
    pub fn from_sources(trackers: &[String], peers: &[SocketAddr]) -> Vec<Self> {
        let mut targets: Vec<Self> = trackers
            .iter()
            .filter_map(|t| reqwest::Url::parse(t).ok())
            .filter_map(|u| {
                Some(Self {
                    host: u.host_str()?.trim_matches(['[', ']']).to_owned(),
                    port: u.port_or_known_default()?,
                })
            })
            .collect();
        targets.extend(peers.iter().map(|p| Self {
            host: p.ip().to_string(),
            port: p.port(),
        }));
        targets.dedup();
        targets
    }
}

/// Parse one proxy list line. Keeps `ip:port` and `socks5://ip:port` with a
/// literal IP. Drops http, https, socks4 and anything with a host name.
pub fn parse_proxy_line(line: &str) -> Option<SocketAddr> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let rest = match line.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("socks5") => rest,
        Some(_) => return None,
        None => line,
    };
    rest.trim_end_matches('/').parse::<SocketAddr>().ok()
}

/// Parse a whole proxy list, keeping order and dropping duplicates.
pub fn parse_proxy_list(text: &str) -> Vec<SocketAddr> {
    let mut seen = HashSet::new();
    text.lines()
        .filter_map(parse_proxy_line)
        .filter(|p| seen.insert(*p))
        .collect()
}

/// Explicit SOCKS5 endpoints plus a public list, health-checked and rotated.
/// The list is fetched lazily, only when no explicit proxy works, and at most
/// once per pool.
pub struct ProxyPool {
    explicit: Vec<SocketAddr>,
    list_url: Option<String>,
    cache_dir: Option<PathBuf>,
    listed: OnceLock<Vec<SocketAddr>>,
    bad: Mutex<HashSet<SocketAddr>>,
    pub max_attempts: usize,
    pub connect_timeout: Duration,
}

impl ProxyPool {
    pub fn new(
        explicit: Vec<SocketAddr>,
        list_url: Option<String>,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            explicit,
            list_url: list_url.filter(|u| !u.trim().is_empty()),
            cache_dir,
            listed: OnceLock::new(),
            bad: Mutex::new(HashSet::new()),
            max_attempts: DEFAULT_MAX_PROXY_ATTEMPTS,
            connect_timeout: Duration::from_secs(5),
        }
    }

    /// Mark a proxy as failed for the rest of this run.
    pub fn mark_bad(&self, proxy: HealthyProxy) {
        self.bad.lock().unwrap().insert(proxy.0);
    }

    fn is_bad(&self, proxy: SocketAddr) -> bool {
        self.bad.lock().unwrap().contains(&proxy)
    }

    fn listed(&self) -> &[SocketAddr] {
        self.listed.get_or_init(|| match &self.list_url {
            Some(url) => fetch_list(url, "peer-proxy-list", self.cache_dir.as_deref())
                .map(|text| parse_proxy_list(&text))
                .unwrap_or_default(),
            None => Vec::new(),
        })
    }

    /// Return the next proxy that completes a SOCKS5 CONNECT to one of
    /// `targets`. Proxies that fail are marked bad. At most `max_attempts`
    /// health checks run per call. With no target, nothing is contacted.
    pub fn next_healthy(&self, targets: &[HealthTarget]) -> Option<HealthyProxy> {
        self.next_healthy_cancelled(targets, &AtomicBool::new(false))
    }

    fn next_healthy_cancelled(
        &self,
        targets: &[HealthTarget],
        cancelled: &AtomicBool,
    ) -> Option<HealthyProxy> {
        if targets.is_empty() {
            return None;
        }
        let mut tried = 0;
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let explicit = self.explicit.clone();
        for proxy in explicit.into_iter().chain(self.listed().iter().copied()) {
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            if tried >= self.max_attempts {
                break;
            }
            if self.is_bad(proxy) {
                continue;
            }
            tried += 1;
            if targets.iter().take(3).any(|t| {
                !cancelled.load(Ordering::Relaxed)
                    && socks5_connect_check(proxy, t, self.connect_timeout)
            }) {
                return Some(HealthyProxy(proxy));
            }
            self.bad.lock().unwrap().insert(proxy);
        }
        None
    }

    /// Recheck a proxy already in use.
    pub fn still_healthy(&self, proxy: HealthyProxy, targets: &[HealthTarget]) -> bool {
        targets
            .iter()
            .take(3)
            .any(|t| socks5_connect_check(proxy.0, t, self.connect_timeout))
    }
}

/// Do a SOCKS5 no-auth CONNECT to `target` through `proxy` and close it.
/// Returns true only when the proxy reports success.
pub fn socks5_connect_check(proxy: SocketAddr, target: &HealthTarget, timeout: Duration) -> bool {
    fn attempt(
        proxy: SocketAddr,
        target: &HealthTarget,
        timeout: Duration,
    ) -> std::io::Result<bool> {
        let deadline = std::time::Instant::now() + timeout;
        // Read until full or the deadline. A timed read can end early with
        // WouldBlock (for example when a debugger attaches a thread), so
        // retry until the deadline instead of failing the proxy.
        let read_full = |stream: &mut TcpStream, buffer: &mut [u8]| -> std::io::Result<()> {
            let mut filled = 0;
            while filled < buffer.len() {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    return Err(std::io::ErrorKind::TimedOut.into());
                }
                stream.set_read_timeout(Some(left))?;
                match stream.read(&mut buffer[filled..]) {
                    Ok(0) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
                    Ok(count) => filled += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        };
        let mut stream = TcpStream::connect_timeout(&proxy, timeout)?;
        stream.set_write_timeout(Some(timeout))?;
        stream.write_all(&[5, 1, 0])?;
        let mut reply = [0u8; 2];
        read_full(&mut stream, &mut reply)?;
        if reply != [5, 0] {
            return Ok(false);
        }
        let mut request = vec![5, 1, 0];
        match target.host.parse::<IpAddr>() {
            Ok(IpAddr::V4(ip)) => {
                request.push(1);
                request.extend(ip.octets());
            }
            Ok(IpAddr::V6(ip)) => {
                request.push(4);
                request.extend(ip.octets());
            }
            Err(_) => {
                let host = target.host.as_bytes();
                if host.is_empty() || host.len() > 255 {
                    return Ok(false);
                }
                request.push(3);
                request.push(host.len() as u8);
                request.extend(host);
            }
        }
        request.extend(target.port.to_be_bytes());
        stream.write_all(&request)?;
        let mut head = [0u8; 4];
        read_full(&mut stream, &mut head)?;
        Ok(head[0] == 5 && head[1] == 0)
    }
    attempt(proxy, target, timeout).unwrap_or(false)
}

/// Keep only http(s) trackers, trimmed and deduplicated in order.
pub fn http_trackers(trackers: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    trackers
        .into_iter()
        .map(|t| t.trim().to_owned())
        .filter(|t| {
            reqwest::Url::parse(t)
                .map(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
                .unwrap_or(false)
        })
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

/// Fetch tracker lists as data and return their lines. Failures fall back to
/// the cached copy, then to nothing.
pub fn fetch_tracker_lists(urls: &[String], cache_dir: Option<&Path>) -> Vec<String> {
    urls.iter()
        .enumerate()
        .filter_map(|(i, url)| fetch_list(url, &format!("peer-tracker-list-{i}"), cache_dir).ok())
        .flat_map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListCache {
    url: String,
    fetched_at: String,
    body: String,
}

/// Fetch a small text list over http(s) (or read a `file://` URL). A good
/// fetch is cached under `cache_dir`; a failed one falls back to that cache
/// if it was for the same URL.
fn fetch_list(url: &str, cache_name: &str, cache_dir: Option<&Path>) -> Result<String> {
    let cache = cache_dir.map(|d| d.join(format!("{cache_name}.json")));
    let fetched = (|| -> Result<String> {
        let parsed = reqwest::Url::parse(url).context("Invalid list URL")?;
        match parsed.scheme() {
            "file" => {
                let path = parsed
                    .to_file_path()
                    .map_err(|_| anyhow::anyhow!("Invalid file URL"))?;
                ensure!(
                    std::fs::metadata(&path)?.is_file(),
                    "List source must be a regular file"
                );
                let mut text = String::new();
                File::open(path)?
                    .take(LIST_BYTES_LIMIT)
                    .read_to_string(&mut text)?;
                Ok(text)
            }
            "http" | "https" => {
                // No system or environment proxy: list fetches are direct
                // requests for public data, and ALL_PROXY/HTTPS_PROXY must not
                // silently reroute them.
                let client = reqwest::blocking::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(20))
                    .build()?;
                let response = client.get(parsed).send()?.error_for_status()?;
                let mut text = String::new();
                response.take(LIST_BYTES_LIMIT).read_to_string(&mut text)?;
                Ok(text)
            }
            other => anyhow::bail!("Unsupported list URL scheme {other}"),
        }
    })();
    match (fetched, cache) {
        (Ok(body), Some(cache)) => {
            let record = ListCache {
                url: url.to_owned(),
                fetched_at: chrono::Utc::now().to_rfc3339(),
                body: body.clone(),
            };
            if let Some(parent) = cache.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_vec(&record) {
                let _ = crate::recovery::atomic_write(&cache, &json);
            }
            Ok(body)
        }
        (Ok(body), None) => Ok(body),
        (Err(error), Some(cache)) => std::fs::read(&cache)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ListCache>(&bytes).ok())
            .filter(|record| record.url == url)
            .map(|record| record.body)
            .ok_or(error),
        (Err(error), None) => Err(error),
    }
}

/// An authored magnet reduced to what a proxied session may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedMagnet {
    /// Rebuilt magnet: `xt`, `dn` and http(s) `tr` only.
    pub uri: String,
    /// Lower-case hex BTv1 infohash.
    pub info_hash: String,
    pub name: Option<String>,
    /// http(s) trackers only: the set a proxied session may use.
    pub trackers: Vec<String>,
    /// Every `tr=` in the magnet, UDP included. Only the direct path uses it.
    pub all_trackers: Vec<String>,
}

/// Rebuild a magnet from its BTv1 `xt`, its `dn`, and http(s) trackers from
/// the magnet plus `extra_trackers`. Everything else is dropped, including
/// UDP trackers, `x.pe` peer hints and web seeds, so a guide cannot choose
/// endpoints the client dials outside the tracker set. Magnet trackers with
/// a host name are kept only if `extra_trackers` also lists them, so no
/// guide-chosen name reaches the local resolver. `all_trackers` keeps every
/// `tr=` for the direct path.
pub fn sanitize_magnet(magnet: &str, extra_trackers: &[String]) -> Result<SanitizedMagnet> {
    ensure!(magnet.starts_with("magnet:?"), "A magnet URI is required");
    let parsed = reqwest::Url::parse(magnet).context("Invalid magnet URI")?;
    let mut info_hash = None;
    let mut name = None;
    let mut trackers = Vec::new();
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "xt" => {
                if let Some(hash) = value.strip_prefix("urn:btih:") {
                    ensure!(
                        hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                        "The magnet needs a 40-digit hex BTv1 infohash"
                    );
                    info_hash = Some(hash.to_ascii_lowercase());
                }
            }
            "dn" if !value.is_empty() => name = Some(value.into_owned()),
            "tr" => trackers.push(value.into_owned()),
            _ => {}
        }
    }
    let info_hash = info_hash.context("The magnet needs a BTv1 infohash")?;
    let all_trackers = trackers.clone();
    // A guide-authored tracker host name would be resolved by the user's
    // own DNS resolver (reqwest resolves `socks5://` proxy targets locally),
    // telling it which guide resource is being fetched. Keep magnet trackers
    // only when the host is a literal IP, or when the same tracker is in the
    // fetched public list or the user's own trackers.
    trackers.retain(|tracker| {
        reqwest::Url::parse(tracker).is_ok_and(|url| {
            url.host_str()
                .is_some_and(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().is_ok())
        }) || extra_trackers.iter().any(|t| t.trim() == tracker.trim())
    });
    trackers.extend(extra_trackers.iter().cloned());
    let trackers = http_trackers(trackers);
    let mut uri = reqwest::Url::parse("magnet:?").unwrap();
    {
        let mut query = uri.query_pairs_mut();
        query.append_pair("xt", &format!("urn:btih:{info_hash}"));
        if let Some(name) = &name {
            query.append_pair("dn", name);
        }
        for tracker in &trackers {
            query.append_pair("tr", tracker);
        }
    }
    Ok(SanitizedMagnet {
        uri: uri.to_string(),
        info_hash,
        name,
        trackers,
        all_trackers,
    })
}

/// Standard base64 (RFC 4648) with padding, for `TorrentInfo`.
pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(TABLE[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode standard or URL-safe base64. Whitespace and padding are ignored.
pub fn base64_decode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => anyhow::bail!("TorrentInfo is not base64"),
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

/// A file name that is safe to create inside a ModSync folder: one path
/// component, no separators, no drive or parent references.
fn safe_file_name(name: &str) -> bool {
    // One component that passes the guide path rules (`paths::relative`):
    // no separators, `..`, drive colons, trailing dots or spaces, or Windows
    // reserved names such as CON.zip, on every host.
    !name.is_empty()
        && name.len() <= 255
        && !name.contains(['/', '\\'])
        && crate::paths::relative(name, false)
            .is_ok_and(|path| path.as_os_str() == std::ffi::OsStr::new(name))
}

/// An authored info dictionary that matched its magnet.
#[derive(Debug, Clone)]
pub struct CheckedInfo {
    /// The single file's name from the info dictionary.
    pub name: String,
    pub length: u64,
    /// A minimal .torrent (`d4:info<info>e`) for librqbit's add-from-bytes path.
    torrent: Vec<u8>,
}

/// Check an authored `TorrentInfo` (bencoded info dictionary) against the
/// magnet's BTv1 infohash. It must describe exactly one file with a safe
/// name, and must not be a private torrent.
pub fn check_torrent_info(info: &[u8], info_hash: &str) -> Result<CheckedInfo> {
    let actual = format!("{:x}", sha1::Sha1::digest(info));
    ensure!(
        actual.eq_ignore_ascii_case(info_hash),
        "TorrentInfo hashes to {actual}, not the magnet's {info_hash}"
    );
    let mut torrent = b"d4:info".to_vec();
    torrent.extend_from_slice(info);
    torrent.push(b'e');
    let parsed = librqbit::torrent_from_bytes(&torrent)
        .map_err(|error| anyhow::anyhow!("TorrentInfo is not a valid info dictionary: {error}"))?;
    // librqbit identifies the torrent by the SHA-1 of the parsed info span,
    // which can differ from the raw input (for example trailing bytes).
    ensure!(
        parsed.info_hash.as_string().eq_ignore_ascii_case(info_hash),
        "TorrentInfo parses to infohash {}, not the magnet's {info_hash}",
        parsed.info_hash.as_string()
    );
    let data = &parsed.info.data;
    ensure!(
        data.files.is_none(),
        "ModSync torrents must contain exactly one archive"
    );
    ensure!(!data.private, "Private torrents are not shared");
    let length = data.length.context("TorrentInfo has no file length")?;
    let name = data
        .name
        .as_ref()
        .and_then(|n| std::str::from_utf8(n.as_ref()).ok())
        .context("TorrentInfo needs a UTF-8 file name")?
        .to_owned();
    ensure!(safe_file_name(&name), "TorrentInfo has an unsafe file name");
    Ok(CheckedInfo {
        name,
        length,
        torrent,
    })
}

/// The authored identity of one archive: its `Magnet`, and optionally its
/// `TorrentInfo`. With the info dictionary the client adds the torrent from
/// metadata, so it is known to the session (and accepts peers) at once.
#[derive(Debug, Clone, Copy)]
pub struct AuthoredTorrent<'a> {
    pub magnet: &'a str,
    /// Decoded `TorrentInfo` bytes (see `base64_decode`).
    pub torrent_info: Option<&'a [u8]>,
}

/// Where sharing copies live when no profile directory is given.
pub fn default_share_root() -> PathBuf {
    crate::profiles::settings_directory(None)
        .map(|dir| dir.join("peer-share"))
        .unwrap_or_else(|_| std::env::temp_dir().join("modsync-peer-share"))
}

/// A private copy of a user's archive that the torrent engine may open for
/// writing. librqbit opens every served file read-write
/// (`storage/filesystem/fs.rs:161`), so it never gets the user's file. The
/// folder is removed when this is dropped.
struct ShareCopy {
    // Dropped first: the lock must be released (and the file closed) before
    // the folder is removed, or Windows refuses the delete.
    _owner: File,
    dir: tempfile::TempDir,
    path: PathBuf,
}

const SHARE_PREFIX: &str = ".modsync-share-";
const SHARE_OWNER: &str = ".modsync-share-owner";
const SHARE_ROOT_LOCK: &str = ".modsync-share.lock";

/// Hold the share root's lock while folders are created or swept, so a sweep
/// never sees a folder whose owner lock is not taken yet.
fn lock_share_root(root: &Path) -> Result<File> {
    lock_share_root_cancelled(root, &AtomicBool::new(false))
}

fn lock_share_root_cancelled(root: &Path, cancelled: &AtomicBool) -> Result<File> {
    use fs2::FileExt;
    std::fs::create_dir_all(root)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(SHARE_ROOT_LOCK))?;
    loop {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Sharing preparation cancelled"
        );
        match lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(lock)
}

fn sweep_locked(root: &Path) -> usize {
    use fs2::FileExt;
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(SHARE_PREFIX)
            || !entry.file_type().is_ok_and(|t| t.is_dir())
        {
            continue;
        }
        let owner = entry.path().join(SHARE_OWNER);
        // A live process holds its owner lock for as long as it shares. A
        // folder without an owner file cannot be mid-creation, because
        // creation holds the root lock that this sweep holds now.
        let stale = match OpenOptions::new().write(true).open(&owner) {
            Ok(file) => file.try_lock_exclusive().is_ok(),
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        };
        if stale && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Remove sharing folders left by processes that died (SIGKILL, power loss).
/// Folders of live processes, in this or another process, are kept.
pub fn sweep_stale_shares(root: &Path) -> Result<usize> {
    let _lock = lock_share_root(root)?;
    Ok(sweep_locked(root))
}

impl ShareCopy {
    /// Copy `archive` to `<root>/.modsync-share-*/<name>` and check the copy's
    /// SHA-256. Stale folders from dead processes are swept first. The
    /// folder carries an owner lock for as long as this value lives.
    /// Copies in bounded chunks and independently hashes the result. Returns `None` when
    /// the copy does not hash to `sha256`.
    fn create(archive: &Path, name: &str, root: &Path, sha256: &str) -> Result<Option<Self>> {
        Self::create_cancelled(archive, name, root, sha256, &AtomicBool::new(false))
    }

    fn create_cancelled(
        archive: &Path,
        name: &str,
        root: &Path,
        sha256: &str,
        cancelled: &AtomicBool,
    ) -> Result<Option<Self>> {
        use fs2::FileExt;
        ensure!(safe_file_name(name), "Unsafe torrent file name");
        let (owner, dir) = {
            let _root = lock_share_root_cancelled(root, cancelled)?;
            sweep_locked(root);
            let dir = tempfile::Builder::new()
                .prefix(SHARE_PREFIX)
                .tempdir_in(root)?;
            let owner = File::create(dir.path().join(SHARE_OWNER))?;
            owner.lock_exclusive()?;
            (owner, dir)
        };
        let path = dir.path().join(name);
        let copy = Self {
            _owner: owner,
            dir,
            path,
        };
        let path = &copy.path;
        let mut source = crate::paths::open_regular_read(archive)?;
        let mut target = File::create(path)?;
        let mut block = [0u8; 64 * 1024];
        loop {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Sharing preparation cancelled"
            );
            let count = source.read(&mut block)?;
            if count == 0 {
                break;
            }
            target.write_all(&block[..count])?;
        }
        drop(target);
        if !file_sha256_cancelled(path, cancelled)?.eq_ignore_ascii_case(sha256) {
            return Ok(None);
        }
        Ok(Some(copy))
    }
}

/// Everything a proxied peer session needs. Built by `PeerSettings::policy`
/// or by hand. Listener, UPnP, LSD, DHT and UDP trackers are not options:
/// the proxied session always turns them off.
pub struct PeerPolicy {
    pub proxy: ProxyPool,
    /// Upload pieces while a peer download runs (default true, needed for
    /// 90548). `false` sets librqbit's `disable_upload` on download sessions.
    pub upload_while_downloading: bool,
    /// Share finished, verified downloads (`peerKeepSharing`). When false,
    /// `share_verified*` returns `SharingOff` and opens nothing.
    pub keep_sharing: bool,
    /// Upload cap. Values under `MIN_UPLOAD_BPS` are raised to it.
    pub upload_bps: Option<NonZeroU32>,
    /// http(s) trackers: list-fetched plus user-entered. Magnet `tr=` entries
    /// are merged per target. UDP entries are always dropped.
    pub trackers: Vec<String>,
    /// User-entered peers only, dialed through the proxy. Never from guides.
    pub extra_peers: Vec<SocketAddr>,
    /// Give up when no byte (or no metadata) arrives for this long.
    pub stall_timeout: Duration,
    /// Recheck the proxy in use this often; rotate when it fails.
    pub proxy_recheck_interval: Duration,
    /// ModSync-owned folder for sharing copies (see `ShareCopy`).
    pub share_root: PathBuf,
}

impl PeerPolicy {
    fn capped_upload(&self) -> Option<NonZeroU32> {
        self.upload_bps
            .map(|bps| bps.max(NonZeroU32::new(MIN_UPLOAD_BPS).unwrap()))
    }
}

/// Progress of a peer download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerProgress {
    /// librqbit stats, polled every 100 ms. `peers` is the live peer count.
    Transferring {
        bytes: u64,
        total: Option<u64>,
        peers: Option<u32>,
        proxy: SocketAddr,
    },
    /// The proxy failed its recheck; the next one is being tried.
    ProxyRotated { from: SocketAddr },
    /// Bytes are complete; SHA-256 and archive checks are running.
    Verifying,
}

fn pause() -> Duration {
    Duration::from_millis(100)
}

/// Build the proxied session. Taking `HealthyProxy` means no session (and so
/// no socket, not even librqbit's always-bound UDP tracker socket) exists
/// until a proxy has passed a health check.
async fn proxied_session(
    root: PathBuf,
    proxy: HealthyProxy,
    trackers: &[String],
    upload_bps: Option<NonZeroU32>,
    disable_upload: bool,
) -> Result<std::sync::Arc<Session>> {
    Session::new_with_opts(
        root,
        librqbit::SessionOptions {
            disable_upload,
            dht: None,
            listen: None,
            connect: Some(librqbit::ConnectionOptions {
                proxy_url: Some(proxy.url()),
                enable_tcp: true,
                peer_opts: Some(sharing_peer_options()),
            }),
            disable_local_service_discovery: true,
            ipv4_only: true,
            persistence: None,
            fastresume: false,
            blocklist_url: None,
            allowlist_url: None,
            trackers: trackers
                .iter()
                .filter_map(|t| reqwest::Url::parse(t).ok())
                .collect(),
            ratelimits: librqbit::limits::LimitsConfig {
                upload_bps,
                download_bps: None,
            },
            ..Default::default()
        },
    )
    .await
}

enum AttemptEnd {
    Finished,
    ProxyFailed,
}

/// Download one authored archive from peers through a SOCKS5 proxy, then
/// verify and publish it exactly like `fetch`. Returns the verified SHA-256.
///
/// Fails closed with `PeerUnavailable` before any peer socket when there is
/// no tracker or peer to ask, or when no proxy passes its health check.
pub fn fetch_observed(
    magnet: &str,
    destination: &Path,
    sha256: &str,
    policy: &PeerPolicy,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    on: impl FnMut(PeerProgress),
) -> Result<String> {
    fetch_authored(
        AuthoredTorrent {
            magnet,
            torrent_info: None,
        },
        destination,
        sha256,
        policy,
        cancelled,
        options,
        on,
    )
}

/// `fetch_observed` for an authored resource. With `TorrentInfo` the torrent
/// is added from its metadata: no magnet resolution, and other ModSync peers
/// can connect to it from the first second.
pub fn fetch_authored(
    authored: AuthoredTorrent<'_>,
    destination: &Path,
    sha256: &str,
    policy: &PeerPolicy,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    mut on: impl FnMut(PeerProgress),
) -> Result<String> {
    let started = std::time::Instant::now();
    let mut bytes = 0;
    let result = fetch_authored_inner(
        authored,
        destination,
        sha256,
        policy,
        cancelled,
        options,
        |update| {
            if let PeerProgress::Transferring {
                bytes: observed, ..
            } = &update
            {
                bytes = bytes.max(*observed);
            }
            on(update);
        },
    );
    let error = result.as_ref().err().map(|error| format!("{error:#}"));
    crate::telemetry::global().record_download(
        &destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        result.is_ok(),
        started.elapsed(),
        bytes,
        Some("peer"),
        error.as_deref(),
    );
    result
}

fn fetch_authored_inner(
    authored: AuthoredTorrent<'_>,
    destination: &Path,
    sha256: &str,
    policy: &PeerPolicy,
    cancelled: &AtomicBool,
    options: &crate::archive::Options,
    mut on: impl FnMut(PeerProgress),
) -> Result<String> {
    ensure!(!destination.exists(), "Download destination already exists");
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "A 64-digit SHA-256 is required"
    );
    let magnet = sanitize_magnet(authored.magnet, &policy.trackers)?;
    let info = authored
        .torrent_info
        .map(|info| check_torrent_info(info, &magnet.info_hash))
        .transpose()?;
    let targets = HealthTarget::from_sources(&magnet.trackers, &policy.extra_peers);
    if targets.is_empty() {
        return Err(PeerUnavailable::NoPeerSource.into());
    }
    let parent = destination
        .parent()
        .context("Choose a destination folder")?;
    let download = tempfile::Builder::new()
        .prefix(".modsync-torrent-")
        .tempdir_in(parent)?;
    let mut attempts = 0;
    loop {
        ensure!(!cancelled.load(Ordering::Relaxed), "Download cancelled");
        attempts += 1;
        if attempts > policy.proxy.max_attempts {
            return Err(PeerUnavailable::NoWorkingProxy.into());
        }
        let Some(proxy) = policy.proxy.next_healthy(&targets) else {
            return Err(PeerUnavailable::NoWorkingProxy.into());
        };
        let runtime = tokio::runtime::Runtime::new()?;
        let end = runtime.block_on(async {
            let session = proxied_session(
                download.path().to_path_buf(),
                proxy,
                &magnet.trackers,
                policy.capped_upload(),
                !policy.upload_while_downloading,
            )
            .await?;
            let result = async {
                let add = match &info {
                    Some(info) => AddTorrent::from_bytes(info.torrent.clone()),
                    None => AddTorrent::from_url(&magnet.uri),
                };
                let handle = session
                    .add_torrent(
                        add,
                        Some(librqbit::AddTorrentOptions {
                            overwrite: true,
                            initial_peers: (!policy.extra_peers.is_empty())
                                .then(|| policy.extra_peers.clone()),
                            ..Default::default()
                        }),
                    );
                // Magnet metadata resolution waits for a peer; bound it.
                let handle = tokio::select! {
                    added = handle => added?.into_handle().context("Torrent did not start")?,
                    _ = wait_for_cancel(cancelled) => anyhow::bail!("Download cancelled"),
                    _ = tokio::time::sleep(policy.stall_timeout) => {
                        return Err(PeerUnavailable::Stalled.into());
                    }
                    _ = watch_proxy(&policy.proxy, proxy, &targets, policy.proxy_recheck_interval) => {
                        return Ok(AttemptEnd::ProxyFailed);
                    }
                };
                let recheck = watch_proxy(
                    &policy.proxy,
                    proxy,
                    &targets,
                    policy.proxy_recheck_interval,
                );
                tokio::pin!(recheck);
                let mut last_bytes = u64::MAX;
                let mut last_change = std::time::Instant::now();
                loop {
                    let stats = handle.stats();
                    if let Some(error) = &stats.error {
                        anyhow::bail!("Torrent failed: {error}");
                    }
                    let total = (stats.total_bytes > 0).then_some(stats.total_bytes);
                    on(PeerProgress::Transferring {
                        bytes: stats.progress_bytes,
                        total,
                        peers: stats.live.as_ref().map(|l| l.snapshot.peer_stats.live),
                        proxy: proxy.address(),
                    });
                    if stats.finished {
                        return Ok(AttemptEnd::Finished);
                    }
                    if stats.progress_bytes != last_bytes {
                        last_bytes = stats.progress_bytes;
                        last_change = std::time::Instant::now();
                    } else if last_change.elapsed() >= policy.stall_timeout {
                        return Err(PeerUnavailable::Stalled.into());
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(pause()) => {}
                        _ = wait_for_cancel(cancelled) => anyhow::bail!("Download cancelled"),
                        _ = &mut recheck => return Ok(AttemptEnd::ProxyFailed),
                    }
                }
            }
            .await;
            session.stop().await;
            result
        });
        drop(runtime);
        match end? {
            AttemptEnd::Finished => break,
            AttemptEnd::ProxyFailed => {
                policy.proxy.mark_bad(proxy);
                on(PeerProgress::ProxyRotated {
                    from: proxy.address(),
                });
            }
        }
    }
    on(PeerProgress::Verifying);
    let file = only_file(download.path())?;
    publish(&file, destination, sha256, cancelled, options)?;
    Ok(sha256.to_ascii_lowercase())
}

/// Resolve when the proxy in use fails a periodic recheck.
async fn watch_proxy(
    pool: &ProxyPool,
    proxy: HealthyProxy,
    targets: &[HealthTarget],
    every: Duration,
) {
    loop {
        tokio::time::sleep(every).await;
        let ok = tokio::task::block_in_place(|| pool.still_healthy(proxy, targets));
        if !ok {
            return;
        }
    }
}

/// When `share_verified` stops. `None` means no limit on that axis.
#[derive(Debug, Clone, Copy)]
pub struct ShareUntil {
    pub max_duration: Option<Duration>,
    pub max_ratio: Option<f64>,
}

impl Default for ShareUntil {
    /// 24 hours or an upload ratio of 1.0, whichever comes first.
    fn default() -> Self {
        Self {
            max_duration: Some(Duration::from_secs(24 * 60 * 60)),
            max_ratio: Some(1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShareStop {
    Ratio,
    Duration,
    Cancelled,
    FileChanged,
    NoWorkingProxy,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "outcome"
)]
pub enum ShareOutcome {
    /// "Keep sharing finished downloads" is off. Nothing was opened.
    SharingOff,
    /// The authored name is not a single safe file name. Nothing was shared.
    UnsafeName { name: String },
    /// The file (or its private copy) does not match the authored SHA-256.
    HashMismatch { expected: String, actual: String },
    /// The torrent built over this file does not have the authored infohash
    /// (another name, piece length or layout). Nothing was shared.
    IdentityMismatch { authored: String, rebuilt: String },
    /// Shared under the authored infohash until `reason`.
    Stopped {
        reason: ShareStop,
        info_hash: String,
        uploaded_bytes: u64,
        seconds: u64,
    },
}

/// Real transport events. Counters are cumulative across proxy rotations in
/// this invocation; preparation, verification and between-session gaps are excluded.
#[derive(Clone, Copy, Debug)]
pub enum ShareEvent {
    Started {
        listen_addr: Option<SocketAddr>,
    },
    Progress {
        uploaded_bytes: u64,
        active: Duration,
    },
}

/// Observed completed-sharing sibling; retains the original public wrapper and
/// outcome schema. Cancellation also covers hashing, copying and identity rebuild.
pub fn share_verified_authored_observed(
    archive: &Path,
    authored: AuthoredTorrent<'_>,
    sha256: &str,
    policy: &PeerPolicy,
    until: ShareUntil,
    cancelled: &AtomicBool,
    observe: impl FnMut(ShareEvent),
) -> Result<ShareOutcome> {
    if !policy.keep_sharing {
        return Ok(ShareOutcome::SharingOff);
    }
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Sharing preparation cancelled"
    );
    // The no-info fallback reproduces the pinned engine's single-file layout,
    // but checks cancellation during reads instead of an unyielding async loop.
    // Its SHA-1 must still equal the authored magnet; it never authorizes a new swarm.
    let rebuilt;
    let authored = if authored.torrent_info.is_none() {
        let magnet = sanitize_magnet(authored.magnet, &[])?;
        let name = magnet
            .name
            .as_deref()
            .or_else(|| archive.file_name()?.to_str())
            .context("Missing archive name")?;
        rebuilt = rebuild_single_info(archive, name, cancelled)?;
        if !format!("{:x}", sha1::Sha1::digest(&rebuilt)).eq_ignore_ascii_case(&magnet.info_hash) {
            return Ok(ShareOutcome::IdentityMismatch {
                authored: magnet.info_hash,
                rebuilt: format!("{:x}", sha1::Sha1::digest(&rebuilt)),
            });
        }
        AuthoredTorrent {
            magnet: authored.magnet,
            torrent_info: Some(&rebuilt),
        }
    } else {
        authored
    };
    share_authored_observed(
        archive,
        authored,
        sha256,
        ShareTransport::Proxied(policy),
        until,
        cancelled,
        observe,
    )
}

fn rebuild_single_info(path: &Path, name: &str, cancelled: &AtomicBool) -> Result<Vec<u8>> {
    ensure!(safe_file_name(name), "Unsafe torrent name");
    const PIECE: usize = 2 * 1024 * 1024;
    let mut file = crate::paths::open_regular_read(path)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut length = 0u64;
    let mut remaining = PIECE;
    let mut hash = sha1::Sha1::new();
    let mut pieces = Vec::new();
    loop {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Sharing identity rebuild cancelled"
        );
        let capacity = remaining.min(buffer.len());
        let read = file.read(&mut buffer[..capacity])?;
        if read == 0 {
            break;
        }
        length += read as u64;
        remaining -= read;
        hash.update(&buffer[..read]);
        if remaining == 0 {
            pieces.extend_from_slice(&hash.finalize_reset());
            remaining = PIECE;
            ensure!(
                pieces.len() <= 32 * 1024,
                "Rebuilt torrent info exceeds size limit"
            );
        }
    }
    // Preserve the pinned librqbit 9.0.1 creator's final-piece behavior, including
    // its empty final checksum at an exact piece boundary. Hash comparison and
    // the engine's subsequent piece recheck still fail closed.
    if remaining > 0 && length > 0 {
        pieces.extend_from_slice(&hash.finalize());
    }
    let mut info = format!(
        "d6:lengthi{length}e4:name{}:{name}12:piece lengthi{PIECE}e6:pieces{}:",
        name.len(),
        pieces.len()
    )
    .into_bytes();
    info.extend_from_slice(&pieces);
    info.push(b'e');
    ensure!(
        info.len() <= 32 * 1024,
        "Rebuilt torrent info exceeds size limit"
    );
    Ok(info)
}

/// How a sharing session reaches peers.
#[derive(Clone, Copy)]
enum ShareTransport<'a> {
    /// Through a health-checked SOCKS5 proxy (the product path).
    Proxied(&'a PeerPolicy),
    /// Direct, with a listener. Only for the explicit `--no-proxy` CLI path.
    Direct {
        upload_bps: Option<NonZeroU32>,
        share_root: &'a Path,
    },
}

/// Share one verified archive under its authored magnet, through the proxy.
/// See `share_verified_authored`.
pub fn share_verified(
    archive: &Path,
    magnet: &str,
    sha256: &str,
    policy: &PeerPolicy,
    until: ShareUntil,
    cancelled: &AtomicBool,
) -> Result<ShareOutcome> {
    share_verified_authored(
        archive,
        AuthoredTorrent {
            magnet,
            torrent_info: None,
        },
        sha256,
        policy,
        until,
        cancelled,
    )
}

/// Share one verified archive under its authored identity, through the proxy.
///
/// Never mints a torrent, and never hands the user's file to the torrent
/// engine. The archive is copied (reflink where possible) into a
/// ModSync-owned folder under the authored name: the `TorrentInfo` name, else
/// the magnet's `dn`, else the local name. The copy must hash to `sha256`.
/// With `TorrentInfo` that dictionary is served as is; otherwise the torrent
/// is rebuilt over the copy and must give the authored infohash. librqbit's
/// recheck must then find every piece. Stops at the first `until` limit, on
/// `cancelled`, or when the user's file changes or disappears; the copy is
/// deleted on every exit. The caller sets `cancelled` before an install
/// reads the file.
pub fn share_verified_authored(
    archive: &Path,
    authored: AuthoredTorrent<'_>,
    sha256: &str,
    policy: &PeerPolicy,
    until: ShareUntil,
    cancelled: &AtomicBool,
) -> Result<ShareOutcome> {
    if !policy.keep_sharing {
        return Ok(ShareOutcome::SharingOff);
    }
    share_authored(
        archive,
        authored,
        sha256,
        ShareTransport::Proxied(policy),
        until,
        cancelled,
        |_| {},
    )
}

fn file_stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

fn share_authored(
    archive: &Path,
    authored: AuthoredTorrent<'_>,
    sha256: &str,
    transport: ShareTransport<'_>,
    until: ShareUntil,
    cancelled: &AtomicBool,
    mut on_started: impl FnMut(Option<SocketAddr>),
) -> Result<ShareOutcome> {
    share_authored_observed(
        archive,
        authored,
        sha256,
        transport,
        until,
        cancelled,
        |event| {
            if let ShareEvent::Started { listen_addr } = event {
                on_started(listen_addr);
            }
        },
    )
}

fn share_authored_observed(
    archive: &Path,
    authored: AuthoredTorrent<'_>,
    sha256: &str,
    transport: ShareTransport<'_>,
    until: ShareUntil,
    cancelled: &AtomicBool,
    mut observe: impl FnMut(ShareEvent),
) -> Result<ShareOutcome> {
    ensure!(archive.is_file(), "Share archive does not exist");
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "A 64-digit SHA-256 is required"
    );
    let (extra, share_root): (&[String], &Path) = match transport {
        ShareTransport::Proxied(policy) => (&policy.trackers, &policy.share_root),
        ShareTransport::Direct { share_root, .. } => (&[], share_root),
    };
    let magnet = sanitize_magnet(authored.magnet, extra)?;
    let info = match authored.torrent_info {
        Some(bytes) => match check_torrent_info(bytes, &magnet.info_hash) {
            Ok(info) => Some(info),
            Err(error) => {
                return Ok(ShareOutcome::IdentityMismatch {
                    authored: magnet.info_hash,
                    rebuilt: format!("TorrentInfo rejected: {error}"),
                });
            }
        },
        None => None,
    };
    let local_name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .context("Share archive needs a UTF-8 file name")?;
    let name = match (&info, &magnet.name) {
        (Some(info), _) => info.name.clone(),
        (None, Some(dn)) => dn.clone(),
        (None, None) => local_name.to_owned(),
    };
    if !safe_file_name(&name) {
        return Ok(ShareOutcome::UnsafeName { name });
    }
    // Baseline of the user's file. It is never opened for writing, so any
    // change after this point is the user's (or an install's).
    let stamp = file_stamp(archive);
    let actual = file_sha256_cancelled(archive, cancelled)?;
    if !actual.eq_ignore_ascii_case(sha256) {
        return Ok(ShareOutcome::HashMismatch {
            expected: sha256.to_ascii_lowercase(),
            actual,
        });
    }
    let Some(copy) = ShareCopy::create_cancelled(archive, &name, share_root, sha256, cancelled)?
    else {
        return Ok(ShareOutcome::HashMismatch {
            expected: sha256.to_ascii_lowercase(),
            actual: "copy differs".to_owned(),
        });
    };
    if cancelled.load(Ordering::Relaxed) {
        return Ok(ShareOutcome::Stopped {
            reason: ShareStop::Cancelled,
            info_hash: magnet.info_hash,
            uploaded_bytes: 0,
            seconds: 0,
        });
    }
    let runtime = tokio::runtime::Runtime::new()?;
    let torrent_bytes: Vec<u8> = match &info {
        Some(info) => info.torrent.clone(),
        None => {
            let created = runtime.block_on(async {
                let spawner = librqbit::spawn_utils::BlockingSpawner::new(1);
                librqbit::create_torrent(
                    &copy.path,
                    librqbit::CreateTorrentOptions {
                        name: Some(&name),
                        trackers: magnet.trackers.clone(),
                        piece_length: None,
                    },
                    &spawner,
                )
                .await
            })?;
            let rebuilt = created.info_hash().as_string();
            if !rebuilt.eq_ignore_ascii_case(&magnet.info_hash) {
                return Ok(ShareOutcome::IdentityMismatch {
                    authored: magnet.info_hash,
                    rebuilt,
                });
            }
            created.as_bytes()?.to_vec()
        }
    };
    let folder = copy.dir.path().to_path_buf();
    let targets = match transport {
        ShareTransport::Proxied(policy) => {
            let targets = HealthTarget::from_sources(&magnet.trackers, &policy.extra_peers);
            if targets.is_empty() {
                return Err(PeerUnavailable::NoPeerSource.into());
            }
            targets
        }
        ShareTransport::Direct { .. } => Vec::new(),
    };
    let tracker_opt = match transport {
        ShareTransport::Proxied(_) => magnet.trackers.clone(),
        ShareTransport::Direct { .. } => magnet.all_trackers.clone(),
    };
    let mut active = Duration::ZERO;
    let mut uploaded_before = 0u64;
    let mut announced = false;
    let mut attempts = 0;
    loop {
        let proxy = match transport {
            ShareTransport::Proxied(policy) => {
                attempts += 1;
                let next = (attempts <= policy.proxy.max_attempts)
                    .then(|| policy.proxy.next_healthy_cancelled(&targets, cancelled))
                    .flatten();
                match next {
                    Some(proxy) => Some(proxy),
                    None if !announced => return Err(PeerUnavailable::NoWorkingProxy.into()),
                    None => {
                        return Ok(ShareOutcome::Stopped {
                            reason: ShareStop::NoWorkingProxy,
                            info_hash: magnet.info_hash,
                            uploaded_bytes: uploaded_before,
                            seconds: active.as_secs(),
                        });
                    }
                }
            }
            ShareTransport::Direct { .. } => None,
        };
        let announced = &mut announced;
        let observe = &mut observe;
        let mut transport_started = None;
        let mut last_uploaded = 0u64;
        let mut final_handle = None;
        let end = runtime.block_on(async {
            let session = match (transport, proxy) {
                (ShareTransport::Proxied(policy), Some(proxy)) => {
                    proxied_session(
                        folder.clone(),
                        proxy,
                        &magnet.trackers,
                        policy.capped_upload(),
                        false,
                    )
                    .await?
                }
                (ShareTransport::Direct { upload_bps, .. }, _) => {
                    direct_session(folder.clone(), upload_bps, Some(sharing_peer_options()))
                        .await?
                }
                (ShareTransport::Proxied(_), None) => anyhow::bail!("No proxy"),
            };
            let result = async {
                let handle = session
                    .add_torrent(
                        AddTorrent::from_bytes(torrent_bytes.clone()),
                        Some(librqbit::AddTorrentOptions {
                            paused: true,
                            overwrite: true,
                            output_folder: Some(folder.to_string_lossy().into_owned()),
                            trackers: Some(tracker_opt.clone()),
                            initial_peers: match transport {
                                ShareTransport::Proxied(policy)
                                    if !policy.extra_peers.is_empty() =>
                                {
                                    Some(policy.extra_peers.clone())
                                }
                                _ => None,
                            },
                            ..Default::default()
                        }),
                    )
                    .await?
                    .into_handle()
                    .context("Torrent did not start")?;
                final_handle = Some(handle.clone());
                // The paused add still rechecks the copy. Share only if every
                // piece is already present, so nothing is ever downloaded.
                loop {
                    let stats = handle.stats();
                    if let Some(error) = &stats.error {
                        anyhow::bail!("Torrent failed: {error}");
                    }
                    if !matches!(stats.state, librqbit::TorrentStatsState::Initializing { .. }) {
                        ensure!(
                            stats.finished && stats.progress_bytes == stats.total_bytes,
                            "The archive does not match the authored torrent pieces"
                        );
                        break;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(pause()) => {}
                        _ = wait_for_cancel(cancelled) => return Ok((ShareStop::Cancelled, 0, false)),
                    }
                }
                ensure!(!cancelled.load(Ordering::Relaxed), "Sharing preparation cancelled");
                session.unpause(&handle).await?;
                transport_started = Some(std::time::Instant::now());
                if !*announced {
                    *announced = true;
                    observe(ShareEvent::Started { listen_addr: session.listen_addr().map(|a| local_peer(a.port())) });
                }
                let total = handle.stats().total_bytes.max(1);
                let recheck = async {
                    match (transport, proxy) {
                        (ShareTransport::Proxied(policy), Some(proxy)) => {
                            watch_proxy(
                                &policy.proxy,
                                proxy,
                                &targets,
                                policy.proxy_recheck_interval,
                            )
                            .await
                        }
                        _ => std::future::pending().await,
                    }
                };
                tokio::pin!(recheck);
                let mut uploaded = 0;
                let mut ticks = 0u32;
                loop {
                    let stats = handle.stats();
                    uploaded = uploaded.max(stats.uploaded_bytes);
                    last_uploaded = uploaded;
                    let total_uploaded = uploaded_before.saturating_add(uploaded);
                    let elapsed = active + transport_started.map_or(Duration::ZERO, |started: std::time::Instant| started.elapsed());
                    observe(ShareEvent::Progress { uploaded_bytes: total_uploaded, active: elapsed });
                    if let Some(limit) = until.max_ratio
                        && total_uploaded as f64 / total as f64 >= limit
                    {
                        return Ok((ShareStop::Ratio, uploaded, false));
                    }
                    if let Some(limit) = until.max_duration
                        && elapsed >= limit
                    {
                        return Ok((ShareStop::Duration, uploaded, false));
                    }
                    ticks += 1;
                    if ticks.is_multiple_of(10) && file_stamp(archive) != stamp {
                        return Ok((ShareStop::FileChanged, uploaded, false));
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(pause()) => {}
                        _ = wait_for_cancel(cancelled) => return Ok((ShareStop::Cancelled, uploaded, false)),
                        _ = &mut recheck => return Ok((ShareStop::NoWorkingProxy, uploaded, true)),
                    }
                }
            };
            let result = tokio::select! {
                result = result => result,
                _ = wait_for_cancel(cancelled) => Ok((ShareStop::Cancelled, 0, false)),
            };
            session.stop().await;
            if let Some(handle) = final_handle { last_uploaded = last_uploaded.max(handle.stats().uploaded_bytes); }
            if let Some(started) = transport_started {
                active += started.elapsed();
            }
            observe(ShareEvent::Progress { uploaded_bytes: uploaded_before.saturating_add(last_uploaded), active });
            result.map(|(reason, uploaded, rotate)| (reason, uploaded.max(last_uploaded), rotate))
        });
        let (reason, uploaded, rotate) = end?;
        uploaded_before += uploaded;
        if rotate && let (ShareTransport::Proxied(policy), Some(proxy)) = (transport, proxy) {
            policy.proxy.mark_bad(proxy);
            continue;
        }
        drop(runtime);
        drop(copy);
        return Ok(ShareOutcome::Stopped {
            reason,
            info_hash: magnet.info_hash,
            uploaded_bytes: uploaded_before,
            seconds: active.as_secs(),
        });
    }
}

/// The `Peer` acquisition source. The orchestrator wraps these inherent
/// methods in its `Acquire` implementation.
pub struct PeerSource {
    policy: PeerPolicy,
}

impl PeerSource {
    pub fn new(policy: PeerPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> &PeerPolicy {
        &self.policy
    }

    /// Fetch through the proxy. See `fetch_authored`.
    pub fn acquire(
        &self,
        authored: AuthoredTorrent<'_>,
        sha256: &str,
        destination: &Path,
        cancelled: &AtomicBool,
        options: &crate::archive::Options,
        on: impl FnMut(PeerProgress),
    ) -> Result<String> {
        fetch_authored(
            authored,
            destination,
            sha256,
            &self.policy,
            cancelled,
            options,
            on,
        )
    }

    /// Share a verified archive under its authored identity. See
    /// `share_verified_authored`.
    pub fn share(
        &self,
        archive: &Path,
        authored: AuthoredTorrent<'_>,
        sha256: &str,
        until: ShareUntil,
        cancelled: &AtomicBool,
    ) -> Result<ShareOutcome> {
        share_verified_authored(archive, authored, sha256, &self.policy, until, cancelled)
    }
}

// ---------------------------------------------------------------------------
// CLI entry points for `torrent-download` and `torrent-seed`.
// ---------------------------------------------------------------------------

/// Proxy and list options shared by the torrent CLI verbs.
#[derive(Debug, Clone, Default)]
pub struct CliProxyOptions {
    /// `--no-proxy`: the direct legacy path. Peers see the user's IP.
    pub no_proxy: bool,
    /// `--peer-proxy`: explicit SOCKS5 endpoints.
    pub proxies: Vec<String>,
    /// `--peer-proxy-list-url`. `None` means the default list; empty disables.
    pub proxy_list_url: Option<String>,
    /// `--tracker-list-url`. Empty means the default ngosang lists.
    pub tracker_list_urls: Vec<String>,
    /// `--upload-kibps`. `None` means the default 100 KiB/s.
    pub upload_kibps: Option<u32>,
    /// `--no-upload-while-downloading`.
    pub no_upload_while_downloading: bool,
    /// `--share-hours`: stop sharing an authored archive after this long.
    /// `None` means 24; 0 means no limit.
    pub share_hours: Option<f64>,
    /// `--share-ratio`: stop at this upload ratio. `None` means 1.0; 0 means
    /// no limit.
    pub share_ratio: Option<f64>,
}

fn cli_cache_dir() -> Option<PathBuf> {
    crate::profiles::settings_directory(None)
        .ok()
        .map(|d| d.join("peer-cache"))
}

impl CliProxyOptions {
    fn upload_bps(&self) -> Option<NonZeroU32> {
        let kibps = self.upload_kibps.unwrap_or(DEFAULT_UPLOAD_KIBPS);
        NonZeroU32::new(kibps.saturating_mul(1024).max(MIN_UPLOAD_BPS))
    }

    /// The plan's stop rules (24 h, ratio 1.0), overridable; 0 disables one.
    fn share_until(&self) -> ShareUntil {
        let defaults = ShareUntil::default();
        ShareUntil {
            max_duration: match self.share_hours {
                None => defaults.max_duration,
                Some(h) if h > 0.0 => Some(Duration::from_secs_f64(h * 3600.0)),
                Some(_) => None,
            },
            max_ratio: match self.share_ratio {
                None => defaults.max_ratio,
                Some(r) if r > 0.0 => Some(r),
                Some(_) => None,
            },
        }
    }

    fn policy(
        &self,
        extra_trackers: Vec<String>,
        extra_peers: Vec<SocketAddr>,
    ) -> Result<PeerPolicy> {
        let mut proxies = Vec::new();
        for proxy in &self.proxies {
            proxies.push(parse_proxy_line(proxy).with_context(|| {
                format!("--peer-proxy must be socks5://IP:PORT with a literal IP: {proxy}")
            })?);
        }
        let list_url = match &self.proxy_list_url {
            None => Some(DEFAULT_PROXY_LIST_URL.to_owned()),
            Some(url) if url.trim().is_empty() => None,
            Some(url) => Some(url.clone()),
        };
        let list_urls: Vec<String> = if self.tracker_list_urls.is_empty() {
            DEFAULT_TRACKER_LIST_URLS.map(str::to_owned).to_vec()
        } else {
            self.tracker_list_urls
                .iter()
                .filter(|u| !u.trim().is_empty())
                .cloned()
                .collect()
        };
        let cache = cli_cache_dir();
        let mut trackers = extra_trackers;
        trackers.extend(fetch_tracker_lists(&list_urls, cache.as_deref()));
        Ok(PeerPolicy {
            proxy: ProxyPool::new(proxies, list_url, cache),
            upload_while_downloading: !self.no_upload_while_downloading,
            keep_sharing: true,
            upload_bps: self.upload_bps(),
            trackers: http_trackers(trackers),
            extra_peers,
            stall_timeout: Duration::from_secs(120),
            proxy_recheck_interval: Duration::from_secs(15),
            share_root: default_share_root(),
        })
    }
}

/// Return a flag that is set on SIGINT or SIGTERM (Ctrl+C, Ctrl+Break or
/// console close on Windows), so sessions stop cleanly and temp and share
/// folders are removed. The handlers are registered before this returns, so
/// call it before creating anything that needs cleanup.
///
/// Public so a binary entry point (for example a CLI command that owns its
/// own cancellation flag) can install the same signal handling once, instead
/// of duplicating signal registration.
pub fn stop_signal_flag() -> Result<std::sync::Arc<AtomicBool>> {
    let flag = std::sync::Arc::new(AtomicBool::new(false));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    #[cfg(unix)]
    let wait = {
        use tokio::signal::unix::{SignalKind, signal};
        let _guard = runtime.enter();
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut terminate = signal(SignalKind::terminate())?;
        async move {
            tokio::select! {
                _ = interrupt.recv() => {}
                _ = terminate.recv() => {}
            }
        }
    };
    #[cfg(windows)]
    let wait = {
        use tokio::signal::windows;
        let _guard = runtime.enter();
        let mut c = windows::ctrl_c()?;
        let mut brk = windows::ctrl_break()?;
        let mut close = windows::ctrl_close()?;
        async move {
            tokio::select! {
                _ = c.recv() => {}
                _ = brk.recv() => {}
                _ = close.recv() => {}
            }
        }
    };
    #[cfg(not(any(unix, windows)))]
    let wait = {
        let _guard = runtime.enter();
        async move {
            let _ = tokio::signal::ctrl_c().await;
        }
    };
    let set = flag.clone();
    std::thread::spawn(move || {
        runtime.block_on(wait);
        set.store(true, Ordering::Relaxed);
    });
    Ok(flag)
}

fn decode_cli_info(torrent_info: Option<&str>) -> Result<Option<Vec<u8>>> {
    torrent_info
        .map(|text| base64_decode(text).context("--torrent-info must be base64"))
        .transpose()
}

/// `torrent-download`: proxied by default, direct only with `--no-proxy`.
/// `torrent_info` is the authored base64 info dictionary, if any.
pub fn cli_download(
    magnet: &str,
    torrent_info: Option<&str>,
    destination: &Path,
    sha256: &str,
    peers: Vec<SocketAddr>,
    proxy: &CliProxyOptions,
    options: &crate::archive::Options,
) -> Result<()> {
    let info = decode_cli_info(torrent_info)?;
    if proxy.no_proxy {
        let checked = match &info {
            Some(bytes) => Some(check_torrent_info(
                bytes,
                &sanitize_magnet(magnet, &[])?.info_hash,
            )?),
            None => None,
        };
        return fetch_direct(
            magnet,
            checked.as_ref(),
            destination,
            sha256,
            peers,
            &AtomicBool::new(false),
            options,
        );
    }
    let policy = proxy.policy(Vec::new(), peers)?;
    let cancelled = stop_signal_flag()?;
    fetch_authored(
        AuthoredTorrent {
            magnet,
            torrent_info: info.as_deref(),
        },
        destination,
        sha256,
        &policy,
        &cancelled,
        options,
        |_| {},
    )?;
    Ok(())
}

/// `torrent-seed`. With `--authored-magnet` it shares under that identity and
/// never mints a torrent. Without it, it mints one for a maintainer to author.
/// Proxied by default; `--no-proxy` opens a listener. Every path serves a
/// private copy, never the user's file.
pub fn cli_seed(
    archive: &Path,
    trackers: Vec<String>,
    authored_magnet: Option<&str>,
    torrent_info: Option<&str>,
    proxy: &CliProxyOptions,
) -> Result<()> {
    let info = decode_cli_info(torrent_info)?;
    let until = proxy.share_until();
    match (authored_magnet, proxy.no_proxy) {
        (None, true) => {
            ensure!(info.is_none(), "--torrent-info needs --authored-magnet");
            seed(archive, trackers).map(|_| ())
        }
        (None, false) => {
            ensure!(info.is_none(), "--torrent-info needs --authored-magnet");
            seed_minted_proxied(archive, trackers, proxy)
        }
        (Some(magnet), no_proxy) => {
            // Handlers first, so a signal during the copy still cleans up.
            let cancelled = stop_signal_flag()?;
            ensure!(archive.is_file(), "Seed archive does not exist");
            crate::archive::verify(
                archive,
                archive,
                &crate::archive::Options::default(),
                &AtomicBool::new(false),
            )?;
            let sha256 = file_sha256(archive)?;
            let policy;
            let share_root = default_share_root();
            let transport = if no_proxy {
                ShareTransport::Direct {
                    upload_bps: proxy.upload_bps(),
                    share_root: &share_root,
                }
            } else {
                policy = proxy.policy(trackers, Vec::new())?;
                ShareTransport::Proxied(&policy)
            };
            let magnet_text = magnet.to_owned();
            let sha = sha256.clone();
            let outcome = share_authored(
                archive,
                AuthoredTorrent {
                    magnet,
                    torrent_info: info.as_deref(),
                },
                &sha256,
                transport,
                until,
                &cancelled,
                |peer| {
                    // Same line as legacy `torrent-seed`, printed once sharing starts.
                    if let Ok(line) = serde_json::to_string(&SeedInfo {
                        magnet: magnet_text.clone(),
                        sha256: sha.clone(),
                        peer: peer.map(|p| p.to_string()),
                        torrent_info: None,
                    }) {
                        println!("{line}");
                    }
                },
            )?;
            println!("{}", serde_json::to_string(&outcome)?);
            match outcome {
                ShareOutcome::Stopped { .. } => Ok(()),
                other => anyhow::bail!("Not shared: {}", serde_json::to_string(&other)?),
            }
        }
    }
}

fn seed_minted_proxied(
    archive: &Path,
    trackers: Vec<String>,
    proxy: &CliProxyOptions,
) -> Result<()> {
    ensure!(archive.is_file(), "Seed archive does not exist");
    crate::archive::verify(
        archive,
        archive,
        &crate::archive::Options::default(),
        &AtomicBool::new(false),
    )?;
    let sha256 = file_sha256(archive)?;
    let cancelled = stop_signal_flag()?;
    let policy = proxy.policy(trackers, Vec::new())?;
    let targets = HealthTarget::from_sources(&policy.trackers, &[]);
    if targets.is_empty() {
        return Err(PeerUnavailable::NoPeerSource.into());
    }
    let proxy = policy
        .proxy
        .next_healthy(&targets)
        .ok_or(PeerUnavailable::NoWorkingProxy)?;
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .context("Seed archive needs a UTF-8 file name")?;
    let copy = ShareCopy::create(archive, name, &policy.share_root, &sha256)?
        .context("Seed archive changed while it was copied")?;
    ensure!(!cancelled.load(Ordering::Relaxed), "Seeding cancelled");
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let session = proxied_session(
            copy.dir.path().to_path_buf(),
            proxy,
            &policy.trackers,
            policy.capped_upload(),
            false,
        )
        .await?;
        let (torrent, _) = session
            .create_and_serve_torrent(
                &copy.path,
                librqbit::CreateTorrentOptions {
                    trackers: policy.trackers.clone(),
                    ..Default::default()
                },
            )
            .await?;
        println!(
            "{}",
            serde_json::to_string(&SeedInfo {
                magnet: torrent.as_magnet().to_string(),
                sha256: sha256.clone(),
                peer: None,
                torrent_info: Some(base64_encode(torrent.as_info().info.raw_bytes.as_ref())),
            })?
        );
        wait_for_cancel(&cancelled).await;
        session.stop().await;
        Ok::<_, anyhow::Error>(())
    })
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
