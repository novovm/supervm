//! Explicit real Rust -> Node IPC check; starts no SDK/model inference or node.
use anyhow::{ensure, Context, Result};
use novovm_asas_plugin::{Config, Worker};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "usage: worker_smoke ABSOLUTE_NODE ABSOLUTE_BUILT_MAIN_JS"
    );
    let root = std::env::temp_dir().join(format!(
        "asas-ipc-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    std::fs::create_dir(&root)?;
    let _cleanup = Cleanup(root.clone());
    let entry = PathBuf::from(&args[1]);
    let service_config = root.join("service.json");
    std::fs::write(
        &service_config,
        serde_json::to_vec(&serde_json::json!({
            "directory":root.join("data"),"models":["default"],
            "peers":[{"id":"fixture-B","tokenSha256":"a".repeat(64),"projects":["b".repeat(64)]}]
        }))?,
    )?;
    let config = Config {
        version: 1,
        node: PathBuf::from(&args[0]),
        entry_sha256: format!("{:x}", Sha256::digest(std::fs::read(&entry)?)),
        entry,
        service_config,
        shutdown_ms: 5000,
        network_peers: Default::default(),
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut worker = Worker::start(&config).await.context("real ASAS startup")?;
            ensure!(
                worker
                    .request("not-authorized", "submit", serde_json::json!({}))
                    .await
                    .is_err(),
                "unrecognized principal must fail"
            );
            worker
                .shutdown()
                .await
                .context("ASAS remains alive after an application rejection")
        })?;
    ensure!(
        !root.join("data/owner.lock").exists(),
        "worker failed to release its own data lock"
    );
    println!("ASAS Rust/Node IPC handshake, principal rejection and orderly drain passed; no inference or NOVONET route tested");
    Ok(())
}
