# Peer-assisted mod acquisition plan

Status: design only. Nothing in this plan is implemented. Revised 2026-09-27 for the user's default-on direction, lead rulings (a)–(f), and the user's no-external-dependency override (transport: librqbit SOCKS5). It was written read-only against `codex/rust-port` at `cd8f0b07d994f2833e57327118c9919ecf3da58d`. The pinned C# oracle is `d31ccbd3bfcddb4705917eafa6483715c166de3b`, and C# master has no P2P. This is a Wizard-authored feature, not a parity item. It depends on the shared acquisition pipeline owned by the download-orchestration slice (`docs/parity/download-orchestration-plan.md`, being written by that slice). The interface in the "Agreed acquisition-source interface" section below was negotiated with that slice and has converged.

## Product source and how it was read

The product decisions come from Wizard's messages in `Direct Messages - Sniggles [1106611389402972160].json` under `boden_wizard_chat_context/archive/originals/discord/discord_exports/discord_dms/`. Line numbers below are export line numbers of each message's `content` field. The export was treated as data. Proposals and compromises from Sniggles/Doug were not implemented; they appear only where a Wizard line answers them. Checkpoint/backup ideas (for example the storage analysis at 89737) are excluded.

Wizard's position over time, from message timestamps:

| Date | Line | Wizard's position |
|---|---|---|
| 2023-06-03 | 16653, 16728 | P2P "wouldn't be a reupload" |
| 2023-06-03 | 16778, 16828 | Download only what is permitted; a "download available mods" button is worth it even at 30 of 80 |
| 2024-03-07 | 35075 | Other AIO installers ship magnet lists, with version info bound to each archive |
| 2025-10-14 | 88232 | Nexus rate-limits free users heavily |
| 2025-10-14 | 90448 | Proposes P2P to fix the spoiler-free issue and the Nexus download issue: faster downloads, rate limiting stops mattering, no premium purchase |
| 2025-10-14 | 90548, 90573, 90598 | User B downloads from user A while A is still downloading, "in addition to the download links" |
| 2025-10-14 | 90648 | No reupload happens anywhere |
| 2025-10-14 | 86988 | The app's downloader works in the background, so spoiler-avoiding download pages don't apply |
| 2025-10-27 | 97025 | "So how does p2p violate this again?" (the reupload rule) |
| 2026-04-14 | C# `97b00c48`, `93c74e57` | Wizard removes his earlier hidden MonoTorrent "distributed cache" from C# master |
| 2026-04-21/22 | 114891, 114916, 116381 | Community licensing: credit OpenKotOR and the creator, share-alike. Goal: stop needing per-author permission for ModSync |

## Wizard decisions this plan implements

1. **Peers supplement host links. They never replace them.** The host link stays the canonical source. When P2P is on, a peer transfer runs alongside the host download, and the first verified copy wins. Referents: 90573 ("in addition to the download links"); 90448 (the goals are speed and making Nexus rate limiting unimportant, and neither is met if peers only run after a throttled host succeeds). Wizard's own earlier C# code raced the two paths with `Task.WhenAny`: `97b00c48^:src/KOTORModSync.Core/Services/Download/DownloadCacheOptimizer.cs` about lines 78–106.
2. **Content identity is the archive's SHA-256, and a peer copy is used only when it matches.** Referent: 35075 (each mod's version is pinned in the archive). This is already enforced by `src/p2p.rs:83-86` (64-hex SHA-256 required) and `src/p2p.rs:192-196` (hash, then `archive::verify`, then `persist_noclobber`).
3. **Sharing is not reuploading.** Peers exchange the exact bytes each user is already fetching from the host, and nothing is posted to a new host. Referents: 90498, 90648, 16728, 97025. The plan therefore needs no new hosting, mirror, or upload service. It only needs a guide field.
4. **Eligibility is authored per resource.** A resource is peer-eligible only when its guide entry carries an authored `ContentHashSHA256` and `Magnet`. There is no global "share everything" switch. This fits every Wizard position on the timeline: permitted-only (16778, 16828), not-a-reupload (90648), and community licensing (114916, 116381) under which maintainers can mark resources whose terms allow it. The maintainer decides by authoring the fields, and removing the fields withdraws the resource.
5. **Downloads run in the background, with no host page.** A peer transfer fetches by hash and never opens a spoiler-bearing host page. Referents: 86988, 90448 (the "spoiler-free issue"). Censored or alternative spoiler-free archives are out of scope. They are different bytes, so they could only be shared under their own authored hash.
6. **Nexus throttling is a reason to race, not a failure mode.** Referents: 88232, 90448. A slow Origin does not block the Peer source.
7. **Rejected from Wizard's earlier C# design:** the "Phase 0 Stealth and Inconspicuousness" rules (`97b00c48^:docs/distributed-file-resilience.plan.md` lines 12–45: neutral jargon, base64-obfuscated type names). Wizard removed them himself in `97b00c48`/`93c74e57`. Sharing is now on by default (ruling (a) below), but it is never hidden: every Rust surface names peer sharing plainly, and the user can see it and turn it off. The C# numbers that are still useful are reused below with their referents.

## Lead rulings (binding, 2026-09-27)

These replace the earlier opt-in stance. The user overrode it: peer sharing is on by default, with no opt-in prompt and no prominent UI, and peers must not see the user's IP address.

- **(a) Default on, never hidden.** Peer download and peer upload are both on by default. There is no opt-in prompt. The plan keeps three things:
  - a discoverable setting that turns sharing off;
  - a one-line disclosure in Settings and About;
  - no obfuscation or stealth naming.
  Silently uploading from users' machines with no way to see or stop it would deceive them.
- **(b) Per-resource eligibility stays.** Only resources with an authored `ContentHashSHA256` and `Magnet` are ever fetched from or shared with peers.
- **(c) Nothing system-wide, and no external networking dependency (user override).** The privacy transport is librqbit's own in-process SOCKS5 support. No downloaded binary, sidecar, driver, service, TUN adapter, admin prompt or system VPN (not the system `warp-svc`).
- **(d) Fail closed.** Peer steps open sockets only through a health-checked SOCKS5 proxy. When it is down, Peer steps are skipped (`SourceFailed { will_fallback: true }`), no peer socket is opened, and Origin runs as normal. No IP is exposed.
- **(e) No leak outside the proxy.** On the acquisition path the incoming listener, UPnP, DHT, UDP trackers and local service discovery (LSD) are all off. Each one reaches peers or the LAN without going through the proxy.
- **(f) Gated targets never fall back to Peer (R2 from the orchestration plan).** `ManualReason::{LoginRequired, Captcha, FreeUserWait, HtmlLanding}` are terminal, even when Origin finds the gate mid-run. `Unsupported` (MEGA, pre-1c DeadlyStream, GameFront) may fall back to Peer, but only for share-eligible resources.

## Binding constraints

- Do not edit `AGENTS.md`, `src/ui.rs`, or `src/ui/installing_page.rs`. They are user-owned and dirty. GUI wiring waits for an explicit release (see File ownership).
- The user has frozen Rust tests. Do not create, modify, or run Rust tests of any kind. Preserve the inline `#[cfg(test)]` block in `src/p2p.rs` (lines 210–223, `torrent_payload_must_be_one_archive`) byte-for-byte. Verification uses production binaries and direct probes only.
- Do not add a second downloader. P2P is one `Acquire` implementation inside the shared orchestrator. The CLI and GUI render shared facts and infer no source, verification, or completion facts themselves.
- P2P is on by default (ruling (a)) and fails closed without a working SOCKS5 proxy (ruling (d)). No direct peer socket, listener, DHT packet, UDP tracker packet, LSD packet or UPnP request is ever made on the acquisition path. The off-setting closes every peer socket.
- Never publish unverified bytes. Every peer copy passes the authored SHA-256, then `archive::verify`, then `persist_noclobber` before anything can read it. Nothing from a peer is executed, extracted, or installed before publish.
- Guides may use only the `<<modDirectory>>`/`<<kotorDirectory>>` placeholders. Guide-authored peer addresses and trackers are ignored by default (see Security).
- Keep the `torrent-download` and `torrent-seed` CLI verbs (`src/main.rs:201-221`, `:843-863`). **Superseded by the user's direction (2026-09-27): both verbs are proxied by default**, because sharing is on by default and the user's IP must stay hidden from peers. `--no-proxy` is the explicit direct path (listener, DHT, UPnP; its help says peers will see your IP), and it keeps the old output. Proxied mode adds output: `torrent-seed` prints `peer: null`, the proxied mint adds `torrentInfo`, and `--authored-magnet` prints a second `ShareOutcome` line (`outcome`, `reason`, `infoHash`, `uploadedBytes`, `seconds`). With no working proxy the proxied verbs exit 1 (fail closed).

## Source findings that govern the design

- **Rust transport.** `src/p2p.rs` has `fetch` (`:73`) and `seed` (`:21`). `fetch` requires a magnet URI and a 64-hex SHA-256, refuses an existing destination, downloads into `.modsync-torrent-*` under the destination parent, requires exactly one file (`only_file`, `:156`), then hashes, verifies, and publishes no-clobber (`:172-200`). The session enables UPnP port forwarding unconditionally (`:129`), with IPv4 only. DHT is on because `SessionOptions` is left at defaults (librqbit 9.0.1 `session.rs:419-421`: "Set to None to disable DHT entirely"). `fetch` stops its session on completion (`:111`), so today nothing is shared after the download finishes. No progress is reported.
- **Seeding creates a new torrent.** `seed` calls `create_and_serve_torrent` (`:39-47`). In librqbit 9.0.1, `create_torrent_file.rs:18-22,50-53,212-240` builds the info dict from the file name (default), a fixed 2 MiB piece length, and the bytes, with no creation date. The infohash is deterministic for identical bytes, name, and piece length, but a different local filename or piece length gives a different infohash. Any seeding of a guide resource must therefore reuse the authored magnet over the verified file. It must never mint a new one.
- **Rust direct download.** `src/download.rs:26-76` has the same temp, hash, verify, no-clobber contract. It computes the SHA-256 even when none is expected (`:56-67`) and then discards it.
- **Queue.** `src/download_queue.rs:15` `Job {url, destination, sha256}`; `:55` `run_with` is generic over the executor.
- **Resource registry.** `Component.resource_registry` is a raw `serde_json::Map` (`src/model.rs:56`), so new per-URL keys round-trip without a struct change. `src/resources.rs` writes only `{ContentKey, MetadataHash, Files, HandlerMetadata, FileSize}`. `ContentKey`/`MetadataHash` there are a SHA-1 of the normalized URL, not of archive bytes. Referent: `grep -n -i 'sha256|magnet' src/model.rs src/resources.rs` returns nothing.
- **C# oracle.** `ResourceMetadata` (`src/ModSync.Core/ModComponent.cs:2330-2370`) has `ContentKey`, `MetadataHash` (SHA-256 of provider metadata), `HandlerMetadata`, `Files`, `FileSize`, `FirstSeen`, `LastVerified`, and version fields. It has no byte hash. `93c74e57` removed `ContentId`, `ContentHashSHA256`, `PieceLength`, `PieceHashes`, and `TrustLevel`. This plan reuses the name `ContentHashSHA256` for alignment. It does not reuse `ContentId`/`PieceHashes`, because the magnet already carries the infohash and librqbit verifies pieces.
- **Settings store.** `profiles::Store` persists `settings.json` keys with `recovery::atomic_write` (`src/profiles.rs:306-323`, the `nexusModsApiKey` pattern). Peer settings use the same store.
- **GUI today.** It has a single manual direct download (`src/ui.rs:2352-2368`) and host hyperlinks per selected component (`:505`, `:1266`). It has no batch acquisition UI; the orchestration slice owns that.

## Guide metadata (where hashes and magnets come from)

Per resource URL key in `ResourceRegistry`:

| Key | Type | Meaning |
|---|---|---|
| `ContentHashSHA256` | 64-hex string | SHA-256 of the exact archive bytes the host link serves. Required for any peer use. |
| `Magnet` | `magnet:?xt=urn:btih:…` | Authored magnet whose single file is that archive. Its `dn` should equal the archive filename. |
| `TorrentInfo` | base64 string | Standard base64 of the torrent's bencoded info dictionary, as printed by `torrent-seed` (`torrentInfo`). Optional but recommended. Its SHA-1 must equal the magnet's btih, and it must describe one file with a safe name and no `private` flag; otherwise it is ignored with a warning. With it, clients add the torrent from metadata (librqbit's add-from-bytes path), so the torrent is in the session before any peer connects, and sharing serves this exact dictionary. A `.torrent` URL was rejected: it would be a guide-authored endpoint every client dials. |
| `Peers` | array of strings | Reserved. Preserved on round-trip and **ignored**; P2 may only warn that it is ignored. A guide author must not be able to pick endpoints that clients dial. |

How the values are produced: the guide maintainer downloads from the canonical host, runs the existing `modsync torrent-seed <archive>`, and copies the printed `{magnet, sha256}` (`src/p2p.rs:52-59`) into the entry. Editor/CLI support to write these fields belongs to a later task. `download-plan` output is unchanged.

Discovery without trust: after an Origin download, `Completed { sha256 }` carries the computed hash. ModSync may write it only to a local, per-user `observed-hashes.json` under the profile store, and may show it to maintainers as a proposal. It never makes a resource peer-eligible, and it is never sent anywhere. Only guide-authored values enable peers. Referent for why: `ContentKey`/`MetadataHash` are not byte identities (see findings), and an unauthenticated hash learned from peers would let a poisoned swarm self-certify.

Why `TorrentInfo` exists (P1 follow-up, 2026-09-27): librqbit 9.0.1 only accepts an inbound peer for a torrent that is already in its session table (`session.rs:943-955`), and a magnet-only torrent enters that table only after its metadata has been fetched from a peer. A proxied seeder can only dial out, so a magnet-only leecher rejected it, and two ModSync users could never meet. Adding from the authored info dictionary removes that step. Sharing also uses it: the verified archive is copied (reflink where possible) into a ModSync-owned folder under the dictionary's file name, and that copy is served, because librqbit opens served files read-write (`storage/filesystem/fs.rs:161`). The user's file is never handed to the engine. Receipt: `/tmp/p2p-privacy-probe/p1/RECEIPT.md`.

Validation: a malformed hash or magnet is a warning, and that resource stays Origin-only. A magnet whose infohash disagrees with a seed probe is also a warning (see probe 3). An entry that has a hash and a `Files` map showing multiple archives is rejected for peer use, because torrents must hold exactly one archive (`src/p2p.rs:165-168`).

## Agreed acquisition-source interface

This was negotiated with audit-download-orchestration and converged in round 2. They own `src/acquisition.rs`, the orchestrator, and the CLI/GUI adapters. This slice owns the `Peer` implementation of `Acquire` in `src/p2p.rs`.

```rust
pub enum SourceKind { Existing, Origin(download_hosts::Provider), Peer, Manual }

pub enum Source {
    Existing { path: PathBuf },
    Origin   { url: String, provider: download_hosts::Provider },
    Peer     { magnet: String, sha256: String, peers: Vec<PeerEndpoint> }, // only with authored sha256+magnet; peers from user settings only
    Manual   { url: String, reason: ManualReason }, // LoginRequired | Captcha | HtmlLanding | FreeUserWait | Unsupported
}

// A peer the proxy can reach. librqbit's SOCKS path takes a SocketAddr (stream_connect.rs:88-110).
// Only ever dialed through the SOCKS5 proxy, never directly; never read from guides.
pub struct PeerEndpoint(pub SocketAddr);

pub struct ArchiveTarget {
    pub component_guid: String,
    pub registry_url: String,
    pub filename: String,
    pub expected_sha256: Option<String>,
    pub sources: Vec<Source>, // already in fallback order
}

pub trait Acquire {
    fn kind(&self) -> SourceKind;
    fn acquire(&self, target: &ArchiveTarget, dest: &Path, cancel: &AtomicBool,
               on: &mut dyn FnMut(AcquisitionEvent)) -> Result<()>;
}

pub struct AcquisitionEvent {
    pub target_index: usize, pub component_guid: String, pub filename: String,
    pub source: SourceKind, pub phase: Phase,
}

pub enum Phase {
    Resolving,
    Transferring { bytes: u64, total: Option<u64>, peers: Option<u32> }, // peers None for Origin
    Verifying,
    Completed { path: PathBuf, sha256: String },
    Skipped { reason: String },
    SourceFailed { error: String, will_fallback: bool },
    Superseded,
    ManualRequired { url: String, reason: ManualReason },
    Cancelled,
}
```

Order per target, with setting `peer_mode: Off | Race | Fallback`:

1. `Existing`: a local file that already satisfies the plan.
2. If `peer_mode = Race`, a working SOCKS5 proxy is available, and the target has an authored `ContentHashSHA256` and `Magnet`, the automatable `Origin` and `Peer` race. Otherwise `Origin` runs alone.
3. `Peer` runs alone only with an authored hash and magnet and a working proxy, and only when:
   - `Origin` failed with a non-gate error (network, 5xx, verify or hash failure); or
   - the target's only other source is `Manual(Unsupported)`.
4. `Manual` is terminal. It is never automated around (no CAPTCHA or login bypass).
   - `LoginRequired`, `Captcha`, `FreeUserWait` and `HtmlLanding` end the target without trying Peer. This holds even when Origin discovers the gate at run time. A peer copy must not get around a host gate.
   - `Unsupported` ends the target only when it is not share-eligible.

`peer_mode` defaults:
- `Race` when a working SOCKS5 proxy is available. There is no opt-in step.
- Effectively `Off` while no proxy works: Peer steps fail closed and open no sockets.
- `Off` when the user turns sharing off. `Fallback` stays available as a setting.

Race mechanics: each source gets a child `AtomicBool` linked to the parent cancel, plus its own temp in the destination parent. The orchestrator marks the **first source to return `Ok`** as the winner. It sets the loser's child flag, **joins the loser before returning**, and reports the loser as `Superseded` whatever its error text says (no parsing of error strings). The C# re-await "wait briefly" at `DownloadCacheOptimizer.cs:~108` was a no-op and is not copied. Speed, ETA, and "from N peers" wording are presentation. Byte counts and peer counts are facts.

## Peer implementation (this slice, `src/p2p.rs`)

Additive API. `fetch` and `seed` keep their signatures and behavior for the CLI.

```rust
pub struct PeerPolicy {
    pub proxy: ProxyPool,                 // explicit socks5 endpoints + list-URL candidates, health-checked; empty = no peer session
    pub upload_while_downloading: bool,   // default true (ruling (a); needed for 90548)
    pub upload_bps: Option<NonZeroU32>,   // default 100 KiB/s
    pub trackers: Vec<String>,            // http(s) only: magnet tr= + ngosang/trackerslist http(s) + user-entered; udp:// dropped
    pub extra_peers: Vec<PeerEndpoint>,   // user-entered only
    // Not configurable on the acquisition path: listener, UPnP, LSD, DHT and UDP trackers are off.
}

pub struct PeerSource { policy: PeerPolicy /* + session handle */ }
impl Acquire for PeerSource { /* Peer only */ }

pub fn fetch_observed(magnet, destination, sha256, policy: &PeerPolicy, cancelled, options,
                      on: impl FnMut(u64, Option<u64>, Option<u32>)) -> Result<String /* verified sha256 */>;

pub fn share_verified(archive: &Path, magnet: &str, sha256: &str, policy: &PeerPolicy,
                      until: ShareUntil, cancelled: &AtomicBool) -> Result<ShareOutcome>;
```

- `fetch_observed` polls `ManagedTorrent::stats()` (`progress_bytes`, `total_bytes`, `live` peer count; librqbit `torrent_state/stats.rs:72-84`) on the same 100 ms cadence as `wait_for_cancel`. It emits `Transferring`, then reuses the existing `publish` path unchanged, which emits `Verifying` then `Completed`. With `upload_while_downloading = false` it sets `SessionOptions.disable_upload`.
- `share_verified` never calls `create_and_serve_torrent` and never hands the user's file to librqbit, which opens served files read-write (`storage/filesystem/fs.rs:161`). *As built (P1):* it copies the verified archive (reflink where the platform can) into `<profile>/peer-share/.modsync-share-*/` under the authored name. The name is the `TorrentInfo` name, else the magnet `dn`, else the local name. The copy must hash to the authored SHA-256. With `TorrentInfo` that exact dictionary is served. Otherwise the torrent is rebuilt over the copy and must give the authored infohash (`IdentityMismatch` if not). The torrent is added paused with `overwrite: true`, and sharing starts only if librqbit's recheck finds every piece. Each folder holds an owner lock while it is shared. It is removed on stop, SIGINT or SIGTERM, and folders left by dead processes are swept at the next share. `peerKeepSharing = false` returns `SharingOff` before anything opens.
- Session construction takes `PeerPolicy` and always sets, on the acquisition path: `connect: Some(ConnectionOptions { proxy_url: Some("socks5://host:port"), .. })`, `listen: None` (so no listener, no uTP socket and no UPnP), `dht: None` (librqbit `session.rs:420-421`), `disable_local_service_discovery: true`, and `ratelimits.upload_bps = policy.upload_bps`. *As built:* librqbit ignores `AddTorrentOptions.trackers` for magnets (`session.rs:1105-1117`), so the magnet is rebuilt from `xt`, `dn` and HTTP(S) `tr` only, and the tracker set is also passed as `SessionOptions.trackers`. With `TorrentInfo` the torrent is added from bytes. Peer read timeout is 180 s and keep-alive 30 s, instead of librqbit's 10 s and 120 s, so a capped seeder does not drop its leecher. If the proxy fails mid-run, the session stops and restarts on the next healthy proxy (see "Network privacy transport"). The existing `fetch`/`seed` CLI paths keep today's behavior, because the CLI user invoked them explicitly, and they are documented as not private.
- Guide-authored `Peers` never reach `initial_peers`.

## Seeding policy

- **On by default, one off-switch (ruling (a)).** Both parts are on unless the user turns sharing off:
  - downloading from other ModSync users, with pieces uploaded while a peer transfer is active (the minimum for 90548/90573);
  - sharing finished, verified downloads through `share_verified` within the stop rules below.
  Settings keeps a second, finer control ("Keep sharing finished downloads") so a user can download from peers without seeding afterwards. Both run only while a working proxy is available. Because proxies accept no inbound, uploads go only to peers this client dials itself (see "Seeding in proxied mode").
- **Upload cap** defaults to 100 KiB/s and is user-editable. Referent: `DownloadCacheOptimizer.cs:24` `MaxSendKbps = 100` (Wizard's earlier choice).
- **When to stop:** at the first of 24 hours of sharing, upload ratio ≥ 1.0, the user pressing Stop, or app exit. Referent: the `CleanupIdleSharesAsync` doc comment in the same file ("at least 24 hours or until ratio >= 1.0"), used here as an upper bound instead of a minimum. ModSync runs no background service or daemon after exit.
- Sharing happens only from the user's mod directory copy that ModSync verified. Sharing stops immediately if the file is moved, modified, or deleted, or if an install or extract step is about to read it.
- Metered or battery conditions are not detected. That is an open question, and it matters more now that sharing is on by default.

## Privacy and security

- The disclosure is one line in Settings and About. It names BitTorrent, says ModSync shares mod files with other ModSync users, and says it goes through a public proxy. Draft: "ModSync shares mod files with other users over BitTorrent, through a public proxy. Turn this off in Settings." The copy follows plain-UI rules. When no proxy works, peer steps fail closed and open no peer sockets, so no IP is exposed.
- UPnP, LSD, DHT, UDP trackers and the listener are always off on the acquisition path. Today's unconditional `enable_upnp_port_forwarding: true` (`src/p2p.rs:129`) changes the user's router, and librqbit's LSD multicasts the listen port on the LAN (probe: `BT-SEARCH` to `239.192.152.143:6771`, `/tmp/p2p-privacy-probe/RECEIPT.md` step 3). The acquisition path must not inherit either.
- The client never connects to guide-authored addresses or trackers by default. Otherwise a guide author could pick endpoints that log user IPs.
- A peer copy is untrusted until publish: temp directory only, SHA-256 match against the authored value, then `archive::verify` (expanded-size budget and path checks from `archive::Options`), then no-clobber publish. On mismatch the temp is discarded (`"SHA-256 mismatch; torrent discarded"`, `src/p2p.rs:192-195`) and the orchestrator falls back.
- Torrent payloads must be exactly one file. Multi-file or path-bearing torrents are rejected before anything is read (`only_file`).
- Nothing about peers goes into telemetry: no magnets, hashes, IPs, or byte counts. Referent for the same rule in Wizard's earlier plan: `distributed-file-resilience.plan.md` line 1140.
- Session state and the proxy and tracker list caches live under the ModSync profile directory, not a global librqbit default. Cancelling or exiting removes torrent temp directories.

## Network privacy transport

User override (2026-09-27, authoritative): no external networking dependencies. No i2pd, wireproxy, WARP client or other downloaded binary. Use only what already exists and is torrent-relevant, preferring externally maintained, popular lists. The earlier I2P recommendation is withdrawn.

The transport is librqbit 9.0.1's own SOCKS5 support, `SessionOptions.connect.proxy_url`. Raw evidence: `/tmp/p2p-privacy-probe/RECEIPT.md` (WARP probe) and `/tmp/p2p-privacy-probe/proxy/RECEIPT.md` (proxy probe), both disposable.

### What this gives, stated plainly

- **Peers see the proxy's IP, not the user's.** The proxy operator sees the user's IP and every connection, and can log it. BitTorrent traffic is unencrypted (librqbit 9.0.1 has no MSE/PE: `grep -rli 'diffie|rc4'` finds nothing), so the operator can read it too.
- **Integrity does not depend on the proxy.** A tampered piece fails librqbit's piece check. A tampered file fails the authored SHA-256 and `archive::verify` before publish (`src/p2p.rs:192-196`).
- **No inbound.** Public SOCKS5 proxies accept no inbound connections. librqbit uses SOCKS5 CONNECT only, with no BIND (`stream_connect.rs:88-110`). Two proxied ModSync users therefore cannot connect to each other. Sharing only helps when some reachable seeder exists, for example a maintainer seed. Host links stay canonical, and Origin always runs.
- **Why not WARP.** The earlier live probe showed WARP accepts no inbound either. A BitTorrent handshake to this host's WARP exit addresses got TCP-level "connects" and no reply, and so did a control port with nothing listening (`/tmp/p2p-privacy-probe/RECEIPT.md` step 2). WARP also needs a client binary, which the user override rules out.

### Proxied session: every leak closed

What librqbit 9.0.1 sends where (source: `~/.cargo/registry/src/index.crates.io-*/librqbit-9.0.1/`), and what proxied mode sets:

| Traffic | Through `proxy_url`? | Referent | Proxied mode |
|---|---|---|---|
| Outgoing peer TCP | Yes. TCP and uTP dialing are skipped when a proxy is set | `src/stream_connect.rs:35-38`, `:228-237` | On, through the proxy |
| HTTP(S) tracker announces | Yes, via `reqwest::Proxy::all` | `src/session.rs:707-710` | On, through the proxy |
| Incoming listener (TCP, uTP) | No | `src/session.rs:591-603` | Off: `listen: None` |
| UPnP port mapping | No | `src/session.rs:848-856`; today `src/p2p.rs:129` enables it unconditionally | Off (implied by `listen: None`) |
| DHT (UDP) | No | `src/session.rs:606-626` | Off: `dht: None` (`:420-421`) |
| UDP tracker announces | No | `src/session.rs:761` | Off: `udp://` entries dropped before the magnet is added |
| LSD multicast | No | `src/session.rs:763-775` | Off: `disable_local_service_discovery: true` |

What librqbit announces with no listener:
- `announce_port` comes only from the listener (`src/session.rs:792`), so it is `None`.
- DHT and LSD would announce `None` (`:1541`, `:1549`), but both are off.
- HTTP trackers get port **4240**: `self.announce_port().unwrap_or(4240)` (`src/session.rs:1577`), because "trackers don't work with 0 port" (`librqbit-tracker-comms-9.0.1/src/tracker_comms.rs:35-36`, `:272`). The request sends `ip: None` (`:278`).
- So a tracker records `<proxy exit IP>:4240`. That is a dead endpoint for other peers, and never the user's IP.

Other details P1 must handle:
- **DNS residual (evaluated, partly mitigated).** The proxy URL scheme must be exactly `socks5` (`src/stream_connect.rs:73-74`), and librqbit passes that same URL to `reqwest::Proxy::all` (`session.rs:707-710`). reqwest maps `socks5://` to local name resolution (reqwest `connect.rs:540`; only `socks5h://` resolves at the proxy, and librqbit rejects that scheme). Tracker host names are therefore looked up by the user's own resolver. Peers and trackers still see only the proxy's address. The resolver (usually the ISP or a configured DNS service) learns which tracker hosts are contacted.
  - Resolving through the proxy is not feasible without a new crate or patching librqbit. SOCKS5 has no standard resolve command, and librqbit owns the HTTP client and its proxy scheme. Our own health check can use ATYP domain because it speaks SOCKS5 itself; librqbit's announces cannot.
  - Mitigation in P1: on the proxied path, a guide-authored magnet `tr=` with a host name is dropped unless the same tracker is also in the fetched public list or the user's trackers. IP-literal `tr=` entries are kept. So no guide-chosen name reaches the resolver; only well-known public tracker names do. The direct `--no-proxy` path keeps every `tr=`.
  - List fetches (proxy list, tracker lists) are direct HTTPS with `no_proxy()`. System or environment proxies cannot silently reroute them.
- When `proxy_url` is `None`, librqbit builds reqwest with default settings (`src/session.rs:711-718`), which read `HTTP_PROXY`/`ALL_PROXY` from the environment. The proxy probe showed that this proxies the tracker announce and nothing else: peer TCP to `192.0.2.2:6881` went direct, with 1540 DHT sends, 3 LSD sends and 1 UPnP SSDP send in about 25 s (`/tmp/p2p-privacy-probe/proxy/RECEIPT.md` step 2). Environment variables are not a privacy mode, and ModSync must not describe them as one.
- The current production binary cannot be pointed at a proxy at all. It has no proxy flag (`grep -n -i 'proxy|socks' src/*.rs` finds nothing), and `session()` hard-codes the options (`src/p2p.rs:124-138`).

### Proxy sources (generic mechanism)

Settings hold two sources. Both are data, fetched at runtime; nothing is executed or installed.
1. `peerProxies`: explicit `socks5://host:port` endpoints the user typed.
2. `peerProxyListUrl`: a URL pointing to an externally maintained public proxy list. Default (lead ruling, per the user's preference for an externally maintained popular list): `https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/socks5.txt` (MIT; re-checked hourly by fetching a real URL). The user may change or clear it.

Handling:
- Parse lines as `host:port` or `socks5://host:port`. Keep SOCKS5 only. Drop `http://`, `https://` and `socks4://` entries, and drop anything that is not a literal IP and port.
- Health check before use: a SOCKS5 CONNECT through the candidate to an HTTP(S) tracker from the tracker set, with a short timeout. Keep the ones that answer. No other test endpoint is contacted.
- Rotate: one proxy per session. When the proxy fails (connect errors from the peer connector or tracker errors), stop the session, mark the proxy bad for this run, and start over with the next healthy one. Try at most N proxies per target (default 5).
- Fail closed: with no working proxy, the Peer step reports `SourceFailed { will_fallback: true }` and opens no peer socket. Origin is unaffected.
- The list is cached under the profile directory with a fetch time, and refreshed at most once per run.

Popular maintained SOCKS5 lists (checked 2026-09-27 with `gh api repos/<repo>` and each README):

| Repo | SOCKS5 list URL | Update cadence (from README) | Stars / last push | Licence |
|---|---|---|---|---|
| `proxifly/free-proxy-list` | https://cdn.jsdelivr.net/gh/proxifly/free-proxy-list@main/proxies/protocols/socks5/data.txt | "Validated every 5 minutes"; commits several times a day | 6,815 / 2026-09-27 | GPL-3.0 |
| `TheSpeedX/PROXY-List` | https://raw.githubusercontent.com/TheSpeedX/PROXY-List/master/socks5.txt | "updated from time to time"; commits several times a day | 5,830 / 2026-09-27 | None declared |
| `monosans/proxy-list` | https://raw.githubusercontent.com/monosans/proxy-list/main/proxies/socks5.txt | "re-checked every hour"; each proxy "had to fetch a real URL in full" | 1,520 / 2026-09-27 | MIT |
| `hookzof/socks5_list` | https://raw.githubusercontent.com/hookzof/socks5_list/master/proxy.txt | No stated cadence; commits several times a day | 1,062 / 2026-09-27 | None declared; README also advertises a paid private proxy |

**Ethics note, recorded for the user's decision.** Popular scraped proxy lists are largely misconfigured or compromised machines. Their owners usually did not agree to carry anyone's traffic. The people running them, or whoever compromised them, can see and log all traffic, including the user's IP and which archives the user fetches. Integrity stays safe because of the SHA-256 and archive verification. So the mechanism is built generic, and which list, if any, is the default is the user's call.

### Tracker discovery

With DHT off, peers can only come from trackers and user-entered peers.
- The authored `Magnet` carries its own `tr=` trackers. Only `http://` and `https://` ones are used.
- ModSync also fetches `ngosang/trackerslist` at runtime as data: `trackers_all_http.txt` and `trackers_all_https.txt` from https://github.com/ngosang/trackerslist (55,230 stars; GPL-2.0; README: "automatically updated every day"). On 2026-09-27 these held 16 and 10 entries. The 48 `udp://` entries are dropped, because UDP tracker traffic bypasses SOCKS.
- The download plan records the tracker URLs used for each target.

### Seeding in proxied mode

Seeding through a proxy has no listener, so a proxied seeder is only useful to peers it dials itself. It learns about those peers from trackers. In practice, the reachable party in each pair is a maintainer seed or any ordinary BitTorrent client that has the same authored magnet and an open port. Probe 3's fix still applies: sharing adds the **authored** magnet over the verified file and never mints a new torrent (see "Probe 3 result").

### Ranked alternatives, kept for the record

1. **SOCKS5 through librqbit's `proxy_url` (chosen).** Nothing to install, available today in the vendored crate. Outbound only.
2. WARP, userspace or system: eliminated. It needs a downloaded client, and the probe showed no inbound.
3. I2P (i2pd or emissary): withdrawn by the user override. It would give inbound between users, but it is an external networking dependency.
4. Tor onion services via arti: an external dependency, and the Tor Project discourages BitTorrent over Tor (https://blog.torproject.org/bittorrent-over-tor-isnt-good-idea/).
5. Docker: eliminated. It needs admin once on Windows (WSL 2) and privileged setup on macOS (https://docs.docker.com/desktop/setup/install/windows-permission-requirements/, https://docs.docker.com/desktop/setup/install/mac-permission-requirements/), and it would not add inbound.

### Cloudflare terms (kept from the WARP evaluation)

Cloudflare Application Terms of Service, last updated 2024-10-18, https://www.cloudflare.com/application/terms/. Not legal advice. Section 2 grants a "personal" licence and bars reverse engineering and derivative works of the Application. Section 6 bars infringing content and "undue burden". There is no clause on automated registration, on BitTorrent, or on device limits (unverified). This no longer affects the design, because WARP is not used.

## Licensing and redistribution stance

This is recorded as product policy, not legal advice. Wizard's stance (90498, 90648, 97025) is that peer exchange of the host's own bytes among users who are each downloading that mod is not a reupload. His latest stated direction (114891, 114916, 116381, April 2026) is community-maintained permissions, with credit to OpenKotOR and the creator and share-alike terms. The mechanism enforces that stance by construction:

- Only resources whose maintainer authored `ContentHashSHA256` and `Magnet` are ever shared.
- ModSync never creates or announces a torrent for anything else.
- The host link always stays listed and is tried in parallel.
- Removing the fields from the guide withdraws the resource on the next guide load.

## GUI exposure and progress

Owned by the orchestration adapters, not by this slice. Wiring waits for `src/ui.rs` to be released.

- **Settings:** the off-switch (default on), "Keep sharing finished downloads" (default on), upload cap, SOCKS5 proxies, the proxy list URL, the tracker list URL, user peers and trackers, `peer_mode`, and the one-line disclosure. There is no port-mapping control. They persist in `profiles::Store` settings as `peerSharing`, `peerKeepSharing`, `peerUploadKiBps`, `peerMode`, `peerProxies`, `peerProxyListUrl`, `peerTrackerListUrl` (default the ngosang HTTP and HTTPS lists), `peerExtraPeers` and `peerTrackers`. About repeats the disclosure line.
- **Proxy state:** Settings shows how many proxies passed the health check, and that peers are skipped while none work. It is not a prominent status bar item.
- **Per-target row:** source label (host name or "Other ModSync users"), bytes/total, "from N peers" when `peers` is present, then `Verifying`, `Done`, or `Using host download` on `Superseded`. A target that is not eligible shows no peer text.
- **Sharing panel:** each shared archive with uploaded bytes, ratio, time left before the 24 h limit, and a Stop control, plus one Stop sharing all.
- The CLI adds an additive `--peers` flag on the orchestrated batch command. NDJSON uses the orchestration envelope. Legacy `download`, `download-batch`, and `torrent-*` output is unchanged.

## File ownership and task sequence

| Task | Owner | Owned files | Responsibility |
|---|---|---|---|
| O1 | download-orchestration | `src/acquisition.rs` (new), `src/lib.rs` | Shared types above, orchestrator, race/join/Superseded, Completed sha256 |
| P1 | this slice | `src/p2p.rs` | `PeerPolicy`, `fetch_observed`, `share_verified`, `impl Acquire`, and a proxied session (`connect.proxy_url` set; listener, UPnP, DHT, UDP trackers and LSD off). Also `ProxyPool`: parse the explicit and list-URL sources, keep SOCKS5 only, health-check, rotate, fail closed. Also the HTTP(S) tracker set from magnet `tr=` plus the ngosang list. The current binary has no proxy hook at all (proxy probe step 2). Existing `fetch`/`seed` and the inline test unchanged |
| P2 | this slice | `src/resources.rs` or a new `src/peer_metadata.rs` | Parse/validate `ContentHashSHA256`, `Magnet`, and `Peers` from `resource_registry`; warnings; build `Source::Peer` |
| O2 | download-orchestration | `src/main.rs` | CLI `--peers` adapter, NDJSON |
| O3 | download-orchestration, after the user releases `src/ui.rs` | `src/ui.rs`, possibly a new `src/ui/acquisition_page.rs` | Settings, rows, sharing panel |
| E1 | this slice | `docs/parity/p2p-acquisition-native/` (new), `COMPATIBILITY.md` | Probe receipts and an honest status note |

P1 starts after O1's types are approved. P2 is independent. Workers report any edit outside their files before making it.

## Direct probes (no Rust tests)

Use a disposable root outside the repository and game trees, loopback or LAN-only sessions, and fixture archives the prober created. Receipts record the Rust HEAD and dirty-state hash, the copied binary SHA-256, argv, exit code, stdout/stderr hashes, before/after manifests, and a statement that no Rust tests or harnesses were created, changed, or run.

1. **Legacy verbs unchanged.** Run `torrent-seed` and `torrent-download` with `--peer`, and compare the JSON key set and publish behavior with the pre-change binary.
2. **Race, Peer wins.** Throttle the local HTTPS origin, seed from peer A, download with `peer_mode=Race`. Check: `Transferring{peers:Some(n)}`, one `Completed` with the authored hash, `Superseded` for Origin, the loser's temp gone, and one file published.
3. **Seed infohash identity (blocking).** Seed identical bytes twice under the authored name and compare magnets. Seed under a different filename and show the magnet differs. Then run `share_verified` with the authored magnet and show that peer B completes. If this fails, sharing is disabled and only racing ships.
4. **Poisoned peer.** Peer A serves different bytes under a magnet whose hash field names the real archive. Show a SHA-256 mismatch, nothing published, fallback to Origin, and the final hash equals the authored one.
5. **Ineligible resource.** No authored hash means no peer session: no listening socket (`ss -lntup` before and after) and no DHT traffic.
6. **Off-setting and fail-closed.** (i) With sharing turned off: no peer socket, no proxy or list fetch, Origin only. (ii) With sharing on and no working proxy (all candidates dead): no peer socket (`ss` diff of the owned process), Peer reported as `SourceFailed { will_fallback: true }`, Origin runs. In both: no UPnP SSDP packet and no LSD packet to `239.192.152.143:6771` (capture inside a disposable netns).
7. **Guide peers ignored.** A guide-authored `Peers` entry pointing to a loopback listener sees no connection.
7a. **No leak outside the proxy.** Use the stdlib SOCKS5 probe server (`/tmp/p2p-privacy-probe/proxy/socks5.py`). During a proxied peer transfer, `strace -f -e trace=connect,sendto,sendmsg` shows every TCP connect from the peer session going to the proxy, no UDP send (no DHT, UDP tracker, LSD or SSDP), and no listening socket. The proxy log shows the peer and HTTP(S) tracker CONNECTs. Baseline to beat: the current binary with proxy environment variables proxied only the tracker (proxy probe step 2).
7b. **Proxy rotation.** The first listed proxy is dead and the second works. Show one health-check failure, then the transfer through the second proxy. With every proxy dead, see probe 6(ii).
8. **Upload cap and stop rule.** Measure upload at or below the cap (±10%). Sharing stops when ratio ≥ 1.0, on Stop, on app exit, and when the file is modified.
9. **Cancel mid-race.** Both children stop, both temps are removed, the target reports `Cancelled`, and nothing is published.
10. **Halfway scenario (90548).** A starts a peer download from seed S. B starts while A is at about 50%. Show that B receives bytes from A (A's `uploaded_bytes > 0`) before A completes. In proxied mode this passes only if one of A and B is reachable. Two proxied clients cannot connect, and the receipt must show that negative case too.

## Review gates

- **P1 spec review:** off-setting and fail-closed gating of every socket; listener, UPnP, LSD, DHT and UDP trackers unreachable on the acquisition path; `PeerEndpoint` dialed only through the proxy; the proxy list treated as data (SOCKS5 lines only, nothing executed); authored-magnet-only sharing; verification before publish; existing API byte behavior; the inline test untouched.
- **P1 quality review:** session and temp cleanup on every exit path; the stats polling cadence; no blocking of the orchestrator thread; no error-string classification.
- **P2 review:** round-trip preservation of unknown registry keys; warnings, not failures, for malformed peer metadata; `Peers` ignored by default.
- **E1 final review:** every probe receipt; docs claim nothing unprobed (for example, no claim that NAT traversal works); and confirmation that `AGENTS.md`, `src/ui.rs`, and `src/ui/installing_page.rs` are untouched by P-tasks.

## Open questions and defaults

| Question | Default until decided |
|---|---|
| BEP-19 web seeds (host URL as a torrent web seed) would unify both paths. Does librqbit 9.0.1 support them? | Assume not. `grep -rn -i 'url-list\|webseed'` over the crate source found nothing. Keep the orchestrator race. |
| Should A share host-downloaded bytes before A's own verify completes (90548 literal)? | No. Peer-swarm pieces are shared while downloading, because librqbit verifies each piece. Host-downloaded bytes are shared only after publish. |
| Keep sharing after app exit? | No. There is no daemon. |
| Metered-network or battery detection | Not detected. Sharing is on by default, so this needs a decision before release. Until then the 100 KiB/s cap and the stop rules bound the cost. |
| Default proxy list | Decided: monosans/proxy-list SOCKS5 (see line above). The ethics note under "Network privacy transport" was surfaced to the user. If the list yields no working proxy, Peer fails closed and Origin is unaffected. |
| Where reachable seeders come from | Proxies give no inbound, so users cannot seed each other. A maintainer seed, or any ordinary client with an open port and the authored magnet, is needed for Peer to deliver anything. Who runs it is open. |
| Local takedown blocklist (Wizard's earlier C# `BlockContentId`, dfr.md lines 35–45) | None. Withdrawal happens by removing guide fields. Revisit if a shared community blocklist appears. |
| Accept magnets without `dn`, or with a `dn` that differs from the archive filename | Accept for download. For sharing, report `NameMismatch`. |
| Spoiler-free variant archives | Out of scope unless authored with their own hash and magnet. |
| IPv6 | IPv4 only, as today (`src/p2p.rs:130-133`). Peers are dialed through the proxy either way. |
| Where the maintainer tooling writes `ContentHashSHA256`/`Magnet` | A later editor/CLI task. For now the fields are entered by hand from `torrent-seed` output. |

## Completion boundary

This slice is complete when P1, P2, and E1 are approved, probes 1–10 (with 7a and 7b) have receipts, and the orchestration slice has landed O1/O2 using the agreed interface. GUI exposure (O3) is required before it is complete for users, and O3 is blocked until `src/ui.rs` is released. The plan claims no C# parity and no legal clearance.

## Probe 3 result — seed identity (run 2026-09-27, lead)

Blocking check executed with the production headless CLI at HEAD `dcd1373` (binary SHA-256 `b6097a7905204f23ec88e985e0d5095f8d2ea4e6037032a4cd4c771989139fa8`, built `--no-default-features`). One ZIP (SHA-256 `afc7a72963482811ba62bde363a49a6f5d2639302ad10bcd683c9beb1d404573`) was seeded twice for 10 s with `modsync torrent-seed`, once as `mod-v1.zip` and once as a byte-identical copy `renamed.zip`.

| File | Archive SHA-256 | Infohash |
|---|---|---|
| `mod-v1.zip` | `afc7a729…4573` | `771c2e3917be1b676fe9876142b321b07c554ae7` |
| `renamed.zip` | `afc7a729…4573` | `71370f6af7ad7a9a906f6059e40f4d08cb38d4e5` |

Verdict: holds. Current `p2p::seed` derives a filename-dependent infohash, so independent holders of the same bytes would form separate swarms. Task P1 must seed from the authored magnet/info dictionary (or a canonical content-addressed name) before sharing can ship. Fixture: `/tmp/p2p-probe3` (disposable). Both seed processes ended by `timeout`; none remain.
