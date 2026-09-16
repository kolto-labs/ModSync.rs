use crate::paths::relative;
use anyhow::{Result, bail, ensure};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
    path::PathBuf,
};

pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

/// Bound expansion and reject traversal before publishing any archive entries to the VFS.
pub fn unpack(bytes: &[u8]) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    let mut add = |name: &str, reader: &mut dyn Read| -> Result<()> {
        ensure!(files.len() < 100_000, "Archive has too many files");
        let path = relative(name, false)?;
        ensure!(!path.as_os_str().is_empty(), "Empty archive entry");
        ensure!(
            names.insert(path.to_string_lossy().to_ascii_lowercase()),
            "Duplicate archive entry: {name}"
        );
        let mut data = Vec::new();
        reader
            .take(MAX_ARCHIVE_BYTES - total + 1)
            .read_to_end(&mut data)?;
        total += data.len() as u64;
        ensure!(
            total <= MAX_ARCHIVE_BYTES,
            "Archive expands beyond the 512 MiB safety limit"
        );
        files.push((path, data));
        Ok(())
    };
    if bytes.starts_with(b"Rar!\x1a\x07") {
        use std::io::Write;
        let mut temp = tempfile::NamedTempFile::new()?;
        temp.write_all(bytes)?;
        temp.flush()?;
        let mut archive = unrar::Archive::new(temp.path()).open_for_processing()?;
        let mut declared = 0u64;
        while let Some(entry) = archive.read_header()? {
            let header = entry.entry();
            let name = header
                .filename
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Non-UTF8 RAR entry"))?
                .to_owned();
            relative(&name, false)?;
            ensure!(
                !header.is_split() && !header.is_encrypted(),
                "Multipart/encrypted RAR is unsupported"
            );
            let kind = header.file_attr & 0o170000;
            ensure!(
                kind == 0 || kind == 0o100000 || kind == 0o040000,
                "RAR links and special files are forbidden"
            );
            if header.is_directory() {
                archive = entry.skip()?;
            } else {
                declared = declared
                    .checked_add(header.unpacked_size)
                    .ok_or_else(|| anyhow::anyhow!("RAR size overflow"))?;
                ensure!(
                    declared <= MAX_ARCHIVE_BYTES,
                    "RAR expands beyond the 512 MiB safety limit"
                );
                let (data, next) = entry.read()?;
                add(&name, &mut Cursor::new(data))?;
                archive = next;
            }
        }
    } else if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        ensure!(zip.len() <= 100_000, "Archive has too many entries");
        for n in 0..zip.len() {
            let mut entry = zip.by_index(n)?;
            relative(entry.name(), false)?;
            let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
            ensure!(
                kind == 0 || kind == 0o100000 || kind == 0o040000,
                "Archive links and special files are forbidden"
            );
            if !entry.is_dir() {
                let name = entry.name().to_owned();
                add(&name, &mut entry)?;
            }
        }
    } else if bytes.starts_with(b"7z\xBC\xAF\x27\x1C") {
        sevenz_rust2::decompress_with_extract_fn(Cursor::new(bytes), ".", |entry, reader, _| {
            let result = (|| -> Result<()> {
                relative(entry.name(), false)?;
                if !entry.is_directory() {
                    add(entry.name(), reader)?;
                }
                Ok(())
            })();
            result
                .map(|_| true)
                .map_err(|e| std::io::Error::other(e.to_string()).into())
        })?;
    } else {
        bail!(
            "Unsupported archive signature. ZIP, 7z, and RAR are supported. SFX executables are never executed as archives."
        );
    }
    Ok(files)
}
