use clap::Args;

#[derive(Debug, Args)]
pub struct RolloutControlArgs {
    #[arg(long)]
    pub queue_file: String,

    #[arg(long, default_value = "upgrade")]
    pub plan_action: String,

    #[arg(long)]
    pub controller_id: Option<String>,

    #[arg(long)]
    pub operation_id: Option<String>,

    #[arg(long)]
    pub audit_file: Option<String>,

    #[arg(long)]
    pub policy_cli_binary_file: Option<String>,

    #[arg(long)]
    pub node_binary_file: Option<String>,

    #[arg(long, default_value_t = false)]
    pub resume_from_snapshot: bool,

    #[arg(long, default_value_t = false)]
    pub replay_conflicts_on_start: bool,

    #[arg(long, default_value_t = false)]
    pub continue_on_plan_failure: bool,

    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    #[arg(long, default_value_t = false)]
    pub print_effective_queue: bool,
}
