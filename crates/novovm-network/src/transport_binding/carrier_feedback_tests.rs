// Scheduling regression fixtures with real signed/Finished channel state and
// bounded in-memory carrier IO. Not a QUIC throughput or mobile acceptance test.
use super::{binding, finished_pair, NOW};
use crate::novorudp::{
    carrier::{NativeDatagramIo, NativeRoute, VerifiedCarrierRoute},
    datagram::NativeUdpSession,
    endpoint::NativeEndpoint,
    fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits, UDP_CHUNK_BYTES},
    object_ack::decode_object_ack,
    object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
    object_sender::ObjectSendState,
    NovoRudpTransportFrameKindV0 as Kind, NovoRudpTransportFrameV0 as Frame,
};
use anyhow::Result;
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const STREAM: u64 = 11;
const DOMAIN: [u8; 32] = [41; 32];
const MAX: usize = 2 * 1024 * 1024;
type Queue = Arc<Mutex<VecDeque<Vec<u8>>>>;
struct MemoryCarrier {
    incoming: Queue,
    outgoing: Queue,
    route: NativeRoute,
}
impl NativeDatagramIo for MemoryCarrier {
    fn try_receive(&self, dst: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        let Some(bytes) = self.incoming.lock().unwrap().pop_front() else {
            return Err(io::ErrorKind::WouldBlock.into());
        };
        assert!(bytes.len() <= dst.len());
        dst[..bytes.len()].copy_from_slice(&bytes);
        Ok(Some((bytes.len(), self.route)))
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        assert_eq!(route, self.route);
        let mut queue = self.outgoing.lock().unwrap();
        if queue.len() >= 128 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        queue.push_back(bytes.to_vec());
        Ok(bytes.len())
    }
}
fn io_pair() -> (Arc<MemoryCarrier>, Arc<MemoryCarrier>) {
    let a = Queue::default();
    let b = Queue::default();
    (
        Arc::new(MemoryCarrier {
            incoming: a.clone(),
            outgoing: b.clone(),
            route: NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding(true, 7))),
        }),
        Arc::new(MemoryCarrier {
            incoming: b,
            outgoing: a,
            route: NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding(false, 7))),
        }),
    )
}
fn link(endpoint: NativeEndpoint) -> Result<NativeObjectLink> {
    NativeObjectLink::new(
        endpoint,
        vec![NativeLaneConfig {
            stream: STREAM,
            domain: DOMAIN,
            receive: ReassemblyLimits {
                max_message_bytes: MAX,
                messages: 2,
                bytes: MAX * 2,
                peer_messages: 2,
                peer_bytes: MAX * 2,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: MAX * 2,
                peer_messages: 2,
                peer_bytes: MAX * 2,
            },
        }],
        NativeSendPolicy::reliable_carrier_v1(Duration::from_secs(5)),
    )
}
fn body(frames: usize, value: u8) -> Result<Arc<OutgoingMessage>> {
    Ok(Arc::new(OutgoingMessage::with_profile(
        DOMAIN,
        vec![value; frames * UDP_CHUNK_BYTES - 7],
        MAX,
        FragmentProfile::NativeUdpV2,
    )?))
}

#[test]
fn reliable_carrier_feedback_window_boundaries_and_bidirectional_completion() -> Result<()> {
    for frames in [1, 63, 64, 65, 2084] {
        let (a_channel, b_channel) = finished_pair();
        let (a_io, b_io) = io_pair();
        let mut a = link(NativeEndpoint::from_bound_channel(
            a_io,
            a_channel,
            &binding(true, 7),
            vec![STREAM],
        )?)?;
        let mut b = link(NativeEndpoint::from_bound_channel(
            b_io,
            b_channel,
            &binding(false, 7),
            vec![STREAM],
        )?)?;
        let now = Instant::now();
        let large = body(frames, 3)?;
        let small = body(1, 9)?;
        assert!(a.try_send(0, STREAM, &large, now)?);
        assert!(b.try_send(0, STREAM, &small, now)?);
        let mut done = [false; 2];
        let mut received = [false; 2];
        let mut feedback = [0; 2];
        let mut duplicates = [0; 2];
        for _ in 0..5000 {
            for (side, target) in [&mut a, &mut b].into_iter().enumerate() {
                // One-frame polls expose accidental per-frame feedback even
                // when a larger poll quantum would incidentally coalesce it.
                let report = target.poll(NOW, now, 1)?;
                assert_eq!(
                    report.rejected + report.receive_errors + report.send_failures,
                    0
                );
                feedback[side] += report.feedback_sent;
                duplicates[side] += report.duplicate_fragments;
                for completed in report.completed {
                    assert_eq!(completed.state, ObjectSendState::Delivered);
                    assert_eq!(
                        completed.body.id(),
                        if side == 0 { large.id() } else { small.id() }
                    );
                    done[side] = true;
                }
                if let Some(item) = target.poll_complete(STREAM, now, 64)? {
                    let expected = if side == 0 { &small } else { &large };
                    assert_eq!(item.id, expected.id());
                    assert_eq!(
                        item.chunks.iter().map(Vec::len).sum::<usize>(),
                        expected.byte_len()
                    );
                    assert!(item
                        .chunks
                        .iter()
                        .flatten()
                        .all(|b| *b == if side == 0 { 9 } else { 3 }));
                    assert!(!received[side], "duplicate completion");
                    received[side] = true;
                }
            }
            if done.iter().all(|v| *v) && received.iter().all(|v| *v) {
                break;
            }
        }
        assert_eq!(done, [true; 2], "sender completion for {frames} fragments");
        assert_eq!(received, [true; 2]);
        assert_eq!(
            feedback[0], 1,
            "small reverse object needs only integrity DONE"
        );
        // on_ack may retain a missing-window Repair cursor while initial Data
        // is still advancing. Its first post-initial frame can already be at
        // the receiver: that duplicate intentionally receives immediate ACK.
        // Count those observed frames instead of attributing their ACK to Data
        // scheduling. Keep a tight lossless-fixture guard against repair storms.
        assert!(
            duplicates.iter().all(|count| *count <= 2),
            "unexpected lossless repair traffic: {duplicates:?}"
        );
        assert!(
            feedback[1] <= frames / 64 + 1 + duplicates[1],
            "per-fragment ACK regression: feedback={feedback:?} duplicates={duplicates:?}"
        );
    }
    Ok(())
}

struct ManualPeer {
    sender: NativeUdpSession,
    receiver: NativeObjectLink,
    tx: Arc<MemoryCarrier>,
    now: Instant,
}
impl ManualPeer {
    fn new() -> Result<Self> {
        let (a, b) = finished_pair();
        let (tx, rx) = io_pair();
        Ok(Self {
            sender: NativeUdpSession::from_bound_channel(a, &binding(true, 7))?,
            receiver: link(NativeEndpoint::from_bound_channel(
                rx,
                b,
                &binding(false, 7),
                vec![STREAM],
            )?)?,
            tx,
            now: Instant::now(),
        })
    }
    fn send(
        &mut self,
        body: &OutgoingMessage,
        index: usize,
        kind: Kind,
        corrupt: bool,
    ) -> Result<usize> {
        let mut payload = body.frame(index)?;
        if corrupt {
            payload[80] ^= 1;
        }
        let frame = Frame::new(
            kind,
            self.sender.session_id(),
            STREAM,
            u64::from_be_bytes(body.id()[..8].try_into()?),
            index as u64,
            0,
            payload,
        );
        let bytes = self.sender.encode(&frame)?;
        self.tx.try_send(&bytes, self.tx.route)?;
        let report = self.receiver.poll(NOW, self.now, 1)?;
        assert_eq!(report.rejected, 0);
        Ok(report.feedback_sent)
    }
    fn ack(&mut self, body: &OutgoingMessage) -> Result<crate::novorudp::NovoRudpAckFrame> {
        let bytes = self
            .tx
            .incoming
            .lock()
            .unwrap()
            .pop_front()
            .expect("ACK expected");
        let frame = self.sender.open_route(self.tx.route, &bytes)?;
        decode_object_ack(
            &frame,
            self.sender.session_id(),
            STREAM,
            body.id(),
            body.frame_count() as u64,
        )
    }
}

#[test]
fn reliable_carrier_duplicate_and_repair_feedback_do_not_wait_for_window() -> Result<()> {
    let mut pair = ManualPeer::new()?;
    let body = body(3, 5)?;
    assert_eq!(pair.send(&body, 0, Kind::Data, false)?, 0);
    assert_eq!(pair.send(&body, 0, Kind::Data, false)?, 1);
    let duplicate = pair.ack(&body)?;
    assert!(!duplicate.receiver_done);
    assert_eq!(duplicate.missing_count, 2);
    // A missing earlier index can be repaired after a later index arrived.
    assert_eq!(pair.send(&body, 2, Kind::Data, false)?, 0);
    assert_eq!(pair.send(&body, 1, Kind::Repair, false)?, 1);
    let received_only = pair.ack(&body)?;
    assert_eq!(received_only.missing_count, 0);
    assert!(
        !received_only.receiver_done,
        "receipt before integrity verification"
    );
    assert!(pair.receiver.poll_complete(STREAM, pair.now, 64)?.is_some());
    assert_eq!(pair.receiver.poll(NOW, pair.now, 1)?.feedback_sent, 1);
    assert!(pair.ack(&body)?.receiver_done);
    // A fresh encrypted repair of an already-completed object re-emits DONE,
    // without handing the application the object a second time.
    assert_eq!(pair.send(&body, 2, Kind::Repair, false)?, 1);
    assert!(pair.ack(&body)?.receiver_done);
    assert!(pair.receiver.poll_complete(STREAM, pair.now, 64)?.is_none());
    Ok(())
}

#[test]
fn reliable_carrier_corrupt_complete_object_never_emits_done() -> Result<()> {
    let mut pair = ManualPeer::new()?;
    let body = body(1, 7)?;
    assert_eq!(pair.send(&body, 0, Kind::Data, true)?, 0);
    let error = pair
        .receiver
        .poll_complete(STREAM, pair.now, 64)
        .err()
        .expect("object hash must reject");
    assert!(error.to_string().contains("hash mismatch"));
    assert_eq!(pair.receiver.poll(NOW, pair.now, 1)?.feedback_sent, 0);
    assert!(pair.tx.incoming.lock().unwrap().is_empty());
    Ok(())
}
