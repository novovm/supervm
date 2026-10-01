use crate::cli::up::UpArgs;
use clap::Args;

#[derive(Debug, Args)]
pub struct DaemonArgs {
    #[command(flatten)]
    pub up: UpArgs,

    #[arg(long, default_value_t = 3)]
    pub restart_delay_seconds: u64,

    #[arg(long, default_value_t = 1000)]
    pub supervisor_poll_ms: u64,

    #[arg(long, default_value_t = 0)]
    pub max_restarts: u64,

    #[arg(long, default_value_t = false)]
    pub no_gateway: bool,

    #[arg(long, default_value_t = false)]
    pub build_before_run: bool,

    #[arg(long, default_value_t = false)]
    pub lean_io: bool,

    #[arg(long, default_value = "127.0.0.1:9899")]
    pub gateway_bind: String,

    #[arg(long, default_value = "artifacts/ingress/spool")]
    pub spool_dir: String,

    #[arg(long, default_value_t = 0)]
    pub gateway_max_requests: u32,
}
