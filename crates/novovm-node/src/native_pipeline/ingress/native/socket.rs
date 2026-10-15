//! APFL adapter over the SAME native bounded object socket used by consensus.
use super::*;
use novovm_network::novorudp::{
    endpoint::NativeEndpoint,
    object_link::{
        NativeLaneConfig, NativeLinkPoll, NativeObjectLink, NativeSendLimits, NativeSendPolicy,
    },
};
pub type NativeIngressPoll = NativeLinkPoll;
pub struct NativeApflSocket {
    link: NativeObjectLink,
    wire_limit: usize,
    expanded: ApflLimits,
    policy: DirectNovFeePolicy,
}
impl NativeApflSocket {
    pub fn new(
        endpoint: NativeEndpoint,
        domain: [u8; 32],
        peers: Vec<String>,
        limits: ReassemblyLimits,
        expanded: ApflLimits,
        policy: DirectNovFeePolicy,
    ) -> Result<Self> {
        policy.validate()?;
        ensure!(
            peers.len() == endpoint.peer_count(),
            "native ingress peer count mismatch"
        );
        for (index, peer) in peers.iter().enumerate() {
            ensure!(
                endpoint.peer_id(index)? == peer,
                "native ingress peer ordering mismatch"
            );
        }
        let wire_limit = limits.max_message_bytes;
        let send = NativeSendLimits {
            messages: limits.messages,
            bytes: limits.bytes,
            peer_messages: limits.peer_messages,
            peer_bytes: limits.peer_bytes,
        };
        // Receive-only adapter: uses no outgoing object reservations.
        let send_policy = NativeSendPolicy {
            window: Default::default(),
            pacing: Default::default(),
            ttl: limits.ttl,
        };
        let link = NativeObjectLink::new(
            endpoint,
            vec![NativeLaneConfig {
                stream: APFL_STREAM_V1,
                domain,
                receive: limits,
                send,
            }],
            send_policy,
        )?;
        Ok(Self {
            link,
            wire_limit,
            expanded,
            policy,
        })
    }
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.link.local_addr()
    }
    pub fn session_id(&self, peer: usize) -> Option<[u8; 16]> {
        self.link.session_id(peer)
    }
    pub fn pending_messages(&self) -> usize {
        self.link
            .pending_messages(APFL_STREAM_V1)
            .expect("configured APFL lane")
    }
    pub fn reserved_bytes(&self) -> usize {
        self.link
            .reserved_bytes(APFL_STREAM_V1)
            .expect("configured APFL lane")
    }
    pub fn poll(
        &mut self,
        wall_ms: u64,
        now: Instant,
        quantum: usize,
    ) -> Result<NativeIngressPoll> {
        self.link.poll(wall_ms, now, quantum)
    }
    /// Caller reserves an assembly slot, retaining work on AOEM backpressure.
    pub fn poll_request(
        &mut self,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<NativeApflInput>> {
        self.link
            .poll_complete(APFL_STREAM_V1, now, quantum)?
            .map(|complete| {
                decode_complete(
                    complete,
                    self.wire_limit,
                    self.expanded,
                    self.policy.clone(),
                )
            })
            .transpose()
    }
}
#[cfg(test)]
pub(crate) mod tests;
