//! Exact Galaxy base input/choice admission, NOT a complete component approval.
//!
//! There is deliberately no instruction generator, DraftReview mutation, or
//! completion API here. The dedicated recipe consumer binds/replays the whole
//! component; the engine defers its complete native base and then its overlay.
use crate::{Component, Guide, archive};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const PROFILE: &str = include_str!("galaxy_map_vanilla.json");
const PROFILE_SHA256: &str = "76794533372b440b9b804a3afac5a3909e1e323da3479bab165ffdfe664167b7";
const GUID: &str = "7c11497d-de47-8eee-b80a-967b984a2822";
const OUTCOME: &str = "native-zero-warnings-changed-game-v1";
const LIMIT: u64 = 32 * 1024 * 1024;
const GAPS: [&str; 2] = [
    "actual-full-base-native-outcome-CI-proof",
    "actual-install-checkpoint-recovery-and-receipt-proof",
];

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    schema: u32,
    component_guid: String,
    source_guide_revision: String,
    source_guide_sha256: String,
    source_plan_sha256: String,
    raw_input_component: Value,
    canonical_input_component: Value,
    plan_step: Value,
    archive: ArchivePin,
    members: Vec<Member>,
    namespaces: Vec<Namespace>,
    selected_namespace_id: String,
    selected_namespace_index: usize,
    native_outcome_contract: String,
    admission_scope: String,
    consumer_gaps: Vec<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArchivePin {
    filename: String,
    sha256: String,
    bytes: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Member {
    path: String,
    sha256: String,
    bytes: u64,
    directory: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Namespace {
    index: usize,
    namespace_id: String,
    name: String,
    ini_filename: String,
    info_filename: String,
    data_path: String,
    description: String,
}

/// Only the exact explicit-choice factory can construct this input authority.
/// Possession does NOT authorize execution or clear the original component review.
pub(crate) struct VanillaContract {
    profile: Profile,
}
impl VanillaContract {
    pub(crate) fn namespace_index(&self) -> usize {
        self.profile.selected_namespace_index
    }
    pub(crate) fn namespace_id(&self) -> &str {
        &self.profile.selected_namespace_id
    }
    /// A consumer must require the built-in backend, fresh native success log,
    /// zero parser/install warnings/errors/failed operations and changed game
    /// files before committing its component checkpoint. Exit status is insufficient.
    pub(crate) fn strict_outcome_contract(&self) -> &str {
        OUTCOME
    }
    pub(crate) fn consumer_gaps(&self) -> &[String] {
        &self.profile.consumer_gaps
    }

    pub(crate) fn input_receipt(&self) -> Result<Value> {
        Ok(json!({"Schema":1,"Kind":"GalaxyVanillaInputAdmissionOnly",
            "FullComponentApproved":false,"ProfileSha256":PROFILE_SHA256,
            "ExplicitNamespaceId":self.namespace_id(),"NamespaceIndex":self.namespace_index(),
            "NativeOutcomeContract":OUTCOME,"BuiltinBackendRequired":true,
            "ConsumerGaps":self.consumer_gaps(),"Profile":serde_json::to_value(&self.profile)?}))
    }

    /// Materialize only into an engine-selected private scratch directory, then
    /// authenticate the complete raw tree before native namespace normalization.
    pub(crate) fn stage_private(
        &self,
        archives_root: &Path,
        options: &archive::Options,
        cancelled: &AtomicBool,
    ) -> Result<tempfile::TempDir> {
        let scratch = options
            .scratch_dir
            .as_ref()
            .context("Galaxy needs private scratch")?;
        let staged = tempfile::Builder::new()
            .prefix("modsync-k1-galaxy-")
            .tempdir_in(scratch)?;
        let mut bounded = options.clone();
        bounded.max_expanded_bytes = Some(options.max_expanded_bytes.unwrap_or(LIMIT).min(LIMIT));
        let filename = Path::new(&self.profile.archive.filename);
        let archive_path = archives_root.join(filename);
        verify_file(
            &archive_path,
            self.profile.archive.bytes,
            &self.profile.archive.sha256,
            cancelled,
        )?;
        for member in &self.profile.members {
            if member.directory {
                std::fs::create_dir_all(
                    staged
                        .path()
                        .join(literal(member.path.trim_end_matches('/'))?),
                )?;
            }
        }
        for entry in archive::unpack(&archive_path, filename, &bounded, cancelled)? {
            check(cancelled)?;
            let output = staged.path().join(&entry.path);
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut input = crate::paths::open_regular_read(entry.file.path())?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            std::io::copy(&mut input, &mut output)?;
        }
        self.verify_staged(staged.path(), cancelled)?;
        verify_file(
            &archive_path,
            self.profile.archive.bytes,
            &self.profile.archive.sha256,
            cancelled,
        )?;
        Ok(staged)
    }

    /// Verify the COMPLETE private extracted raw payload before normalization or
    /// namespace staging. No decoder/process is invoked, no files are changed.
    pub(crate) fn verify_staged(&self, base: &Path, cancelled: &AtomicBool) -> Result<()> {
        real_directory(base)?;
        let base = base.canonicalize()?;
        let expected = expected_members(&self.profile)?;
        let mut seen = BTreeSet::new();
        let mut pending = vec![base.clone()];
        while let Some(directory) = pending.pop() {
            check(cancelled)?;
            for entry in std::fs::read_dir(directory)? {
                check(cancelled)?;
                let path = entry?.path();
                let metadata = std::fs::symlink_metadata(&path)?;
                ordinary(&metadata)?;
                ensure!(
                    path.canonicalize()?.starts_with(&base),
                    "Galaxy staged path escaped its root"
                );
                let relative = path
                    .strip_prefix(&base)?
                    .to_str()
                    .context("Non-UTF8 Galaxy staged path")?
                    .replace('\\', "/");
                ensure!(
                    seen.len() < 128 && seen.insert(relative.clone()),
                    "Duplicate or oversized Galaxy staged inventory"
                );
                let pin = expected
                    .get(&relative)
                    .context("Unreviewed extra Galaxy staged member")?;
                ensure!(
                    metadata.is_dir() == pin.directory,
                    "Galaxy staged member kind changed"
                );
                if pin.directory {
                    pending.push(path);
                } else {
                    verify_file(&path, pin.bytes, &pin.sha256, cancelled)?;
                }
            }
        }
        ensure!(
            seen.len() == expected.len(),
            "Incomplete Galaxy staged payload"
        );
        verify_namespaces(&self.profile, &base.join("tslpatchdata/namespaces.ini"))?;
        check(cancelled)
    }
}

fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Galaxy input admission cancelled"
    );
    Ok(())
}
fn ordinary(metadata: &std::fs::Metadata) -> Result<()> {
    ensure!(
        !metadata.file_type().is_symlink() && (metadata.is_dir() || metadata.is_file()),
        "Galaxy input rejects links and special files"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Galaxy input rejects reparse points"
        );
    }
    Ok(())
}
fn real_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    ordinary(&metadata)?;
    ensure!(metadata.is_dir(), "Galaxy input requires a real directory");
    Ok(())
}
fn literal(path: &str) -> Result<PathBuf> {
    ensure!(
        !path.is_empty()
            && path.len() <= 4096
            && !path.contains(['\\', '*', '?', ':'])
            && !path.chars().any(char::is_control),
        "Unsafe Galaxy member name"
    );
    let relative = crate::paths::literal_relative(path)?;
    ensure!(
        relative.to_str() == Some(path),
        "Noncanonical Galaxy member name"
    );
    Ok(relative)
}
fn verify_file(path: &Path, size: u64, expected: &str, cancelled: &AtomicBool) -> Result<()> {
    ensure!(size <= LIMIT, "Galaxy file exceeds bounded profile");
    let mut file = crate::paths::open_regular_read(path)?;
    ensure!(file.metadata()?.len() == size, "Galaxy file size differs");
    let mut digest = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        check(cancelled)?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        count += read as u64;
        ensure!(count <= size, "Galaxy file grew during verification");
        digest.update(&buffer[..read]);
    }
    ensure!(
        count == size && format!("{:x}", digest.finalize()) == expected,
        "Galaxy file identity differs"
    );
    Ok(())
}
fn expected_members(profile: &Profile) -> Result<BTreeMap<String, &Member>> {
    let mut result = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    ensure!(
        profile.members.len() == 18,
        "Galaxy profile inventory differs"
    );
    for member in &profile.members {
        let name = if member.directory {
            member
                .path
                .strip_suffix('/')
                .context("Galaxy directory is not canonical")?
        } else {
            &member.path
        };
        literal(name)?;
        ensure!(
            names.insert(crate::unicode_case::ordinal_key(name))
                && result.insert(name.to_owned(), member).is_none(),
            "Galaxy member alias"
        );
        ensure!(
            !member.directory || member.bytes == 0,
            "Galaxy directory has data"
        );
        total = total
            .checked_add(member.bytes)
            .context("Galaxy inventory size overflow")?;
        ensure!(total <= LIMIT, "Galaxy expanded inventory exceeds bound");
    }
    Ok(result)
}
fn verify_namespaces(profile: &Profile, path: &Path) -> Result<()> {
    use odypatcher::patcher::{
        ini::{ConfigParser, OptionXform},
        reader::NamespaceReader,
    };
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(path)?
        .take(1022)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() == 1021
            && format!("{:x}", Sha256::digest(&bytes))
                == "1197890c946e5f87bed2d1acd97b2cfd2deea9667e4947f3ac666a87d7c1a0f1",
        "Galaxy namespace bytes differ"
    );
    let text = std::str::from_utf8(&bytes).context("Pinned Galaxy namespace text is not UTF-8")?;
    let ini = ConfigParser::read_string(text, OptionXform::Lower)
        .map_err(|error| anyhow::anyhow!("Parse pinned Galaxy namespaces: {error}"))?;
    let actual = NamespaceReader::new(ini)
        .load()
        .map_err(|error| anyhow::anyhow!("Read pinned Galaxy namespaces: {error}"))?;
    ensure!(
        actual.len() == 2 && profile.namespaces.len() == 2,
        "Galaxy namespace inventory differs"
    );
    for (index, (actual, expected)) in actual.iter().zip(&profile.namespaces).enumerate() {
        ensure!(
            expected.index == index
                && actual.namespace_id == expected.namespace_id
                && actual.name == expected.name
                && actual.ini_filename == expected.ini_filename
                && actual.info_filename == expected.info_filename
                && actual.data_folderpath == Path::new(&expected.data_path)
                && actual.description == expected.description,
            "Galaxy namespace identity or order differs"
        );
    }
    let selected = &actual[profile.selected_namespace_index];
    ensure!(
        selected.namespace_id == "Vanilla"
            && selected.name == "Vanilla version"
            && selected.ini_filename == "changes.ini"
            && selected.info_filename == "info.rtf"
            && selected.data_folderpath.as_os_str().is_empty(),
        "Explicit Galaxy Vanilla namespace differs"
    );
    Ok(())
}

/// Admit only the named Vanilla variant of this exact full source component.
/// Ord Mantell and missing/implicit choices fail. This does not mutate the model.
pub(crate) fn admit_vanilla(
    component: &Component,
    explicit_choice: &str,
    archives_root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<VanillaContract> {
    check(cancelled)?;
    ensure!(
        explicit_choice == "Vanilla",
        "Galaxy requires the explicit reviewed Vanilla choice"
    );
    ensure!(
        PROFILE.len() <= 256 * 1024
            && format!("{:x}", Sha256::digest(PROFILE.as_bytes())) == PROFILE_SHA256,
        "Galaxy profile digest differs"
    );
    let profile: Profile = serde_json::from_str(PROFILE)?;
    ensure!(
        profile.schema == 1
            && profile.component_guid == GUID
            && component.guid == GUID
            && profile.source_guide_revision == "67dc5fd8da6abde324c51d32eee3d04687070dd6"
            && profile.source_guide_sha256
                == "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed"
            && profile.source_plan_sha256
                == "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74"
            && profile.selected_namespace_id == "Vanilla"
            && profile.selected_namespace_index == 0
            && profile.native_outcome_contract == OUTCOME
            && profile.consumer_gaps.iter().map(String::as_str).eq(GAPS),
        "Galaxy profile scope differs"
    );
    let original = serde_json::to_string(&profile.raw_input_component)?;
    let loaded = Guide::parse_preserving_selection(&original, "json")?;
    ensure!(
        loaded.components.len() == 1
            && serde_json::to_value(&loaded.components[0])? == profile.canonical_input_component
            && profile.raw_input_component == profile.canonical_input_component,
        "Galaxy raw/canonical source proof differs"
    );
    ensure!(
        serde_json::to_value(component)? == profile.canonical_input_component,
        "Galaxy complete source context differs"
    );
    real_directory(archives_root)?;
    let root = archives_root.canonicalize()?;
    let filename = literal(&profile.archive.filename)?;
    ensure!(
        filename.components().count() == 1,
        "Galaxy archive must be a root filename"
    );
    let archive_path = root.join(&filename);
    verify_file(
        &archive_path,
        profile.archive.bytes,
        &profile.archive.sha256,
        cancelled,
    )?;
    let expected = expected_members(&profile)?;
    let mut bounded = options.clone();
    bounded.max_expanded_bytes = Some(options.max_expanded_bytes.unwrap_or(LIMIT).min(LIMIT));
    let (inventory, expanded) =
        archive::list_original_with_size(&archive_path, &filename, &bounded, cancelled)?;
    ensure!(
        inventory.len() == expected.len()
            && expanded == profile.members.iter().map(|m| m.bytes).sum::<u64>(),
        "Galaxy archive inventory size differs"
    );
    let mut seen = BTreeSet::new();
    for item in inventory {
        let name = item.path.trim_end_matches('/');
        let pin = expected
            .get(name)
            .context("Unexpected Galaxy archive member")?;
        ensure!(
            seen.insert(name.to_owned()) && pin.directory == item.is_directory,
            "Galaxy archive kind or duplicate differs"
        );
    }
    let entries = archive::unpack(&archive_path, &filename, &bounded, cancelled)?;
    let mut files = BTreeSet::new();
    for entry in entries {
        let name = entry
            .path
            .to_str()
            .context("Non-UTF8 Galaxy archive member")?
            .replace('\\', "/");
        let pin = expected
            .get(&name)
            .context("Unexpected Galaxy decoded member")?;
        ensure!(
            !pin.directory && files.insert(name.to_owned()),
            "Galaxy decoded member duplicated"
        );
        verify_file(entry.file.path(), pin.bytes, &pin.sha256, cancelled)?;
        if name == "tslpatchdata/namespaces.ini" {
            verify_namespaces(&profile, entry.file.path())?;
        }
    }
    ensure!(
        files.len() == profile.members.iter().filter(|m| !m.directory).count(),
        "Incomplete Galaxy decoded payload"
    );
    verify_file(
        &archive_path,
        profile.archive.bytes,
        &profile.archive.sha256,
        cancelled,
    )?;
    Ok(VanillaContract { profile })
}
