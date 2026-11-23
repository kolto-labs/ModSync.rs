//! The exact original Companion installer treats its two `(lang-1)` writes as
//! successful no-ops on the existing localized fields. Preserve those operations.
use anyhow::{Context, Result, ensure};
use odypatcher::patcher::mods::gff::{
    FieldValueKind, GffPath, GffValue, LocalizedStringDelta, ModifyGFF,
};
use sha2::{Digest, Sha256};

pub(super) fn adapt(
    contract: Option<&crate::archive_generation::reviewed_manual::NativePatcherContract>,
    installer: &mut odypatcher::ModInstaller,
) -> Result<()> {
    if !contract.is_some_and(|contract| contract.companion_selector_compatibility()) {
        return Ok(());
    }
    let config = installer
        .config()
        .map_err(|error| anyhow::anyhow!("Read original Companion configuration: {error}"))?;
    let expected_files = [
        "a_light_x02.uti",
        "a_light_x09.uti",
        "p_baodur.utc",
        "p_disciple.utc",
        "p_handmaiden.utc",
        "p_kreia.utc",
        "p_visas.utc",
    ];
    let expected_copies = [
        "a_light_x02.uti",
        "a_light_x09.uti",
        "m_armor_bd.uti",
        "m_armor_dp.uti",
        "m_armor_hm.uti",
        "m_armor_kr.uti",
        "m_armor_vm.uti",
        "p_baodur.utc",
        "p_disciple.utc",
        "p_handmaiden.utc",
        "p_kreia.utc",
        "p_visas.utc",
    ];
    ensure!(
        config.patches_gff.len() == expected_files.len()
            && config.install_list.len() == expected_copies.len()
            && config.patches_2da.is_empty()
            && config.patches_ssf.is_empty()
            && config.patches_nss.is_empty()
            && config.patches_ncs.is_empty()
            && config.patches_tlk.modifiers.is_empty()
            && config.patches_tlk.female.is_none(),
        "Original Companion configuration inventory changed; expected 12 copies and 7 GFF file operations"
    );
    for ((patch, expected), modifiers) in config
        .patches_gff
        .iter()
        .zip(expected_files)
        .zip([2, 2, 1, 1, 1, 1, 1])
    {
        ensure!(
            patch.base.sourcefile == expected
                && patch.base.saveas == expected
                && patch.base.sourcefolder == "."
                && patch.base.destination == "Override"
                && !patch.base.replace_file
                && patch.base.action == "Patch "
                && patch.base.override_type == "warn"
                && !patch.base.skip_if_not_replace
                && patch.modifiers.len() == modifiers,
            "Original Companion GFF source, output, defaults, order or modifier count changed for {expected}"
        );
    }
    for (copy, expected) in config.install_list.iter().zip(expected_copies) {
        ensure!(
            copy.base.sourcefile == expected
                && copy.base.saveas == expected
                && copy.base.sourcefolder == "."
                && copy.base.destination == "Override"
                && !copy.base.replace_file
                && copy.base.action == "Copy "
                && copy.base.override_type == "warn"
                && copy.base.skip_if_not_replace,
            "Original Companion copy source, output, defaults or order changed for {expected}"
        );
    }
    // Installer::push_lists counts file operations (12 copies + 7 GFF patches).
    // PatcherConfig::patch_count instead flattens field operations, including
    // Bao-Dur's added struct and its two child fields (12 copies + 11 fields).
    const FILE_OPERATIONS: usize = 19;
    const FLATTENED_OPERATIONS: usize = 23;
    ensure!(
        config.install_list.len() + config.patches_gff.len() == FILE_OPERATIONS,
        "Companion file-operation count changed; expected {FILE_OPERATIONS}"
    );
    let flattened = config.clone().patch_count();
    ensure!(
        flattened == FLATTENED_OPERATIONS,
        "Companion flattened field-operation count changed: expected {FLATTENED_OPERATIONS}, observed {flattened}; file-operation contract remains {FILE_OPERATIONS}"
    );
    let expected = [
        (
            "a_light_x02.uti",
            "217f6cbefa877b1006f94c092568b2aee577a105d2658224194c2a0a51a649e7",
        ),
        (
            "a_light_x09.uti",
            "00c03d7c439cf2562dbb2592a1eca8ec7e21fba85957049a0c3088ca13aa00fc",
        ),
    ];
    // Check both original operations before mutating either parsed configuration.
    let mut positions = Vec::new();
    for (filename, value_sha) in expected {
        let index = config
            .patches_gff
            .iter()
            .position(|patch| patch.base.sourcefile == filename)
            .context("Original Companion GFF patch absent")?;
        let patch = &config.patches_gff[index];
        ensure!(
            patch.modifiers.len() == 2,
            "Companion GFF operations changed"
        );
        let matching: Vec<_> = patch
            .modifiers
            .iter()
            .enumerate()
            .filter_map(|(position, modifier)| match modifier {
                ModifyGFF::ModifyField(field)
                    if field.path == GffPath::new("DescIdentified(lang-1)") =>
                {
                    Some((position, field))
                }
                _ => None,
            })
            .collect();
        ensure!(matching.len() == 1, "Companion selector count changed");
        let (position, field) = matching[0];
        ensure!(
            field.identifier == filename && field.path_token_id.is_none() && !field.relative_path,
            "Companion selector context changed"
        );
        let FieldValueKind::Raw { raw_value, .. } = &field.value else {
            anyhow::bail!("Companion selector value representation changed");
        };
        ensure!(
            format!("{:x}", Sha256::digest(raw_value.as_bytes())) == value_sha,
            "Companion selector source value changed"
        );
        positions.push((index, position));
    }
    for (index, position) in positions {
        let ModifyGFF::ModifyField(field) = &mut config.patches_gff[index].modifiers[position]
        else {
            anyhow::bail!("Companion selector operation changed");
        };
        // Empty delta preserves StrRef and every substring. A genuinely missing
        // base field still fails normally. Never infer English from the sentinel.
        field.path = GffPath::new("DescIdentified");
        field.value = FieldValueKind::constant(GffValue::LocDelta(LocalizedStringDelta::new(None)));
    }
    ensure!(
        config.install_list.len() + config.patches_gff.len() == FILE_OPERATIONS
            && config.clone().patch_count() == FLATTENED_OPERATIONS,
        "Companion file or flattened field-operation count changed after selector adaptation"
    );
    installer.log.add_note(
        "Original Companion negative-language-selector compatibility v1: retained two no-op localized-field operations",
    );
    Ok(())
}
