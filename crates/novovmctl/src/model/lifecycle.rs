use crate::model::up::UpAuditRecord;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedIngressManifest {
    pub dir: String,
    pub bootstrap_file: String,
    pub bootstrap_seeded: bool,
    pub bootstrap_bytes: usize,
    pub opsw1_count_after_seed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleAuditRecord {
    pub action: String,
    pub repo_root: String,
    pub release_root: String,
    pub runtime_state_file: String,
    pub runtime_pid_file: String,
    pub runtime_log_dir: String,
    pub state_exists: bool,
    pub updated: bool,
    pub applied: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub current_release: String,
    pub previous_release: String,
    pub registered_release: Option<String>,
    pub target_release: Option<String>,
    pub rollback_release: Option<String>,
    pub current_profile: Option<String>,
    pub current_role_profile: Option<String>,
    pub current_runtime_profile: Option<String>,
    pub node_group: Option<String>,
    pub upgrade_window: Option<String>,
    pub launch_audit: Option<UpAuditRecord>,
    pub managed_ingress: Option<ManagedIngressManifest>,
    pub reason: String,
    pub state: Value,
}
