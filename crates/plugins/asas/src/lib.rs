//! ASAS is an off-chain application service, never a transaction/consensus plugin.
//! The original NOVONET owner must authenticate peers before application dispatch.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

pub const PLUGIN_ID: &str = "asas";
pub const IPC_VERSION: u64 = 2;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub node: PathBuf,
    pub entry: PathBuf,
    pub entry_sha256: String,
    pub service_config: PathBuf,
    pub shutdown_ms: u64,
    #[serde(default)]
    pub network_peers: BTreeMap<String, String>,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "ASAS plugin configuration version");
        ensure!(
            self.network_peers.len() <= 64
                && self.network_peers.iter().all(|(node, principal)| {
                    node.strip_prefix("novovm-mldsa65:").is_some_and(|hash| {
                        hash.len() == 64
                            && hash
                                .bytes()
                                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                    }) && !principal.is_empty()
                        && principal.len() <= 200
                }),
            "ASAS explicit authenticated node/principal bindings required"
        );
        ensure!(
            self.node.is_absolute()
                && self.entry.is_absolute()
                && self.service_config.is_absolute(),
            "ASAS plugin paths must be absolute"
        );
        ensure!(
            (100..=30_000).contains(&self.shutdown_ms),
            "ASAS shutdown budget"
        );
        ensure!(
            self.entry_sha256.len() == 64
                && self.entry_sha256.bytes().all(|c| c.is_ascii_hexdigit()),
            "ASAS entry checksum required"
        );
        ensure!(
            self.entry.extension().is_some_and(|value| value == "js"),
            "ASAS requires a built JavaScript entry, never a shell command"
        );
        ensure!(
            self.node
                .file_name()
                .is_some_and(|value| { value == "node" || value == "node.exe" }),
            "ASAS requires an explicit Node runtime"
        );
        Ok(())
    }
}

/// A single, inherited private pipe. It opens no sockets, owns no node keys and
/// does not advertise a route until the original network dispatcher is wired.
pub struct Worker {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    next_id: u64,
    shutdown: Duration,
}

impl Worker {
    pub async fn start(config: &Config) -> Result<Self> {
        config.validate()?;
        let metadata = std::fs::metadata(&config.entry).context("ASAS entry unavailable")?;
        ensure!(
            metadata.is_file() && metadata.len() <= 2 * 1024 * 1024,
            "ASAS entry budget"
        );
        let digest = format!("{:x}", Sha256::digest(std::fs::read(&config.entry)?));
        ensure!(
            digest.eq_ignore_ascii_case(&config.entry_sha256),
            "ASAS entry checksum mismatch"
        );
        let mut command = Command::new(&config.node);
        command
            .arg(&config.entry)
            .arg("--stdio")
            .arg(&config.service_config)
            .current_dir(config.entry.parent().context("ASAS entry directory")?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // No inherited loader hooks, proxies, model tokens or engine selectors.
        command.env_clear();
        for name in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "SystemRoot",
            "TEMP",
            "TMP",
            "LANG",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut child = command.spawn().context("ASAS worker spawn failed")?;
        let input = child.stdin.take().context("ASAS stdin unavailable")?;
        let output = BufReader::new(child.stdout.take().context("ASAS stdout unavailable")?);
        let mut worker = Self {
            child,
            input: Some(input),
            output,
            next_id: 0,
            shutdown: Duration::from_millis(config.shutdown_ms),
        };
        let result = worker.exchange(json!({"operation":"health"})).await;
        match result {
            Ok(value)
                if value["version"].as_u64() == Some(IPC_VERSION)
                    && value["remoteRouteReady"] == false =>
            {
                Ok(worker)
            }
            _ => {
                let _ = worker.shutdown().await;
                anyhow::bail!("ASAS worker handshake failed; remote route not advertised")
            }
        }
    }

    async fn exchange(&mut self, mut request: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .context("ASAS IPC identity exhausted")?;
        request["id"] = json!(id);
        let mut bytes = serde_json::to_vec(&request)?;
        ensure!(bytes.len() < MAX_FRAME_BYTES, "ASAS IPC request budget");
        bytes.push(b'\n');
        let result = timeout(Duration::from_secs(30), async {
            let input = self.input.as_mut().context("ASAS worker stopped")?;
            input.write_all(&bytes).await?;
            input.flush().await?;
            let mut frame = Vec::new();
            (&mut self.output)
                .take((MAX_FRAME_BYTES + 1) as u64)
                .read_until(b'\n', &mut frame)
                .await?;
            ensure!(
                frame.len() <= MAX_FRAME_BYTES && frame.last() == Some(&b'\n'),
                "ASAS IPC frame budget or incomplete response"
            );
            let response: Value = serde_json::from_slice(&frame)?;
            ensure!(
                response["id"].as_u64() == Some(id),
                "ASAS IPC response identity mismatch"
            );
            ensure!(
                response.get("error").is_some() != response.get("result").is_some(),
                "ASAS ambiguous IPC response"
            );
            Ok::<Value, anyhow::Error>(response)
        })
        .await;
        match result {
            Ok(Ok(value)) => {
                ensure!(
                    value.get("error").is_none(),
                    "ASAS application rejected request; no replay"
                );
                value
                    .get("result")
                    .cloned()
                    .context("ASAS IPC result missing")
            }
            _ => {
                // A timeout may follow a real tool effect. Poison the pipe and
                // stop this worker; never reuse it for a new request/replay.
                self.input.take();
                let _ = self.child.kill().await;
                anyhow::bail!(
                    "ASAS IPC failed; execution state may be unknown; recover original task"
                )
            }
        }
    }

    /// Local host API only. peer_id is resolved by the owning authenticated
    /// network handler, NEVER taken from an untrusted application envelope.
    pub async fn request(&mut self, peer_id: &str, operation: &str, input: Value) -> Result<Value> {
        self.request_from_node(peer_id, None, operation, input)
            .await
    }

    /// node_id comes from the existing authenticated NOVONET connection, not B's body.
    pub async fn request_from_node(
        &mut self,
        peer_id: &str,
        node_id: Option<&str>,
        operation: &str,
        input: Value,
    ) -> Result<Value> {
        ensure!(
            ["capabilities", "submit", "events", "result", "cancel"].contains(&operation),
            "ASAS operation not supported"
        );
        ensure!(
            !peer_id.is_empty() && peer_id.len() <= 200,
            "ASAS principal required"
        );
        let mut request = json!({"peerId":peer_id,"operation":operation,"input":input});
        if let Some(node_id) = node_id {
            request["nodeId"] = node_id.into();
        }
        self.exchange(request).await
    }

    pub async fn health(&mut self) -> Result<Value> {
        self.exchange(json!({"operation":"health"})).await
    }

    pub async fn unexpected_exit(&mut self) -> Result<()> {
        let _ = self.child.wait().await?;
        anyhow::bail!("ASAS worker exited; tasks require inspection, not automatic restart")
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.input.take();
        match timeout(self.shutdown, self.child.wait()).await {
            Ok(result) => {
                ensure!(result?.success(), "ASAS worker failed during drain");
                Ok(())
            }
            Err(_) => {
                self.child.kill().await?;
                anyhow::bail!("ASAS drain timed out; inspect original tasks")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        let root = std::env::temp_dir();
        Config {
            version: 1,
            node: root.join(if cfg!(windows) { "node.exe" } else { "node" }),
            entry: root.join("main.js"),
            entry_sha256: "a".repeat(64),
            service_config: root.join("asas.json"),
            shutdown_ms: 5000,
            network_peers: BTreeMap::new(),
        }
    }
    #[test]
    fn explicit_local_paths_version_runtime_and_budgets() {
        let value = config();
        value.validate().unwrap();
        let mut bad = value.clone();
        bad.version = 2;
        assert!(bad.validate().is_err());
        bad = value.clone();
        bad.node = PathBuf::from("node");
        assert!(bad.validate().is_err());
        bad = value.clone();
        bad.entry = bad.entry.with_extension("sh");
        assert!(bad.validate().is_err());
        bad = value.clone();
        bad.shutdown_ms = 0;
        assert!(bad.validate().is_err());
        bad = value;
        bad.entry_sha256.clear();
        assert!(bad.validate().is_err());
    }
    #[test]
    fn network_grants_require_an_explicit_authenticated_node_id() {
        let mut value = config();
        value
            .network_peers
            .insert("body-supplied-peer".into(), "B".into());
        assert!(value.validate().is_err());
        value.network_peers.clear();
        value
            .network_peers
            .insert(format!("novovm-mldsa65:{}", "a".repeat(64)), "B".into());
        assert!(value.validate().is_ok());
    }
    #[test]
    fn rejects_unknown_configuration_instead_of_assuming_permissions() {
        let mut value = serde_json::to_value(config()).unwrap();
        value["share_credentials"] = json!(true);
        assert!(serde_json::from_value::<Config>(value).is_err());
    }
}
