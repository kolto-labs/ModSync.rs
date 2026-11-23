//! Source-specific classification of authored INI sections unreachable from any
//! active patch list. This never permits a warning, nor a generic orphan note.
use super::{Entry, field};
use anyhow::{Context, Result, ensure};
use odypatcher::patcher::ini::ConfigParser;
use std::collections::BTreeSet;

const PREFIX: &str = "There are some orphaned ini sections found in the changes:\n";

/// Win32 enumeration plus first-value lookup differs from a last-value map.
/// Unknown duplicate list entries need a source-specific traversal witness.
pub(super) fn validate_legacy_lists(text: &str) -> Result<()> {
    let mut active = None;
    let mut seen = BTreeSet::new();
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.split_once(']').map(|(section, _)| section))
        {
            let section = section.trim().to_ascii_lowercase();
            active = [
                "installlist",
                "gfflist",
                "2dalist",
                "tlklist",
                "compilelist",
                "ssflist",
            ]
            .contains(&section.as_str())
            .then_some(section);
        } else if let Some(section) = &active
            && let Some((key, _)) = line.split_once('=')
        {
            ensure!(
                seen.insert((
                    section.clone(),
                    crate::unicode_case::ordinal_key(key.trim())
                )),
                "Original legacy INI duplicate list key requires a certified traversal: [{section}] {}",
                key.trim()
            );
        }
    }
    Ok(())
}
const CHILDREN: [(&str, &str, &str); 14] = [
    ("Appearance", "Byte", "Value"),
    ("Description", "ExoLocString", "StrRef"),
    ("HasMapNote", "Byte", "Value"),
    ("LinkedTo", "ExoString", "Value"),
    ("LocalizedName", "ExoLocString", "StrRef"),
    ("MapNote", "ExoLocString", "StrRef"),
    ("MapNoteEnabled", "Byte", "Value"),
    ("Tag", "ExoString", "Value"),
    ("TemplateResRef", "ResRef", "Value"),
    ("XPosition", "Float", "Value"),
    ("XOrientation", "Float", "Value"),
    ("YOrientation", "Float", "Value"),
    ("ZPosition", "Float", "Value"),
    ("YPosition", "Float", "Value"),
];

/// The English standalone Enclave source contains exactly eight inactive
/// waypoint Struct roots and fourteen static AddField children per root. Its
/// empty GFFList makes that entire closure inert; active install groups remain.
pub(super) fn classify(entry: &Entry, parser: &ConfigParser, message: &str) -> Result<bool> {
    let Some(names) = message.strip_prefix(PREFIX) else {
        return Ok(false);
    };
    let row = &entry.row;
    if row.guid == "07416fb0-1f9b-88ba-952f-4029575ad036"
        && row.game == "k1"
        && row.ini.sha256 == "5c3f46fd3403c6671e232a3ca8486455d524e73a3e12597fb34b33632ce057f9"
    {
        return classify_unreachable_graph(parser, names);
    }
    if row.game == "k1"
        && ((row.guid == "8e70428e-2dab-8972-bf4c-6900d4ed8c26"
            && row.archive.sha256
                == "9e9462731ae2247126be75f08819e5b6d66d7aa50b1a4e5fcc44bcaf10cfd032"
            && matches!(
                (row.ini.path.as_str(), row.ini.sha256.as_str()),
                (
                    "tslpatchdata/changes.ini",
                    "430735565ccc98fe90a85d1474dcdacef070ad50fcbfaaa7a25676449a0311c5"
                ) | (
                    "tslpatchdata/CP.ini",
                    "296b99041edfda635ef54d962a1ee1734129873a20327c3a2739d0dfb9027033"
                )
            ))
            || (row.guid == "3f5cb051-3598-8fb9-abb0-b50ac47d269d"
                && row.archive.sha256
                    == "5c1c3e2a23de8a96f7bdf275d0326a8ed79aac786d8e5f096c92e9b799a7bbf9"
                && row.ini.path == "tslpatchdata/changes.ini"
                && row.ini.sha256
                    == "99fd073e161921b9ece05dd4975bcfbec213cbb8e85004c0b5f246dc5b232bec"))
    {
        return classify_unreachable_graph(parser, names);
    }
    if row.guid == "581433cd-4c71-848a-92f3-03064c7eae94" {
        return classify_swoop(entry, parser, names);
    }
    if row.guid == "e6488994-327d-825c-baed-6f869dad8ac2" {
        return classify_sunry(entry, parser, names);
    }
    if row.guid != "403daebc-dd7f-892c-ac37-823cf1ae6600"
        || row.game != "k2"
        || row.archive.sha256 != "ee329c08a30fd9b0e66766e51d841127f477bc8d9acfdc5be94e10fa4933db70"
        || row.ini.sha256 != "36766175e0c5cb99c8f9f5846a0eb4ebe6b7c76f9fd05fad8e2dcea672d786ee"
        || row.ini.path != "ExtendedEnclave_English/tslpatchdata/rcmchanges.ini"
        || !row.namespace.as_ref().is_some_and(|n| {
            n.index == 0
                && n.id == "TSLRCM"
                && n.ini_name == "rcmchanges.ini"
                && n.data_path.is_empty()
        })
    {
        return Ok(false);
    }
    for list in ["TLKList", "2DAList", "GFFList", "CompileList", "SSFList"] {
        ensure!(
            parser.section(list).is_some_and(|s| s.is_empty()),
            "Enclave inactive closure is referenced by an active patch list"
        );
    }
    let mut closure = BTreeSet::new();
    for index in 0..8 {
        let root = format!("gff_650dan_WaypointList_{}_0", index + 36);
        let section = parser
            .section(&root)
            .context("Enclave inactive root absent")?;
        ensure!(
            section.len() == 18
                && field(parser, &root, "FieldType") == Some("Struct")
                && field(parser, &root, "Path") == Some("WaypointList")
                && field(parser, &root, "Label") == Some("")
                && field(parser, &root, "TypeId") == Some("5"),
            "Enclave inactive root shape differs"
        );
        closure.insert(root.clone());
        for (ordinal, (label, kind, value_key)) in CHILDREN.iter().enumerate() {
            let child = format!("gff_650dan_{label}_{index}");
            ensure!(
                field(parser, &root, &format!("AddField{ordinal}")) == Some(child.as_str()),
                "Enclave inactive AddField edge differs"
            );
            let section = parser
                .section(&child)
                .context("Enclave inactive child absent")?;
            ensure!(
                section.len() == 3
                    && field(parser, &child, "FieldType") == Some(*kind)
                    && field(parser, &child, "Label") == Some(*label)
                    && field(parser, &child, value_key).is_some(),
                "Enclave inactive child shape differs"
            );
            closure.insert(child);
        }
    }
    let logged: Vec<_> = names.lines().collect();
    ensure!(
        closure.len() == 120
            && logged.len() == closure.len()
            && logged.iter().copied().collect::<BTreeSet<_>>()
                == closure.iter().map(String::as_str).collect(),
        "Enclave orphan diagnostic is not the exact inactive closure"
    );
    let mut complete = closure;
    complete.extend(
        [
            "Settings",
            "TLKList",
            "InstallList",
            "2DAList",
            "GFFList",
            "CompileList",
            "SSFList",
        ]
        .into_iter()
        .map(String::from),
    );
    let install = parser
        .section("InstallList")
        .context("Enclave InstallList absent")?;
    ensure!(
        install.len() == 11,
        "Enclave active install group count differs"
    );
    for index in 0..11 {
        let group = format!("install_folder{index}");
        ensure!(
            field(parser, "InstallList", &group).is_some()
                && parser.section(&group).is_some_and(|s| !s.is_empty()),
            "Enclave active install group absent"
        );
        complete.insert(group);
    }
    ensure!(
        parser.sections().collect::<BTreeSet<_>>() == complete.iter().map(String::as_str).collect(),
        "Enclave INI has an unclassified active or inactive section"
    );
    Ok(true)
}

// Sunry's active m26ae.git AddField roots use the _0 variants. Three older
// utility/mod templates and five unused _1 variants remain in the authored INI.
fn classify_sunry(entry: &Entry, parser: &ConfigParser, names: &str) -> Result<bool> {
    let row = &entry.row;
    if row.game != "k1"
        || row.archive.sha256 != "91e546732e8c96584449c6a0f0823d303fbbba04a147ba926445f331b0d860ec"
        || row.ini.sha256 != "37477fc1c44a35b8bfc9b1ebd1f08356d3a3d0442d63c0c7b1f3dea87036daa4"
        || row.ini.path != "SMRE/tslpatchdata/changes.ini"
        || row.namespace.is_some()
    {
        return Ok(false);
    }
    for list in ["TLKList", "2DAList", "CompileList", "SSFList"] {
        ensure!(
            parser.section(list).is_some_and(|s| s.is_empty()),
            "Sunry inactive closure has an unexpected active patch list"
        );
    }
    ensure!(
        parser.section("GFFList").is_some_and(|s| s.len() == 1)
            && field(parser, "GFFList", "File0") == Some("m26ae.git"),
        "Sunry active GFF root differs"
    );
    let mut roots = vec![
        (
            "gff_sa_ope_doo_PropertiesList_0_0".to_owned(),
            "PropertiesList",
            "0",
            7,
        ),
        (
            "gff_sa_ori_arm_PropertiesList_0_0".to_owned(),
            "PropertiesList",
            "0",
            7,
        ),
        (
            "gff_m01aa_Placeable List_60_0".to_owned(),
            "Placeable List",
            "9",
            5,
        ),
        (
            "gff_m26ae_CameraList_19_1".to_owned(),
            "CameraList",
            "14",
            7,
        ),
    ];
    roots.extend((43..47).map(|i| {
        (
            format!("gff_m26ae_Placeable List_{i}_1"),
            "Placeable List",
            "9",
            5,
        )
    }));
    let mut closure = BTreeSet::new();
    for (root, path, type_id, children) in roots {
        let section = parser
            .section(&root)
            .context("Sunry inactive root absent")?;
        ensure!(
            section.len() == children + 4
                && field(parser, &root, "FieldType") == Some("Struct")
                && field(parser, &root, "Path") == Some(path)
                && field(parser, &root, "Label") == Some("")
                && field(parser, &root, "TypeId") == Some(type_id),
            "Sunry inactive root shape differs"
        );
        ensure!(closure.insert(root.clone()), "Sunry inactive root repeated");
        for ordinal in 0..children {
            let child = field(parser, &root, &format!("AddField{ordinal}"))
                .context("Sunry inactive AddField absent")?;
            let section = parser
                .section(child)
                .context("Sunry inactive child absent")?;
            ensure!(
                child.starts_with("gff_")
                    && section.len() == 3
                    && matches!(
                        field(parser, child, "FieldType"),
                        Some(
                            "Float"
                                | "Byte"
                                | "Word"
                                | "ResRef"
                                | "Position"
                                | "Int"
                                | "Orientation"
                        )
                    )
                    && field(parser, child, "Label").is_some_and(|label| !label.is_empty())
                    && field(parser, child, "Value").is_some()
                    && closure.insert(child.to_owned()),
                "Sunry inactive child shape or identity differs"
            );
        }
    }
    let logged: Vec<_> = names.lines().collect();
    ensure!(
        closure.len() == 54
            && logged.len() == 54
            && logged.iter().copied().collect::<BTreeSet<_>>()
                == closure.iter().map(String::as_str).collect(),
        "Sunry orphan diagnostic is not the exact inactive closure"
    );
    let mut active: BTreeSet<String> = [
        "Settings",
        "TLKList",
        "InstallList",
        "2DAList",
        "GFFList",
        "CompileList",
        "SSFList",
        "install_folder0",
        "install_folder1",
        "m26ae.git",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    ensure!(
        parser.section("InstallList").is_some_and(|s| s.len() == 2)
            && field(parser, "InstallList", "install_folder0") == Some("Modules")
            && field(parser, "InstallList", "install_folder1") == Some(r"Modules\manm26ae.MOD"),
        "Sunry active install groups differ"
    );
    let mut pending = vec!["m26ae.git".to_owned()];
    while let Some(name) = pending.pop() {
        let section = parser
            .section(&name)
            .context("Sunry active GFF section absent")?;
        for (key, value) in section.items() {
            if key.starts_with("AddField") {
                let child = value
                    .as_deref()
                    .context("Sunry active AddField is dynamic")?;
                ensure!(
                    !closure.contains(child),
                    "Sunry classified inactive section is reachable"
                );
                if active.insert(child.to_owned()) {
                    ensure!(active.len() <= 96, "Sunry active closure exceeds bound");
                    pending.push(child.to_owned());
                }
            }
        }
    }
    ensure!(
        active.len() == 42 && closure.is_disjoint(&active),
        "Sunry active and inactive sections differ"
    );
    active.extend(closure);
    ensure!(
        parser.sections().collect::<BTreeSet<_>>() == active.iter().map(String::as_str).collect(),
        "Sunry INI has an unclassified section"
    );
    Ok(true)
}

// The original TSLPatcher enumerates both File5 occurrences but reads the
// first value each time. Only after that exact source-specific translation does
// manm26ab_module.ifo become inactive. The extra store Struct is also unused.
fn classify_swoop(entry: &Entry, parser: &ConfigParser, names: &str) -> Result<bool> {
    let row = &entry.row;
    if row.game != "k1"
        || row.archive.sha256 != "f6111e02c24ffed221e14dec51aa2d50dfc4e0c614550681456aed1dba57a2bb"
        || row.ini.sha256 != "a1cf26b1e96af3ab2e8ff2e04a8c8b548cbe3d3c0716dbd78f80681eb250d4ed"
        || row.ini.path != "TSLPatchdata/changes.ini"
        || row.namespace.is_some()
    {
        return Ok(false);
    }
    for list in ["2DAList", "CompileList", "SSFList"] {
        ensure!(
            parser.section(list).is_some_and(|s| s.is_empty()),
            "Swoop inactive classification has an unexpected patch list"
        );
    }
    let root = "manm26ab_module.ifo";
    ensure!(
        parser.section(root).is_some_and(|s| s.len() == 3)
            && field(parser, root, "!Destination") == Some(r"modules\manm26ab.mod")
            && field(parser, root, "!Filename") == Some("module.ifo")
            && field(parser, root, "Mod_OnAcquirItem") == Some("acq_swpbmpupg"),
        "Swoop original-unselected File5 section differs"
    );
    let root = "gff_k_man_yortal_31_0";
    ensure!(
        parser.section(root).is_some_and(|s| s.len() == 7)
            && field(parser, root, "FieldType") == Some("Struct")
            && field(parser, root, "Path") == Some("ItemList")
            && field(parser, root, "Label") == Some("")
            && field(parser, root, "TypeId") == Some("27")
            && field(parser, root, "AddField0") == Some("gff_man_yortal_InventoryRes_0")
            && field(parser, root, "AddField1") == Some("gff_man_yortal_Repos_PosX_0")
            && field(parser, root, "AddField2") == Some("gff_man_yortal_Repos_PosY_0"),
        "Swoop unused store template differs"
    );
    let inactive = BTreeSet::from(["manm26ab_module.ifo", "gff_k_man_yortal_31_0"]);
    let logged: Vec<_> = names.lines().collect();
    ensure!(
        logged.len() == 2 && logged.into_iter().collect::<BTreeSet<_>>() == inactive,
        "Swoop orphan note is not the exact inactive pair"
    );
    let gff = [
        "k_ptat_yukastore.utm",
        "man_yortal.utm",
        "man_yortalk.utm",
        "swp_speedup.uti",
        "swp_bumpup.uti",
        "tat_m17ac_module.ifo",
        "tat_m17ac_module.ifo",
    ];
    ensure!(
        parser
            .section("GFFList")
            .is_some_and(|s| s.len() == gff.len()),
        "Swoop active GFF selection count differs"
    );
    let mut active: BTreeSet<String> = [
        "Settings",
        "TLKList",
        "InstallList",
        "2DAList",
        "GFFList",
        "CompileList",
        "SSFList",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    for (index, name) in gff.iter().enumerate() {
        ensure!(
            field(parser, "GFFList", &format!("File{index}")) == Some(*name),
            "Swoop active GFF selection differs from original first-value iterations"
        );
        active.insert((*name).into());
    }
    ensure!(
        parser.section("InstallList").is_some_and(|s| s.len() == 6),
        "Swoop active install groups differ"
    );
    for index in 0..6 {
        let name = format!("install_folder{index}");
        ensure!(
            field(parser, "InstallList", &name).is_some()
                && parser.section(&name).is_some_and(|s| !s.is_empty()),
            "Swoop active install group absent"
        );
        active.insert(name);
    }
    let mut pending: Vec<_> = gff.into_iter().map(String::from).collect();
    while let Some(name) = pending.pop() {
        let section = parser
            .section(&name)
            .context("Swoop active GFF section absent")?;
        for (key, value) in section.items() {
            if key.starts_with("AddField") {
                let child = value
                    .as_deref()
                    .context("Swoop active AddField is dynamic")?;
                ensure!(
                    !inactive.contains(child),
                    "Swoop inactive section is reachable"
                );
                if active.insert(child.to_owned()) {
                    ensure!(
                        active.len() <= 29,
                        "Swoop active section closure exceeds bound"
                    );
                    pending.push(child.to_owned());
                }
            }
        }
    }
    ensure!(
        active.len() == 27 && inactive.iter().all(|name| !active.contains(*name)),
        "Swoop active/inactive section count or separation differs"
    );
    active.extend(inactive.into_iter().map(String::from));
    ensure!(
        parser.sections().collect::<BTreeSet<_>>() == active.iter().map(String::as_str).collect(),
        "Swoop INI has an unclassified section"
    );
    Ok(true)
}

/// Preserve the exact original TSLPatcher duplicate-key behavior for this one
/// compiled source. The packaged 1a267454... installer is byte-identical to the
/// original Wine receipt proving first-value lookup for every repeated key.
/// Source byte checks happen before this private, explicitly derived translation.
pub(super) fn effective_swoop_ini(owner_guid: &str, raw: &str) -> Result<Option<String>> {
    if owner_guid != "581433cd-4c71-848a-92f3-03064c7eae94" {
        return Ok(None);
    }
    ensure!(
        super::sha(raw.as_bytes())
            == "a1cf26b1e96af3ab2e8ff2e04a8c8b548cbe3d3c0716dbd78f80681eb250d4ed",
        "Swoop original INI bytes differ before duplicate-key translation"
    );
    let first = "File5=tat_m17ac_module.ifo";
    let duplicate = "File5=manm26ab_module.ifo";
    ensure!(
        raw.matches(first).count() == 1
            && raw.matches(duplicate).count() == 1
            && raw.find(first) < raw.find(duplicate),
        "Swoop original duplicate-key evidence differs"
    );
    Ok(Some(raw.replacen(
        duplicate,
        "File6=tat_m17ac_module.ifo",
        1,
    )))
}

/// Follow every section-valued key and value from every active list. This
/// over-approximates active traversal, so only a disconnected closed subgraph
/// can be classified. Callers provide an exact owner and authenticated INI pin.
fn classify_unreachable_graph(parser: &ConfigParser, names: &str) -> Result<bool> {
    let inactive = unreachable_sections(parser)?;
    let logged: Vec<_> = names.lines().collect();
    ensure!(
        !inactive.is_empty()
            && logged.len() == inactive.len()
            && logged.iter().copied().collect::<BTreeSet<_>>() == inactive,
        "Pinned orphan diagnostic differs from unreachable section closure"
    );
    Ok(true)
}

/// The authenticated Korriban source leaves two Yuthura templates disconnected.
/// One contains an authored kor53 typo. Exclude that proved inert pair only from
/// conservative graph analysis. The original INI and native patch model stay intact.
pub(super) fn effective_korriban_graph(
    owner_guid: &str,
    raw: &str,
    parser: &ConfigParser,
) -> Result<Option<String>> {
    if owner_guid != "8e70428e-2dab-8972-bf4c-6900d4ed8c26"
        || ![
            "430735565ccc98fe90a85d1474dcdacef070ad50fcbfaaa7a25676449a0311c5",
            "296b99041edfda635ef54d962a1ee1734129873a20327c3a2739d0dfb9027033",
        ]
        .contains(&super::sha(raw.as_bytes()).as_str())
    {
        return Ok(None);
    }
    let inactive = unreachable_sections(parser)?;
    ensure!(
        inactive == BTreeSet::from(["gff_kor35_yuthura_FeatList_0_0", "gff_kor35_yuthura_Feat_0",]),
        "Korriban malformed templates differ from the exact inactive pair"
    );
    let mut include = true;
    let mut result = String::new();
    for line in raw.split_inclusive('\n') {
        if let Some(section) = line
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
        {
            include = !inactive.contains(section);
        }
        if include {
            result.push_str(line);
        }
    }
    Ok(Some(result))
}

fn unreachable_sections(parser: &ConfigParser) -> Result<BTreeSet<&str>> {
    let sections: BTreeSet<_> = parser.sections().collect();
    let standard = [
        "Settings",
        "TLKList",
        "InstallList",
        "2DAList",
        "GFFList",
        "CompileList",
        "SSFList",
        "HACKList",
    ];
    let mut active = BTreeSet::new();
    let mut pending: Vec<_> = standard
        .into_iter()
        .filter(|name| sections.contains(name))
        .collect();
    while let Some(name) = pending.pop() {
        if !active.insert(name) {
            continue;
        }
        let section = parser
            .section(name)
            .context("Pinned inactive graph section absent")?;
        for (key, value) in section.items() {
            for target in std::iter::once(key.as_str()).chain(value.as_deref()) {
                if let Some(target) = sections
                    .iter()
                    .find(|section| section.eq_ignore_ascii_case(target))
                {
                    pending.push(*target);
                }
            }
        }
    }
    Ok(sections.difference(&active).copied().collect())
}

/// The original HQ guide deletes this source before the installer and explicitly
/// expects its one missing-source error. The section has no memory stores or
/// AddField children, so skipping its failed load has no later patch effects.
pub(super) fn effective_hq_ini(raw: &str) -> Result<String> {
    ensure!(
        super::sha(raw.as_bytes())
            == "17084e0e86b8de5d18b6118c8f33150dce9dd99fbfd4c7d659f3c4cd89ccaa43",
        "HQ original INI differs before authored source-omission interpretation"
    );
    let parser = ConfigParser::read_string(raw, odypatcher::patcher::ini::OptionXform::Identity)
        .map_err(|error| anyhow::anyhow!("Parse exact HQ source INI: {error}"))?;
    ensure!(
        field(&parser, "GFFList", "File6") == Some("keblastore.utm")
            && parser
                .section("keblastore.utm")
                .is_some_and(|section| section.len() == 2)
            && field(&parser, "keblastore.utm", "!ReplaceFile") == Some("1")
            && field(&parser, "keblastore.utm", "ItemList\\40\\InventoryRes")
                == Some("g_w_blstrrfl006")
            && raw.matches("keblastore.utm").count() == 2,
        "HQ omitted section is not its exact closed source-only patch"
    );
    let list = "File6=keblastore.utm\r\n";
    let section =
        "[keblastore.utm]\r\n!ReplaceFile=1\r\nItemList\\40\\InventoryRes=g_w_blstrrfl006\r\n";
    ensure!(
        raw.matches(list).count() == 1 && raw.matches(section).count() == 1,
        "HQ source omission byte boundaries differ"
    );
    let effective = raw.replacen(list, "", 1).replacen(section, "", 1);
    ensure!(
        super::sha(effective.as_bytes())
            == "dd7df5824257c4392f688f051e2962821766b0a7c0db73038c48b7f3249ef7cb",
        "HQ effective native INI identity differs"
    );
    Ok(effective)
}
