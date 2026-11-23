//! Preserve the exact original TOMB installer's implicit Override policy for its
//! intentional duplicate script copy. Both writes and all 37 operations remain.
//! Only a private replayed contract enables this; fresh byte proof is mandatory.
use anyhow::{Result, ensure};
use odypatcher::patcher::mods::{
    template::{ModBase, OverrideType},
    tlk::TlkModifier,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use super::tomb_shadow_observer;
use crate::archive_generation::reviewed_manual::NativePatcherContract;

const POLICY: &str = "exact-tomb-original-default-and-identical-script-v1";
const OPERATIONS: usize = 37;
const MODULE_COPY: usize = 14;
const COPIES: [&str; 24] = [
    "k_oei_userdef.ncs",
    "pmbj02.tpc",
    "tsl_randomrevan.ncs",
    "711kor_ud.ncs",
    "a_1st_act.ncs",
    "a_evil_kreia.ncs",
    "a_mala_act.ncs",
    "a_spawn_mand.ncs",
    "c_chk_hp.ncs",
    "k_fab_fade2_ud.ncs",
    "k_fab_fade_ud.ncs",
    "k_fab_malak_ud.ncs",
    "k_fab_revan_ud.ncs",
    "k_hb_check_mines.ncs",
    "k_oei_userdef.ncs",
    "them_revan_djc.utc",
    "them_revan_djg.utc",
    "them_fexile_jc.utc",
    "them_fexile_jg.utc",
    "them_fexile_js.utc",
    "them_mexile_jc.utc",
    "them_mexile_jg.utc",
    "them_mexile_js.utc",
    "tr_last_vision.ncs",
];
const GFF: [&str; 10] = [
    "n_jedicounf001.utc",
    "n_jedicounm001.utc",
    "n_jedimemf001.utc",
    "n_jedimemm001.utc",
    "n_jedimemm002.utc",
    "npc_v_malak.utc",
    "p_kreia_evil001.utc",
    "them_revan_jc.utc",
    "them_revan_jg.utc",
    "101atton.dlg",
];
// ConfigReader emits one top-level modifier per non-directive source field.
const GFF_FIELDS: [usize; 10] = [14, 18, 18, 14, 18, 14, 7, 1, 1, 29];

/// The caller must consume this after install, before publishing success. There
/// is no constructor or Deserialize path other than successful exact adaptation.
#[must_use = "TOMB compatibility requires its mandatory post-install verification"]
pub(super) struct Prepared {
    base: PathBuf,
}

impl Prepared {
    pub(super) fn finish(
        self,
        base: &Path,
        game: &Path,
        report: &odypatcher::RunReport,
        cancelled: &AtomicBool,
    ) -> Result<Value> {
        check(cancelled)?;
        ensure!(base == self.base, "TOMB verification staging root changed");
        ensure!(
            report.patches_configured == OPERATIONS
                && report.patches_completed == OPERATIONS
                && report.patches_skipped == 0
                && report.patches_failed == 0
                && report.warnings == 0
                && report.errors == 0,
            "TOMB compatibility requires all 37 original operations completed with zero skips, warnings, errors or failures"
        );
        let verified = tomb_shadow_observer::verify(base, game, cancelled)?;
        check(cancelled)?;
        Ok(json!({
            "Policy": POLICY,
            "Configured": OPERATIONS,
            "Completed": OPERATIONS,
            "Skipped": 0,
            "Failed": 0,
            "Warnings": 0,
            "Errors": 0,
            "IdentityVerification": verified.receipt(),
        }))
    }
}

/// Raw INI/staged inventory verification precedes the caller's normal private
/// INI normalization. The compiled contract pins that original 35,454-byte INI
/// (82e88f...a2fb), whose entire text contains no OverrideType directive. Do not
/// edit normalized/raw INI bytes here or alter any existing log severity.
pub(super) fn adapt(
    contract: Option<&NativePatcherContract>,
    installer: &mut odypatcher::ModInstaller,
    base: &Path,
    cancelled: &AtomicBool,
) -> Result<Option<Prepared>> {
    if !contract.is_some_and(|contract| contract.tomb_shadow_identity_observation()) {
        return Ok(None);
    }
    check(cancelled)?;
    tomb_shadow_observer::verify_source(base, cancelled)?;
    let config = installer
        .config()
        .map_err(|error| anyhow::anyhow!("Read exact original TOMB configuration: {error}"))?;
    ensure!(
        config.install_list.len() == COPIES.len()
            && config.patches_2da.len() == 2
            && config.patches_gff.len() == GFF.len()
            && config.patches_ncs.is_empty()
            && config.patches_nss.is_empty()
            && config.patches_ssf.is_empty()
            && config.patches_tlk.modifiers.len() == 1
            && config.patches_tlk.female.is_none(),
        "TOMB original file-operation inventory differs"
    );
    for (index, (copy, name)) in config.install_list.iter().zip(COPIES).enumerate() {
        let destination = match index {
            0 | 1 => "Override",
            2 => r"Modules\101per.mod",
            _ => r"Modules\711kor.mod",
        };
        let mut expected = original_base(name, destination);
        expected.action = "Copy ".into();
        expected.skip_if_not_replace = true;
        expected.replace_file = index != 0;
        ensure!(copy.base == expected, "TOMB original copy {index} differs");
    }
    for (patch, name) in config
        .patches_2da
        .iter()
        .zip(["appearance.2da", "globalcat.2da"])
    {
        ensure!(
            patch.base == original_base(name, "Override") && patch.modifiers.len() == 1,
            "TOMB original 2DA operation differs"
        );
    }
    for (index, (patch, name)) in config.patches_gff.iter().zip(GFF).enumerate() {
        let destination = if index == 9 {
            r"Modules\101per.mod"
        } else {
            r"Modules\711kor.mod"
        };
        ensure!(
            patch.base == original_base(name, destination)
                && patch.modifiers.len() == GFF_FIELDS[index],
            "TOMB original GFF operation {index} differs"
        );
    }
    let tlk = &config.patches_tlk;
    let mut expected_tlk = original_base("append.tlk", ".");
    expected_tlk.saveas = "dialog.tlk".into();
    ensure!(
        tlk.base == expected_tlk && tlk.sourcefile_f == "appendf.tlk" && tlk.store_memory,
        "TOMB original TLK descriptor differs"
    );
    let TlkModifier::Merge(merge) = &tlk.modifiers[0] else {
        anyhow::bail!("TOMB original TLK merge was replaced");
    };
    ensure!(
        merge.strref_mappings.len() == 4
            && (0..4).all(|index| merge.strref_mappings.get(&index) == Some(&index))
            && merge.tlk_filepath_f.is_none(),
        "TOMB original TLK mappings differ"
    );
    // Linked ConfigReader preserves these raw destination strings. This is the
    // sole mutation after every descriptor and source check has succeeded.
    check(cancelled)?;
    config.install_list[MODULE_COPY].base.override_type = OverrideType::IGNORE.into();
    installer.log.add_note(format!(
        "TOMB compatibility {POLICY}: preserved all 37 original operations and both k_oei_userdef.ncs copies; restored the original implicit Override policy only for Modules/711kor.mod. Mandatory fresh pinned source/Override/module equality is required before reviewed success."
    ));
    Ok(Some(Prepared {
        base: base.to_path_buf(),
    }))
}

fn original_base(source: &str, destination: &str) -> ModBase {
    ModBase {
        sourcefile: source.into(),
        sourcefolder: ".".into(),
        saveas: source.into(),
        replace_file: false,
        destination: destination.into(),
        action: "Patch ".into(),
        override_type: OverrideType::WARN.into(),
        skip_if_not_replace: false,
    }
}

fn check(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Cancelled during exact TOMB compatibility verification"
    );
    Ok(())
}
