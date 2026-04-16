#![forbid(unsafe_code)]

use crate::{EthPeerSelectionWindowPolicyV1, EthPeerSelectionWindowRoleV1};

fn eth_selection_policy_env_keys_v1(chain_id: u64, suffix: &str) -> [String; 2] {
    [
        format!("NOVOVM_NETWORK_ETH_SELECTION_CHAIN_{chain_id}_{suffix}"),
        format!("NOVOVM_NETWORK_ETH_SELECTION_{suffix}"),
    ]
}

fn eth_selection_policy_lookup_u64_v1(
    chain_id: u64,
    suffix: &str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Option<(u64, String)> {
    for key in eth_selection_policy_env_keys_v1(chain_id, suffix) {
        if let Some(raw) = lookup(key.as_str()) {
            if let Ok(parsed) = raw.trim().parse::<u64>() {
                return Some((parsed, key));
            }
        }
    }
    None
}

pub(crate) fn normalize_eth_peer_selection_window_policy_v1(
    policy: &mut EthPeerSelectionWindowPolicyV1,
) {
    policy.medium_term_rounds = policy
        .medium_term_rounds
        .max(policy.short_term_rounds.max(1));
    policy.long_term_rounds = policy.long_term_rounds.max(policy.medium_term_rounds);
    policy.short_term_role = EthPeerSelectionWindowRoleV1::ShortTermVeto;
    policy.medium_term_role = EthPeerSelectionWindowRoleV1::MediumTermStability;
    policy.long_term_role = EthPeerSelectionWindowRoleV1::LongTermRetention;
    policy.sync_short_term_weight_bps = policy.sync_short_term_weight_bps.min(100_000);
    policy.sync_medium_term_weight_bps = policy.sync_medium_term_weight_bps.min(100_000);
    policy.sync_long_term_weight_bps = policy.sync_long_term_weight_bps.min(100_000);
    policy.bootstrap_short_term_weight_bps = policy.bootstrap_short_term_weight_bps.min(100_000);
    policy.bootstrap_medium_term_weight_bps = policy.bootstrap_medium_term_weight_bps.min(100_000);
    policy.bootstrap_long_term_weight_bps = policy.bootstrap_long_term_weight_bps.min(100_000);
    policy.medium_term_selection_hit_rate_floor_bps =
        policy.medium_term_selection_hit_rate_floor_bps.min(10_000);
    policy.long_term_selection_hit_rate_floor_bps =
        policy.long_term_selection_hit_rate_floor_bps.min(10_000);
    policy.long_term_body_success_rate_floor_bps =
        policy.long_term_body_success_rate_floor_bps.min(10_000);
}

pub(crate) fn apply_eth_peer_selection_window_policy_lookup_v1(
    chain_id: u64,
    policy: &mut EthPeerSelectionWindowPolicyV1,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut applied_keys = Vec::new();
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "SHORT_TERM_ROUNDS", lookup)
    {
        policy.short_term_rounds = value.max(1);
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "MEDIUM_TERM_ROUNDS", lookup)
    {
        policy.medium_term_rounds = value.max(1);
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "LONG_TERM_ROUNDS", lookup)
    {
        policy.long_term_rounds = value.max(1);
        applied_keys.push(key);
    }

    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "SYNC_SHORT_TERM_WEIGHT_BPS", lookup)
    {
        policy.sync_short_term_weight_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "SYNC_MEDIUM_TERM_WEIGHT_BPS", lookup)
    {
        policy.sync_medium_term_weight_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "SYNC_LONG_TERM_WEIGHT_BPS", lookup)
    {
        policy.sync_long_term_weight_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "BOOTSTRAP_SHORT_TERM_WEIGHT_BPS", lookup)
    {
        policy.bootstrap_short_term_weight_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "BOOTSTRAP_MEDIUM_TERM_WEIGHT_BPS", lookup)
    {
        policy.bootstrap_medium_term_weight_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_selection_policy_lookup_u64_v1(chain_id, "BOOTSTRAP_LONG_TERM_WEIGHT_BPS", lookup)
    {
        policy.bootstrap_long_term_weight_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) = eth_selection_policy_lookup_u64_v1(
        chain_id,
        "MEDIUM_TERM_SELECTION_HIT_RATE_FLOOR_BPS",
        lookup,
    ) {
        policy.medium_term_selection_hit_rate_floor_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) = eth_selection_policy_lookup_u64_v1(
        chain_id,
        "LONG_TERM_SELECTION_HIT_RATE_FLOOR_BPS",
        lookup,
    ) {
        policy.long_term_selection_hit_rate_floor_bps = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) = eth_selection_policy_lookup_u64_v1(
        chain_id,
        "LONG_TERM_BODY_SUCCESS_RATE_FLOOR_BPS",
        lookup,
    ) {
        policy.long_term_body_success_rate_floor_bps = value;
        applied_keys.push(key);
    }
    normalize_eth_peer_selection_window_policy_v1(policy);
    applied_keys
}

fn resolve_eth_peer_selection_window_policy_with_lookup_v1(
    chain_id: u64,
    lookup: impl Fn(&str) -> Option<String>,
) -> EthPeerSelectionWindowPolicyV1 {
    let mut policy = crate::default_eth_peer_selection_window_policy_v1();
    let _ = apply_eth_peer_selection_window_policy_lookup_v1(chain_id, &mut policy, &lookup);
    policy
}

#[must_use]
pub fn resolve_eth_peer_selection_window_policy_v1(
    chain_id: u64,
) -> EthPeerSelectionWindowPolicyV1 {
    resolve_eth_peer_selection_window_policy_with_lookup_v1(chain_id, |name| {
        std::env::var(name).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn resolve_eth_peer_selection_window_policy_defaults_are_pinned() {
        let policy = resolve_eth_peer_selection_window_policy_with_lookup_v1(1, |_| None);
        assert_eq!(policy.short_term_rounds, 16);
        assert_eq!(policy.medium_term_rounds, 64);
        assert_eq!(policy.long_term_rounds, 256);
        assert_eq!(policy.sync_short_term_weight_bps, 10_000);
        assert_eq!(policy.sync_medium_term_weight_bps, 8_500);
        assert_eq!(policy.sync_long_term_weight_bps, 9_500);
        assert_eq!(policy.bootstrap_short_term_weight_bps, 10_000);
        assert_eq!(policy.bootstrap_medium_term_weight_bps, 6_000);
        assert_eq!(policy.bootstrap_long_term_weight_bps, 3_500);
        assert_eq!(policy.medium_term_selection_hit_rate_floor_bps, 4_500);
        assert_eq!(policy.long_term_selection_hit_rate_floor_bps, 4_000);
        assert_eq!(policy.long_term_body_success_rate_floor_bps, 2_000);
    }

    #[test]
    fn resolve_eth_peer_selection_window_policy_honors_chain_specific_overrides() {
        let mut env = HashMap::new();
        env.insert(
            "NOVOVM_NETWORK_ETH_SELECTION_SYNC_SHORT_TERM_WEIGHT_BPS".to_string(),
            "11111".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_SELECTION_CHAIN_1_SYNC_SHORT_TERM_WEIGHT_BPS".to_string(),
            "22222".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_SELECTION_MEDIUM_TERM_ROUNDS".to_string(),
            "48".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_SELECTION_LONG_TERM_BODY_SUCCESS_RATE_FLOOR_BPS".to_string(),
            "3333".to_string(),
        );
        let policy = resolve_eth_peer_selection_window_policy_with_lookup_v1(1, |name| {
            env.get(name).cloned()
        });
        assert_eq!(policy.sync_short_term_weight_bps, 22_222);
        assert_eq!(policy.medium_term_rounds, 48);
        assert_eq!(policy.long_term_rounds, 256);
        assert_eq!(policy.long_term_body_success_rate_floor_bps, 3_333);
    }
}
