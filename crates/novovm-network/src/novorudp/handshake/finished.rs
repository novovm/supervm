//! Native Finished uses the original channel, frame shape and immutable attempt
//! deadline. No TLS exporter is invented for UDP; this is a distinct proof.
use super::*;
use crate::product_overlay::{binary, session_finish::finished_frame};

pub(super) const MAGIC: &[u8; 8] = b"NVHSHK03";
const DOMAIN: &[u8] = b"novovm.native-session.finished.mldsa65.v3";

pub(super) struct Pending {
    channel: E2eSecureChannelV1,
    transcript: [u8; 32],
    initiator: bool,
    local: Option<Vec<u8>>,
    remote: Option<Vec<u8>>,
    sent: bool,
}

impl Pending {
    pub(super) fn new(channel: E2eSecureChannelV1, transcript: [u8; 32], initiator: bool) -> Self {
        Self {
            channel,
            transcript,
            initiator,
            local: None,
            remote: None,
            sent: false,
        }
    }
    pub(super) fn wire(&mut self) -> Result<Vec<u8>> {
        if let Some(wire) = &self.local {
            return Ok(wire.clone());
        }
        let frame = finished_frame(
            DOMAIN,
            self.channel.session_id(),
            self.transcript,
            self.initiator,
        );
        let envelope = self.channel.seal_novorudp_frame(&frame)?;
        ensure!(
            envelope.sequence == 0,
            "native Finished must be first frame"
        );
        let mut wire = MAGIC.to_vec();
        wire.extend(binary::encode(&envelope, MAX_DATAGRAM_BYTES - MAGIC.len())?);
        self.local = Some(wire.clone());
        Ok(wire)
    }
    pub(super) fn receive(&mut self, wire: &[u8]) -> Result<()> {
        if let Some(previous) = &self.remote {
            ensure!(previous == wire, "different native Finished retry");
            return Ok(());
        }
        ensure!(
            wire.starts_with(MAGIC) && wire.len() <= MAX_DATAGRAM_BYTES,
            "native Finished layout"
        );
        let envelope = binary::decode(&wire[MAGIC.len()..], MAX_DATAGRAM_BYTES - MAGIC.len())?;
        ensure!(envelope.sequence == 0, "native Finished sequence");
        let frame = self.channel.open_novorudp_frame(&envelope)?;
        ensure!(
            frame
                == finished_frame(
                    DOMAIN,
                    self.channel.session_id(),
                    self.transcript,
                    !self.initiator
                ),
            "native Finished transcript/role"
        );
        self.remote = Some(wire.to_vec());
        Ok(())
    }
    pub(super) fn sent(&mut self) {
        self.sent = true;
    }
    pub(super) fn complete(&self) -> bool {
        self.sent && self.remote.is_some()
    }
    pub(super) fn into_parts(self) -> (E2eSecureChannelV1, Vec<u8>) {
        (self.channel, self.remote.expect("verified Finished"))
    }
}
