//! Smaller routes: opening web pages, mod page details and screenshots, app
//! updates and release notes, DeadlyStream sign-in, and the window commands
//! the interface calls directly.
use crate::state::{Reply, fail};
use serde_json::json;
use tauri::AppHandle;

/// Where releases of this app are published.
pub const UPDATE_REPO: &str = "ChristopherVR/ModSync.rs";

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

#[tauri::command]
pub fn set_nxm_handler() {}

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
            {
                let mut inner = st.lock();
                inner.signed_in = true;
                inner.ui.deadlystream_username = account.username.clone();
            }
            st.save_ui();
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
