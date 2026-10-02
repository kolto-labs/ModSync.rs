//! One lazily captured immutable local-zone context per profile decode/save.
#[cfg(any(unix, windows))]
#[path = "local/calendar.rs"]
mod calendar;
#[cfg(unix)]
#[path = "local/unix.rs"]
mod unix;
#[cfg(windows)]
#[path = "local/windows.rs"]
mod windows;
use anyhow::Result;
use chrono::NaiveDateTime;
#[cfg(unix)]
use unix::Snapshot;
#[cfg(windows)]
use windows::Snapshot;
#[cfg(not(any(unix, windows)))]
struct Snapshot;
#[cfg(not(any(unix, windows)))]
impl Snapshot {
    fn capture() -> Result<Self> {
        anyhow::bail!("Native local timezone profile is not implemented on this platform")
    }
    fn normalize_fallback(&self, _wall: NaiveDateTime, _offset: i32) -> Result<String> {
        anyhow::bail!("Native local timezone profile is not implemented on this platform")
    }
    fn normalize(&self, _wall: NaiveDateTime, _offset: Option<i32>) -> Result<String> {
        anyhow::bail!("Native local timezone profile is not implemented on this platform")
    }
}
#[derive(Debug)]
pub(super) struct InvalidFallbackDate;
impl std::fmt::Display for InvalidFallbackDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Fallback local date exceeds DateTime range")
    }
}
impl std::error::Error for InvalidFallbackDate {}
pub(in super::super) struct ContextSnapshot {
    snapshot: Option<std::result::Result<Snapshot, String>>,
    pub warnings: Vec<String>,
}
impl ContextSnapshot {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            warnings: Vec::new(),
        }
    }
    pub fn normalize(&mut self, wall: NaiveDateTime, offset: Option<i32>) -> Result<String> {
        let snapshot = self
            .snapshot
            .get_or_insert_with(|| Snapshot::capture().map_err(|error| format!("{error:#}")));
        let snapshot = snapshot
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        snapshot.normalize(wall, offset)
    }
    pub fn normalize_fallback(&mut self, wall: NaiveDateTime, offset: i32) -> Result<String> {
        let snapshot = self
            .snapshot
            .get_or_insert_with(|| Snapshot::capture().map_err(|error| format!("{error:#}")));
        snapshot
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error}"))?
            .normalize_fallback(wall, offset)
    }
    pub fn warn(&mut self, key: &str, error: &anyhow::Error) {
        self.warnings.push(format!(
            "Profile {key} retains its authored local date: {}",
            format!("{error:#}").chars().take(512).collect::<String>()
        ));
    }
}
