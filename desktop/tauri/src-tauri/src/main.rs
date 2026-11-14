// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod builds;
mod extras;
mod library;
mod pipeline;
mod prefs;
mod state;

use tauri::Manager;

fn main() {
    let app_state = state::AppState::load();
    let _ = api::ensure_default_profiles(&app_state);
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state);
    let builder = builder
        .register_asynchronous_uri_scheme_protocol("kmm-image", |_ctx, request, responder| {
            extras::image_protocol(request, responder)
        });
    builder
        .on_window_event(|window, event| {
            // Closing mid-install would stop the engine between steps. Ask the
            // install to stop instead and keep the window open until it has.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.state::<state::AppState>().request_stop()
            {
                api.prevent_close();
                state::log(
                    window.app_handle(),
                    "Stopping the install safely. Close the window again once it has stopped.",
                    "warning",
                );
            }
        })
        .invoke_handler(tauri::generate_handler![
            api::api_request,
            extras::open_gamefront,
            extras::close_gamefront,
            extras::finish_gamefront,
            extras::gamefront_account,
            extras::sign_out_gamefront,
            extras::set_nxm_handler,
        ])
        .run(tauri::generate_context!())
        .expect("the KOTOR Mod Installer window could not start");
}
