use serde::{Deserialize, Serialize};

use crate::model::up::AutoProfileDecision;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonAuditRecord {
    pub profile: String,
    pub role_profile: String,
    pub policy_bin: String,
    pub node_bin: String,
    pub no_gateway: bool,
    pub build_before_run: bool,
    pub lean_io: bool,
    pub use_node_watch_mode: bool,
    pub poll_ms: Option<u64>,
    pub supervisor_poll_ms: u64,
    pub node_watch_batch_max_files: Option<usize>,
    pub idle_exit_seconds: Option<u64>,
    pub gateway_bind: Option<String>,
    pub gateway_spool_dir: Option<String>,
    pub gateway_max_requests: Option<u32>,
    pub ops_wire_dir: Option<String>,
    pub ops_wire_watch_done_dir: Option<String>,
    pub ops_wire_watch_failed_dir: Option<String>,
    pub ops_wire_watch_drop_failed: bool,
    pub restart_delay_seconds: u64,
    pub max_restarts: u64,
    pub launched_cycles: u64,
    pub last_reason: String,
    pub dry_run: bool,
    pub auto_profile_decision: Option<AutoProfileDecision>,
}
