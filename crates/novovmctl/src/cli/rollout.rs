use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct RolloutArgs {
    #[arg(long, default_value = "status")]
    pub action: String,

    #[arg(long, default_value = "config/runtime/lifecycle/rollout.plan.json")]
    pub plan_file: String,

    #[arg(long)]
    pub target_version: Option<String>,

    #[arg(long)]
    pub rollback_version: Option<String>,

    #[arg(long, value_delimiter = ',')]
    pub group_order: Vec<String>,

    #[arg(long, default_value_t = 12)]
    pub upgrade_health_seconds: u64,

    #[arg(long, default_value_t = 0)]
    pub default_max_failures: usize,

    #[arg(long, default_value_t = 3)]
    pub pause_seconds_between_nodes: u64,

    #[arg(long, default_value = "local")]
    pub default_transport: String,

    #[arg(long, default_value = "ssh")]
    pub ssh_binary: String,

    #[arg(long)]
    pub ssh_identity_file: Option<String>,

    #[arg(long)]
    pub ssh_known_hosts_file: Option<String>,

    #[arg(long, default_value = "accept-new")]
    pub ssh_strict_host_key_checking: String,

    #[arg(long, default_value_t = 30)]
    pub remote_timeout_seconds: u64,

    #[arg(long, default_value = "powershell")]
    pub remote_shell: String,

    #[arg(long)]
    pub winrm_credential_user_env: Option<String>,

    #[arg(long)]
    pub winrm_credential_password_env: Option<String>,

    #[arg(long, default_value = "local-controller")]
    pub controller_id: String,

    #[arg(long)]
    pub operation_id: Option<String>,

    #[arg(long)]
    pub audit_file: Option<String>,

    #[arg(long)]
    pub overlay_route_mode: Option<String>,

    #[arg(long)]
    pub overlay_route_runtime_file: Option<String>,

    #[arg(long)]
    pub overlay_route_runtime_profile: Option<String>,

    #[arg(long)]
    pub overlay_route_relay_directory_file: Option<String>,

    #[arg(long, default_value_t = false)]
    pub enable_auto_profile: bool,

    #[arg(long)]
    pub auto_profile_state_file: Option<String>,

    #[arg(long)]
    pub auto_profile_profiles: Option<String>,

    #[arg(long)]
    pub auto_profile_min_hold_seconds: Option<u64>,

    #[arg(long)]
    pub auto_profile_switch_margin: Option<f64>,

    #[arg(long)]
    pub auto_profile_switchback_cooldown_seconds: Option<u64>,

    #[arg(long)]
    pub auto_profile_recheck_seconds: Option<u64>,

    #[arg(long)]
    pub auto_profile_binary_path: Option<String>,

    #[arg(long, default_value_t = false)]
    pub ignore_upgrade_window: bool,

    #[arg(long, default_value_t = false)]
    pub auto_rollback_on_failure: bool,

    #[arg(long, default_value_t = false)]
    pub continue_on_failure: bool,

    #[arg(long, default_value_t = false)]
    pub print_effective_plan: bool,

    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}
