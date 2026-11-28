//! Session admission reads fresh published state through the original node SDK.
use super::*;
use novovm_asas_plugin::UcaAuthorityConfigV1;
use novovm_network::product_overlay::{peer_id_from_mldsa65_public_key_v1, TransportIdentity};
use novovm_protocol::{
    asas_login::{application_id, LoginChallengeV1, LoginProofV1, CONTEXT},
    native_query::{ApplicationGrantViewV1, UcaIdentityIdV1},
    uca_application::ApplicationGrantSelectorV1,
};
use serde_json::json;
use std::{path::Path, time::Instant};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

pub(crate) fn key_ref(public: &[u8]) -> Result<[u8; 32]> {
    ensure!(public.len() == 1952, "ML-DSA device key required");
    novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1(
        novovm_adapter_api::UcaKeyAlgo::Mldsa65,
        public,
    )
    .try_into()
    .map_err(|_| anyhow::anyhow!("device reference length"))
}
fn pinned(path: &Path, digest: &str, max: u64) -> Result<()> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    ensure!(file.metadata()?.len() <= max, "authority file bound");
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == digest,
        "authority file changed"
    );
    Ok(())
}
struct Authority {
    _child: Child,
    input: tokio::process::ChildStdin,
    output: BufReader<tokio::process::ChildStdout>,
    next: u64,
    chain_id: u64,
    genesis: [u8; 32],
    failed: bool,
}
trait PublishedGrants: Send {
    fn domain(&self) -> (u64, [u8; 32]);
    fn read<'a>(
        &'a mut self,
        selector: &'a ApplicationGrantSelectorV1,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ApplicationGrantViewV1>> + Send + 'a>,
    >;
}
impl PublishedGrants for Authority {
    fn domain(&self) -> (u64, [u8; 32]) {
        (self.chain_id, self.genesis)
    }
    fn read<'a>(
        &'a mut self,
        selector: &'a ApplicationGrantSelectorV1,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ApplicationGrantViewV1>> + Send + 'a>,
    > {
        Box::pin(self.observe(selector))
    }
}
impl Authority {
    async fn start(config: &UcaAuthorityConfigV1) -> Result<Self> {
        pinned(
            &config.executable,
            &config.executable_sha256,
            1024 * 1024 * 1024,
        )?;
        pinned(&config.config, &config.config_sha256, 4 * 1024 * 1024)?;
        let mut command = Command::new(&config.executable);
        command
            .arg(&config.config)
            .arg("--asas-authority")
            .arg(&config.config_sha256)
            .env_clear()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
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
        let mut child = command.spawn()?;
        let input = child.stdin.take().context("authority pipe")?;
        let output = BufReader::new(child.stdout.take().context("authority output")?);
        let mut owner = Self {
            _child: child,
            input,
            output,
            next: 0,
            chain_id: 0,
            genesis: [0; 32],
            failed: false,
        };
        let hello = owner.read().await?;
        ensure!(
            hello["version"] == 1 && hello["role"] == "asas-published-authority",
            "native authority contract"
        );
        owner.chain_id = hello["chain_id"].as_u64().context("authority chain")?;
        owner.genesis = serde_json::from_value(hello["genesis"].clone())?;
        ensure!(
            owner.chain_id > 0 && owner.genesis != [0; 32],
            "authority domain missing"
        );
        Ok(owner)
    }
    async fn read(&mut self) -> Result<Value> {
        timeout(Duration::from_secs(10), async {
            let mut bytes = Vec::new();
            (&mut self.output)
                .take(8193)
                .read_until(b'\n', &mut bytes)
                .await?;
            ensure!(
                bytes.len() <= 8192 && bytes.last() == Some(&b'\n'),
                "authority response bound"
            );
            Ok(serde_json::from_slice(&bytes)?)
        })
        .await
        .context("authority deadline")?
    }
    async fn observe(
        &mut self,
        selector: &ApplicationGrantSelectorV1,
    ) -> Result<ApplicationGrantViewV1> {
        ensure!(!self.failed, "authority pipe requires restart");
        let result = async {
            self.next = self
                .next
                .checked_add(1)
                .context("authority sequence exhausted")?;
            let bytes = serde_json::to_vec(&json!({"id":self.next,"selector":selector}))?;
            self.input.write_all(&bytes).await?;
            self.input.write_all(b"\n").await?;
            self.input.flush().await?;
            let reply = self.read().await?;
            ensure!(
                reply["id"].as_u64() == Some(self.next),
                "authority response identity mismatch"
            );
            Ok::<_, anyhow::Error>(reply)
        }
        .await;
        if result.is_err() {
            self.failed = true;
        }
        let reply = result?;
        ensure!(
            reply.get("error").is_none(),
            "published authority unavailable"
        );
        Ok(serde_json::from_value(reply["result"].clone())?)
    }
}
type Binding = (String, [u8; 16]);
struct Pending {
    challenge: LoginChallengeV1,
    peer: AuthenticatedApplicationPeerV1,
    until: Instant,
}
struct Session {
    challenge: LoginChallengeV1,
    peer: AuthenticatedApplicationPeerV1,
    until: Instant,
}
pub(super) struct LoginGate {
    authority: Box<dyn PublishedGrants>,
    identity: TransportIdentity,
    pending: BTreeMap<Binding, Pending>,
    sessions: BTreeMap<Binding, Session>,
}
impl LoginGate {
    pub(super) fn status(&self, peer: &AuthenticatedApplicationPeerV1) -> Value {
        match self
            .sessions
            .get(&(peer.id().to_owned(), peer.session_id()))
        {
            Some(session) => {
                json!({"version":1,"status":"authenticated", "identity":session.challenge.selector.identity,
                "revision":session.challenge.revision.to_string(),"service":session.challenge.selector.service,"device":session.challenge.selector.device})
            }
            None => json!({"version":1,"status":"login_required"}),
        }
    }
    pub(super) async fn start(
        config: &UcaAuthorityConfigV1,
        identity: TransportIdentity,
    ) -> Result<Self> {
        Ok(Self {
            authority: Box::new(Authority::start(config).await?),
            identity,
            pending: BTreeMap::new(),
            sessions: BTreeMap::new(),
        })
    }
    pub(super) async fn request(
        &mut self,
        peer: &AuthenticatedApplicationPeerV1,
        operation: &str,
        input: &Value,
    ) -> Result<Option<Value>> {
        let now = Instant::now();
        self.pending
            .retain(|_, entry| entry.peer.is_current() && now < entry.until);
        self.sessions
            .retain(|_, entry| entry.peer.is_current() && now < entry.until);
        let binding = (peer.id().to_owned(), peer.session_id());
        if operation == "login_begin" {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Begin {
                identity: UcaIdentityIdV1,
                public_key: Vec<u8>,
            }
            let begin: Begin = serde_json::from_value(input.clone())?;
            ensure!(
                peer_id_from_mldsa65_public_key_v1(&begin.public_key) == peer.id(),
                "device differs from authenticated peer"
            );
            self.pending.remove(&binding);
            self.sessions.remove(&binding);
            ensure!(
                self.pending.len() + self.sessions.len() < 64,
                "ASAS login capacity"
            );
            let (chain_id, genesis) = self.authority.domain();
            let selector = ApplicationGrantSelectorV1 {
                chain_id,
                genesis,
                identity: begin.identity,
                application: application_id(),
                service: key_ref(self.identity.public_key())?,
                device: key_ref(&begin.public_key)?,
            };
            let view = self.authority.read(&selector).await?;
            ensure!(
                view.permits_device_at_observed_height(&selector, view.revision),
                "current UCA application permission required"
            );
            let challenge = LoginChallengeV1 {
                version: 1,
                selector,
                revision: view.revision,
                session: peer.session_id(),
                nonce: rand::random(),
                expires_at_ms: novovm_network::product_rendezvous_daemon::now_ms()?
                    .checked_add(30_000)
                    .context("login clock")?,
            };
            self.pending.insert(
                binding,
                Pending {
                    challenge: challenge.clone(),
                    peer: peer.clone(),
                    until: now + Duration::from_secs(30),
                },
            );
            return Ok(Some(serde_json::to_value(challenge)?));
        }
        if operation == "login_complete" {
            // Consume BEFORE checking or awaiting: even an invalid proof is one-use.
            let pending = self
                .pending
                .remove(&binding)
                .context("fresh login challenge required")?;
            let proof: LoginProofV1 = serde_json::from_value(input.clone())?;
            ensure!(
                proof.challenge == pending.challenge
                    && peer.is_current()
                    && Instant::now() < pending.until
                    && novovm_network::product_rendezvous_daemon::now_ms()?
                        < proof.challenge.expires_at_ms
                    && peer_id_from_mldsa65_public_key_v1(&proof.public_key) == peer.id()
                    && key_ref(&proof.public_key)? == proof.challenge.selector.device,
                "login proof binding mismatch"
            );
            self.identity.verify(
                &proof.public_key,
                &proof
                    .challenge
                    .signing_bytes()
                    .map_err(anyhow::Error::msg)?,
                CONTEXT,
                &proof.signature,
            )?;
            let view = self.authority.read(&proof.challenge.selector).await?;
            ensure!(
                view.permits_device_at_observed_height(
                    &proof.challenge.selector,
                    proof.challenge.revision
                ) && peer.is_current(),
                "login grant changed"
            );
            let response = json!({"version":1,"status":"authenticated","identity":proof.challenge.selector.identity,
                "revision":view.revision.to_string(),"service":proof.challenge.selector.service,"device":proof.challenge.selector.device});
            self.sessions.insert(
                binding,
                Session {
                    challenge: proof.challenge,
                    peer: peer.clone(),
                    until: now + Duration::from_secs(900),
                },
            );
            return Ok(Some(response));
        }
        let session = self.sessions.get(&binding).context("UCA login required")?;
        let challenge = session.challenge.clone();
        let observed = self.authority.read(&challenge.selector).await;
        let checked = observed.and_then(|view| {
            ensure!(
                view.permits_device_at_observed_height(&challenge.selector, challenge.revision)
                    && peer.is_current(),
                "UCA login revoked or superseded"
            );
            Ok(())
        });
        if checked.is_err() {
            self.sessions.remove(&binding);
        }
        checked?;
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
