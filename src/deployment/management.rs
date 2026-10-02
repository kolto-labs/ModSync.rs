//! Worker-facing managed deployment actions. Each public call owns the game lock.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementStatus {
    pub game_directory: PathBuf,
    pub manifest_root: PathBuf,
    pub active_profile: Option<String>,
    pub manifests: Vec<Manifest>,
    pub warnings: Vec<String>,
    /// Identity of the reviewed manifests, including warnings and root binding.
    pub identity: String,
}
#[derive(Clone, Debug)]
pub enum ManagementAction {
    Uninstall(String),
    Purge,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementReceipt {
    pub game_directory: PathBuf,
    pub manifest_root: PathBuf,
    pub reports: Vec<UninstallReport>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub cancelled: bool,
    /// False for skipped files, warnings, partial failures, or cancellation.
    pub complete: bool,
}
impl Service {
    fn management_status_locked(&self) -> Result<ManagementStatus> {
        let (manifests, warnings) = match fs::symlink_metadata(self.manifests()) {
            Err(error) if error.kind() == ErrorKind::NotFound => (Vec::new(), Vec::new()),
            Err(error) => return Err(error.into()),
            Ok(_) => self.deployed_with_warnings()?,
        };
        let identity = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &self.game,
                &self.root,
                &self.active_profile,
                &manifests,
                &warnings,
            ))?)
        );
        Ok(ManagementStatus {
            game_directory: self.game.clone(),
            manifest_root: self.root.clone(),
            active_profile: self.active_profile.clone(),
            manifests,
            warnings,
            identity,
        })
    }
    /// Read-only apart from the ordinary game lock file. Does not create profile
    /// directories. Pending checkpoint sessions reject management consistently.
    pub fn management_status(&self) -> Result<ManagementStatus> {
        let _lock = self.lock()?;
        self.management_status_locked()
    }
    /// Run on an owned background worker. Cancellation is observed between
    /// component operations; an in-progress uninstall is allowed to settle.
    /// On failure its filesystem effects may be partial: inspect before retrying.
    pub fn manage(
        &self,
        expected_identity: &str,
        action: ManagementAction,
        cancelled: &AtomicBool,
    ) -> Result<ManagementReceipt> {
        let _lock = self.lock()?;
        let status = self.management_status_locked()?;
        ensure!(
            status.identity == expected_identity,
            "Deployment status changed; refresh before managing it"
        );
        // Unreadable ownership cannot safely participate in backup handoffs.
        ensure!(
            status.warnings.is_empty(),
            "Deployment inventory has unreadable manifests: {}",
            status.warnings.join("; ")
        );
        let mut targets = match action {
            ManagementAction::Uninstall(component) => {
                let id = component_id(&component)?;
                ensure!(
                    status
                        .manifests
                        .iter()
                        .any(|m| component_id(&m.component_guid)
                            .is_ok_and(|known| known.eq_ignore_ascii_case(&id))),
                    "Deployment no longer exists"
                );
                vec![id]
            }
            ManagementAction::Purge => status
                .manifests
                .iter()
                .rev()
                .map(|m| m.component_guid.clone())
                .collect(),
        };
        let mut receipt = ManagementReceipt {
            game_directory: self.game.clone(),
            manifest_root: self.root.clone(),
            reports: Vec::new(),
            warnings: Vec::new(),
            error: None,
            cancelled: false,
            complete: false,
        };
        for component in targets.drain(..) {
            if cancelled.load(Ordering::Acquire) {
                receipt.cancelled = true;
                break;
            }
            match self.uninstall_report(&component) {
                Ok(report) => {
                    let partial = !report.skipped.is_empty()
                        || !report.warnings.is_empty()
                        || !report.removed;
                    receipt.reports.push(report);
                    if partial {
                        receipt.warnings.push("Management stopped after an incomplete uninstall; inspect retained files and ownership before continuing.".into());
                        break;
                    }
                }
                Err(error) => {
                    receipt.error = Some(format!(
                        "Component {component}: {error:#}. This component may have partial filesystem changes; inspect before retrying."
                    ));
                    break;
                }
            }
        }
        receipt.complete =
            !receipt.cancelled && receipt.error.is_none() && receipt.warnings.is_empty();
        Ok(receipt)
    }
}
