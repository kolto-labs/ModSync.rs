//! Bounded guide ingestion using the reference loader's primary UTF-8 policy.
use anyhow::{Result, ensure};
use std::{io::Read, path::Path};

pub(super) const GUIDE_LIMIT: u64 = 8 * 1024 * 1024;

pub(super) fn read(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(path)?
        .take(GUIDE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= GUIDE_LIMIT, "Guide exceeds 8 MiB");
    // FileLoadingService.ReadFileWithEncodingFallback uses nonthrowing UTF-8
    // first, then replaces U+FFFD, including authored replacement characters.
    // Keep the BOM for the existing parser's trim_start_matches boundary.
    Ok(String::from_utf8_lossy(&bytes).replace('\u{fffd}', "_"))
}
