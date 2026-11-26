//! Two authenticated Status-only native providers and one independently verified
//! archive. All three sockets and the installed Client are actual loopback peers.
use super::*;
use crate::native_pipeline::consensus::{native::query_lane, transport::fragment_domain};
use crate::native_pipeline::service::client::operator::ClientConfig;
use native_query::{Answer, QueryError, Response, STREAM};
use novovm_network::novorudp::{
    endpoint::{NativeEndpoint, NativeHandshakeBudget},
    fragments::{FragmentProfile, OutgoingMessage},
    object_link::{NativeObjectLink, NativeSendPolicy},
};
use novovm_relay::transport_identity::ephemeral_transport_identity;
use sha2::{Digest, Sha256};
use std::{fs, net::UdpSocket, process::Stdio, sync::Arc};

struct StatusPeer {
    link: NativeObjectLink,
    pin: Value,
    domain: [u8; 32],
    pending: Option<(usize, Arc<OutgoingMessage>)>,
    statuses: usize,
    rejected_headers: usize,
}

impl StatusPeer {
    fn open(config: &ClientConfig, domain: [u8; 32]) -> Result<Self> {
        let (identity, _) = ephemeral_transport_identity(&config.library, &config.library_sha256)?;
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let pin = serde_json::json!({"transport_key":identity.public_key(), "address":socket.local_addr()?});
        let mut endpoint = NativeEndpoint::listen_clients(
            socket,
            identity,
            2,
            vec![STREAM],
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(30),
                retry: Duration::from_millis(50),
            },
        )?;
        // Match the node ingress policy for reauthentication from a new client socket.
        endpoint.enable_client_reconnect_v1()?;
        let link = NativeObjectLink::new(
            endpoint,
            vec![query_lane(domain)],
            NativeSendPolicy {
                window: Default::default(),
                pacing: Default::default(),
                ttl: Duration::from_secs(10),
            },
        )?;
        Ok(Self {
            link,
            pin,
            domain,
            pending: None,
            statuses: 0,
            rejected_headers: 0,
        })
    }

    fn poll(&mut self, owner: &mut QueryOwner) -> Result<()> {
        let now = Instant::now();
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        let _ = self.link.poll(wall, now, 32)?;
        if let Some((peer, body)) = &self.pending {
            if self.link.try_send(*peer, STREAM, body, now)? {
                self.pending = None;
            }
        } else if let Some(message) = self.link.poll_complete(STREAM, now, 8)? {
            let peer = self
                .link
                .peer_index(&message.peer)
                .context("authenticated query peer")?;
            let Frame::Request(request) =
                native_query::decode(&message.chunks.concat()).map_err(anyhow::Error::msg)?
            else {
                anyhow::bail!("expected a native read request");
            };
            let answer = match &request.query {
                Query::Status => {
                    self.statuses += 1;
                    owner.rpc.answer_carrier_query(&request.query, now)?
                }
                Query::FinalizedHeader(_) => {
                    self.rejected_headers += 1;
                    Answer::Error(QueryError::Unavailable)
                }
                _ => anyhow::bail!("unexpected request to Status-only provider"),
            };
            let bytes = native_query::encode(Frame::Response(Response { request, answer }))
                .map_err(anyhow::Error::msg)?;
            self.pending = Some((
                peer,
                Arc::new(OutgoingMessage::with_profile(
                    self.domain,
                    bytes,
                    native_query::MAX_BYTES,
                    FragmentProfile::NativeUdpV2,
                )?),
            ));
        }
        Ok(())
    }
}

pub(super) fn verify(
    owner: &mut QueryOwner,
    binary: &Path,
    profile: &Path,
    config: &ClientConfig,
    provider_key: &[u8],
    pins: (u64, [u8; 32], [u8; 32]),
) -> Result<()> {
    let domain = fragment_domain(pins.0, pins.1, pins.2);
    let mut peers = [
        StatusPeer::open(config, domain)?,
        StatusPeer::open(config, domain)?,
    ];
    ensure!(
        peers[0].pin["transport_key"] != peers[1].pin["transport_key"]
            && peers[0].pin["address"] != peers[1].pin["address"],
        "duplicate test route"
    );
    let stage = profile.parent().context("test stage")?;
    let mut installed: Value = serde_json::from_slice(&fs::read(profile)?)?;
    installed["native_query_peers"] = serde_json::json!([peers[0].pin, peers[1].pin]);
    for peer in &peers {
        installed["lookup_transport_keys"]
            .as_array_mut()
            .context("lookup keys")?
            .push(peer.pin["transport_key"].clone());
    }
    let first_key = installed["seeds"][0]["transport_key"].clone();
    installed["seeds"][0] =
        serde_json::json!({"transport_key":first_key,"locator":stage.join("missing-source.json")});
    let bytes = serde_json::to_vec(&installed)?;
    let pin = format!("{:x}", Sha256::digest(&bytes));
    fs::write(profile, bytes)?;
    let provider = format!("{:x}", Sha256::digest(provider_key));
    let head = owner.rpc.node.controller.parent();
    let baseline = std::env::var_os("NOVOVM_QUERY_COPY_FALLBACK_BASELINE").map(PathBuf::from);
    for (exe, legacy) in baseline
        .as_deref()
        .map(|path| (path, true))
        .into_iter()
        .chain([(binary, false)])
    {
        let label = if legacy { "baseline" } else { "candidate" };
        let data = stage.join(format!("{label}-device"));
        ensure!(!data.exists(), "expected empty Client state");
        let log = stage.join(format!("{label}-follow.jsonl"));
        let mut child = IsolatedFollowChild(
            Command::new(exe)
                .arg(profile)
                .arg("follow-state")
                .arg(&pin)
                .env("LOCALAPPDATA", &data)
                .env("XDG_STATE_HOME", &data)
                .stdout(Stdio::from(fs::File::create_new(&log)?))
                .stderr(Stdio::from(fs::File::create_new(
                    stage.join(format!("{label}-stderr.log")),
                )?))
                .spawn()?,
        );
        let mut accepted = false;
        let mut verified = false;
        for peer in &mut peers {
            peer.statuses = 0;
            peer.rejected_headers = 0;
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            owner.poll()?;
            for peer in &mut peers {
                peer.poll(owner)?;
            }
            ensure!(child.0.try_wait()?.is_none(), "{label} Client exited");
            for line in fs::read_to_string(&log)?
                .split_inclusive('\n')
                .filter(|line| line.ends_with('\n'))
            {
                let event: Value = serde_json::from_str(line)?;
                if event["event"] == "verified_locator_accepted"
                    && event["peer_transport_sha256"] == provider
                {
                    accepted = true;
                }
                if event["event"] == "verified_state" {
                    ensure!(
                        accepted
                            && event["transport"] == "iroh"
                            && event["peer_transport_sha256"] == provider
                            && event["height"] == head.height
                            && event["state_root"] == super::super::hex(&head.state_root)
                            && event["verified_nodes"].as_u64().is_some_and(|n| n > 0)
                            && event["downloaded_nodes"] == event["verified_nodes"],
                        "unexpected verified state: {event}"
                    );
                    verified = true;
                }
            }
            if verified
                || peers.iter().all(|p| p.rejected_headers >= 2)
                || Instant::now() >= deadline
            {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let counts: Vec<_> = peers
            .iter()
            .map(|p| (&p.pin["address"], p.statuses, p.rejected_headers))
            .collect();
        println!("{label}: pid={}, native (address, statuses, rejected_headers)={counts:?} accepted_archive={accepted} verified_archive={verified} profile_sha256={pin}",child.0.id());
        ensure!(
            accepted
                && peers
                    .iter()
                    .all(|p| p.statuses >= 2 && p.rejected_headers >= 1),
            "{label} did not exercise both authenticated Status-only routes"
        );
        if legacy {
            ensure!(
                !verified && peers.iter().all(|p| p.rejected_headers >= 2),
                "baseline did not repeat both failed native routes"
            );
        } else {
            ensure!(verified, "Client repeated the failed native routes without synchronizing through the Iroh archive");
        }
    }
    Ok(())
}
