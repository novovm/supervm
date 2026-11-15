use anyhow::{ensure, Result};
use novovm_relay::asas_client::{ClientConfigV1, ClientOwnerV1};
async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--client-contract" {
        println!("{{\"version\":1,\"role\":\"asas-b-client\",\"transport\":\"novorudp\",\"startsAgent\":false}}");
        return Ok(());
    }
    ensure!(
        args.len() == 4 && args[0] == "--profile" && args[2] == "--profile-sha256",
        "explicit B profile/digest required"
    );
    let digest = args[3]
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("profile digest required"))?;
    let config = ClientConfigV1::load(std::path::Path::new(&args[1]), digest)?;
    ClientOwnerV1::connect(&config).await?.serve_stdio().await
}
fn main() {
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .and_then(|runtime| runtime.block_on(run()).map_err(std::io::Error::other));
    if result.is_err() {
        eprintln!(
            "ASAS B client failed; inspect selected profile and original task; no automatic replay"
        );
        std::process::exit(1);
    }
}
