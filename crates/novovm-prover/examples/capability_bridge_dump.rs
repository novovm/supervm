use anyhow::Result;
use novovm_exec::{AoemExecFacade, AoemRuntimeConfig};
use novovm_prover::ProverCapabilityContract;

fn main() -> Result<()> {
    let runtime = AoemRuntimeConfig::from_env()?;
    let facade = AoemExecFacade::open_with_runtime(&runtime)?;
    let aoem = facade.capability_contract()?;
    let prover = ProverCapabilityContract::from_aoem(&aoem);

    println!("{}", serde_json::to_string_pretty(&prover)?);

    // Keep AOEM DLL resident for process lifetime to avoid teardown races on Windows.
    std::mem::forget(facade);
    Ok(())
}

