//! B's SDK process adapter. Only the original NOVONET client owns network IO.
//! No ASAS worker, model executable, login, shell or agent loop is started here.
use crate::asas_route::{domain, STREAM};
use anyhow::{ensure, Context, Result};
use novovm_network::{
    native_application::{lane, NativeApplicationClientV1, DEADLINE, MAX_WIRE_BYTES},
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
        object_link::{NativeObjectLink, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
    product_overlay::{peer_id_from_mldsa65_public_key_v1, MLDSA65_PUBLIC_KEY_BYTES},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub const IPC_LIMIT: usize = 1024 * 1024;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientConfigV1 {
    pub version: u8,
    pub identity_path: PathBuf,
    pub chain_id: u64,
    pub library: PathBuf,
    pub library_sha256: String,
    pub server_key: Vec<u8>,
    pub server_address: SocketAddr,
    pub bind_address: SocketAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uca: Option<ClientUcaV1>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUcaV1 {
    pub identity: novovm_protocol::native_query::UcaIdentityIdV1,
    pub genesis: [u8; 32],
}
impl ClientConfigV1 {
    /// Public authorization target only; does not publish a grant or log in.
    pub fn uca_selector(
        &self,
    ) -> Result<novovm_protocol::uca_application::ApplicationGrantSelectorV1> {
        self.validate()?;
        let uca = self
            .uca
            .as_ref()
            .context("B UCA identity and genesis required")?;
        let key = crate::transport_identity::load_transport_identity(
            &self.identity_path,
            self.chain_id,
            &self.library,
            &self.library_sha256,
        )?;
        Ok(
            novovm_protocol::uca_application::ApplicationGrantSelectorV1 {
                chain_id: self.chain_id,
                genesis: uca.genesis,
                identity: uca.identity.clone(),
                application: novovm_protocol::asas_login::application_id(),
                service: crate::asas_route::auth::key_ref(&self.server_key)?,
                device: crate::asas_route::auth::key_ref(key.public_key())?,
            },
        )
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.identity_path.is_absolute() && self.library.is_absolute(),
            "explicit B identity/runtime paths required"
        );
        ensure!(
            self.uca.as_ref().is_none_or(|uca| uca.genesis != [0; 32]),
            "pinned UCA genesis required"
        );
        ensure!(
            self.library_sha256.len() == 64
                && self
                    .library_sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "qualified runtime digest required"
        );
        ensure!(
            self.server_key.len() == MLDSA65_PUBLIC_KEY_BYTES
                && self.server_key.iter().any(|&b| b != 0),
            "pinned A node key required"
        );
        ensure!(
            self.server_address.port() != 0
                && !self.server_address.ip().is_unspecified()
                && !self.server_address.ip().is_multicast(),
            "explicit A endpoint required"
        );
        ensure!(
            self.server_address.is_ipv4() == self.bind_address.is_ipv4(),
            "address family mismatch"
        );
        Ok(())
    }
    pub fn load(file: &Path, expected: &str) -> Result<Self> {
        ensure!(
            file.is_absolute() && expected.len() == 64,
            "pinned B profile required"
        );
        let metadata = std::fs::metadata(file)?;
        ensure!(
            metadata.is_file() && metadata.len() <= 64 * 1024,
            "B profile bounds"
        );
        let bytes = std::fs::read(file)?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == expected,
            "B profile changed; explicit reconnect required"
        );
        let value: Self = serde_json::from_slice(&bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub fn server_id(&self) -> String {
        peer_id_from_mldsa65_public_key_v1(&self.server_key)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    operation: String,
    #[serde(default)]
    input: Value,
}
pub struct ClientOwnerV1 {
    link: NativeObjectLink,
    application: NativeApplicationClientV1,
    readiness: tokio::net::UdpSocket,
    login: Option<Value>,
}
fn wall_ms() -> Result<u64> {
    novovm_network::product_rendezvous_daemon::now_ms()
}
impl ClientOwnerV1 {
    pub async fn connect(config: &ClientConfigV1) -> Result<Self> {
        config.validate()?;
        let key = crate::transport_identity::load_transport_identity(
            &config.identity_path,
            config.chain_id,
            &config.library,
            &config.library_sha256,
        )?;
        Self::connect_with_identity(config, key).await
    }
    pub(crate) async fn connect_with_identity(
        config: &ClientConfigV1,
        key: novovm_network::product_overlay::TransportIdentity,
    ) -> Result<Self> {
        let socket = UdpSocket::bind(config.bind_address)?;
        socket.set_nonblocking(true)?;
        let readiness = tokio::net::UdpSocket::from_std(socket.try_clone()?)?;
        let endpoint = NativeEndpoint::connect_clients(
            socket,
            key.clone(),
            vec![NativePeer {
                key: config.server_key.clone(),
                address: config.server_address,
            }],
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(10),
                retry: Duration::from_millis(200),
            },
            wall_ms()?,
            Instant::now(),
        )?;
        let mut link = NativeObjectLink::new(
            endpoint,
            vec![lane(STREAM, domain(), MAX_WIRE_BYTES * 2)?],
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: DEADLINE,
            },
        )?;
        tokio::time::timeout(Duration::from_secs(10), async {
            while link.session_id(0).is_none() {
                let _ = link.poll(wall_ms()?, Instant::now(), 64)?;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("A node handshake deadline")??;
        ensure!(
            link.peer_id(0)? == config.server_id(),
            "A node pin mismatch"
        );
        let application = NativeApplicationClientV1::attach(&mut link, 0, STREAM, domain())?;
        let mut owner = Self {
            link,
            application,
            readiness,
            login: None,
        };
        if let Some(uca) = &config.uca {
            owner.login(config, uca, &key).await?;
        }
        Ok(owner)
    }
    async fn login(
        &mut self,
        config: &ClientConfigV1,
        uca: &ClientUcaV1,
        key: &novovm_network::product_overlay::TransportIdentity,
    ) -> Result<()> {
        use novovm_protocol::asas_login::{
            application_id, LoginChallengeV1, LoginProofV1, CONTEXT,
        };
        let challenge: LoginChallengeV1 = serde_json::from_value(
            self.request(
                "login_begin",
                json!({"identity":uca.identity,"public_key":key.public_key()}),
            )
            .await?,
        )?;
        let now = wall_ms()?;
        ensure!(
            challenge.selector.chain_id == config.chain_id
                && challenge.selector.genesis == uca.genesis
                && challenge.selector.identity == uca.identity
                && challenge.selector.application == application_id()
                && challenge.selector.service
                    == crate::asas_route::auth::key_ref(&config.server_key)?
                && challenge.selector.device == crate::asas_route::auth::key_ref(key.public_key())?
                && Some(challenge.session) == self.link.session_id(0)
                && now < challenge.expires_at_ms
                && challenge.expires_at_ms <= now.saturating_add(30_000),
            "A login challenge differs from pinned B scope"
        );
        let bytes = challenge.signing_bytes().map_err(anyhow::Error::msg)?;
        let proof = LoginProofV1 {
            challenge: challenge.clone(),
            public_key: key.public_key().to_vec(),
            signature: key.sign(&bytes, CONTEXT)?,
        };
        let response = self
            .request("login_complete", serde_json::to_value(proof)?)
            .await?;
        ensure!(
            response["version"] == 1
                && response["status"] == "authenticated"
                && response["identity"] == serde_json::to_value(&uca.identity)?
                && response["revision"].as_str() == Some(challenge.revision.to_string().as_str())
                && response["service"] == serde_json::to_value(challenge.selector.service)?
                && response["device"] == serde_json::to_value(challenge.selector.device)?,
            "A login completion binding mismatch"
        );
        self.login = Some(response);
        Ok(())
    }
    pub(crate) async fn request(&mut self, operation: &str, input: Value) -> Result<Value> {
        ensure!(
            [
                "capabilities",
                "submit",
                "events",
                "result",
                "cancel",
                "login_begin",
                "login_complete"
            ]
            .contains(&operation),
            "unknown B operation"
        );
        let payload = serde_json::to_vec(&json!({"operation":operation,"input":input}))?;
        ensure!(payload.len() < IPC_LIMIT, "B request bound");
        self.application.begin(&payload, Instant::now())?;
        let mut peek = vec![0; 65536];
        loop {
            let now = Instant::now();
            let _ = self.link.poll(wall_ms()?, now, 64)?;
            if let Some(bytes) = self.application.poll(&mut self.link, now)? {
                let mut value: Value = serde_json::from_slice(&bytes)?;
                if operation == "capabilities" {
                    value["serverNodeId"] = self.link.peer_id(0)?.into();
                    value["localNodeId"] =
                        peer_id_from_mldsa65_public_key_v1(self.link.local_identity()).into();
                    if let Some(login) = &self.login {
                        ensure!(
                            value.get("ucaLogin") == Some(login),
                            "A login state changed"
                        );
                    }
                }
                return Ok(value);
            }
            tokio::select! {
                result = self.readiness.peek_from(&mut peek) => { result?; },
                _ = tokio::time::sleep(Duration::from_millis(10)) => {},
            }
        }
    }
    pub async fn serve_stdio(mut self) -> Result<()> {
        self.serve_io(BufReader::new(tokio::io::stdin()), tokio::io::stdout())
            .await
    }
    async fn serve_io<R: tokio::io::AsyncBufRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
        &mut self,
        mut input: R,
        mut output: W,
    ) -> Result<()> {
        let mut frame = Vec::new();
        let mut previous = None;
        loop {
            let mut bounded = (&mut input).take((IPC_LIMIT + 1 - frame.len()) as u64);
            let read = tokio::select! {
                read = bounded.read_until(b'\n', &mut frame) => Some(read?),
                _ = tokio::time::sleep(Duration::from_millis(25)) => None,
            };
            let _ = self.link.poll(wall_ms()?, Instant::now(), 64)?;
            let Some(read) = read else {
                continue;
            };
            if read == 0 {
                ensure!(frame.is_empty(), "partial B IPC frame");
                return Ok(());
            }
            ensure!(
                frame.len() <= IPC_LIMIT && frame.last() == Some(&b'\n'),
                "B IPC frame bound"
            );
            let request: Request = serde_json::from_slice(&frame)?;
            frame.clear();
            ensure!(
                request.id <= 9_007_199_254_740_991 && previous.is_none_or(|id| request.id > id),
                "B IPC identity order"
            );
            previous = Some(request.id);
            let value = self.request(&request.operation, request.input).await;
            let failed = value.is_err();
            let value = match value {
                Ok(result) => json!({"id":request.id,"result":result}),
                Err(_) => {
                    json!({"id":request.id,"error":"ASAS request failed; inspect original task; no replay"})
                }
            };
            let mut bytes = serde_json::to_vec(&value)?;
            ensure!(bytes.len() < IPC_LIMIT, "B IPC response bound");
            bytes.push(b'\n');
            output.write_all(&bytes).await?;
            output.flush().await?;
            ensure!(!failed, "B connection requires explicit recovery");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn b_frames_never_accept_a_host_command_or_spoofed_principal() {
        assert!(serde_json::from_value::<Request>(
            json!({"id":1,"operation":"submit","peerId":"A","input":{}})
        )
        .is_err());
        assert!(serde_json::from_value::<ClientConfigV1>(json!({"entry":"A-server.js"})).is_err());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn b_sdk_uses_native_client_handshake_and_principal_free_stdio() -> Result<()> {
        use crate::transport_identity::component_fixture::component_identity;
        use novovm_network::native_application::{
            ApplicationFutureV1, AuthenticatedApplicationPeerV1, NativeApplicationDispatcherV1,
            NativeApplicationHandlerV1, NativeApplicationRouteV1,
        };
        use std::sync::Arc;
        struct Fixture;
        impl NativeApplicationHandlerV1 for Fixture {
            fn request(
                &self,
                peer: AuthenticatedApplicationPeerV1,
                input: Vec<u8>,
            ) -> ApplicationFutureV1 {
                Box::pin(async move {
                    ensure!(peer.is_current(), "retired peer");
                    let request: Value = serde_json::from_slice(&input)?;
                    ensure!(
                        request["operation"] == "capabilities" && request.get("peerId").is_none(),
                        "B envelope"
                    );
                    Ok(serde_json::to_vec(
                        &json!({"version":2,"remoteRouteReady":true,"peerId":peer.id()}),
                    )?)
                })
            }
        }
        let a = component_identity([95; 32]);
        let b = component_identity([96; 32]);
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let address = socket.local_addr()?;
        let endpoint = NativeEndpoint::listen_clients(
            socket,
            a.clone(),
            2,
            vec![STREAM],
            NativeHandshakeBudget {
                lifetime: DEADLINE,
                retry: Duration::from_millis(10),
            },
        )?;
        let mut link = NativeObjectLink::new(
            endpoint,
            vec![lane(STREAM, domain(), MAX_WIRE_BYTES * 2)?],
            NativeSendPolicy {
                window: NovoRudpWindowConfig::default(),
                pacing: NovoRudpPacingProfile::default(),
                ttl: DEADLINE,
            },
        )?;
        let mut dispatcher = NativeApplicationDispatcherV1::new(NativeApplicationRouteV1 {
            stream: STREAM,
            domain: domain(),
            handler: Arc::new(Fixture),
        });
        let config = ClientConfigV1 {
            version: 1,
            identity_path: "/fixture/not-loaded".into(),
            chain_id: 1,
            library: "/fixture/not-loaded.so".into(),
            library_sha256: "a".repeat(64),
            server_key: a.public_key().to_vec(),
            server_address: address,
            bind_address: "127.0.0.1:0".parse()?,
            uca: None,
        };
        let server = async {
            loop {
                let now = Instant::now();
                let _ = link.poll(wall_ms()?, now, 64)?;
                dispatcher.poll(&mut link, now, None)?;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        };
        let client = async {
            let mut owner = ClientOwnerV1::connect_with_identity(&config, b.clone()).await?;
            let (io, mut harness) = tokio::io::duplex(65536);
            let (reader, writer) = tokio::io::split(io);
            let serving = owner.serve_io(BufReader::new(reader), writer);
            let exchange = async {
                harness
                    .write_all(b"{\"id\":0,\"operation\":\"capabilities\"}\n")
                    .await?;
                let mut line = String::new();
                let mut reader = BufReader::new(&mut harness);
                reader.read_line(&mut line).await?;
                let value: Value = serde_json::from_str(&line)?;
                assert_eq!(value["result"]["serverNodeId"], config.server_id());
                assert_eq!(
                    value["result"]["peerId"],
                    peer_id_from_mldsa65_public_key_v1(b.public_key())
                );
                drop(reader);
                harness
                    .write_all(
                        b"{\"id\":1,\"operation\":\"capabilities\",\"peerId\":\"spoofed\"}\n",
                    )
                    .await?;
                Ok::<_, anyhow::Error>(())
            };
            let (result, exchange) = tokio::join!(serving, exchange);
            exchange?;
            ensure!(result.is_err(), "spoofed principal must close B IPC");
            Ok::<_, anyhow::Error>(())
        };
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::select! { result = server => result, result = client => result }
        })
        .await??;
        Ok(())
    }
}
