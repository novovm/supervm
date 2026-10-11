//! Real executables and Windows custody. The parent never signs an authority
//! manifest. All directories, policies and client keys are isolated TEST data.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    discovery_control::DiscoveryClient,
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    product_directory::{BootstrapSourceV1, BootstrapTrustPolicyV1, PeerSignedRelayRecordV1},
    product_rendezvous_daemon::now_ms,
    product_service_lease::{request_service_offer_v2, IrohServiceEndpointV1, ServiceLeaseBookV1},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::Read,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const PUBLISHER: &str = env!("CARGO_BIN_EXE_novovm-bootstrap-publisher");
const NODE: &str = env!("CARGO_BIN_EXE_novovm-rendezvous-service");
pub fn client() -> SigningKey {
    SigningKey::from_bytes(&[91; 32])
}
pub fn command(binary: &str) -> Command {
    let mut cmd = Command::new(binary);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000);
    cmd
}
pub fn once(action: &str, path: &Path, success: bool) -> Result<()> {
    let output = command(PUBLISHER).arg(action).arg(path).output()?;
    ensure!(
        output.status.success() == success,
        "publisher {action}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
pub struct Process(pub Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Process {
    pub async fn finish(&mut self, success: bool, seconds: u64) -> Result<()> {
        let until = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < until {
            if let Some(status) = self.0.try_wait()? {
                let mut error = String::new();
                self.0.stderr.as_mut().unwrap().read_to_string(&mut error)?;
                ensure!(
                    status.success() == success,
                    "unexpected process result: {error}"
                );
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        anyhow::bail!("process completion deadline")
    }
}
pub struct Fixture {
    pub root: PathBuf,
    pub publisher_pin: [u8; 32],
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
pub fn json_file(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let stage = path.with_extension("test-stage");
    std::fs::write(&stage, serde_json::to_vec(value)?)?;
    std::fs::rename(stage, path)?;
    Ok(())
}
pub async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        match std::fs::read(path) {
            Ok(bytes) => return Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && Instant::now() < until => {}
            Err(e) => return Err(e.into()),
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
impl Fixture {
    pub fn new() -> Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "novovm-publisher-process-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&root)?;
        std::fs::create_dir(root.join("publisher"))?;
        std::fs::create_dir(root.join("node"))?;
        once("--initialize", &root.join("publisher"), true)?;
        let identity: Value =
            serde_json::from_slice(&std::fs::read(root.join("publisher/identity.json"))?)?;
        let publisher_pin: [u8; 32] =
            serde_json::from_value(identity["publisher_public_key"].clone())?;
        std::fs::write(
            root.join("node/initialize.request"),
            b"explicit test enrollment",
        )?;
        json_file(
            &root.join("node-config.json"),
            &json!({
                "version":1,"prepare_before_ms":5000,"epoch":{
                    "version":2,"engineering_direct_allowed":true,"trusted_signers":[publisher_pin],
                    "minimum_signatures":1,"allowed_clients":[client().verifying_key().to_bytes()],"relays":[],
                    "nameserver":"127.0.0.1","bind_address":"127.0.0.1:0","relay_only":false,
                    "journal_path":root.join("node/service.journal"),"source_path":root.join("node/authority.json"),
                    "enroll":false,"record_id":"publisher-process-test-node","grant_sequence":1,"run_for_ms":10000,"max_sessions":64
                }
            }),
        )?;
        let output = command(NODE)
            .arg("--provision")
            .arg(root.join("node-config.json"))
            .output()?;
        ensure!(
            output.status.success(),
            "node provisioning failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(
            root.join("node/identity.json").is_file()
                && !root.join("node/service.journal").exists(),
            "provisioning must export identity without activating a carrier"
        );
        Ok(Self {
            root,
            publisher_pin,
        })
    }
    pub fn launch_node(&self, epochs: u8) -> Result<Process> {
        Ok(Process(
            command(NODE)
                .arg("--foreground")
                .arg(self.root.join("node-config.json"))
                .arg(epochs.to_string())
                .spawn()?,
        ))
    }
    pub fn launch_publisher(&self, seconds: u8) -> Result<Process> {
        Ok(Process(
            command(PUBLISHER)
                .arg("--foreground")
                .arg(self.root.join("publisher-config.json"))
                .arg(seconds.to_string())
                .spawn()?,
        ))
    }
    pub fn enroll(&self, node: [u8; 32]) -> Result<()> {
        let config = self.root.join("publisher-config.json");
        json_file(
            &config,
            &json!({
                "version":1,"state_directory":self.root.join("publisher"),"exchange_directory":self.root.join("node"),
                "publisher_public_key":self.publisher_pin,"node_public_key":node,
                "record_id":"publisher-process-test-node","manifest_id":"test-independent-publisher",
                "allowed_relays":[],"max_grant_ms":10000
            }),
        )?;
        once("--enroll-policy", &config, true)
    }
    pub async fn proposal(&self, seen: &BTreeSet<u64>) -> Result<PeerSignedRelayRecordV1> {
        let until = Instant::now() + Duration::from_secs(8);
        while Instant::now() < until {
            for entry in std::fs::read_dir(self.root.join("node"))? {
                let path = entry?.path();
                if path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".proposal.json")
                {
                    let record: PeerSignedRelayRecordV1 = read_json(&path).await?;
                    if !seen.contains(&record.sequence) {
                        return Ok(record);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        anyhow::bail!("proposal deadline")
    }
    pub fn authority_path(&self, sequence: u64) -> PathBuf {
        self.root
            .join(format!("node/epoch-{sequence}.authority.json"))
    }
    pub async fn source(&self, record: &PeerSignedRelayRecordV1) -> Result<Vec<BootstrapSourceV1>> {
        read_json(&self.authority_path(record.sequence)).await
    }
    pub async fn exchange(&self, record: &PeerSignedRelayRecordV1) -> Result<()> {
        let sources = self.source(record).await?;
        let endpoint = IrohServiceEndpointV1::from_record(record)?;
        let policy = BootstrapTrustPolicyV1::single_signer(self.publisher_pin);
        let node = record.relay_public_key;
        let mut book = ServiceLeaseBookV1::new(policy, sources, now_ms()?)?;
        book.check_route(
            node,
            endpoint.address.endpoint_key,
            endpoint.instance,
            now_ms()?,
        )?;
        tokio::time::sleep(Duration::from_millis(180)).await;
        let config = IrohConfigV1::new(&[], "127.0.0.1".parse()?)?
            .with_bind_address("127.0.0.1:0".parse()?)?;
        with_iroh_scope_v1(
            config,
            IrohControlV1::new(Instant::now() + Duration::from_secs(3), || Ok(()))?,
            move |scope| {
                Box::pin(async move {
                    let mut stream = scope.connect(&endpoint.address).await?.into();
                    let offer = request_service_offer_v2(&mut stream).await?;
                    book.accept_offer(
                        offer,
                        node,
                        endpoint.address.endpoint_key,
                        endpoint.instance,
                        now_ms()?,
                    )?;
                    let mut rpc =
                        DiscoveryClient::connect(&mut stream, &client(), node, endpoint.instance)
                            .await?;
                    rpc.put([21; 32], 1, 2000, vec![22; 128]).await?;
                    let value = rpc.get([21; 32]).await?.context("missing opaque object")?;
                    ensure!(
                        value.ciphertext == vec![22; 128],
                        "opaque transfer mismatch"
                    );
                    ensure!(
                        rpc.ack([21; 32], value.revision, value.digest).await?,
                        "missing ACK"
                    );
                    ensure!(
                        !rpc.ack([21; 32], value.revision, value.digest).await?,
                        "duplicate ACK"
                    );
                    rpc.close().await
                })
            },
        )
        .await
    }
}
