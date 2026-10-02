//! One lazily captured immutable local-zone context per profile decode/save.
#[cfg(unix)]
#[path = "local/unix.rs"]
mod unix;
use anyhow::Result;
use chrono::NaiveDateTime;
#[cfg(unix)]
use unix::Snapshot;
#[cfg(not(unix))]
struct Snapshot;
#[cfg(not(unix))]
impl Snapshot {
    fn capture() -> Result<Self> {
        anyhow::bail!("Native local timezone profile is not implemented on this platform")
    }
    fn normalize(&self, _wall: NaiveDateTime, _offset: Option<i32>) -> Result<String> {
        anyhow::bail!("Native local timezone profile is not implemented on this platform")
    }
}
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
    pub fn warn(&mut self, key: &str, error: &anyhow::Error) {
        self.warnings.push(format!(
            "Profile {key} retains its authored local date: {}",
            format!("{error:#}").chars().take(512).collect::<String>()
        ));
    }
}
