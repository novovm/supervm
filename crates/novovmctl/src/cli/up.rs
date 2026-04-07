use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct UpArgs {
    #[arg(long, default_value = "prod")]
    pub profile: String,

    #[arg(long, default_value = "full")]
    pub role_profile: String,

    #[arg(long)]
    pub overlay_route_runtime_file: Option<String>,

    #[arg(long)]
    pub overlay_route_runtime_profile: Option<String>,

    #[arg(long)]
    pub overlay_route_mode: Option<String>,

    #[arg(long)]
    pub overlay_route_relay_directory_file: Option<String>,

    #[arg(long)]
    pub policy_cli_binary_file: Option<String>,

    #[arg(long)]
    pub node_binary_file: Option<String>,

    #[arg(long, default_value_t = false)]
    pub use_node_watch_mode: bool,

    #[arg(long)]
    pub poll_ms: Option<u64>,

    #[arg(long)]
    pub node_watch_batch_max_files: Option<usize>,

    #[arg(long)]
    pub idle_exit_seconds: Option<u64>,

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

    #[arg(long, default_value_t = true)]
    pub auto_profile_enabled: bool,

    #[arg(long, default_value_t = false)]
    pub skip_policy_warmup: bool,

    #[arg(long, default_value_t = true)]
    pub foreground: bool,

    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    #[arg(long)]
    pub audit_file: Option<String>,

    #[arg(long)]
    pub log_file: Option<String>,

    #[arg(long, default_value_t = false)]
    pub print_effective_config: bool,
}
