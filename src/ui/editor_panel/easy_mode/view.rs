//! Small, private view context; recipe revisions remain owned by Session.
use super::{Action, App};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct View {
    format: u32,
    session: PathBuf,
    component: String,
    instructions: usize,
    style: usize,
    folder: PathBuf,
}

impl App {
    pub(in crate::ui) fn queue_easy_resume(&mut self, directory: PathBuf) {
        self.editor.easy.directory = directory.to_string_lossy().into_owned();
        self.editor.easy.pending_resume = Some(directory);
        self.editor.easy.enabled = true;
        self.utility = true;
        self.page = 3;
    }

    pub(in crate::ui) fn poll_easy_resume(&mut self, ctx: &egui::Context) {
        if self.editor.easy.pending_resume.is_none()
            || !self.editor.settled()
            || self.receiver.is_some()
            || self.desktop.busy()
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
            || !self.sharing.ready()
        {
            return;
        }
        if let Some(directory) = self.editor.easy.pending_resume.take()
            && let Err(error) = self.easy_work(Action::Open(directory), ctx)
        {
            self.editor.easy.status = format!("Could not reopen the saved session: {error:#}");
        }
    }

    fn easy_view_path(&self, session: &std::path::Path) -> Result<PathBuf> {
        let profile = std::fs::canonicalize(&self.profile_directory)
            .context("Settings folder is unavailable")?;
        let directory = profile.join("authoring-views");
        for root in [&self.game, &self.mods]
            .into_iter()
            .filter(|value| !value.is_empty())
        {
            let root = crate::ui::canonical_with_missing_tail(std::path::Path::new(root));
            ensure!(
                !directory.starts_with(&root) && !root.starts_with(&directory),
                "View context overlaps an install root"
            );
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&directory) {
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Unsafe view context directory"
            );
        }
        let key = format!("{:x}", Sha256::digest(session.to_string_lossy().as_bytes()));
        Ok(directory.join(format!("{key}.json")))
    }

    pub(super) fn persist_easy_view(&mut self) {
        let Some(session) = self.editor.easy.session.as_ref() else {
            return;
        };
        let Some(component) = session.guide().components.get(self.editor.easy.component) else {
            return;
        };
        let view = View {
            format: 1,
            session: session.directory().to_owned(),
            component: component.guid.clone(),
            instructions: self
                .editor
                .easy
                .instructions
                .min(component.instructions.len()),
            style: self.editor.easy.style,
            folder: self.editor.easy.folder.clone(),
        };
        let result = (|| -> Result<()> {
            let bytes = serde_json::to_vec_pretty(&view)?;
            if self.editor.easy.saved_view.as_ref() == Some(&bytes) {
                return Ok(());
            }
            // Retry only when the view changes, rather than blocking every frame
            // on a missing/full settings volume. Recipe autosave is independent.
            self.editor.easy.saved_view = Some(bytes.clone());
            let path = self.easy_view_path(&view.session)?;
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                ensure!(
                    metadata.is_file() && !metadata.file_type().is_symlink(),
                    "Unsafe view context file"
                );
            }
            modsync::recovery::atomic_write(&path, &bytes)
        })();
        if let Err(error) = result {
            self.logs
                .push(format!("Editor view could not be saved: {error:#}"));
        }
    }

    pub(super) fn restore_easy_view(&mut self) {
        let result = (|| -> Result<()> {
            let session = self
                .editor
                .easy
                .session
                .as_ref()
                .context("No authoring session")?;
            self.editor.easy.directory = session.directory().to_string_lossy().into_owned();
            let path = self.easy_view_path(session.directory())?;
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 16384,
                "Invalid saved editor view"
            );
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(16385)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 16384, "Saved editor view grew while reading");
            let view: View = serde_json::from_slice(&bytes)?;
            ensure!(
                view.format == 1 && view.session == session.directory() && view.style <= 2,
                "Saved editor view belongs to another session or version"
            );
            self.editor.easy.component = session
                .guide()
                .components
                .iter()
                .position(|component| component.guid.eq_ignore_ascii_case(&view.component))
                .unwrap_or(0);
            self.editor.easy.instructions = session
                .guide()
                .components
                .get(self.editor.easy.component)
                .map_or(0, |component| {
                    view.instructions.min(component.instructions.len())
                });
            self.editor.easy.style = view.style;
            if view.folder.as_os_str().is_empty()
                || [&self.game, &self.mods]
                    .into_iter()
                    .filter(|value| !value.is_empty())
                    .any(|root| {
                        view.folder
                            .starts_with(crate::ui::canonical_with_missing_tail(
                                std::path::Path::new(root),
                            ))
                    })
            {
                self.editor.easy.folder = view.folder;
            }
            self.editor.easy.saved_view = None;
            Ok(())
        })();
        if let Err(error) = result {
            self.logs
                .push(format!("Saved editor view was not restored: {error:#}"));
        }
    }
}
