//! "Install": get the files the selected mods need, check the install with the
//! engine's validation (nothing on disk changes), then install with the
//! engine, reporting each mod's state as live events.
use crate::api::Request;
use crate::state::{Reply, emit, fail, fail_any, game_of, log, state};
use anyhow::{Result, anyhow};
use modsync::{
    Engine, Guide, Settings,
    acquisition::Phase as Acq,
    download_orchestrator as orch,
    install_progress::Phase,
    selection::{self, Action, Scope},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::AppHandle;

fn status(app: &AppHandle, id: &str, st: &str, label: &str, detail: &str) {
    emit(
        app,
        json!({"type": "status", "file_id": id, "status": st, "status_label": label, "detail": detail}),
    );
}

pub fn control(app: &AppHandle, action: &str) -> Reply {
    match action {
        "stop" => {
            if state(app).request_stop() {
                log(app, "Stopping after the current step.", "warning");
            }
            Ok(json!({"ok": true, "action": action}))
        }
        "retry" => Ok(json!({"ok": true, "action": action})),
        "pause" => {
            if state(app).set_paused(true) {
                log(
                    app,
                    "Pausing after the current file or mod. Nothing is left half installed.",
                    "warning",
                );
            }
            Ok(json!({"ok": true, "action": action}))
        }
        "resume" => {
            if state(app).set_paused(false) {
                log(app, "Resuming.", "info");
            }
            Ok(json!({"ok": true, "action": action}))
        }
        _ => Err(fail(404, "Unknown action")),
    }
}

struct Job {
    guide: Guide,
    selected: Vec<String>,
    game: PathBuf,
    mods: PathBuf,
    nexus_key: Option<String>,
    resolution: Option<(u32, u32)>,
    case_insensitive: bool,
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    /// Stop after the check; nothing is installed.
    check_only: bool,
}

/// The folders of an install that stopped and left a restore point: the
/// captured ones, or the active install's folders when a restore point is
/// pending there.
pub(crate) fn pending_roots(app: &AppHandle) -> Option<modsync::paths::Roots> {
    let st = state(app);
    let pending = |roots: &modsync::paths::Roots| {
        modsync::recovery::state_dir(roots).join("pending").exists()
    };
    if let Some(roots) = st.lock().failed_roots.clone().filter(|r| pending(r)) {
        return Some(roots);
    }
    let store = st.prefs.store().ok()?;
    let profile = store.load(&store.active_name().ok()??).ok()??;
    let game = profile.kotor_directory?;
    modsync::paths::Roots::new(Path::new(&st.mods_dir()), Path::new(&game))
        .ok()
        .filter(|r| pending(r))
}

pub fn recovery_state(app: &AppHandle) -> Reply {
    Ok(match pending_roots(app) {
        Some(roots) => {
            json!({"pending": true, "game_path": roots.game.display().to_string(), "mods_path": roots.mods.display().to_string()})
        }
        None => json!({"pending": false}),
    })
}

/// Put both folders back to the restore point (`recovery::restore`).
pub fn restore(app: &AppHandle) -> Reply {
    let st = state(app);
    if st.lock().running {
        return Err(fail(409, "Install already running"));
    }
    let roots =
        pending_roots(app).ok_or_else(|| fail(400, "There is no unfinished install to restore"))?;
    log(
        app,
        "Restoring your game to how it was before the install.",
        "info",
    );
    modsync::recovery::restore(&roots).map_err(fail_any)?;
    st.lock().failed_roots = None;
    log(
        app,
        "Your game is back to how it was before the install.",
        "success",
    );
    emit(app, json!({"type": "library", "event": "changed"}));
    Ok(json!({"ok": true}))
}

/// Everything an install or a check needs, gathered and checked up front.
/// The fingerprint names the exact inputs (selection, options and folders), so
/// an install can require a passing check of the same inputs.
fn prepare(app: &AppHandle, r: &Request, check_only: bool) -> Result<(Job, String, String), Value> {
    let st = state(app);
    if pending_roots(app).is_some() {
        return Err(fail(409, "restore_pending"));
    }
    let key = r.b("build_key");
    let selected: Vec<String> = r
        .body
        .get("selected_file_ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let (guide, game) = {
        let inner = st.lock();
        if inner.running {
            return Err(fail(409, "Install already running"));
        }
        let Some(loaded) = inner.loaded.as_ref().filter(|l| l.key == key) else {
            return Err(fail(400, "Load the mod list first"));
        };
        if !inner.signed_in {
            return Err(fail(401, "Not logged in"));
        }
        (loaded.guide.clone(), loaded.game.clone())
    };
    if selected.is_empty() {
        return Err(fail(400, "No mods selected"));
    }
    let store = st.prefs.store().map_err(fail_any)?;
    let mut profile = crate::builds::target_profile(&st, &game).map_err(fail_any)?;
    let override_path = r.b("game_path");
    if !override_path.trim().is_empty() {
        profile.kotor_directory = Some(override_path.trim().to_owned());
        store.save(&profile).map_err(fail_any)?;
    }
    let path = profile.kotor_directory.clone().unwrap_or_default();
    if path.trim().is_empty() || !Path::new(&path).is_dir() {
        let mut e = fail(400, "game_path_required");
        e["game"] = game.into();
        return Err(e);
    }
    if game_of(&path).is_none() {
        let mut e = fail(400, "game_path_invalid");
        e["game"] = game.into();
        e["path"] = path.into();
        return Err(e);
    }
    let mods = mods_folder(&st);
    std::fs::create_dir_all(&mods).map_err(|e| fail(500, e.to_string()))?;
    store.activate(&profile.name).map_err(fail_any)?;
    store
        .save_directory_preferences(&mods, &path)
        .map_err(fail_any)?;
    let fingerprint = {
        use std::hash::{Hash, Hasher};
        let mut sorted = selected.clone();
        sorted.sort();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (
            sorted,
            serde_json::to_string(&guide.components).unwrap_or_default(),
            &path,
            &mods,
        )
            .hash(&mut hasher);
        format!("{:x}", hasher.finish())
    };
    let inner = st.lock();
    let job = Job {
        guide,
        selected,
        game: PathBuf::from(&path),
        mods: PathBuf::from(&mods),
        nexus_key: st.nexus_key(),
        resolution: screen_resolution(app, &st),
        case_insensitive: inner.case_insensitive,
        cancelled: Arc::new(AtomicBool::new(false)),
        paused: Arc::new(AtomicBool::new(false)),
        check_only,
    };
    Ok((job, path, fingerprint))
}

fn begin(app: &AppHandle, job: &Job) -> Result<(), Value> {
    let st = state(app);
    let mut inner = st.lock();
    if inner.running {
        return Err(fail(409, "Install already running"));
    }
    inner.running = true;
    inner.cancelled = job.cancelled.clone();
    inner.paused = job.paused.clone();
    Ok(())
}

/// Get the files and check the install, changing nothing in the game. A
/// passing check unlocks Install for the same inputs.
pub fn check(app: &AppHandle, r: &Request) -> Reply {
    let (job, _, fingerprint) = prepare(app, r, true)?;
    begin(app, &job)?;
    let mut tally = Tally::default();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(app, job, &mut tally)))
            .unwrap_or_else(|_| Err(anyhow!("The check stopped unexpectedly.")));
    let st = state(app);
    let passed = result.is_ok() && tally.report.as_ref().is_some_and(|r| r["success"] == true);
    {
        let mut inner = st.lock();
        inner.running = false;
        inner.checked = passed.then_some(fingerprint);
    }
    if let Err(error) = &result {
        log(app, format!("{error:#}"), "error");
    }
    Ok(
        json!({"ok": passed, "report": tally.report, "error": result.err().map(|e| format!("{e:#}"))}),
    )
}

pub fn start(app: &AppHandle, r: &Request) -> Reply {
    let (job, path, fingerprint) = prepare(app, r, false)?;
    if state(app).lock().checked.as_deref() != Some(fingerprint.as_str()) {
        return Err(fail(409, "check_required"));
    }
    begin(app, &job)?;
    let selected = job.selected.clone();
    let worker = app.clone();
    std::thread::spawn(move || {
        let total = job.selected.len();
        emit(
            &worker,
            json!({"type": "pipeline", "event": "started", "total": total}),
        );
        let mut tally = Tally::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&worker, job, &mut tally)
        }))
        .unwrap_or_else(|_| Err(anyhow!("The install stopped unexpectedly.")));
        if let Err(error) = result {
            log(&worker, format!("{error:#}"), "error");
        }
        let st = state(&worker);
        {
            let mut inner = st.lock();
            inner.running = false;
            inner.checked = None;
        }
        emit(
            &worker,
            json!({"type": "pipeline", "event": "finished", "done": tally.done, "total": total, "manual": tally.manual, "errors": tally.errors}),
        );
        emit(&worker, json!({"type": "library", "event": "changed"}));
    });
    Ok(json!({"ok": true, "total": selected.len(), "game_path": path}))
}

/// The shared mod folder, or Downloads/KOTOR_Mods when none is set yet.
pub(crate) fn mods_folder(st: &crate::state::AppState) -> String {
    let mods = st.mods_dir();
    if !mods.trim().is_empty() {
        return mods;
    }
    dirs_home()
        .join("Downloads")
        .join("KOTOR_Mods")
        .display()
        .to_string()
}

fn dirs_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// The size of the monitor the app is on when ModSync's
/// `filterDownloadsByResolution` setting is on, so only the widescreen files
/// that match it download (as in the ModSync desktop app).
fn screen_resolution(app: &AppHandle, st: &crate::state::AppState) -> Option<(u32, u32)> {
    use tauri::Manager;
    if !st.prefs.filter_by_resolution() {
        return None;
    }
    let size = app
        .get_webview_window("main")
        .and_then(|w| {
            w.current_monitor()
                .ok()
                .flatten()
                .or_else(|| w.primary_monitor().ok().flatten())
        })
        .map(|m| *m.size())
        .filter(|s| s.width >= 1 && s.height >= 1);
    if size.is_none() {
        log(
            app,
            "Monitor resolution unavailable; downloads will not be filtered by resolution.",
            "warning",
        );
    }
    size.map(|s| (s.width, s.height))
}

#[derive(Default)]
pub(crate) struct Tally {
    pub done: usize,
    pub manual: usize,
    pub errors: usize,
    /// The engine's validation report, from the check step.
    pub report: Option<Value>,
}

fn name_of(guide: &Guide, id: &str) -> String {
    guide
        .components
        .iter()
        .find(|c| c.guid.eq_ignore_ascii_case(id))
        .map_or_else(|| id.to_owned(), |c| c.name.clone())
}

fn run(app: &AppHandle, job: Job, tally: &mut Tally) -> Result<()> {
    let Job {
        mut guide,
        selected,
        game,
        mods,
        nexus_key,
        resolution,
        case_insensitive,
        cancelled,
        paused,
        check_only,
    } = job;
    // 1. The player's selection, with what each mod needs (engine selection rules).
    selection::apply(&mut guide.components, None, Action::DeselectBase)?;
    for id in &selected {
        let outcome = selection::apply(
            &mut guide.components,
            Some(modsync::game::detect(&modsync::game::resolve_install_root(
                &game,
            ))),
            Action::SetComponent {
                guid: id.clone(),
                selected: true,
            },
        )?;
        for added in outcome
            .auto_selected
            .iter()
            .filter(|g| !g.eq_ignore_ascii_case(id))
        {
            log(
                app,
                format!(
                    "Also installing {} because {} needs it.",
                    name_of(&guide, added),
                    name_of(&guide, id)
                ),
                "info",
            );
        }
        for warning in outcome.warnings {
            log(app, warning, "warning");
        }
    }
    // 2. Files.
    let skipped = download(
        app,
        &mut guide,
        &mods,
        nexus_key,
        resolution,
        (&cancelled, &paused),
        tally,
    )?;
    for id in skipped {
        let outcome = selection::apply(
            &mut guide.components,
            None,
            Action::SetComponent {
                guid: id.clone(),
                selected: false,
            },
        )?;
        for removed in outcome
            .auto_deselected
            .iter()
            .filter(|g| !g.eq_ignore_ascii_case(&id))
        {
            status(
                app,
                removed,
                "SKIPPED",
                "Skipped",
                &format!("Needs {}", name_of(&guide, &id)),
            );
        }
    }
    if cancelled.load(Ordering::Acquire) {
        log(app, "Install stopped.", "warning");
        return Ok(());
    }
    if !guide.components.iter().any(|c| c.is_selected) {
        return Ok(());
    }
    let target = Target {
        mods,
        game,
        case_insensitive,
        cancelled,
        paused,
    };
    check_and_install(app, &guide, &target, check_only, tally)
}

/// Where and how a guide installs, and the run's stop and pause flags.
pub(crate) struct Target {
    pub mods: PathBuf,
    pub game: PathBuf,
    pub case_insensitive: bool,
    pub cancelled: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

/// Check `guide` with the engine (changing nothing on disk) and then, unless
/// `check_only`, install it into the game install's library. Used for builds and
/// for imported mods.
pub(crate) fn check_and_install(
    app: &AppHandle,
    guide: &Guide,
    target: &Target,
    check_only: bool,
    tally: &mut Tally,
) -> Result<()> {
    let Target {
        mods,
        game,
        case_insensitive,
        cancelled,
        paused,
    } = target;
    let case_insensitive = *case_insensitive;
    // 3. Check, changing nothing on disk.
    let settings = rebuild_settings(app, mods, game, cancelled, paused)?;
    let roots = settings.roots.clone();
    let prepared = modsync::phase::prepare_with_roots(
        guide,
        modsync::phase::Phase::All,
        modsync::phase::CompletionContext::None,
        None,
        &roots,
        case_insensitive,
        cancelled,
    )?
    .guide;
    log(app, "Checking the selected mods before installing.", "info");
    let report = Engine::new_with_case_insensitive_pathing(settings.clone(), case_insensitive)?
        .validate_report_with_options_observed(&prepared, |_| {}, true, true, false, |_| {});
    tally.report = serde_json::to_value(&report).ok();
    if !report.success {
        for stage in report.stages.iter().filter(|s| !s.passed) {
            for message in &stage.messages {
                log(app, message, "error");
            }
        }
        let mut failed: BTreeMap<String, String> = BTreeMap::new();
        for issue in report
            .dry_run
            .iter()
            .flat_map(|d| &d.issues)
            .filter(|i| i.severity == "Error")
        {
            log(app, &issue.message, "error");
            if let Some(id) = &issue.component_guid {
                failed
                    .entry(id.clone())
                    .or_insert_with(|| issue.message.clone());
            }
        }
        for (id, message) in &failed {
            status(app, id, "ERROR", "Error", message);
        }
        tally.errors += failed.len().max(1);
        anyhow::bail!("The check found problems, so nothing was installed.");
    }
    if check_only {
        log(app, "The check passed. You can install now.", "success");
        return Ok(());
    }
    // 4. Install, into the active game install's library when possible.
    let mut engine = Engine::new_with_case_insensitive_pathing(settings, case_insensitive)?;
    match crate::library::service_for(app, &roots.game.to_string_lossy()) {
        Ok(service) => match engine
            .with_component_checkpoints(true)
            .with_managed_deployment(service)
        {
            Ok(managed) => engine = managed,
            Err(error) => {
                log(
                    app,
                    format!("Installing without the library: {error:#}"),
                    "warning",
                );
                engine = Engine::new_with_case_insensitive_pathing(
                    rebuild_settings(app, mods, game, cancelled, paused)?,
                    case_insensitive,
                )?;
            }
        },
        Err(error) => log(
            app,
            format!("Installing without the library: {error:#}"),
            "warning",
        ),
    }
    let mut current: Option<String> = None;
    let result = engine.install_observed_report(&prepared, |_| {}, |p| {
        if p.phase == Phase::Paused {
            log(app, "Paused. Press Resume to carry on with the next mod.", "warning");
            return;
        }
        let Some(id) = p.top_level_component_guid.clone() else { return };
        match p.phase {
            Phase::InstallingComponent => {
                current = Some(id.clone());
                status(app, &id, "INSTALLING", "Installing", "");
            }
            Phase::RunningInstruction => {
                if let (Some(i), Some(n)) = (p.instruction_index, p.instruction_total) {
                    emit(app, json!({"type": "install_progress", "file_id": id, "pct": i as f64 / n.max(1) as f64, "label": p.operation.clone().unwrap_or_default()}));
                }
            }
            Phase::ComponentCompleted => {
                status(app, &id, "DONE", "Done", "");
                tally.done += 1;
            }
            _ => {}
        }
    });
    if let Err(error) = result {
        let message = format!("{error:#}");
        if let Some(id) = &current {
            status(app, id, "ERROR", "Error", &message);
        }
        tally.errors += 1;
        if modsync::recovery::state_dir(&roots)
            .join("pending")
            .exists()
        {
            state(app).lock().failed_roots = Some(roots);
            log(
                app,
                "Your game can be put back to how it was before this install: Installed mods > Restore game backup.",
                "warning",
            );
        }
        return Err(error);
    }
    log(app, "Install finished.", "success");
    Ok(())
}

fn rebuild_settings(
    app: &AppHandle,
    mods: &Path,
    game: &Path,
    cancelled: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
) -> Result<Settings> {
    let mut settings = Settings::new(mods, game)?;
    settings.archive = state(app).archive_options();
    settings.allow_processes = !state(app).lock().ui.block_programs;
    if let Ok(prefs) = state(app)
        .prefs
        .store()
        .and_then(|s| s.patcher_preferences())
    {
        settings.patcher_engine = prefs.engine;
        settings.patcher = prefs
            .executable
            .filter(|p| !p.trim().is_empty())
            .map(PathBuf::from);
    }
    settings.cancelled = cancelled.clone();
    settings.paused = paused.clone();
    Ok(settings)
}

/// Download what the selection needs. Returns mods that could not be fetched.
fn download(
    app: &AppHandle,
    guide: &mut Guide,
    mods: &Path,
    nexus_key: Option<String>,
    resolution: Option<(u32, u32)>,
    // The run's stop and pause flags.
    (cancelled, paused): (&Arc<AtomicBool>, &Arc<AtomicBool>),
    tally: &mut Tally,
) -> Result<Vec<String>> {
    let candidates = selection::download_candidates(&guide.components, Scope::All);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let options = orch::PlanOptions {
        source_dir: mods.to_path_buf(),
        resolution,
        peer_mode: orch::PeerMode::Off,
        user_peers: Vec::new(),
        nexus_key_present: nexus_key.is_some(),
        archive: state(app).archive_options(),
    };
    let plan = orch::plan_with_cancel(guide, &candidates, &options, cancelled)?;
    for skip in &plan.skipped {
        status(app, &skip.component_guid, "READY", "Ready", "");
    }
    let settings = orch::RunSettings {
        nexus_api_key: nexus_key,
        archive: state(app).archive_options(),
        paused: Some(paused.clone()),
        ..Default::default()
    };
    let plan = orch::discover(guide, &plan, &options, &settings, cancelled)?;
    let events = app.clone();
    let report = orch::run_with_source_context(
        &plan,
        &settings,
        guide,
        cancelled,
        move |e| {
            let id = e.component_guid.clone();
            match e.phase {
                Acq::Resolving => status(&events, &id, "DOWNLOADING", "Downloading", ""),
                Acq::Transferring { bytes, total, .. } => emit(
                    &events,
                    json!({
                        "type": "progress", "file_id": id,
                        "pct": total.map_or(0.0, |t| bytes as f64 / t.max(1) as f64),
                        "kb": bytes / 1024, "total_kb": total.unwrap_or(0) / 1024,
                    }),
                ),
                Acq::Verifying => status(&events, &id, "EXTRACTING", "Extracting", ""),
                Acq::Completed { .. } | Acq::Skipped { .. } => {
                    status(&events, &id, "READY", "Ready", "")
                }
                _ => {}
            }
        },
        None,
    )?;
    let mut skipped = Vec::new();
    for outcome in &report.outcomes {
        match outcome.status {
            orch::TargetStatus::ManualRequired => {
                let url = outcome
                    .manual
                    .as_ref()
                    .map_or(outcome.registry_url.clone(), |m| m.url.clone());
                emit(
                    app,
                    json!({"type": "manual", "file_id": outcome.component_guid, "name": name_of(guide, &outcome.component_guid),
                    "folder": mods.display().to_string(),
                    "readme": format!("Download this mod from {url} and put the archive in {}. Then install again.", mods.display())}),
                );
                tally.manual += 1;
                skipped.push(outcome.component_guid.clone());
            }
            orch::TargetStatus::Failed => {
                status(
                    app,
                    &outcome.component_guid,
                    "ERROR",
                    "Error",
                    outcome.error.as_deref().unwrap_or("The download failed."),
                );
                tally.errors += 1;
                skipped.push(outcome.component_guid.clone());
            }
            _ => {}
        }
    }
    if !report.cancelled {
        let metadata = orch::collect_metadata(&plan, &report, &settings, cancelled, None);
        orch::record(guide, &report)?;
        let _ = orch::record_metadata(guide, &metadata);
    }
    skipped.sort();
    skipped.dedup();
    Ok(skipped)
}
