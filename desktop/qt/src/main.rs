mod backend;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn main() {
    backend::ffi::QQuickStyle::use_shared_style(&QString::from("Basic"));
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    let failed = Arc::new(AtomicBool::new(false));
    let load_failed = Arc::clone(&failed);
    let mut engine_ref = engine.as_mut().expect("Qt QML engine could not be created");
    let _connection = engine_ref
        .as_mut()
        .on_object_creation_failed(move |_, url| {
            eprintln!("Could not load ModSync interface: {url:?}");
            load_failed.store(true, Ordering::Relaxed);
        });
    let source = if cfg!(debug_assertions) {
        std::env::var_os("MODSYNC_QML_DEV_SOURCE")
            .map(|path| {
                let path = std::path::PathBuf::from(path)
                    .canonicalize()
                    .map_err(|error| format!("Development QML path: {error}"))?;
                let metadata = path.metadata().map_err(|error| error.to_string())?;
                if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
                    return Err(
                        "Development QML must be a regular file no larger than 8 MiB".into(),
                    );
                }
                let path = path.to_str().ok_or("Development QML path must be UTF-8")?;
                Ok::<_, String>(QUrl::from_local_file(&QString::from(path)))
            })
            .transpose()
    } else {
        Ok(None)
    }
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    if let Some(source) = source {
        backend::ffi::install_development_reload(engine_ref.as_mut(), &source);
        engine_ref.load(&source);
    } else {
        engine_ref.load(&QUrl::from("qrc:/qt/qml/org/modsync/desktop/qml/Main.qml"));
    }
    if failed.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
