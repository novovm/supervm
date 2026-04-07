use clap::Args;

#[derive(Debug, Args)]
pub struct LifecycleArgs {
    #[arg(long, default_value = "status")]
    pub action: String,

    #[arg(long)]
    pub repo_root: Option<String>,

    #[arg(long)]
    pub version: Option<String>,

    #[arg(long)]
    pub target_version: Option<String>,

    #[arg(long)]
    pub rollback_version: Option<String>,

    #[arg(long)]
    pub gateway_binary_from: Option<String>,

    #[arg(long)]
    pub node_binary_from: Option<String>,

    #[arg(long, default_value_t = false)]
    pub set_current: bool,

    #[arg(long)]
    pub release_root: Option<String>,

    #[arg(long)]
    pub runtime_state_file: Option<String>,

    #[arg(long)]
    pub audit_file: Option<String>,

    #[arg(long)]
    pub runtime_pid_file: Option<String>,

    #[arg(long)]
    pub runtime_log_dir: Option<String>,

    #[arg(long)]
    pub profile: Option<String>,

    #[arg(long)]
    pub role_profile: Option<String>,

    #[arg(long)]
    pub overlay_route_mode: Option<String>,

    #[arg(long)]
    pub overlay_route_runtime_file: Option<String>,

    #[arg(long)]
    pub overlay_route_runtime_profile: Option<String>,

    #[arg(long)]
    pub overlay_route_relay_directory_file: Option<String>,

    #[arg(long, default_value_t = false)]
    pub use_node_watch_mode: bool,

    #[arg(long)]
    pub poll_ms: Option<u64>,

    #[arg(long)]
    pub node_watch_batch_max_files: Option<usize>,

    #[arg(long)]
    pub idle_exit_seconds: Option<u64>,

    #[arg(long, default_value_t = false)]
    pub auto_profile_enabled: bool,

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
    pub policy_cli_binary_file: Option<String>,

    #[arg(long)]
    pub runtime_template_file: Option<String>,

    #[arg(long, default_value_t = false)]
    pub restart_after_set_runtime: bool,

    #[arg(long)]
    pub start_grace_seconds: Option<u64>,

    #[arg(long)]
    pub upgrade_health_seconds: Option<u64>,

    #[arg(long)]
    pub node_group: Option<String>,

    #[arg(long)]
    pub upgrade_window: Option<String>,

    #[arg(long)]
    pub require_node_group: Option<String>,

    #[arg(long, default_value_t = false)]
    pub print_effective_state: bool,

    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    #[arg(long, default_value_t = false)]
    pub force: bool,
}
