//! Explicit operator-selected local execution, not network proposal admission.
use anyhow::{bail, Context, Result};
use novovm_node::native_candidate_plan::NovNativeCandidateExecutionPlanV1;
use std::{io::Read, path::PathBuf};

const MODE: &str = "native_candidate_execute";
const PLAN_PATH: &str = "NOVOVM_NATIVE_CANDIDATE_PLAN_PATH";
const PLAN_PIN: &str = "NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT";
// JSON byte arrays expand the 2 MiB binary body; never read an unbounded file.
const MAX_PLAN_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn selected(mode: &str, query_selected: bool) -> Result<bool> {
    let path = std::env::var_os(PLAN_PATH);
    let pin = std::env::var_os(PLAN_PIN);
    if !mode.eq_ignore_ascii_case(MODE) {
        if path.is_some() || pin.is_some() {
            bail!("candidate plan configuration requires native_candidate_execute mode");
        }
        return Ok(false);
    }
    if query_selected {
        bail!("candidate execution cannot be combined with a query override");
    }
    if path.is_none() || pin.is_none() {
        bail!("candidate execution requires explicit plan path and commitment");
    }
    Ok(true)
}

pub(super) fn run(params: &serde_json::Value) -> Result<()> {
    let path = PathBuf::from(std::env::var_os(PLAN_PATH).context("candidate plan path missing")?);
    let pin = std::env::var(PLAN_PIN).context("candidate plan commitment must be UTF-8")?;
    if pin.len() != 64
        || !pin
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("candidate plan commitment must be 64 lowercase hexadecimal characters");
    }
    let file = std::fs::File::open(&path).context("open candidate plan failed")?;
    if !file.metadata()?.is_file() {
        bail!("candidate plan must be a regular file");
    }
    let mut bytes = Vec::new();
    file.take(MAX_PLAN_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PLAN_BYTES {
        bail!("candidate plan exceeds 16 MiB JSON limit");
    }
    let plan: NovNativeCandidateExecutionPlanV1 =
        serde_json::from_slice(&bytes).context("decode candidate plan failed")?;
    plan.validate()?;
    let actual = plan
        .plan_commitment
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if actual != pin {
        bail!("candidate plan commitment does not match operator pin");
    }
    if params["chain_id"].as_u64() != Some(plan.context.chain_id) {
        bail!("candidate plan chain does not match local node configuration");
    }
    if params["aoem_owned_gate_config"]["production_candidate"].as_bool() != Some(true) {
        bail!("candidate execution requires AOEM production ownership enabled");
    }
    let protocol = plan
        .protocol_config_commitment
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if protocol != novovm_node::tx_ingress::native_business_protocol_config_commitment_v1()? {
        bail!("candidate plan protocol does not match local node configuration");
    }
    // Finish structural and operator pin checks before recovery can write state.
    novovm_node::tx_ingress::verify_native_business_protocol_config_pin_for_aoem_production_v1(
        params,
    )?;
    let _session = novovm_node::tx_ingress::NativeAoemSemanticSessionScopeV1::default();
    novovm_node::tx_ingress::recover_nov_native_host_projection_from_aoem_v1(params)?;
    novovm_node::tx_ingress::recover_nov_native_block_ledger_from_aoem_v1(params)?;
    let output =
        novovm_node::tx_ingress::run_nov_native_candidate_execution_plan_v1(&plan, params)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
