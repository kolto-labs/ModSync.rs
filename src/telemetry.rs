//! Opt-in telemetry: consent configuration, HMAC request signing and an
//! OTLP/HTTP exporter using the JSON encoding.
//!
//! Parity oracle: C# `TelemetryConfiguration`, `TelemetryAuthenticator` and
//! `TelemetryService` plus the `telemetry-auth/` sidecar. Where this module is
//! stricter than C#, the reason is in `docs/parity/telemetry-csharp-audit.md`.
//!
//! Guarantees:
//! - No consent, no endpoint, or no signing secret means no exporter thread, no
//!   HTTP client and no socket. Every `record_*` call drops its data in the
//!   calling thread.
//! - Revoking consent (`configure` with a non-ready config) drops buffered
//!   data without a send, then joins the exporter thread. Once it returns,
//!   no telemetry thread, client or connection exists.
//! - Producers never block and memory is bounded: a fixed-size queue (full
//!   means drop and count), a span cap, and a per-instrument series cap.
//! - The signing secret is never serialized and never printed by `Debug`.
//! - Names, messages, paths and stack traces leave the process only as
//!   truncated SHA-256 hashes.

pub mod lifecycle;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub user_consented: bool,
    #[serde(default)]
    pub consent_date: Option<String>,
    #[serde(default)]
    pub anonymous_user_id: String,
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(default = "yes")]
    pub collect_usage_data: bool,
    #[serde(default = "yes")]
    pub collect_performance_metrics: bool,
    #[serde(default = "yes")]
    pub collect_crash_reports: bool,
    /// Kept for C# config compatibility. Nothing machine-identifying is sent.
    #[serde(default)]
    pub collect_machine_info: bool,
    #[serde(default = "yes")]
    pub enable_otlp_exporter: bool,
    /// OTLP base URL. `/v1/traces` and `/v1/metrics` are appended. Reads the
    /// C# key `otlp_endpoint` too. Unlike C#, there is no built-in default host.
    #[serde(default, alias = "otlp_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default = "default_client_version")]
    pub client_version: String,
    /// True when no configuration file existed. Frontends show the consent
    /// prompt when this is set (C# `IsFirstRun`).
    #[serde(default)]
    pub is_first_run: bool,
    #[serde(skip)]
    pub signing_secret: Option<String>,
}
fn default_client_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}
fn default_environment() -> String {
    "production".into()
}
fn yes() -> bool {
    true
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            user_consented: false,
            consent_date: None,
            anonymous_user_id: uuid::Uuid::new_v4().to_string(),
            environment: default_environment(),
            collect_usage_data: true,
            collect_performance_metrics: true,
            collect_crash_reports: true,
            collect_machine_info: false,
            enable_otlp_exporter: true,
            endpoint: String::new(),
            session_id: String::new(),
            client_version: default_client_version(),
            is_first_run: true,
            signing_secret: None,
        }
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("enabled", &self.enabled)
            .field("user_consented", &self.user_consented)
            .field("consent_date", &self.consent_date)
            .field("anonymous_user_id", &self.anonymous_user_id)
            .field("environment", &self.environment)
            .field("collect_usage_data", &self.collect_usage_data)
            .field(
                "collect_performance_metrics",
                &self.collect_performance_metrics,
            )
            .field("collect_crash_reports", &self.collect_crash_reports)
            .field("collect_machine_info", &self.collect_machine_info)
            .field("enable_otlp_exporter", &self.enable_otlp_exporter)
            .field("endpoint", &self.endpoint_display())
            .field("session_id", &self.session_id)
            .field("client_version", &self.client_version)
            .field("is_first_run", &self.is_first_run)
            .field(
                "signing_secret",
                &self.signing_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

fn read_config_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    crate::profiles::storage::read_bounded(path, limit)?
        .context("Telemetry configuration file is missing")
}

fn secret(directory: &Path) -> Result<Option<String>> {
    // Environment precedence remains unchanged. A present unsafe/unreadable key
    // is an error, not authority to silently use another directory's secret.
    let value = match std::env::var("MODSYNC_SIGNING_SECRET")
        .ok()
        .or_else(|| std::env::var("KOTORMODSYNC_SIGNING_SECRET").ok())
    {
        Some(value) => Some(value),
        None => {
            crate::profiles::storage::read_bounded(&directory.join("telemetry.key"), 16 * 1024)?
                .map(|bytes| {
                    String::from_utf8(bytes)
                        .map_err(|_| anyhow::anyhow!("Telemetry key is not UTF-8"))
                })
                .transpose()?
        }
    };
    Ok(value
        .filter(|value| value.len() <= 16 * 1024)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty()))
}

impl Config {
    /// Preserve implicit-default migration authority. An explicit settings root
    /// never consults the global legacy directory. Current configuration wins;
    /// the legacy file/key remain untouched when a later save copies forward.
    pub fn load_from_settings_directory(explicit: Option<&Path>) -> Result<Self> {
        let directory = crate::profiles::settings_directory(explicit)?;
        if explicit.is_some() {
            return Self::load(&directory);
        }
        let legacy = dirs::config_dir().map(|base| base.join("KOTORModSync"));
        let path = directory.join("telemetry_config.json");
        let current_missing = !crate::profiles::storage::exists(&path)?;
        let mut config = if current_missing {
            if let Some(legacy) = legacy.as_ref()
                && crate::profiles::storage::exists(&legacy.join("telemetry_config.json"))?
            {
                Self::load_config(legacy)?
            } else {
                Self::load_config(&directory)?
            }
        } else {
            Self::load_config(&directory)?
        };
        // A copied-forward config deliberately does not serialize its secret.
        // Retain read-only access to the old key only for the implicit store.
        config.signing_secret = secret(&directory)?.or(config.signing_secret);
        if config.signing_secret.is_none()
            && let Some(legacy) = legacy.as_deref()
        {
            config.signing_secret = secret(legacy)?;
        }
        Ok(config)
    }

    pub fn load(directory: &Path) -> Result<Self> {
        let mut config = Self::load_config(directory)?;
        config.signing_secret = secret(directory)?;
        Ok(config)
    }

    fn load_config(directory: &Path) -> Result<Self> {
        let path = directory.join("telemetry_config.json");
        crate::profiles::storage::directory(directory, false)?;
        let mut config = if crate::profiles::storage::exists(&path)? {
            let bytes = read_config_file(&path, 1024 * 1024)?;
            let mut config: Self = serde_json::from_slice(&bytes)
                .map_err(|_| anyhow::anyhow!("Invalid telemetry configuration JSON"))?;
            config.is_first_run = false;
            config
        } else {
            Self::default()
        };
        if config.anonymous_user_id.trim().is_empty() {
            config.anonymous_user_id = uuid::Uuid::new_v4().to_string();
        }
        config.session_id = uuid::Uuid::new_v4().to_string();
        Ok(config)
    }

    pub fn save(&self, directory: &Path) -> Result<()> {
        let path = directory.join("telemetry_config.json");
        let bytes = serde_json::to_vec_pretty(self)?;
        anyhow::ensure!(
            bytes.len() <= 1024 * 1024,
            "Telemetry configuration exceeds 1 MiB"
        );
        crate::profiles::storage::directory(directory, true)?;
        let _lock = crate::profiles::storage::lock(&directory.join("profiles"))?;
        if let Some(existing) = crate::profiles::storage::read_bounded(&path, 1024 * 1024)? {
            serde_json::from_slice::<Self>(&existing).map_err(|_| {
                anyhow::anyhow!("Refusing to replace malformed telemetry configuration")
            })?;
        }
        crate::profiles::storage::write(&path, &bytes)
    }

    pub fn set_consent(&mut self, enabled: bool) {
        self.user_consented = enabled;
        self.enabled = enabled;
        self.consent_date = enabled.then(|| chrono::Utc::now().to_rfc3339());
    }

    /// Consent, enablement and a signing secret are all present.
    pub fn active(&self) -> bool {
        self.enabled
            && self.user_consented
            && self
                .signing_secret
                .as_deref()
                .is_some_and(|secret| !secret.is_empty())
    }

    /// `active()` plus an OTLP exporter that has somewhere to send.
    pub fn exporter_ready(&self) -> bool {
        self.active() && self.enable_otlp_exporter && otlp_base(&self.endpoint).is_some()
    }

    /// URL for diagnostics: credentials, query and fragment never leave the editor.
    pub fn endpoint_display(&self) -> String {
        if self.endpoint.trim().is_empty() {
            return "(none)".into();
        }
        let Ok(mut url) = reqwest::Url::parse(self.endpoint.trim()) else {
            return "(invalid endpoint)".into();
        };
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        url.to_string()
    }

    /// Plain-text list of what is and is not collected (C# `GetPrivacySummary`).
    pub fn privacy_summary(&self) -> String {
        if !self.enabled || !self.user_consented {
            return "Telemetry is off. Nothing is collected.".into();
        }
        let mut summary = String::from(if self.exporter_ready() {
            "Telemetry is ready. ModSync collects:\n\n"
        } else {
            "Telemetry is currently inactive. When configured, ModSync collects:\n\n"
        });
        if self.collect_usage_data {
            summary.push_str("- Usage data: which features you use\n");
        }
        if self.collect_performance_metrics {
            summary.push_str("- Performance data: how long operations take\n");
        }
        if self.collect_crash_reports {
            summary.push_str("- Error reports, as hashes only\n");
        }
        summary.push_str("\nModSync does not collect:\n");
        summary.push_str("- Personal information such as your name or email\n");
        summary.push_str("- File contents or mod names, which are sent as hashes\n");
        summary.push_str("- Passwords or login tokens\n");
        summary.push_str("- Your machine name\n");
        summary.push_str(&format!(
            "\nAnonymous user ID: {}\n",
            self.anonymous_user_id
        ));
        summary.push_str(&format!("Session ID: {}\n", self.session_id));
        if self.enable_otlp_exporter && !self.endpoint.trim().is_empty() {
            summary.push_str(&format!("\nData goes to: {}\n", self.endpoint_display()));
        }
        if !self.active() {
            summary.push_str("\nNothing is sent until a signing key is installed.\n");
        }
        summary.push_str("\nYou can turn telemetry off at any time in Settings.\n");
        summary
    }
}

/// Legacy single-event payload used by `export`.
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub name: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Export {
    /// Inactive: no consent, no endpoint, no secret or no exporter.
    Disabled,
    Sent,
    /// Active, but nothing was buffered.
    Empty,
    /// Active, but the send failed or timed out. Details go to the app log.
    Failed,
}

/// HMAC-SHA256 over `POST|{path}|{timestamp}|{session}`, lowercase hex. This
/// is the exact message `telemetry-auth/auth_service.py` recomputes.
pub fn signature(secret: &str, session: &str, path: &str, timestamp: i64) -> String {
    let mut key = secret.as_bytes().to_vec();
    if key.len() > 64 {
        key = Sha256::digest(&key).to_vec();
    }
    key.resize(64, 0);
    let message = format!("POST|{path}|{timestamp}|{session}");
    let mut inner = Sha256::new();
    inner.update(key.iter().map(|byte| byte ^ 0x36).collect::<Vec<_>>());
    inner.update(message.as_bytes());
    let mut outer = Sha256::new();
    outer.update(key.iter().map(|byte| byte ^ 0x5c).collect::<Vec<_>>());
    outer.update(inner.finalize());
    outer
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Legacy one-shot POST of a single `Event` to `{endpoint}{path}`.
///
/// This is not OTLP: no OTLP collector serves `/v1/events`. New callers use
/// `global().record_event(..)`, which batches into signed OTLP requests.
pub fn export(config: &Config, path: &str, event: &Event) -> Result<Export> {
    if !config.active() || !config.collect_usage_data || config.endpoint.trim().is_empty() {
        return Ok(Export::Disabled);
    }
    anyhow::ensure!(path.starts_with('/'), "Telemetry path must start with '/'");
    anyhow::ensure!(
        !config.session_id.trim().is_empty(),
        "Telemetry session ID is required"
    );
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock predates Unix epoch")?
        .as_secs() as i64;
    let endpoint = format!(
        "{}{}",
        otlp_base(&config.endpoint).context("Invalid telemetry endpoint")?,
        path
    );
    let body = serde_json::to_vec(event).context("Serialize telemetry event")?;
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Build telemetry HTTP client")?;
    let response = client
        .post(&endpoint)
        .header(
            "X-KMS-Signature",
            signature(
                config.signing_secret.as_deref().unwrap(),
                &config.session_id,
                path,
                timestamp,
            ),
        )
        .header("X-KMS-Timestamp", timestamp.to_string())
        .header("X-KMS-Session-ID", &config.session_id)
        .header("X-KMS-Client-Version", &config.client_version)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .with_context(|| format!("Send telemetry event to {endpoint}"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "Telemetry endpoint returned {}",
        response.status()
    );
    Ok(Export::Sent)
}

// ---------------------------------------------------------------------------
// Hashing and attributes
// ---------------------------------------------------------------------------

pub type Attributes = BTreeMap<String, Value>;

/// First 16 uppercase hex digits of SHA-256, `"empty"` for empty input
/// (C# `TelemetryService.HashString`).
pub fn hash_string(input: &str) -> String {
    if input.is_empty() {
        return "empty".into();
    }
    Sha256::digest(input.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn attributes<const N: usize>(pairs: [(&str, Value); N]) -> Attributes {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn with_event_name(event_name: &str, mut tags: Attributes) -> Attributes {
    tags.insert("event.name".into(), Value::String(event_name.into()));
    tags
}

fn add_error_hash(tags: &mut Attributes, success: bool, error: Option<&str>) {
    if let Some(error) = error.filter(|error| !success && !error.is_empty()) {
        tags.insert("error.type".into(), Value::String(hash_string(error)));
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

// ---------------------------------------------------------------------------
// OTLP/JSON encoding (pure functions; no I/O)
// ---------------------------------------------------------------------------

pub const SCOPE_NAME: &str = "ModSync";
const DEFAULT_BOUNDS: [f64; 15] = [
    0.0, 5.0, 10.0, 25.0, 50.0, 75.0, 100.0, 250.0, 500.0, 750.0, 1000.0, 2500.0, 5000.0, 7500.0,
    10000.0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Instrument {
    Counter,
    Histogram,
}

struct MetricSpec {
    name: &'static str,
    unit: &'static str,
    description: &'static str,
    instrument: Instrument,
}

/// The instruments C# creates in `TelemetryService.Initialize`.
const METRICS: &[MetricSpec] = &[
    spec(
        "modsync.events",
        "events",
        "Number of events recorded",
        Instrument::Counter,
    ),
    spec(
        "modsync.errors",
        "errors",
        "Number of errors recorded",
        Instrument::Counter,
    ),
    spec(
        "modsync.operation.duration",
        "ms",
        "Duration of operations",
        Instrument::Histogram,
    ),
    spec(
        "modsync.mods.installed",
        "mods",
        "Number of mods installed",
        Instrument::Counter,
    ),
    spec(
        "modsync.mods.validated",
        "mods",
        "Number of mods validated",
        Instrument::Counter,
    ),
    spec(
        "modsync.downloads",
        "downloads",
        "Number of downloads",
        Instrument::Counter,
    ),
    spec(
        "modsync.download.size",
        "bytes",
        "Size of downloads",
        Instrument::Histogram,
    ),
    spec(
        "modsync.cache.hits",
        "hits",
        "Number of cache hits",
        Instrument::Counter,
    ),
    spec(
        "modsync.cache.misses",
        "misses",
        "Number of cache misses",
        Instrument::Counter,
    ),
    spec(
        "modsync.cache.size",
        "bytes",
        "Total cache size in bytes",
        Instrument::Counter,
    ),
    spec(
        "modsync.cache.integrity.verified",
        "verifications",
        "Number of integrity verifications",
        Instrument::Counter,
    ),
    spec(
        "modsync.cache.operation.duration",
        "ms",
        "Duration of cache operations",
        Instrument::Histogram,
    ),
];

const fn spec(
    name: &'static str,
    unit: &'static str,
    description: &'static str,
    instrument: Instrument,
) -> MetricSpec {
    MetricSpec {
        name,
        unit,
        description,
        instrument,
    }
}

/// OTLP `AnyValue`. int64 is a decimal string per the OTLP/JSON mapping.
pub fn otlp_any_value(value: &Value) -> Value {
    match value {
        Value::String(text) => json!({ "stringValue": text }),
        Value::Bool(flag) => json!({ "boolValue": flag }),
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            json!({ "intValue": number.to_string() })
        }
        Value::Number(number) => json!({ "doubleValue": number.as_f64() }),
        Value::Array(items) => {
            json!({ "arrayValue": { "values": items.iter().map(otlp_any_value).collect::<Vec<_>>() } })
        }
        Value::Null => json!({ "stringValue": "" }),
        Value::Object(_) => json!({ "stringValue": value.to_string() }),
    }
}

/// OTLP `KeyValue` list.
pub fn otlp_attributes(attributes: &Attributes) -> Value {
    Value::Array(
        attributes
            .iter()
            .map(|(key, value)| json!({ "key": key, "value": otlp_any_value(value) }))
            .collect(),
    )
}

/// Resource attributes (C# `ResourceBuilder.AddService(..).AddAttributes(..)`).
pub fn resource_attributes(config: &Config) -> Attributes {
    attributes([
        ("service.name", json!("ModSync")),
        ("service.version", json!(config.client_version)),
        ("service.instance.id", json!(config.session_id)),
        ("telemetry.sdk.language", json!("rust")),
        ("user.id", json!(config.anonymous_user_id)),
        ("session.id", json!(config.session_id)),
        ("environment", json!(config.environment)),
        ("platform", json!(std::env::consts::OS)),
    ])
}

#[derive(Clone, Debug)]
pub struct SpanData {
    pub trace_id: String,
    pub span_id: String,
    pub name: String,
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
    pub attributes: Attributes,
    /// `None` = unset, `Some(true)` = ok, `Some(false)` = error.
    pub success: Option<bool>,
}

/// `ExportTraceServiceRequest` as OTLP/JSON.
pub fn encode_traces(resource: &Attributes, version: &str, spans: &[SpanData]) -> Value {
    let spans: Vec<Value> = spans
        .iter()
        .map(|span| {
            let code = match span.success {
                None => 0,
                Some(true) => 1,
                Some(false) => 2,
            };
            json!({
                "traceId": span.trace_id,
                "spanId": span.span_id,
                "name": span.name,
                "kind": 1,
                "startTimeUnixNano": span.start_unix_nano.to_string(),
                "endTimeUnixNano": span.end_unix_nano.to_string(),
                "attributes": otlp_attributes(&span.attributes),
                "status": { "code": code },
            })
        })
        .collect();
    json!({
        "resourceSpans": [{
            "resource": { "attributes": otlp_attributes(resource) },
            "scopeSpans": [{
                "scope": { "name": SCOPE_NAME, "version": version },
                "spans": spans,
            }],
        }],
    })
}

#[derive(Clone, Debug)]
enum Point {
    Sum(i64),
    Histogram {
        count: u64,
        sum: f64,
        min: f64,
        max: f64,
        buckets: Vec<u64>,
    },
}

/// Distinct attribute sets kept per instrument before new ones fold into the
/// overflow series (the OTel SDK default cardinality limit).
pub const MAX_SERIES_PER_METRIC: usize = 2000;
/// Attribute marking the overflow series (OTel SDK convention).
pub const OVERFLOW_ATTRIBUTE: &str = "otel.metric.overflow";

/// Cumulative in-process aggregation, the C# OTLP exporter's default
/// temporality. Series are keyed by metric name, then attribute set.
#[derive(Clone, Debug)]
pub struct Metrics {
    start_unix_nano: u64,
    series: BTreeMap<&'static str, BTreeMap<String, (Attributes, Point)>>,
}

impl Metrics {
    pub fn new(start_unix_nano: u64) -> Self {
        Self {
            start_unix_nano,
            series: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    fn add(&mut self, name: &'static str, value: f64, mut tags: Attributes) {
        let Some(spec) = METRICS.iter().find(|spec| spec.name == name) else {
            return;
        };
        let mut key = serde_json::to_string(&tags).unwrap_or_default();
        let series = self.series.entry(spec.name).or_default();
        if !series.contains_key(&key) && series.len() >= MAX_SERIES_PER_METRIC {
            // OTel SDK cardinality-limit behaviour: fold new attribute sets
            // into one overflow series instead of growing without bound.
            tags = attributes([(OVERFLOW_ATTRIBUTE, json!(true))]);
            key = serde_json::to_string(&tags).unwrap_or_default();
            // The value is kept, its attributes are not: count the loss.
            count_dropped(1);
        }
        let entry = series.entry(key).or_insert_with(|| {
            let point = match spec.instrument {
                Instrument::Counter => Point::Sum(0),
                Instrument::Histogram => Point::Histogram {
                    count: 0,
                    sum: 0.0,
                    min: f64::INFINITY,
                    max: f64::NEG_INFINITY,
                    buckets: vec![0; DEFAULT_BOUNDS.len() + 1],
                },
            };
            (tags, point)
        });
        match &mut entry.1 {
            Point::Sum(total) => *total = total.saturating_add(value as i64),
            Point::Histogram {
                count,
                sum,
                min,
                max,
                buckets,
            } => {
                *count += 1;
                *sum += value;
                *min = min.min(value);
                *max = max.max(value);
                let index = DEFAULT_BOUNDS
                    .iter()
                    .position(|bound| value <= *bound)
                    .unwrap_or(DEFAULT_BOUNDS.len());
                buckets[index] += 1;
            }
        }
    }
}

/// `ExportMetricsServiceRequest` as OTLP/JSON, temporality CUMULATIVE (2).
pub fn encode_metrics(
    resource: &Attributes,
    version: &str,
    metrics: &Metrics,
    now_unix_nano: u64,
) -> Value {
    let start = metrics.start_unix_nano.to_string();
    let now = now_unix_nano.to_string();
    let encoded: Vec<Value> = metrics
        .series
        .iter()
        .filter_map(|(name, series)| {
            let spec = METRICS.iter().find(|spec| spec.name == *name)?;
            let points: Vec<Value> = series
                .values()
                .map(|(tags, point)| match point {
                    Point::Sum(total) => json!({
                        "attributes": otlp_attributes(tags),
                        "startTimeUnixNano": start,
                        "timeUnixNano": now,
                        "asInt": total.to_string(),
                    }),
                    Point::Histogram {
                        count,
                        sum,
                        min,
                        max,
                        buckets,
                    } => json!({
                        "attributes": otlp_attributes(tags),
                        "startTimeUnixNano": start,
                        "timeUnixNano": now,
                        "count": count.to_string(),
                        "sum": sum,
                        "min": min,
                        "max": max,
                        "bucketCounts": buckets.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "explicitBounds": DEFAULT_BOUNDS,
                    }),
                })
                .collect();
            let data = match spec.instrument {
                Instrument::Counter => json!({
                    "sum": {
                        "aggregationTemporality": 2,
                        "isMonotonic": true,
                        "dataPoints": points,
                    }
                }),
                Instrument::Histogram => json!({
                    "histogram": {
                        "aggregationTemporality": 2,
                        "dataPoints": points,
                    }
                }),
            };
            let mut metric = json!({
                "name": spec.name,
                "unit": spec.unit,
                "description": spec.description,
            });
            if let (Some(metric), Some(data)) = (metric.as_object_mut(), data.as_object()) {
                metric.extend(data.clone());
            }
            Some(metric)
        })
        .collect();
    json!({
        "resourceMetrics": [{
            "resource": { "attributes": otlp_attributes(resource) },
            "scopeMetrics": [{
                "scope": { "name": SCOPE_NAME, "version": version },
                "metrics": encoded,
            }],
        }],
    })
}

/// Trimmed base URL, or `None` unless it is an absolute http(s) URL.
fn otlp_base(endpoint: &str) -> Option<String> {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(trimmed).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then(|| trimmed.to_owned())
}

/// `{base}/v1/{signal}`, the OTLP/HTTP signal URL.
pub fn otlp_url(endpoint: &str, signal: &str) -> Option<String> {
    otlp_base(endpoint).map(|base| format!("{base}/v1/{signal}"))
}

fn now_unix_nano() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn random_hex(len: usize) -> String {
    let mut hex = String::new();
    while hex.len() < len {
        hex.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    hex.truncate(len);
    hex
}

// ---------------------------------------------------------------------------
// Exporter
// ---------------------------------------------------------------------------

/// How often buffered data is sent while the app runs (OTel SDK metric
/// reader default).
pub const EXPORT_INTERVAL: Duration = Duration::from_secs(60);
const MAX_BUFFERED_SPANS: usize = 2048;
/// Commands that may wait for the exporter thread. Producers never block:
/// when the queue is full the record is dropped and counted.
pub const QUEUE_CAPACITY: usize = 4096;
/// Whole-request timeout. Revocation and shutdown join the exporter, so this
/// bounds how long they can block on a POST that is already on the wire.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// How often an idle exporter re-checks its cancel flag.
const CANCEL_POLL: Duration = Duration::from_millis(200);

static DROPPED: AtomicU64 = AtomicU64::new(0);

fn count_dropped(count: u64) {
    DROPPED.fetch_add(count, Ordering::Relaxed);
}

enum Command {
    Metric(&'static str, f64, Attributes),
    Span(SpanData),
    Flush(mpsc::Sender<Export>),
    /// Replace the exporter's config (secret, endpoint, resource) in place.
    Reconfigure(Box<Config>),
}

#[derive(Clone, Copy, Default)]
struct Gates {
    usage: bool,
    performance: bool,
    crash: bool,
}

struct Worker {
    sender: mpsc::SyncSender<Command>,
    cancelled: Arc<AtomicBool>,
    gates: Gates,
    handle: std::thread::JoinHandle<()>,
}

impl Worker {
    /// Cancel and wait for the thread. On return the thread has exited and
    /// its HTTP client (and pooled connections) are dropped. Blocks for up to
    /// `REQUEST_TIMEOUT` when a POST is already on the wire, or until the
    /// system resolver gives up on a stalled lookup: dropping the blocking
    /// reqwest client joins its runtime, which waits for a pending
    /// `getaddrinfo` call.
    fn stop(self) {
        self.cancelled.store(true, Ordering::SeqCst);
        drop(self.sender);
        if self.handle.join().is_err() {
            crate::logging::warning("[Telemetry] Exporter thread panicked");
        }
    }
}

/// Never blocks the caller. A full queue drops the record and counts it.
fn enqueue(sender: &mpsc::SyncSender<Command>, command: Command) {
    if let Err(mpsc::TrySendError::Full(_)) = sender.try_send(command) {
        count_dropped(1);
    }
}

/// Process-wide recorder. Starts inactive: no thread, no client.
pub struct Telemetry {
    worker: Mutex<Option<Worker>>,
}

/// The process-wide recorder (C# `TelemetryService.Instance`).
pub fn global() -> &'static Telemetry {
    static GLOBAL: OnceLock<Telemetry> = OnceLock::new();
    GLOBAL.get_or_init(Telemetry::new)
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            worker: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Worker>> {
        self.worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Whether an exporter thread is alive.
    pub fn is_running(&self) -> bool {
        self.lock()
            .as_ref()
            .is_some_and(|worker| !worker.handle.is_finished())
    }

    /// Request revocation without waiting for HTTP or resolver shutdown. Frontends
    /// must subsequently await configure(off) or shutdown before reporting stopped.
    pub fn request_stop(&self) {
        if let Some(worker) = self.lock().as_mut() {
            worker.gates = Gates::default();
            worker.cancelled.store(true, Ordering::SeqCst);
        }
    }

    /// Records dropped since process start: queue full, span buffer full, or
    /// a metric attribute set folded into the overflow series.
    pub fn dropped_records(&self) -> u64 {
        DROPPED.load(Ordering::Relaxed)
    }

    /// Panic-hook recorder: never waits for a mutex, queue, network or flush.
    /// A panic while the exporter slot is borrowed must still unwind normally.
    pub(crate) fn try_record_panic(&self, message: &str, location: &str) {
        let sender = match self.worker.try_lock() {
            Ok(slot) => slot
                .as_ref()
                .filter(|worker| worker.gates.crash && !worker.cancelled.load(Ordering::Acquire))
                .map(|worker| worker.sender.clone()),
            Err(_) => None,
        };
        if let Some(sender) = sender {
            let tags = attributes([
                ("error.type", json!("panic")),
                ("error.message.hash", json!(hash_string(message))),
                ("error.stacktrace.hash", json!(hash_string(location))),
            ]);
            enqueue(
                &sender,
                Command::Metric("modsync.errors", 1.0, with_event_name("error", tags)),
            );
        }
    }

    /// Apply a configuration (C# `Initialize` + `UpdateConfiguration`).
    ///
    /// When `config.exporter_ready()` is false, any running exporter is
    /// cancelled and joined before this returns, and its buffer is dropped
    /// without a send: after return there is no telemetry thread, client or
    /// socket. That can block for up to `REQUEST_TIMEOUT`, or until the system
    /// resolver gives up on a stalled lookup.
    ///
    /// When ready, a running exporter is refreshed in place: the new secret,
    /// endpoint and resource take effect for the next POST and buffered data
    /// is kept. A new session first sends the old session's data under the
    /// old config, then starts empty. The refresh is a blocking send, so with
    /// a full queue it can wait for an export in progress (two POSTs, about
    /// `2 * REQUEST_TIMEOUT`). A dead exporter is replaced. Returns whether
    /// an exporter is running afterwards.
    ///
    /// Every call can block, so frontends must call it off the UI thread.
    pub fn configure(&self, config: &Config) -> bool {
        let gates = Gates {
            usage: config.collect_usage_data,
            performance: config.collect_performance_metrics,
            crash: config.collect_crash_reports,
        };
        let mut slot = self.lock();
        if !config.exporter_ready() {
            let previous = slot.take();
            drop(slot);
            if let Some(worker) = previous {
                worker.stop();
                crate::logging::verbose("[Telemetry] Exporter stopped; buffered data dropped");
            }
            return false;
        }
        if slot.as_ref().is_some_and(|worker| {
            !worker.handle.is_finished() && !worker.cancelled.load(Ordering::SeqCst)
        }) {
            let Some(worker) = slot.as_mut() else {
                return false;
            };
            worker.gates = gates;
            let sender = worker.sender.clone();
            drop(slot);
            // Blocking send: a refresh must not be dropped. The exporter
            // drains the queue unless a POST is in flight (bounded by
            // REQUEST_TIMEOUT). FIFO order means every later POST sees it.
            return sender
                .send(Command::Reconfigure(Box::new(config.clone())))
                .is_ok();
        }
        let dead = slot.take();
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let thread_config = config.clone();
        let thread_cancelled = cancelled.clone();
        let spawned = std::thread::Builder::new()
            .name("modsync-telemetry".into())
            .spawn(move || run_exporter(thread_config, receiver, thread_cancelled));
        let handle = match spawned {
            Ok(handle) => handle,
            Err(error) => {
                drop(slot);
                if let Some(worker) = dead {
                    worker.stop();
                }
                crate::logging::warning(format!("[Telemetry] Could not start exporter: {error}"));
                return false;
            }
        };
        *slot = Some(Worker {
            sender,
            cancelled,
            gates,
            handle,
        });
        drop(slot);
        if let Some(worker) = dead {
            worker.stop();
        }
        crate::logging::info("[Telemetry] Telemetry exporter started");
        true
    }

    /// Stop the exporter and join it. With `flush`, buffered data is sent
    /// first, waiting at most `timeout`. Call at app exit after
    /// `record_session_end`.
    pub fn shutdown(&self, flush: bool, timeout: Duration) -> Export {
        let result = if flush {
            self.flush(timeout)
        } else {
            Export::Disabled
        };
        let worker = self.lock().take();
        if let Some(worker) = worker {
            worker.stop();
        }
        result
    }

    /// Send buffered data now and wait up to `timeout` (C# `Flush`). Blocks:
    /// never call from a UI thread or inside an async runtime.
    pub fn flush(&self, timeout: Duration) -> Export {
        let (reply, answer) = mpsc::channel();
        let sender = {
            let slot = self.lock();
            let Some(worker) = slot.as_ref() else {
                return Export::Disabled;
            };
            worker.sender.clone()
        };
        if sender.try_send(Command::Flush(reply)).is_err() {
            return Export::Failed;
        }
        answer.recv_timeout(timeout).unwrap_or(Export::Failed)
    }

    fn metric(&self, gate: fn(Gates) -> bool, name: &'static str, value: f64, tags: Attributes) {
        let slot = self.lock();
        if let Some(worker) = slot.as_ref().filter(|worker| gate(worker.gates)) {
            enqueue(&worker.sender, Command::Metric(name, value, tags));
        }
    }

    fn usage_enabled(&self) -> bool {
        self.lock()
            .as_ref()
            .is_some_and(|worker| worker.gates.usage)
    }

    fn performance_enabled(&self) -> bool {
        self.lock()
            .as_ref()
            .is_some_and(|worker| worker.gates.performance)
    }

    fn counted(&self, metric: &'static str, event: &str, tags: Attributes) {
        self.metric(|g| g.usage, metric, 1.0, with_event_name(event, tags));
    }

    fn timed(&self, event: &str, tags: &Attributes, duration: Duration) {
        self.metric(
            |g| g.usage && g.performance,
            "modsync.operation.duration",
            millis(duration),
            with_event_name(event, tags.clone()),
        );
    }

    /// Generic counted event (C# `RecordEvent`). Attribute values must not
    /// carry names, paths or free text; hash them with `hash_string` first.
    pub fn record_event(&self, name: &str, tags: Attributes) {
        if self.usage_enabled() {
            self.counted("modsync.events", name, tags);
        }
    }

    /// A finished GUI/CLI job, e.g. `installation`, `download`, `validate`.
    pub fn record_operation(&self, operation: &str, success: bool, duration: Duration) {
        if !self.usage_enabled() {
            return;
        }
        let tags = attributes([("operation", json!(operation)), ("success", json!(success))]);
        self.counted("modsync.events", "operation.completed", tags.clone());
        self.timed("operation.completed", &tags, duration);
    }

    pub fn record_ui_interaction(&self, element: &str, action: &str) {
        if self.usage_enabled() {
            let tags = attributes([("element", json!(element)), ("action", json!(action))]);
            self.counted("modsync.events", "ui.interaction", tags);
        }
    }

    pub fn record_mod_installation(
        &self,
        mod_name: &str,
        success: bool,
        duration: Duration,
        error: Option<&str>,
    ) {
        if !self.usage_enabled() {
            return;
        }
        let mut tags = attributes([
            ("mod.name.hash", json!(hash_string(mod_name))),
            ("success", json!(success)),
        ]);
        add_error_hash(&mut tags, success, error);
        self.counted("modsync.mods.installed", "mod.install", tags.clone());
        self.timed("mod.install", &tags, duration);
    }

    pub fn record_download(
        &self,
        mod_name: &str,
        success: bool,
        duration: Duration,
        bytes: u64,
        source: Option<&str>,
        error: Option<&str>,
    ) {
        if !self.usage_enabled() {
            return;
        }
        let mut tags = attributes([
            ("mod.name.hash", json!(hash_string(mod_name))),
            ("success", json!(success)),
            ("download.source", json!(source.unwrap_or("unknown"))),
        ]);
        add_error_hash(&mut tags, success, error);
        self.counted("modsync.downloads", "download", tags.clone());
        if bytes > 0 {
            self.metric(
                |g| g.usage,
                "modsync.download.size",
                bytes as f64,
                with_event_name("download.size", tags.clone()),
            );
        }
        self.timed("download", &tags, duration);
    }

    pub fn record_validation(
        &self,
        kind: &str,
        success: bool,
        issue_count: usize,
        duration: Duration,
    ) {
        if !self.usage_enabled() {
            return;
        }
        let tags = attributes([
            ("validation.type", json!(kind)),
            ("success", json!(success)),
            ("issue.count", json!(issue_count)),
        ]);
        self.counted("modsync.mods.validated", "validation", tags.clone());
        self.timed("validation", &tags, duration);
    }

    pub fn record_file_operation(
        &self,
        kind: &str,
        success: bool,
        file_count: usize,
        duration: Duration,
        error: Option<&str>,
    ) {
        if !self.usage_enabled() {
            return;
        }
        let mut tags = attributes([
            ("operation.type", json!(kind)),
            ("success", json!(success)),
            ("file.count", json!(file_count)),
        ]);
        add_error_hash(&mut tags, success, error);
        self.counted("modsync.events", "file.operation", tags.clone());
        self.timed("file.operation", &tags, duration);
    }

    pub fn record_component_execution(
        &self,
        component_name: &str,
        success: bool,
        instruction_count: usize,
        duration: Duration,
        error: Option<&str>,
    ) {
        if !self.usage_enabled() {
            return;
        }
        let mut tags = attributes([
            ("component.name.hash", json!(hash_string(component_name))),
            ("success", json!(success)),
            ("instruction.count", json!(instruction_count)),
        ]);
        add_error_hash(&mut tags, success, error);
        self.counted("modsync.events", "component.execution", tags.clone());
        self.timed("component.execution", &tags, duration);
    }

    pub fn record_parsing_operation(
        &self,
        file_type: &str,
        success: bool,
        component_count: usize,
        duration: Duration,
        error: Option<&str>,
    ) {
        if !self.usage_enabled() {
            return;
        }
        let mut tags = attributes([
            ("file.type", json!(file_type)),
            ("success", json!(success)),
            ("component.count", json!(component_count)),
        ]);
        add_error_hash(&mut tags, success, error);
        self.counted("modsync.events", "parsing", tags.clone());
        self.timed("parsing", &tags, duration);
    }

    pub fn record_session_start(&self, component_count: usize, selected_count: usize) {
        if self.usage_enabled() {
            let tags = attributes([
                ("component.total", json!(component_count)),
                ("component.selected", json!(selected_count)),
            ]);
            self.counted("modsync.events", "session.start", tags);
        }
    }

    pub fn record_session_end(&self, duration: Duration, completed: bool) {
        if !self.usage_enabled() {
            return;
        }
        let tags = attributes([("completed", json!(completed))]);
        self.counted("modsync.events", "session.end", tags.clone());
        self.timed("session", &tags, duration);
    }

    /// Error report; message and stack are hashed (C# `RecordError`).
    pub fn record_error(&self, error_type: &str, message: &str, stack: Option<&str>) {
        let mut tags = attributes([
            ("error.type", json!(error_type)),
            ("error.message.hash", json!(hash_string(message))),
        ]);
        if let Some(stack) = stack.filter(|stack| !stack.is_empty()) {
            tags.insert("error.stacktrace.hash".into(), json!(hash_string(stack)));
        }
        self.metric(
            |g| g.crash,
            "modsync.errors",
            1.0,
            with_event_name("error", tags),
        );
    }

    pub fn record_cache_hit(&self, provider: &str, content_type: Option<&str>) {
        let mut tags = attributes([("provider", json!(provider))]);
        if let Some(content_type) = content_type.filter(|value| !value.is_empty()) {
            tags.insert("content.type".into(), json!(content_type));
        }
        self.counted("modsync.cache.hits", "cache.hit", tags);
    }

    pub fn record_cache_miss(&self, provider: &str, reason: Option<&str>) {
        let mut tags = attributes([("provider", json!(provider))]);
        if let Some(reason) = reason.filter(|value| !value.is_empty()) {
            tags.insert("reason".into(), json!(reason));
        }
        self.counted("modsync.cache.misses", "cache.miss", tags);
    }

    pub fn record_cache_size(&self, bytes: u64, provider: Option<&str>) {
        let mut tags = Attributes::new();
        if let Some(provider) = provider.filter(|value| !value.is_empty()) {
            tags.insert("provider".into(), json!(provider));
        }
        self.metric(
            |g| g.usage,
            "modsync.cache.size",
            bytes as f64,
            with_event_name("cache.size", tags),
        );
    }

    pub fn record_integrity_verification(&self, success: bool, kind: Option<&str>) {
        let mut tags = attributes([("success", json!(success))]);
        if let Some(kind) = kind.filter(|value| !value.is_empty()) {
            tags.insert("type".into(), json!(kind));
        }
        self.counted(
            "modsync.cache.integrity.verified",
            "integrity.verification",
            tags,
        );
    }

    /// C# sends the raw error text here; this port sends its hash.
    pub fn record_cache_operation(
        &self,
        kind: &str,
        success: bool,
        duration: Duration,
        provider: Option<&str>,
        error: Option<&str>,
    ) {
        let mut tags = attributes([("operation.type", json!(kind)), ("success", json!(success))]);
        if let Some(provider) = provider.filter(|value| !value.is_empty()) {
            tags.insert("provider".into(), json!(provider));
        }
        if let Some(error) = error.filter(|value| !value.is_empty()) {
            tags.insert("error.hash".into(), json!(hash_string(error)));
        }
        self.metric(
            |g| g.usage,
            "modsync.cache.operation.duration",
            millis(duration),
            with_event_name("cache.operation", tags),
        );
    }

    /// Start a trace span (C# `StartActivity`). Inactive telemetry returns an
    /// inert span. The span is sent when `end` is called or it is dropped.
    pub fn start_span(&self, name: &str, tags: Attributes) -> Span {
        let sender = self
            .performance_enabled()
            .then(|| self.lock().as_ref().map(|worker| worker.sender.clone()))
            .flatten();
        Span {
            sender,
            data: SpanData {
                trace_id: random_hex(32),
                span_id: random_hex(16),
                name: name.to_owned(),
                start_unix_nano: now_unix_nano(),
                end_unix_nano: 0,
                attributes: tags,
                success: None,
            },
            started: Instant::now(),
        }
    }
}

/// A running span. Cheap and inert when telemetry is inactive.
pub struct Span {
    sender: Option<mpsc::SyncSender<Command>>,
    data: SpanData,
    started: Instant,
}

impl Span {
    pub fn is_recording(&self) -> bool {
        self.sender.is_some()
    }

    pub fn set_attribute(&mut self, key: &str, value: Value) {
        if self.sender.is_some() {
            self.data.attributes.insert(key.into(), value);
        }
    }

    /// Finish with an ok/error status.
    pub fn end(mut self, success: bool) {
        self.data.success = Some(success);
        self.finish();
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn finish(&mut self) {
        if let Some(sender) = self.sender.take() {
            let mut data = std::mem::replace(
                &mut self.data,
                SpanData {
                    trace_id: String::new(),
                    span_id: String::new(),
                    name: String::new(),
                    start_unix_nano: 0,
                    end_unix_nano: 0,
                    attributes: Attributes::new(),
                    success: None,
                },
            );
            data.end_unix_nano = data
                .start_unix_nano
                .saturating_add(self.started.elapsed().as_nanos() as u64);
            enqueue(&sender, Command::Span(data));
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        self.finish();
    }
}

struct Exporter {
    config: Config,
    resource: Attributes,
    metrics: Metrics,
    metrics_dirty: bool,
    spans: Vec<SpanData>,
    client: Option<reqwest::blocking::Client>,
    cancelled: Arc<AtomicBool>,
}

fn run_exporter(config: Config, receiver: mpsc::Receiver<Command>, cancelled: Arc<AtomicBool>) {
    let mut exporter = Exporter {
        resource: resource_attributes(&config),
        config,
        metrics: Metrics::new(now_unix_nano()),
        metrics_dirty: false,
        spans: Vec::new(),
        client: None,
        cancelled,
    };
    let mut last_export = Instant::now();
    loop {
        if exporter.cancelled.load(Ordering::SeqCst) {
            return;
        }
        let until_export = EXPORT_INTERVAL.saturating_sub(last_export.elapsed());
        if until_export.is_zero() {
            exporter.export();
            last_export = Instant::now();
            continue;
        }
        // Short waits so a revoke is seen even while live `Span`s keep a
        // sender clone alive and the channel never disconnects.
        match receiver.recv_timeout(until_export.min(CANCEL_POLL)) {
            Ok(Command::Metric(name, value, tags)) => {
                exporter.metrics.add(name, value, tags);
                exporter.metrics_dirty = true;
            }
            Ok(Command::Span(span)) => {
                if exporter.spans.len() < MAX_BUFFERED_SPANS {
                    exporter.spans.push(span);
                } else {
                    count_dropped(1);
                }
            }
            Ok(Command::Flush(reply)) => {
                let _ = reply.send(exporter.export());
                last_export = Instant::now();
            }
            Ok(Command::Reconfigure(config)) => {
                if config.session_id != exporter.config.session_id {
                    exporter.retire_session();
                }
                // Secret rotation, endpoint or session change: the next POST
                // signs with the new values. The HTTP client is config-free.
                exporter.resource = resource_attributes(&config);
                exporter.config = *config;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

impl Exporter {
    /// Before a session change: send what was recorded under the old session
    /// with the old config, then start fresh, so no old-session data goes
    /// out labelled with the new `session.id`. Whatever fails to send (for
    /// example because the old secret was rotated away) is dropped and counted.
    fn retire_session(&mut self) {
        self.export();
        let mut lost = self.spans.len() as u64;
        if self.metrics_dirty {
            lost += self
                .metrics
                .series
                .values()
                .map(|s| s.len() as u64)
                .sum::<u64>();
        }
        count_dropped(lost);
        self.spans.clear();
        self.metrics = Metrics::new(now_unix_nano());
        self.metrics_dirty = false;
    }

    fn export(&mut self) -> Export {
        if self.cancelled.load(Ordering::SeqCst) {
            return Export::Disabled;
        }
        if self.spans.is_empty() && !self.metrics_dirty {
            return Export::Empty;
        }
        let version = self.config.client_version.clone();
        let mut failed = false;
        if !self.spans.is_empty() {
            let body = encode_traces(&self.resource, &version, &self.spans);
            match self.post("traces", &body) {
                Ok(()) => self.spans.clear(),
                Err(error) => {
                    failed = true;
                    crate::logging::warning(format!("[Telemetry] Trace export failed: {error:#}"));
                }
            }
        }
        if self.metrics_dirty && !self.cancelled.load(Ordering::SeqCst) {
            let body = encode_metrics(&self.resource, &version, &self.metrics, now_unix_nano());
            match self.post("metrics", &body) {
                Ok(()) => self.metrics_dirty = false,
                Err(error) => {
                    failed = true;
                    crate::logging::warning(format!("[Telemetry] Metric export failed: {error:#}"));
                }
            }
        }
        if failed { Export::Failed } else { Export::Sent }
    }

    /// POST one OTLP/JSON body, signed over the path actually requested.
    fn post(&mut self, signal: &str, body: &Value) -> Result<()> {
        let url = otlp_url(&self.config.endpoint, signal).context("Invalid telemetry endpoint")?;
        let path = reqwest::Url::parse(&url)
            .context("Parse telemetry URL")?
            .path()
            .to_owned();
        let secret = self
            .config
            .signing_secret
            .as_deref()
            .filter(|secret| !secret.is_empty())
            .context("Telemetry signing secret is missing")?;
        if self.client.is_none() {
            self.client = Some(
                reqwest::blocking::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(REQUEST_TIMEOUT)
                    .connect_timeout(CONNECT_TIMEOUT)
                    .build()
                    .context("Build telemetry HTTP client")?,
            );
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("System clock predates Unix epoch")?
            .as_secs() as i64;
        anyhow::ensure!(
            !self.cancelled.load(Ordering::SeqCst),
            "Telemetry was turned off"
        );
        let response = self
            .client
            .as_ref()
            .context("Telemetry HTTP client missing")?
            .post(&url)
            .header(
                "X-KMS-Signature",
                signature(secret, &self.config.session_id, &path, timestamp),
            )
            .header("X-KMS-Timestamp", timestamp.to_string())
            .header("X-KMS-Session-ID", &self.config.session_id)
            .header("X-KMS-Client-Version", &self.config.client_version)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(body).context("Serialize OTLP payload")?)
            .send()
            .with_context(|| format!("Send telemetry to {url}"))?;
        anyhow::ensure!(
            response.status().is_success(),
            "Telemetry endpoint returned {}",
            response.status()
        );
        Ok(())
    }
}
