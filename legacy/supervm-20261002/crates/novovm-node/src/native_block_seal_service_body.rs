//! Send only the locally signed proposal's exact verified body. No signing here.
use super::*;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_candidate_body::network::CandidateBodySenderV1;

pub(super) struct BodyDeliveryV1 {
    proposal_hash: [u8; 32],
    senders: Vec<CandidateBodySenderV1>,
}

impl BodyDeliveryV1 {
    pub(super) fn build(
        config: &NovNativeSealServiceConfigV1,
        message: &Message,
        raws: &[Vec<u8>],
    ) -> Result<Option<Self>> {
        let proposal = message
            .proposal()
            .context("body delivery requires proposal")?;
        if !config.receive_successors
            || !config.is_fresh_successor()
            || proposal.proposer_id != config.local_validator_id
        {
            return Ok(None);
        }
        let source = config
            .authority
            .transport_peer_id(config.local_validator_id)?;
        let wire = super::super::round_wire::encode_nov_native_seal_round_wire_v1(
            message,
            &config.authority,
            config.height,
            source,
        )?;
        let senders = config
            .authority
            .transport_bindings
            .iter()
            .filter(|b| b.validator_id != config.local_validator_id)
            .map(|b| {
                CandidateBodySenderV1::new(
                    wire.clone(),
                    raws,
                    &config.authority,
                    config.height,
                    source,
                    &b.transport_peer_id,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(Self {
            proposal_hash: proposal.proposal_hash,
            senders,
        }))
    }

    pub(super) fn poll(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        for sender in &mut self.senders {
            sender.poll_at(runtime, now)?;
        }
        Ok(())
    }

    pub(super) fn len(&self) -> usize {
        self.senders.len()
    }
}

impl NovNativeSealServiceV1 {
    pub(super) fn poll_body_delivery(
        &mut self,
        view: &NovNativeBlockLedgerV1,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        if !self.config.receive_successors || !self.config.is_fresh_successor() {
            return Ok(());
        }
        let Some(message) = self.bridge.local_body_proposal() else {
            self.body_delivery = None;
            return Ok(());
        };
        let hash = message
            .proposal()
            .context("local body proposal missing")?
            .proposal_hash;
        if self
            .body_delivery
            .as_ref()
            .is_none_or(|body| body.proposal_hash != hash)
        {
            let (_, block) = view.load_seal_eligible_local_candidate_v1(
                self.config.chain_id,
                self.config.block_hash,
            )?;
            self.body_delivery =
                BodyDeliveryV1::build(&self.config, &message, &block.body.raw_txs)?;
        }
        if let Some(body) = &mut self.body_delivery {
            body.poll(runtime, now)?;
        }
        Ok(())
    }
}
