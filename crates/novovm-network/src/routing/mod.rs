pub mod l3_regional;
pub mod l4_local;
pub mod selector;
pub mod sync;
pub mod types;

pub use l3_regional::{
    relay_convergence_policy_view, L3RegionalRoutingSnapshot, L3RegionalRoutingTable,
    L3RelayReadonlyView, RelayConvergencePolicyView, RelayRuntimeFeedbackView,
    L3_BASELINE_FINGERPRINT, L3_BASELINE_LOCK_VERSION, L3_BASELINE_PHASE,
    L3_POLICY_BASELINE_VERSION, L3_READONLY_EXPORT_BASELINE_VERSION, L3_REGRESSION_LOCKSET,
    L3_RELAY_CONVERGENCE_ORDERING, L3_RELAY_RUNTIME_COOLDOWN_PENALTY,
    L3_RELAY_RUNTIME_DECAY_HALF_LIFE_MS, L3_RELAY_RUNTIME_FAILURE_WEIGHT,
    L3_RELAY_RUNTIME_FEEDBACK_SCALE, L3_RELAY_RUNTIME_STREAK_FAILURE_WEIGHT,
    L3_RELAY_RUNTIME_STREAK_SUCCESS_WEIGHT, L3_RELAY_RUNTIME_SUCCESS_WEIGHT, L3_RELAY_SCORE_SCALE,
    L3_RELAY_SELECTED_STICKY_MARGIN, L3_RELAY_SOURCE_BONUS_CONFIGURED, L3_RELAY_SOURCE_BONUS_POOL,
    L3_RELAY_SOURCE_BONUS_SNAPSHOT,
};
pub use l4_local::{L4LocalRoutingSnapshot, L4LocalRoutingTable};
pub use selector::{RouteSelector, SelectedPath};
pub use sync::{PathHint, QueueSummary, RoutingSummary};
pub use types::{
    unix_ms_now, EndpointHint, L4ParticipationLevel, L4PeerRef, NodeId, NodeRef, NodeTier,
    Reachability, RegionId, RelayCapacityClass, RelayHealth, RelayRef, RelayScoreView,
    RoutingSource,
};

pub trait RoutingTableProvider {
    fn l4_local_snapshot(&self) -> L4LocalRoutingSnapshot;

    fn l3_regional_snapshot(&self) -> Option<L3RegionalRoutingSnapshot> {
        None
    }
}

pub trait ParticipationPolicy {
    fn local_participation_level(&self) -> L4ParticipationLevel;

    fn can_act_as_routing_node(&self) -> bool {
        matches!(
            self.local_participation_level(),
            L4ParticipationLevel::RoutingNode
                | L4ParticipationLevel::RelayNode
                | L4ParticipationLevel::NatAssistNode
        )
    }

    fn can_act_as_relay(&self) -> bool {
        matches!(
            self.local_participation_level(),
            L4ParticipationLevel::RelayNode | L4ParticipationLevel::NatAssistNode
        )
    }

    fn can_act_as_nat_assist(&self) -> bool {
        matches!(
            self.local_participation_level(),
            L4ParticipationLevel::NatAssistNode
        )
    }
}

pub trait PathSelection {
    fn select_path(&self) -> SelectedPath;
}
