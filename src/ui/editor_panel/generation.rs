//! Local archive and provider filename inference stays an identity-bound, explicitly adopted draft.
use super::{State as EditorState, Work};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::{Component, Guide, archive, archive_generation::resolved, editor};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub(super) struct State {
    replace: bool,
    providers: bool,
    pub(super) preview: Option<Preview>,
}
pub(super) struct Request {
    draft: Component,
    fingerprint: String,
    replace: bool,
    providers: bool,
    root: Option<PathBuf>,
    key: Option<String>,
    providers_allowed: bool,
}
pub(crate) struct Preview {
    pub(super) fingerprint: String,
    candidate: Component,
    text: String,
    diagnostics: String,
}

fn parsed(state: &EditorState) -> Result<Component> {
    ensure!(
        state.resources.len() <= 8 * 1024 * 1024 && state.metadata.len() <= 8 * 1024 * 1024,
        "Draft JSON exceeds the 8 MiB limit"
    );
    let mut draft = state
        .draft
        .clone()
        .context("Select a component or option first")?;
    draft.resource_registry =
        serde_json::from_str(&state.resources).context("Invalid resource JSON")?;
    draft.metadata = serde_json::from_str(&state.metadata).context("Invalid metadata JSON")?;
    Ok(draft)
}

pub(super) fn fingerprint(
    state: &EditorState,
    guide: &Guide,
    context: &str,
    mods: &str,
    key: Option<&str>,
    providers_allowed: bool,
) -> Result<String> {
    ensure!(
        mods.len() <= 32768,
        "Mod directory path exceeds 32768 bytes"
    );
    let identity = modsync::updates::guide_identity(guide)?;
    ensure!(
        identity == state.identity,
        "Guide changed; reload the component draft"
    );
    editor::render_preview(&(
        context,
        identity,
        &state.guid,
        parsed(state)?,
        &state.resources,
        &state.metadata,
        state.generation.replace,
        state.generation.providers,
        providers_allowed,
        mods,
        key.map(|key| format!("{:x}", Sha256::digest(key.as_bytes()))),
    ))
}

pub(super) fn run(mut request: Request, cancelled: &AtomicBool) -> Result<Preview> {
    let mut guide = Guide {
        components: vec![request.draft.clone()],
        ..Default::default()
    };
    let (files, root) = if request.providers {
        ensure!(
            request.providers_allowed,
            "Provider generation is unavailable in isolated probe launches"
        );
        let report = modsync::download_orchestrator::filenames::resolve(
            &guide,
            request.key.take(),
            cancelled,
        )
        .map_err(|_| {
            anyhow::anyhow!("Provider filename discovery failed; no candidate was produced")
        })?;
        ensure!(
            !cancelled.load(Ordering::Acquire) && !report.cancelled,
            "Provider filename discovery cancelled"
        );
        ensure!(
            report.diagnostics.is_empty(),
            "Provider filename discovery is incomplete: {}",
            editor::render_preview(&report.diagnostics)?
        );
        ensure!(
            !report.resolved.is_empty()
                && report
                    .resolved
                    .iter()
                    .any(|item| !item.filenames.is_empty()),
            "No provider filenames were resolved; no candidate was produced"
        );
        (report.resolved, request.root.take())
    } else {
        let path = rfd::FileDialog::new()
            .add_filter("Local archives", &["zip", "7z", "rar", "exe", "tar", "gz"])
            .pick_file()
            .context("Archive selection cancelled")?;
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Archive generation cancelled"
        );
        let text = path.to_str().context("Archive path must be UTF-8")?;
        ensure!(text.len() <= 32768, "Archive path exceeds 32768 bytes");
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("Archive filename missing")?;
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            matches!(
                extension.as_str(),
                "zip" | "7z" | "rar" | "exe" | "tar" | "gz"
            ),
            "Select a supported local archive"
        );
        // The shared generator additionally rejects symlink/reparse ancestors and leafs.
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Archive must be an ordinary file"
        );
        ensure!(
            metadata.len() <= 256 * 1024 * 1024,
            "Local generation limits archive input to 256 MiB"
        );
        (
            vec![resolved::ResolvedFiles {
                component_guid: request.draft.guid.clone(),
                filenames: vec![filename.into()],
            }],
            Some(
                path.parent()
                    .context("Archive parent missing")?
                    .to_path_buf(),
            ),
        )
    };
    if request.replace {
        request.draft.instructions.clear();
    }
    guide.components[0] = request.draft;
    let reports = resolved::generate(
        &mut guide,
        &files,
        root.as_deref(),
        &archive::Options {
            max_expanded_bytes: Some(256 * 1024 * 1024),
            ..Default::default()
        },
        cancelled,
    )?;
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Archive generation cancelled"
    );
    let candidate = guide
        .components
        .pop()
        .context("Generated component missing")?;
    let text = editor::render_preview(&candidate)?;
    let diagnostics = editor::render_preview(&reports)?;
    // Pre-render both adoption fields so adoption cannot partially change the draft.
    editor::render_preview(&candidate.resource_registry)?;
    editor::render_preview(&candidate.metadata)?;
    Ok(Preview {
        fingerprint: request.fingerprint,
        candidate,
        text,
        diagnostics,
    })
}

pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    guide: &Guide,
    context: &str,
    mods: &str,
    key: Option<String>,
    providers_allowed: bool,
) -> Option<Work> {
    if state.draft.is_none() {
        state.generation.preview = None;
        return None;
    }
    let mut work = None;
    ui.collapsing("Auto-generate instructions", |ui| {
        ui.label("Infer instructions from one local archive (256 MiB input and expanded-content limit). Nothing is installed or saved. Generated instructions require review. Archive filenames in instructions refer to the mod directory; generation does not copy the archive there.");
        ui.radio_value(&mut state.generation.providers, false, "Local archive");
        ui.radio_value(&mut state.generation.providers, true, "Provider link filenames");
        if state.generation.providers && !providers_allowed {
            ui.label("Provider generation is unavailable in isolated probe launches. Local archive generation remains available.");
        }
        if state.generation.providers {
            ui.label("On request, contact providers for filename metadata from this draft's resource links. No archives are downloaded. Cached archives in the current mod directory are inspected when present; otherwise filenames produce reviewed inference. Incomplete discovery produces no candidate.");
        }
        ui.radio_value(&mut state.generation.replace, false, "Append inferred instructions");
        ui.radio_value(&mut state.generation.replace, true, "Replace instructions only (retain options and review history)");
        let stamp = fingerprint(state, guide, context, mods, key.as_deref(), providers_allowed);
        if state.generation.preview.as_ref().is_some_and(|p| stamp.as_ref().map_or(true, |s| s != &p.fingerprint)) {
            state.generation.preview = None;
        }
        if ui.add_enabled(stamp.is_ok() && (!state.generation.providers || providers_allowed), egui::Button::new(if state.generation.providers { "Resolve provider filenames and preview" } else { "Choose local archive and preview" })).clicked() {
            match parsed(state).and_then(|draft| Ok(Request { draft, fingerprint: stamp.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))?.clone(), replace: state.generation.replace, providers: state.generation.providers, root: (!mods.trim().is_empty()).then(|| PathBuf::from(mods)), key: key.clone(), providers_allowed })) {
                Ok(request) => {
                    state.generation.preview = None;
                    work = Some(Work::Generate(Box::new(request)));
                }
                Err(error) => state.status = format!("Generation unavailable: {error:#}"),
            }
        }
        if let Err(error) = stamp { ui.label(format!("Generation unavailable: {error:#}")); }
        let mut adopt = false;
        if let Some(preview) = &state.generation.preview {
            ui.label("Inference diagnostics");
            let mut diagnostics = preview.diagnostics.as_str();
            ui.add(egui::TextEdit::multiline(&mut diagnostics).desired_rows(5));
            ui.collapsing("Candidate including review provenance", |ui| {
                let mut text = preview.text.as_str();
                ui.add(egui::TextEdit::multiline(&mut text).desired_rows(12));
            });
            adopt = ui.button("Adopt candidate into editor draft").clicked();
        }
        if adopt {
            let result = (|| -> Result<_> {
                let preview = state.generation.preview.as_ref().context("Candidate missing")?;
                ensure!(fingerprint(state, guide, context, mods, key.as_deref(), providers_allowed)? == preview.fingerprint, "Draft changed; regenerate preview");
                Ok((preview.candidate.clone(), editor::render_preview(&preview.candidate.resource_registry)?, editor::render_preview(&preview.candidate.metadata)?))
            })();
            match result {
                Ok((draft, resources, metadata)) => {
                    state.draft = Some(draft);
                    state.resources = resources;
                    state.metadata = metadata;
                    state.instruction = 0;
                    state.generation.preview = None;
                    state.status = "Candidate adopted into the draft. Inspect it, then Apply to validate the guide; draft review remains required.".into();
                }
                Err(error) => state.status = format!("Candidate rejected: {error:#}"),
            }
        }
    });
    // A collapsed preview must also lose authority when another field changes.
    if state.generation.preview.as_ref().is_some_and(|p| {
        fingerprint(
            state,
            guide,
            context,
            mods,
            key.as_deref(),
            providers_allowed,
        )
        .as_ref()
        .map_or(true, |s| s != &p.fingerprint)
    }) {
        state.generation.preview = None;
    }
    work
}
