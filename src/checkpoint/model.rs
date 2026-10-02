use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootBinding {
    pub mods: String,
    pub game: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mode {
    pub readonly: bool,
    /// Unix permission bits when supported. No ACL/ownership/xattr equivalence claim.
    pub unix: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Entry {
    Directory {
        mode: Mode,
    },
    File {
        sha256: String,
        size: u64,
        mode: Mode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tree {
    pub root_mode: Mode,
    #[serde(deserialize_with = "unique_map")]
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParticipantKind {
    File,
    Directory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParticipantBinding {
    pub path: String,
    pub kind: ParticipantKind,
}

#[derive(Clone, Debug, Default)]
pub struct InstallContext {
    pub ordered_components: Vec<serde_json::Value>,
    pub initial_completed_components: Vec<serde_json::Value>,
    pub participants: BTreeMap<String, ParticipantBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum AuxiliarySnapshot {
    Absent,
    File {
        sha256: String,
        size: u64,
        mode: Mode,
    },
    Directory {
        tree: Tree,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptBinding {
    pub sha256: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_report: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session {
    pub schema_version: u32,
    pub session_id: String,
    pub created_utc: String,
    pub roots: RootBinding,
    pub install_scope: String,
    pub ordered_components: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_completed_components: Vec<serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub participants: BTreeMap<String, ParticipantBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ancestor_shells: Vec<String>,
    #[serde(default)]
    pub definitions_sha256: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_checkpoint_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_session_id: Option<String>,
    pub origin: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Checkpoint {
    pub schema_version: u32,
    pub checkpoint_id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub sequence: u64,
    pub kind: String,
    pub completed_components: Vec<serde_json::Value>,
    #[serde(default)]
    pub completed_sha256: Option<String>,
    #[serde(default)]
    pub component: Option<ComponentIdentity>,
    pub roots: RootBinding,
    pub mods: Tree,
    pub game: Tree,
    #[serde(deserialize_with = "unique_map")]
    pub auxiliary: BTreeMap<String, Tree>,
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub auxiliary_snapshots: BTreeMap<String, AuxiliarySnapshot>,
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub ancestor_shells: BTreeMap<String, Option<Mode>>,
    pub file_count: u64,
    pub total_bytes: u64,
    pub creator: String,
    pub origin: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Head {
    pub schema_version: u32,
    pub generation: u64,
    pub checkpoint_id: String,
    pub status: String,
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalization_receipt: Option<ReceiptBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_operation: Option<OperationIntent>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: String,
    pub checkpoint_id: Option<String>,
    pub generation: Option<u64>,
    pub status: String,
    pub created_utc: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub sessions: Vec<SessionSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub file_count: u64,
    pub total_bytes: u64,
    pub durability: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub verified_objects: u64,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: &'static str,
    pub files: u64,
    pub bytes: u64,
}

/// A manifest must not silently collapse duplicate paths before validation.
fn unique_map<'de, D, V>(deserializer: D) -> Result<BTreeMap<String, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct Unique<V>(std::marker::PhantomData<V>);
    impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for Unique<V> {
        type Value = BTreeMap<String, V>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object with unique checkpoint paths")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            let mut values = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, V>()? {
                if values.insert(key, value).is_some() {
                    return Err(A::Error::custom("Duplicate checkpoint path key"));
                }
                if values.len() > 100_000 {
                    return Err(A::Error::custom("Checkpoint map exceeds 100000 entries"));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentIdentity {
    pub guid: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeRoot {
    Mods,
    Game,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedHead {
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationIntent {
    pub schema_version: u32,
    pub operation_id: String,
    pub kind: String,
    pub session_id: String,
    pub expected_generation: u64,
    pub from_checkpoint_id: String,
    pub roots: RootBinding,
    pub component_definition: serde_json::Value,
    pub component_sha256: String,
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalization_receipt: Option<ReceiptBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<TransitionBinding>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub head_checkpoint_id: String,
    pub session: Session,
    pub checkpoint: Checkpoint,
    pub pending_operations: Vec<OperationIntent>,
    pub finalization_receipt: Option<ReceiptBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathChange {
    pub tree: TreeRoot,
    pub path: String,
    pub change: &'static str,
    pub before: Option<Entry>,
    pub after: Option<Entry>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuxiliaryChange {
    pub participant: String,
    pub before: AuxiliarySnapshot,
    pub after: AuxiliarySnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub generation: u64,
    pub from_checkpoint_id: String,
    pub to_checkpoint_id: String,
    pub changes: Vec<PathChange>,
    pub mods_root_mode_changed: bool,
    pub game_root_mode_changed: bool,
    pub auxiliary_changes: Vec<AuxiliaryChange>,
    pub ancestor_shell_changes: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceEvent {
    pub checkpoint_id: String,
    pub sequence: u64,
    pub component: Option<ComponentIdentity>,
    pub change: &'static str,
    pub entry: Option<Entry>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub tree: TreeRoot,
    pub path: String,
    pub history: Vec<ProvenanceEvent>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub parent_checkpoint_id: String,
    pub generation: u64,
    pub operation_id: String,
    pub completed_components: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub operation_id: String,
    pub receipt: ReceiptBinding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionBinding {
    pub target_checkpoint_id: String,
    pub branch_session_id: String,
    pub branch_checkpoint_id: String,
    pub source_operation_id: String,
    pub source_status: String,
    pub safety_session_id: Option<String>,
    pub action: String,
    pub branch_kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeRequest {
    pub expected: ExpectedHead,
    pub accept_source_changes: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePlan {
    pub expected: ExpectedHead,
    pub completed_components: Vec<serde_json::Value>,
    pub remaining_components: Vec<serde_json::Value>,
    pub source_changes: Vec<PathChange>,
    pub source_root_mode_changed: bool,
    #[serde(skip)]
    pub(super) context: InstallContext,
    #[serde(skip)]
    pub(super) observed_sha256: String,
    #[serde(skip)]
    pub(super) source_rebase: bool,
    #[serde(skip)]
    pub(super) validated_expected: ExpectedHead,
    #[serde(skip)]
    pub(super) source_operation_id: String,
    #[serde(skip)]
    pub(super) source_status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub action: String,
    pub source_session_id: String,
    pub source_checkpoint_id: String,
    pub target_checkpoint_id: String,
    pub session_id: String,
    pub checkpoint_id: String,
    pub generation: u64,
    pub completed_components: usize,
    pub safety_session_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub schema_version: u32,
    pub operation: &'static str,
    pub action: String,
    pub restored: Option<RestoreReport>,
    pub session_id: Option<String>,
    pub checkpoint_id: Option<String>,
}
