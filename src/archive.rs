use crate::paths::literal_relative;
mod timestamps;
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
    time::SystemTime,
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
    /// Authored modification time, when the decoder exposes one.
    pub modified: Option<SystemTime>,
}

struct Output<'a> {
    options: &'a Options,
    cancelled: &'a AtomicBool,
    spool: bool,
    limit: u64,
    total: u64,
    names: BTreeSet<String>,
    original_entries: Option<Vec<OriginalEntry>>,
    original_name_bytes: usize,
    files: Vec<Entry>,
}
impl Output<'_> {
    fn name(&mut self, name: &str, mode: Option<u32>, directory: bool) -> Result<PathBuf> {
        ensure!(
            !self.cancelled.load(Ordering::Relaxed),
            "Archive operation cancelled"
        );
        ensure!(self.names.len() < 100_000, "Archive has too many entries");
        let path = literal_relative(name)?;
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
        if let Some(entries) = &mut self.original_entries {
            // Only callers requesting authored names incur this inventory's
            // allocation and additional budget. Existing list/unpack/verify
            // callers keep their established admission rules and storage.
            self.original_name_bytes = self
                .original_name_bytes
                .checked_add(path.as_os_str().len())
                .ok_or_else(|| anyhow::anyhow!("Archive name inventory size overflow"))?;
            ensure!(
                self.original_name_bytes <= 16 * 1024 * 1024,
                "Archive original-name inventory exceeds 16 MiB budget"
            );
            entries.push(OriginalEntry {
                path: path.to_string_lossy().replace('\\', "/"),
                is_directory: directory,
            });
        }
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
    fn add(
        &mut self,
        name: &str,
        mode: Option<u32>,
        modified: Option<SystemTime>,
        reader: &mut dyn Read,
    ) -> Result<()> {
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
                modified,
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
    process(path, name, options, cancelled, true, false).map(|result| result.files)
}
pub fn verify(path: &Path, name: &Path, options: &Options, cancelled: &AtomicBool) -> Result<()> {
    process(path, name, options, cancelled, false, false).map(|_| ())
}
/// Verify like [`verify`] and return every entry name (lowercase, `/`-separated),
/// without spooling any entry to scratch storage.
pub fn list(
    path: &Path,
    name: &Path,
    options: &Options,
    cancelled: &AtomicBool,
) -> Result<BTreeSet<String>> {
    process(path, name, options, cancelled, false, false).map(|result| result.names)
}

/// Verified entry names with authored case retained and `/` separators. Like
/// [`list`], this never spools extracted files or invokes external programs.
/// Existing case-collision and path/link admission rules remain identical.
/// The authored name inventory has an additional 16 MiB aggregate byte bound.
pub fn list_original(
    path: &Path,
    name: &Path,
    options: &Options,
    cancelled: &AtomicBool,
) -> Result<BTreeSet<String>> {
    list_original_with_size(path, name, options, cancelled)
        .map(|(entries, _)| entries.into_iter().map(|entry| entry.path).collect())
}

/// Retain explicit directory/file kind for reference archive-path matching.
#[derive(Clone)]
pub(crate) struct OriginalEntry {
    pub(crate) path: String,
    pub(crate) is_directory: bool,
}

pub(crate) fn list_original_with_size(
    path: &Path,
    name: &Path,
    options: &Options,
    cancelled: &AtomicBool,
) -> Result<(Vec<OriginalEntry>, u64)> {
    process(path, name, options, cancelled, false, true)
        .map(|result| (result.original_entries, result.expanded_bytes))
}

struct ProcessResult {
    files: Vec<Entry>,
    names: BTreeSet<String>,
    original_entries: Vec<OriginalEntry>,
    expanded_bytes: u64,
}
const RAR_HOST_UNIX: u32 = 3;

struct RarHeaderInfo {
    host_os: u32,
    directory: bool,
    unpacked_size: u64,
}

/// RAR `file_attr` is a Unix `st_mode` only when the archiving host was Unix.
/// Archives written on Windows/DOS/OS2 carry a Windows attribute bitmask whose
/// bits (for example 0x2000 not-content-indexed) alias Unix file-type bits.
fn rar_unix_mode(host_os: u32, file_attr: u32) -> Option<u32> {
    (host_os == RAR_HOST_UNIX && file_attr & 0o170000 != 0).then_some(file_attr)
}

const SEVENZ_UNIX_EXTENSION: u32 = 0x8000;

/// A 7z attribute word carries a Unix `st_mode` in its high 16 bits only when
/// the Unix-extension flag (0x8000 in the low word) is set. Archives written on
/// Windows leave those bits unspecified, so they must never be read as a mode.
fn sevenz_unix_mode(has_attributes: bool, attributes: u32) -> Option<u32> {
    if !has_attributes || attributes & SEVENZ_UNIX_EXTENSION == 0 {
        return None;
    }
    let mode = attributes >> 16;
    (mode & 0o170000 != 0).then_some(mode)
}

/// Permission bits applied to an extracted file: whatever the archive asked for,
/// but never without owner read/write, so later install steps can read, move and
/// replace the file.
pub(crate) fn extracted_file_mode(mode: u32) -> u32 {
    (mode & 0o777) | 0o600
}

/// Unknown or mismatched header metadata is treated as a Unix host, which
/// keeps the strict link/special-file rejection.
fn rar_host_os(info: Option<&RarHeaderInfo>, directory: bool, unpacked_size: u64) -> u32 {
    match info {
        Some(info) if info.directory == directory && info.unpacked_size == unpacked_size => {
            info.host_os
        }
        _ => RAR_HOST_UNIX,
    }
}

#[cfg(any(target_os = "linux", target_os = "netbsd"))]
fn rar_native_name(path: &Path) -> Option<std::ffi::CString> {
    std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()
}

#[cfg(windows)]
fn rar_native_name(path: &Path) -> Option<Vec<unrar_sys::WCHAR>> {
    use std::os::windows::ffi::OsStrExt;
    let mut name: Vec<unrar_sys::WCHAR> = path.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return None;
    }
    name.push(0);
    Some(name)
}

#[cfg(not(any(target_os = "linux", target_os = "netbsd", windows)))]
fn rar_native_name(path: &Path) -> Option<Vec<unrar_sys::WCHAR>> {
    let mut name: Vec<unrar_sys::WCHAR> = path.to_str()?.chars().map(|c| c as _).collect();
    if name.contains(&0) {
        return None;
    }
    name.push(0);
    Some(name)
}

struct RarHandle(*const unrar_sys::Handle);
impl Drop for RarHandle {
    fn drop(&mut self) {
        unsafe { unrar_sys::RARCloseArchive(self.0) };
    }
}

/// The `unrar` crate does not expose the header host OS, so list the headers
/// once through the native bindings. Any failure yields a short inventory and
/// the caller falls back to strict Unix semantics for the missing entries.
fn rar_header_inventory(path: &Path) -> Vec<RarHeaderInfo> {
    let mut headers = Vec::new();
    let Some(name) = rar_native_name(path) else {
        return headers;
    };
    let mut open = unrar_sys::OpenArchiveDataEx::new(name.as_ptr(), unrar_sys::RAR_OM_LIST);
    let handle = unsafe { unrar_sys::RAROpenArchiveEx(std::ptr::addr_of_mut!(open)) };
    if handle.is_null() {
        return headers;
    }
    let handle = RarHandle(handle);
    if open.open_result != 0 {
        return headers;
    }
    let mut header = Box::new(unrar_sys::HeaderDataEx::default());
    while unsafe { unrar_sys::RARReadHeaderEx(handle.0, std::ptr::addr_of_mut!(*header)) }
        == unrar_sys::ERAR_SUCCESS
    {
        headers.push(RarHeaderInfo {
            host_os: header.host_os,
            directory: header.flags & unrar_sys::RHDF_DIRECTORY != 0,
            unpacked_size: (u64::from(header.unp_size_high) << 32) | u64::from(header.unp_size),
        });
        let skipped = unsafe {
            unrar_sys::RARProcessFile(
                handle.0,
                unrar_sys::RAR_SKIP,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if skipped != unrar_sys::ERAR_SUCCESS {
            break;
        }
    }
    headers
}

fn process(
    path: &Path,
    name: &Path,
    options: &Options,
    cancelled: &AtomicBool,
    spool: bool,
    original_inventory: bool,
) -> Result<ProcessResult> {
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
        original_entries: original_inventory.then(Vec::new),
        original_name_bytes: 0,
        files: Vec::new(),
    };
    let mut file = crate::paths::open_regular_read(path)?;
    let mut signature = [0u8; 8];
    let n = file.read(&mut signature)?;
    let signature = &signature[..n];
    file.rewind()?;
    if signature.starts_with(b"Rar!\x1a\x07") {
        let headers = rar_header_inventory(path);
        let mut archive = unrar::Archive::new(path).open_for_processing()?;
        let mut index = 0;
        while let Some(entry) = archive.read_header()? {
            let header = entry.entry();
            let name = header
                .filename
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Non-UTF8 RAR entry"))?
                .to_owned();
            let host_os = rar_host_os(
                headers.get(index),
                header.is_directory(),
                header.unpacked_size,
            );
            index += 1;
            let mode = rar_unix_mode(host_os, header.file_attr);
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
            let has_modified = header.file_time != 0;
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
                let modified = if has_modified {
                    Some(meta.modified()?)
                } else {
                    None
                };
                output.files.push(Entry {
                    path,
                    file: Arc::new(temp),
                    mode,
                    modified,
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
                let modified = timestamps::zip(&entry)?;
                output.add(&name, mode, modified, &mut entry)?;
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
                        sevenz_unix_mode(entry.has_windows_attributes, entry.windows_attributes);
                    if entry.is_directory() {
                        output.name(entry.name(), mode, true)?;
                    } else {
                        let modified = entry
                            .has_last_modified_date
                            .then(|| timestamps::windows(u64::from(entry.last_modified_date)))
                            .transpose()?;
                        output.add(entry.name(), mode, modified, reader)?;
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
        let modified = gzip
            .header()
            .map(|header| header.mtime())
            .filter(|seconds| *seconds != 0)
            .map(|seconds| timestamps::unix(u64::from(seconds)))
            .transpose()?;
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
            output.add(name, None, modified, &mut reader)?;
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
    Ok(ProcessResult {
        files: output.files,
        names: output.names,
        original_entries: output.original_entries.unwrap_or_default(),
        expanded_bytes: output.total,
    })
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
            let modified = Some(timestamps::unix(entry.header().mtime()?)?);
            output.add(&name, mode, modified, &mut entry)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const RAR_HOST_MSDOS: u32 = 0;
    const RAR_HOST_OS2: u32 = 1;
    const RAR_HOST_WINDOWS: u32 = 2;
    const RAR_HOST_BEOS: u32 = 5;

    #[test]
    fn windows_attributes_are_never_unix_modes() {
        for host in [RAR_HOST_MSDOS, RAR_HOST_OS2, RAR_HOST_WINDOWS] {
            for attr in [0x20, 0x2000, 0x4000, 0x8000, 0x2020, 0xa000, 0x6000] {
                assert_eq!(
                    rar_unix_mode(host, attr),
                    None,
                    "host {host} attr {attr:#x}"
                );
            }
        }
    }

    #[test]
    fn unix_host_keeps_mode_and_special_kinds_are_rejected() {
        let mut output = Output {
            options: &Options::default(),
            cancelled: &AtomicBool::new(false),
            spool: false,
            limit: u64::MAX,
            total: 0,
            names: BTreeSet::new(),
            original_entries: None,
            original_name_bytes: 0,
            files: Vec::new(),
        };
        let regular = rar_unix_mode(RAR_HOST_UNIX, 0o100644);
        let symlink = rar_unix_mode(RAR_HOST_UNIX, 0o120777);
        let character = rar_unix_mode(RAR_HOST_UNIX, 0o020000);
        assert_eq!(regular, Some(0o100644));
        assert_eq!(symlink, Some(0o120777));
        assert_eq!(character, Some(0o020000));
        assert_eq!(rar_unix_mode(RAR_HOST_UNIX, 0x20), None);
        assert!(output.name("plain.bin", regular, false).is_ok());
        assert!(output.name("link", symlink, false).is_err());
        assert!(output.name("device", character, false).is_err());
    }

    #[test]
    fn windows_attribute_aliasing_a_special_kind_is_accepted() {
        let mut output = Output {
            options: &Options::default(),
            cancelled: &AtomicBool::new(false),
            spool: false,
            limit: u64::MAX,
            total: 0,
            names: BTreeSet::new(),
            original_entries: None,
            original_name_bytes: 0,
            files: Vec::new(),
        };
        let mode = rar_unix_mode(RAR_HOST_WINDOWS, 0x2000);
        assert!(output.name("example.tga", mode, false).is_ok());
    }

    #[test]
    fn beos_and_unknown_hosts_do_not_expose_windows_bits() {
        assert_eq!(rar_unix_mode(RAR_HOST_BEOS, 0x2000), None);
        assert_eq!(rar_unix_mode(99, 0x2000), None);
    }

    const SEVENZ_ARCHIVE_ATTRIBUTE: u32 = 0x20;

    fn sevenz_with_attributes(name: &str, attributes: Option<u32>) -> Vec<u8> {
        let mut writer =
            sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).unwrap();
        let mut entry = sevenz_rust2::ArchiveEntry::new_file(name);
        if let Some(attributes) = attributes {
            entry.has_windows_attributes = true;
            entry.windows_attributes = attributes;
        }
        writer
            .push_archive_entry(entry, Some(&b"sample"[..]))
            .unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn unpack_sevenz(bytes: &[u8]) -> Vec<Entry> {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(bytes).unwrap();
        unpack(
            file.path(),
            Path::new("sample.7z"),
            &Options::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    #[test]
    fn sevenz_mode_needs_the_unix_extension_flag() {
        let unix = (0o100644u32 << 16) | SEVENZ_UNIX_EXTENSION | SEVENZ_ARCHIVE_ATTRIBUTE;
        assert_eq!(sevenz_unix_mode(true, unix), Some(0o100644));
        assert_eq!(sevenz_unix_mode(false, unix), None);
        let without_flag = unix & !SEVENZ_UNIX_EXTENSION;
        assert_eq!(sevenz_unix_mode(true, without_flag), None);
        assert_eq!(sevenz_unix_mode(true, 0xffff_0020), None);
        assert_eq!(sevenz_unix_mode(true, 0x8000_0020), None);
    }

    #[test]
    fn sevenz_mode_needs_a_file_type_bit_even_with_the_flag() {
        assert_eq!(sevenz_unix_mode(true, SEVENZ_UNIX_EXTENSION), None);
        assert_eq!(
            sevenz_unix_mode(true, (0o644u32 << 16) | SEVENZ_UNIX_EXTENSION),
            None
        );
        assert_eq!(
            sevenz_unix_mode(true, (0o040755u32 << 16) | SEVENZ_UNIX_EXTENSION),
            Some(0o040755)
        );
    }

    #[test]
    fn sevenz_unix_links_are_still_rejected() {
        let mut output = Output {
            options: &Options::default(),
            cancelled: &AtomicBool::new(false),
            spool: false,
            limit: u64::MAX,
            total: 0,
            names: BTreeSet::new(),
            original_entries: None,
            original_name_bytes: 0,
            files: Vec::new(),
        };
        let symlink = sevenz_unix_mode(true, (0o120777u32 << 16) | SEVENZ_UNIX_EXTENSION);
        assert_eq!(symlink, Some(0o120777));
        assert!(output.name("link", symlink, false).is_err());
    }

    #[test]
    fn extracted_file_mode_keeps_owner_read_and_write() {
        assert_eq!(extracted_file_mode(0), 0o600);
        assert_eq!(extracted_file_mode(0o100000), 0o600);
        assert_eq!(extracted_file_mode(0o100444), 0o644);
        assert_eq!(extracted_file_mode(0o100755), 0o755);
        assert_eq!(extracted_file_mode(0o104755), 0o755);
    }

    #[test]
    fn sevenz_entry_with_garbage_attribute_high_bits_has_no_mode() {
        for attributes in [0x8000_0020, 0xffff_0020, 0x0001_0020] {
            let entries = unpack_sevenz(&sevenz_with_attributes("sample.bin", Some(attributes)));
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].mode, None, "attributes {attributes:#x}");
        }
        let plain = unpack_sevenz(&sevenz_with_attributes("sample.bin", None));
        assert_eq!(plain[0].mode, None);
    }

    #[test]
    fn sevenz_entry_with_unix_extension_keeps_its_mode() {
        let attributes = (0o100755u32 << 16) | SEVENZ_UNIX_EXTENSION | SEVENZ_ARCHIVE_ATTRIBUTE;
        let entries = unpack_sevenz(&sevenz_with_attributes("sample.bin", Some(attributes)));
        assert_eq!(entries[0].mode, Some(0o100755));
    }

    #[test]
    fn missing_or_mismatched_header_info_is_strict_unix() {
        let info = RarHeaderInfo {
            host_os: RAR_HOST_WINDOWS,
            directory: false,
            unpacked_size: 7,
        };
        assert_eq!(rar_host_os(Some(&info), false, 7), RAR_HOST_WINDOWS);
        assert_eq!(rar_host_os(Some(&info), true, 7), RAR_HOST_UNIX);
        assert_eq!(rar_host_os(Some(&info), false, 8), RAR_HOST_UNIX);
        assert_eq!(rar_host_os(None, false, 7), RAR_HOST_UNIX);
    }
}
