#![forbid(unsafe_code)]

mod cli;
mod commands;
mod error;

use clap::Parser;
use cli::{Cli, DomainCommand};
use error::PolicyError;
use std::env;
use std::process;

fn run() -> Result<i32, PolicyError> {
    let raw_args: Vec<String> = env::args().collect();
    if let Some(first) = raw_args.get(1).map(|v| v.trim()) {
        if commands::legacy::is_flat_tool(first) {
            return commands::legacy::dispatch_flat(first, raw_args[2..].to_vec());
        }
    }

    let cli = Cli::parse();
    match cli.command {
        DomainCommand::Overlay(cmd) => commands::overlay::dispatch(cmd.command),
        DomainCommand::Rollout(cmd) => commands::rollout::dispatch(cmd.command),
        DomainCommand::Risk(cmd) => commands::risk::dispatch(cmd.command),
        DomainCommand::Failover(cmd) => commands::failover::dispatch(cmd.command),
    }
}

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(err) => {
            eprintln!("{err}");
            process::exit(err.exit_code());
        }
    }
}
