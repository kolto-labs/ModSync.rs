//! Read-only component cards. Actions return to the application's existing owners.
use super::rich_content;
use eframe::egui;
use modsync::Component;
use serde_json::Value;

#[derive(Default)]
pub(super) struct Actions {
    pub download: bool,
    pub jump_to_instruction: Option<usize>,
}

fn metadata<'a>(component: &'a Component, key: &str) -> Option<&'a Value> {
    component
        .metadata
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}
fn text(component: &Component, key: &str) -> String {
    match key {
        "Description" => component.description.clone(),
        "Directions" => component.directions.clone(),
        _ => metadata(component, key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
    }
}
fn projected(component: &Component, key: &str, spoiler_free: bool) -> String {
    if !spoiler_free {
        return text(component, key);
    }
    if key == "Description" {
        return modsync::selection::tooltip_description(component, true);
    }
    if let Some(value) = metadata(component, &format!("{key}SpoilerFree"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return value.into();
    }
    match key {
        "Directions" => {
            "Installation instructions available. Please review carefully before proceeding."
        }
        "DownloadInstructions" => {
            "Download instructions provided. Follow the steps to obtain the required files."
        }
        "UsageWarning" => "Please read the full warning before using this mod.",
        _ => "",
    }
    .into()
}
fn card(ui: &mut egui::Ui, label: &str, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    ui.group(|ui| {
        ui.strong(label);
        rich_content::show(ui, label, body);
    });
}
fn link(ui: &mut egui::Ui, value: &str) {
    match reqwest::Url::parse(value).ok().filter(|url| {
        matches!(url.scheme(), "https" | "http")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    }) {
        Some(url) => {
            ui.hyperlink_to(value, url.as_str())
                .on_hover_text(url.as_str());
        }
        None => {
            ui.label(value);
            ui.small("This is not an ordinary web link; automatic opening is unavailable.");
        }
    }
    if ui.small_button("Copy link").clicked() {
        ui.ctx().copy_text(value.into());
    }
}
fn links(ui: &mut egui::Ui, component: &Component) {
    let mut urls = std::collections::BTreeSet::new();
    urls.extend(component.resource_registry.keys().map(String::as_str));
    for key in ["ModLink", "DownloadLink"] {
        match metadata(component, key) {
            Some(Value::String(url)) => {
                urls.insert(url.as_str());
            }
            Some(Value::Array(values)) => {
                urls.extend(values.iter().filter_map(Value::as_str));
            }
            _ => {}
        }
    }
    for (index, url) in urls.into_iter().enumerate() {
        ui.push_id(index, |ui| link(ui, url));
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    component: &Component,
    spoiler_free: bool,
    editor_mode: bool,
) -> Actions {
    let mut actions = Actions::default();
    ui.push_id((&component.guid, "component-summary", spoiler_free), |ui| {
        ui.group(|ui| {
            ui.heading(modsync::selection::display_name(component, spoiler_free));
            for (label, value) in [
                ("Author", component.author.clone()),
                ("Method", text(component, "InstallationMethod")),
                ("Categories", component.category.join(", ")),
                ("Tier", component.tier.clone()),
            ] {
                if !value.trim().is_empty() {
                    ui.label(format!("{label}: {value}"));
                }
            }
            if let Some(language) = metadata(component, "Language") {
                let languages = match language {
                    Value::String(text) => text.clone(),
                    Value::Array(items) => items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => String::new(),
                };
                if !languages.is_empty() {
                    ui.label(format!("Languages: {languages}"));
                }
            }
            if editor_mode {
                ui.label(format!("GUID: {}", component.guid));
            }
            for (key, label) in [
                ("WidescreenOnly", "Widescreen only"),
                ("AspyrExclusive", "Aspyr only"),
            ] {
                if metadata(component, key).and_then(Value::as_bool) == Some(true) {
                    ui.strong(label);
                }
            }
        });
        if !component.resource_registry.is_empty()
            || metadata(component, "ModLink").is_some()
            || metadata(component, "DownloadLink").is_some()
        {
            ui.group(|ui| {
                ui.strong("Download links");
                actions.download = ui
                    .add_enabled(
                        component.is_selected,
                        egui::Button::new("Fetch this component's downloads"),
                    )
                    .on_disabled_hover_text("Select this component before fetching its downloads.")
                    .clicked();
                if spoiler_free {
                    ui.collapsing("Show download URLs (may contain spoilers)", |ui| {
                        links(ui, component)
                    });
                } else {
                    links(ui, component);
                }
            });
        }
        for (key, label) in [
            ("Description", "Description"),
            ("Directions", "Installation notes"),
            ("DownloadInstructions", "Download instructions"),
            ("UsageWarning", "Usage warning"),
            ("Screenshots", "Screenshots"),
        ] {
            let original = text(component, key);
            let alternate = metadata(component, &format!("{key}SpoilerFree"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !original.trim().is_empty() || (spoiler_free && !alternate.trim().is_empty()) {
                card(ui, label, &projected(component, key, spoiler_free));
            }
        }
        for (key, label) in [
            ("KnownBugs", "Known bugs"),
            ("InstallationWarning", "Installation warning"),
            ("CompatibilityWarning", "Compatibility warning"),
            ("SteamNotes", "Steam notes"),
        ] {
            card(ui, label, &text(component, key));
        }
        if !component.options.is_empty() {
            ui.collapsing("Option summary", |ui| {
                for option in &component.options {
                    ui.push_id(&option.guid, |ui| {
                        ui.label(format!(
                            "{} — {}",
                            modsync::selection::display_name(option, spoiler_free),
                            if option.is_selected {
                                "selected"
                            } else {
                                "not selected"
                            }
                        ));
                        card(
                            ui,
                            "Description",
                            &modsync::selection::tooltip_description(option, spoiler_free),
                        );
                        ui.label(format!(
                            "{} instructions; {} nested options",
                            option.instructions.len(),
                            option.options.len()
                        ));
                    });
                }
            });
        }
        // Authored source paths can reveal story names; require an explicit reveal.
        let label = if spoiler_free {
            "Installation instructions (may contain spoilers)"
        } else {
            "Installation instructions"
        };
        ui.collapsing(label, |ui| {
            for (index, instruction) in component.instructions.iter().enumerate() {
                ui.push_id(index, |ui| {
                    ui.collapsing(format!("{}. {}", index + 1, instruction.action), |ui| {
                        if editor_mode && ui.button("Jump to instruction in editor").clicked() {
                            actions.jump_to_instruction = Some(index);
                        }
                        for source in &instruction.source {
                            ui.label(format!("Source: {source}"));
                        }
                        if !instruction.destination.is_empty() {
                            ui.label(format!("Destination: {}", instruction.destination));
                        }
                        if !instruction.arguments.is_empty() {
                            ui.label(format!("Arguments: {}", instruction.arguments));
                        }
                        ui.label(format!("Overwrite: {}", instruction.overwrite));
                        if !instruction.dependencies.is_empty() {
                            ui.label(format!(
                                "Dependencies: {}",
                                instruction.dependencies.join(", ")
                            ));
                        }
                        if !instruction.restrictions.is_empty() {
                            ui.label(format!(
                                "Restrictions: {}",
                                instruction.restrictions.join(", ")
                            ));
                        }
                    });
                });
            }
        });
    });
    actions
}
