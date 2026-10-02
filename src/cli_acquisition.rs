//! Explicit acquisition prephase for selected-guide CLI processing. Downloaded
//! archives may persist after failure; the caller's guide changes only on success.
mod deadline;
use anyhow::{Context, Result, ensure};
use modsync::{
    Guide,
    acquisition::{Acquire, SkipReason},
    archive,
    download_hosts::{self, Provider},
    download_orchestrator as orchestration, p2p,
    peer_acquire::PeerAcquire,
    profiles::{SettingsFile, Store},
    selection,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
/// Caller-owned acquisition overrides. Deliberately not Debug: API keys are secrets.
pub(crate) struct Options {
    pub timeout: Option<std::time::Duration>,
    pub guide_path: Option<PathBuf>,
    pub nexus_api_key: Option<String>,
    pub workers: usize,
    pub fomod_choices: Option<PathBuf>,
    pub fomod_skip: bool,
    pub fomod_interactive: bool,
    pub fomod_non_interactive: bool,
    /// Verified phase prefix retained in the full guide; never acquired or rewritten.
    pub completed: Vec<String>,
    /// Conversion projects every resource owner into acquisition scope, then
    /// restores the authored top-level selection before publishing its guide.
    pub include_unselected_authoring: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            timeout: None,
            guide_path: None,
            nexus_api_key: None,
            workers: 4,
            fomod_choices: None,
            fomod_skip: false,
            fomod_interactive: false,
            fomod_non_interactive: true,
            completed: Vec::new(),
            include_unselected_authoring: false,
        }
    }
}
/// Shared caller policy: explicit key, stored key, then first nonblank environment
/// fallback. Keep None as the implicit settings-directory authority so legacy
/// lookup remains available. No settings/environment lookup for unrelated hosts.
pub(crate) fn resolve_nexus_key(
    explicit: Option<&str>,
    settings_dir: Option<&Path>,
    needs_nexus: bool,
) -> Result<Option<String>> {
    if explicit.is_some() {
        return crate::install_nexus_key(explicit);
    }
    if !needs_nexus {
        return Ok(None);
    }
    if let Some(key) = Store::from_settings_directory(settings_dir)?.nexus_api_key()?
        && !key.trim().is_empty()
    {
        return Ok(Some(key));
    }
    crate::install_nexus_key(None)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        return Err(crate::cli_session::Exit(130).into());
    }
    Ok(())
}
fn valid_skips(plan: &orchestration::AcquisitionPlan) -> Result<()> {
    ensure!(
        !plan.skipped.iter().any(|item| matches!(
            item.reason,
            SkipReason::InvalidUrl | SkipReason::UnsafeFilename
        )),
        "Acquisition contains invalid URLs or unsafe filenames; review the selected resources before processing"
    );
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn before_process(
    guide: &mut Guide,
    source: &Path,
    game: Option<&Path>,
    settings_dir: Option<&Path>,
    options: &archive::Options,
    cancelled: Arc<AtomicBool>,
    overrides: &Options,
) -> Result<()> {
    let timer = overrides
        .timeout
        .map(|duration| deadline::Deadline::start(duration, cancelled.clone()))
        .transpose()?;
    let mut candidate = guide.clone();
    let result = before_process_inner(
        &mut candidate,
        source,
        game,
        settings_dir,
        options,
        &cancelled,
        overrides,
    );
    let expired = timer.is_some_and(deadline::Deadline::finish);
    if expired {
        eprintln!("Acquisition reached its configured time limit; processing stopped");
    }
    check_cancelled(&cancelled)?;
    result?;
    *guide = candidate;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn before_process_inner(
    guide: &mut Guide,
    source: &Path,
    game: Option<&Path>,
    settings_dir: Option<&Path>,
    options: &archive::Options,
    cancelled: &AtomicBool,
    overrides: &Options,
) -> Result<()> {
    check_cancelled(cancelled)?;
    ensure!(
        overrides.workers > 0 && overrides.workers <= 64,
        "Acquisition worker count must be between 1 and 64"
    );
    ensure!(
        overrides
            .nexus_api_key
            .as_ref()
            .is_none_or(|key| !key.trim().is_empty()),
        "An explicit Nexus API key cannot be blank"
    );
    if let Some(game) = game {
        modsync::paths::Roots::new(source, &modsync::game::resolve_install_root(game))
            .context("Acquisition requires separate existing mod and game directories")?;
    }
    let mut candidate = guide.clone();
    let mut original_selection = std::collections::BTreeMap::new();
    if overrides.include_unselected_authoring {
        for component in &candidate.components {
            ensure!(
                !component.guid.is_empty()
                    && original_selection
                        .insert(
                            modsync::fomod::fold_case(&component.guid),
                            (component.guid.clone(), component.is_selected),
                        )
                        .is_none(),
                "Authoring acquisition requires unique nonempty component identities"
            );
        }
        for component in &mut candidate.components {
            if !component.resource_registry.is_empty() {
                component.is_selected = true;
            }
        }
    }
    let mut completed_definitions = std::collections::BTreeMap::new();
    for id in &overrides.completed {
        let component = guide
            .components
            .iter()
            .find(|component| component.guid.eq_ignore_ascii_case(id))
            .with_context(|| format!("Completed acquisition prefix component is missing: {id}"))?;
        ensure!(
            completed_definitions
                .insert(id.to_ascii_lowercase(), serde_json::to_value(component)?)
                .is_none(),
            "Duplicate completed acquisition prefix component: {id}"
        );
    }
    let candidates = selection::download_candidates(&candidate.components, selection::Scope::All)
        .into_iter()
        .filter(|candidate| {
            !completed_definitions.contains_key(&candidate.guid.to_ascii_lowercase())
        })
        .collect::<Vec<_>>();
    if !candidates.iter().any(|item| {
        !candidate.components[item.index]
            .resource_registry
            .is_empty()
    }) {
        return Ok(());
    }
    let stored = SettingsFile::resolve(settings_dir)?;
    let peer_settings = p2p::PeerSettings::from_settings(&stored.load()?);
    let effective_mode = peer_settings.effective_mode();
    let keep_sharing =
        peer_settings.sharing && peer_settings.keep_sharing && effective_mode != p2p::PeerMode::Off;
    let needs_nexus = candidates.iter().any(|item| {
        candidate.components[item.index]
            .resource_registry
            .keys()
            .any(|url| matches!(download_hosts::provider(url), Ok(Provider::Nexus)))
    });
    let nexus_api_key = resolve_nexus_key(
        overrides.nexus_api_key.as_deref(),
        settings_dir,
        needs_nexus,
    )?;
    let plan_options = orchestration::PlanOptions {
        source_dir: source.to_owned(),
        resolution: None,
        peer_mode: match effective_mode {
            p2p::PeerMode::Off => orchestration::PeerMode::Off,
            p2p::PeerMode::Race => orchestration::PeerMode::Race,
            p2p::PeerMode::Fallback => orchestration::PeerMode::Fallback,
        },
        user_peers: Vec::new(),
        nexus_key_present: nexus_api_key.is_some(),
        archive: options.clone(),
    };
    let plan = orchestration::plan_with_cancel(&candidate, &candidates, &plan_options, cancelled)?;
    valid_skips(&plan)?;
    let peer: Option<Arc<dyn Acquire>> = if effective_mode == p2p::PeerMode::Off {
        None
    } else {
        Some(Arc::new(PeerAcquire::new(
            &candidate,
            peer_settings,
            options.clone(),
            settings_dir.map(Path::to_owned),
        )))
    };
    let settings = orchestration::RunSettings {
        workers: overrides.workers,
        archive: options.clone(),
        nexus_api_key,
        peer,
        ..Default::default()
    };
    check_cancelled(cancelled)?;
    let plan = orchestration::discover(&candidate, &plan, &plan_options, &settings, cancelled)?;
    valid_skips(&plan)?;
    let report = orchestration::run(&plan, &settings, cancelled, |_| {})?;
    check_cancelled(cancelled)?;
    if report.cancelled
        || report
            .outcomes
            .iter()
            .any(|outcome| outcome.status == orchestration::TargetStatus::Cancelled)
    {
        return Err(crate::cli_session::Exit(130).into());
    }
    let indices: BTreeSet<_> = report
        .outcomes
        .iter()
        .map(|outcome| outcome.target_index)
        .collect();
    ensure!(
        report.outcomes.len() == plan.targets.len() && indices == (0..plan.targets.len()).collect(),
        "Acquisition did not return one terminal result for every selected target"
    );
    let failed = report
        .outcomes
        .iter()
        .filter(|outcome| outcome.status == orchestration::TargetStatus::Failed)
        .count();
    let manual = report
        .outcomes
        .iter()
        .filter(|outcome| outcome.status == orchestration::TargetStatus::ManualRequired)
        .count();
    ensure!(
        failed == 0 && manual == 0,
        "Acquisition is incomplete: {failed} failed and {manual} require manual action; complete Fetch Downloads before processing"
    );
    ensure!(
        report.outcomes.iter().all(|outcome| outcome.status
            == orchestration::TargetStatus::Completed
            && outcome.path.is_some()),
        "Acquisition contains an incomplete archive result"
    );

    let metadata = orchestration::collect_metadata(&plan, &report, &settings, cancelled, None);
    check_cancelled(cancelled)?;
    if metadata.cancelled {
        return Err(crate::cli_session::Exit(130).into());
    }
    let scan = orchestration::collect_fomod_consumers(
        &candidate,
        &report,
        options,
        cancelled,
        &overrides.completed,
    );
    ensure!(
        scan.inspection_errors.is_empty(),
        "FOMOD inspection failed for {} acquired archives; review Fetch Downloads results",
        scan.inspection_errors.len()
    );
    let configuration = modsync::fomod_cli::dispatch(
        &mut candidate,
        scan,
        &modsync::fomod_cli::Options {
            skip: overrides.fomod_skip,
            choices: overrides.fomod_choices.clone(),
            interactive: overrides.fomod_interactive,
            non_interactive: overrides.fomod_non_interactive,
            settings_dir: settings_dir.map(Path::to_owned),
        },
        cancelled,
        &overrides.completed,
    );
    check_cancelled(cancelled)?;
    if configuration.cancelled {
        return Err(crate::cli_session::Exit(130).into());
    }
    ensure!(
        !configuration.has_errors()
            && configuration.unresolved == 0
            && configuration.followups().is_empty(),
        "FOMOD configuration is incomplete; supply verified choices through Fetch Downloads before processing"
    );
    orchestration::record(&mut candidate, &report)?;
    orchestration::record_metadata(&mut candidate, &metadata)?;
    configuration.apply_recorded(&mut candidate, &overrides.completed)?;
    if overrides.include_unselected_authoring {
        ensure!(
            candidate.components.len() == original_selection.len(),
            "Authoring acquisition changed component ownership; original guide was retained"
        );
        let mut restored = BTreeSet::new();
        for component in &mut candidate.components {
            let key = modsync::fomod::fold_case(&component.guid);
            let (original_guid, selected) = original_selection
                .get(&key)
                .context("Authoring acquisition introduced an unknown component identity")?;
            ensure!(
                component.guid == *original_guid && restored.insert(key),
                "Authoring acquisition changed or duplicated component identity"
            );
            component.is_selected = *selected;
        }
    }
    for (id, original) in &completed_definitions {
        let current = candidate
            .components
            .iter()
            .find(|component| component.guid.eq_ignore_ascii_case(id))
            .context("Acquisition removed a completed component")?;
        ensure!(
            serde_json::to_value(current)? == *original,
            "Acquisition changed completed component {id}; original guide was retained"
        );
    }
    check_cancelled(cancelled)?;
    if keep_sharing {
        // Durable admission does not run a background sharing controller here.
        // A guide from stdin has no saved authority and remains approval-held.
        match modsync::peer_sharing::Catalog::open(&stored) {
            Ok(catalog) => {
                let enrolled = modsync::peer_sharing::enroll_report_cancelled(
                    &catalog,
                    &candidate,
                    &plan,
                    &report,
                    options,
                    overrides.guide_path.as_deref(),
                    true,
                    cancelled,
                );
                if !enrolled.failures.is_empty() {
                    eprintln!(
                        "Warning: {} verified sharing enrollments could not be persisted",
                        enrolled.failures.len()
                    );
                }
            }
            Err(_) => eprintln!("Warning: verified sharing enrollment storage is unavailable"),
        }
    }
    check_cancelled(cancelled)?;
    *guide = candidate;
    Ok(())
}
