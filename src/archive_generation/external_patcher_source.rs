//! Fresh original-package authority for external patcher backends.
//!
//! This admission deliberately does not invoke the native INI/NSS compiler: an
//! external patcher receives freshly authenticated original package bytes, with
//! exact source-deletion interpretations recorded separately in private staging.
//! Serialized bindings are declarations, never archive authority.
use crate::{Component, Guide, Instruction, archive, draft};
use anyhow::{Context, Result, bail, ensure};
use odypatcher::patcher::ini::{CaseInsensitiveDict, ConfigParser, OptionXform};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
mod authority;
mod decoder;
const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
const MAX_MEMBER: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 10000;
const UNIFORM: &str = "231ca858a5c354c3a76f8408b06256376930bcc999e8967491b1bc750547abe0";

#[derive(Clone, Serialize, PartialEq, Eq)]
struct Member {
    path: String,
    directory: bool,
    bytes: u64,
    sha256: String,
}
#[derive(Clone, Serialize)]
struct ArchivePin {
    filename: String,
    sha256: String,
    bytes: u64,
}
struct Slot {
    tree: tempfile::TempDir,
    members: BTreeMap<String, Member>,
    archive: ArchivePin,
    native: String,
}

/// Only `resolve` can create this witness. Its private directory owns all source
/// bytes for the entire duration of staging and execution.
pub(crate) struct ExternalPatcherSource {
    component_guid: String,
    operation_index: usize,
    game: odypatcher::Game,
    tree: tempfile::TempDir,
    executable: PathBuf,
    namespace: Option<usize>,
    ini: PathBuf,
    members: BTreeMap<String, Member>,
    provenance: Value,
}
impl ExternalPatcherSource {
    pub(crate) fn component_guid(&self) -> &str {
        &self.component_guid
    }
    pub(crate) fn operation_index(&self) -> usize {
        self.operation_index
    }
    pub(crate) fn expected_game(&self) -> odypatcher::Game {
        self.game
    }
    pub(crate) fn original_package_root(&self) -> &Path {
        self.tree.path()
    }
    pub(crate) fn executable_relative_path(&self) -> &Path {
        &self.executable
    }
    pub(crate) fn selected_namespace(&self) -> Option<usize> {
        self.namespace
    }
    pub(crate) fn selected_ini_relative_path(&self) -> &Path {
        &self.ini
    }
    pub(crate) fn provenance(&self) -> &Value {
        &self.provenance
    }
    /// Interpret the original guide's one deliberately deleted HQ resource only
    /// after authenticating the complete original program and private payload.
    pub(crate) fn prepare_source_ini(
        &self,
        base: &Path,
        cancelled: &AtomicBool,
    ) -> Result<Option<Value>> {
        if self.component_guid != "97c5f09c-0521-82df-beec-0896382e83f9" {
            return Ok(None);
        }
        self.verify_staged_original(base, cancelled)?;
        ensure!(
            base.canonicalize()? != self.tree.path().canonicalize()?,
            "HQ source interpretation requires a separate private staging copy"
        );
        let source = &self.provenance;
        let program = &source["OriginalProgram"];
        let program_sha = super::reviewed_manual::value_digest(program)?;
        let missing = "High Quality Blasters 1.1/tslpatchdata/keblastore.utm";
        ensure!(
            self.game == odypatcher::Game::K1
                && self.operation_index == 2
                && self.namespace.is_none()
                && source["OriginalOperation"] == 2
                && source["Archive"]["filename"] == "High Quality Blasters 1.1.zip"
                && source["Archive"]["sha256"]
                    == "7450288a232020e2b7ea7b408cb0d19e86783b364d57b1f3de4c1b5939671f63"
                && source["Archive"]["bytes"] == 24860026
                && program["id"] == "step-137"
                && program_sha
                    == "113c1abe70cae858ff1f11f353169c9ac3544525f6a5acf38c0d15bec42e7b65"
                && program["operations"][1]
                    == json!({"type":"delete","path":"{extract}/step-137/1/High Quality Blasters 1.1/tslpatchdata/keblastore.utm"})
                && program["operations"][2]["type"] == "gui_patcher"
                && source["OriginalSourceDeletions"] == json!([missing])
                && self.ini == Path::new("High Quality Blasters 1.1/tslpatchdata/changes.ini")
                && !self
                    .members
                    .keys()
                    .any(|name| name.eq_ignore_ascii_case(missing)),
            "HQ external source interpretation lacks exact original program authority"
        );
        let ini = base.join(&self.ini);
        let original = fs::read(&ini)?;
        ensure!(
            original.len() == 9650
                && sha(&original)
                    == "17084e0e86b8de5d18b6118c8f33150dce9dd99fbfd4c7d659f3c4cd89ccaa43",
            "HQ external raw INI identity differs"
        );
        let raw = std::str::from_utf8(&original).context("HQ original INI is not UTF-8")?;
        let effective = super::source_plan_namespace::effective_hq_ini_text(raw)?;
        ensure!(
            effective.len() == 9552
                && sha(effective.as_bytes())
                    == "dd7df5824257c4392f688f051e2962821766b0a7c0db73038c48b7f3249ef7cb",
            "HQ external effective INI identity differs"
        );
        check(cancelled)?;
        ensure!(fs::read(&ini)? == original, "HQ external raw INI changed");
        crate::recovery::atomic_copy(&ini, std::io::Cursor::new(effective.as_bytes()), cancelled)?;
        ensure!(
            fs::read(&ini)? == effective.as_bytes(),
            "HQ external effective INI publication differs"
        );
        Ok(Some(
            json!({"Policy":"exact-hq-author-deleted-store-source-v1",
            "OriginalDeleteOperation":1,"OriginalPatcherOperation":2,
            "OriginalSourceProgramSha256":program_sha,
            "RawIni":{"path":self.ini,"bytes":original.len(),"sha256":sha(&original)},
            "EffectiveIni":{"path":self.ini,"bytes":effective.len(),"sha256":sha(effective.as_bytes())},
            "OmittedGffListEntry":"File6=keblastore.utm","OmittedClosedSection":"keblastore.utm"}),
        ))
    }
    /// Exhaustive equality after original ordered source deletions, including
    /// empty directories and retained author resources. No normalized INI or native NSS substitution is
    /// accepted, and newly added files are rejected before external execution.
    pub(crate) fn verify_staged_original(&self, base: &Path, cancelled: &AtomicBool) -> Result<()> {
        ordinary(&fs::symlink_metadata(base)?, true)?;
        let base = base.canonicalize()?;
        let mut pending = vec![base.clone()];
        let mut seen = BTreeSet::new();
        while let Some(dir) = pending.pop() {
            for item in fs::read_dir(dir)? {
                check(cancelled)?;
                let path = item?.path();
                let metadata = fs::symlink_metadata(&path)?;
                ordinary(&metadata, metadata.is_dir())?;
                let name = path
                    .strip_prefix(&base)?
                    .to_str()
                    .context("Non UTF8 original package path")?
                    .replace('\\', "/");
                literal(&name)?;
                let expected = self
                    .members
                    .get(&name)
                    .context("Unexpected original package member")?;
                ensure!(
                    seen.insert(name.clone()) && seen.len() <= MAX_ENTRIES,
                    "Duplicate original package member"
                );
                ensure!(
                    expected.directory == metadata.is_dir(),
                    "Original package member kind differs"
                );
                if metadata.is_dir() {
                    pending.push(path);
                } else {
                    let (digest, bytes) = hash_file(&path, MAX_MEMBER, cancelled)?;
                    ensure!(
                        digest == expected.sha256 && bytes == expected.bytes,
                        "Original package member bytes differ: {name}"
                    );
                }
            }
        }
        ensure!(
            seen.len() == self.members.len(),
            "Original package member missing"
        );
        ensure!(
            self.members
                .get(&self.executable.to_string_lossy().replace('\\', "/"))
                .is_some_and(|m| !m.directory)
                && self
                    .members
                    .get(&self.ini.to_string_lossy().replace('\\', "/"))
                    .is_some_and(|m| !m.directory),
            "Original executable or selected INI missing"
        );
        check(cancelled)
    }
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Original external source admission cancelled"
    );
    Ok(())
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn literal(value: &str) -> Result<PathBuf> {
    ensure!(
        !value.is_empty()
            && value.len() <= 4096
            && !value.contains(['\\', '*', '?', ':'])
            && !value.chars().any(char::is_control),
        "Unsafe original source path"
    );
    let path = crate::paths::literal_relative(value)?;
    ensure!(
        path.to_string_lossy() == value && path.components().count() <= 64,
        "Noncanonical original source path"
    );
    Ok(path)
}
fn ordinary(metadata: &fs::Metadata, directory: bool) -> Result<()> {
    ensure!(
        !metadata.file_type().is_symlink()
            && if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
        "Original source rejects links and special files"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Original source rejects reparse points"
        );
    }
    Ok(())
}
fn hash_file(path: &Path, limit: u64, cancelled: &AtomicBool) -> Result<(String, u64)> {
    ordinary(&fs::symlink_metadata(path)?, false)?;
    let mut file = crate::paths::open_regular_read(path)?;
    let before = file.metadata()?;
    ensure!(before.len() <= limit, "Original source exceeds byte bound");
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check(cancelled)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(bytes <= before.len(), "Original source grew during read");
        digest.update(&buffer[..n]);
    }
    let after = file.metadata()?;
    ensure!(
        bytes == before.len()
            && after.len() == before.len()
            && after.modified().ok() == before.modified().ok(),
        "Original source changed during read"
    );
    Ok((format!("{:x}", digest.finalize()), bytes))
}
fn expansion_limit(digest: &str) -> u64 {
    if digest == UNIFORM {
        542155537
    } else {
        MAX_EXPANDED
    }
}
fn snapshot(
    root: &Path,
    filename: &str,
    digest: &str,
    scratch: &Path,
    cancelled: &AtomicBool,
) -> Result<(tempfile::NamedTempFile, ArchivePin)> {
    ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid original archive digest"
    );
    let path = root.join(literal(filename)?);
    ordinary(&fs::symlink_metadata(&path)?, false)?;
    let mut file = crate::paths::open_regular_read(&path)?;
    let expected = file.metadata()?.len();
    ensure!(
        expected > 0 && expected <= MAX_EXPANDED,
        "Original archive exceeds snapshot bound"
    );
    let mut output = tempfile::Builder::new()
        .prefix("modsync-external-input-")
        .tempfile_in(scratch)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        check(cancelled)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(bytes <= expected, "Original archive grew during snapshot");
        hash.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
    }
    ensure!(
        bytes == expected && format!("{:x}", hash.finalize()) == digest,
        "Original archive SHA256 differs: {filename}"
    );
    output.flush()?;
    Ok((
        output,
        ArchivePin {
            filename: filename.into(),
            sha256: digest.into(),
            bytes,
        },
    ))
}
fn member<'a>(members: &'a BTreeMap<String, Member>, path: &str) -> Result<&'a Member> {
    literal(path)?;
    let key = crate::unicode_case::ordinal_key(path);
    let mut found = members
        .values()
        .filter(|m| crate::unicode_case::ordinal_key(&m.path) == key);
    let value = found
        .next()
        .context("Original selected package member absent")?;
    ensure!(
        found.next().is_none() && !value.directory,
        "Original selected package member ambiguous"
    );
    Ok(value)
}
fn parser(path: &Path) -> Result<ConfigParser> {
    ordinary(&fs::symlink_metadata(path)?, false)?;
    ensure!(
        fs::metadata(path)?.len() <= 2 * 1024 * 1024,
        "Original namespace map exceeds bound"
    );
    let bytes = fs::read(path)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "Original namespace map exceeds bound"
    );
    let text = super::source_plan_namespace::decode_ini_bytes(&bytes)
        .map_err(|e| anyhow::anyhow!("Original namespace decode: {e}"))?;
    ConfigParser::read_string(text.trim_start_matches('\u{feff}'), OptionXform::Identity)
        .map_err(|e| anyhow::anyhow!("Original namespace map: {e}"))
}
fn field<'a>(parser: &'a ConfigParser, section: &str, key: &str) -> Option<&'a str> {
    parser
        .sections()
        .find(|s| s.eq_ignore_ascii_case(section))
        .and_then(|s| parser.section(s))?
        .items()
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.as_deref())
}
fn selection(slot: &Slot, executable: &str, namespace: Option<usize>) -> Result<(String, Value)> {
    let parent = Path::new(executable)
        .parent()
        .context("Original installer parent absent")?;
    let map_name = parent
        .join("tslpatchdata/namespaces.ini")
        .to_string_lossy()
        .replace('\\', "/");
    if let Some(index) = namespace {
        let map = member(&slot.members, &map_name)?;
        let parser = parser(&slot.tree.path().join(literal(&map.path)?))?;
        let section = parser
            .sections()
            .find(|s| s.eq_ignore_ascii_case("Namespaces"))
            .and_then(|s| parser.section(s))
            .context("Original namespace list absent")?;
        let mut names = BTreeSet::new();
        ensure!(
            section
                .keys()
                .all(|key| names.insert(crate::unicode_case::ordinal_key(key))),
            "Ambiguous original namespace key"
        );
        let fields = CaseInsensitiveDict::from_section(section);
        ensure!(
            fields.items().count() <= 256,
            "Original namespace count exceeds bound"
        );
        let (key, id) = fields
            .items()
            .nth(index)
            .context("Original namespace index absent")?;
        let ini = field(&parser, id, "IniName")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(odypatcher::patcher::namespaces::PatcherNamespace::DEFAULT_INI_FILENAME);
        let data = field(&parser, id, "DataPath")
            .unwrap_or("")
            .replace('\\', "/");
        let mut path = Path::new(&map.path)
            .parent()
            .context("Namespace map parent absent")?
            .to_owned();
        if !data.is_empty() && data != "." {
            path.push(literal(&data)?);
        }
        path.push(literal(&ini.replace('\\', "/"))?);
        let selected = member(&slot.members, &path.to_string_lossy().replace('\\', "/"))?;
        Ok((
            selected.path.clone(),
            json!({"Index":index,"Key":key,"Id":id,"Map":map,"IniName":ini,"DataPath":data}),
        ))
    } else {
        let ini = parent
            .join("tslpatchdata/changes.ini")
            .to_string_lossy()
            .replace('\\', "/");
        let selected = member(&slot.members, &ini)?;
        Ok((selected.path.clone(), Value::Null))
    }
}

/// Authenticate a real selected run against immutable original context and a
/// freshly snapshotted whole archive. No external program is invoked here.
pub(crate) fn resolve(
    component: &Component,
    instruction: &Instruction,
    operation_index: usize,
    mods_root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<Option<ExternalPatcherSource>> {
    check(cancelled)?;
    if !instruction.action.eq_ignore_ascii_case("Patcher") {
        return Ok(None);
    }
    ensure!(component.instructions.get(operation_index).is_some_and(|i| serde_json::to_value(i).ok() == serde_json::to_value(instruction).ok()), "External source run differs from component ordinal");
    let Some(context) = authority::context(component)? else {
        return Ok(None);
    };
    let root = super::resolved::local_root(mods_root)?;
    let scratch = options
        .scratch_dir
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    ordinary(&fs::symlink_metadata(&scratch)?, true)?;
    let scratch = scratch.canonicalize()?;
    ensure!(
        !scratch.starts_with(&root),
        "External source scratch must be outside mods root"
    );
    let mut slots = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut input_bytes = 0u64;
    let mut expanded_bytes = 0u64;
    let expansion_bound = context
        .extracts
        .iter()
        .map(|(_, _, digest)| expansion_limit(digest))
        .max()
        .unwrap_or(MAX_EXPANDED);
    for (slot_name, filename, digest) in &context.extracts {
        let (body, pin) = snapshot(&root, filename, digest, &scratch, cancelled)?;
        input_bytes = input_bytes
            .checked_add(pin.bytes)
            .context("Original aggregate input size overflow")?;
        ensure!(
            input_bytes <= 1024 * 1024 * 1024,
            "Original aggregate input bound exceeded"
        );
        inputs.push(json!(pin));
        let native = format!(
            "<<modDirectory>>/.modsync-reviewed-{}-slot{}",
            component.guid,
            slots.len()
        );
        let slot = decoder::decode_slot(body.path(), pin, &scratch, options, cancelled, native)?;
        expanded_bytes = expanded_bytes
            .checked_add(
                slot.members
                    .values()
                    .map(|member| member.bytes)
                    .sum::<u64>(),
            )
            .context("Original aggregate expansion overflow")?;
        ensure!(
            expanded_bytes <= expansion_bound,
            "Original aggregate expansion bound exceeded"
        );
        ensure!(
            slots.insert(slot_name.clone(), slot).is_none(),
            "Duplicate original extraction slot"
        );
    }
    // This compares every executable instruction, including expanded directory
    // copies and original ordinal-derived GUIDs, independently of native INIs.
    let selected = authority::select_run(&context, component, operation_index, &slots)?;
    let mut slot = slots
        .remove(&selected.slot)
        .context("Selected original archive slot absent")?;
    let original_manifest = slot.members.clone();
    for relative in &selected.source_deletions {
        check(cancelled)?;
        if let Some(deleted) = slot.members.remove(relative) {
            ensure!(
                !deleted.directory,
                "Original source deletion targets a directory"
            );
            let target = slot.tree.path().join(literal(&deleted.path)?);
            let (digest, bytes) = hash_file(&target, MAX_MEMBER, cancelled)?;
            ensure!(
                digest == deleted.sha256 && bytes == deleted.bytes,
                "Original source deletion member changed"
            );
            fs::remove_file(target)?;
        }
    }
    let installer = member(&slot.members, &selected.executable)?;
    ensure!(
        installer.sha256 == selected.executable_sha,
        "Original shipped executable SHA256 differs"
    );
    let executable = installer.path.clone();
    let (ini, namespace_member) = selection(&slot, &executable, selected.namespace)?;
    let provenance = json!({"Policy":"fresh-original-external-package-v1","ComponentGuid":component.guid,"InstructionIndex":operation_index,
        "OriginalOperation":selected.ordinal,"Game":context.game,"GuideSha256":context.guide_sha,"PlanSha256":context.plan_sha,
        "OriginalOwner":context.original,"OriginalProgram":context.step,"Authority":context.authority,
        "Inputs":inputs,"Archive":slot.archive,
        "OriginalPackageManifest":original_manifest.values().collect::<Vec<_>>(),
        "OriginalPackageManifestSha256":sha(&serde_json::to_vec(&original_manifest.values().collect::<Vec<_>>())?),
        "OriginalSourceDeletions":selected.source_deletions,
        "PackageManifest":slot.members.values().collect::<Vec<_>>(),
        "PackageManifestSha256":sha(&serde_json::to_vec(&slot.members.values().collect::<Vec<_>>())?),
        "Executable":installer,"NamespaceMember":namespace_member,"RawSelectedINI":member(&slot.members,&ini)?});
    let witness = ExternalPatcherSource {
        component_guid: component.guid.clone(),
        operation_index,
        game: if context.game == "k1" {
            odypatcher::Game::K1
        } else {
            odypatcher::Game::K2
        },
        tree: slot.tree,
        executable: literal(&executable)?,
        namespace: selected.namespace,
        ini: literal(&ini)?,
        members: slot.members,
        provenance,
    };
    witness.verify_staged_original(witness.original_package_root(), cancelled)?;
    Ok(Some(witness))
}

/// Validate complete original source owners before external backend processing.
/// Nonpatcher file-effect owners remain with their existing runtime authority.
pub(crate) fn verify_component_original(
    component: &Component,
    mods_root: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<bool> {
    let claimed = [
        "AuthoredRecipeBinding",
        "SourcePlanPatcherBinding",
        "SourcePlanNamespaceBinding",
    ]
    .iter()
    .any(|key| component.metadata.contains_key(*key));
    if !claimed
        || !component
            .instructions
            .iter()
            .any(|i| i.action.eq_ignore_ascii_case("Patcher"))
    {
        return Ok(false);
    }
    for (index, instruction) in component
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| i.action.eq_ignore_ascii_case("Patcher"))
    {
        ensure!(
            resolve(component, instruction, index, mods_root, options, cancelled)?.is_some(),
            "Original external component patcher lacks authority"
        );
    }
    Ok(true)
}
