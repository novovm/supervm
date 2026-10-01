use anyhow::Result;
use std::path::PathBuf;

#[path = "../mainline_preflight.rs"]
mod mainline_preflight;

fn parse_repo_root() -> PathBuf {
    let mut args = std::env::args().skip(1);
    let mut repo_root = PathBuf::from(".");
    while let Some(arg) = args.next() {
        if arg.as_str() == "--repo-root" {
            if let Some(v) = args.next() {
                repo_root = PathBuf::from(v);
            }
        }
    }
    repo_root
}

fn main() -> Result<()> {
    let repo_root = parse_repo_root();
    let outcome = mainline_preflight::run_preflight(&repo_root)?;

    println!(
        "mainline preflight passed: status={} delivery={} age={}s max_age={}s",
        outcome.status_path.display(),
        outcome.delivery_path.display(),
        outcome.age_seconds,
        outcome.max_age_seconds
    );

    Ok(())
}
