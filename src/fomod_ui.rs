use eframe::egui;
use modsync::fomod::{ArchiveModule, ChoicesFile, visible_steps};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};

pub enum Outcome {
    Choices(ChoicesFile),
    Dismissed(String),
}

#[derive(Default)]
pub struct Editor {
    module: Option<ArchiveModule>,
    fallback_warnings: Vec<String>,
    filename: String,
    receiver: Option<Receiver<anyhow::Result<Option<ArchiveModule>>>>,
    import_receiver: Option<Receiver<Option<ChoicesFile>>>,
    imported_choices: Option<ChoicesFile>,
    cancel: Arc<AtomicBool>,
    error: String,
    open: bool,
    dismiss_non_fomod: bool,
    detected_fomod: bool,
    step_cursor: usize,
}

struct Prepared {
    module: Option<ArchiveModule>,
    imported_choices: Option<ChoicesFile>,
}

impl Editor {
    pub fn take_imported_choices(&mut self) -> Option<ChoicesFile> {
        self.imported_choices.take()
    }
    /// Includes asynchronous inspection and error dialogs, which may close without an Outcome.
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn dismissal(&self) -> Option<Outcome> {
        (self.dismiss_non_fomod && self.detected_fomod)
            .then(|| Outcome::Dismissed(self.filename.clone()))
    }

    pub fn launch(
        &mut self,
        path: std::path::PathBuf,
        options: modsync::archive::Options,
        saved: Option<modsync::fomod::ArchiveChoices>,
        dismiss_non_fomod: bool,
    ) {
        self.launch_bound(
            path,
            options,
            saved.into_iter().collect(),
            dismiss_non_fomod,
            None,
        );
    }

    pub fn launch_bound(
        &mut self,
        path: std::path::PathBuf,
        options: modsync::archive::Options,
        saved: Vec<modsync::fomod::ArchiveChoices>,
        dismiss_non_fomod: bool,
        sidecar: Option<std::path::PathBuf>,
    ) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.module = None;
        self.fallback_warnings.clear();
        self.imported_choices = None;
        self.step_cursor = 0;
        self.error.clear();
        self.open = true;
        self.dismiss_non_fomod = dismiss_non_fomod;
        self.detected_fomod = false;
        self.filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let (import_tx, import_rx) = mpsc::channel();
        self.import_receiver = Some(import_rx);
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Prepared> {
                let imported_choices = sidecar
                    .map(|path| -> anyhow::Result<ChoicesFile> {
                        anyhow::ensure!(
                            !cancel.load(Ordering::Relaxed),
                            "FOMOD preparation cancelled"
                        );
                        ChoicesFile::load(&path)
                    })
                    .transpose()?;
                let mut module = modsync::fomod::inspect_archive(&path, &options, &cancel)?;
                let filename = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                let external = imported_choices
                    .as_ref()
                    .and_then(|file| file.archive(filename))
                    .cloned();
                let candidates = external.map(|entry| vec![entry]).unwrap_or(saved);
                if let Some(module) = &mut module {
                    let mut normalized = None;
                    for saved in candidates {
                        let current =
                            modsync::fomod::current_archive_choices(module, &saved, filename)?;
                        if let Some(prior) = &normalized {
                            anyhow::ensure!(
                                serde_json::to_value(prior)? == serde_json::to_value(&current)?,
                                "Bound FOMOD consumers have different saved selections; configure them individually"
                            );
                        }
                        normalized = Some(current);
                    }
                    if let Some(saved) = normalized {
                        saved.apply_to(&mut module.config.steps)?;
                    }
                }
                Ok(Prepared {
                    module,
                    imported_choices,
                })
            })();
            let result = result.map(|prepared| {
                let _ = import_tx.send(prepared.imported_choices);
                prepared.module
            });
            let _ = tx.send(result);
        });
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        if !self.open {
            return None;
        }
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.receiver = None;
                    self.imported_choices = self
                        .import_receiver
                        .take()
                        .and_then(|receiver| receiver.try_recv().ok())
                        .flatten();
                    match result {
                        Ok(Some(module)) => {
                            self.detected_fomod = true;
                            self.fallback_warnings = module.config.fallback_warnings();
                            self.module = Some(module);
                        }
                        Ok(None) if self.dismiss_non_fomod => self.open = false,
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
        if !self.open {
            return None;
        }
        let mut outcome = None;
        egui::Modal::new(egui::Id::new("fomod_editor")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 48.0).clamp(180.0, 540.0));
            ui.heading(format!("Configure {}", self.filename));
            if self.receiver.is_some() {
                ui.spinner();
                ui.label("Reading archive…");
            }
            if let Some(module) = &mut self.module {
                ui.label(&module.config.name);
                let warnings = &self.fallback_warnings;
                if !warnings.is_empty() {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Installer compatibility warnings — review before Finish",
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("fomod_compatibility_warnings")
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for warning in warnings {
                                ui.label(warning);
                            }
                        });
                }
                let visible = visible_steps(&module.config.steps);
                self.step_cursor = self.step_cursor.min(visible.len().saturating_sub(1));
                if visible.is_empty() {
                    ui.label("This package has no optional install steps.");
                } else {
                    ui.label(format!(
                        "Step {} of {}",
                        self.step_cursor + 1,
                        visible.len()
                    ));
                }
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 180.0).clamp(60.0, 420.0))
                    .show(ui, |ui| {
                        for &index in visible.get(self.step_cursor).into_iter() {
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
                                            let response = ui.add_enabled(
                                                !plugin.required,
                                                egui::Checkbox::new(&mut selected, label),
                                            );
                                            if !plugin.description.trim().is_empty() {
                                                ui.label(&plugin.description);
                                            }
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
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.step_cursor > 0, egui::Button::new("Back"))
                        .clicked()
                    {
                        self.step_cursor -= 1;
                        self.error.clear();
                    }
                    let last = self.step_cursor + 1 >= visible.len();
                    if ui
                        .button(if last { "Use these choices" } else { "Next" })
                        .clicked()
                    {
                        match advance_step(&module.config.steps, &mut self.step_cursor) {
                            Ok(false) => self.error.clear(),
                            Ok(true) => {
                                match ChoicesFile::capture(&self.filename, &module.config.steps) {
                                    Ok(choices) => {
                                        outcome = Some(Outcome::Choices(choices));
                                        self.open = false;
                                    }
                                    Err(error) => self.error = error.to_string(),
                                }
                            }
                            Err(error) => self.error = error.to_string(),
                        }
                    }
                });
            }
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::RED, &self.error);
            }
            if ui.button("Cancel").clicked() {
                self.cancel.store(true, Ordering::Relaxed);
                self.receiver = None;
                self.module = None;
                self.open = false;
                outcome = self.dismissal();
            }
        });
        outcome
    }
}
// Recompute after edits: a flag change can hide or reveal subsequent steps.
fn advance_step(
    steps: &[modsync::fomod::StepSelection],
    cursor: &mut usize,
) -> anyhow::Result<bool> {
    let visible = visible_steps(steps);
    *cursor = (*cursor).min(visible.len().saturating_sub(1));
    if let Some(&index) = visible.get(*cursor) {
        for group in &steps[index].groups {
            group.validate()?;
        }
    }
    if *cursor + 1 < visible.len() {
        *cursor += 1;
        Ok(false)
    } else {
        Ok(true)
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_cancel_only_records_a_confirmed_fomod() {
        let mut editor = Editor::default();
        editor.filename = "package.zip".into();
        editor.dismiss_non_fomod = true;
        assert!(editor.dismissal().is_none());
        editor.detected_fomod = true;
        assert!(matches!(
            editor.dismissal(),
            Some(Outcome::Dismissed(filename)) if filename == "package.zip"
        ));
        editor.dismiss_non_fomod = false;
        assert!(editor.dismissal().is_none());
    }

    #[test]
    fn navigation_validates_current_step_and_recomputes_flag_visibility() {
        let mut steps = modsync::fomod::parse_selection_xml(r#"
            <config><installSteps>
              <installStep name="First"><optionalFileGroups><group name="Choice" type="SelectExactlyOne">
                <plugins><plugin name="Show"><conditionFlags><flag name="extra">yes</flag></conditionFlags></plugin>
                <plugin name="Hide"/></plugins>
              </group></optionalFileGroups></installStep>
              <installStep name="Conditional"><visible><flagDependency flag="extra" value="yes"/></visible></installStep>
              <installStep name="Last"/>
            </installSteps></config>"#).unwrap();
        let mut cursor = 0;
        assert!(!advance_step(&steps, &mut cursor).unwrap());
        assert_eq!(visible_steps(&steps)[cursor], 1);
        // Back and revise the first step: the next action must skip the hidden step.
        cursor -= 1;
        steps[0].groups[0].set_selected(1, true).unwrap();
        assert!(!advance_step(&steps, &mut cursor).unwrap());
        assert_eq!(visible_steps(&steps)[cursor], 2);
        assert!(advance_step(&steps, &mut cursor).unwrap());
        cursor = 0;
        steps[0].groups[0].set_selected(1, false).unwrap();
        assert!(advance_step(&steps, &mut cursor).is_err());
        assert_eq!(cursor, 0);
        assert!(advance_step(&[], &mut cursor).unwrap());
    }

    #[test]
    fn replacing_inspection_cancels_old_worker_and_rejects_its_late_result() {
        let scratch = tempfile::tempdir().unwrap();
        let (old_sender, old_receiver) = mpsc::channel();
        let mut editor = Editor::default();
        editor.receiver = Some(old_receiver);
        editor.open = true;
        let old_cancel = editor.cancel.clone();

        editor.launch(
            scratch.path().join("missing.zip"),
            modsync::archive::Options::default(),
            None,
            false,
        );

        assert!(old_cancel.load(Ordering::Relaxed));
        assert!(old_sender.send(Ok(None)).is_err());
        assert!(!editor.cancel.load(Ordering::Relaxed));
        assert_eq!(editor.filename, "missing.zip");
        let result = editor
            .receiver
            .as_ref()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("replacement worker must report its own result");
        assert!(result.is_err(), "the replacement archive does not exist");

        let current_cancel = editor.cancel.clone();
        drop(editor);
        assert!(current_cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn automatic_inspection_closes_for_ordinary_archives_but_keeps_errors_visible() {
        let ctx = egui::Context::default();
        for failed in [false, true] {
            let (tx, rx) = mpsc::channel();
            tx.send(if failed {
                Err(anyhow::anyhow!("corrupt archive"))
            } else {
                Ok(None)
            })
            .unwrap();
            let mut editor = Editor::default();
            editor.receiver = Some(rx);
            editor.open = true;
            editor.dismiss_non_fomod = true;
            let _ = ctx.run(Default::default(), |ctx| {
                assert!(editor.show(ctx).is_none());
            });
            assert_eq!(editor.open, failed);
            assert!(editor.receiver.is_none());
            if failed {
                assert!(editor.error.contains("corrupt archive"));
            }
        }
    }
}
