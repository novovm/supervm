use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use super::types::{
    unix_ms_now, NodeId, NodeRef, RegionId, RelayHealth, RelayRef, RelayScoreView, RoutingSource,
};

pub const L3_RELAY_SCORE_SCALE: i64 = 10;
pub const L3_RELAY_SELECTED_STICKY_MARGIN: i64 = 15;
pub const L3_RELAY_SOURCE_BONUS_CONFIGURED: i64 = 1;
pub const L3_RELAY_SOURCE_BONUS_SNAPSHOT: i64 = 3;
pub const L3_RELAY_SOURCE_BONUS_POOL: i64 = 6;
pub const L3_RELAY_CONVERGENCE_ORDERING: &str =
    "forced>health>composite(score*scale+source_bonus)>relay_id";
pub const L3_BASELINE_PHASE: &str = "F.0-F.5";
pub const L3_POLICY_BASELINE_VERSION: u32 = 1;
pub const L3_READONLY_EXPORT_BASELINE_VERSION: u32 = 1;
pub const L3_BASELINE_LOCK_VERSION: u32 = 1;
pub const L3_REGRESSION_LOCKSET: &str = "relay_path_tests+queue_replay_smoke";
pub const L3_BASELINE_FINGERPRINT: &str =
    "l3-baseline:f0-f5:v1:forced>health>composite(score*scale+source_bonus)>relay_id";
pub const L3_RELAY_RUNTIME_FEEDBACK_SCALE: i64 = 4;
pub const L3_RELAY_RUNTIME_SUCCESS_WEIGHT: i64 = 6;
pub const L3_RELAY_RUNTIME_FAILURE_WEIGHT: i64 = 8;
pub const L3_RELAY_RUNTIME_STREAK_SUCCESS_WEIGHT: i64 = 2;
pub const L3_RELAY_RUNTIME_STREAK_FAILURE_WEIGHT: i64 = 3;
pub const L3_RELAY_RUNTIME_COOLDOWN_PENALTY: i64 = 30;
pub const L3_RELAY_RUNTIME_DECAY_HALF_LIFE_MS: u64 = 120_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayConvergencePolicyView {
    pub policy_baseline_version: u32,
    pub baseline_lock_version: u32,
    pub baseline_phase: &'static str,
    pub regression_lockset: &'static str,
    pub baseline_fingerprint: &'static str,
    pub score_scale: i64,
    pub selected_sticky_margin: i64,
    pub source_bonus_configured: i64,
    pub source_bonus_snapshot: i64,
    pub source_bonus_pool: i64,
    pub runtime_feedback_scale: i64,
    pub runtime_success_weight: i64,
    pub runtime_failure_weight: i64,
    pub runtime_streak_success_weight: i64,
    pub runtime_streak_failure_weight: i64,
    pub runtime_cooldown_penalty: i64,
    pub runtime_decay_half_life_ms: u64,
    pub ordering: &'static str,
}

pub fn relay_convergence_policy_view() -> RelayConvergencePolicyView {
    RelayConvergencePolicyView {
        policy_baseline_version: L3_POLICY_BASELINE_VERSION,
        baseline_lock_version: L3_BASELINE_LOCK_VERSION,
        baseline_phase: L3_BASELINE_PHASE,
        regression_lockset: L3_REGRESSION_LOCKSET,
        baseline_fingerprint: L3_BASELINE_FINGERPRINT,
        score_scale: L3_RELAY_SCORE_SCALE,
        selected_sticky_margin: L3_RELAY_SELECTED_STICKY_MARGIN,
        source_bonus_configured: L3_RELAY_SOURCE_BONUS_CONFIGURED,
        source_bonus_snapshot: L3_RELAY_SOURCE_BONUS_SNAPSHOT,
        source_bonus_pool: L3_RELAY_SOURCE_BONUS_POOL,
        runtime_feedback_scale: L3_RELAY_RUNTIME_FEEDBACK_SCALE,
        runtime_success_weight: L3_RELAY_RUNTIME_SUCCESS_WEIGHT,
        runtime_failure_weight: L3_RELAY_RUNTIME_FAILURE_WEIGHT,
        runtime_streak_success_weight: L3_RELAY_RUNTIME_STREAK_SUCCESS_WEIGHT,
        runtime_streak_failure_weight: L3_RELAY_RUNTIME_STREAK_FAILURE_WEIGHT,
        runtime_cooldown_penalty: L3_RELAY_RUNTIME_COOLDOWN_PENALTY,
        runtime_decay_half_life_ms: L3_RELAY_RUNTIME_DECAY_HALF_LIFE_MS,
        ordering: L3_RELAY_CONVERGENCE_ORDERING,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L3RelayReadonlyView {
    pub readonly_export_baseline_version: u32,
    pub baseline_lock_version: u32,
    pub baseline_phase: &'static str,
    pub regression_lockset: &'static str,
    pub baseline_fingerprint: &'static str,
    pub policy: RelayConvergencePolicyView,
    pub runtime_feedback: Vec<RelayRuntimeFeedbackView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRuntimeFeedbackView {
    pub relay_id: String,
    pub cooldown_until_unix_ms: Option<u64>,
    pub recent_successes: u16,
    pub recent_failures: u16,
    pub consecutive_successes: u16,
    pub consecutive_failures: u16,
    pub runtime_score: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct RelayRuntimeFeedback {
    recent_successes: u16,
    recent_failures: u16,
    consecutive_successes: u16,
    consecutive_failures: u16,
    last_event_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub struct L3RegionalRoutingSnapshot {
    pub region_id: RegionId,
    pub version: u64,
    pub l2_upstreams: Vec<NodeRef>,
    pub l3_relays: Vec<RelayRef>,
    pub l4_neighbors: Vec<NodeRef>,
    pub nat_assist_nodes: Vec<NodeRef>,
    pub last_updated_unix_ms: u64,
}

pub struct L3RegionalRoutingTable {
    region_id: RegionId,
    version: RwLock<u64>,
    l2_upstreams: RwLock<HashMap<NodeId, NodeRef>>,
    l3_relays: RwLock<HashMap<NodeId, RelayRef>>,
    relay_cooldown_until_unix_ms: RwLock<HashMap<NodeId, u64>>,
    relay_runtime_feedback: RwLock<HashMap<NodeId, RelayRuntimeFeedback>>,
    l4_neighbors: RwLock<HashMap<NodeId, NodeRef>>,
    nat_assist_nodes: RwLock<HashMap<NodeId, NodeRef>>,
    last_updated_unix_ms: RwLock<u64>,
}

impl L3RegionalRoutingTable {
    pub fn new(region_id: impl Into<String>) -> Self {
        Self {
            region_id: region_id.into(),
            version: RwLock::new(1),
            l2_upstreams: RwLock::new(HashMap::new()),
            l3_relays: RwLock::new(HashMap::new()),
            relay_cooldown_until_unix_ms: RwLock::new(HashMap::new()),
            relay_runtime_feedback: RwLock::new(HashMap::new()),
            l4_neighbors: RwLock::new(HashMap::new()),
            nat_assist_nodes: RwLock::new(HashMap::new()),
            last_updated_unix_ms: RwLock::new(unix_ms_now()),
        }
    }

    pub fn upsert_l2_upstream(&self, node: NodeRef) {
        self.l2_upstreams
            .write()
            .unwrap()
            .insert(node.node_id.clone(), node);
        self.touch();
    }

    pub fn upsert_l3_relay(&self, relay: RelayRef) {
        self.l3_relays
            .write()
            .unwrap()
            .insert(relay.node_id.clone(), relay);
        self.touch();
    }

    pub fn upsert_l3_relay_discovered(&self, mut relay: RelayRef) -> bool {
        let mut relays = self.l3_relays.write().unwrap();
        if let Some(existing) = relays.get(&relay.node_id) {
            if matches!(existing.source, RoutingSource::OperatorForced)
                && !matches!(relay.source, RoutingSource::OperatorForced)
            {
                return false;
            }
            relay.score = relay.score.max(existing.score);
        }
        relays.insert(relay.node_id.clone(), relay);
        drop(relays);
        self.touch();
        true
    }

    pub fn prune_discovered_relays_except(&self, keep_relay_ids: &HashSet<String>) -> usize {
        let mut relays = self.l3_relays.write().unwrap();
        let before = relays.len();
        relays.retain(|relay_id, relay| {
            if keep_relay_ids.contains(relay_id) {
                return true;
            }
            !matches!(
                relay.source,
                RoutingSource::PeerHinted | RoutingSource::LocalObserved
            )
        });
        let removed = before.saturating_sub(relays.len());
        drop(relays);
        if removed > 0 {
            self.touch();
        }
        removed
    }

    pub fn upsert_l4_neighbor(&self, node: NodeRef) {
        self.l4_neighbors
            .write()
            .unwrap()
            .insert(node.node_id.clone(), node);
        self.touch();
    }

    pub fn upsert_nat_assist_node(&self, node: NodeRef) {
        self.nat_assist_nodes
            .write()
            .unwrap()
            .insert(node.node_id.clone(), node);
        self.touch();
    }

    pub fn healthy_relays(&self) -> Vec<RelayRef> {
        let mut relays: Vec<_> = self
            .l3_relays
            .read()
            .unwrap()
            .values()
            .filter(|r| matches!(r.health, RelayHealth::Healthy | RelayHealth::Degraded))
            .cloned()
            .collect();

        relays.sort_by(|left, right| {
            relay_health_rank(left.health)
                .cmp(&relay_health_rank(right.health))
                .then_with(|| right.score.cmp(&left.score))
                .then_with(|| left.node_id.cmp(&right.node_id))
                .then_with(|| left.addr.cmp(&right.addr))
        });

        relays
    }

    pub fn relay_pool(&self, now_unix_ms: u64, max_candidates: usize) -> Vec<RelayRef> {
        let cooldowns = self.relay_cooldown_until_unix_ms.read().unwrap();
        let mut relays: Vec<_> = self
            .l3_relays
            .read()
            .unwrap()
            .values()
            .filter(|r| matches!(r.health, RelayHealth::Healthy | RelayHealth::Degraded))
            .filter(|r| {
                cooldowns
                    .get(&r.node_id)
                    .map(|until| *until <= now_unix_ms)
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        relays.sort_by(|left, right| {
            relay_health_rank(left.health)
                .cmp(&relay_health_rank(right.health))
                .then_with(|| right.score.cmp(&left.score))
                .then_with(|| left.node_id.cmp(&right.node_id))
                .then_with(|| left.addr.cmp(&right.addr))
        });
        relays.truncate(max_candidates.max(1));
        relays
    }

    pub fn relay_is_available(&self, relay_id: &str, now_unix_ms: u64) -> bool {
        let relays = self.l3_relays.read().unwrap();
        let relay = match relays.get(relay_id) {
            Some(v) => v,
            None => return false,
        };
        if !matches!(relay.health, RelayHealth::Healthy | RelayHealth::Degraded) {
            return false;
        }
        self.relay_cooldown_until_unix_ms
            .read()
            .unwrap()
            .get(relay_id)
            .map(|until| *until <= now_unix_ms)
            .unwrap_or(true)
    }

    pub fn relay_score_snapshot(&self) -> Vec<RelayScoreView> {
        let mut views: Vec<_> = self
            .l3_relays
            .read()
            .unwrap()
            .values()
            .map(|relay| RelayScoreView {
                relay_id: relay.node_id.clone(),
                health: relay.health,
                score: relay.score,
            })
            .collect();

        views.sort_by(|left, right| {
            relay_health_rank(left.health)
                .cmp(&relay_health_rank(right.health))
                .then_with(|| right.score.cmp(&left.score))
                .then_with(|| left.relay_id.cmp(&right.relay_id))
        });

        views
    }

    pub fn relay_score_view(&self, relay_id: &str) -> Option<RelayScoreView> {
        self.l3_relays
            .read()
            .unwrap()
            .get(relay_id)
            .map(|relay| RelayScoreView {
                relay_id: relay.node_id.clone(),
                health: relay.health,
                score: relay.score,
            })
    }

    pub fn relay_runtime_feedback_score(&self, relay_id: &str, now_unix_ms: u64) -> i64 {
        let in_cooldown = self
            .relay_cooldown_until_unix_ms
            .read()
            .unwrap()
            .get(relay_id)
            .map(|until| *until > now_unix_ms)
            .unwrap_or(false);
        let feedback = self
            .relay_runtime_feedback
            .read()
            .unwrap()
            .get(relay_id)
            .copied();
        relay_runtime_feedback_score_inner(feedback, now_unix_ms, in_cooldown)
    }

    pub fn relay_runtime_feedback_snapshot(
        &self,
        now_unix_ms: u64,
    ) -> Vec<RelayRuntimeFeedbackView> {
        let relays = self.l3_relays.read().unwrap();
        let cooldowns = self.relay_cooldown_until_unix_ms.read().unwrap();
        let feedbacks = self.relay_runtime_feedback.read().unwrap();
        let mut views: Vec<_> = relays
            .keys()
            .map(|relay_id| {
                let cooldown_until_unix_ms = cooldowns.get(relay_id).copied();
                let in_cooldown = cooldown_until_unix_ms
                    .map(|until| until > now_unix_ms)
                    .unwrap_or(false);
                let feedback = feedbacks.get(relay_id).copied().unwrap_or_default();
                RelayRuntimeFeedbackView {
                    relay_id: relay_id.clone(),
                    cooldown_until_unix_ms,
                    recent_successes: feedback.recent_successes,
                    recent_failures: feedback.recent_failures,
                    consecutive_successes: feedback.consecutive_successes,
                    consecutive_failures: feedback.consecutive_failures,
                    runtime_score: relay_runtime_feedback_score_inner(
                        Some(feedback),
                        now_unix_ms,
                        in_cooldown,
                    ),
                }
            })
            .collect();

        views.sort_by(|left, right| {
            right
                .runtime_score
                .cmp(&left.runtime_score)
                .then_with(|| left.relay_id.cmp(&right.relay_id))
        });
        views
    }

    pub fn relay_readonly_view(
        &self,
        now_unix_ms: u64,
        max_runtime_items: usize,
    ) -> L3RelayReadonlyView {
        let mut runtime_feedback = self.relay_runtime_feedback_snapshot(now_unix_ms);
        if max_runtime_items > 0 {
            runtime_feedback.truncate(max_runtime_items);
        } else {
            runtime_feedback.clear();
        }
        L3RelayReadonlyView {
            readonly_export_baseline_version: L3_READONLY_EXPORT_BASELINE_VERSION,
            baseline_lock_version: L3_BASELINE_LOCK_VERSION,
            baseline_phase: L3_BASELINE_PHASE,
            regression_lockset: L3_REGRESSION_LOCKSET,
            baseline_fingerprint: L3_BASELINE_FINGERPRINT,
            policy: relay_convergence_policy_view(),
            runtime_feedback,
        }
    }

    pub fn bump_relay_score(&self, relay_id: &str, delta: i32) -> bool {
        let mut relays = self.l3_relays.write().unwrap();
        if let Some(relay) = relays.get_mut(relay_id) {
            relay.score = relay.score.saturating_add(delta);
            drop(relays);
            self.touch();
            true
        } else {
            false
        }
    }

    pub fn mark_relay_success(&self, relay_id: &str) -> bool {
        let mut relays = self.l3_relays.write().unwrap();
        if let Some(relay) = relays.get_mut(relay_id) {
            relay.score = relay.score.saturating_add(1);
            drop(relays);
            self.relay_cooldown_until_unix_ms
                .write()
                .unwrap()
                .remove(relay_id);
            let now = unix_ms_now();
            let mut feedback = self.relay_runtime_feedback.write().unwrap();
            let entry = feedback.entry(relay_id.to_string()).or_default();
            entry.recent_successes = entry.recent_successes.saturating_add(1);
            entry.recent_failures = entry.recent_failures.saturating_sub(1);
            entry.consecutive_successes = entry.consecutive_successes.saturating_add(1);
            entry.consecutive_failures = 0;
            entry.last_event_unix_ms = now;
            self.touch();
            true
        } else {
            false
        }
    }

    pub fn mark_relay_failure(&self, relay_id: &str, now_unix_ms: u64, cooldown_ms: u64) -> bool {
        let mut relays = self.l3_relays.write().unwrap();
        if let Some(relay) = relays.get_mut(relay_id) {
            relay.score = relay.score.saturating_sub(1);
            drop(relays);
            let until = now_unix_ms.saturating_add(cooldown_ms);
            self.relay_cooldown_until_unix_ms
                .write()
                .unwrap()
                .insert(relay_id.to_string(), until);
            let mut feedback = self.relay_runtime_feedback.write().unwrap();
            let entry = feedback.entry(relay_id.to_string()).or_default();
            entry.recent_failures = entry.recent_failures.saturating_add(1);
            entry.recent_successes = entry.recent_successes.saturating_sub(1);
            entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
            entry.consecutive_successes = 0;
            entry.last_event_unix_ms = now_unix_ms;
            self.touch();
            true
        } else {
            false
        }
    }

    pub fn any_upstream(&self) -> Option<NodeRef> {
        let mut upstreams: Vec<_> = self
            .l2_upstreams
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect();
        upstreams.sort_by(|left, right| {
            left.node_id
                .cmp(&right.node_id)
                .then_with(|| left.addr_hint.cmp(&right.addr_hint))
        });
        upstreams.into_iter().next()
    }

    pub fn snapshot(&self) -> L3RegionalRoutingSnapshot {
        L3RegionalRoutingSnapshot {
            region_id: self.region_id.clone(),
            version: *self.version.read().unwrap(),
            l2_upstreams: self
                .l2_upstreams
                .read()
                .unwrap()
                .values()
                .cloned()
                .collect(),
            l3_relays: self.l3_relays.read().unwrap().values().cloned().collect(),
            l4_neighbors: self
                .l4_neighbors
                .read()
                .unwrap()
                .values()
                .cloned()
                .collect(),
            nat_assist_nodes: self
                .nat_assist_nodes
                .read()
                .unwrap()
                .values()
                .cloned()
                .collect(),
            last_updated_unix_ms: *self.last_updated_unix_ms.read().unwrap(),
        }
    }

    fn touch(&self) {
        *self.version.write().unwrap() += 1;
        *self.last_updated_unix_ms.write().unwrap() = unix_ms_now();
    }
}

fn relay_health_rank(health: RelayHealth) -> u8 {
    match health {
        RelayHealth::Healthy => 0,
        RelayHealth::Degraded => 1,
        RelayHealth::Unavailable => 2,
    }
}

fn relay_runtime_feedback_score_inner(
    feedback: Option<RelayRuntimeFeedback>,
    now_unix_ms: u64,
    in_cooldown: bool,
) -> i64 {
    let mut score = 0i64;
    if let Some(feedback) = feedback {
        score += i64::from(feedback.recent_successes) * L3_RELAY_RUNTIME_SUCCESS_WEIGHT;
        score -= i64::from(feedback.recent_failures) * L3_RELAY_RUNTIME_FAILURE_WEIGHT;
        score += i64::from(feedback.consecutive_successes) * L3_RELAY_RUNTIME_STREAK_SUCCESS_WEIGHT;
        score -= i64::from(feedback.consecutive_failures) * L3_RELAY_RUNTIME_STREAK_FAILURE_WEIGHT;

        if feedback.last_event_unix_ms > 0 && now_unix_ms > feedback.last_event_unix_ms {
            let age_ms = now_unix_ms - feedback.last_event_unix_ms;
            let decay_steps = (age_ms / L3_RELAY_RUNTIME_DECAY_HALF_LIFE_MS).min(4);
            for _ in 0..decay_steps {
                score /= 2;
            }
        }
    }

    if in_cooldown {
        score -= L3_RELAY_RUNTIME_COOLDOWN_PENALTY;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::types::{NodeTier, RelayCapacityClass, RelayHealth, RoutingSource};

    #[test]
    fn healthy_relays_prefers_healthy_before_degraded() {
        let table = L3RegionalRoutingTable::new("ap-east");

        table.upsert_l3_relay(RelayRef {
            node_id: "relay-degraded".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9001".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Degraded,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        table.upsert_l3_relay(RelayRef {
            node_id: "relay-healthy".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9002".into(),
            capacity_class: RelayCapacityClass::Large,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        let relays = table.healthy_relays();
        assert_eq!(relays.len(), 2);
        assert_eq!(relays[0].node_id, "relay-healthy");
    }

    #[test]
    fn any_upstream_returns_inserted_upstream() {
        let table = L3RegionalRoutingTable::new("eu-west");

        table.upsert_l2_upstream(NodeRef {
            node_id: "l2-up-1".into(),
            tier: NodeTier::L2,
            region: Some("eu-west".into()),
            addr_hint: Some("10.0.0.1:30303".into()),
            source: RoutingSource::RegionalAnnounced,
        });

        let upstream = table.any_upstream().expect("expected upstream");
        assert_eq!(upstream.node_id, "l2-up-1");
    }

    #[test]
    fn healthy_relays_prefers_higher_score_with_same_health() {
        let table = L3RegionalRoutingTable::new("ap-east");

        table.upsert_l3_relay(RelayRef {
            node_id: "relay-low".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9101".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        table.upsert_l3_relay(RelayRef {
            node_id: "relay-high".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9102".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        assert!(table.bump_relay_score("relay-high", 2));
        let relays = table.healthy_relays();
        assert_eq!(relays[0].node_id, "relay-high");
    }

    #[test]
    fn relay_score_snapshot_orders_by_health_score_and_node_id() {
        let table = L3RegionalRoutingTable::new("ap-east");

        table.upsert_l3_relay(RelayRef {
            node_id: "relay-b".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9202".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 1,
            source: RoutingSource::RegionalAnnounced,
        });

        table.upsert_l3_relay(RelayRef {
            node_id: "relay-a".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9201".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 1,
            source: RoutingSource::RegionalAnnounced,
        });

        table.upsert_l3_relay(RelayRef {
            node_id: "relay-c".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9203".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Degraded,
            score: 100,
            source: RoutingSource::RegionalAnnounced,
        });

        let snapshot = table.relay_score_snapshot();
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot[0].relay_id, "relay-a");
        assert_eq!(snapshot[1].relay_id, "relay-b");
        assert_eq!(snapshot[2].relay_id, "relay-c");
    }

    #[test]
    fn relay_pool_skips_relay_in_cooldown() {
        let table = L3RegionalRoutingTable::new("ap-east");
        table.upsert_l3_relay(RelayRef {
            node_id: "relay-a".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9301".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 1,
            source: RoutingSource::RegionalAnnounced,
        });
        table.upsert_l3_relay(RelayRef {
            node_id: "relay-b".into(),
            region: "ap-east".into(),
            addr: "127.0.0.1:9302".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 0,
            source: RoutingSource::RegionalAnnounced,
        });

        let now = 1_000_000u64;
        assert!(table.mark_relay_failure("relay-a", now, 10_000));
        let pool = table.relay_pool(now + 1, 3);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool[0].node_id, "relay-b");
        let pool_after = table.relay_pool(now + 10_001, 3);
        assert_eq!(pool_after.len(), 2);
    }

    #[test]
    fn l3_readonly_baseline_constants_are_stable() {
        assert_eq!(L3_BASELINE_PHASE, "F.0-F.5");
        assert_eq!(L3_POLICY_BASELINE_VERSION, 1);
        assert_eq!(L3_READONLY_EXPORT_BASELINE_VERSION, 1);
        assert_eq!(L3_BASELINE_LOCK_VERSION, 1);
        assert_eq!(L3_REGRESSION_LOCKSET, "relay_path_tests+queue_replay_smoke");
        assert_eq!(
            L3_BASELINE_FINGERPRINT,
            "l3-baseline:f0-f5:v1:forced>health>composite(score*scale+source_bonus)>relay_id"
        );
    }

    #[test]
    fn discovered_upsert_must_not_override_operator_forced_relay() {
        let table = L3RegionalRoutingTable::new("ap-east");
        table.upsert_l3_relay(RelayRef {
            node_id: "relay-forced".into(),
            region: "ap-east".into(),
            addr: "forced.addr:9001".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 42,
            source: RoutingSource::OperatorForced,
        });

        let applied = table.upsert_l3_relay_discovered(RelayRef {
            node_id: "relay-forced".into(),
            region: "ap-east".into(),
            addr: "discovered.addr:9001".into(),
            capacity_class: RelayCapacityClass::Large,
            health: RelayHealth::Degraded,
            score: -7,
            source: RoutingSource::PeerHinted,
        });
        assert!(!applied);

        let keep = table
            .relay_score_view("relay-forced")
            .expect("relay exists");
        assert_eq!(keep.score, 42);
        assert_eq!(keep.health, RelayHealth::Healthy);
    }

    #[test]
    fn prune_discovered_relays_except_keeps_forced_and_static() {
        let table = L3RegionalRoutingTable::new("ap-east");
        table.upsert_l3_relay(RelayRef {
            node_id: "relay-forced".into(),
            region: "ap-east".into(),
            addr: "forced.addr:9001".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 10,
            source: RoutingSource::OperatorForced,
        });
        table.upsert_l3_relay(RelayRef {
            node_id: "relay-static".into(),
            region: "ap-east".into(),
            addr: "static.addr:9002".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 3,
            source: RoutingSource::RegionalAnnounced,
        });
        table.upsert_l3_relay(RelayRef {
            node_id: "relay-discovery".into(),
            region: "ap-east".into(),
            addr: "discovery.addr:9003".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 1,
            source: RoutingSource::PeerHinted,
        });
        table.upsert_l3_relay(RelayRef {
            node_id: "relay-local".into(),
            region: "ap-east".into(),
            addr: "local.addr:9004".into(),
            capacity_class: RelayCapacityClass::Medium,
            health: RelayHealth::Healthy,
            score: 1,
            source: RoutingSource::LocalObserved,
        });

        let mut keep = HashSet::new();
        keep.insert("relay-discovery".to_string());
        let removed = table.prune_discovered_relays_except(&keep);
        assert_eq!(removed, 1);

        let relays = table.snapshot().l3_relays;
        let ids: HashSet<_> = relays.into_iter().map(|r| r.node_id).collect();
        assert!(ids.contains("relay-forced"));
        assert!(ids.contains("relay-static"));
        assert!(ids.contains("relay-discovery"));
        assert!(!ids.contains("relay-local"));
    }
}
