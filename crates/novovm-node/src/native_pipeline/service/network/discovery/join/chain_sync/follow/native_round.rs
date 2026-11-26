use std::{collections::BTreeMap, net::SocketAddr};

const MAX_FAILED_ADDRESSES_PER_PEER: usize = 2;

#[derive(Default)]
pub(super) struct NativeRound {
    // Allow one signed replacement address without allowing endless locator churn.
    failed: BTreeMap<String, Vec<SocketAddr>>,
}

impl NativeRound {
    pub(super) fn record_failure(&mut self, peer: String, address: SocketAddr) {
        let addresses = self.failed.entry(peer).or_default();
        if addresses.len() < MAX_FAILED_ADDRESSES_PER_PEER && !addresses.contains(&address) {
            addresses.push(address);
        }
    }

    pub(super) fn excludes(&self, peer: &str, address: SocketAddr) -> bool {
        self.failed.get(peer).is_some_and(|addresses| {
            addresses.len() >= MAX_FAILED_ADDRESSES_PER_PEER || addresses.contains(&address)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::NativeRound;
    use std::net::SocketAddr;

    fn candidate(peer: &'static str, port: u16) -> (&'static str, SocketAddr) {
        (peer, ([127, 0, 0, 1], port).into())
    }

    #[test]
    fn two_status_only_native_peers_yield_to_iroh_state() {
        let a = candidate("native-a", 40101);
        let b = candidate("native-b", 40102);
        let mut round = NativeRound::default();
        let mut failed = Vec::new();
        for _ in 0..6 {
            let status_reply = [a, b]
                .into_iter()
                .find(|(peer, address)| !round.excludes(peer, *address));
            let Some((peer, address)) = status_reply else {
                assert_eq!(failed, [a.0, b.0]);
                return;
            };
            // Both answer Status; neither can supply the first state.
            failed.push(peer);
            round.record_failure(peer.to_owned(), address);
        }
        panic!("Iroh never receives a turn; native attempts: {failed:?}");
    }

    #[test]
    fn first_failure_keeps_a_healthy_native_peer_eligible_and_next_round_retries() {
        let (first, a) = candidate("native-a", 40101);
        let (second, b) = candidate("native-b", 40102);
        let mut round = NativeRound::default();
        round.record_failure(first.to_owned(), a);
        assert!(round.excludes(first, a));
        assert!(!round.excludes(second, b));
        assert!(!NativeRound::default().excludes(first, a));
    }

    #[test]
    fn signed_replacement_address_is_eligible_but_rotation_is_bounded() {
        let (peer, original) = candidate("native-a", 40101);
        let (rotated_peer, rotated) = candidate(peer, 40103);
        assert_ne!(original, rotated);
        let mut round = NativeRound::default();
        round.record_failure(peer.to_owned(), original);
        round.record_failure(peer.to_owned(), original);
        assert!(!round.excludes(rotated_peer, rotated));
        round.record_failure(rotated_peer.to_owned(), rotated);
        let (_, third) = candidate(peer, 40104);
        assert!(round.excludes(peer, third));
    }
}
