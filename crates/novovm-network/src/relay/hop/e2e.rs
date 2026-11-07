//! Original ML-DSA native handshake and AEAD, through opaque hop frames.
//! No relay is given the end-to-end channel or its business plaintext.
use super::*;
use crate::product_overlay::{
    handshake_wire::{self, HandshakeMessage},
    peer_id_from_mldsa65_public_key_v1, E2eSecureChannelV1, NodeHandshakeInitiatorV1,
    NodeHandshakeResponderV1,
};

/// Opaque completion evidence. Only an actual hop stream, independently pinned
/// original native key and route-bound AEAD confirmation can construct it.
pub struct VerifiedRelaySession {
    pub(crate) channel: E2eSecureChannelV1,
    pub(crate) identity: TransportIdentity,
    pub(crate) remote: Vec<u8>,
    pub(crate) route: VerifiedCarrierRoute,
}
impl HopStream<'_> {
    pub async fn authenticate_native(
        &mut self,
        identity: &TransportIdentity,
        expected: &[u8],
        initiator: bool,
    ) -> Result<VerifiedRelaySession> {
        ensure!(self.ready, "relay circuit not ready");
        let (local, remote) = if initiator {
            (self.route.origin.as_slice(), self.route.target())
        } else {
            (self.route.target(), self.route.origin.as_slice())
        };
        ensure!(
            identity.public_key() == local && expected == remote,
            RelayProtocolRejected("end-to-end roles/pins differ")
        );
        let mut replay = crate::product_overlay::HandshakeReplayCacheV1::new(8);
        let mut channel = if initiator {
            let pending = NodeHandshakeInitiatorV1::start(
                identity,
                peer_id_from_mldsa65_public_key_v1(expected),
                wall()?,
                30_000,
            )?;
            self.write(&handshake_wire::encode_offer(pending.offer())?)
                .await?;
            let HandshakeMessage::Response(response) = handshake_wire::decode(&self.read().await?)
                .context(RelayProtocolRejected("native response encoding"))?
            else {
                anyhow::bail!(RelayProtocolRejected("expected native response"))
            };
            ensure!(
                response.responder_identity_public_key == expected,
                RelayProtocolRejected("final native key differs")
            );
            pending
                .complete(&response, wall()?, &mut replay)
                .context(RelayProtocolRejected("native response authentication"))?
        } else {
            let HandshakeMessage::Offer(offer) = handshake_wire::decode(&self.read().await?)
                .context(RelayProtocolRejected("native offer encoding"))?
            else {
                anyhow::bail!(RelayProtocolRejected("expected native offer"))
            };
            ensure!(
                offer.initiator_identity_public_key == expected,
                RelayProtocolRejected("origin native key differs")
            );
            let responder =
                NodeHandshakeResponderV1::respond(&offer, identity, wall()?, 30_000, &mut replay)
                    .context(RelayProtocolRejected("native offer authentication"))?;
            self.write(&handshake_wire::encode_response(responder.response())?)
                .await?;
            responder.into_channel()
        };
        let mut payload = b"NVHFIN01".to_vec();
        payload.extend_from_slice(&self.route.digest());
        let finished = Frame::new(
            Kind::Data,
            channel.session_id(),
            0,
            u64::from_be_bytes(*b"NVHFIN01"),
            0,
            0,
            payload,
        );
        if initiator {
            self.write(&crate::product_overlay::binary::encode(
                &channel.seal_novorudp_frame(&finished)?,
                MAX_INNER,
            )?)
            .await?;
        }
        let wire = self.read().await?;
        let envelope = crate::product_overlay::binary::decode(&wire, MAX_INNER)
            .context(RelayProtocolRejected("native confirmation encoding"))?;
        let decoded = channel
            .open_novorudp_frame(&envelope)
            .context(RelayProtocolRejected("native confirmation authentication"))?;
        ensure!(
            decoded == finished,
            RelayProtocolRejected("end-to-end confirmation/route differs")
        );
        if !initiator {
            self.write(&crate::product_overlay::binary::encode(
                &channel.seal_novorudp_frame(&finished)?,
                MAX_INNER,
            )?)
            .await?;
        }
        Ok(VerifiedRelaySession {
            channel,
            identity: identity.clone(),
            remote: expected.to_vec(),
            route: self.first_hop_route()?,
        })
    }
}
