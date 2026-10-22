//! Original node's verified-fragment/feedback boundary, shared by APFL and
//! consensus lanes. No business decoder, signature verifier or ledger here.
use super::{
    datagram::NativeUdpSession,
    endpoint::NativeReceivedFrame,
    fragments::{
        CompletedMessage, FragmentAdmission, FragmentProfile, Reassembler, ReassemblyLimits,
    },
    NovoRudpAckFrame, NovoRudpFrameHeader, NovoRudpFrameKind, NovoRudpTransportFrameKindV0,
    NovoRudpTransportFrameV0, NovoRudpWindowConfig,
};
use anyhow::{ensure, Result};
use std::{net::SocketAddr, time::Instant};

pub struct NativeObjectReceiver {
    stream: u64,
    objects: Reassembler,
}
impl NativeObjectReceiver {
    pub(crate) fn has_pending_peer(&self, peer: &str) -> bool {
        self.objects.has_pending_peer(peer)
    }
    pub(crate) fn retire_authenticated_peer(&mut self, peer: &str) {
        self.objects.retire_authenticated_peer(peer);
    }
    pub(crate) fn retire_peer_session(&mut self, peer: &str) -> usize {
        self.objects.retire_peer_session(peer)
    }
    pub(crate) fn awaiting_authenticated_peers(
        stream: u64,
        domain: [u8; 32],
        limits: ReassemblyLimits,
    ) -> Result<Self> {
        ensure!(stream != 0, "zero native stream");
        Ok(Self {
            stream,
            objects: Reassembler::awaiting_authenticated_peers(
                domain,
                limits,
                FragmentProfile::NativeUdpV2,
            )?
            .with_completion_history(),
        })
    }
    pub(crate) fn admit_authenticated_peer(&mut self, peer: String) -> Result<()> {
        self.objects.admit_authenticated_peer(peer)
    }
    pub fn new(
        stream: u64,
        domain: [u8; 32],
        peers: Vec<String>,
        limits: ReassemblyLimits,
    ) -> Result<Self> {
        ensure!(stream != 0, "zero native stream");
        Ok(Self {
            stream,
            objects: Reassembler::with_profile(
                domain,
                peers,
                limits,
                FragmentProfile::NativeUdpV2,
            )?
            .with_completion_history(),
        })
    }
    pub fn accept_datagram(
        &mut self,
        session: &mut NativeUdpSession,
        source: SocketAddr,
        wire: &[u8],
        now: Instant,
    ) -> Result<FragmentAdmission> {
        let frame = session.open(source, wire)?;
        self.accept_open_frame(session.remote_peer_id(), &frame, now)
    }
    pub fn accept_received(
        &mut self,
        received: &NativeReceivedFrame,
        now: Instant,
    ) -> Result<FragmentAdmission> {
        self.accept_open_frame(received.peer_id(), received.frame(), now)
    }
    fn accept_open_frame(
        &mut self,
        peer: &str,
        frame: &NovoRudpTransportFrameV0,
        now: Instant,
    ) -> Result<FragmentAdmission> {
        ensure!(
            frame.stream_id == self.stream
                && matches!(
                    frame.kind,
                    NovoRudpTransportFrameKindV0::Data | NovoRudpTransportFrameKindV0::Repair
                ),
            "unexpected object stream or frame kind"
        );
        ensure!(frame.payload.len() > 80, "truncated object fragment");
        ensure!(
            frame.object_id == u64::from_be_bytes(frame.payload[40..48].try_into()?)
                && frame.sequence
                    == u64::from(u32::from_be_bytes(frame.payload[76..80].try_into()?)),
            "object/index binding mismatch"
        );
        self.objects.push(peer, &frame.payload, now)
    }
    /// Caller reserves an output slot before taking custody of the owned chunks.
    pub fn poll_complete(
        &mut self,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<CompletedMessage>> {
        self.objects.poll_complete(now, quantum)
    }
    pub(crate) fn poll_complete_with_progress(
        &mut self,
        now: Instant,
        quantum: usize,
    ) -> Result<(Option<CompletedMessage>, bool)> {
        self.objects.poll_complete_with_progress(now, quantum)
    }
    pub fn pending_messages(&self) -> usize {
        self.objects.pending_messages()
    }
    pub fn reserved_bytes(&self) -> usize {
        self.objects.reserved_bytes()
    }
    pub fn expire(&mut self, now: Instant) -> usize {
        self.objects.expire(now)
    }
    /// DONE means hash-checked bytes handed to the caller, not business validity.
    pub fn transport_feedback(
        &mut self,
        peer: &str,
        id: [u8; 32],
        session: [u8; 16],
        epoch: u64,
        now: Instant,
    ) -> Result<Option<NovoRudpAckFrame>> {
        ensure!(epoch > 0, "ACK epoch must be nonzero");
        self.objects.expire(now);
        let completed = self.objects.completed_total(peer, id, now);
        let snapshot = if let Some(total) = completed {
            Some((total, 0, None))
        } else {
            self.objects
                .window_snapshot(peer, id, &NovoRudpWindowConfig::default())?
        };
        let Some((total, missing, window)) = snapshot else {
            return Ok(None);
        };
        Ok(Some(NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: session,
                epoch,
                sequence: None,
                window_id: window.as_ref().map(|w| w.window_id),
            },
            expected_total: total,
            receiver_done: completed.is_some(),
            missing_count: missing,
            current_window: window.as_ref().map(|w| w.range),
            current_window_missing_ranges: window.map(|w| w.missing_ranges).unwrap_or_default(),
        }))
    }
}
