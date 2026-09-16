//! Local archive inference shared by guide conversion and desktop callers.
mod merge;
use crate::{Component, Guide, Instruction, archive};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GenerationResult {
    pub component_guid: String,
    pub component_name: String,
    pub success: bool,
    pub instructions_generated: usize,
    pub skip_reason: String,
}

/// Fill instruction-free components from matching top-level downloads. Reports retain
/// failures; an unsuccessful component is left unchanged and inputs are never deleted.
pub fn generate_local(guide: &mut Guide, source: &Path) -> Result<Vec<GenerationResult>> {
    ensure!(source.is_dir(), "Source path is not an existing directory");
    let mut archives = Vec::new();
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && matches!(
                entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("zip" | "rar" | "7z" | "exe")
            )
        {
            archives.push((entry.path(), entry.metadata()?.modified()?));
        }
    }
    archives.sort_by_key(|(path, _)| {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "zip" => 0,
            "rar" => 1,
            "7z" => 2,
            _ => 3,
        }
    });
    let mut results = Vec::new();
    for component in &mut guide.components {
        let mut report = GenerationResult {
            component_guid: component.guid.clone(),
            component_name: component.name.clone(),
            success: false,
            instructions_generated: 0,
            skip_reason: String::new(),
        };
        let outcome = (|| -> Result<()> {
            ensure!(
                component.instructions.is_empty(),
                "Component already has instructions"
            );
            ensure!(
                !component.resource_registry.is_empty(),
                "No mod links available"
            );
            let link = component.resource_registry.keys().next().unwrap();
            let term = search_term(link, &component.name)?;
            let normalized = normalize(&term);
            let matching = archives
                .iter()
                .filter_map(|(path, modified)| {
                    let name = normalize(path.file_stem()?.to_str()?);
                    let score = if name == normalized {
                        100
                    } else if name.contains(&normalized) {
                        50
                    } else if normalized.contains(&name) {
                        25
                    } else {
                        0
                    };
                    (score > 0).then_some((score, modified, path))
                })
                .reduce(|best, candidate| {
                    if (candidate.0, candidate.1) > (best.0, best.1) {
                        candidate
                    } else {
                        best
                    }
                })
                .with_context(|| format!("No matching archive found for '{term}'"))?;
            ensure!(
                generate_from_archive(
                    component,
                    matching.2,
                    &archive::Options::default(),
                    &AtomicBool::new(false)
                )?,
                "No supported game files or patcher data found"
            );
            component
                .metadata
                .insert("IsDownloaded".into(), true.into());
            Ok(())
        })();
        match outcome {
            Ok(()) => {
                report.success = true;
                report.instructions_generated = component.instructions.len();
            }
            Err(error) => report.skip_reason = format!("{error:#}"),
        }
        results.push(report);
    }
    Ok(results)
}

fn normalize(text: &str) -> String {
    text.to_lowercase().replace(['-', '_', ' '], "")
}

fn search_term(link: &str, fallback: &str) -> Result<String> {
    let mut term = if link.contains("://") {
        let url = reqwest::Url::parse(link)?;
        let segment = url
            .path()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("");
        match segment.split_once('-') {
            Some((number, name))
                if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) =>
            {
                name.to_owned()
            }
            _ => segment.to_owned(),
        }
    } else {
        link.rsplit(['/', '\\']).next().unwrap_or("").to_owned()
    };
    if let Some((stem, _)) = term.rsplit_once('.') {
        term = stem.to_owned();
    }
    if term.trim().is_empty() {
        term = fallback.to_owned();
    }
    Ok(term)
}

/// Infer or augment a component's instructions without writing into the mod/game trees.
/// The existing streaming archive decoder checks integrity and stages into scratch.
pub fn generate_from_archive(
    component: &mut Component,
    path: &Path,
    options: &archive::Options,
    cancelled: &AtomicBool,
) -> Result<bool> {
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Archive operation cancelled"
    );
    ensure!(path.is_file(), "Archive does not exist: {}", path.display());
    let filename = path
        .file_name()
        .and_then(|v| v.to_str())
        .context("Archive filename is not UTF-8")?;
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .context("Archive has no extension")?;
    let extracted = filename.replace(&format!(".{extension}"), "");
    let author = component.author.to_lowercase();
    let dedupe = component
        .name
        .eq_ignore_ascii_case("Remove Duplicate TGA/TPC")
        || (author.contains("flachzangen") && author.contains("th3w1zard1"))
        || component.resource_registry.keys().any(|link| {
            let link = link.to_lowercase();
            link.contains("nexusmods.com/kotor/mods/1384") || link.contains("pastebin.com/6wcn122s")
        });
    if dedupe {
        add_step(component, instruction("DelDuplicate", vec![], "", ".tpc"));
        return Ok(true);
    }
    let entries = match archive::unpack(path, path, options, cancelled) {
        Ok(entries) => entries,
        Err(_) if extension.eq_ignore_ascii_case("exe") && !cancelled.load(Ordering::Relaxed) => {
            // C# treats an EXE that cannot be opened as an archive as an
            // executable installer. Inference only records the step; the shared
            // engine still requires explicit external-program trust to run it.
            if add_step(
                component,
                instruction(
                    "Execute",
                    vec![format!("<<modDirectory>>\\{filename}")],
                    "",
                    "",
                ),
            ) {
                component
                    .metadata
                    .insert("InstallationMethod".into(), "Executable Installer".into());
            }
            return Ok(true);
        }
        Err(error) => return Err(error),
    };
    let paths: Vec<_> = entries
        .iter()
        .map(|e| e.path.to_string_lossy().replace('\\', "/"))
        .collect();
    let mut patch_data = false;
    let mut changes = false;
    let mut namespace_entry = None;
    let mut patcher_path = String::new();
    let mut executable = None;
    let mut flat = false;
    let mut folders = Vec::new();
    let mut loose = false;
    for (index, entry) in paths.iter().enumerate() {
        let parts: Vec<_> = entry.split('/').collect();
        let name = parts.last().unwrap();
        if let Some(patch_index) = parts
            .iter()
            .position(|s| s.eq_ignore_ascii_case("tslpatchdata"))
        {
            patch_data = true;
            if name.eq_ignore_ascii_case("namespaces.ini") {
                namespace_entry.get_or_insert(index);
                patcher_path = parts[..patch_index].join("/");
            } else if name.eq_ignore_ascii_case("changes.ini") {
                changes = true;
                if patcher_path.is_empty() {
                    patcher_path = parts[..patch_index].join("/");
                }
            } else if name.to_ascii_lowercase().ends_with(".exe") && executable.is_none() {
                executable = Some(*name);
            }
        } else if game_file(entry) {
            loose = true;
            if parts.len() == 1 {
                flat = true;
            } else if !folders.contains(&parts[0]) {
                folders.push(parts[0]);
            }
        }
    }
    if !patch_data && !loose {
        return Ok(false);
    }
    // Stage the model too: namespace decode failures must not publish a half-generated guide.
    let mut generated = component.clone();
    add_step(
        &mut generated,
        instruction(
            "Extract",
            vec![format!("<<modDirectory>>\\{filename}")],
            "",
            "",
        ),
    );
    let patcher_root = if patcher_path.is_empty() {
        &extracted
    } else {
        &patcher_path
    };
    if let Some(index) = namespace_entry {
        let namespaces = parse_namespaces(&fs::read_to_string(entries[index].file.path())?);
        if let Some(values) = namespaces.get("namespaces") {
            let mut ids = Vec::new();
            for (index, (_, namespace)) in values.iter().enumerate() {
                if let Some(data) = namespaces.get(&namespace.to_lowercase()) {
                    let field = |name: &str| {
                        data.iter()
                            .find(|(k, _)| k == name)
                            .map(|(_, v)| v.as_str())
                    };
                    let name = field("name").unwrap_or(namespace);
                    let description = field("description").unwrap_or("");
                    let step = instruction(
                        "Patcher",
                        vec![format!(
                            "<<modDirectory>>\\{patcher_root}\\{namespace}\\{namespace}.exe"
                        )],
                        "<<gameDirectory>>",
                        &index.to_string(),
                    );
                    ids.push(add_option(&mut generated, name, description, step));
                }
            }
            add_choose(&mut generated, ids);
        }
        merge::consolidate(&mut generated);
    } else if changes {
        add_step(
            &mut generated,
            instruction(
                "Patcher",
                vec![format!(
                    "<<modDirectory>>\\{patcher_root}\\{}",
                    executable.unwrap_or("TSLPatcher.exe")
                )],
                "<<gameDirectory>>",
                "",
            ),
        );
    }
    let patch_top = patcher_path.split('/').next().unwrap_or("");
    let folders: Vec<_> = folders
        .into_iter()
        .filter(|f| {
            !f.eq_ignore_ascii_case("tslpatchdata")
                && (patcher_path.is_empty() || !f.eq_ignore_ascii_case(patch_top))
        })
        .collect();
    if loose {
        if folders.len() > 1 {
            let mut ids = Vec::new();
            for folder in folders {
                let source = format!("<<modDirectory>>\\{extracted}\\{folder}\\*");
                if merge::folder_covered(&generated, &source) {
                    continue;
                }
                let step = instruction("Move", vec![source], "<<gameDirectory>>\\Override", "");
                ids.push(add_option(
                    &mut generated,
                    folder,
                    &format!("Install files from {folder} folder"),
                    step,
                ));
            }
            add_choose(&mut generated, ids);
        } else if let Some(folder) = folders.first() {
            add_step(
                &mut generated,
                instruction(
                    "Move",
                    vec![format!("<<modDirectory>>\\{extracted}\\{folder}\\*")],
                    "<<gameDirectory>>\\Override",
                    "",
                ),
            );
        } else if flat {
            add_step(
                &mut generated,
                instruction(
                    "Move",
                    vec![format!("<<modDirectory>>\\{extracted}\\*")],
                    "<<gameDirectory>>\\Override",
                    "",
                ),
            );
        }
    }
    generated.metadata.insert(
        "InstallationMethod".into(),
        (match (patch_data, loose) {
            (true, true) => "Hybrid (TSLPatcher + Loose Files)",
            (true, false) => "TSLPatcher",
            _ => "Loose-File Mod",
        })
        .into(),
    );
    merge::consolidate(&mut generated);
    *component = generated;
    Ok(true)
}

fn add_step(component: &mut Component, step: Instruction) -> bool {
    if (step.action == "Move"
        && step
            .source
            .iter()
            .any(|s| merge::folder_covered(component, s)))
        || component
            .instructions
            .iter()
            .any(|existing| merge::equivalent(existing, &step))
    {
        return false;
    }
    component.instructions.push(step);
    true
}

fn instruction(
    action: &str,
    source: Vec<String>,
    destination: &str,
    arguments: &str,
) -> Instruction {
    Instruction {
        guid: String::new(),
        parent: String::new(),
        action: action.into(),
        source,
        destination: destination.into(),
        arguments: arguments.into(),
        overwrite: true,
        dependencies: vec![],
        restrictions: vec![],
    }
}

fn add_option(
    component: &mut Component,
    name: &str,
    description: &str,
    step: Instruction,
) -> String {
    let existing = component
        .options
        .iter()
        .find(|o| o.instructions.len() == 1 && merge::equivalent(&o.instructions[0], &step));
    if let Some(existing) = existing {
        existing.guid.clone()
    } else {
        let id = crate::markdown::generated_id(
            &format!("{}:archive:{name}", component.guid),
            component.options.len(),
        );
        component.options.push(Component {
            guid: id.clone(),
            name: name.into(),
            description: description.into(),
            instructions: vec![step],
            ..Default::default()
        });
        id
    }
}

fn add_choose(component: &mut Component, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    if let Some(choose) = component
        .instructions
        .iter_mut()
        .find(|i| i.action == "Choose")
    {
        for id in ids {
            if !choose.source.iter().any(|s| s.eq_ignore_ascii_case(&id)) {
                choose.source.push(id);
            }
        }
    } else {
        component
            .instructions
            .push(instruction("Choose", ids, "", ""));
    }
}

fn game_file(path: &str) -> bool {
    let Some((_, extension)) = path.rsplit_once('.') else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "2da"
            | "are"
            | "bik"
            | "dds"
            | "dlg"
            | "erf"
            | "git"
            | "gui"
            | "ifo"
            | "mod"
            | "jrl"
            | "lip"
            | "lyt"
            | "mdl"
            | "mdx"
            | "ncs"
            | "pth"
            | "rim"
            | "ssf"
            | "tga"
            | "tlk"
            | "txi"
            | "tpc"
            | "utc"
            | "utd"
            | "ute"
            | "uti"
            | "utm"
            | "utp"
            | "uts"
            | "utw"
            | "vis"
            | "wav"
    )
}

fn parse_namespaces(text: &str) -> BTreeMap<String, Vec<(String, String)>> {
    let mut sections = BTreeMap::<String, Vec<(String, String)>>::new();
    let mut section = None;
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].to_lowercase();
            sections.insert(name.clone(), Vec::new());
            section = Some(name);
        } else if let Some(section) = &section {
            let parts: Vec<_> = line.split('=').collect();
            if parts.len() != 2 {
                continue;
            }
            let key = parts[0].trim().to_lowercase();
            let value = parts[1].trim().to_owned();
            let values = sections.get_mut(section).unwrap();
            if let Some((_, old)) = values.iter_mut().find(|(k, _)| k == &key) {
                *old = value;
            } else {
                values.push((key, value));
            }
        }
    }
    sections
}
