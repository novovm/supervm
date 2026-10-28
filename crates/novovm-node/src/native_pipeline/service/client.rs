//! Native application adapter over the SAME NovoRUDP object implementation.
//! No HTTP/JSON, validator secret, mempool, business execution or consensus.
//! Transport completion means custody only. The application retains signed
//! transactions until it verifies their final receipts; queries are separate.
pub mod operator;
use crate::native_pipeline::{
    consensus::{
        message_evidence,
        native::{LaneLimits, MessageDomain, APPLICATION_STREAM},
        transport::{self, DecodeLimits, Message, TransactionsScope},
    },
    ingress::apfl::ApflTransferBatch,
};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::novorudp::{
    endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
    fragments::{FragmentProfile, OutgoingMessage},
    object_link::{NativeLaneConfig, NativeLinkPoll, NativeObjectLink, NativeSendPolicy},
};
use novovm_protocol::native_query::{self as query, Frame, Query, Request, Response};
use novovm_protocol::uca_spend_batch::AssetSpendBatchV2;
use std::{
    net::UdpSocket,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct PreparedQuery {
    owner: Arc<()>,
    request: Request,
    body: Arc<OutgoingMessage>,
}

pub struct PreparedApplicationInput {
    owner: Arc<()>,
    body: Arc<OutgoingMessage>,
}
impl PreparedApplicationInput {
    pub fn id(&self) -> [u8; 32] {
        self.body.id()
    }
    pub fn byte_len(&self) -> usize {
        self.body.byte_len()
    }
}

pub struct NativeApplicationClient {
    link: NativeObjectLink,
    domain: MessageDomain,
    codec: DecodeLimits,
    owner: Arc<()>,
    session: [u8; 32],
    sequence: u64,
    query_pending: Option<Request>,
}
impl NativeApplicationClient {
    #[allow(clippy::too_many_arguments)]
    pub fn connect(
        socket: UdpSocket,
        identity: SigningKey,
        node: NativePeer,
        domain: MessageDomain,
        codec: DecodeLimits,
        limits: LaneLimits,
        policy: NativeSendPolicy,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Self> {
        ensure!(
            domain.chain_id == domain.validators.chain_id()
                && domain.genesis != [0; 32]
                && domain.protocol != [0; 32],
            "invalid application chain domain"
        );
        ensure!(
            limits.receive.max_message_bytes <= codec.message_bytes,
            "application decoder budget mismatch"
        );
        let endpoint = NativeEndpoint::connect_client(
            socket,
            identity,
            node,
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(30),
                retry: Duration::from_millis(50),
            },
            wall_ms,
            now,
        )?;
        let lane = NativeLaneConfig {
            stream: APPLICATION_STREAM,
            domain: transport::fragment_domain(domain.chain_id, domain.genesis, domain.protocol),
            receive: limits.receive,
            send: limits.send,
        };
        Ok(Self {
            link: NativeObjectLink::new(
                endpoint,
                vec![
                    lane,
                    crate::native_pipeline::consensus::native::query_lane(
                        transport::fragment_domain(
                            domain.chain_id,
                            domain.genesis,
                            domain.protocol,
                        ),
                    ),
                ],
                policy,
            )?,
            domain,
            codec,
            owner: Arc::new(()),
            session: rand::random(),
            sequence: 0,
            query_pending: None,
        })
    }

    /// Preserve APFL columns and original transaction signatures through the
    /// wire. The receiver independently authenticates them on its AOEM owner.
    pub fn prepare(&mut self, batch: Arc<ApflTransferBatch>) -> Result<PreparedApplicationInput> {
        self.prepare_input(|scope| Message::ApflTransactions { scope, batch })
    }

    /// Shared canonical ML-DSA-65 input on the same native application lane.
    /// Actual payer/recipient accounts are signed. The receiving native owner
    /// independently checks current asset authority at its trusted parent.
    pub fn prepare_asset_spend(
        &mut self,
        batch: Arc<AssetSpendBatchV2>,
    ) -> Result<PreparedApplicationInput> {
        self.prepare_input(|scope| Message::AssetTransactions { scope, batch })
    }

    fn prepare_input(
        &mut self,
        build: impl FnOnce(TransactionsScope) -> Message,
    ) -> Result<PreparedApplicationInput> {
        let sequence = self
            .sequence
            .checked_add(1)
            .context("application sequence exhausted")?;
        let message = build(TransactionsScope {
            chain_id: self.domain.chain_id,
            genesis: self.domain.genesis,
            protocol: self.domain.protocol,
            epoch: self.domain.validators.epoch(),
            validator_set_hash: self.domain.validators.hash(),
            session: self.session,
            sequence,
        });
        message_evidence::verify(&message, &self.domain)?;
        let body = Arc::new(OutgoingMessage::with_profile(
            transport::fragment_domain(
                self.domain.chain_id,
                self.domain.genesis,
                self.domain.protocol,
            ),
            transport::encode(&message, self.codec)?,
            self.codec.message_bytes,
            FragmentProfile::NativeUdpV2,
        )?);
        self.sequence = sequence;
        Ok(PreparedApplicationInput {
            owner: self.owner.clone(),
            body,
        })
    }

    /// False leaves ownership with the caller; retry the same immutable object.
    pub fn try_send(&mut self, input: &PreparedApplicationInput, now: Instant) -> Result<bool> {
        ensure!(
            Arc::ptr_eq(&input.owner, &self.owner),
            "foreign application input handle"
        );
        self.link.try_send(0, APPLICATION_STREAM, &input.body, now)
    }

    /// Reports exact object delivery/expiry, NEVER transaction admission/finality.
    pub fn poll(&mut self, wall_ms: u64, now: Instant, quantum: usize) -> Result<NativeLinkPoll> {
        self.link.poll(wall_ms, now, quantum)
    }

    /// One outstanding read per client; independent of transaction admission.
    /// The request echoes session, generation and exact selector in the reply.
    pub fn prepare_query(&mut self, query: Query) -> Result<PreparedQuery> {
        ensure!(
            self.query_pending.is_none(),
            "application query already pending"
        );
        let sequence = self
            .sequence
            .checked_add(1)
            .context("application sequence exhausted")?;
        let request = Request {
            session: self.session,
            sequence,
            query,
        };
        let body = Arc::new(OutgoingMessage::with_profile(
            transport::fragment_domain(
                self.domain.chain_id,
                self.domain.genesis,
                self.domain.protocol,
            ),
            query::encode(Frame::Request(request.clone())).map_err(anyhow::Error::msg)?,
            query::MAX_BYTES,
            FragmentProfile::NativeUdpV2,
        )?);
        self.sequence = sequence;
        self.query_pending = Some(request.clone());
        Ok(PreparedQuery {
            owner: self.owner.clone(),
            request,
            body,
        })
    }
    pub fn try_send_query(&mut self, request: &PreparedQuery, now: Instant) -> Result<bool> {
        ensure!(
            Arc::ptr_eq(&request.owner, &self.owner)
                && self.query_pending.as_ref() == Some(&request.request),
            "foreign/stale application query handle"
        );
        self.link.try_send(0, query::STREAM, &request.body, now)
    }
    pub fn poll_query_response(
        &mut self,
        request: &PreparedQuery,
        now: Instant,
    ) -> Result<Option<Response>> {
        ensure!(
            Arc::ptr_eq(&request.owner, &self.owner)
                && self.query_pending.as_ref() == Some(&request.request),
            "foreign/stale application query handle"
        );
        let Some(complete) = self.link.poll_complete(query::STREAM, now, 8)? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        for chunk in complete.chunks {
            ensure!(
                chunk.len() <= query::MAX_BYTES.saturating_sub(bytes.len()),
                "query response size"
            );
            bytes.extend_from_slice(&chunk);
        }
        let Frame::Response(response) = query::decode(&bytes).map_err(anyhow::Error::msg)? else {
            anyhow::bail!("client expected native query response");
        };
        if response.request != request.request {
            return Ok(None);
        } // Late old generation cannot satisfy a new read.
        self.query_pending = None;
        Ok(Some(response))
    }
    /// Abandon only the read correlation. Existing bounded native delivery owns
    /// admitted bytes until DONE/expiry; this never cancels an accepted transfer.
    pub fn abandon_query(&mut self, request: &PreparedQuery) -> Result<()> {
        ensure!(
            Arc::ptr_eq(&request.owner, &self.owner)
                && self.query_pending.as_ref() == Some(&request.request),
            "foreign/stale application query handle"
        );
        self.query_pending = None;
        Ok(())
    }
}
