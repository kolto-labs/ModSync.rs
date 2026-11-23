//! Original definitions are issued by the existing typed authorities. Every
//! lowered instruction is then reconstructed here without native INI admission.
use super::super::source_plan_namespace::original_plan_protocol as protocol;
use super::*;
use anyhow::Context as _;
use protocol::Effect;

pub(super) struct Context {
    pub game: String,
    pub guide_sha: String,
    pub plan_sha: String,
    pub original: Component,
    pub step: Value,
    pub authority: Value,
    pub extracts: Vec<(String, String, String)>,
    effects: Vec<Effect>,
    flavor: Flavor,
}
#[derive(Clone, Copy)]
enum Flavor {
    Program,
    Namespace,
    Patcher,
    Recipe,
}
pub(super) struct Run {
    pub slot: String,
    pub source_deletions: BTreeSet<String>,
    pub executable: String,
    pub executable_sha: String,
    pub namespace: Option<usize>,
    pub ordinal: usize,
}
struct Original {
    game: String,
    guide_sha: String,
    plan_sha: String,
    owner: Component,
    step: Value,
    hashes: BTreeMap<String, String>,
}
static ORIGINALS: OnceLock<std::result::Result<BTreeMap<String, Original>, String>> =
    OnceLock::new();
fn originals() -> Result<&'static BTreeMap<String, Original>> {
    ORIGINALS
        .get_or_init(|| load_originals().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
}
fn load_originals() -> Result<BTreeMap<String, Original>> {
    let sources = [
        (
            "k1",
            include_bytes!("../source_plan_k1.md").as_slice(),
            "44067788c6cb7a83b38d468817f273ce5cc901cd5e000c0f9ff65f8af15fc6ed",
            include_bytes!("../../../tools/modsync-ci/fixtures/ci/k1-manual-plan.json").as_slice(),
            "1b402ba085625c4863795069eaacaef0f53e8ea7179e56a5bd150d88eb9c0a74",
            197,
        ),
        (
            "k2",
            include_bytes!("../source_plan_k2.md").as_slice(),
            "f442bdf574031623ed577609730d887069b9bc8e38e8e2f38ecc43a7e7487d5f",
            include_bytes!("../../../tools/modsync-ci/fixtures/ci/k2-manual-plan.json").as_slice(),
            "0add8c1aeb7cdfad3df4b21761e135c150aa7832cf3fc0edabe01a91245a9258",
            151,
        ),
    ];
    let mut result = BTreeMap::new();
    for (game, guide, guide_sha, plan, plan_sha, count) in sources {
        ensure!(
            sha(guide) == guide_sha && sha(plan) == plan_sha,
            "External original guide/plan pin differs"
        );
        let mut raw = Guide::parse(std::str::from_utf8(guide)?, "md")?;
        draft::generate(&mut raw);
        let canonical = Guide::parse_preserving_selection(&serde_json::to_string(&raw)?, "json")?;
        ensure!(
            canonical.components.len() == count && raw.components.len() == count,
            "External complete original inventory differs"
        );
        let plan: Value = serde_json::from_slice(plan)?;
        ensure!(
            plan["guide_sha256"] == guide_sha,
            "External original plan belongs to another guide"
        );
        let steps = plan["steps"].as_array().context("Original steps absent")?;
        let hashes: BTreeMap<String, String> =
            serde_json::from_value(plan["archive_sha256"].clone())?;
        ensure!(
            steps.len() <= count,
            "Original steps exceed owner inventory"
        );
        for (index, owner) in canonical.components.into_iter().enumerate() {
            let step = steps.get(index).cloned().unwrap_or(Value::Null);
            if !step.is_null() {
                let heading = owner
                    .metadata
                    .get("Heading")
                    .and_then(Value::as_str)
                    .unwrap_or(&owner.name);
                ensure!(
                    step["id"] == format!("step-{:03}", index + 1)
                        && step["source_heading"] == format!("### {heading}"),
                    "Original full guide ordinal/heading differs"
                );
            }
            ensure!(
                result
                    .insert(
                        owner.guid.clone(),
                        Original {
                            game: game.into(),
                            guide_sha: guide_sha.into(),
                            plan_sha: plan_sha.into(),
                            owner,
                            step,
                            hashes: hashes.clone()
                        }
                    )
                    .is_none(),
                "Original full GUID inventory has duplicates"
            );
        }
    }
    Ok(result)
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("Original {key} absent"))
}
fn token(value: &str, root: &str) -> Result<String> {
    let tail = value
        .strip_prefix(&format!("{{{root}}}/"))
        .context("Original rooted path differs")?;
    literal(tail)?;
    Ok(tail.into())
}
fn core(component: &Component) -> Result<Value> {
    let mut value = serde_json::to_value(component)?;
    let fields = value
        .as_object_mut()
        .context("Original owner object absent")?;
    for key in [
        "Instructions",
        "IsSelected",
        "DraftReview",
        "AuthoredRecipeBinding",
        "SourcePlanPatcherBinding",
        "SourcePlanNamespaceBinding",
    ] {
        fields.remove(key);
    }
    Ok(value)
}
pub(super) fn context(component: &Component) -> Result<Option<Context>> {
    let bindings = [
        "AuthoredRecipeBinding",
        "SourcePlanPatcherBinding",
        "SourcePlanNamespaceBinding",
    ];
    let claims: Vec<_> = bindings
        .into_iter()
        .filter(|key| component.metadata.contains_key(*key))
        .collect();
    if claims.is_empty() {
        return Ok(None);
    }
    ensure!(
        claims.len() == 1,
        "Conflicting original external source authorities"
    );
    let declared = match claims[0] {
        "AuthoredRecipeBinding" => {
            super::super::reviewed_manual::declared_original_definition(component)?
        }
        "SourcePlanPatcherBinding" => {
            super::super::source_plan_patcher::declared_original_definition(component)?
        }
        _ => super::super::source_plan_namespace::declared_original_definition(component)?,
    }
    .context("Original external source definition absent")?;
    let original = originals()?
        .get(&component.guid)
        .context("External source owner outside complete original GUID inventory")?;
    ensure!(
        serde_json::to_value(&declared)? == serde_json::to_value(&original.owner)?,
        "Typed source definition differs from immutable complete original guide"
    );
    ensure!(
        core(component)? == core(&original.owner)?,
        "External source complete owner context differs"
    );
    let binding = &component.metadata[claims[0]];
    let mut step = original.step.clone();
    let mut authority = json!({"Kind":claims[0]});
    let flavor;
    let effects;
    if claims[0] == "SourcePlanNamespaceBinding" && binding["Schema"] == 2 {
        flavor = Flavor::Program;
        // This field has already been compared with the private compiled source
        // by declared_original_definition, including closed revision/supplemental
        // identity. It is never interpreted before that typed comparison.
        step = binding["OriginalSourceChoice"]["SourceProgram"].clone();
        effects = if step["SourceKind"] == "reviewed-publisher-source-revision" {
            revision_effects(&step, original)?
        } else if step["source_kind"] == "authored-supplemental" {
            supplemental_effects(&step, original)?
        } else {
            ensure!(step == original.step, "Original ordered program differs");
            protocol::compile(&step, &original.hashes)?.effects
        };
        authority["ProgramAuthoritySha256"] = binding["AuthoritySha256"].clone();
    } else {
        let (chosen, namespace) = if claims[0] == "AuthoredRecipeBinding" {
            flavor = Flavor::Recipe;
            let recipe = &binding["Recipe"];
            ensure!(
                recipe["plan_step"] == original.step,
                "Recipe original plan differs"
            );
            let native = &recipe["native_patcher"];
            ensure!(!native.is_null(), "Recipe has no original native operation");
            authority["RecipeBookSha256"] = binding["DatabookSha256"].clone();
            (
                Some(
                    native["operation_index"]
                        .as_u64()
                        .context("Recipe native ordinal absent")? as usize,
                ),
                None,
            )
        } else if claims[0] == "SourcePlanNamespaceBinding" {
            flavor = Flavor::Namespace;
            let row = &binding["OriginalSourceChoice"];
            ensure!(
                row["original_step"] == original.step,
                "Namespace original plan differs"
            );
            authority["NamespaceAuthoritySha256"] = binding["AuthoritySha256"].clone();
            (None, row["namespace"]["index"].as_u64().map(|v| v as usize))
        } else {
            flavor = Flavor::Patcher;
            ensure!(
                binding["OriginalStep"] == original.step,
                "Patcher original plan differs"
            );
            authority["MappingSha256"] = binding["MappingSha256"].clone();
            (None, None)
        };
        effects = legacy_effects(&step, &original.hashes, chosen, namespace)?;
    }
    let extracts = effects
        .iter()
        .filter_map(|effect| {
            if let Effect::Extract {
                source,
                destination,
                sha256,
                ..
            } = effect
            {
                Some((destination.clone(), source.clone(), sha256.clone()))
            } else {
                None
            }
        })
        .map(|(destination, source, digest)| {
            Ok((
                token(&destination, "extract")?,
                token(&source, "archives")?,
                digest,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !extracts.is_empty() && extracts.len() <= 32,
        "Original source extraction count exceeds bound"
    );
    Ok(Some(Context {
        game: original.game.clone(),
        guide_sha: original.guide_sha.clone(),
        plan_sha: original.plan_sha.clone(),
        original: declared,
        step,
        authority,
        extracts,
        effects,
        flavor,
    }))
}
fn legacy_effects(
    step: &Value,
    hashes: &BTreeMap<String, String>,
    chosen: Option<usize>,
    namespace: Option<usize>,
) -> Result<Vec<Effect>> {
    let mut effects = Vec::new();
    for (ordinal, op) in step["operations"]
        .as_array()
        .context("Original operations absent")?
        .iter()
        .enumerate()
    {
        let effect = match string(op, "type")? {
            "extract" => Effect::Extract {
                ordinal,
                source: string(op, "source")?.into(),
                destination: string(op, "destination")?.into(),
                sha256: hashes
                    .get(string(op, "source")?)
                    .context("Original archive SHA absent")?
                    .clone(),
            },
            "delete" => Effect::Delete {
                ordinal,
                path: string(op, "path")?.into(),
                missing_ok: op["missing_ok"].as_bool().unwrap_or(false),
                if_any_exists: Vec::new(),
            },
            "copy" => Effect::Copy {
                ordinal,
                source: string(op, "source")?.into(),
                destination: string(op, "destination")?.into(),
                skip_existing: op["skip_existing"].as_bool().unwrap_or(false),
            },
            "patcher" | "gui_patcher" => {
                if op["type"] == "gui_patcher" {
                    ensure!(
                        protocol::legacy_gui_installer(string(op, "sha256")?),
                        "Original GUI patcher executable is outside the reviewed protocol"
                    );
                }
                if let Some(chosen) = chosen {
                    ensure!(
                        chosen == ordinal,
                        "Recipe selected another original operation"
                    );
                }
                Effect::NativeRun {
                    ordinal,
                    patcher: string(op, "patcher")?.into(),
                    sha256: string(op, "sha256")?.into(),
                    cwd: string(op, "cwd")?.into(),
                    namespace,
                }
            }
            other => bail!("Unsupported legacy original operation {other}"),
        };
        effects.push(effect);
    }
    ensure!(
        effects
            .iter()
            .filter(|e| matches!(e, Effect::NativeRun { .. }))
            .count()
            == 1,
        "Legacy original source needs exactly one selected native run"
    );
    Ok(effects)
}
fn revision_effects(step: &Value, original: &Original) -> Result<Vec<Effect>> {
    ensure!(
        step["OriginalSourceProgram"] == original.step,
        "Source revision original program differs"
    );
    let data = &step["SourceRevision"];
    ensure!(
        data["owner_guid"] == original.owner.guid && data["game"] == original.game,
        "Revision original owner differs"
    );
    let pin = &data["revision_archive"];
    let filename = string(pin, "filename")?;
    let digest = string(pin, "sha256")?;
    let (id, namespace) = match original.owner.guid.as_str() {
        "ed488679-bc04-892d-973b-107511862024" => {
            ensure!(digest == UNIFORM, "Uniform original current source differs");
            ("source-revision-k1-uniform-231ca858", Some(1))
        }
        "14d77038-49c0-882b-83a1-0a424efa0f60" => ("source-revision-k1-trask-1.0.2", None),
        _ => bail!("Original revision does not have an external patcher program"),
    };
    let slot = format!("{{extract}}/{id}/1");
    let installer = &data["installer"];
    let path = string(installer, "path")?;
    Ok(vec![
        Effect::Extract {
            ordinal: 0,
            source: format!("{{archives}}/{filename}"),
            destination: slot.clone(),
            sha256: digest.into(),
        },
        Effect::NativeRun {
            ordinal: 1,
            patcher: format!("{slot}/{path}"),
            sha256: string(installer, "sha256")?.into(),
            cwd: format!(
                "{slot}/{}",
                Path::new(path)
                    .parent()
                    .context("Revision installer parent absent")?
                    .to_string_lossy()
            ),
            namespace,
        },
    ])
}
fn supplemental_effects(step: &Value, original: &Original) -> Result<Vec<Effect>> {
    ensure!(
        step["source_owner_guid"] == original.owner.guid
            && step["source_guide_sha256"] == original.guide_sha,
        "Supplemental original owner differs"
    );
    let id = format!(
        "supplemental-{}-{}-{}",
        original.game,
        step["source_owner_index_zero_based"]
            .as_u64()
            .context("Supplemental owner index absent")?,
        string(step, "profile")?
    );
    let mut effects = Vec::new();
    let mut slots = BTreeMap::new();
    for (index, pin) in step["source_inputs"]
        .as_array()
        .context("Supplemental original inputs absent")?
        .iter()
        .enumerate()
    {
        if pin["active"] != true {
            continue;
        }
        let destination = format!("{{extract}}/{id}/{}", index + 1);
        slots.insert(index, destination.clone());
        effects.push(Effect::Extract {
            ordinal: effects.len(),
            source: format!("{{archives}}/{}", string(pin, "filename")?),
            destination,
            sha256: string(pin, "sha256")?.into(),
        });
    }
    for copy in step["selected_copies"]
        .as_array()
        .context("Supplemental copies absent")?
    {
        let slot = slots
            .get(
                &(copy["input"]
                    .as_u64()
                    .context("Supplemental input index absent")? as usize),
            )
            .context("Supplemental copy input inactive")?;
        effects.push(Effect::Copy {
            ordinal: effects.len(),
            source: format!("{slot}/{}", string(copy, "member")?),
            destination: format!("{{game}}/{}", string(copy, "destination")?),
            skip_existing: copy["skip_existing"].as_bool().unwrap_or(false),
        });
    }
    for run in step["native_runs"]
        .as_array()
        .context("Supplemental runs absent")?
    {
        let slot = slots
            .get(
                &(run["input"]
                    .as_u64()
                    .context("Supplemental native input index absent")? as usize),
            )
            .context("Supplemental native input inactive")?;
        effects.push(Effect::NativeRun {
            ordinal: effects.len(),
            patcher: format!("{slot}/{}", string(run, "patcher")?),
            sha256: string(run, "installer_sha256")?.into(),
            cwd: format!("{slot}/{}", string(run, "cwd")?),
            namespace: Some(
                run["namespace"]
                    .as_u64()
                    .context("Supplemental namespace absent")? as usize,
            ),
        });
    }
    Ok(effects)
}
fn extract_path<'a>(
    value: &str,
    slots: &'a BTreeMap<String, Slot>,
) -> Result<(&'a str, &'a Slot, String)> {
    let tail = token(value, "extract")?;
    let found: Vec<_> = slots
        .iter()
        .filter_map(|(key, slot)| {
            if tail == *key {
                Some((key.as_str(), slot, String::new()))
            } else {
                tail.strip_prefix(&format!("{key}/"))
                    .map(|relative| (key.as_str(), slot, relative.into()))
            }
        })
        .collect();
    ensure!(
        found.len() == 1,
        "Original source extraction reference is ambiguous"
    );
    found
        .into_iter()
        .next()
        .context("Original extraction slot absent")
}
fn lower(value: &str, slots: &BTreeMap<String, Slot>) -> Result<String> {
    if value == "{game}" {
        return Ok("<<kotorDirectory>>".into());
    }
    if value.starts_with("{game}/") {
        return Ok(format!("<<kotorDirectory>>/{}", token(value, "game")?));
    }
    if value.starts_with("{archives}/") {
        return Ok(format!("<<modDirectory>>/{}", token(value, "archives")?));
    }
    let (_, slot, tail) = extract_path(value, slots)?;
    Ok(if tail.is_empty() {
        slot.native.clone()
    } else {
        format!("{}/{tail}", slot.native)
    })
}
fn instruction(
    context: &Context,
    index: usize,
    action: &str,
    source: String,
    destination: String,
    arguments: String,
    overwrite: bool,
) -> Instruction {
    let prefix = match context.flavor {
        Flavor::Program | Flavor::Namespace => "source-selected-namespace-v1",
        Flavor::Patcher => "source-plan-native-v1",
        Flavor::Recipe => "reviewed-manual-file-effects",
    };
    Instruction {
        guid: crate::markdown::generated_id(&format!("{}:{prefix}", context.original.guid), index),
        parent: context.original.guid.clone(),
        action: action.into(),
        source: vec![source],
        destination,
        arguments,
        overwrite,
        dependencies: vec![],
        restrictions: vec![],
    }
}
pub(super) fn select_run(
    context: &Context,
    component: &Component,
    index: usize,
    slots: &BTreeMap<String, Slot>,
) -> Result<Run> {
    let mut expected = Vec::new();
    let mut runs = BTreeMap::new();
    let mut deleted: BTreeSet<(String, String)> = BTreeSet::new();
    let mut logs = BTreeSet::new();
    for effect in &context.effects {
        ensure!(
            effect.ordinal() < MAX_ENTRIES,
            "Original operation ordinal exceeds bound"
        );
        match effect {
            Effect::Extract {
                source,
                destination,
                ..
            } => {
                let (_, slot, _) = extract_path(destination, slots)?;
                expected.push(instruction(
                    context,
                    expected.len(),
                    "ExtractTo",
                    lower(source, slots)?,
                    slot.native.clone(),
                    String::new(),
                    false,
                ));
            }
            Effect::NativeRun {
                ordinal,
                patcher,
                sha256,
                namespace,
                ..
            } => {
                let (key, slot, path) = extract_path(patcher, slots)?;
                let installer = member(&slot.members, &path)?;
                ensure!(
                    installer.sha256 == *sha256,
                    "Original complete program executable differs"
                );
                let native_index = expected.len();
                let id_index = if matches!(context.flavor, Flavor::Recipe) {
                    *ordinal
                } else {
                    native_index
                };
                expected.push(instruction(
                    context,
                    id_index,
                    "Patcher",
                    lower(patcher, slots)?,
                    "<<kotorDirectory>>".into(),
                    namespace
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "changes.ini".into()),
                    false,
                ));
                runs.insert(
                    native_index,
                    Run {
                        slot: key.into(),
                        source_deletions: deleted
                            .iter()
                            .filter(|(native, _)| native == &slot.native)
                            .map(|(_, relative)| relative.clone())
                            .collect(),
                        executable: installer.path.clone(),
                        executable_sha: sha256.clone(),
                        namespace: *namespace,
                        ordinal: *ordinal,
                    },
                );
                let parent = Path::new(patcher)
                    .parent()
                    .context("Original executable parent absent")?
                    .to_string_lossy();
                for name in ["installlog.rtf", "installlog.txt"] {
                    logs.insert(format!("{parent}/{name}"));
                }
            }
            Effect::Delete {
                path,
                missing_ok,
                if_any_exists,
                ..
            } => {
                ensure!(
                    if_any_exists.is_empty(),
                    "Conditional original source deletion requires separate authority"
                );
                if path.starts_with("{extract}/") {
                    let (_, slot, relative) = extract_path(path, slots)?;
                    if !slot.members.contains_key(&relative) && logs.contains(path) {
                        continue;
                    }
                    ensure!(
                        !relative.is_empty()
                            && !slot.members.get(&relative).is_some_and(|m| m.directory)
                            && (slot.members.contains_key(&relative) || *missing_ok),
                        "Original complete deletion source differs"
                    );
                    deleted.insert((slot.native.clone(), relative));
                }
                expected.push(instruction(
                    context,
                    expected.len(),
                    "Delete",
                    lower(path, slots)?,
                    String::new(),
                    String::new(),
                    !*missing_ok,
                ));
            }
            Effect::Copy {
                source,
                destination,
                skip_existing,
                ..
            } => {
                let target = lower(destination, slots)?;
                if source.starts_with("{archives}/") {
                    expected.push(instruction(
                        context,
                        expected.len(),
                        "CopyFile",
                        lower(source, slots)?,
                        target,
                        String::new(),
                        !*skip_existing,
                    ));
                } else {
                    let (_, slot, relative) = extract_path(source, slots)?;
                    if slot.members.get(&relative).is_some_and(|m| !m.directory) {
                        ensure!(
                            !deleted.contains(&(slot.native.clone(), relative)),
                            "Original copy references deleted member"
                        );
                        expected.push(instruction(
                            context,
                            expected.len(),
                            "CopyFile",
                            lower(source, slots)?,
                            target,
                            String::new(),
                            !*skip_existing,
                        ));
                    } else {
                        ensure!(
                            relative.is_empty()
                                || slot.members.get(&relative).is_some_and(|m| m.directory),
                            "Original directory copy source absent"
                        );
                        let prefix = if relative.is_empty() {
                            String::new()
                        } else {
                            format!("{relative}/")
                        };
                        let start = expected.len();
                        for member in slot.members.values().filter(|m| {
                            m.path.starts_with(&prefix)
                                && !deleted.contains(&(slot.native.clone(), m.path.clone()))
                        }) {
                            if member.directory {
                                ensure!(
                                    slot.members.values().any(|m| !m.directory
                                        && m.path.starts_with(&format!("{}/", member.path))),
                                    "Original directory copy has unsupported empty directory"
                                );
                            } else {
                                expected.push(instruction(
                                    context,
                                    expected.len(),
                                    "CopyFile",
                                    format!("{}/{}", slot.native, member.path),
                                    format!(
                                        "{target}/{}",
                                        member
                                            .path
                                            .strip_prefix(&prefix)
                                            .context("Original copy prefix differs")?
                                    ),
                                    String::new(),
                                    !*skip_existing,
                                ));
                            }
                        }
                        ensure!(
                            expected.len() > start,
                            "Original complete directory copy has no files"
                        );
                    }
                }
            }
            Effect::Rename {
                source,
                destination,
                ..
            } => {
                ensure!(
                    Path::new(source).parent() == Path::new(destination).parent(),
                    "Original rename changes directories"
                );
                expected.push(instruction(
                    context,
                    expected.len(),
                    "Rename",
                    lower(source, slots)?,
                    Path::new(destination)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .context("Original rename target absent")?
                        .into(),
                    String::new(),
                    true,
                ));
            }
        }
        ensure!(
            expected.len() <= MAX_ENTRIES,
            "Original complete instruction count exceeds bound"
        );
    }
    ensure!(
        serde_json::to_value(&expected)? == serde_json::to_value(&component.instructions)?,
        "External entire instruction program differs from fresh immutable original program"
    );
    runs.remove(&index)
        .context("External selected index is not an original patcher operation")
}
