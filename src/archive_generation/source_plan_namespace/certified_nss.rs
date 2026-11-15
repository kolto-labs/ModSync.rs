//! Strict compiler authority derived only from an authenticated original program run.
use super::Entry;
use anyhow::{Context, Result, ensure};
use odypatcher::{
    Game,
    patcher::{config::PatcherConfig, ini::ConfigParser, mods::ModBase},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

const K1_HEADER: &str = "13e64e3dc9f65a821c7ebf80e97cdfd65505d0c581f5464ea5845af2c310157f";
const K2_HEADER: &str = "afb8adb2c2806b9c5ef75360ec75ecc62b74bcfe9d42cc4f4e1d305b26a7805d";
const MAX_SCRIPT: usize = 128 * 1024;
const MAX_HEADER: usize = 512 * 1024;
const MAX_BUNDLE: usize = 1024 * 1024;
const MAX_MAINS: usize = 32;
const HEADER_PROVIDER_ARCHIVE: &str = "Better Disciple Meditation.zip";
const HEADER_PROVIDER_SHA: &str =
    "abdc7157ce333f841c5fb275c01a237c34c64f3bad3a07446a7cd1ec1f8bad73";
const HEADER_PROVIDER_BYTES: u64 = 532304;
const HEADER_PROVIDER_MEMBER: &str = "Better Disciple Meditation/tslpatchdata/nwscript.nss";
const HEADER_PROVIDER_MEMBER_BYTES: u64 = 266225;

#[derive(Clone, Serialize)]
#[serde(tag = "Kind")]
enum InputOrigin {
    SelectedPackage,
    OriginalPinnedHeader(Box<HeaderOrigin>),
}
#[derive(Clone, Serialize)]
struct HeaderOrigin {
    source_guide_sha256: String,
    source_plan_sha256: String,
    original_step: Value,
    archive_filename: String,
    archive_sha256: String,
    archive_bytes: u64,
    member_path: String,
    member_sha256: String,
    member_bytes: u64,
}

#[derive(Clone)]
pub(crate) struct CertifiedInput {
    package_path: String,
    sha256: String,
    raw: Arc<[u8]>,
    origin: InputOrigin,
}
impl CertifiedInput {
    pub(crate) fn package_path(&self) -> &str {
        &self.package_path
    }
    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }
    pub(crate) fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// No model field or public constructor can create this origin.
    pub(crate) fn is_prepared_header(&self) -> bool {
        matches!(&self.origin, InputOrigin::OriginalPinnedHeader(_))
    }
}
#[derive(Clone)]
pub(crate) struct CertifiedBundle {
    sourcefolder: String,
    inputs: BTreeMap<String, CertifiedInput>,
}
impl CertifiedBundle {
    pub(crate) fn sourcefolder(&self) -> &str {
        &self.sourcefolder
    }
    pub(crate) fn inputs(&self) -> &BTreeMap<String, CertifiedInput> {
        &self.inputs
    }
}
#[derive(Clone)]
pub(crate) struct CertifiedModuleSeed {
    pub(crate) package_path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
    pub(crate) replace_file: bool,
}
#[derive(Clone)]
pub(crate) struct CertifiedMain {
    base: ModBase,
    compiler_flags: String,
    source_name: String,
    bundle_index: usize,
    raw_source_sha256: String,
    tokens: Vec<String>,
    seed: Option<CertifiedModuleSeed>,
}
impl CertifiedMain {
    pub(crate) fn seed(&self) -> Option<&CertifiedModuleSeed> {
        self.seed.as_ref()
    }
    pub(crate) fn base(&self) -> &ModBase {
        &self.base
    }
    pub(crate) fn source_name(&self) -> &str {
        &self.source_name
    }
    pub(crate) fn bundle_index(&self) -> usize {
        self.bundle_index
    }
    pub(crate) fn raw_source_sha256(&self) -> &str {
        &self.raw_source_sha256
    }
}
/// No Deserialize or public constructor: only the parent's authenticated run can issue.
#[derive(Clone)]
pub(crate) struct CertifiedProgramNss {
    game: Game,
    selected_data_relative: String,
    game_number: Option<i32>,
    mains: Vec<CertifiedMain>,
    bundles: Vec<CertifiedBundle>,
}
impl CertifiedProgramNss {
    pub(crate) fn game(&self) -> Game {
        self.game
    }
    pub(crate) fn selected_data_relative(&self) -> &str {
        &self.selected_data_relative
    }
    pub(crate) fn mains(&self) -> &[CertifiedMain] {
        &self.mains
    }
    pub(crate) fn bundles(&self) -> &[CertifiedBundle] {
        &self.bundles
    }
    pub(crate) fn validate_config(&self, config: &PatcherConfig) -> Result<()> {
        ensure!(
            config.game_number == self.game_number
                && config.save_processed_scripts == 0
                && config.script_compiler_flags.is_empty()
                && !config.ignore_file_extensions
                && config.patches_nss.len() == self.mains.len(),
            "Original program compiler configuration changed"
        );
        for (patch, main) in config.patches_nss.iter().zip(&self.mains) {
            ensure!(
                patch.base == main.base && patch.compiler_flags == main.compiler_flags,
                "Original program ordered compiler patch semantics changed"
            );
        }
        Ok(())
    }
    pub(crate) fn binding_json(&self) -> Value {
        json!({"Policy":policy_json(),"Game":if self.game==Game::K1 {1}else{2},
            "SelectedDataRelative":self.selected_data_relative,"GameNumberHint":self.game_number,
            "Mains":self.mains.iter().map(|m|json!({"SourceName":m.source_name,"SourceFile":m.base.sourcefile,
                "SourceFolder":m.base.sourcefolder,"SaveAs":m.base.saveas,"Destination":m.base.destination,
                "Replace":m.base.replace_file,"Action":m.base.action,"OverrideType":m.base.override_type,
                "SkipIfNotReplace":m.base.skip_if_not_replace,"CompilerFlags":m.compiler_flags,
                "Bundle":m.bundle_index,"RawSourceSha256":m.raw_source_sha256,"Tokens":m.tokens,
                "ModuleSeed":m.seed.as_ref().map(|s|json!({"Path":s.package_path,"Sha256":s.sha256,"Bytes":s.bytes,"Replace":s.replace_file}))})).collect::<Vec<_>>(),
            "Bundles":self.bundles.iter().map(|b|json!({"SourceFolder":b.sourcefolder,
                "Inputs":b.inputs.iter().map(|(n,i)|json!({"Name":n,"Path":i.package_path,"Sha256":i.sha256,"Bytes":i.raw.len(),"Origin":i.origin})).collect::<Vec<_>>() })).collect::<Vec<_>>()})
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn canonical_name(name: &str) -> Result<String> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".nss").unwrap_or("");
    ensure!(
        !stem.is_empty()
            && stem.len() <= 64
            && stem
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
        "Original program NSS name is not one flat compiler identifier"
    );
    Ok(lower)
}
fn decoded(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => odypatcher::patcher::mods::nss::decode_windows_1252(bytes),
    }
}
fn relative_text(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .context("Non UTF8 certified compiler path")?
        .replace('\\', "/"))
}
fn member_folder(entry: &Entry, requested: &Path) -> Result<PathBuf> {
    // Joining the default empty source folder adds a trailing separator. Compare
    // path components so that directory spelling cannot make its own entry
    // disappear; case-insensitive aliases still must resolve to exactly one.
    let requested = relative_text(&requested.components().collect::<PathBuf>())?;
    let mut matches = entry
        .row
        .members
        .iter()
        .filter(|m| m.directory && m.path.eq_ignore_ascii_case(&requested));
    let found = matches
        .next()
        .context("Original compiler source folder absent")?;
    ensure!(
        matches.next().is_none(),
        "Original compiler source folder has aliases"
    );
    super::literal(&found.path)
}
fn collect_folder(
    entry: &Entry,
    tree: &Path,
    package: &Path,
    folder: &Path,
    cancel: &AtomicBool,
) -> Result<BTreeMap<String, CertifiedInput>> {
    let mut result = BTreeMap::new();
    for member in &entry.row.members {
        super::check(cancel)?;
        let path = super::literal(&member.path)?;
        if member.directory
            || path.parent() != Some(folder)
            || !path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("nss"))
        {
            continue;
        }
        let name = canonical_name(
            path.file_name()
                .and_then(|s| s.to_str())
                .context("Missing NSS filename")?,
        )?;
        let limit = if name == "nwscript.nss" {
            MAX_HEADER
        } else {
            MAX_SCRIPT
        };
        ensure!(
            member.bytes <= limit as u64,
            "Original NSS input exceeds raw bound"
        );
        let mut raw = Vec::new();
        crate::paths::open_regular_read(&tree.join(&path))?
            .take(limit as u64 + 1)
            .read_to_end(&mut raw)?;
        super::check(cancel)?;
        ensure!(
            raw.len() as u64 == member.bytes && digest(&raw) == member.sha256,
            "Original NSS input changed during issuance"
        );
        ensure!(
            decoded(&raw).len() <= limit,
            "Original NSS decoded input exceeds bound"
        );
        let pin = CertifiedInput {
            package_path: relative_text(path.strip_prefix(package)?)?,
            sha256: member.sha256.clone(),
            raw: raw.into(),
            origin: InputOrigin::SelectedPackage,
        };
        ensure!(
            result.insert(name, pin).is_none(),
            "Original compiler source folder has case aliases"
        );
        ensure!(
            result.len() <= 33,
            "Original compiler source folder exceeds input count bound"
        );
    }
    Ok(result)
}
// A missing header is satisfied by one explicit public ORIGINAL source input,
// never by the installed Better Disciple mod, game files, an ambient include
// directory, a caller-selected archive or a mutable cached filesystem path.
fn original_header_provider(
    game: Game,
    source_root: &Path,
    cancel: &AtomicBool,
) -> Result<CertifiedInput> {
    let (
        game_index,
        step_index,
        step_id,
        heading,
        archive_name,
        archive_sha,
        archive_bytes,
        member_name,
        member_bytes,
        header_sha,
    ) = match game {
        Game::K1 => (
            0,
            108,
            "step-109",
            "### Kill The Czerka Guard",
            "KillCzerkaJerk.zip",
            "c35c49ff297a2cefde992596d261ee365aefdcd233b55206bedbd5661e2d9769",
            489373,
            "tslpatchdata/nwscript.nss",
            216650,
            K1_HEADER,
        ),
        Game::K2 => (
            1,
            103,
            "step-104",
            "### Better Disciple Meditation",
            HEADER_PROVIDER_ARCHIVE,
            HEADER_PROVIDER_SHA,
            HEADER_PROVIDER_BYTES,
            HEADER_PROVIDER_MEMBER,
            HEADER_PROVIDER_MEMBER_BYTES,
            K2_HEADER,
        ),
    };
    let (guide, guide_sha, plan, plan_sha, _) = super::GUIDES[game_index];
    ensure!(
        digest(guide) == guide_sha && digest(plan) == plan_sha,
        "Original header provider guide/plan source changed"
    );
    let plan: Value = serde_json::from_slice(plan)?;
    let step = plan["steps"]
        .get(step_index)
        .context("Original header provider step absent")?;
    ensure!(
        step["id"] == step_id
            && step["source_heading"] == heading
            && step["operations"][0]
                == json!({"type":"extract","source":format!("{{archives}}/{archive_name}"),"destination":format!("{{extract}}/{step_id}/1")})
            && plan["guide_sha256"] == guide_sha
            && plan["archive_sha256"][format!("{{archives}}/{archive_name}")] == archive_sha,
        "Original header provider source association changed"
    );
    ensure!(
        source_root.is_absolute(),
        "Original header provider source root must be absolute"
    );
    let source = source_root.join(archive_name);
    let mut observed = PathBuf::new();
    for part in source.components() {
        ensure!(
            matches!(
                part,
                std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
                    | std::path::Component::Normal(_)
            ),
            "Original header provider path is not confined"
        );
        observed.push(part.as_os_str());
        let metadata = fs::symlink_metadata(&observed)?;
        ensure!(
            !metadata.file_type().is_symlink()
                && if observed == source {
                    metadata.is_file()
                } else {
                    metadata.is_dir()
                },
            "Original header provider path contains a link or special file"
        );
    }
    super::check(cancel)?;
    let file = crate::paths::open_regular_read(&source)?;
    ensure!(
        file.metadata()?.len() == archive_bytes,
        "Original header provider archive size differs"
    );
    // Own this exact bounded byte snapshot before ZIP parsing. No later read can
    // follow a replaced source path or replace a certified raw header.
    let mut snapshot = Vec::new();
    file.take(archive_bytes + 1).read_to_end(&mut snapshot)?;
    super::check(cancel)?;
    ensure!(
        snapshot.len() as u64 == archive_bytes && digest(&snapshot) == archive_sha,
        "Original header provider archive hash differs"
    );
    let mut archive = zip::ZipArchive::new(Cursor::new(snapshot))?;
    ensure!(
        !archive.is_empty() && archive.len() <= 128,
        "Original header provider ZIP entry bound differs"
    );
    let mut names = BTreeSet::new();
    let mut expanded = 0u64;
    let mut header = None;
    for index in 0..archive.len() {
        super::check(cancel)?;
        let member = archive.by_index(index)?;
        let name = member.name().trim_end_matches('/').to_owned();
        let literal = super::literal(&name)?;
        ensure!(
            relative_text(&literal)? == name && names.insert(name.to_ascii_lowercase()),
            "Original header provider ZIP has path aliases"
        );
        let kind = member.unix_mode().unwrap_or(0) & 0o170000;
        ensure!(
            !member.encrypted()
                && matches!(kind, 0 | 0o100000 | 0o040000)
                && (kind != 0o040000 || member.is_dir())
                && (kind != 0o100000 || !member.is_dir()),
            "Original header provider ZIP has encrypted or special members"
        );
        expanded = expanded
            .checked_add(member.size())
            .context("Original header provider expanded size overflow")?;
        ensure!(
            expanded <= 2 * 1024 * 1024 && member.size() <= 1024 * 1024,
            "Original header provider ZIP expansion exceeds bound"
        );
        if name == member_name {
            ensure!(
                !member.is_dir() && member.size() == member_bytes && header.is_none(),
                "Original header provider member differs"
            );
            let mut raw = Vec::new();
            member.take(MAX_HEADER as u64 + 1).read_to_end(&mut raw)?;
            ensure!(
                raw.len() as u64 == member_bytes
                    && digest(&raw) == header_sha
                    && decoded(&raw).len() <= MAX_HEADER,
                "Original header provider member hash differs from actual game"
            );
            header = Some(raw);
        }
    }
    let raw = header.context("Original pinned header member absent")?;
    Ok(CertifiedInput {
        package_path: member_name.into(),
        sha256: header_sha.into(),
        raw: raw.into(),
        origin: InputOrigin::OriginalPinnedHeader(Box::new(HeaderOrigin {
            source_guide_sha256: guide_sha.into(),
            source_plan_sha256: plan_sha.into(),
            original_step: step.clone(),
            archive_filename: archive_name.into(),
            archive_sha256: archive_sha.into(),
            archive_bytes,
            member_path: member_name.into(),
            member_sha256: header_sha.into(),
            member_bytes,
        })),
    })
}

/// Parent invokes only for a fully authenticated original-program run, after graph validation.
pub(super) fn issue(
    entry: &Entry,
    tree: &Path,
    parser: &ConfigParser,
    config: &PatcherConfig,
    source_root: &Path,
    cancel: &AtomicBool,
) -> Result<Option<CertifiedProgramNss>> {
    if config.patches_nss.is_empty() {
        return Ok(None);
    }
    super::check(cancel)?;
    ensure!(
        config.patches_nss.len() <= MAX_MAINS
            && config.save_processed_scripts == 0
            && config.script_compiler_flags.is_empty()
            && !config.ignore_file_extensions,
        "Original program compiler flags, processed scripts or count unsupported"
    );
    let game = match entry.row.game.as_str() {
        "k1" => Game::K1,
        "k2" => Game::K2,
        _ => anyhow::bail!("Unknown compiler game"),
    };
    let hint = if game == Game::K1 { 1 } else { 2 };
    ensure!(
        config.game_number.is_none_or(|n| n == hint)
            || super::field(parser, "Settings", "LookupGameFolder") == Some("0"),
        "Active original compiler game discovery differs from typed game"
    );
    let package = Path::new(&entry.row.installer.path)
        .parent()
        .context("Compiler installer parent absent")?;
    let data = Path::new(&entry.row.ini.path)
        .parent()
        .context("Compiler INI parent absent")?;
    let selected_data_relative = relative_text(data.strip_prefix(package)?)?;
    let mut folders = Vec::<(String, PathBuf)>::new();
    let mut seen = BTreeSet::new();
    for sourcefolder in std::iter::once(".").chain(
        config
            .patches_nss
            .iter()
            .map(|p| p.base.sourcefolder.as_str()),
    ) {
        let requested = data.join(super::folder(sourcefolder)?);
        let actual = member_folder(entry, &requested)?;
        let key = relative_text(&actual)?.to_ascii_lowercase();
        if seen.insert(key) {
            folders.push((sourcefolder.to_owned(), actual));
        }
    }
    let mut sources = BTreeMap::new();
    for (_, folder) in &folders {
        sources.insert(
            folder.clone(),
            collect_folder(entry, tree, package, folder, cancel)?,
        );
    }
    let mut bundles = Vec::new();
    let mut bundle_indexes = BTreeMap::new();
    let mut mains = Vec::new();
    let token = regex::Regex::new(r"#(?:2DAMEMORY|StrRef)[0-9]+#")?;
    for patch in &config.patches_nss {
        super::check(cancel)?;
        let base = &patch.base;
        super::validate_base_paths(base)?;
        let name = canonical_name(&base.sourcefile)?;
        ensure!(
            name != "nwscript.nss" && patch.compiler_flags.is_empty() && base.action == "Compile",
            "Original NSS main or compiler action unsupported"
        );
        let save = super::literal(&base.saveas.replace('\\', "/"))?;
        ensure!(
            save.components().count() == 1
                && save
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("ncs")),
            "Original compiled saveas must be one NCS resource"
        );
        let destination = super::literal(&base.destination.replace('\\', "/"))?;
        let module = destination.components().count() == 2
            && destination
                .components()
                .next()
                .is_some_and(|p| p.as_os_str().eq_ignore_ascii_case("modules"))
            && destination
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("mod"));
        ensure!(
            base.destination.eq_ignore_ascii_case("override") || module,
            "Original NSS destination requires supported loose/MOD output"
        );
        let seed = if module {
            let candidates = config
                .install_list
                .iter()
                .filter(|install| {
                    format!(
                        "{}/{}",
                        install.base.destination.replace('\\', "/"),
                        install.base.saveas
                    )
                    .eq_ignore_ascii_case(&base.destination.replace('\\', "/"))
                })
                .collect::<Vec<_>>();
            ensure!(
                candidates.len() <= 1,
                "Original NSS module has ambiguous preceding seeds"
            );
            if let Some(install) = candidates.first() {
                let wanted = data
                    .join(super::folder(&install.base.sourcefolder)?)
                    .join(super::literal(&install.base.sourcefile.replace('\\', "/"))?);
                let wanted = relative_text(&wanted)?;
                let mut matches = entry
                    .row
                    .members
                    .iter()
                    .filter(|m| !m.directory && m.path.eq_ignore_ascii_case(&wanted));
                let member = matches
                    .next()
                    .context("Original NSS preceding module seed absent")?;
                ensure!(
                    matches.next().is_none(),
                    "Original NSS module seed has case aliases"
                );
                let pin = super::file_pin(
                    &tree.join(super::literal(&member.path)?),
                    &member.path,
                    super::MAX_MEMBER,
                    cancel,
                )?;
                ensure!(
                    pin.sha256 == member.sha256 && pin.bytes == member.bytes,
                    "Original NSS module seed changed"
                );
                Some(CertifiedModuleSeed {
                    package_path: relative_text(Path::new(&member.path).strip_prefix(package)?)?,
                    sha256: pin.sha256,
                    bytes: pin.bytes,
                    replace_file: install.base.replace_file,
                })
            } else {
                None
            }
        } else {
            None
        };
        let index = if let Some(index) = bundle_indexes.get(&base.sourcefolder) {
            *index
        } else {
            let current = member_folder(entry, &data.join(super::folder(&base.sourcefolder)?))?;
            let mut inputs = BTreeMap::new();
            for (_, folder) in folders.iter().filter(|(_, p)| p != &current) {
                inputs.extend(sources[folder].clone());
            }
            inputs.extend(sources[&current].clone());
            if !inputs.contains_key("nwscript.nss") {
                inputs.insert(
                    "nwscript.nss".into(),
                    original_header_provider(game, source_root, cancel)?,
                );
            }
            let header = inputs
                .get("nwscript.nss")
                .context("Original compiler header absent")?;
            ensure!(
                header.sha256
                    == if game == Game::K1 {
                        K1_HEADER
                    } else {
                        K2_HEADER
                    },
                "Original compiler header differs from known actual game"
            );
            let limits = odypatcher::patcher::mods::nss::strict::VirtualLimits::default();
            ensure!(
                inputs.len() <= limits.max_files,
                "Original compiler closure exceeds file budget"
            );
            let mut raw_total = 0usize;
            let mut decoded_total = 0usize;
            for (name, input) in &inputs {
                raw_total = raw_total
                    .checked_add(input.raw.len())
                    .context("NSS total overflow")?;
                let text = decoded(&input.raw);
                decoded_total = decoded_total
                    .checked_add(text.len())
                    .context("NSS decoded total overflow")?;
                let ceiling = if name == "nwscript.nss" {
                    limits.max_header_bytes
                } else {
                    limits.max_source_bytes
                };
                ensure!(
                    text.len() <= ceiling,
                    "Original decoded NSS exceeds actual compiler ceiling"
                );
            }
            ensure!(
                raw_total <= MAX_BUNDLE
                    && decoded_total <= MAX_BUNDLE
                    && decoded_total <= limits.max_total_source_bytes,
                "Original compiler closure exceeds total source budget"
            );
            let index = bundles.len();
            bundles.push(CertifiedBundle {
                sourcefolder: base.sourcefolder.clone(),
                inputs,
            });
            bundle_indexes.insert(base.sourcefolder.clone(), index);
            index
        };
        let input = bundles[index]
            .inputs
            .get(&name)
            .context("Original compiler main absent from its explicit bundle")?;
        let tokens = token
            .find_iter(&decoded(input.raw()))
            .map(|m| m.as_str().to_owned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        mains.push(CertifiedMain {
            base: base.clone(),
            compiler_flags: patch.compiler_flags.clone(),
            source_name: name,
            bundle_index: index,
            raw_source_sha256: input.sha256.clone(),
            tokens,
            seed,
        });
    }
    let result = CertifiedProgramNss {
        game,
        selected_data_relative,
        game_number: config.game_number,
        mains,
        bundles,
    };
    result.validate_config(config)?;
    Ok(Some(result))
}
pub(super) fn policy_json() -> Value {
    use odypatcher::patcher::mods::nss::strict::{POLICY_VERSION, VirtualLimits};
    let l = VirtualLimits::default();
    json!({"Policy":"original-program-closed-nss-v1","Headers":{"K1":K1_HEADER,"K2":K2_HEADER},"MaxMains":MAX_MAINS,
        "SourcePolicy":"default-then-ordered-configured-then-current-flat-nss-v1",
        "MissingHeaderProviderK1":{"Game":1,"OriginalStep":"step-109","SourcePlanSha256":super::GUIDES[0].3,"Archive":"KillCzerkaJerk.zip","ArchiveSha256":"c35c49ff297a2cefde992596d261ee365aefdcd233b55206bedbd5661e2d9769","ArchiveBytes":489373,"Member":"tslpatchdata/nwscript.nss","MemberSha256":K1_HEADER,"MemberBytes":216650},
        "MissingHeaderProvider":{"Game":2,"OriginalStep":"step-104","SourcePlanSha256":super::GUIDES[1].3,"Archive":HEADER_PROVIDER_ARCHIVE,"ArchiveSha256":HEADER_PROVIDER_SHA,"ArchiveBytes":HEADER_PROVIDER_BYTES,"Member":HEADER_PROVIDER_MEMBER,"MemberSha256":K2_HEADER,"MemberBytes":HEADER_PROVIDER_MEMBER_BYTES},"TokenPolicy":"actual-ordered-patcher-memory-main-only-v1",
        "CompilerPolicyVersion":POLICY_VERSION,"Limits":[l.max_source_bytes,l.max_header_bytes,l.max_total_source_bytes,l.max_files,l.max_include_depth,l.max_expanded_bytes,l.max_tokens,l.max_ast_nodes,l.max_syntax_depth,l.max_expression_tokens,l.max_recursion,l.max_work as usize,l.max_diagnostics,l.max_output_bytes],
        "CompilerProvenance":digest(include_bytes!("../../../vendor/bionsscomp/PROVENANCE.json")),
        "BridgeProvenance":digest(include_bytes!("../../../vendor/odypatcher/PROVENANCE.json")),
        "BridgeSource":digest(include_bytes!("../../../vendor/odypatcher/src/patcher/mods/nss/strict.rs"))})
}
