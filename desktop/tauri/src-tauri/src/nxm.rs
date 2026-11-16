//! Nexus Mods "Mod Manager Download" (nxm://) links, as the original handled
//! them: the app claims nxm:// links only while a download is waiting for the
//! player's click, a click while the app runs reaches the running window
//! (`modsync::single_instance`), and the link is downloaded into the mods folder
//! with the saved Nexus API key, then matched to the open build's mod
//! (`download_orchestrator::nxm_target`). Registration uses ModSync's per-user
//! handler ownership (`protocol_registration::owned`), so another app's handler
//! is never overwritten.
use crate::state::{emit, log, state};
use modsync::{
    download_orchestrator as orch,
    protocol::{self, Link},
    protocol_registration::owned,
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc::Receiver},
};
use tauri::{AppHandle, Manager};

/// This app's own handler identity and single-instance channel, separate from
/// the other ModSync frontends so their links never cross.
const PACKAGE: &str = "org.modsync.kotor-mod-installer";
const CHANNEL: &str = "kotor-mod-installer";
/// Nexus game domains this app handles; links for other games are refused.
const GAMES: [&str; 2] = ["kotor", "kotor2"];

/// The link this process was started with: `gui --protocol <url>` as the
/// handler registration launches it, or a bare nxm:// argument.
pub fn startup_link() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let link = match args.iter().position(|a| a == "--protocol") {
        Some(index) => args.get(index + 1).cloned(),
        None => args
            .into_iter()
            .find(|a| a.to_ascii_lowercase().starts_with("nxm://")),
    };
    // A malformed link is dropped, so the launch still reaches an open window.
    link.filter(|l| modsync::single_instance::validate_protocol_request(l).is_ok())
}

/// How this process starts.
pub enum Start {
    /// Another window is already open and has the link (or was brought to the
    /// front); this process should exit.
    Forwarded,
    /// This is the only window. Links from later launches arrive here; the
    /// guard must live as long as the app.
    Primary(Receiver<String>, modsync::single_instance::Guard),
    /// Single-instance delivery is unavailable; links only come from this
    /// process's own start.
    Alone(Option<String>),
}

pub fn claim(link: Option<&str>) -> Start {
    match modsync::single_instance::claim_or_forward_for_channel(link, CHANNEL) {
        Ok(None) => Start::Forwarded,
        Ok(Some((receiver, guard))) => Start::Primary(receiver, guard),
        Err(_) => Start::Alone(link.map(str::to_owned)),
    }
}

/// Handle links as they arrive: bring the window forward, and download links.
pub fn listen(app: AppHandle, receiver: Receiver<String>) {
    std::thread::spawn(move || {
        for message in receiver {
            if message == modsync::single_instance::ACTIVATE_MESSAGE {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
                continue;
            }
            handle(&app, &message);
        }
    });
}

fn request() -> Option<owned::Request> {
    Some(owned::Request {
        executable: std::env::current_exe().ok()?,
        package_token: PACKAGE.into(),
        channel: CHANNEL.into(),
        schemes: vec![owned::Scheme::Nxm],
        replace: false,
    })
}

/// Claim or give back nxm:// links (the interface asks while a Nexus download
/// waits for its click). Another app's handler, such as Vortex's, is left alone.
pub fn set_handler(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let Some(request) = request() else {
        return Err("The app's location could not be found.".into());
    };
    let cancelled = AtomicBool::new(false);
    if enabled {
        match owned::register(&request, &cancelled) {
            Ok(_) => {}
            Err(error) => log(
                app,
                format!(
                    "Nexus \"Mod Manager Download\" links can't be sent to this app ({error:#}). Another mod manager may be handling them; download the file from the mod page instead."
                ),
                "warning",
            ),
        }
    } else {
        release();
    }
    Ok(())
}

/// Give nxm:// links back, if this app holds them. Run at start (a claim left
/// by a crash) and at exit.
pub fn release() {
    if let Some(request) = request() {
        let _ = owned::unregister(&request, &AtomicBool::new(false));
    }
}

/// After a download step: the Nexus mods that need the player's "Mod Manager
/// Download" click. Claims nxm:// links while any are waiting.
pub fn waiting(app: &AppHandle, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    {
        let st = state(app);
        let mut inner = st.lock();
        inner
            .nxm_pending
            .extend(ids.iter().map(|id| id.to_ascii_lowercase()));
    }
    emit(app, json!({"type": "nxm_wait", "active": true}));
    log(
        app,
        "For the Nexus mods above, open each mod's Files page and click \"Mod Manager Download\". The app downloads them, then press Install again.",
        "info",
    );
}

fn delivered(app: &AppHandle, id: &str) {
    let empty = {
        let st = state(app);
        let mut inner = st.lock();
        inner.nxm_pending.remove(&id.to_ascii_lowercase());
        inner.nxm_pending.is_empty()
    };
    if empty {
        emit(app, json!({"type": "nxm_wait", "active": false}));
    }
}

fn status(app: &AppHandle, id: &str, st: &str, label: &str, detail: &str) {
    emit(
        app,
        json!({"type": "status", "file_id": id, "status": st, "status_label": label, "detail": detail}),
    );
}

/// Download one nxm:// link into the mods folder.
pub fn handle(app: &AppHandle, url: &str) {
    let link = match protocol::parse(url) {
        Ok(link @ Link::Nxm { .. }) => link,
        _ => {
            log(app, "That link is not a Nexus download link.", "warning");
            return;
        }
    };
    let Link::Nxm { game_domain, .. } = &link else {
        return;
    };
    if !GAMES.iter().any(|g| g.eq_ignore_ascii_case(game_domain)) {
        log(
            app,
            "That Nexus link is for another game, so it was not downloaded.",
            "warning",
        );
        return;
    }
    let st = state(app);
    let Some(key) = st.nexus_key() else {
        log(
            app,
            "Add your Nexus API key in Settings to download Nexus links.",
            "error",
        );
        return;
    };
    log(app, "Got the Nexus download link - continuing.", "success");
    // Downloads share the mods folder with installs; wait for a running one.
    while st.lock().running {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let mods = PathBuf::from(crate::pipeline::mods_folder(&st));
    if let Err(error) = std::fs::create_dir_all(&mods) {
        log(
            app,
            format!("The mods folder could not be made: {error}"),
            "error",
        );
        return;
    }
    // The open build's mod for this link, else a one-mod guide for it alone.
    let loaded = st.lock().loaded.as_ref().map(|l| l.guide.clone());
    let (guide, in_build) = match loaded {
        Some(guide) if orch::nxm_target(&guide, &link).is_ok() => (guide, true),
        _ => (standalone(&link), false),
    };
    let result = (|| -> anyhow::Result<(orch::RunReport, orch::MetadataReport)> {
        let target = orch::nxm_target(&guide, &link)?;
        let plan = orch::AcquisitionPlan::single(target, &mods)?;
        let settings = orch::RunSettings {
            workers: 1,
            nexus_api_key: Some(key),
            archive: st.archive_options(),
            ..Default::default()
        };
        let cancelled = AtomicBool::new(false);
        let events = app.clone();
        let report = orch::run(&plan, &settings, &cancelled, move |e| {
            if let modsync::acquisition::Phase::Transferring { bytes, total, .. } = e.phase {
                emit(
                    &events,
                    json!({
                        "type": "progress", "file_id": e.component_guid,
                        "pct": total.map_or(0.0, |t| bytes as f64 / t.max(1) as f64),
                        "kb": bytes / 1024, "total_kb": total.unwrap_or(0) / 1024,
                    }),
                );
            }
        })?;
        let metadata = orch::collect_metadata(&plan, &report, &settings, &cancelled, None);
        Ok((report, metadata))
    })();
    let (report, metadata) = match result {
        Ok(done) => done,
        Err(error) => {
            log(
                app,
                format!("The Nexus download failed: {error:#}"),
                "error",
            );
            return;
        }
    };
    let Some(outcome) = report.outcomes.first().cloned() else {
        log(app, "The Nexus download did not start.", "error");
        return;
    };
    let id = outcome.component_guid.clone();
    if outcome.status != orch::TargetStatus::Completed {
        let reason = outcome
            .error
            .unwrap_or_else(|| "The Nexus download did not finish.".into());
        if in_build {
            status(app, &id, "ERROR", "Error", &reason);
        }
        log(app, format!("The Nexus download failed: {reason}"), "error");
        return;
    }
    let file = outcome.filename.unwrap_or_default();
    if in_build {
        // Record the file (and what the site said about it) in the open build, as an
        // install's own downloads are.
        if let Some(loaded) = st.lock().loaded.as_mut() {
            let _ = orch::record(&mut loaded.guide, &report);
            let _ = orch::record_metadata(&mut loaded.guide, &metadata);
        }
        let name = guide
            .components
            .iter()
            .find(|c| c.guid.eq_ignore_ascii_case(&id))
            .map_or(file.clone(), |c| c.name.clone());
        status(app, &id, "READY", "Ready", "");
        log(
            app,
            format!("Downloaded {name} from Nexus. Press Install again to install it."),
            "success",
        );
        delivered(app, &id);
    } else {
        log(
            app,
            format!("Downloaded {file} from Nexus into {}.", mods.display()),
            "success",
        );
    }
}

/// A one-mod guide that names the link's Nexus mod page, so the shared Nexus
/// download path can fetch a link that is not in the open build.
fn standalone(link: &Link) -> modsync::Guide {
    let Link::Nxm {
        game_domain,
        mod_id,
        ..
    } = link
    else {
        return modsync::Guide::default();
    };
    let mut component = modsync::Component {
        guid: uuid_like(*mod_id),
        name: format!("Nexus mod {mod_id}"),
        is_selected: true,
        ..Default::default()
    };
    component.resource_registry.insert(
        format!("https://www.nexusmods.com/{game_domain}/mods/{mod_id}"),
        json!({}),
    );
    modsync::Guide {
        components: vec![component],
        ..Default::default()
    }
}

/// A fixed GUID-shaped id for a standalone Nexus mod.
fn uuid_like(mod_id: u64) -> String {
    format!(
        "00000000-0000-4000-8000-{:012x}",
        mod_id % 0x1_0000_0000_0000
    )
}
