mod audit;
mod cli;
mod commands;
mod error;
mod integration;
mod model;
mod output;
mod runtime;

use clap::Parser;
use cli::{Cli, TopCommand};
use error::CtlError;

fn main() {
    if let Err(err) = run() {
        std::process::exit(err.exit_code());
    }
}

fn run() -> Result<(), CtlError> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if std::env::args_os().nth(1).as_deref()
                == Some(std::ffi::OsStr::new("native-nonce-migration"))
                && !matches!(
                    error.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ) =>
        {
            let error = CtlError::InvalidArgument(error.to_string());
            output::print_error_json("native-nonce-migration", &error);
            return Err(error);
        }
        Err(error) => error.exit(),
    };
    match cli.command {
        TopCommand::Up(args) => commands::up::run(args),
        TopCommand::EvmBlockAccessList(args) => commands::evm_block_access_list::run(args),
        TopCommand::EvmBlockAccessListScan(args) => commands::evm_block_access_list_scan::run(args),
        TopCommand::GovernanceStats(args) => commands::governance_stats::run(args),
        TopCommand::NativeNonceMigration(args) => commands::native_nonce_migration::run(args),
        TopCommand::RolloutControl(args) => commands::rollout_control::run(args),
        TopCommand::Rollout(args) => commands::rollout::run(args),
        TopCommand::Lifecycle(args) => commands::lifecycle::run(args),
        TopCommand::Daemon(args) => commands::daemon::run(args),
    }
}
