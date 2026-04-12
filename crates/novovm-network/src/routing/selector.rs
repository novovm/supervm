use super::{
    l3_regional::L3RegionalRoutingTable,
    l4_local::L4LocalRoutingTable,
    types::{L4PeerRef, NodeRef, RelayRef},
};

#[derive(Debug, Clone)]
pub enum SelectedPath {
    DirectL4(L4PeerRef),
    L4Relay(L4PeerRef),
    L3Relay(RelayRef),
    ForcedUpstream(NodeRef),
    ReadOnlyQueue,
}

pub struct RouteSelector<'a> {
    pub l4_local: &'a L4LocalRoutingTable,
    pub l3_regional: Option<&'a L3RegionalRoutingTable>,
}

impl<'a> RouteSelector<'a> {
    pub fn new(
        l4_local: &'a L4LocalRoutingTable,
        l3_regional: Option<&'a L3RegionalRoutingTable>,
    ) -> Self {
        Self {
            l4_local,
            l3_regional,
        }
    }

    pub fn select_best_path(&self) -> SelectedPath {
        if let Some(peer) = self.l4_local.best_direct_candidates(1).into_iter().next() {
            return SelectedPath::DirectL4(peer);
        }

        if let Some(peer) = self.l4_local.best_relay_candidates(1).into_iter().next() {
            return SelectedPath::L4Relay(peer);
        }

        if let Some(l3) = self.l3_regional {
            if let Some(relay) = l3.healthy_relays().into_iter().next() {
                return SelectedPath::L3Relay(relay);
            }

            if let Some(upstream) = l3.any_upstream() {
                return SelectedPath::ForcedUpstream(upstream);
            }
        }

        SelectedPath::ReadOnlyQueue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::types::{
        L4ParticipationLevel, L4PeerRef, NodeRef, NodeTier, Reachability, RelayCapacityClass,
        RelayHealth, RelayRef, RoutingSource,
    };

    #[test]
    fn prefers_l4_direct_before_all_other_paths() {
        let l4 = L4LocalRoutingTable::new("self", 8);
        l4.upsert_peer(
            L4PeerRef::new("peer-direct".into())
                .with_reachability(Reachability::Reachable)
                .with_score(5),
        );

        let selector = RouteSelector::new(&l4, None);
        match selector.select_best_path() {
            SelectedPath::DirectL4(peer) => assert_eq!(peer.node_id, "peer-direct"),
            _ => panic!("expected direct path"),
        }
    }

    #[test]
    fn falls_back_to_l3_relay_then_upstream() {
        let l4 = L4LocalRoutingTable::new("self", 8);
        let l3 = L3RegionalRoutingTable::new("ap-south");

        l3.upsert_l3_relay(RelayRef {
            node_id: "relay-1".into(),
            region: "ap-south".into(),
            addr: "127.0.0.1:9000".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        let selector = RouteSelector::new(&l4, Some(&l3));
        match selector.select_best_path() {
            SelectedPath::L3Relay(relay) => assert_eq!(relay.node_id, "relay-1"),
            _ => panic!("expected l3 relay"),
        }

        let l4 = L4LocalRoutingTable::new("self", 8);
        let l3 = L3RegionalRoutingTable::new("ap-south");
        l3.upsert_l2_upstream(NodeRef {
            node_id: "upstream-1".into(),
            tier: NodeTier::L2,
            region: Some("ap-south".into()),
            addr_hint: Some("10.1.1.1:30303".into()),
            source: RoutingSource::RegionalAnnounced,
        });

        let selector = RouteSelector::new(&l4, Some(&l3));
        match selector.select_best_path() {
            SelectedPath::ForcedUpstream(node) => assert_eq!(node.node_id, "upstream-1"),
            _ => panic!("expected forced upstream"),
        }
    }

    #[test]
    fn falls_back_to_queue_when_nothing_is_available() {
        let l4 = L4LocalRoutingTable::new("self", 8);
        let selector = RouteSelector::new(&l4, None);

        match selector.select_best_path() {
            SelectedPath::ReadOnlyQueue => {}
            _ => panic!("expected queue fallback"),
        }
    }

    #[test]
    fn prefers_l4_relay_before_l3_relay() {
        let l4 = L4LocalRoutingTable::new("self", 8);
        l4.upsert_peer(
            L4PeerRef::new("relay-l4".into())
                .with_role(L4ParticipationLevel::RelayNode)
                .with_reachability(Reachability::RelayOnly)
                .with_score(9),
        );
        let l3 = L3RegionalRoutingTable::new("ap-north");
        l3.upsert_l3_relay(RelayRef {
            node_id: "relay-l3".into(),
            region: "ap-north".into(),
            addr: "127.0.0.1:9555".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        let selector = RouteSelector::new(&l4, Some(&l3));
        match selector.select_best_path() {
            SelectedPath::L4Relay(peer) => assert_eq!(peer.node_id, "relay-l4"),
            _ => panic!("expected l4 relay"),
        }
    }

    #[test]
    fn hint_only_peer_does_not_preempt_l3_relay() {
        let l4 = L4LocalRoutingTable::new("self", 8);
        l4.upsert_peer(
            L4PeerRef::new("peer-hint".into())
                .with_reachability(Reachability::Unknown)
                .with_score(100)
                .with_source(RoutingSource::OperatorForced),
        );
        let l3 = L3RegionalRoutingTable::new("ap-south");
        l3.upsert_l3_relay(RelayRef {
            node_id: "relay-1".into(),
            region: "ap-south".into(),
            addr: "127.0.0.1:9000".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        let selector = RouteSelector::new(&l4, Some(&l3));
        match selector.select_best_path() {
            SelectedPath::L3Relay(relay) => assert_eq!(relay.node_id, "relay-1"),
            _ => panic!("expected l3 relay"),
        }
    }

    #[test]
    fn local_observed_reachable_peer_drives_direct_l4() {
        let l4 = L4LocalRoutingTable::new("self", 8);
        l4.upsert_peer(
            L4PeerRef::new("peer-observed".into())
                .with_reachability(Reachability::Reachable)
                .with_source(RoutingSource::LocalObserved),
        );
        let l3 = L3RegionalRoutingTable::new("ap-south");
        l3.upsert_l3_relay(RelayRef {
            node_id: "relay-1".into(),
            region: "ap-south".into(),
            addr: "127.0.0.1:9000".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        let selector = RouteSelector::new(&l4, Some(&l3));
        match selector.select_best_path() {
            SelectedPath::DirectL4(peer) => assert_eq!(peer.node_id, "peer-observed"),
            _ => panic!("expected direct l4"),
        }
    }
}
