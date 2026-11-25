//! Real native sessions; grant snapshots below are explicitly component fixtures.
//! These tests do not claim production-chain or official-engine acceptance.
use super::*;
use crate::asas_client::{ClientConfigV1, ClientOwnerV1};
use novovm_network::{
    native_application::{lane, NativeApplicationDispatcherV1, DEADLINE, MAX_WIRE_BYTES},
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget},
        object_link::{NativeObjectLink, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
};
use std::{net::UdpSocket, sync::Mutex};

struct FixtureReader {
    selector: ApplicationGrantSelectorV1,
    view: Arc<Mutex<ApplicationGrantViewV1>>,
}
impl PublishedGrants for FixtureReader {
    fn domain(&self) -> (u64, [u8; 32]) {
        (self.selector.chain_id, self.selector.genesis)
    }
    fn read<'a>(
        &'a mut self,
        selector: &'a ApplicationGrantSelectorV1,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ApplicationGrantViewV1>> + Send + 'a>,
    > {
        Box::pin(async move {
            ensure!(selector == &self.selector, "fixture scope mismatch");
            Ok(*self.view.lock().unwrap())
        })
    }
}
struct Capture(Arc<Mutex<Option<AuthenticatedApplicationPeerV1>>>);
impl NativeApplicationHandlerV1 for Capture {
    fn request(&self, peer: AuthenticatedApplicationPeerV1, _: Vec<u8>) -> ApplicationFutureV1 {
        *self.0.lock().unwrap() = Some(peer);
        Box::pin(async { Ok(b"{\"captured\":true}".to_vec()) })
    }
}
struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn peer(
    a: &TransportIdentity,
    b: &TransportIdentity,
) -> Result<(ClientOwnerV1, AuthenticatedApplicationPeerV1, Server)> {
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
    let capture = Arc::new(Mutex::new(None));
    let mut dispatcher = NativeApplicationDispatcherV1::new(NativeApplicationRouteV1 {
        stream: STREAM,
        domain: domain(),
        handler: Arc::new(Capture(capture.clone())),
    });
    let server = Server(tokio::spawn(async move {
        loop {
            let now = Instant::now();
            if link
                .poll(
                    novovm_network::product_rendezvous_daemon::now_ms().unwrap(),
                    now,
                    64,
                )
                .is_err()
                || dispatcher.poll(&mut link, now, None).is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }));
    let config = ClientConfigV1 {
        version: 1,
        identity_path: "/fixture/not-loaded".into(),
        chain_id: 1,
        library: "/fixture/not-loaded".into(),
        library_sha256: "a".repeat(64),
        server_key: a.public_key().to_vec(),
        server_address: address,
        bind_address: "127.0.0.1:0".parse()?,
        uca: None,
    };
    let mut client = ClientOwnerV1::connect_with_identity(&config, b.clone()).await?;
    client.request("capabilities", Value::Null).await?;
    let peer = capture
        .lock()
        .unwrap()
        .clone()
        .context("native peer not captured")?;
    Ok((client, peer, server))
}
async fn begin(
    gate: &mut LoginGate,
    peer: &AuthenticatedApplicationPeerV1,
    b: &TransportIdentity,
) -> Result<LoginProofV1> {
    let response = gate
        .request(
            peer,
            "login_begin",
            &json!({"identity":"uca:owner", "public_key":b.public_key()}),
        )
        .await?
        .context("challenge missing")?;
    let challenge: LoginChallengeV1 = serde_json::from_value(response)?;
    let signature = b.sign(
        &challenge.signing_bytes().map_err(anyhow::Error::msg)?,
        CONTEXT,
    )?;
    Ok(LoginProofV1 {
        challenge,
        public_key: b.public_key().to_vec(),
        signature,
    })
}
async fn exercise(a: TransportIdentity, b: TransportIdentity) -> Result<()> {
    let (_client, peer, _server) = peer(&a, &b).await?;
    let selector = ApplicationGrantSelectorV1 {
        chain_id: 1,
        genesis: [1; 32],
        identity: UcaIdentityIdV1::new("uca:owner").map_err(anyhow::Error::msg)?,
        application: application_id(),
        service: key_ref(a.public_key())?,
        device: key_ref(b.public_key())?,
    };
    let view = Arc::new(Mutex::new(ApplicationGrantViewV1 {
        state_root: [2; 32],
        height: 3,
        revision: 1,
        issuer_key_ref: [3; 32],
        identity_key_ref: [3; 32],
        identity_status: IdentityStatusV1::Active,
        revoked: false,
        not_before_height: 1,
        expires_at_height: 100,
    }));
    let mut gate = LoginGate {
        authority: Box::new(FixtureReader {
            selector,
            view: view.clone(),
        }),
        identity: a,
        pending: BTreeMap::new(),
        sessions: BTreeMap::new(),
    };
    assert!(gate.request(&peer, "submit", &Value::Null).await.is_err());
    let proof = begin(&mut gate, &peer, &b).await?;
    let old = serde_json::to_value(&proof)?;
    assert!(gate.request(&peer, "login_complete", &old).await?.is_some());
    assert!(gate.request(&peer, "events", &Value::Null).await?.is_none());
    assert!(gate.request(&peer, "login_complete", &old).await.is_err());
    view.lock().unwrap().revoked = true;
    assert!(gate.request(&peer, "events", &Value::Null).await.is_err());
    assert!(gate.sessions.is_empty());
    {
        let mut state = view.lock().unwrap();
        state.revoked = false;
        state.revision = 2;
    }
    assert!(gate.request(&peer, "login_complete", &old).await.is_err());
    let fresh = begin(&mut gate, &peer, &b).await?;
    assert_ne!(fresh.challenge.nonce, proof.challenge.nonce);
    assert!(gate.request(&peer, "login_complete", &old).await.is_err());
    assert!(gate
        .request(&peer, "login_complete", &serde_json::to_value(fresh)?)
        .await
        .is_err());
    for field in 0..7 {
        let mut changed = begin(&mut gate, &peer, &b).await?;
        match field {
            0 => changed.challenge.selector.service[0] ^= 1,
            1 => changed.challenge.selector.device[0] ^= 1,
            2 => changed.challenge.revision += 1,
            3 => changed.challenge.session[0] ^= 1,
            4 => changed.challenge.nonce[0] ^= 1,
            5 => changed.signature[0] ^= 1,
            _ => view.lock().unwrap().revision += 1,
        }
        assert!(gate
            .request(&peer, "login_complete", &serde_json::to_value(changed)?)
            .await
            .is_err());
    }
    let proof = begin(&mut gate, &peer, &b).await?;
    for pending in gate.pending.values_mut() {
        pending.until = Instant::now() - Duration::from_millis(1);
    }
    assert!(gate
        .request(&peer, "login_complete", &serde_json::to_value(proof)?)
        .await
        .is_err());
    let proof = begin(&mut gate, &peer, &b).await?;
    gate.request(&peer, "login_complete", &serde_json::to_value(proof)?)
        .await?;
    view.lock().unwrap().identity_status = IdentityStatusV1::Suspended;
    assert!(gate.request(&peer, "result", &Value::Null).await.is_err());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn native_session_login_consumes_challenges_and_rechecks_revocation() -> Result<()> {
    exercise(
        crate::transport_identity::component_fixture::component_identity([71; 32]),
        crate::transport_identity::component_fixture::component_identity([72; 32]),
    )
    .await
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "qualified AOEM required; real ML-DSA and UDP, fixture grant observations, no chain or model acceptance"]
async fn actual_mldsa_device_login_and_revocation_gate() -> Result<()> {
    use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade};
    let path = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit qualified library required")?;
    let facade = AoemExecFacade::open(Path::new(&path), Default::default())?;
    let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let mut identities = Vec::new();
    for _ in 0..2 {
        let key = runtime.generate_keypair()?;
        let material = crate::purpose_keys::PurposeKeyMaterialV3::new(
            key.public_key,
            key.expanded_secret_key,
        )?;
        identities.push(crate::transport_identity::from_mldsa(
            runtime.clone(),
            material,
        )?);
    }
    exercise(identities.remove(0), identities.remove(0)).await
}
