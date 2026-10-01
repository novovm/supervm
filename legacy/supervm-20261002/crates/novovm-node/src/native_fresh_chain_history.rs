use super::*;
use crate::native_candidate_body::network::CandidateBodySenderV1;
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"NOVHIST1";
const RETRY: Duration = Duration::from_secs(5);

struct HistoryResponse {
    height: u64,
    body: CandidateBodySenderV1,
    prepare: Vec<u8>,
    prepare_at: Instant,
}

pub(super) struct HistorySync {
    requests: BTreeMap<String, (u64, Instant)>,
    responses: BTreeMap<String, HistoryResponse>,
    next_request: Instant,
    namespace: [u8; 32],
    served: u64,
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

impl HistorySync {
    pub(super) fn new(
        config: &NovNativeSealServiceConfigV1,
        params: &serde_json::Value,
        now: Instant,
    ) -> Result<Self> {
        let namespace =
            crate::tx_ingress::native_aoem_owned_state_namespace_digest_v1(params, config.chain_id);
        let namespace = crate::native_block_seal::service_config::decode_hex_32(
            namespace.as_bytes(),
            "history namespace",
        )?;
        Ok(Self {
            requests: BTreeMap::new(),
            responses: BTreeMap::new(),
            next_request: now,
            namespace,
            served: 0,
        })
    }

    pub(super) fn is_request(inbound: &ProductMainlineOverlayInboundV1) -> bool {
        inbound.payload_class == ProductMainlineOverlayPayloadClassV1::NativeSeal
            && inbound.frame.payload.starts_with(MAGIC)
    }

    pub(super) fn enqueue(
        &mut self,
        event: &ProductMainlineOverlayInboundV1,
        config: &NovNativeSealServiceConfigV1,
        now: Instant,
    ) -> bool {
        let bytes = &event.frame.payload;
        if bytes.len() != 80
            || event.frame.stream_id != config.chain_id
            || bytes[8..40] != config.authority.authority_commitment
            || Some(&bytes[40..72])
                != config
                    .fresh_genesis_config_commitment
                    .as_ref()
                    .map(|pin| &pin[..])
            || event.object_hash != hash(bytes)
            || event.payload_sha256 != hash(bytes)
            || config
                .authority
                .validator_for_transport_peer(&event.source_peer_id)
                .is_none()
        {
            return false;
        }
        let height = u64::from_be_bytes(bytes[72..80].try_into().expect("bounded history height"));
        if height < 2
            || self
                .requests
                .get(&event.source_peer_id)
                .is_some_and(|(_, previous)| now.duration_since(*previous) < RETRY)
        {
            return false;
        }
        self.requests
            .insert(event.source_peer_id.clone(), (height, now));
        true
    }

    pub(super) fn poll(
        &mut self,
        config: &NovNativeSealServiceConfigV1,
        ledger: &Path,
        published: bool,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        let finalized = if published {
            config.height
        } else {
            config.height.saturating_sub(1)
        };
        let pin = config
            .fresh_genesis_config_commitment
            .context("history genesis missing")?;
        for (peer, (height, _)) in &mut self.requests {
            if *height < 2 {
                continue;
            }
            let requested = *height;
            *height = 0;
            if requested > finalized {
                self.responses.remove(peer);
                continue;
            }
            if self
                .responses
                .get(peer)
                .is_some_and(|response| response.height == requested)
            {
                continue;
            }
            let (block, proof) = NovNativeBlockLedgerV1::load_fresh_finalized_block_by_height_v1(
                ledger,
                pin,
                self.namespace,
                requested,
            )?
            .context("finalized history has a gap")?;
            if proof.authority != config.authority {
                bail!("history authority changed");
            }
            let source = &runtime.startup().local_peer_id;
            use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
            let Message::DecisionCertificateV3 {
                proposal,
                decision,
                certificate,
            } = &proof.witness
            else {
                bail!("history requires final decision");
            };
            let prepare =
                crate::native_block_seal::round_wire::encode_nov_native_seal_round_wire_v1(
                    &Message::QuorumCertificate {
                        proposal: proposal.clone(),
                        qc: Box::new(decision.prepare.clone()),
                        certificate: certificate.clone(),
                    },
                    &config.authority,
                    requested,
                    source,
                )?;
            let wire = crate::native_block_seal::round_wire::encode_nov_native_seal_round_wire_v1(
                &proof.witness,
                &config.authority,
                requested,
                source,
            )?;
            let sender = CandidateBodySenderV1::new(
                wire,
                &block.body.raw_txs,
                &config.authority,
                requested,
                source,
                peer,
            )?;
            self.responses.insert(
                peer.clone(),
                HistoryResponse {
                    height: requested,
                    body: sender,
                    prepare,
                    prepare_at: now,
                },
            );
            self.served = self.served.saturating_add(1);
        }
        for (peer, response) in &mut self.responses {
            if now >= response.prepare_at {
                runtime.try_submit_to_peer(
                    peer,
                    ProductMainlineOverlayPayloadClassV1::NativeSeal,
                    crate::native_block_seal::round_wire::round_wire_object_hash_v1(
                        &response.prepare,
                    ),
                    response.prepare.clone(),
                )?;
                response.prepare_at = now + Duration::from_secs(1);
            }
            response.body.poll_at(runtime, now)?;
        }
        if now >= self.next_request {
            self.next_request = now + RETRY;
            let requested = finalized
                .checked_add(1)
                .context("history height overflow")?;
            if requested < 2 {
                return Ok(());
            }
            let mut bytes = MAGIC.to_vec();
            bytes.extend_from_slice(&config.authority.authority_commitment);
            bytes.extend_from_slice(&pin);
            bytes.extend_from_slice(&requested.to_be_bytes());
            for binding in &config.authority.transport_bindings {
                if binding.validator_id != config.local_validator_id {
                    runtime.try_submit_to_peer(
                        &binding.transport_peer_id,
                        ProductMainlineOverlayPayloadClassV1::NativeSeal,
                        hash(&bytes),
                        bytes.clone(),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn served(&self) -> u64 {
        self.served
    }
}
