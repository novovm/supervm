use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutNodeResult {
    pub node: String,
    pub node_group: String,
    pub transport: String,
    pub lifecycle_action: String,
    pub target_version: Option<String>,
    pub rollback_version: Option<String>,
    pub result: String,
    pub error: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutAuditRecord {
    pub action: String,
    pub plan_file: String,
    pub controller_id: Option<String>,
    pub operation_id: Option<String>,
    pub allowed_controllers: Vec<String>,
    pub group_order: Vec<String>,
    pub enabled_groups: Vec<String>,
    pub enabled_node_count: usize,
    pub disabled_node_count: usize,
    pub local_node_count: usize,
    pub ssh_node_count: usize,
    pub winrm_node_count: usize,
    pub target_version: Option<String>,
    pub rollback_version: Option<String>,
    pub ok_count: usize,
    pub error_count: usize,
    pub applied: bool,
    pub dry_run: bool,
    pub reason: String,
    pub results: Vec<RolloutNodeResult>,
}
