use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    println!("cargo:rerun-if-changed=src/development_reload.h");
    CxxQtBuilder::new_qml_module(QmlModule::new("org.modsync.desktop").qml_file("qml/Main.qml"))
        .include_dir("src")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("Network")
        .file("src/backend.rs")
        .qrc_resources([
            "assets/fonts/NotoSans-Regular.ttf",
            "assets/fonts/NotoSans-Bold.ttf",
            "assets/fonts/LICENSE.txt",
        ])
        .build();
}
