// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod builds;
mod extras;
mod importing;
mod library;
mod nxm;
mod pipeline;
mod prefs;
mod secrets;
mod state;

use tauri::Manager;

fn main() {
    // A Nexus "Mod Manager Download" click while the app is open goes to the
    // open window, and this process ends.
    let link = nxm::startup_link();
    let (receiver, own_link, _instance) = match nxm::claim(link.as_deref()) {
        nxm::Start::Forwarded => return,
        nxm::Start::Primary(receiver, guard) => (Some(receiver), None, Some(guard)),
        nxm::Start::Alone(link) => (None, link, None),
    };
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
    let app = builder
        .setup(move |app| {
            // Give back nxm:// links held when the app last closed unexpectedly;
            // they are claimed again while a download waits for its click.
            nxm::release();
            let handle = app.handle().clone();
            std::thread::spawn(move || extras::restore_sign_in(handle));
            if let Some(receiver) = receiver {
                nxm::listen(app.handle().clone(), receiver);
            }
            if let Some(link) = own_link {
                let handle = app.handle().clone();
                std::thread::spawn(move || nxm::handle(&handle, &link));
            }
            // macOS delivers links as Apple Events to the running app.
            #[cfg(target_os = "macos")]
            if let Ok((receiver, guard)) = modsync::native_url::install() {
                // The handler must stay installed for the app's whole life.
                std::mem::forget(guard);
                nxm::listen(app.handle().clone(), receiver);
            }
            Ok(())
        })
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
            extras::apply_update,
        ])
        .build(tauri::generate_context!())
        .expect("the KOTOR Mod Installer window could not start");
    app.run(|_app, event| {
        // nxm:// links are for every Nexus game; never keep them after closing.
        if let tauri::RunEvent::Exit = event {
            nxm::release();
        }
    });
}
