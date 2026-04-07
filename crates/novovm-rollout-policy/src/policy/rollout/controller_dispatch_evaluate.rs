#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::fs;

use crate::output::print_success_json;

#[derive(Debug)]
struct Args {
    queue_file: String,
    plan_action: String,
    controller_id: String,
    operation_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct QueuePlan {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    action: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct QueueConfig {
    #[serde(default)]
    max_concurrent_plans: i32,
    #[serde(default)]
    plans: Vec<QueuePlan>,
}

fn parse_args(raw_args: &[String]) -> Result<Args> {
    let mut out = Args {
        queue_file: String::new(),
        plan_action: "upgrade".to_string(),
        controller_id: String::new(),
        operation_id: String::new(),
    };
    let mut it = raw_args.iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--queue-file" => out.queue_file = val.clone(),
            "--plan-action" => out.plan_action = val.clone(),
            "--controller-id" => out.controller_id = val.clone(),
            "--operation-id" => out.operation_id = val.clone(),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    if out.queue_file.trim().is_empty() {
        bail!("--queue-file is required");
    }
    if out.plan_action.trim().is_empty() {
        out.plan_action = "upgrade".to_string();
    }
    Ok(out)
}

pub fn run_with_args(raw_args: &[String]) -> Result<()> {
    let args = parse_args(raw_args)?;
    let raw = fs::read_to_string(&args.queue_file)
        .with_context(|| format!("read queue file failed: {}", args.queue_file))?;
    let queue: QueueConfig = serde_json::from_str(&raw)
        .with_context(|| format!("parse queue file failed: {}", args.queue_file))?;

    let action_norm = args.plan_action.trim().to_ascii_lowercase();
    let matched_plans = queue
        .plans
        .iter()
        .filter(|plan| {
            if !plan.enabled {
                return false;
            }
            let plan_action = plan.action.trim().to_ascii_lowercase();
            plan_action.is_empty() || plan_action == action_norm
        })
        .count();

    let continue_dispatch = queue.max_concurrent_plans > 0 && matched_plans > 0;
    let block_dispatch = !continue_dispatch;
    let reason = if queue.max_concurrent_plans <= 0 {
        "queue_cap_zero"
    } else if matched_plans == 0 {
        "no_matching_enabled_plans"
    } else {
        "matching_enabled_plans"
    };

    let out = json!({
        "continue_dispatch": continue_dispatch,
        "block_dispatch": block_dispatch,
        "reason": reason,
        "matched_plans": matched_plans,
        "max_concurrent_plans": queue.max_concurrent_plans.max(0),
        "controller_id": args.controller_id,
        "operation_id": args.operation_id
    });
    print_success_json("rollout", "controller-dispatch-evaluate", &out)
}

pub fn run_cli() -> Result<()> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    run_with_args(&raw_args)
}
