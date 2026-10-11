//! Browser presentation uses the predicted inventory, never a paint-time disk scan.
use super::State;
use eframe::egui;
use modsync::engine::PredictedEntry;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub(super) fn show(ui: &mut egui::Ui, state: &mut State) {
    ui.separator();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("easy_browser_style")
            .selected_text(["Explorer", "Dolphin", "Finder"][state.style])
            .show_ui(ui, |ui| {
                for (index, name) in ["Explorer", "Dolphin", "Finder"].iter().enumerate() {
                    ui.selectable_value(&mut state.style, index, *name);
                }
            });
        let label = ui.label("Search this folder");
        ui.text_edit_singleline(&mut state.search)
            .labelled_by(label.id);
    });
    let Some(prediction) = &state.prediction else {
        ui.label("Choose a point in the build and show its predicted files to browse.");
        return;
    };
    let Some(roots) = &state.roots else {
        return;
    };
    let mut navigate = None;
    ui.horizontal(|ui| {
        if ui
            .selectable_label(state.folder.starts_with(&roots.game), "Game")
            .clicked()
        {
            navigate = Some(roots.game.clone());
        }
        if ui
            .selectable_label(state.folder.starts_with(&roots.mods), "Mods")
            .clicked()
        {
            navigate = Some(roots.mods.clone());
        }
        let root = if state.folder.starts_with(&roots.game) {
            &roots.game
        } else {
            &roots.mods
        };
        if ui
            .add_enabled(state.folder != *root, egui::Button::new("Up"))
            .clicked()
        {
            navigate = state.folder.parent().map(PathBuf::from);
        }
        ui.label(state.folder.display().to_string());
    });
    let query = state.search.to_lowercase();
    let mut entries: Vec<_> = prediction
        .entries
        .iter()
        .filter(|entry| {
            entry.path.parent() == Some(state.folder.as_path())
                && entry
                    .path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().to_lowercase().contains(&query))
        })
        .collect();
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.cmp(&b.path))
    });
    ui.label(format!(
        "{} items · {} selected · Predicted state",
        entries.len(),
        state.selected.len()
    ));
    if entries.is_empty() {
        ui.label("This folder has no matching predicted items.");
    }
    match state.style {
        1 => icons(ui, &entries, &mut state.selected, &mut navigate),
        2 => {
            let root = if state.folder.starts_with(&roots.game) {
                &roots.game
            } else {
                &roots.mods
            };
            let mut columns: Vec<_> = state
                .folder
                .ancestors()
                .take_while(|folder| folder.starts_with(root))
                .map(PathBuf::from)
                .collect();
            columns.reverse();
            let start = columns.len().saturating_sub(3);
            egui::ScrollArea::horizontal()
                .id_salt("easy_finder_columns")
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        for folder in &columns[start..] {
                            let mut children: Vec<_> = prediction
                                .entries
                                .iter()
                                .filter(|entry| {
                                    entry.path.parent() == Some(folder.as_path())
                                        && (folder != &state.folder
                                            || entry.path.file_name().is_some_and(|name| {
                                                name.to_string_lossy()
                                                    .to_lowercase()
                                                    .contains(&query)
                                            }))
                                })
                                .collect();
                            children.sort_by(|a, b| {
                                b.directory
                                    .cmp(&a.directory)
                                    .then_with(|| a.path.cmp(&b.path))
                            });
                            ui.allocate_ui_with_layout(
                                egui::vec2(230.0, 280.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.strong(
                                        folder.file_name().unwrap_or_default().to_string_lossy(),
                                    );
                                    details(
                                        ui,
                                        &children,
                                        &mut state.selected,
                                        &mut navigate,
                                        true,
                                        folder,
                                    );
                                },
                            );
                            ui.separator();
                        }
                    });
                });
        }
        _ => {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(150.0, 280.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.strong("Folders");
                        egui::ScrollArea::vertical()
                            .id_salt("easy_explorer_folders")
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for (name, root) in [("Game", &roots.game), ("Mods", &roots.mods)] {
                                    if ui.selectable_label(state.folder == *root, name).clicked() {
                                        navigate = Some(root.clone());
                                    }
                                    for entry in prediction.entries.iter().filter(|entry| {
                                        entry.directory
                                            && entry.path.parent() == Some(root.as_path())
                                    }) {
                                        if ui
                                            .selectable_label(
                                                state.folder == entry.path,
                                                format!(
                                                    "  {}",
                                                    entry
                                                        .path
                                                        .file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy()
                                                ),
                                            )
                                            .clicked()
                                        {
                                            navigate = Some(entry.path.clone());
                                        }
                                    }
                                }
                            });
                    },
                );
                ui.separator();
                ui.vertical(|ui| {
                    ui.columns(3, |columns| {
                        columns[0].strong("Name");
                        columns[1].strong("Type");
                        columns[2].strong("Size");
                    });
                    details(
                        ui,
                        &entries,
                        &mut state.selected,
                        &mut navigate,
                        false,
                        &state.folder,
                    );
                });
            });
        }
    }
    if let Some(folder) = navigate {
        state.folder = folder;
        state.selected.clear();
        state.search.clear();
    }
}

fn kind(entry: &PredictedEntry) -> &'static str {
    if entry.directory {
        "Folder"
    } else if !entry.content_known {
        "Patched content unknown"
    } else {
        "File"
    }
}

fn select(
    entry: &PredictedEntry,
    response: egui::Response,
    selected: &mut BTreeSet<PathBuf>,
    navigate: &mut Option<PathBuf>,
    column: bool,
) {
    if entry.directory && (response.double_clicked() || (column && response.clicked())) {
        *navigate = Some(entry.path.clone());
    } else if response.clicked() && !selected.remove(&entry.path) && selected.len() < 4096 {
        selected.insert(entry.path.clone());
    }
}

fn details(
    ui: &mut egui::Ui,
    entries: &[&PredictedEntry],
    selected: &mut BTreeSet<PathBuf>,
    navigate: &mut Option<PathBuf>,
    column: bool,
    folder: &Path,
) {
    let height = ui.text_style_height(&egui::TextStyle::Body) + 10.0;
    egui::ScrollArea::vertical()
        .id_salt(("easy_browser_details", column, folder))
        .max_height(280.0)
        .show_rows(ui, height, entries.len(), |ui, range| {
            for index in range {
                let entry = entries[index];
                let name = format!(
                    "{} {}",
                    if entry.directory { ">" } else { " " },
                    entry.path.file_name().unwrap_or_default().to_string_lossy()
                );
                let response = if column {
                    ui.add_sized(
                        [230.0, height],
                        egui::Button::selectable(selected.contains(&entry.path), name)
                            .wrap_mode(egui::TextWrapMode::Truncate),
                    )
                } else {
                    ui.columns(3, |columns| {
                        let width = columns[0].available_width();
                        let response = columns[0].add_sized(
                            [width, height],
                            egui::Button::selectable(selected.contains(&entry.path), name)
                                .wrap_mode(egui::TextWrapMode::Truncate),
                        );
                        columns[1].label(kind(entry));
                        columns[2].label(
                            entry
                                .bytes
                                .map_or_else(String::new, |bytes| format!("{bytes} bytes")),
                        );
                        response
                    })
                }
                .on_hover_text(format!(
                    "{}\n{}",
                    entry.path.display(),
                    kind(entry)
                ));
                select(entry, response, selected, navigate, column);
            }
        });
}

fn icons(
    ui: &mut egui::Ui,
    entries: &[&PredictedEntry],
    selected: &mut BTreeSet<PathBuf>,
    navigate: &mut Option<PathBuf>,
) {
    let columns = ((ui.available_width() / 180.0).floor() as usize).clamp(1, 8);
    let width = (ui.available_width() / columns as f32 - 8.0).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("easy_dolphin_icons")
        .max_height(280.0)
        .show_rows(ui, 86.0, entries.len().div_ceil(columns), |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    for entry in entries.iter().skip(row * columns).take(columns) {
                        let name = format!(
                            "\n\n{}",
                            entry.path.file_name().unwrap_or_default().to_string_lossy()
                        );
                        let response = ui
                            .add_sized(
                                [width, 78.0],
                                egui::Button::selectable(selected.contains(&entry.path), name),
                            )
                            .on_hover_text(format!("{}\n{}", entry.path.display(), kind(entry)));
                        paint_icon(ui.painter(), response.rect, entry.directory);
                        select(entry, response, selected, navigate, false);
                    }
                });
            }
        });
}

fn paint_icon(painter: &egui::Painter, tile: egui::Rect, directory: bool) {
    let center = egui::pos2(tile.center().x, tile.top() + 23.0);
    let rect = egui::Rect::from_center_size(center, egui::vec2(34.0, 24.0));
    if directory {
        let tab = egui::Rect::from_min_size(rect.min - egui::vec2(0.0, 5.0), egui::vec2(15.0, 9.0));
        painter.rect_filled(tab, 3.0, egui::Color32::from_rgb(191, 143, 42));
        painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(235, 185, 74));
        painter.hline(
            rect.x_range().shrink(3.0),
            rect.top() + 5.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 218, 133)),
        );
    } else {
        let rect = egui::Rect::from_center_size(center, egui::vec2(23.0, 29.0));
        painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(223, 234, 245));
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(102, 137, 168)),
            egui::StrokeKind::Inside,
        );
        for offset in [9.0, 14.0, 19.0] {
            painter.hline(
                rect.x_range().shrink(5.0),
                rect.top() + offset,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(102, 137, 168)),
            );
        }
    }
}
