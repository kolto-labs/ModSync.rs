#[cfg(feature = "ui-qt")]
fn build_qt() {
    use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
    println!("cargo:rerun-if-changed=src/ui_qt/development_reload.h");
    CxxQtBuilder::new_qml_module(QmlModule::new("org.modsync.desktop").qml_files([
        QmlFile::from("src/ui_qt/qml/Main.qml"),
        QmlFile::from("src/ui_qt/qml/EasyBuilder.qml"),
        QmlFile::from("src/ui_qt/qml/WizardView.qml"),
        QmlFile::from("src/ui_qt/qml/SelectionView.qml"),
        QmlFile::from("src/ui_qt/qml/ValidationView.qml"),
        QmlFile::from("src/ui_qt/qml/InstallModal.qml"),
        QmlFile::from("src/ui_qt/qml/StopModal.qml"),
        QmlFile::from("src/ui_qt/qml/ActionButton.qml"),
        QmlFile::from("src/ui_qt/qml/Palette.qml").singleton(true),
    ]))
    .include_dir("src/ui_qt")
    .qt_module("Quick")
    .qt_module("QuickControls2")
    .qt_module("Network")
    .file("src/ui_qt/backend.rs")
    .file("src/ui_qt/wizard.rs")
    .qrc_resources([
        "src/ui_qt/assets/fonts/NotoSans-Regular.ttf",
        "src/ui_qt/assets/fonts/NotoSans-Bold.ttf",
        "src/ui_qt/assets/fonts/LICENSE.txt",
    ])
    .build();
}

fn main() {
    println!("cargo:rerun-if-env-changed=MODSYNC_UI");
    let selected = if std::env::var_os("CARGO_FEATURE_UI_QT").is_some() {
        Some("qt")
    } else if std::env::var_os("CARGO_FEATURE_UI_EGUI").is_some() {
        Some("egui")
    } else {
        None
    };
    if let Ok(requested) = std::env::var("MODSYNC_UI") {
        let requested = requested.trim().to_ascii_lowercase();
        if !matches!(requested.as_str(), "" | "qt" | "egui") {
            println!("cargo:warning=MODSYNC_UI must be qt or egui, not {requested}");
        } else if !requested.is_empty() && selected.is_some_and(|actual| actual != requested) {
            println!(
                "cargo:warning=MODSYNC_UI={requested} does not choose the toolkit; this build uses {}. Use tools/build.sh or select --features ui-{requested} (with --no-default-features for egui).",
                selected.unwrap_or("none")
            );
        }
    }
    #[cfg(feature = "ui-qt")]
    build_qt();
}
