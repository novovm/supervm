use clap::Args;

#[derive(Debug, Args)]
pub struct EvmBlockAccessListScanArgs {
    #[arg(long, value_name = "COUNT")]
    pub latest_count: Option<u64>,

    #[arg(long, value_name = "NUMBER")]
    pub from_block: Option<String>,

    #[arg(long, value_name = "NUMBER_OR_TAG")]
    pub to_block: Option<String>,

    #[arg(long, value_name = "PATH")]
    pub store_path: Option<String>,

    #[arg(long, default_value_t = false)]
    pub only_problems: bool,

    #[arg(long, default_value_t = false)]
    pub require_payload: bool,

    #[arg(long, default_value_t = false)]
    pub require_complete: bool,

    #[arg(long, default_value_t = false)]
    pub require_hash_when_complete: bool,

    #[arg(long, value_name = "PATH")]
    pub json_out: Option<String>,
}
