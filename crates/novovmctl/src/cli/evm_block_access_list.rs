use clap::{ArgGroup, Args};

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("selector")
        .required(true)
        .multiple(false)
        .args(["block_hash", "block_number"])
))]
pub struct EvmBlockAccessListArgs {
    #[arg(long, value_name = "HEX_HASH")]
    pub block_hash: Option<String>,

    #[arg(long, value_name = "NUMBER_OR_TAG")]
    pub block_number: Option<String>,

    #[arg(long, value_name = "PATH")]
    pub store_path: Option<String>,

    #[arg(long, default_value_t = false)]
    pub require_payload: bool,

    #[arg(long, default_value_t = false)]
    pub require_complete: bool,

    #[arg(long, value_name = "PATH")]
    pub json_out: Option<String>,
}
