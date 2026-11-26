use anyhow::{ensure, Result};
use novovm_relay::asas_client::offline::{sign_to_file, SigningKind};
use novovm_relay::asas_client::{ClientConfigV1, ClientOwnerV1};
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--client-contract" {
        println!("{{\"version\":1,\"role\":\"asas-b-client\",\"transport\":\"novorudp\",\"startsAgent\":false,\"ucaLogin\":1,\"credentialDirectory\":1,\"ucaRootMatch\":1,\"offlineUcaSigning\":1}}");
        return Ok(());
    }
    ensure!(
        (args.len() == 4
            || args.len() == 5 && args[4] == "--uca-scope"
            || args.len() == 6 && args[4] == "--uca-root-match"
            || args.len() == 12
                && (args[4] == "--sign-registration" || args[4] == "--sign-application")
                && args[6] == "--request-sha256"
                && args[8] == "--identity-key"
                && args[10] == "--output")
            && args[0] == "--profile"
            && args[2] == "--profile-sha256",
        "explicit B profile/digest required"
    );
    let digest = args[3]
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("profile digest required"))?;
    let config = ClientConfigV1::load(std::path::Path::new(&args[1]), digest)?;
    #[cfg(target_os = "linux")]
    if let Some(directory) = &config.credentials_directory {
        // Main is still single-threaded: bind the original custody before AOEM/Tokio starts.
        std::env::set_var("CREDENTIALS_DIRECTORY", directory);
    }
    if args.len() == 5 {
        println!("{}", serde_json::to_string_pretty(&config.uca_selector()?)?);
        return Ok(());
    }
    if args.len() == 6 {
        println!(
            "{}",
            serde_json::json!({"matches":config.uca_root_matches(std::path::Path::new(&args[5]))?})
        );
        return Ok(());
    }
    if args.len() == 12 {
        let kind = if args[4] == "--sign-registration" {
            SigningKind::Registration
        } else {
            SigningKind::Application
        };
        let request_sha = args[7]
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("request digest required"))?;
        let receipt = sign_to_file(
            &config,
            kind,
            std::path::Path::new(&args[5]),
            request_sha,
            std::path::Path::new(&args[9]),
            std::path::Path::new(&args[11]),
        )?;
        println!("{}", serde_json::to_string(&receipt)?);
        return Ok(());
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async { ClientOwnerV1::connect(&config).await?.serve_stdio().await })
}
fn main() {
    let result = run();
    if result.is_err() {
        eprintln!(
            "ASAS B client failed; inspect selected profile and original task; no automatic replay"
        );
        std::process::exit(1);
    }
}
