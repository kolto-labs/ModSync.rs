//! Install steps shipped with the app for the community builds, for mods the
//! build only describes in words. Each overlay (`resources/overlays/<build>.toml`,
//! see the README there) is a ModSync guide holding just those mods' steps,
//! merged into the build with the engine's merge (`merge::preview`) before the
//! build's prose is drafted, so those mods' steps are authored instead of
//! drafted. Drafting never replaces authored steps, and every other mod is
//! loaded exactly as before.
use crate::state::log;
use modsync::{Guide, merge};
use std::path::Path;
use tauri::AppHandle;

fn overlay(key: &str) -> Option<&'static str> {
    match key {
        "k1_full" => Some(include_str!("../../resources/overlays/k1_full.toml")),
        _ => None,
    }
}

/// Merge options: the build keeps everything it says about a mod (name, links,
/// description, order and so on); the overlay only adds steps, and mods that
/// are not in the build are left out.
fn options() -> merge::Options {
    merge::Options {
        use_existing_order: true,
        exclude_incoming_only: true,
        prefer_existing_name: true,
        prefer_existing_author: true,
        prefer_existing_description: true,
        prefer_existing_directions: true,
        prefer_existing_category: true,
        prefer_existing_tier: true,
        prefer_existing_installation_method: true,
        prefer_existing_resource_registry: true,
        ..Default::default()
    }
}

/// Read a community build for installing: its overlay merged in, then the
/// prose drafted for everything else (`Guide::load_for_install_preserving_selection`).
pub(crate) fn load(app: &AppHandle, path: &Path, key: &str) -> anyhow::Result<Guide> {
    let Some(text) = overlay(key) else {
        return Guide::load_for_install_preserving_selection(path);
    };
    let markdown = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"));
    let mut guide = Guide::load_preserving_selection(path)?;
    match Guide::parse(text, "toml").and_then(|incoming| {
        // Only mods the build has not given steps of its own.
        let mut incoming = incoming;
        incoming.components.retain(|o| {
            guide.components.iter().any(|c| {
                c.guid.eq_ignore_ascii_case(&o.guid)
                    && c.instructions.is_empty()
                    && c.options.is_empty()
            })
        });
        merge::preview(&guide, &incoming, &options()).map(|p| (p, incoming.components.len()))
    }) {
        Ok((preview, count))
            if preview.validation_error.is_none() && preview.warnings.is_empty() =>
        {
            guide.components = preview.merged.components;
            if count > 0 {
                log(
                    app,
                    format!("Added the app's install steps for {count} mod(s) in this build."),
                    "muted",
                );
            }
        }
        Ok((preview, _)) => log(
            app,
            format!(
                "The app's install steps for this build were not used: {}",
                preview
                    .validation_error
                    .unwrap_or_else(|| preview.warnings.join("; "))
            ),
            "warning",
        ),
        Err(error) => log(
            app,
            format!("The app's install steps for this build were not used: {error:#}"),
            "warning",
        ),
    }
    if markdown {
        modsync::draft::generate(&mut guide);
    }
    Ok(guide)
}
