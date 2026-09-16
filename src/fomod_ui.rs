use eframe::egui;
use modsync::fomod::{ArchiveModule, ChoicesFile, visible_steps};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};

#[derive(Default)]
pub struct Editor {
    module: Option<ArchiveModule>,
    filename: String,
    receiver: Option<Receiver<anyhow::Result<Option<ArchiveModule>>>>,
    cancel: Arc<AtomicBool>,
    error: String,
    open: bool,
}
impl Editor {
    pub fn launch(&mut self, path: std::path::PathBuf, options: modsync::archive::Options) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.module = None;
        self.error.clear();
        self.open = true;
        self.filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(modsync::fomod::inspect_archive(&path, &options, &cancel));
        });
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<ChoicesFile> {
        if !self.open {
            return None;
        }
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.receiver = None;
                    match result {
                        Ok(Some(module)) => self.module = Some(module),
                        Ok(None) => self.error = "This archive has no FOMOD installer.".into(),
                        Err(error) => self.error = format!("{error:#}"),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.error = "Archive inspection stopped unexpectedly.".into();
                }
            }
        }
        let mut accepted = None;
        egui::Modal::new(egui::Id::new("fomod_editor")).show(ctx, |ui| {
            ui.set_min_width(540.0);
            ui.heading(format!("Configure {}", self.filename));
            if self.receiver.is_some() {
                ui.spinner();
                ui.label("Reading archive…");
            }
            if let Some(module) = &mut self.module {
                ui.label(&module.config.name);
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        for index in visible_steps(&module.config.steps) {
                            let step = &mut module.config.steps[index];
                            ui.push_id(index, |ui| {
                                ui.heading(&step.name);
                                for (group_index, group) in step.groups.iter_mut().enumerate() {
                                    ui.push_id(group_index, |ui| {
                                        ui.label(format!("{} ({:?})", group.name, group.kind));
                                        for plugin_index in 0..group.plugins().len() {
                                            let plugin = &group.plugins()[plugin_index];
                                            let mut selected = plugin.selected;
                                            let label = if plugin.required {
                                                format!("{} (required)", plugin.name)
                                            } else {
                                                plugin.name.clone()
                                            };
                                            let response = ui
                                                .checkbox(&mut selected, label)
                                                .on_hover_text(&plugin.description);
                                            if response.changed() {
                                                self.error.clear();
                                                if let Err(error) =
                                                    group.set_selected(plugin_index, selected)
                                                {
                                                    self.error = error.to_string();
                                                }
                                            }
                                        }
                                        if let Err(error) = group.validate() {
                                            ui.label(error.to_string());
                                        }
                                    });
                                }
                            });
                        }
                    });
                if ui.button("Use these choices").clicked() {
                    match ChoicesFile::capture(&self.filename, &module.config.steps) {
                        Ok(choices) => {
                            accepted = Some(choices);
                            self.open = false;
                        }
                        Err(error) => self.error = error.to_string(),
                    }
                }
            }
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::RED, &self.error);
            }
            if ui.button("Cancel").clicked() {
                self.cancel.store(true, Ordering::Relaxed);
                self.receiver = None;
                self.module = None;
                self.open = false;
            }
        });
        accepted
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
