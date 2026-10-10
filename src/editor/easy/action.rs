//! Typed author actions. Every easy-mode surface (CLI, GUI, scripts) builds an
//! `Action`; it becomes one instruction and one durable, undoable revision.
use crate::Instruction;
use serde::{Deserialize, Serialize};

/// One step a guide author performs, in the vocabulary of a manual install.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    Extract { archive: String },
    Copy { from: Vec<String>, to: String, #[serde(default = "yes")] overwrite: bool },
    Move { from: Vec<String>, to: String, #[serde(default = "yes")] overwrite: bool },
    Rename { from: String, to: String, #[serde(default = "yes")] overwrite: bool },
    Delete { path: Vec<String> },
    /// Run the patcher shipped in the mod folder. `arguments` selects the namespace
    /// or option exactly as the author chose it.
    Patcher { folder: String, #[serde(default)] arguments: String },
}

fn yes() -> bool { true }

impl Action {
    pub fn label(&self) -> String {
        match self {
            Self::Extract { archive } => format!("Extract {archive}"),
            Self::Copy { from, to, .. } => format!("Copy {} to {to}", from.join(", ")),
            Self::Move { from, to, .. } => format!("Move {} to {to}", from.join(", ")),
            Self::Rename { from, to, .. } => format!("Rename {from} to {to}"),
            Self::Delete { path } => format!("Delete {}", path.join(", ")),
            Self::Patcher { folder, .. } => format!("Run patcher in {folder}"),
        }
    }

    pub fn instruction(&self) -> Instruction {
        let (action, source, destination, arguments, overwrite) = match self {
            Self::Extract { archive } => ("Extract", vec![archive.clone()], String::new(), String::new(), true),
            Self::Copy { from, to, overwrite } => ("Copy", from.clone(), to.clone(), String::new(), *overwrite),
            Self::Move { from, to, overwrite } => ("Move", from.clone(), to.clone(), String::new(), *overwrite),
            Self::Rename { from, to, overwrite } => ("Rename", vec![from.clone()], to.clone(), String::new(), *overwrite),
            Self::Delete { path } => ("Delete", path.clone(), String::new(), String::new(), true),
            Self::Patcher { folder, arguments } => ("Patcher", vec![folder.clone()], String::new(), arguments.clone(), true),
        };
        Instruction {
            guid: uuid::Uuid::new_v4().to_string(), parent: String::new(), action: action.into(),
            source, destination, arguments, overwrite, dependencies: vec![], restrictions: vec![],
        }
    }
}
