//! Owned managed-deployment inventory and explicitly confirmed mutations.
use super::{App, Job, Message, SharingContinuation};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use modsync::deployment::{ManagementAction, ManagementReceipt, ManagementStatus, Service};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    context: String,
    service: Option<Service>,
    inventory: Option<ManagementStatus>,
    confirmation: Option<ManagementAction>,
    status: String,
    receipt: Option<ManagementReceipt>,
    run: u64,
}
enum Output {
    Inventory(Service, ManagementStatus),
    Receipt(ManagementReceipt),
}
pub(super) struct Finished {
    run: u64,
    context: String,
    result: Result<Output>,
}
impl App {
    fn deployment_context(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            self.input_stamp,
            self.active_profile_name,
            self.checkpoints.config.staging,
            self.checkpoints.config.manifests
        )
    }
    pub(super) fn deployment_badge(&self, guid: &str) -> bool {
        self.deployment.context == self.deployment_context()
            && self.deployment.inventory.as_ref().is_some_and(|inventory| {
                inventory
                    .manifests
                    .iter()
                    .any(|manifest| manifest.component_guid.eq_ignore_ascii_case(guid))
            })
    }
    pub(super) fn deployment_start(
        &mut self,
        action: Option<ManagementAction>,
        context: Option<String>,
        ctx: &egui::Context,
    ) {
        if self.receiver.is_some()
            || self.desktop.busy()
            || !self.desktop.preferences_ready(&self.profile_directory)
            || self.sharing_continuation.is_some()
            || self.sharing.closing
            || self.fomod_pending()
        {
            return;
        }
        self.refresh_input_stamp();
        let captured = self.deployment_context();
        if let Some(selected_action) = action.as_ref() {
            if context.as_ref() != Some(&captured) || self.deployment.context != captured {
                self.deployment.status =
                    "Context changed; refresh inventory and confirm again.".into();
                return;
            }
            if !self.sharing.mutation() {
                self.sharing_continuation = Some(SharingContinuation::Deployment(
                    selected_action.clone(),
                    captured,
                ));
                return;
            }
            // Any mutation may partially change files even if it returns an error.
            self.invalidate_validation();
            self.completed_base = None;
            self.pending_base.clear();
            self.install_context = None;
            self.finished = false;
            self.completion = Default::default();
            self.confirm_install = false;
        }
        let service = self.deployment.service.clone();
        let identity = self
            .deployment
            .inventory
            .as_ref()
            .map(|value| value.identity.clone());
        let game = self.game.clone();
        let settings = self.profile_directory.clone();
        let profile = self.active_profile_name.clone();
        let config = self.checkpoints.config.clone();
        self.deployment.run = self.deployment.run.wrapping_add(1);
        let run = self.deployment.run;
        self.deployment.status = "Managed deployment operation running…".into();
        self.deployment.confirmation = None;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let flag = self.cancelled.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.job = Job::Editor;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->Result<Output>{
                ensure!(!flag.load(Ordering::Acquire),"Managed operation cancelled");
                match action {
                    Some(action)=>Ok(Output::Receipt(service.context("Refresh inventory first")?.manage(&identity.context("Missing reviewed inventory identity")?,action,&flag)?)),
                    None=>{
                        let game=modsync::game::resolve_install_root(Path::new(&game));
                        let service=Service::resolve_readonly(&game,(!config.staging.trim().is_empty()).then(||Path::new(&config.staging)),(!config.manifests.trim().is_empty()).then(||Path::new(&config.manifests)),Some(Path::new(&settings)),(!profile.trim().is_empty()).then_some(profile.as_str()))?;
                        let inventory=service.management_status()?;
                        ensure!(!flag.load(Ordering::Acquire),"Inventory cancelled");
                        Ok(Output::Inventory(service,inventory))
                    }
                }
            })).unwrap_or_else(|_|Err(anyhow::anyhow!("Managed deployment worker stopped unexpectedly; inspect captured roots before continuing")));
            let _ = tx.send(Message::DeploymentTerminal(Box::new(Finished {
                run,
                context: captured,
                result,
            })));
            ctx.request_repaint();
        });
    }
    pub(super) fn deployment_terminal(&mut self, finished: Finished) {
        if finished.run != self.deployment.run {
            return;
        }
        self.receiver = None;
        self.refresh_input_stamp();
        let current = finished.context == self.deployment_context() && !self.sharing.closing;
        match finished.result {
            Ok(Output::Inventory(service, inventory))
                if current && !self.cancelled.load(Ordering::Acquire) =>
            {
                self.deployment.service = Some(service);
                self.deployment.inventory = Some(inventory);
                self.deployment.context = finished.context;
                self.deployment.status =
                    "Inventory captured. Review warnings and select an explicit action.".into();
            }
            Ok(Output::Inventory(..)) => {
                self.deployment.status =
                    "Inventory discarded after cancellation or changed inputs.".into()
            }
            Ok(Output::Receipt(receipt)) => {
                self.deployment.status=if receipt.complete{"Managed action completed; refresh inventory before another action."}else{"Managed action incomplete. Inspect every warning, skipped path and error before continuing."}.into();
                if !current {
                    self.deployment
                        .status
                        .push_str(" This receipt belongs to the captured previous context.");
                }
                if current && receipt.complete {
                    self.sharing.resume_after_mutation(self.guide.as_ref());
                }
                self.deployment.receipt = Some(receipt);
                self.deployment.inventory = None;
                self.deployment.service = None;
            }
            Err(error) => {
                self.deployment.status = format!("Managed operation stopped: {error:#}");
                self.deployment.inventory = None;
                self.deployment.service = None;
            }
        }
    }
    pub(super) fn deployment_window(&mut self, ctx: &egui::Context) {
        if !self.deployment.open {
            return;
        }
        self.refresh_input_stamp();
        let current = self.deployment.context == self.deployment_context();
        if !current {
            self.deployment.confirmation = None;
        }
        let available = self.receiver.is_none()
            && !self.desktop.busy()
            && self.sharing_continuation.is_none()
            && !self.sharing.closing
            && !self.fomod_pending();
        let mut open = true;
        let mut refresh = false;
        let mut action = None;
        egui::Window::new("Managed deployment").open(&mut open).default_size([760.0,620.0]).vscroll(true).show(ctx,|ui|{
            ui.label("Uses the game, profile and optional staging/manifest roots configured in Checkpoint history. Standalone uninstall does not create a crash-recovery snapshot.");ui.label(&self.deployment.status);
            refresh=ui.add_enabled(available,egui::Button::new("Refresh managed inventory")).clicked();
            if !current && self.deployment.inventory.is_some(){ui.label("Inventory is stale; refresh for the current roots.");}
            if let Some(inventory)=&self.deployment.inventory {
                ui.label(format!("Game: {}\nManifest root: {}\nProfile: {}",inventory.game_directory.display(),inventory.manifest_root.display(),inventory.active_profile.as_deref().unwrap_or("explicit roots")));
                for warning in &inventory.warnings{ui.colored_label(egui::Color32::YELLOW,warning);}
                let mutable=available && current && inventory.warnings.is_empty();
                for manifest in &inventory.manifests {ui.push_id(&manifest.component_guid,|ui|{
                    ui.horizontal(|ui|{ui.label(format!("Deployed: {} · {} files · {}",manifest.component_name.as_deref().unwrap_or(&manifest.component_guid),manifest.entries.len(),manifest.deployed_utc));if ui.add_enabled(mutable,egui::Button::new("Review uninstall")).clicked(){self.deployment.confirmation=Some(ManagementAction::Uninstall(manifest.component_guid.clone()));}});
                    ui.collapsing("Manifest file provenance",|ui|{if let Ok(text)=serde_json::to_string_pretty(manifest){ui.monospace(text);}});
                });}
                if ui.add_enabled(mutable && !inventory.manifests.is_empty(),egui::Button::new("Review purge of this managed profile")).clicked(){self.deployment.confirmation=Some(ManagementAction::Purge);}
            }
            if let Some(pending)=self.deployment.confirmation.clone(){ui.group(|ui|{
                ui.strong(format!("Confirm {pending:?}"));ui.label("Only recorded ownership is removed/restored. Modified files may be skipped. Errors can leave partial changes and will stop the action; cancellation waits for the active component to settle.");
                if ui.add_enabled(available && current,egui::Button::new("Confirm managed mutation")).clicked(){action=Some(pending);}
                if ui.button("Cancel confirmation").clicked(){self.deployment.confirmation=None;}
            });}
            if let Some(receipt)=&self.deployment.receipt {
                ui.separator();ui.heading(if receipt.complete{"Completed managed action"}else{"Incomplete managed action"});ui.label(format!("Captured game: {}\nCaptured manifests: {}",receipt.game_directory.display(),receipt.manifest_root.display()));
                if let Some(error)=&receipt.error{ui.colored_label(egui::Color32::RED,error);}
                if receipt.cancelled{ui.label("Cancelled; prior completed component reports remain below.");}
                for warning in &receipt.warnings{ui.colored_label(egui::Color32::YELLOW,warning);}
                for report in &receipt.reports{ui.collapsing(format!("{} · removed {}",report.component,report.removed),|ui|{for(label,paths)in[("Deleted",&report.deleted),("Restored",&report.restored),("Transferred",&report.transferred),("Skipped / modified",&report.skipped),("Warnings",&report.warnings)]{ui.strong(label);for path in paths{ui.label(path);}}});}
            }
        });
        self.deployment.open = open;
        if refresh {
            self.deployment_start(None, None, ctx);
        } else if let Some(action) = action {
            self.deployment_start(Some(action), Some(self.deployment_context()), ctx);
        }
    }
}
