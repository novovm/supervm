use crate::availability::AvailabilityDecision;
use crate::routing::SelectedPath;

#[derive(Debug, Default, Clone, Copy)]
pub struct AvailabilityController;

impl AvailabilityController {
    pub fn new() -> Self {
        Self
    }

    pub fn decide(&self, selected_path: &SelectedPath) -> AvailabilityDecision {
        match selected_path {
            SelectedPath::DirectL4(_)
            | SelectedPath::L4Relay(_)
            | SelectedPath::L3Relay(_)
            | SelectedPath::ForcedUpstream(_) => AvailabilityDecision::normal("path_available"),
            SelectedPath::ReadOnlyQueue => AvailabilityDecision::queue_only("no_path"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{
        L4PeerRef, NodeRef, NodeTier, Reachability, RelayCapacityClass, RelayHealth, RelayRef,
        SelectedPath, RoutingSource,
    };

    #[test]
    fn maps_readonly_queue_to_queue_only() {
        let c = AvailabilityController::new();
        let decision = c.decide(&SelectedPath::ReadOnlyQueue);
        assert_eq!(decision.mode, crate::availability::AvailabilityMode::QueueOnly);
    }

    #[test]
    fn maps_routable_paths_to_normal() {
        let c = AvailabilityController::new();

        let direct = SelectedPath::DirectL4(
            L4PeerRef::new("peer-1".to_string()).with_reachability(Reachability::Reachable),
        );
        assert_eq!(c.decide(&direct).mode, crate::availability::AvailabilityMode::Normal);

        let l4_relay = SelectedPath::L4Relay(
            L4PeerRef::new("peer-2".to_string())
                .with_reachability(Reachability::RelayOnly),
        );
        assert_eq!(
            c.decide(&l4_relay).mode,
            crate::availability::AvailabilityMode::Normal
        );

        let l3_relay = SelectedPath::L3Relay(RelayRef {
            node_id: "relay-1".to_string(),
            region: "global".to_string(),
            addr: "127.0.0.1:9000".to_string(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });
        assert_eq!(
            c.decide(&l3_relay).mode,
            crate::availability::AvailabilityMode::Normal
        );

        let upstream = SelectedPath::ForcedUpstream(NodeRef {
            node_id: "upstream-1".to_string(),
            tier: NodeTier::L2,
            region: Some("global".to_string()),
            addr_hint: Some("10.0.0.1:30303".to_string()),
            source: RoutingSource::RegionalAnnounced,
        });
        assert_eq!(
            c.decide(&upstream).mode,
            crate::availability::AvailabilityMode::Normal
        );
    }
}
