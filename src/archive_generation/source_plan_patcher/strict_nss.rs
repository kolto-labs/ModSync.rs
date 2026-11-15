//! Closed original-archive certificate for the reviewed K2 Better Disciple recipe.
use super::{Entry, Member};
use anyhow::{Result, ensure};
use odypatcher::patcher::{config::PatcherConfig, ini::ConfigParser};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const OWNER: &str = "ec7ead25-991c-89e5-bee8-01e1a5b8ef69";
const PACKAGE: &str = "Better Disciple Meditation";
const ARCHIVE_SHA: &str = "abdc7157ce333f841c5fb275c01a237c34c64f3bad3a07446a7cd1ec1f8bad73";
const MAINS: [&str; 3] = [
    "a_forcepointheal.nss",
    "m_imp_dscplmed.nss",
    "m_dscmed_plc_htb.nss",
];
const CLOSURE: [&str; 5] = [
    "a_forcepointheal.nss",
    "m_imp_dscplmed.nss",
    "m_dscmed_plc_htb.nss",
    "k_inc_debug.nss",
    "nwscript.nss",
];
const ORPHANS: [&str; 5] = [
    "gff_disciple_SoundExists_0",
    "gff_disciple_CameraID_0",
    "gff_disciple_SoundExists_1",
    "gff_disciple_CameraID_1",
    "gff_disciple_EditorInfo_0",
];
// Path, directory, exact original byte length and SHA-256, including implicit directories.
const INVENTORY: &[(&str, bool, u64, &str)] = &[
    ("Better Disciple Meditation", true, 0, ""),
    (
        "Better Disciple Meditation/.DS_Store",
        false,
        10244,
        "32465afabdcd7bc13c7c2e28ba6a61b84e7055a5fb9a29174a2f4db63d8e586d",
    ),
    (
        "Better Disciple Meditation/TSLPatcher.exe",
        false,
        685056,
        "638aaeea4e40ffb6c592f68787f2140318a0dec7f2cde85908806ce3b5bc5131",
    ),
    (
        "Better Disciple Meditation/readme.txt",
        false,
        1719,
        "7b283b5d5d654c39c00fbba6cffb8a079630e2ce97dc2a9f4aed2f7709944521",
    ),
    ("Better Disciple Meditation/tslpatchdata", true, 0, ""),
    (
        "Better Disciple Meditation/tslpatchdata/.DS_Store",
        false,
        6148,
        "40cfb15040a264bcd4b9eee2451f70facf303d774d97dd5c05e51fab9eaec38d",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/a_forcepointheal.nss",
        false,
        1408,
        "ece7e615ecc5ac805cf99574cea1850552476952fa9b3ace0e898bb6a0464453",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/append.tlk",
        false,
        69,
        "067c25cf68d0ab31dd6a76479bdaa6cfcc45a3723cd87740082a369bda71f88f",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/changes.ini",
        false,
        2388,
        "19f8776266779fdc10a855934cbc05a1bc74b45fa9ef68d904184b957ee6d1c7",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/disciplemed.tga",
        false,
        65580,
        "d1a7aa0fcba5d8ca3bc0b93635a5733b80c96b1bc77d2b6d42ce70f0218731b2",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/dscplmed_caster.utp",
        false,
        1799,
        "426058d35a89ab834dc0c6d1b7016f70d30c5a7342215d9afa4e6bd6ca10036a",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/effecticon.2da",
        false,
        6975,
        "7dda21d468da42589b813df93cc3b16bae55dd272379387a2b670fa705b12f53",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/info.rtf",
        false,
        2858,
        "b2cd0fcea1131058f40857630d764d5b055c55705ac54de90094e694c29e39c5",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/k_inc_debug.nss",
        false,
        3184,
        "c91e349e992ae3d97fb4a9b77a9b089cc0508285ad6da883e6428389bdbb3813",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/m_dscmed_plc_htb.nss",
        false,
        456,
        "728676fab8116accf28b9879712f0e34441625d9cf863731ec2dc0c129094d31",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/m_imp_dscplmed.nss",
        false,
        1611,
        "b43913d2764d9dddcee744d5aa553f470202bc9e279a2525fdf7bdfa909b4eb7",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/nwnnsscomp.exe",
        false,
        212992,
        "539eb689d2e0d3751aeed273385865278bef6696c46bc0cab116b40c3b2fe820",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/nwscript.nss",
        false,
        266225,
        "afb8adb2c2806b9c5ef75360ec75ecc62b74bcfe9d42cc4f4e1d305b26a7805d",
    ),
    (
        "Better Disciple Meditation/tslpatchdata/spells.2da",
        false,
        49125,
        "9189a1e484fb0c381d3f18711cb26ad2baaae6fafb692223bae4d1af22913845",
    ),
    ("__MACOSX", true, 0, ""),
    ("__MACOSX/Better Disciple Meditation", true, 0, ""),
    (
        "__MACOSX/Better Disciple Meditation/._.DS_Store",
        false,
        120,
        "2f380f4a3d05a8d90c2106f50da75064e9ce57a598599dc5404f8f69a0223aa9",
    ),
    (
        "__MACOSX/Better Disciple Meditation/._TSLPatcher.exe",
        false,
        176,
        "3abdea785122245c7f05e7700ad52816d4882c801f3bb25803806a2afa87b376",
    ),
    (
        "__MACOSX/Better Disciple Meditation/._readme.txt",
        false,
        442,
        "dd73a6ac4032912f96dffd47d3065d879bb7448a3112049d85ca9a89cf23d11e",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata",
        true,
        0,
        "",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._.DS_Store",
        false,
        120,
        "2f380f4a3d05a8d90c2106f50da75064e9ce57a598599dc5404f8f69a0223aa9",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._a_forcepointheal.nss",
        false,
        384,
        "d321684e008b06fe2973d7e95c7c7712e8bb747feaf66eeefeab3003ce447bb1",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._append.tlk",
        false,
        213,
        "6ea44abeb64f5b2d371d0f6cc4306975070d2aaaf6c0ffa10a0119d34c451b6a",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._changes.ini",
        false,
        176,
        "b05875ad374f553fe41eab638d3495d189594a76bac712ee769d9f8e9cae8cc0",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._disciplemed.tga",
        false,
        233,
        "325c784026f1fb2ded5a684c47ef218903f9e190523d7dea336c2052fa2092d6",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._info.rtf",
        false,
        266,
        "1f35ee64461299467cbe2556c31f51e04953b569eb0f3a512423ebc838ea9ca3",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._k_inc_debug.nss",
        false,
        176,
        "eb11e3b45c26643e8c6d7f840446a3a0573e27e620f3bed6a5491840784ffae2",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._m_dscmed_plc_htb.nss",
        false,
        285,
        "5a2dfa33797eca9fb702f373b803373788c037503f81b731b3b19232272d8d7d",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._m_imp_dscplmed.nss",
        false,
        285,
        "25a2b1776a85842dfd66c2fb548f1faea22c55936bd6524841da139a77c5805c",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._nwnnsscomp.exe",
        false,
        213,
        "ac6601a98a3832fc7fba41ff50cdbf331fb1bc2cbb34637810d607f72b779a04",
    ),
    (
        "__MACOSX/Better Disciple Meditation/tslpatchdata/._nwscript.nss",
        false,
        269,
        "98898bcd7fc5bb3cf1647a41dad27a27457ebce0b3913510d033c6a1920d9612",
    ),
];

/// Authority can only be issued by this module after checking the complete archive.
#[derive(Clone)]
pub(crate) struct CertifiedNssInputs {
    _sealed: (),
}

pub(super) fn issue(
    entry: &Entry,
    archive_bytes: u64,
    members: &BTreeMap<String, Member>,
) -> Result<Option<CertifiedNssInputs>> {
    if entry.row.guid != OWNER {
        return Ok(None);
    }
    ensure!(
        entry.row.game == "k2"
            && entry.row.component_index == 103
            && entry.row.step_index == 103
            && entry.step.id == "step-104",
        "Certified NSS owner identity changed"
    );
    ensure!(
        entry.archive == "Better Disciple Meditation.zip"
            && entry.archive_sha == ARCHIVE_SHA
            && archive_bytes == 532304
            && entry.package == PACKAGE,
        "Certified NSS archive identity changed"
    );
    ensure!(
        entry.installer == format!("{PACKAGE}/TSLPatcher.exe")
            && entry.installer_sha
                == "638aaeea4e40ffb6c592f68787f2140318a0dec7f2cde85908806ce3b5bc5131"
            && entry.ini == format!("{PACKAGE}/tslpatchdata/changes.ini"),
        "Certified NSS installer identity changed"
    );
    ensure!(
        members.len() == INVENTORY.len()
            && INVENTORY
                .iter()
                .all(
                    |(path, directory, bytes, hash)| members.get(*path).is_some_and(|m| m.path
                        == *path
                        && m.directory == *directory
                        && m.bytes == *bytes
                        && m.sha256 == *hash)
                ),
        "Certified NSS complete original inventory changed"
    );
    Ok(Some(CertifiedNssInputs { _sealed: () }))
}

pub(super) fn policy_json() -> Value {
    use odypatcher::patcher::mods::nss::strict::{POLICY_VERSION, VirtualLimits};
    use sha2::{Digest, Sha256};
    // These are the production API defaults also used by engine::strict_nss.
    // Bind their actual values, not a separately maintained copy of the ceilings.
    let limits = VirtualLimits::default();
    let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    // include_bytes embeds immutable build inputs; no runtime filesystem lookup
    // or submitted metadata can change this authority's dependency identities.
    let compiler_provenance = include_bytes!("../../../vendor/bionsscomp/PROVENANCE.json");
    let bridge_provenance = include_bytes!("../../../vendor/odypatcher/PROVENANCE.json");
    let bridge_source = include_bytes!("../../../vendor/odypatcher/src/patcher/mods/nss/strict.rs");
    json!({"policy":"better-disciple-closed-nss-v1", "owner":OWNER,
        "archive_sha256":ARCHIVE_SHA, "archive_bytes":532304,
        "inventory":INVENTORY, "mains":MAINS, "closure":CLOSURE,
        "game":2, "sourcefolder":".", "inactive_sections":ORPHANS,
        "compiler": {
            "policy_version":POLICY_VERSION,
            "provenance_sha256":digest(compiler_provenance),
            "bridge_provenance_sha256":digest(bridge_provenance),
            "bridge_source_sha256":digest(bridge_source),
            "limits": {
                "max_source_bytes":limits.max_source_bytes,
                "max_header_bytes":limits.max_header_bytes,
                "max_total_source_bytes":limits.max_total_source_bytes,
                "max_files":limits.max_files,
                "max_include_depth":limits.max_include_depth,
                "max_expanded_bytes":limits.max_expanded_bytes,
                "max_tokens":limits.max_tokens,
                "max_ast_nodes":limits.max_ast_nodes,
                "max_syntax_depth":limits.max_syntax_depth,
                "max_expression_tokens":limits.max_expression_tokens,
                "max_recursion":limits.max_recursion,
                "max_work":limits.max_work,
                "max_diagnostics":limits.max_diagnostics,
                "max_output_bytes":limits.max_output_bytes
            }
        }
    })
}

impl CertifiedNssInputs {
    pub(crate) fn mains(&self) -> &'static [&'static str] {
        &MAINS
    }
    pub(crate) fn sourcefolder(&self) -> &'static str {
        "."
    }
    pub(crate) fn game(&self) -> odypatcher::Game {
        odypatcher::Game::K2
    }
    pub(crate) fn inputs(&self) -> Vec<(&'static str, u64, &'static str)> {
        CLOSURE
            .iter()
            .map(|name| {
                let (_, _, bytes, hash) = INVENTORY
                    .iter()
                    .find(|(path, _, _, _)| *path == format!("{PACKAGE}/tslpatchdata/{name}"))
                    .expect("closed certificate input");
                (*name, *bytes, *hash)
            })
            .collect()
    }
    pub(super) fn contains_member(&self, member: &Member) -> bool {
        INVENTORY.iter().any(|(path, directory, bytes, hash)| {
            member.path == *path
                && member.directory == *directory
                && member.bytes == *bytes
                && member.sha256 == *hash
        })
    }
    pub(super) fn is_inert_member(&self, member: &Member) -> bool {
        self.contains_member(member)
            && (member.path.starts_with("__MACOSX/")
                || member.path.ends_with("/.DS_Store")
                || member.path == format!("{PACKAGE}/tslpatchdata/nwnnsscomp.exe"))
    }
    pub(super) fn consumed_paths(&self) -> BTreeSet<String> {
        CLOSURE
            .iter()
            .map(|n| format!("{PACKAGE}/tslpatchdata/{n}"))
            .collect()
    }
    pub(super) fn allows_log(&self, message: &str) -> bool {
        message
            == format!(
                "There are some orphaned ini sections found in the changes:\n{}",
                ORPHANS.join("\n")
            )
    }
    pub(super) fn validate_ini(&self, parser: &ConfigParser, config: &PatcherConfig) -> Result<()> {
        let fields = parser
            .section("CompileList")
            .ok_or_else(|| anyhow::anyhow!("Certified CompileList missing"))?;
        let expected: Vec<_> = MAINS
            .iter()
            .enumerate()
            .map(|(i, n)| (format!("Replace{i}"), Some((*n).to_owned())))
            .collect();
        ensure!(
            fields.items() == expected.as_slice(),
            "Certified CompileList order changed"
        );
        self.validate_config(config)
    }
    pub(crate) fn validate_config(&self, config: &PatcherConfig) -> Result<()> {
        ensure!(
            config.game_number == Some(2)
                && config.save_processed_scripts == 0
                && config.script_compiler_flags.is_empty()
                && !config.ignore_file_extensions
                && config.patches_nss.len() == 3,
            "Certified NSS configuration changed"
        );
        for (patch, name) in config.patches_nss.iter().zip(MAINS) {
            let b = &patch.base;
            ensure!(
                b.sourcefile == name
                    && b.sourcefolder == "."
                    && b.destination == "Override"
                    && b.saveas == name.replace(".nss", ".ncs")
                    && b.replace_file
                    && b.action == "Compile"
                    && b.skip_if_not_replace
                    && b.override_type == "warn"
                    && patch.compiler_flags.is_empty(),
                "Certified NSS patch semantics changed"
            );
        }
        Ok(())
    }
}
