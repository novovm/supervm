use clap::Args;

#[derive(Debug, Args)]
pub struct GovernanceStatsArgs {
    #[arg(long)]
    pub events_file: Vec<String>,

    #[arg(long)]
    pub events_dir: Option<String>,

    #[arg(long, default_value_t = 10)]
    pub phase4_block_threshold: u64,

    #[arg(long, default_value_t = 7)]
    pub phase4_window_cycles: u64,

    #[arg(long, default_value_t = 5)]
    pub phase4_blocked_per_cycle_threshold: u64,

    #[arg(long, default_value_t = 3)]
    pub phase4_blocked_consecutive_cycles: u64,

    #[arg(long, default_value_t = 10)]
    pub phase4_inflow_per_cycle_threshold: u64,

    #[arg(long, default_value_t = 3)]
    pub phase4_inflow_consecutive_cycles: u64,

    #[arg(long, default_value_t = 1)]
    pub phase4_shadow_closed_loops_required: u64,

    #[arg(long, default_value_t = 20)]
    pub phase4_shadow_min_register_samples: u64,

    #[arg(long, default_value_t = 0.6)]
    pub phase4_shadow_closed_loop_rate_threshold: f64,

    #[arg(long, default_value_t = 0.4)]
    pub phase4_privacy_rejected_rate_threshold: f64,

    #[arg(long, default_value_t = 50)]
    pub phase4_privacy_min_required_requests: u64,

    #[arg(long, default_value_t = false)]
    pub as_prometheus: bool,

    #[arg(long, default_value_t = false)]
    pub write_phase4_gate_report: bool,

    #[arg(long)]
    pub phase4_gate_report_out: Option<String>,

    #[arg(long)]
    pub phase4_gate_report_md_out: Option<String>,
}
