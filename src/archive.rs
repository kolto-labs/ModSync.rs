use crate::paths::relative;
use anyhow::{Result, bail, ensure};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub scratch_dir: Option<PathBuf>,
    pub max_expanded_bytes: Option<u64>,
}

pub struct Entry {
    pub path: PathBuf,
    pub file: Arc<tempfile::NamedTempFile>,
    pub mode: Option<u32>,
}

struct Output<'a> {
    options: &'a Options,
    cancelled: &'a AtomicBool,
    spool: bool,
    limit: u64,
    total: u64,
    names: BTreeSet<String>,
    files: Vec<Entry>,
}
impl Output<'_> {
    fn name(&mut self, name: &str, mode: Option<u32>, directory: bool) -> Result<PathBuf> {
        ensure!(
            !self.cancelled.load(Ordering::Relaxed),
            "Archive operation cancelled"
        );
        ensure!(self.names.len() < 100_000, "Archive has too many entries");
        let path = relative(name, false)?;
        ensure!(
            !name.is_empty() && (directory || !path.as_os_str().is_empty()),
            "Empty archive entry"
        );
        let kind = mode.unwrap_or(0) & 0o170000;
        ensure!(
            kind == 0 || kind == if directory { 0o040000 } else { 0o100000 },
            "Archive links and special files are forbidden"
        );
        ensure!(
            self.names
                .insert(path.to_string_lossy().replace('\\', "/").to_lowercase()),
            "Duplicate archive entry: {name}"
        );
        Ok(path)
    }
    fn temp(&self) -> Result<tempfile::NamedTempFile> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("modsync-entry-");
        Ok(match &self.options.scratch_dir {
            Some(dir) => builder.tempfile_in(dir)?,
            None => builder.tempfile()?,
        })
    }
    fn add(&mut self, name: &str, mode: Option<u32>, reader: &mut dyn Read) -> Result<()> {
        let path = self.name(name, mode, false)?;
        let mut temp = if self.spool { Some(self.temp()?) } else { None };
        let mut buffer = [0u8; 64 * 1024];
        loop {
            ensure!(
                !self.cancelled.load(Ordering::Relaxed),
                "Archive operation cancelled"
            );
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            self.total = self
                .total
                .checked_add(n as u64)
                .ok_or_else(|| anyhow::anyhow!("Archive size overflow"))?;
            ensure!(
                self.total <= self.limit,
                "Archive exceeds expanded-byte budget or available scratch space"
            );
            if let Some(file) = &mut temp {
                file.write_all(&buffer[..n])?;
            }
        }
        if let Some(file) = temp {
            self.files.push(Entry {
                path,
                file: Arc::new(file),
                mode,
            });
        }
        Ok(())
    }
}

pub fn unpack(
    path: &Path,
    name: &Path,
    options: &Options,
    cancelled: &AtomicBool,
) -> Result<Vec<Entry>> {
    process(path, name, options, cancelled, true)
}
pub fn verify(path: &Path, name: &Path, options: &Options, cancelled: &AtomicBool) -> Result<()> {
    process(path, name, options, cancelled, false).map(|_| ())
}
fn process(
    path: &Path,
    name: &Path,
    options: &Options,
    cancelled: &AtomicBool,
    spool: bool,
) -> Result<Vec<Entry>> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Archive operation cancelled"
    );
    let available = if spool {
        fs2::available_space(
            options
                .scratch_dir
                .clone()
                .unwrap_or_else(std::env::temp_dir),
        )?
    } else {
        u64::MAX
    };
    let mut output = Output {
        options,
        cancelled,
        spool,
        limit: options
            .max_expanded_bytes
            .unwrap_or(u64::MAX)
            .min(available),
        total: 0,
        names: BTreeSet::new(),
        files: Vec::new(),
    };
    let mut file = File::open(path)?;
    let mut signature = [0u8; 8];
    let n = file.read(&mut signature)?;
    let signature = &signature[..n];
    file.rewind()?;
    if signature.starts_with(b"Rar!\x1a\x07") {
        let mut archive = unrar::Archive::new(path).open_for_processing()?;
        while let Some(entry) = archive.read_header()? {
            let header = entry.entry();
            let name = header
                .filename
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Non-UTF8 RAR entry"))?
                .to_owned();
            let mode = (header.file_attr & 0o170000 != 0).then_some(header.file_attr);
            let path = output.name(&name, mode, header.is_directory())?;
            ensure!(
                !header.is_split() && !header.is_encrypted(),
                "Multipart/encrypted RAR is unsupported"
            );
            if header.is_directory() {
                archive = entry.skip()?;
                continue;
            }
            let size = header.unpacked_size;
            output.total = output
                .total
                .checked_add(size)
                .ok_or_else(|| anyhow::anyhow!("RAR size overflow"))?;
            ensure!(
                output.total <= output.limit,
                "Archive exceeds expanded-byte budget or available scratch space"
            );
            if spool {
                // Native UnRAR opens output exclusively on Windows. Keep path ownership,
                // but release our handle before asking the decoder to create the file.
                let temp_path = output.temp()?.into_temp_path();
                archive = entry.extract_to(&temp_path)?;
                let meta = std::fs::symlink_metadata(&temp_path)?;
                ensure!(
                    meta.is_file() && meta.len() == size,
                    "Invalid extracted RAR entry"
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    ensure!(meta.nlink() == 1, "RAR hard links are forbidden");
                }
                let temp = tempfile::NamedTempFile::from_parts(File::open(&temp_path)?, temp_path);
                output.files.push(Entry {
                    path,
                    file: Arc::new(temp),
                    mode,
                });
            } else {
                archive = entry.test()?;
            }
        }
    } else if signature.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(file)?;
        ensure!(zip.len() <= 100_000, "Archive has too many entries");
        for n in 0..zip.len() {
            let mut entry = zip.by_index(n)?;
            let name = entry.name().to_owned();
            let mode = entry.unix_mode();
            if entry.is_dir() {
                output.name(&name, mode, true)?;
            } else {
                output.add(&name, mode, &mut entry)?;
            }
        }
    } else if signature.starts_with(b"7z\xBC\xAF\x27\x1C") || signature.starts_with(b"MZ") {
        let base = if signature.starts_with(b"MZ") {
            find_7z(&mut file, cancelled)?
        } else {
            0
        };
        file.seek(SeekFrom::Start(base))?;
        sevenz_rust2::decompress_with_extract_fn(
            Section { file, base },
            ".",
            |entry, reader, _| {
                let result = (|| -> Result<()> {
                    let mode =
                        if entry.has_windows_attributes && entry.windows_attributes >> 16 != 0 {
                            Some(entry.windows_attributes >> 16)
                        } else {
                            None
                        };
                    if entry.is_directory() {
                        output.name(entry.name(), mode, true)?;
                    } else {
                        output.add(entry.name(), mode, reader)?;
                    }
                    Ok(())
                })();
                result
                    .map(|_| true)
                    .map_err(|e| std::io::Error::other(e.to_string()).into())
            },
        )?;
    } else if signature.starts_with(b"\x1f\x8b") {
        let gzip = flate2::read::MultiGzDecoder::new(file);
        let mut reader = BufReader::new(gzip);
        let header = reader.fill_buf()?;
        if name.to_string_lossy().to_lowercase().ends_with(".tar.gz")
            || name
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("tgz"))
            || header.get(257..262) == Some(b"ustar")
        {
            read_tar(&mut reader, &mut output)?;
            // Consume the gzip trailer, including its CRC.
            std::io::copy(&mut reader, &mut std::io::sink())?;
        } else {
            let name = name
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("GZip needs a filename"))?;
            output.add(name, None, &mut reader)?;
        }
    } else if name
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("tar"))
    {
        read_tar(file, &mut output)?;
    } else {
        bail!(
            "Unsupported archive signature. ZIP, 7z, RAR, TAR and GZip are supported. Only embedded 7z SFX payloads are decoded; executables are never run as archives."
        );
    }
    Ok(output.files)
}
fn read_tar(reader: impl Read, output: &mut Output<'_>) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry
            .path()?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Non-UTF8 TAR entry"))?
            .to_owned();
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "TAR links and special files are forbidden"
        );
        let mode = Some(entry.header().mode()?);
        if kind.is_dir() {
            output.name(&name, mode, true)?;
        } else {
            output.add(&name, mode, &mut entry)?;
        }
    }
    Ok(())
}

// Rebase seeks so an embedded 7z payload is decoded without executing its SFX stub.
struct Section {
    file: File,
    base: u64,
}
impl Read for Section {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buffer)
    }
}
impl Seek for Section {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let from = match from {
            SeekFrom::Start(n) => SeekFrom::Start(
                self.base
                    .checked_add(n)
                    .ok_or_else(|| std::io::Error::other("Archive seek overflow"))?,
            ),
            other => other,
        };
        self.file
            .seek(from)?
            .checked_sub(self.base)
            .ok_or_else(|| std::io::Error::other("Seek before archive payload"))
    }
}
fn find_7z(file: &mut File, cancelled: &AtomicBool) -> Result<u64> {
    const SIGNATURE: &[u8] = b"7z\xBC\xAF\x27\x1C";
    let mut matched = 0;
    let mut position = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Archive operation cancelled"
        );
        let n = file.read(&mut buffer)?;
        ensure!(
            n > 0,
            "Executable has no supported 7z payload; it will not be executed"
        );
        for byte in &buffer[..n] {
            position += 1;
            if *byte == SIGNATURE[matched] {
                matched += 1;
            } else {
                matched = usize::from(*byte == SIGNATURE[0]);
            }
            if matched == SIGNATURE.len() {
                return Ok(position - SIGNATURE.len() as u64);
            }
        }
    }
}
