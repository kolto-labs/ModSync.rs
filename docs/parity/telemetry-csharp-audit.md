# Telemetry, logging and diagnostics: C# parity audit (ledger F27)

Oracle: C# ModSync at `d31ccbd3` (`/home/brunner56/.codex/worktrees/5813/ModSync`).
Rust base: `ac9cf3a`. C# paths are relative to the oracle root. Rust paths are
relative to this repo.

## 1. C# surface

### Configuration: `src/ModSync.Core/Services/TelemetryConfiguration.cs`

- The file lives at `%AppData%/ModSync/telemetry_config.json` (:15-19). Load falls back to `KOTORModSync/` (:118-126).
- JSON keys (:27-94) are `enabled`, `user_consented`, `consent_date`, `anonymous_user_id`, `session_id`, `environment`, the four `collect_*` switches, the four `enable_*_exporter` switches, `otlp_endpoint` and `prometheus_*`. The rest are `api_key`, `is_first_run`, `last_send_time`, `local_log_path`, `max_log_file_size_mb` and `retention_days`.
- `otlp_endpoint` defaults to the real host `https://otlp.bolabaden.org` (:73). `enable_otlp_exporter` defaults to true (:64).
- `SigningSecret` is `[JsonIgnore]` (:96-97). It loads from `MODSYNC_SIGNING_SECRET`, then `KOTORMODSYNC_SIGNING_SECRET`, then `telemetry.key` and its legacy path. Official builds can also embed it (:162-215).
- Load gives each run a new `SessionId` and sets `IsFirstRun = false` (:133-135). A missing file yields a disabled first-run config (:145-155).
- `SetUserConsent` sets `UserConsented` and `IsEnabled`, then saves (:238-243). It never writes `ConsentDate`.
- `GetPrivacySummary` (:245-299) and `CleanupOldData` (:301-332) delete `*.log` files under `telemetry/` that are older than `retention_days`.

### Authentication: `TelemetryAuthenticator.cs` and `telemetry-auth/auth_service.py`

- The signed message is `POST|{path}|{timestamp}|{session}`, signed with HMAC-SHA256 over the raw UTF-8 secret and sent as lowercase hex (`TelemetryAuthenticator.cs:29-35`).
- The sidecar is a Traefik ForwardAuth service. It takes the path from `X-Forwarded-Uri` (`auth_service.py:75, 100`). It checks for the headers (:78) and allows ±300 s of drift (:30, 88-91). It compares in constant time (:109), and a failed check returns 401/403.

### Service: `src/ModSync.Core/Services/TelemetryService.cs`

- It is a singleton (:26, 53). `Initialize` does nothing without consent (:62) and turns telemetry off when no secret is present (:69-76).
- The resource carries `service.name=ModSync`, `service.version`, `service.instance.id=session`, `user.id`, `session.id`, `environment` and `platform` (:78-89).
- It declares 12 instruments (:95-108): event, error, mod-install, validation, download and cache counters, plus the operation, download-size and cache-operation histograms.
- OTLP export is `HttpProtobuf` to `OtlpEndpoint`, for traces (:125-133) and metrics (:146-154). The console exporter is optional (:114, 141). The file exporter is only logged as "not yet implemented" (:119-123). Prometheus runs a localhost listener (:156-175).
- Auth headers come from `GetAuthHeaders` (:773-801) and are **computed once, inside `Initialize`**.
- `UpdateConfiguration` (:188-208) initializes on enable. On disable it disposes, and `Dispose` calls `Flush` first (:803-825).
- The recorders are `RecordEvent` (:210), `StartActivity` (:228), `RecordModInstallation` (:254), `RecordUiInteraction` (:287), `RecordDownload` (:310), `RecordValidation` (:355), `RecordFileOperation` (:384), `RecordComponentExecution` (:418), `RecordParsingOperation` (:452), `RecordSessionStart`/`End` (:486, 509) and `RecordError` (:536). The cache recorders run from :566 to :706.
- Names and errors are hashed with `HashString` (:742-769): SHA-256, uppercase hex, first 16 characters. The exception is `RecordCacheOperation`, which sends the **raw** error text (:695-698).
- Every tag list includes `event.name` (:724-740).

### Emission call sites (C#)

| Where | What |
|---|---|
| `ModSync.GUI/Program.cs:66, 72-75, 80-81` | Session start, then session end and flush around the Avalonia lifetime |
| `ModSync.GUI/MainWindow.axaml.cs:519-548` | First-run consent dialog, then `Initialize` if the user consented |
| `ModSync.GUI/MainWindow.axaml.cs:2835, 2908, 2934, 2951, 3361` | UI interactions, `file.loaded`, load errors |
| `ModSync.Core/ModComponent.cs:877-955, 1874-1886` | Mod installation and component execution |
| `ModSync.Core/Instruction.cs:511-548, 816, 920, 1134-1277` | File operations |
| `ModSync.Core/Services/Download/DownloadManager.cs:115-152, 482, 527` | Download file operations and downloads |
| `ModSync.Core/Services/Validation/DryRunValidator.cs:444-581` | Validation |
| `ModSync.Core/Parsing/MarkdownParser.cs:298` | Parsing |
| `ModSync.Core/Services/DownloadCacheService.cs:356, 360` | Cache hits and misses |
| `ModSync.Core/Logger.cs:426` | `RecordError` for every logged exception |
| `ModSync.GUI/Dialogs/SettingsDialog.cs:734-745, 1094` | Consent toggle, `UpdateConfiguration`, privacy details |
| `ModSync.GUI/Dialogs/TelemetryConsentDialog.axaml.cs:35, 68, 82` | Accept and decline |

### Logging and diagnostics: `src/ModSync.Core/Logger.cs` and `NLog.config`

- File target: `${basedir}/Logs/kotormodsync_${shortdate}.log` with layout `${longdate} ${message}`. It rotates daily and keeps at most 30 archives (`NLog.config:10-17`).
- The console target is colored, and the memory target keeps 200 lines (:20-34).
- Info and above always go to file and memory (:39). Debug goes there only when `DebugLogging` is on (:49-53). Console mirroring is suppressed in machine-readable mode (:42-46, 56-61; `Logger.cs:26`).
- Level prefixes are `[Warning] `, `[Error] ` and `[Verbose] ` (`Logger.cs:361`).
- `GetRecentLogMessages` and `ClearLogHistory` work on the memory target (`Logger.cs:191, 224`).
- Exception details go to the log with the stack, then `ExceptionLogged`, then `TelemetryService.RecordError` (`Logger.cs:408-426`).
- Hooks for unhandled and unobserved-task exceptions are registered at `Logger.cs:67-68` and handled at :448 and :460.

## 2. Rust surface before this lane (`ac9cf3a`)

- `src/telemetry.rs` (180 lines) has `Config` with a subset of the C# keys and an `endpoint` field, not `otlp_endpoint`. `signing_secret` is `#[serde(skip)]`, but the derived `Debug` **printed it**.
- `signature()` is correct RFC 2104 HMAC-SHA256.
- `export()` POSTs one JSON `Event` to `{endpoint}/v1/events`. There is no OTLP, no batching, no metrics or spans, and no hashing helpers.
- The only caller is `src/ui.rs:1575-1585` (`emit_telemetry`, one thread per event), for `operation.completed` (:182, :278) and `guide.loaded` (:1699).
- Consent UI is at `src/ui.rs:883-902`, and the config loads at `src/ui.rs:1660`.
- There is no persistent log file. Logs are an in-memory `Vec<String>` in the GUI. There is no panic hook, no diagnostics snapshot and no privacy summary.

## 3. Ranked gaps

| # | Gap | Impact | Status after this lane |
|---|---|---|---|
| 1 | No OTLP export. `/v1/events` is not an OTLP route, so even consented Rust telemetry cannot reach the C# backend. | Telemetry is dead end-to-end | **Closed in library.** OTLP/HTTP JSON traces and metrics, signed per request (`telemetry.rs` `encode_traces` :521, `encode_metrics` :642, `Exporter::post` :1460). The frontend must migrate (§5). |
| 2 | No persistent log file (C# `Logs/`) | Support has no logs to ask for | **Closed in library.** `logging.rs` `init` :97 writes `<settings>/Logs/kotormodsync_YYYY-MM-DD.log` (the NLog target name), keeps 30 files (`prune` :351) and holds 200 lines in memory (`recent` :208). |
| 3 | No automatic lifecycle, install or download emission | No data even when consented | **Recorders ported** (`record_*` :1007-1261). Wiring is frontend work (§5): ui.rs and main.rs are out of scope for this lane. |
| 4 | Derived `Debug` on `Config` leaked the signing secret | Secret could reach logs | **Closed.** Manual `Debug` redacts it (`telemetry.rs:100-126`). |
| 5 | No exception-to-telemetry bridge or panic logging | Crashes go unrecorded | **Closed in library.** `logging::log_error` :195 hashes and forwards. `install_panic_hook` :221 chains the previous hook. |
| 6 | No diagnostics snapshot or privacy summary | Weak support workflow | **Closed in library.** `logging::diagnostics_report` :243 reports only whether a secret is present. `Config::privacy_summary` :187. |
| 7 | No first-run flag, so the frontend cannot tell when to ask for consent | Consent prompt never shown | **Closed.** `Config::is_first_run` (set in `load` :138). |
| 8 | C# config keys missing (`otlp_endpoint`, `environment`, `collect_crash_reports`, `enable_otlp_exporter`) | Migrated C# configs lose settings | **Closed.** Fields added. `otlp_endpoint` is a serde alias of `endpoint` (:56). |
| 9 | No legacy `KOTORModSync/` config and key fallback | Minor migration gap | Open. The caller can pass the legacy directory to `Config::load`. |
| 10 | No Prometheus listener or console exporter | Dev-only in C# | Open on purpose. The Prometheus listener is local-only, dev-only in C#. |
| 11 | `CleanupOldData` of telemetry files | C# file exporter never existed | Not applicable. Log pruning covers `Logs/`. |
| 12 | Unbounded telemetry memory (review of 72c3b86, finding 2): unbounded producer channel, span cap applied only after dequeue, metric series never capped | Slow or dead collector grows memory without limit | **Closed.** Bounded `sync_channel(QUEUE_CAPACITY=4096)` (:758, :911); producers use `try_send` and count drops (`enqueue` :811). Span cap drops are counted (:1380). Per-instrument cap `MAX_SERIES_PER_METRIC=2000` folds new attribute sets into an `otel.metric.overflow=true` series and counts them (:567, `Metrics::add` :591). `Telemetry::dropped_records` :856 and the diagnostics report expose the count. |
| 13 | Revocation did not stop an in-flight POST or join the worker (review finding 3) | "Zero network after revoke" did not hold at return | **Closed in library.** `configure(off)` sets the cancel flag, drops the sender and joins the thread (`Worker::stop` :801, :884-891). `post` re-checks the flag right before sending (:1487). A POST already on the wire is bounded by `REQUEST_TIMEOUT=3s` with a 2 s connect timeout (:761, :1474-1478), except during DNS resolution: dropping the blocking reqwest client joins its tokio runtime (reqwest 0.12.28 `blocking/client.rs:1354`), and that runtime waits for pending `spawn_blocking` work, including hyper-util's `getaddrinfo` (`dns.rs:119`). So the join takes up to `REQUEST_TIMEOUT`, or until the system resolver gives up on a stalled lookup. The idle loop polls the flag every 200 ms (:1374), so live `Span` sender clones cannot keep it waiting. After `configure(off)` returns, the thread has exited and its client and connections are dropped. |
| 14 | Secret rotation left the exporter on the old secret (review finding 4) | 403s until restart | **Closed.** A ready `configure` sends `Command::Reconfigure(config)` to a live exporter (:893-908). The exporter swaps its config and resource (:1390), so the next POST signs with the new secret, endpoint and session. A dead exporter (`handle.is_finished()`) is replaced instead of reported as running (:893-895, `is_running` :848-852). |
| 15 | Legacy GUI path spawns a thread per event before the consent check (review finding 1) | Consent-off still creates an OS thread (no client, no socket) | **Deferred to the ui.rs wiring task.** `ui.rs:1582` runs `std::thread::spawn` before `export()` checks consent at `telemetry.rs:269`. Fixed by F5, which removes the legacy call. |

## 4. Deliberate divergences from the oracle (Rust is stricter)

1. **Signing per request.** C# builds the header string once, in `Initialize` (`TelemetryService.cs:131, 152`). Every export after 300 s then fails the sidecar drift check (`auth_service.py:88`). Rust signs every POST with a fresh timestamp (`Exporter::post`).
2. **The signed path is the requested path.** Rust signs `Url::parse(url).path()`, so an endpoint with a path prefix still verifies. C# signs a bare `/v1/traces` and `/v1/metrics`. The OpenTelemetry .NET 1.15.3 exporter (`ModSync.Core.csproj:95`) requires the full signal URL for `HttpProtobuf` (`~/.nuget/packages/opentelemetry.exporter.opentelemetryprotocol/1.15.3/README.md:340-342`). C# sets the bare base endpoint, so it POSTs both signals to that base URL while signing `/v1/traces` and `/v1/metrics`, and cannot pass ForwardAuth. This comes from the documentation plus the C# source, not from a runtime capture.
3. **Revocation sends nothing.** C# `UpdateConfiguration` then `Dispose` then `Flush` sends buffered data after the user opts out (`TelemetryService.cs:204-206, 812`). Rust `Telemetry::configure` with a non-ready config sets a cancel flag, drops the buffer and joins the exporter before returning (:876-883, `Worker::stop` :798). The exporter checks the flag at its loop head (:1355), before each export (:1395, :1414) and right before each send (:1455).
4. **No default host.** C# defaults `otlp_endpoint` to a real server. Rust leaves `endpoint` empty. A migrated C# file still carries its value through the alias.
5. **`RecordCacheOperation` error text.** C# sends it raw. Rust sends `error.hash`.
6. **Log location.** C# writes under `${basedir}` (the install directory, possibly read-only). Rust writes under the settings directory. The file name matches the NLog target, `kotormodsync_${shortdate}.log` (`NLog.config:12`). C#'s `Logger.LogFileName = "modsync_"` (`Logger.cs:17`) is not what NLog uses.
7. **Stdout.** Rust never writes logs to stdout. The stderr mirror is opt-in, so CLI JSON output stays clean.
8. **Metrics temporality.** Rust uses CUMULATIVE, the C# OTLP exporter default. Histogram bounds are the OTel SDK defaults.
9. **Spans have no parent context.** Each span is its own trace. C# `Activity.Current` nesting is not modelled.

## 5. Frontend call sites still needed (ui.rs and main.rs, not touched here)

| # | Where | Call |
|---|---|---|
| F1 | `ui.rs` `App::new` (:1660) and `main.rs` `gui()` | Load `telemetry::Config` **after** `--settings-dir` is applied. Today it loads from the default directory before `settings_dir` overrides `profile_directory` (`ui.rs:3155-3168`). Then call `telemetry::global().configure(&config)`. |
| F2 | same | Load telemetry before `app.load()`. Today `load()` runs first (`ui.rs:1654-1655` vs :1660), so the `guide.loaded` event for a `--input` guide is always dropped. |
| F3 | `ui.rs` startup | If `config.is_first_run`, show the consent prompt. On answer, `set_consent`, `save`, then `global().configure`. |
| F4 | `ui.rs:885-897` (consent checkbox and endpoint save) | After `save`, call `telemetry::global().configure(&self.telemetry)`. Unchecking must reach `configure` so buffered data is dropped. Run **every** `configure` call off the egui thread. `configure(off)` joins the exporter and can block for up to `REQUEST_TIMEOUT` (3 s), or until the system resolver gives up on a stalled lookup. `configure(ready)` sends a blocking `Reconfigure`: with a full queue during an export it can wait about 6 s (two 3 s POSTs). At `ui.rs:902`, show readiness from `exporter_ready()`, not `active()`. `active()` ignores the endpoint and the exporter switch. |
| F5 | `ui.rs:1575-1585` `emit_telemetry` | Remove the per-event `std::thread::spawn` at :1582, which runs before the consent check (gap 15). Replace `export(&config, "/v1/events", ..)` with `global().record_event(name, attrs)`, or with `record_operation(job, success, elapsed)` for `operation.completed`. |
| F6 | `ui.rs` job start and finish (:182, :278) | Measure job duration, then call `record_operation`. Install, download and validate map to `record_mod_installation`, `record_download` and `record_validation`, fed from engine `Event`s and `InstallProgress`. |
| F7 | `main.rs` before the GUI or CLI dispatch | Call `logging::init(&settings_dir, Options { debug_logging, mirror_stderr: !json_output, .. })` and `logging::install_panic_hook()`. Then call `global().record_session_start(total, selected)`. |
| F8 | GUI exit (eframe `on_exit`) and CLI end | `record_session_end(elapsed, completed)`, then `global().shutdown(true, timeout)`, off the egui thread. Shutdown costs the flush wait (up to `timeout`) plus the join, which takes up to `REQUEST_TIMEOUT` or longer during a DNS stall. `flush` uses `try_send`, so with a full queue `shutdown(true, ..)` returns `Failed` and sends nothing. |
| F9 | `ui.rs` log pane and `Stopped:` error paths | Mirror `self.logs.push` into `logging::info` or `logging::error`, and use `logging::log_error(&e, ctx)` for `Err(e)` terminals. |
| F10 | Settings page | Add "Copy diagnostics", which puts `logging::diagnostics_report(Some(dir), Some(&cfg), 50)` on the clipboard. Add "Privacy details", which shows `cfg.privacy_summary()`. Add "Open log folder", which opens `logging::log_directory()`. |

## 6. Verification

The build, format and clippy gates are listed in the lane report.

**Runtime: unverified.** There is no production path that reaches the new
exporter. `configure` has no caller until F1-F8 land, and no CLI command loads
telemetry. The one existing production caller is the legacy `export` in
`ui.rs:1583`. Every trigger for it needs a click: install, a job, the protocol
"Download & Open" button (`ui.rs:786`) or the update check (`ui.rs:916`). The
only trigger that needs no click, `guide.loaded` via `--input`, fires before the
config loads (F2), so it cannot produce a positive control. Tests are frozen, so
no harness drove the library.

The probe kit is ready at `/tmp/modsync-tel-probe/`. It holds `capture.py`, which
counts TCP accepts, stores bodies and forwards the `X-KMS-*` headers plus
`X-Forwarded-Uri` to the unchanged `auth_service.py`. The kit was self-checked
with a Python-signed request (200) and a zero signature (403). Once F1-F8 land,
run the GUI with `XDG_CONFIG_HOME=<tmp>`, `MODSYNC_SIGNING_SECRET=<test>` and
`endpoint=http://127.0.0.1:14318`, once with consent on and once with it off.

**Source walkthrough.**

- *Consent off means zero network.* `configure` returns before spawning when
  `!exporter_ready()` (:884-891). `exporter_ready` needs `active()`, which needs
  enabled, consented and a secret, plus `enable_otlp_exporter` and an http(s)
  endpoint (:182). With no worker, `metric()` and `start_span` find `None` and
  send nothing. `flush` returns `Disabled`. The `reqwest` client is built only
  inside `Exporter::post`, on the exporter thread (:1472-1480).
- *Revocation.* `configure(off)` takes the worker out of the slot, releases the
  lock, sets `cancelled`, drops the sender and joins (`Worker::stop` :801).
  Nothing new is sent: the flag is checked at the loop head (:1363), before each
  export (:1427, :1446) and right before `send()` (:1487). A POST already on the
  wire ends within `REQUEST_TIMEOUT` (3 s), or when the system resolver gives up
  on a stalled lookup (see gap 13). The join waits for it. The exporter
  owns the client, so the client and its pooled connections drop when the thread
  exits. Once `configure` returns, no telemetry thread, client or connection
  exists. The legacy `export` path in ui.rs is outside this guarantee (gap 15).
- *Bounded memory.* Queue capacity 4096 (:758). `try_send` never blocks and
  counts a full queue (:811). Spans are capped at 2048 after dequeue (:1380).
  Metric series are capped at 2000 per instrument plus one overflow series
  (:591). `Reconfigure` is the only blocking send, and it is bounded by the
  exporter draining the queue.
- *Rotation.* A second `configure` with a new secret and the same endpoint
  queues `Reconfigure`. FIFO order puts it ahead of any later POST, and `post`
  reads the secret from the swapped config.
- *Session change.* When `Reconfigure` carries a new `session_id`,
  `Exporter::retire_session` first exports the buffer under the old config,
  then clears spans and resets metrics. Anything unsent is counted as dropped.
  So old-session data never goes out labelled with the new `session.id` or
  `service.instance.id` (resource attributes, `telemetry.rs:499, 502`). If the
  same call also rotates the secret, the old-session send is signed with the
  old secret. If the collector rejects it, that data is dropped.
- *Signing.* `post` computes the timestamp per request and signs
  `POST|<URL path>|<ts>|<session>` with `signature()`. That is RFC 2104 HMAC:
  keys over 64 bytes are hashed, then zero-padded, with ipad 0x36 and opad 0x5c.
  Python `hmac.new(secret, msg, sha256)` computes the same function, which is
  what `auth_service.py:102-106` recomputes.
- *Secret never serialized.* `#[serde(skip)] signing_secret` (:62-63). The
  manual `Debug` prints `<redacted>`. `diagnostics_report` prints only
  `signing_secret_present`.
- *OTLP/JSON shape.* Keys are lowerCamelCase. `traceId` and `spanId` are hex
  (`random_hex`). int64 values are decimal strings (`asInt`, `count`,
  `bucketCounts`, `*TimeUnixNano`). Attributes use the `{key, value:{stringValue|
  boolValue|intValue|doubleValue}}` shape. Sums set `isMonotonic: true`, and
  both sums and histograms use `aggregationTemporality: 2`.

## Private telemetry persistence source (2026-10-02)

Telemetry configuration and signing-key reads now use the same private storage
boundary as profiles/settings: bounded regular-file reads, ancestor/link/reparse
checks, and current-user ownership checks. Configuration remains bounded to 1 MiB;
key files are bounded to 16 KiB before reading their contents. A present unsafe,
unreadable or invalid-UTF-8 key is refused rather than silently falling through to
another directory's key. Environment-secret precedence remains unchanged.

Explicit settings roots never consult legacy storage. Implicit defaults prefer
current configuration; only absence permits the legacy configuration. Key lookup
first considers the current root and then the legacy root only when needed, so an
unused malformed legacy key cannot block a valid current key. Reading configuration
alone does not write either root. Successful saves copy configuration forward,
never serialize signing secrets, and leave legacy files untouched.

Configuration saves share the profile/settings write lock, refuse malformed
existing configuration, enforce the serialized size bound before directory/lock
creation, and publish a private replacement through the shared writer (0600 Unix;
protected current-user DACL in Windows source). No native ACL tests, filesystem
probes, network telemetry calls or build/test commands were executed for this
integration. Native storage and migration acceptance remain unproven.

## Current endpoint and first-run continuation — 2026-10-02

Current production source admits a configured base URL or a literal terminal
`/v1/traces` or `/v1/metrics`, including trailing slashes. The shared endpoint
projection removes one recognized suffix, preserves the preceding encoded path
prefix and constructs each signal URL from that canonical base. Readiness and
transport share the projection; a migrated full-signal C# endpoint no longer
produces doubled signal paths. Authored configuration is retained.

Endpoint admission is bounded to 8192 bytes and continues to refuse credentials,
queries and fragments. Repeated or ambiguous terminal signal suffixes are
refused. A single percent-decoding pass is used only to detect and refuse
encoded/case-varied reserved terminal spellings; decoded bytes never construct
a request URL. Arbitrary encoded prefixes remain encoded. Unknown signal names
are refused by the OTLP URL helper. Diagnostics describe the failing rule without
including the authored URL. No default remote host was added.

The desktop now consumes genuine missing-configuration `is_first_run` after its
owned settings load, with explicit allow/decline actions and usage/performance/
hashed-error preferences. Load-error defaults do not count as first-run admission.
The owned modal remains root-bound and survives until a matching save revision
and configuration lifecycle ticket settle. Both choices persist a false first-run
flag. Failed saves retain a retryable choice, keep event admission disabled and
preserve the current session identity. Save verification compares the reloaded
serialized configuration with the requested projection; intervening changes
cannot silently turn a decline into a different enablement choice.

Lifecycle configuration errors are now separate from persistent-log initialization
errors. A successful disable/join can acknowledge a saved decline even when
logging initialization failed; the logging diagnostic remains visible. Directory
changes and close invalidate the pending prompt and retain existing owned worker
shutdown. The ordinary settings editor cannot supersede a pending first-run
choice. The prompt does not install a signing key or infer exporter readiness
from consent alone.

This is source implementation under independent review. Earlier audit sections
remain historical and do not establish current runtime behavior. No first-run
desktop, transport, filesystem or CI acceptance was run for this continuation.
