extern crate self as modsync;

#[cfg(all(feature = "ui-qt", feature = "ui-egui"))]
compile_error!(
    "Enable exactly one GUI toolkit: ui-qt (default) or ui-egui (--no-default-features --features ui-egui)."
);

pub mod acquisition;
pub mod acquisition_guard;
pub mod archive;
pub mod archive_generation;
pub mod deployment;
pub mod direct_markdown;
pub mod download;
pub mod download_hosts;
pub mod download_orchestrator;
pub mod download_queue;
pub mod draft;
pub mod editor;
pub mod engine;
pub mod fomod_cli;
pub mod game;
pub mod host_metadata;
pub mod install_progress;
pub mod merge;
pub mod model;
pub mod p2p;
pub mod patcher;
pub mod paths;
pub mod peer_acquire;
pub mod peer_sharing;
pub mod phase;
pub mod recovery;
pub mod validation;

pub use engine::{Engine, Event, PatcherEngine, Settings};
pub use model::{Component, Guide, Instruction};

mod markdown;
pub use markdown::spoiler;
mod xml;

mod resources;

pub mod download_plan;
mod reference_paths;

pub mod profiles;
pub mod protocol;
pub mod protocol_registration;
pub mod single_instance;

pub mod logging;
pub mod maintenance;
pub mod selection;
pub mod telemetry;
pub mod updates;

pub mod fomod;

pub mod checkpoint;
pub mod conflicts;
pub mod ui_backend;
#[cfg(feature = "ui-qt")]
pub mod ui_qt;
pub mod wizard;

#[cfg(target_os = "macos")]
pub mod native_url;

mod guid;
mod json_native;
pub mod resource_watcher;
mod unicode_case;
