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
