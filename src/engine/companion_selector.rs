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
    ensure!(
        config.patches_gff.len() == expected_files.len()
            && config
                .patches_gff
                .iter()
                .zip(expected_files)
                .all(|(patch, expected)| patch.base.sourcefile == expected
                    && patch.base.saveas == expected
                    && patch.base.destination == "Override")
            && config.clone().patch_count() == 19,
        "Original Companion configuration shape changed"
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
        config.clone().patch_count() == 19,
        "Companion operation count changed"
    );
    installer.log.add_note(
        "Original Companion negative-language-selector compatibility v1: retained two no-op localized-field operations",
    );
    Ok(())
}
