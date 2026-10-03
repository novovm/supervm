//! Explicit operator-selected local execution, not network proposal admission.
use anyhow::{bail, Context, Result};
use novovm_node::native_candidate_plan::NovNativeCandidateExecutionPlanV1;
use std::{io::Read, path::PathBuf};

const MODE: &str = "native_candidate_execute";
const FRESH_MODE: &str = "native_fresh_genesis_prepare";
const GENESIS_PATH: &str = "NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_PATH";
const GENESIS_PIN: &str = "NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_COMMITMENT";
const PLAN_PATH: &str = "NOVOVM_NATIVE_CANDIDATE_PLAN_PATH";
const PLAN_PIN: &str = "NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT";
// JSON byte arrays expand the 2 MiB binary body; never read an unbounded file.
const MAX_PLAN_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn selected(mode: &str, query_selected: bool) -> Result<bool> {
    let path = std::env::var_os(PLAN_PATH);
    let pin = std::env::var_os(PLAN_PIN);
    let fresh = mode.eq_ignore_ascii_case(FRESH_MODE);
    let genesis_path = std::env::var_os(GENESIS_PATH);
    let genesis_pin = std::env::var_os(GENESIS_PIN);
    if !fresh && (genesis_path.is_some() || genesis_pin.is_some()) {
        bail!("fresh genesis configuration requires native_fresh_genesis_prepare mode");
    }
    if fresh && (genesis_path.is_none() || genesis_pin.is_none()) {
        bail!("fresh genesis preparation requires explicit config path and commitment");
    }
    if !mode.eq_ignore_ascii_case(MODE) && !fresh {
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
    // Preparation traverses the same bounded AOEM execution/verification chain
    // as fresh confirmation. Do not depend on the smaller Windows main stack.
    let params = params.clone();
    std::thread::Builder::new()
        .name("native-candidate-preparation".into())
        .stack_size(novovm_node::native_block_seal::service::FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1)
        .spawn(move || run_inner(&params))
        .context("start native candidate preparation")?
        .join()
        .map_err(|_| anyhow::anyhow!("native candidate preparation panicked"))?
}

fn run_inner(params: &serde_json::Value) -> Result<()> {
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
    if std::env::var("NOVOVM_NODE_MODE").is_ok_and(|mode| mode.eq_ignore_ascii_case(FRESH_MODE)) {
        return prepare_fresh_genesis(&plan, params);
    }
    novovm_node::tx_ingress::recover_nov_native_host_projection_from_aoem_v1(params)?;
    novovm_node::tx_ingress::recover_nov_native_block_ledger_from_aoem_v1(params)?;
    let output =
        novovm_node::tx_ingress::run_nov_native_candidate_execution_plan_v1(&plan, params)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn prepare_fresh_genesis(
    plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
) -> Result<()> {
    use novovm_node::tx_ingress::{candidate_workspace as workspace, fresh_genesis};
    let path = PathBuf::from(std::env::var_os(GENESIS_PATH).context("genesis path missing")?);
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        bail!("fresh genesis configuration must be a regular file");
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    let config = fresh_genesis::FreshGenesisConfigV1::from_json(&bytes)?;
    let compiled = config.compile()?;
    let pin = compiled.config_commitment();
    let expected = std::env::var(GENESIS_PIN).context("genesis commitment missing")?;
    let actual = pin.iter().map(|b| format!("{b:02x}")).collect::<String>();
    if expected != actual {
        bail!("fresh genesis configuration does not match operator pin");
    }
    if config.chain_id != plan.context.chain_id
        || config.protocol_config_commitment != plan.protocol_config_commitment
        || plan.context.block_height != 1
        || plan.context.parent_block_hash != [0; 32]
        || plan.context.timestamp_unix_ms < config.timestamp_unix_ms
        || plan.aoem_parent.is_some()
        || plan.pre_state_root != compiled.state_root()
    {
        bail!("first candidate plan does not match fresh genesis");
    }
    let genesis = fresh_genesis::publication::initialize_v1(&config, pin, params)?;
    let input = workspace::create_from_genesis_v1(plan, pin, params)?;
    let execution = workspace::execute_v1(config.chain_id, input.workspace_id, params)?;
    let candidate = workspace::register_genesis_block_candidate_v1(
        config.chain_id,
        input.workspace_id,
        pin,
        params,
    )?;
    let artifact = workspace::load_block_artifact_v1(config.chain_id, input.workspace_id, params)?
        .context("registered fresh candidate artifact missing")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "mode": FRESH_MODE,
            "genesis": genesis,
            "workspace_id": input.workspace_id,
            "execution": execution,
            "candidate": candidate,
            "durable_block_candidate": artifact.block(),
            "finalized": false,
            "chain_canonical": false,
        }))?
    );
    Ok(())
}
