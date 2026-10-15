//! Private validation for the historical network-only gate, not a product ACK API.
//! Reuses the original wire; no sender, scheduler or production protocol is added.
//! Source checks are NOT cryptographic authentication or execution/finality evidence.
use novovm_network::novorudp::{
    NovoRudpRange, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkOnlyAckV0 {
    pub expected_total: u64,
    pub received_unique_count: u64,
    pub missing_ranges: Vec<NovoRudpRange>,
    pub receiver_done: bool,
    pub ack_epoch: u64,
}

/// One sender transfer, not a global ACK owner. Invalid input never advances
/// the epoch. Each lane must pass its actual recv_from source, not its target.
pub struct AckScopeV0 {
    peer: SocketAddr,
    session: [u8; 16],
    expected: u64,
    latest_epoch: Option<u64>,
}

impl AckScopeV0 {
    pub fn new(peer: SocketAddr, session: [u8; 16], expected: u64) -> Self {
        Self {
            peer,
            session,
            expected,
            latest_epoch: None,
        }
    }

    pub fn decode(&mut self, source: SocketAddr, wire: &[u8]) -> Option<NetworkOnlyAckV0> {
        // Bound parsing/allocation by the largest UDP payload, before decoding.
        if source != self.peer || wire.len() > 65_507 {
            return None;
        }
        let frame = NovoRudpTransportFrameV0::decode(wire).ok()?;
        if frame.kind != NovoRudpTransportFrameKindV0::Ack
            || frame.session_id != self.session
            || frame.stream_id != 1
            || frame.object_id != 0
            || frame.sequence != 0
        {
            return None;
        }
        let ack: NetworkOnlyAckV0 = serde_json::from_slice(&frame.payload).ok()?;
        if ack.expected_total != self.expected
            || ack.ack_epoch != frame.ack_epoch
            || self
                .latest_epoch
                .is_some_and(|epoch| ack.ack_epoch <= epoch)
            || ack.received_unique_count > self.expected
        {
            return None;
        }
        let mut previous = None;
        let mut missing = 0u64;
        for range in &ack.missing_ranges {
            if range.start > range.end_inclusive
                || range.end_inclusive >= self.expected
                || previous.is_some_and(|end| range.start <= end)
            {
                return None;
            }
            missing = missing.checked_add(range.count())?;
            previous = Some(range.end_inclusive);
        }
        if missing.checked_add(ack.received_unique_count)? != self.expected
            || (ack.receiver_done && missing != 0)
        {
            return None;
        }
        self.latest_epoch = Some(ack.ack_epoch);
        Some(ack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ack(epoch: u64) -> NetworkOnlyAckV0 {
        NetworkOnlyAckV0 {
            expected_total: 3,
            received_unique_count: 2,
            missing_ranges: vec![NovoRudpRange::new(1, 1)],
            receiver_done: false,
            ack_epoch: epoch,
        }
    }
    fn wire(a: &NetworkOnlyAckV0) -> Vec<u8> {
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Ack,
            [7; 16],
            1,
            0,
            0,
            a.ack_epoch,
            serde_json::to_vec(a).unwrap(),
        )
        .encode()
    }
    #[test]
    fn rejects_scope_and_replay_without_poisoning_valid_epoch() {
        let peer = "127.0.0.1:31001".parse().unwrap();
        let mut scope = AckScopeV0::new(peer, [7; 16], 3);
        assert!(scope
            .decode("127.0.0.1:31002".parse().unwrap(), &wire(&ack(2)))
            .is_none());
        let mut frame = NovoRudpTransportFrameV0::decode(&wire(&ack(2))).unwrap();
        frame.session_id = [8; 16];
        assert!(scope.decode(peer, &frame.encode()).is_none());
        frame.session_id = [7; 16];
        frame.stream_id = 2;
        assert!(scope.decode(peer, &frame.encode()).is_none());
        assert!(scope.decode(peer, &wire(&ack(1))).is_some());
        assert!(scope.decode(peer, &wire(&ack(1))).is_none());
        assert!(scope.decode(peer, &wire(&ack(0))).is_none());
        assert!(scope.decode(peer, &wire(&ack(2))).is_some());
    }
    #[test]
    fn rejects_false_completion_and_inconsistent_ranges() {
        let peer = "127.0.0.1:31001".parse().unwrap();
        let mut scope = AckScopeV0::new(peer, [7; 16], 3);
        let mut bad = ack(100);
        bad.receiver_done = true;
        assert!(scope.decode(peer, &wire(&bad)).is_none());
        bad = ack(100);
        bad.expected_total = 4;
        assert!(scope.decode(peer, &wire(&bad)).is_none());
        bad = ack(100);
        bad.received_unique_count = 3;
        assert!(scope.decode(peer, &wire(&bad)).is_none());
        bad = ack(100);
        bad.missing_ranges.push(NovoRudpRange::new(1, 1));
        assert!(scope.decode(peer, &wire(&bad)).is_none());
        bad = ack(100);
        bad.missing_ranges = vec![NovoRudpRange::new(3, 3)];
        assert!(scope.decode(peer, &wire(&bad)).is_none());
        let mut frame = NovoRudpTransportFrameV0::decode(&wire(&ack(100))).unwrap();
        frame.ack_epoch = 101;
        assert!(scope.decode(peer, &frame.encode()).is_none());
        let done = NetworkOnlyAckV0 {
            expected_total: 3,
            received_unique_count: 3,
            missing_ranges: vec![],
            receiver_done: true,
            ack_epoch: 1,
        };
        assert!(scope.decode(peer, &wire(&done)).unwrap().receiver_done);
    }
    #[test]
    fn actual_udp_source_is_checked_before_ack_consumption() {
        use std::{net::UdpSocket, time::Duration};
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut scope = AckScopeV0::new(receiver.local_addr().unwrap(), [7; 16], 3);
        let mut buf = [0u8; 4096];
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        // Deliberately omit 1 and deliver 2 before 0 with the existing codec.
        let mut delivered = std::collections::BTreeSet::new();
        for sequence in [2, 0] {
            let frame = NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                [7; 16],
                1,
                sequence,
                sequence,
                0,
                vec![sequence as u8],
            );
            sender
                .send_to(&frame.encode(), receiver.local_addr().unwrap())
                .unwrap();
            let (n, _) = receiver.recv_from(&mut buf).unwrap();
            delivered.insert(
                NovoRudpTransportFrameV0::decode(&buf[..n])
                    .unwrap()
                    .sequence,
            );
        }
        assert_eq!(delivered, [0, 2].into_iter().collect());
        stranger
            .send_to(&wire(&ack(100)), sender.local_addr().unwrap())
            .unwrap();
        let (n, source) = sender.recv_from(&mut buf).unwrap();
        assert!(scope.decode(source, &buf[..n]).is_none());
        receiver
            .send_to(&wire(&ack(1)), sender.local_addr().unwrap())
            .unwrap();
        let (n, source) = sender.recv_from(&mut buf).unwrap();
        let accepted = scope.decode(source, &buf[..n]).unwrap();
        let plan = novovm_network::novorudp::build_repair_plan(
            &accepted.missing_ranges,
            accepted.expected_total,
            &novovm_network::novorudp::NovoRudpWindowConfig::default(),
        )
        .unwrap();
        assert_eq!(plan.window.missing_count, 1);
        for range in plan.window.missing_ranges {
            for sequence in range.start..=range.end_inclusive {
                let frame = NovoRudpTransportFrameV0::new(
                    NovoRudpTransportFrameKindV0::Repair,
                    [7; 16],
                    1,
                    sequence,
                    sequence,
                    accepted.ack_epoch,
                    vec![sequence as u8],
                );
                sender
                    .send_to(&frame.encode(), receiver.local_addr().unwrap())
                    .unwrap();
                let (n, _) = receiver.recv_from(&mut buf).unwrap();
                delivered.insert(
                    NovoRudpTransportFrameV0::decode(&buf[..n])
                        .unwrap()
                        .sequence,
                );
            }
        }
        assert_eq!(delivered, [0, 1, 2].into_iter().collect());
        let done = NetworkOnlyAckV0 {
            expected_total: 3,
            received_unique_count: delivered.len() as u64,
            missing_ranges: vec![],
            receiver_done: true,
            ack_epoch: 2,
        };
        receiver
            .send_to(&wire(&done), sender.local_addr().unwrap())
            .unwrap();
        let (n, source) = sender.recv_from(&mut buf).unwrap();
        assert!(scope.decode(source, &buf[..n]).unwrap().receiver_done);
    }
}
