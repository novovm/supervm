#![forbid(unsafe_code)]

use crate::eth_rlpx::{EthForkIdV1, EthRlpxStatusV1};
use serde::{Deserialize, Serialize};

pub const ETH_MAINNET_GENESIS_HASH_V1: [u8; 32] = [
    0xd4, 0xe5, 0x67, 0x40, 0xf8, 0x76, 0xae, 0xf8, 0xc0, 0x10, 0xb8, 0x6a, 0x40, 0xd5, 0xf5, 0x67,
    0x45, 0xa1, 0x18, 0xd0, 0x90, 0x6a, 0x34, 0xe6, 0x9a, 0xec, 0x8c, 0x0d, 0xb1, 0xcb, 0x8f, 0xa3,
];

pub const ETH_MAINNET_FORK_ID_TIMESTAMP_THRESHOLD_V1: u64 = 1_438_269_973;

const ETH_MAINNET_BLOCK_FORKS_V1: &[u64] = &[
    1_150_000, 1_920_000, 2_463_000, 2_675_000, 4_370_000, 7_280_000, 9_069_000, 9_200_000,
    12_244_000, 12_965_000, 13_773_000, 15_050_000,
];

const ETH_MAINNET_TIME_FORKS_V1: &[u64] = &[
    1_681_338_455,
    1_710_338_135,
    1_746_612_311,
    1_764_798_551,
    1_765_290_071,
    1_767_747_671,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeChainForkScheduleV1 {
    pub block_forks: Vec<u64>,
    pub time_forks: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthChainConfigV1 {
    pub chain_id: u64,
    pub genesis_hash: [u8; 32],
    pub fork_id_timestamp_threshold: u64,
    pub fork_schedule: EthNativeChainForkScheduleV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthChainConfigPeerValidationReasonV1 {
    WrongNetwork,
    WrongGenesis,
    RemoteStaleForkId,
    UnsupportedForkProgression,
}

impl EthChainConfigPeerValidationReasonV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WrongNetwork => "wrong_network",
            Self::WrongGenesis => "wrong_genesis",
            Self::RemoteStaleForkId => "remote_stale_fork_id",
            Self::UnsupportedForkProgression => "unsupported_fork_progression",
        }
    }
}

fn eth_chain_config_parse_fixed_hex_v1<const N: usize>(raw: &str) -> Option<[u8; N]> {
    let trimmed = raw.trim().strip_prefix("0x").unwrap_or(raw.trim());
    if trimmed.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (idx, slot) in out.iter_mut().enumerate() {
        let hi = trimmed.as_bytes().get(idx * 2).copied()?;
        let lo = trimmed.as_bytes().get(idx * 2 + 1).copied()?;
        let hi = (hi as char).to_digit(16)? as u8;
        let lo = (lo as char).to_digit(16)? as u8;
        *slot = (hi << 4) | lo;
    }
    Some(out)
}

fn eth_chain_config_parse_u64_list_v1(raw: &str) -> Vec<u64> {
    let mut out = raw
        .split(',')
        .filter_map(|item| {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                None
            } else {
                trimmed.parse::<u64>().ok()
            }
        })
        .collect::<Vec<_>>();
    out.sort_unstable();
    out.dedup();
    out
}

#[must_use]
pub fn resolve_eth_chain_config_v1(chain_id: u64) -> EthChainConfigV1 {
    let genesis_hash = std::env::var("NOVOVM_NETWORK_ETH_GENESIS_HASH_HEX")
        .ok()
        .and_then(|raw| eth_chain_config_parse_fixed_hex_v1::<32>(raw.as_str()))
        .unwrap_or_else(|| match chain_id {
            1 => ETH_MAINNET_GENESIS_HASH_V1,
            _ => [0u8; 32],
        });

    let mut block_forks = match chain_id {
        1 => ETH_MAINNET_BLOCK_FORKS_V1.to_vec(),
        _ => Vec::new(),
    };
    let mut time_forks = match chain_id {
        1 => ETH_MAINNET_TIME_FORKS_V1.to_vec(),
        _ => Vec::new(),
    };

    if let Ok(raw) = std::env::var("NOVOVM_NETWORK_ETH_FORK_BLOCKS") {
        let parsed = eth_chain_config_parse_u64_list_v1(raw.as_str());
        if !parsed.is_empty() {
            block_forks = parsed;
        }
    }
    if let Ok(raw) = std::env::var("NOVOVM_NETWORK_ETH_FORK_TIMES") {
        let parsed = eth_chain_config_parse_u64_list_v1(raw.as_str());
        if !parsed.is_empty() {
            time_forks = parsed;
        }
    }
    let fork_id_timestamp_threshold = std::env::var("NOVOVM_NETWORK_ETH_FORK_TIME_THRESHOLD")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or_else(|| match chain_id {
            1 => ETH_MAINNET_FORK_ID_TIMESTAMP_THRESHOLD_V1,
            _ => time_forks
                .iter()
                .min()
                .copied()
                .map(|fork| fork.saturating_sub(1))
                .unwrap_or(u64::MAX),
        });

    EthChainConfigV1 {
        chain_id,
        genesis_hash,
        fork_id_timestamp_threshold,
        fork_schedule: EthNativeChainForkScheduleV1 {
            block_forks,
            time_forks,
        },
    }
}

#[must_use]
pub fn eth_chain_config_genesis_hash_v1(chain_id: u64) -> [u8; 32] {
    resolve_eth_chain_config_v1(chain_id).genesis_hash
}

#[must_use]
pub fn eth_native_chain_fork_schedule_v1(chain_id: u64) -> EthNativeChainForkScheduleV1 {
    resolve_eth_chain_config_v1(chain_id).fork_schedule
}

fn eth_chain_config_crc32_ieee_update_v1(crc: u32, bytes: &[u8]) -> u32 {
    let mut state = !crc;
    for &byte in bytes {
        state ^= byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(state & 1);
            state = (state >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !state
}

fn eth_chain_config_crc32_ieee_v1(bytes: &[u8]) -> u32 {
    eth_chain_config_crc32_ieee_update_v1(0, bytes)
}

fn eth_chain_config_fork_checksum_update_v1(hash: u32, fork: u64) -> u32 {
    eth_chain_config_crc32_ieee_update_v1(hash, fork.to_be_bytes().as_slice())
}

fn eth_chain_config_checksum_to_bytes_v1(hash: u32) -> [u8; 4] {
    hash.to_be_bytes()
}

#[must_use]
pub fn build_eth_native_fork_id_from_schedule_v1(
    schedule: &EthNativeChainForkScheduleV1,
    genesis_hash: [u8; 32],
    head_block: u64,
    head_time: u64,
) -> EthForkIdV1 {
    let mut hash = eth_chain_config_crc32_ieee_v1(genesis_hash.as_slice());
    for &fork in &schedule.block_forks {
        if fork <= head_block {
            hash = eth_chain_config_fork_checksum_update_v1(hash, fork);
        } else {
            return EthForkIdV1 {
                hash: eth_chain_config_checksum_to_bytes_v1(hash),
                next: fork,
            };
        }
    }
    for &fork in &schedule.time_forks {
        if fork <= head_time {
            hash = eth_chain_config_fork_checksum_update_v1(hash, fork);
        } else {
            return EthForkIdV1 {
                hash: eth_chain_config_checksum_to_bytes_v1(hash),
                next: fork,
            };
        }
    }
    EthForkIdV1 {
        hash: eth_chain_config_checksum_to_bytes_v1(hash),
        next: 0,
    }
}

#[must_use]
pub fn build_eth_fork_id_from_chain_config_v1(
    config: &EthChainConfigV1,
    head_block: u64,
    head_time: u64,
) -> EthForkIdV1 {
    build_eth_native_fork_id_from_schedule_v1(
        &config.fork_schedule,
        config.genesis_hash,
        head_block,
        head_time,
    )
}

fn eth_chain_config_fork_checksum_sequence_v1(config: &EthChainConfigV1) -> Vec<[u8; 4]> {
    let mut sums = Vec::with_capacity(
        config.fork_schedule.block_forks.len() + config.fork_schedule.time_forks.len() + 1,
    );
    let mut hash = eth_chain_config_crc32_ieee_v1(config.genesis_hash.as_slice());
    sums.push(eth_chain_config_checksum_to_bytes_v1(hash));
    for &fork in &config.fork_schedule.block_forks {
        hash = eth_chain_config_fork_checksum_update_v1(hash, fork);
        sums.push(eth_chain_config_checksum_to_bytes_v1(hash));
    }
    for &fork in &config.fork_schedule.time_forks {
        hash = eth_chain_config_fork_checksum_update_v1(hash, fork);
        sums.push(eth_chain_config_checksum_to_bytes_v1(hash));
    }
    sums
}

fn eth_chain_config_fork_progression_v1(config: &EthChainConfigV1) -> Vec<(u64, bool)> {
    let mut out = config
        .fork_schedule
        .block_forks
        .iter()
        .copied()
        .map(|fork| (fork, false))
        .collect::<Vec<_>>();
    out.extend(
        config
            .fork_schedule
            .time_forks
            .iter()
            .copied()
            .map(|fork| (fork, true)),
    );
    out.push((u64::MAX, !config.fork_schedule.time_forks.is_empty()));
    out
}

#[must_use]
pub fn validate_eth_fork_id_against_chain_config_v1(
    config: &EthChainConfigV1,
    local_head_block: u64,
    local_head_time: u64,
    remote_fork_id: EthForkIdV1,
) -> Result<(), EthChainConfigPeerValidationReasonV1> {
    let forks = eth_chain_config_fork_progression_v1(config);
    let sums = eth_chain_config_fork_checksum_sequence_v1(config);

    for (idx, (fork, is_time_fork)) in forks.iter().copied().enumerate() {
        let head = if is_time_fork {
            local_head_time
        } else {
            local_head_block
        };
        if head >= fork {
            continue;
        }
        if sums[idx] == remote_fork_id.hash {
            if remote_fork_id.next > 0
                && (local_head_block >= remote_fork_id.next
                    || (remote_fork_id.next > config.fork_id_timestamp_threshold
                        && local_head_time >= remote_fork_id.next))
            {
                return Err(EthChainConfigPeerValidationReasonV1::UnsupportedForkProgression);
            }
            return Ok(());
        }
        for subset_idx in 0..idx {
            if sums[subset_idx] == remote_fork_id.hash {
                if forks[subset_idx].0 != remote_fork_id.next {
                    return Err(EthChainConfigPeerValidationReasonV1::RemoteStaleForkId);
                }
                return Ok(());
            }
        }
        for superset_idx in (idx + 1)..sums.len() {
            if sums[superset_idx] == remote_fork_id.hash {
                return Ok(());
            }
        }
        return Err(EthChainConfigPeerValidationReasonV1::UnsupportedForkProgression);
    }
    Ok(())
}

#[must_use]
pub fn validate_eth_chain_config_peer_status_v1(
    config: &EthChainConfigV1,
    local_head_block: u64,
    local_head_time: u64,
    remote_status: &EthRlpxStatusV1,
) -> Result<(), EthChainConfigPeerValidationReasonV1> {
    if remote_status.network_id != config.chain_id {
        return Err(EthChainConfigPeerValidationReasonV1::WrongNetwork);
    }
    if remote_status.genesis_hash != config.genesis_hash {
        return Err(EthChainConfigPeerValidationReasonV1::WrongGenesis);
    }
    validate_eth_fork_id_against_chain_config_v1(
        config,
        local_head_block,
        local_head_time,
        remote_status.fork_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_eth_chain_config_mainnet_defaults_are_pinned() {
        let config = resolve_eth_chain_config_v1(1);
        assert_eq!(config.chain_id, 1);
        assert_eq!(config.genesis_hash, ETH_MAINNET_GENESIS_HASH_V1);
        assert_eq!(
            config.fork_id_timestamp_threshold,
            ETH_MAINNET_FORK_ID_TIMESTAMP_THRESHOLD_V1
        );
        assert_eq!(config.fork_schedule.block_forks, ETH_MAINNET_BLOCK_FORKS_V1);
        assert_eq!(config.fork_schedule.time_forks, ETH_MAINNET_TIME_FORKS_V1);
    }

    #[test]
    fn local_mainnet_fork_id_matches_geth_reference_vectors() {
        let config = resolve_eth_chain_config_v1(1);
        assert_eq!(
            build_eth_fork_id_from_chain_config_v1(&config, 0, 0),
            EthForkIdV1 {
                hash: [0xfc, 0x64, 0xec, 0x04],
                next: 1_150_000,
            }
        );
        assert_eq!(
            build_eth_fork_id_from_chain_config_v1(&config, 15_050_000, 0),
            EthForkIdV1 {
                hash: [0xf0, 0xaf, 0xd0, 0xe3],
                next: 1_681_338_455,
            }
        );
        assert_eq!(
            build_eth_fork_id_from_chain_config_v1(&config, 30_000_000, 1_710_338_135),
            EthForkIdV1 {
                hash: [0x9f, 0x3d, 0x22, 0x54],
                next: 1_746_612_311,
            }
        );
    }

    #[test]
    fn peer_status_validation_rejects_wrong_network_genesis_and_stale_forks() {
        let config = resolve_eth_chain_config_v1(1);
        assert_eq!(
            validate_eth_chain_config_peer_status_v1(
                &config,
                15_050_000,
                0,
                &EthRlpxStatusV1 {
                    protocol_version: 70,
                    network_id: 9_999,
                    genesis_hash: config.genesis_hash,
                    fork_id: build_eth_fork_id_from_chain_config_v1(&config, 15_050_000, 0),
                    earliest_block: 0,
                    latest_block: 15_050_000,
                    latest_block_hash: [0u8; 32],
                }
            ),
            Err(EthChainConfigPeerValidationReasonV1::WrongNetwork)
        );
        assert_eq!(
            validate_eth_chain_config_peer_status_v1(
                &config,
                15_050_000,
                0,
                &EthRlpxStatusV1 {
                    protocol_version: 70,
                    network_id: 1,
                    genesis_hash: [0x11; 32],
                    fork_id: build_eth_fork_id_from_chain_config_v1(&config, 15_050_000, 0),
                    earliest_block: 0,
                    latest_block: 15_050_000,
                    latest_block_hash: [0u8; 32],
                }
            ),
            Err(EthChainConfigPeerValidationReasonV1::WrongGenesis)
        );
        assert_eq!(
            validate_eth_chain_config_peer_status_v1(
                &config,
                20_000_000,
                1_681_338_455,
                &EthRlpxStatusV1 {
                    protocol_version: 70,
                    network_id: 1,
                    genesis_hash: config.genesis_hash,
                    fork_id: EthForkIdV1 {
                        hash: [0xf0, 0xaf, 0xd0, 0xe3],
                        next: 0,
                    },
                    earliest_block: 0,
                    latest_block: 15_050_000,
                    latest_block_hash: [0u8; 32],
                }
            ),
            Err(EthChainConfigPeerValidationReasonV1::RemoteStaleForkId)
        );
        assert_eq!(
            validate_eth_chain_config_peer_status_v1(
                &config,
                15_050_000,
                0,
                &EthRlpxStatusV1 {
                    protocol_version: 70,
                    network_id: 1,
                    genesis_hash: config.genesis_hash,
                    fork_id: EthForkIdV1 {
                        hash: [0xde, 0xad, 0xbe, 0xef],
                        next: 16_600_000,
                    },
                    earliest_block: 0,
                    latest_block: 16_600_000,
                    latest_block_hash: [0u8; 32],
                }
            ),
            Err(EthChainConfigPeerValidationReasonV1::UnsupportedForkProgression)
        );
    }
}
