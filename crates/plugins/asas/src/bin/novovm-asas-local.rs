//! Private OS-owned pipe entry for the same native plugin. No public listener,
//! node key, independent network, or production route advertisement.
use anyhow::{ensure, Result};
use novovm_asas_plugin::{Config, Worker, MAX_FRAME_BYTES};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    operation: String,
    #[serde(rename = "peerId")]
    peer_id: Option<String>,
    input: Option<Value>,
}

async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4 && args[0] == "--host" && args[2] == "--peer",
        "usage: --host PRIVATE_HOST_CONFIG --peer LOCAL_PRINCIPAL"
    );
    let peer = args[3]
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("local principal required"))?;
    ensure!(
        !peer.is_empty() && peer.len() <= 200,
        "local principal bounds"
    );
    let file = std::path::PathBuf::from(&args[1]);
    ensure!(
        file.is_absolute() && std::fs::metadata(&file)?.len() <= 65536,
        "private host configuration bounds"
    );
    let config: Config = serde_json::from_slice(&std::fs::read(file)?)?;
    let mut worker = Worker::start(&config).await?;
    let mut input = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    let result = async {
        loop {
            let mut frame = Vec::new();
            (&mut input).take((MAX_FRAME_BYTES + 1) as u64).read_until(b'\n', &mut frame).await?;
            if frame.is_empty() { break; }
            ensure!(frame.len() <= MAX_FRAME_BYTES && frame.last() == Some(&b'\n'), "local IPC frame bounds");
            let request: Request = serde_json::from_slice(&frame)?;
            ensure!(request.id <= 9_007_199_254_740_991, "IPC request identity bounds");
            let response = if request.operation == "health" {
                worker.health().await
            } else if request.peer_id.as_deref() == Some(peer) {
                worker.request(peer, &request.operation, request.input.unwrap_or(Value::Null)).await
            } else { Err(anyhow::anyhow!("local principal mismatch")) };
            let value = match response {
                Ok(value) => json!({"id":request.id,"result":value}),
                Err(_) => json!({"id":request.id,"error":"ASAS local request rejected; inspect original task; no replay"}),
            };
            let mut bytes = serde_json::to_vec(&value)?;
            ensure!(bytes.len() < MAX_FRAME_BYTES, "local IPC output bounds");
            bytes.push(b'\n'); output.write_all(&bytes).await?; output.flush().await?;
        }
        Ok::<(), anyhow::Error>(())
    }.await;
    let drain = worker.shutdown().await;
    result.and(drain)
}

fn main() {
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .and_then(|runtime| runtime.block_on(run()).map_err(std::io::Error::other));
    if result.is_err() {
        eprintln!("ASAS local plugin failed; inspect private task state; no automatic replay");
        std::process::exit(1);
    }
}
