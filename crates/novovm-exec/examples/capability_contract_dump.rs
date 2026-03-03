use anyhow::Result;
use novovm_exec::{AoemExecFacade, AoemRuntimeConfig};

fn main() -> Result<()> {
    let runtime = AoemRuntimeConfig::from_env()?;
    let facade = AoemExecFacade::open_with_runtime(&runtime)?;
    let contract = facade.capability_contract_json()?;

    println!("{}", serde_json::to_string_pretty(&contract)?);

    // Keep AOEM DLL resident for process lifetime to avoid teardown races on Windows.
    std::mem::forget(facade);
    Ok(())
}
