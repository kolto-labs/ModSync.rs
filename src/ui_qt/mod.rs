mod backend;

use crate::ui_backend::{self, LaunchRequest, PreparedLaunch, Toolkit, UiBackend};
use anyhow::{Result, bail};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const MAIN_QML: &str = "qrc:/qt/qml/org/modsync/desktop/src/ui_qt/qml/EasyBuilder.qml";

pub struct QtBackend;

impl UiBackend for QtBackend {
    const TOOLKIT: Toolkit = Toolkit::Qt;

    fn run(request: LaunchRequest) -> Result<()> {
        let Some(prepared) = ui_backend::prepare(request)? else {
            return Ok(());
        };
        run_application(prepared)
    }
}

fn development_source() -> Result<Option<QUrl>> {
    if !cfg!(debug_assertions) {
        return Ok(None);
    }
    let Some(path) = std::env::var_os("MODSYNC_QML_DEV_SOURCE") else {
        return Ok(None);
    };
    let path = std::path::PathBuf::from(path)
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("Development QML path: {error}"))?;
    let metadata = path.metadata()?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        bail!("Development QML must be a regular file no larger than 8 MiB");
    }
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Development QML path must be UTF-8"))?;
    Ok(Some(QUrl::from_local_file(&QString::from(path))))
}

fn run_application(prepared: PreparedLaunch) -> Result<()> {
    let _prepared = prepared;
    let source = development_source()?;
    backend::ffi::QQuickStyle::use_shared_style(&QString::from("Basic"));
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    let failed = Arc::new(AtomicBool::new(false));
    let load_failed = Arc::clone(&failed);
    let mut engine_ref = engine
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("Qt QML engine could not be created"))?;
    let _connection = engine_ref
        .as_mut()
        .on_object_creation_failed(move |_, url| {
            eprintln!("Could not load ModSync interface: {url:?}");
            load_failed.store(true, Ordering::Relaxed);
        });
    if let Some(source) = source {
        backend::ffi::install_development_reload(engine_ref.as_mut(), &source);
        engine_ref.load(&source);
    } else {
        engine_ref.load(&QUrl::from(MAIN_QML));
    }
    if failed.load(Ordering::Relaxed) {
        bail!("The Qt interface could not be loaded");
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
    Ok(())
}
