//! Smaller routes: opening web pages, mod page details and screenshots, app
//! updates and release notes, DeadlyStream sign-in, and the window commands
//! the interface calls directly.
use crate::state::{Reply, fail};
use serde_json::json;
use tauri::AppHandle;

/// Where releases of this app are published.
pub const UPDATE_REPO: &str = "kolto-labs/ModSync.rs";

pub fn open_url(app: &AppHandle, url: &str, releases_default: bool) -> Reply {
    use tauri_plugin_opener::OpenerExt;
    let target = if url.is_empty() && releases_default {
        format!("https://github.com/{UPDATE_REPO}/releases/latest")
    } else {
        url.to_owned()
    };
    if !(target.starts_with("https://") || target.starts_with("http://")) {
        return Err(fail(400, "invalid_url"));
    }
    app.opener()
        .open_url(target, None::<&str>)
        .map_err(|e| fail(500, e.to_string()))?;
    Ok(json!({"ok": true}))
}

// Window commands the interface calls directly (GameFront session, nxm links).
// GameFront files are listed as manual downloads, so the session window only
// opens the site in the player's browser.

#[tauri::command]
pub fn open_gamefront(app: AppHandle) -> Result<(), String> {
    open_url(&app, "https://www.gamefront.com/", false)
        .map(|_| ())
        .map_err(|e| e["error"].as_str().unwrap_or("error").to_owned())
}

#[tauri::command]
pub fn close_gamefront() {}

#[tauri::command]
pub fn finish_gamefront() {}

#[tauri::command]
pub fn gamefront_account() -> bool {
    false
}

#[tauri::command]
pub fn sign_out_gamefront() {}

/// Claim or give back nxm:// links (`nxm::set_handler`).
#[tauri::command]
pub fn set_nxm_handler(app: AppHandle, enabled: bool) -> Result<(), String> {
    crate::nxm::set_handler(&app, enabled)
}

fn component_link(app: &AppHandle, id: &str) -> Option<String> {
    let st = crate::state::state(app);
    let inner = st.lock();
    let component = inner
        .loaded
        .as_ref()?
        .guide
        .components
        .iter()
        .find(|c| c.guid.eq_ignore_ascii_case(id))?
        .clone();
    drop(inner);
    let mut links: Vec<String> = component.resource_registry.keys().cloned().collect();
    if let Some(serde_json::Value::String(url)) = component.metadata.get("Link") {
        links.push(url.clone());
    }
    links
        .into_iter()
        .find(|l| modsync::mod_info::page(l).is_some())
}

pub fn mod_info(app: &AppHandle, r: &crate::api::Request) -> Reply {
    let id = r.q("file_id");
    let Some(url) = component_link(app, &id) else {
        return Ok(
            json!({"title": "", "description": "", "images": [], "author": "", "ds_url": "", "nexus_url": "", "error": "No mod page"}),
        );
    };
    let key = crate::state::state(app).nexus_key();
    match modsync::mod_info::fetch(&url, key, &std::sync::atomic::AtomicBool::new(false)) {
        Ok(Some(info)) => Ok(json!({
            "title": info.title,
            "description": info.description,
            "images": info.images,
            "author": info.author,
            "ds_url": if info.source == "deadlystream" { info.page_url.clone() } else { String::new() },
            "nexus_url": if info.source == "nexus" { info.page_url.clone() } else { String::new() },
        })),
        Ok(None) => Err(fail(404, "No mod page")),
        Err(error) => Ok(
            json!({"title": "", "description": "", "images": [], "author": "", "ds_url": url, "nexus_url": "", "error": format!("{error:#}")}),
        ),
    }
}

/// Serve `kmm-image://<url>`: a screenshot fetched by the engine, so the
/// mod sites' hotlink protection does not block it.
pub fn image_protocol(
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    std::thread::spawn(move || {
        let raw = request.uri().path().trim_start_matches('/').to_owned();
        let url = percent_decode(&raw);
        let response = match modsync::mod_info::fetch_image(
            &url,
            &std::sync::atomic::AtomicBool::new(false),
        ) {
            Ok((kind, bytes)) => tauri::http::Response::builder()
                .header("Content-Type", kind)
                .body(bytes),
            Err(_) => tauri::http::Response::builder()
                .status(404)
                .body(Vec::new()),
        };
        if let Ok(response) = response {
            responder.respond(response);
        }
    });
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const CHANGELOG: &str = include_str!("../../CHANGELOG.md");
const NO_SELF_UPDATE_MARKER: &str = "no-self-update.flag";

fn self_update_allowed() -> bool {
    !std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join(NO_SELF_UPDATE_MARKER)))
        .is_some_and(|path| path.is_file())
}

pub fn update_check() -> Reply {
    match modsync::app_update::check(
        UPDATE_REPO,
        crate::api::VERSION,
        &std::sync::atomic::AtomicBool::new(false),
    ) {
        Ok(check) => {
            let latest = check.latest.as_ref();
            Ok(json!({
                "available": check.available,
                "current_version": crate::api::VERSION,
                "latest_version": latest.map(|r| r.version.clone()),
                "url": latest.map(|r| r.url.clone()),
                "asset_url": latest.and_then(|r| r.asset_url.clone()),
                "notes": latest.map(|r| r.notes.clone()).unwrap_or_default(),
                "repo": UPDATE_REPO,
                "can_self_update": self_update_allowed(),
            }))
        }
        Err(error) => Ok(
            json!({"available": false, "current_version": crate::api::VERSION, "error": format!("{error:#}")}),
        ),
    }
}

fn update_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("kotor-mod-installer-update")
}

pub fn update_download(app: &AppHandle) -> Reply {
    if !self_update_allowed() {
        return Err(fail(403, "self_update_disabled"));
    }
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let check = modsync::app_update::check(UPDATE_REPO, crate::api::VERSION, &cancelled)
        .map_err(crate::state::fail_any)?;
    let release = check
        .latest
        .filter(|_| check.available)
        .ok_or_else(|| fail(400, "no_exe_asset"))?;
    if release.asset_url.is_none() {
        return Err(fail(400, "no_exe_asset"));
    }
    let path = modsync::app_update::download(&release, &update_dir(), &cancelled)
        .map_err(crate::state::fail_any)?;
    crate::state::emit(
        app,
        json!({"type": "update_progress", "pct": 100, "downloaded": 0, "total": 0}),
    );
    Ok(json!({"ok": true, "path": path.display().to_string(), "version": release.version}))
}

pub fn whats_new(app: &AppHandle) -> Reply {
    let notes = modsync::app_update::changelog_section(CHANGELOG, crate::api::VERSION)
        .map(|section| format!("## [{}]\n\n{section}", crate::api::VERSION))
        .unwrap_or_default();
    let seen = crate::state::state(app).lock().ui.last_seen_version.clone();
    Ok(
        json!({"version": crate::api::VERSION, "show": !notes.is_empty() && seen != crate::api::VERSION, "notes": notes}),
    )
}

/// Replace the running program with the downloaded one once it exits, then
/// start it again.
#[tauri::command]
pub fn apply_update(app: AppHandle, new_path: String) -> Result<(), String> {
    use std::{path::PathBuf, process::Command};
    if !self_update_allowed() {
        return Err("Self-update is disabled in this build".into());
    }
    if crate::state::state(&app).lock().running {
        return Err("Wait for the install to finish before updating.".into());
    }
    let new_path = PathBuf::from(new_path);
    if !new_path.starts_with(update_dir()) || !new_path.is_file() {
        return Err("The downloaded update could not be found.".into());
    }
    let current = match std::env::var_os("APPIMAGE") {
        Some(path) if cfg!(unix) => PathBuf::from(path),
        _ => std::env::current_exe().map_err(|e| e.to_string())?,
    };
    let pid = std::process::id();
    let dir = update_dir();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let quote = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
        let script = dir.join("swap.ps1");
        let content = format!(
            "try {{ Wait-Process -Id {pid} -Timeout 30 }} catch {{}}\n\
             Start-Sleep -Milliseconds 800\n\
             for ($i=0; $i -lt 10; $i++) {{ try {{ Copy-Item -LiteralPath '{new}' -Destination '{cur}' -Force; break }} catch {{ Start-Sleep -Milliseconds 700 }} }}\n\
             Start-Process -FilePath '{cur}'\n\
             Remove-Item -LiteralPath '{new}' -Force -ErrorAction SilentlyContinue\n",
            new = quote(&new_path),
            cur = quote(&current),
        );
        std::fs::write(&script, content).map_err(|e| e.to_string())?;
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
            ])
            .arg(&script)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::{fs::PermissionsExt, process::CommandExt};
        let script = dir.join("swap.sh");
        let content = "#!/bin/sh\n\
            pid=\"$1\"; new=\"$2\"; cur=\"$3\"\n\
            i=0\n\
            while kill -0 \"$pid\" 2>/dev/null && [ $i -lt 60 ]; do sleep 0.5; i=$((i+1)); done\n\
            sleep 1\n\
            i=0\n\
            until cp -f \"$new\" \"$cur.tmp\" && chmod 755 \"$cur.tmp\" && mv -f \"$cur.tmp\" \"$cur\"; do\n\
              i=$((i+1)); [ $i -ge 10 ] && exit 1; sleep 0.7\n\
            done\n\
            rm -f \"$new\"\n\
            nohup \"$cur\" >/dev/null 2>&1 &\n";
        std::fs::write(&script, content).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Command::new("/bin/sh")
            .arg(&script)
            .arg(pid.to_string())
            .arg(&new_path)
            .arg(&current)
            .process_group(0)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (pid, dir, current);
        return Err("Updating is not supported on this system.".into());
    }
    app.exit(0);
    Ok(())
}

pub fn login(app: &AppHandle, r: &crate::api::Request) -> Reply {
    let username = r.b("username");
    let password = r.b("password");
    match modsync::deadlystream_account::sign_in(
        &username,
        &password,
        &std::sync::atomic::AtomicBool::new(false),
    ) {
        Ok(account) => {
            let st = crate::state::state(app);
            let session = account.session.header().to_owned();
            {
                let mut inner = st.lock();
                inner.signed_in = true;
                inner.deadlystream_session = Some(session.clone());
                inner.ui.deadlystream_username = account.username.clone();
            }
            st.save_ui();
            // Like the original, the sign-in is kept only when asked to; unlike it,
            // the password is never kept, only the signed-in session.
            let save = r
                .body
                .get("save")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            if !save {
                crate::secrets::forget();
            } else if let Err(error) = crate::secrets::save(&account.username, &session) {
                crate::state::log(
                    app,
                    format!(
                        "Your sign-in could not be saved ({error}), so you will be asked to sign in again next time."
                    ),
                    "warning",
                );
            }
            crate::state::emit(
                app,
                json!({"type": "auth", "logged_in": true, "username": account.username}),
            );
            Ok(json!({"ok": true, "username": account.username}))
        }
        Err(error) => {
            crate::state::log(
                app,
                format!("DeadlyStream sign-in failed: {error:#}"),
                "warning",
            );
            Err(fail(401, format!("{error:#}")))
        }
    }
}

/// Check a saved DeadlyStream sign-in once the app starts, like the original's
/// sign-in at startup. A session the site no longer accepts is forgotten and the
/// player is asked to sign in again; an offline start keeps it.
pub fn restore_sign_in(app: AppHandle) {
    use modsync::deadlystream_account::{Session, check_session};
    let st = crate::state::state(&app);
    let (Some(saved), username) = ({
        let inner = st.lock();
        (
            inner.deadlystream_session.clone(),
            inner.ui.deadlystream_username.clone(),
        )
    }) else {
        return;
    };
    let checked = Session::from_header(&saved)
        .and_then(|session| check_session(&session, &std::sync::atomic::AtomicBool::new(false)));
    match checked {
        Ok(Some(renewed)) => {
            if renewed.header() != saved {
                let _ = crate::secrets::save(&username, renewed.header());
            }
            {
                let mut inner = st.lock();
                inner.signed_in = true;
                inner.deadlystream_session = Some(renewed.header().to_owned());
            }
            crate::state::emit(
                &app,
                json!({"type": "auth", "logged_in": true, "username": username}),
            );
            crate::state::log(&app, format!("Logged in as {username}."), "success");
        }
        Ok(None) => {
            crate::secrets::forget();
            {
                let mut inner = st.lock();
                inner.signed_in = false;
                inner.deadlystream_session = None;
            }
            crate::state::emit(
                &app,
                json!({"type": "auth", "logged_in": false, "username": ""}),
            );
            crate::state::log(
                &app,
                "Your DeadlyStream sign-in has expired. Sign in again to download mods.",
                "warning",
            );
        }
        Err(error) => crate::state::log(
            &app,
            format!("Could not check your DeadlyStream sign-in: {error:#}"),
            "warning",
        ),
    }
}
