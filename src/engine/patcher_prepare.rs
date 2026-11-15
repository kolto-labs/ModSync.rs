//! Install-only preparation matching the reference's three INI replacements.
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const LIMIT: usize = 8 * 1024 * 1024;

/// Reproduce the pinned legacy installer's duplicate-key traversal in an
/// already authenticated private staging copy, before ordinary log preparation.
pub(super) fn prepare_legacy_ini(
    contract: Option<&crate::archive_generation::reviewed_manual::NativePatcherContract>,
    ini: &Path,
    cancelled: &AtomicBool,
) -> Result<Option<serde_json::Value>> {
    use sha2::{Digest, Sha256};
    let Some(contract) = contract
        .filter(|contract| contract.component_guid() == "581433cd-4c71-848a-92f3-03064c7eae94")
    else {
        return Ok(None);
    };
    ensure!(
        contract.original_program_run().is_some()
            && contract.expected_game() == Some(odypatcher::Game::K1),
        "Legacy duplicate-key preparation requires its private original-program contract"
    );
    let original = read(ini, cancelled)?;
    let text = std::str::from_utf8(&original).context("Legacy Swoop INI is not UTF-8")?;
    let effective = crate::archive_generation::source_plan_namespace::effective_ini_text(
        contract.component_guid(),
        text,
    )?
    .context("Legacy Swoop translation absent")?;
    let effective_sha = format!("{:x}", Sha256::digest(effective.as_bytes()));
    ensure!(
        effective_sha == "9cdd73beb2ec1f8a87e22543180809712353ca9fc16cff1742b23cf7c79e5ecb",
        "Legacy Swoop translated INI identity differs"
    );
    ensure!(
        read(ini, cancelled)? == original,
        "Legacy Swoop private INI changed"
    );
    crate::recovery::atomic_copy(ini, std::io::Cursor::new(effective.as_bytes()), cancelled)?;
    ensure!(
        read(ini, cancelled)? == effective.as_bytes(),
        "Legacy Swoop preparation publication differs"
    );
    Ok(Some(
        serde_json::json!({"Policy":"exact-swoop-legacy-first-value-twice-v1",
        "OriginalIniSha256":format!("{:x}", Sha256::digest(&original)),
        "EffectiveIniSha256":effective_sha,"GffOperations":7}),
    ))
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Patcher preparation cancelled"
    );
    Ok(())
}
fn read(path: &Path, cancelled: &AtomicBool) -> Result<Vec<u8>> {
    let mut file = crate::paths::open_regular_read(path)?.take((LIMIT + 1) as u64);
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 16384];
    loop {
        check(cancelled)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        ensure!(bytes.len() <= LIMIT, "Patcher INI exceeds 8 MiB");
    }
    Ok(bytes)
}
/// Common BOM forms, without guessing a legacy code page. Invalid text fails.
pub(super) fn decode(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        ensure!(
            bytes.len().is_multiple_of(4),
            "Patcher INI has incomplete UTF-32 text"
        );
        let little = bytes[0] == 0xff;
        return bytes[4..]
            .chunks_exact(4)
            .map(|part| {
                let raw = [part[0], part[1], part[2], part[3]];
                char::from_u32(if little {
                    u32::from_le_bytes(raw)
                } else {
                    u32::from_be_bytes(raw)
                })
                .context("Patcher INI contains invalid UTF-32")
            })
            .collect();
    }
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        ensure!(
            bytes.len().is_multiple_of(2),
            "Patcher INI has incomplete UTF-16 text"
        );
        let little = bytes[0] == 0xff;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|part| {
                if little {
                    u16::from_le_bytes([part[0], part[1]])
                } else {
                    u16::from_be_bytes([part[0], part[1]])
                }
            })
            .collect();
        return String::from_utf16(&units).context("Patcher INI contains invalid UTF-16");
    }
    Ok(
        std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .context("Patcher INI needs UTF-8 or BOM-marked Unicode text")?
            .to_owned(),
    )
}

fn ordinary(path: &Path) -> Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink() && (metadata.is_dir() || metadata.is_file()),
        "Patcher preparation refuses links and special files"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Patcher preparation refuses reparse points"
        );
    }
    Ok(metadata)
}

/// Copy the complete payload so namespace selection and relative assets retain
/// their layout, while preparation and backend writes cannot alter source INIs.
pub(super) fn stage(
    source: &Path,
    cancelled: &AtomicBool,
) -> Result<(tempfile::TempDir, std::path::PathBuf)> {
    ensure!(
        ordinary(source)?.is_dir(),
        "Patcher source must be a directory"
    );
    let owner = tempfile::tempdir()?;
    let root = owner.path().join(
        if source
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("tslpatchdata"))
        {
            "tslpatchdata"
        } else {
            "payload"
        },
    );
    std::fs::create_dir(&root)?;
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    for (index, entry) in walkdir::WalkDir::new(source)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .enumerate()
    {
        check(cancelled)?;
        ensure!(index < 100_000, "Patcher staging inventory exceeds bounds");
        let entry = entry?;
        let metadata = ordinary(entry.path())?;
        let target = root.join(entry.path().strip_prefix(source)?);
        if metadata.is_dir() {
            std::fs::create_dir(&target)?;
            continue;
        }
        let mut input = crate::paths::open_regular_read(entry.path())?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        loop {
            check(cancelled)?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .context("Patcher staging size overflow")?;
            ensure!(
                total <= 64 * 1024 * 1024 * 1024,
                "Patcher staging exceeds 64 GiB"
            );
            output.write_all(&buffer[..count])?;
        }
        output.set_permissions(metadata.permissions())?;
    }
    Ok((owner, root))
}

pub(super) fn normalize(root: &Path, cancelled: &AtomicBool) -> Result<usize> {
    normalize_scope(root, None, cancelled)
}

// The caller verifies the original selected configuration and full staged inventory first.
pub(super) fn normalize_selected(
    root: &Path,
    selected: &Path,
    cancelled: &AtomicBool,
) -> Result<usize> {
    ensure!(
        selected.starts_with(root) && selected != root,
        "Selected preparation path escapes package"
    );
    normalize_scope(root, Some(selected), cancelled)
}
fn normalize_scope(root: &Path, selected: Option<&Path>, cancelled: &AtomicBool) -> Result<usize> {
    let metadata = ordinary(root)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Patcher preparation needs an ordinary directory"
    );
    let replacements = [
        (r"^\s*PlaintextLog\s*=\s*0\s*$", "PlaintextLog=1"),
        (r"^\s*LookupGameFolder\s*=\s*1\s*$", "LookupGameFolder=0"),
        (r"^\s*ConfirmMessage\s*=\s*.*$", "ConfirmMessage=N/A"),
    ]
    .into_iter()
    .map(|(pattern, replacement)| {
        Ok((
            regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .multi_line(true)
                .build()?,
            replacement,
        ))
    })
    .collect::<Result<Vec<_>>>()?;
    let mut pending = Vec::new();
    let mut seen = 0usize;
    let mut total = 0usize;
    for (index, entry) in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        check(cancelled)?;
        ensure!(
            index < 100_000,
            "Patcher preparation inventory exceeds bounds"
        );
        let entry = entry?;
        ordinary(entry.path())?;
        ensure!(
            !entry.file_type().is_symlink()
                && (entry.file_type().is_dir() || entry.file_type().is_file()),
            "Patcher preparation refuses links and special files"
        );
        if !entry.file_type().is_file()
            || !entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ini"))
        {
            continue;
        }
        if selected.is_some_and(|path| path != entry.path()) {
            continue;
        }
        seen += 1;
        ensure!(seen <= 4096, "Patcher preparation exceeds 4096 INI files");
        let before = read(entry.path(), cancelled)?;
        total = total
            .checked_add(before.len())
            .context("Patcher INI size overflow")?;
        ensure!(
            total <= 64 * 1024 * 1024,
            "Patcher preparation exceeds 64 MiB of INI input"
        );
        let mut after = decode(&before)?;
        for (pattern, replacement) in &replacements {
            check(cancelled)?;
            after = pattern.replace_all(&after, *replacement).into_owned();
        }
        ensure!(after.len() <= LIMIT, "Normalized patcher INI exceeds 8 MiB");
        if before != after.as_bytes() {
            pending.push((entry.into_path(), before, after));
        }
    }
    ensure!(seen > 0, "No INI files found for patcher preparation");
    // Validate the entire input batch before writing. Existing run recovery owns
    // any partial preparation if cancellation or I/O fails during publication.
    for (path, before, after) in &pending {
        check(cancelled)?;
        ensure!(
            read(path, cancelled)? == *before,
            "Patcher INI changed during preparation"
        );
        let permissions = crate::paths::open_regular_read(path)?
            .metadata()?
            .permissions();
        let mut temporary =
            tempfile::NamedTempFile::new_in(path.parent().context("INI needs a parent")?)?;
        temporary.write_all(after.as_bytes())?;
        temporary.as_file().set_permissions(permissions)?;
        temporary.as_file().sync_all()?;
        check(cancelled)?;
        temporary.persist(path).map_err(|error| error.error)?;
    }
    Ok(pending.len())
}

/// Keep built-in installation and future prediction inside the process, even
/// when a Windows package supplies its own compiler executable.
pub(super) fn disable_external_compilers(installer: &mut odypatcher::ModInstaller) -> Result<()> {
    for script in &mut installer
        .config()
        .map_err(|error| anyhow::anyhow!("Read native patch configuration: {error}"))?
        .patches_nss
    {
        script.nwnnsscomp_path = None;
    }
    Ok(())
}

/// These closed source recipes cover the original English installer branch.
/// Inspect the actual game header in both private prediction and real execution.
pub(super) fn verify_required_game_language(
    contract: Option<&crate::archive_generation::reviewed_manual::NativePatcherContract>,
    game: &Path,
    cancelled: &AtomicBool,
) -> Result<()> {
    if !contract.is_some_and(|contract| contract.requires_english_game()) {
        return Ok(());
    }
    check(cancelled)?;
    let root = std::fs::symlink_metadata(game)?;
    ensure!(
        root.is_dir() && !root.file_type().is_symlink(),
        "Community Patch game root must be a real directory"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            root.file_attributes() & 0x400 == 0,
            "Community Patch game root cannot be a reparse point"
        );
    }
    let mut found = None;
    for (index, entry) in std::fs::read_dir(game)?.enumerate() {
        check(cancelled)?;
        ensure!(
            index < 100_000,
            "Community Patch game root exceeds its entry bound"
        );
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case("dialog.tlk"))
        {
            ensure!(
                found.is_none(),
                "Ambiguous Community Patch game language file"
            );
            found = Some(entry.path());
        }
    }
    let path = found.context("Community Patch requires the actual game dialog.tlk")?;
    let mut file = crate::paths::open_regular_read(&path)?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header)?;
    check(cancelled)?;
    ensure!(
        &header[..8] == b"TLK V3.0" && u32::from_le_bytes(header[8..].try_into()?) == 0,
        "Closed native Community Patch recipes currently require an English dialog.tlk"
    );
    Ok(())
}
