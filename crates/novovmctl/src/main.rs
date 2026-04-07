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
    let cli = Cli::parse();
    match cli.command {
        TopCommand::Up(args) => commands::up::run(args),
        TopCommand::RolloutControl(args) => commands::rollout_control::run(args),
        TopCommand::Rollout(args) => commands::rollout::run(args),
        TopCommand::Lifecycle(args) => commands::lifecycle::run(args),
        TopCommand::Daemon(args) => commands::daemon::run(args),
    }
}
