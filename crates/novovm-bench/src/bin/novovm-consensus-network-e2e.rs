// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop, clippy::vec_init_then_push)]

#[path = "../bincode_compat.rs"]
mod bincode_compat;

use anyhow::{bail, Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use novovm_consensus::{BFTConfig, BFTEngine, BFTProposal, NodeId as CNodeId, ValidatorSet, Vote};
use novovm_exec::AoemRuntimeConfig;
use novovm_network::{InMemoryTransport, Transport, UdpTransport};
use novovm_node::tx_ingress::{
    build_exec_batch_from_records, build_ops_wire_v1_from_records, load_tx_records_from_wire_file,
    TxIngressRecord,
};
use novovm_protocol::{
    encode as protocol_encode, CheckpointId, FinalityMessage, NodeId as PNodeId, ProtocolMessage,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
enum BenchFinalityPayload {
    Proposal(BFTProposal),
    Vote(Vote),
}

#[derive(Debug, Clone, Serialize)]
struct E2eSummary {
    generated_at_utc: String,
    variant: String,
    d1_ingress_mode: String,
    d1_input_source: String,
    d1_codec: String,
    aoem_ingress_path: String,
    network_transport: String,
    repeat_count: usize,
    warm_excludes_first_repeat: bool,
    txs_total_per_repeat: usize,
    txs_total_all_repeats: usize,
    validators: usize,
    txs_total: usize,
    batches: usize,
    batch_size: usize,
    consensus_network_e2e_tps_p50: f64,
    consensus_network_e2e_tps_p90: f64,
    consensus_network_e2e_tps_p99: f64,
    consensus_network_e2e_latency_ms_p50: f64,
    consensus_network_e2e_latency_ms_p90: f64,
    consensus_network_e2e_latency_ms_p99: f64,
    aoem_kernel_tps_p50: f64,
    aoem_kernel_tps_p90: f64,
    aoem_kernel_tps_p99: f64,
    network_message_count: u64,
    network_message_bytes: u64,
    repeat_wall_tps_p50: f64,
    repeat_wall_tps_p90: f64,
    repeat_wall_tps_p99: f64,
    repeat_loop_ms_p50: f64,
    repeat_loop_ms_p90: f64,
    repeat_loop_ms_p99: f64,
    warm_wall_tps_p50: f64,
    warm_wall_tps_p90: f64,
    warm_wall_tps_p99: f64,
    warm_loop_ms_p50: f64,
    warm_loop_ms_p90: f64,
    warm_loop_ms_p99: f64,
    runtime_total_ms: f64,
    tx_wire_load_ms: f64,
    setup_ms: f64,
    loop_total_ms: f64,
    stage_batch_admission_ms: f64,
    stage_ingress_pack_ms: f64,
    stage_aoem_submit_ms: f64,
    stage_proposal_build_ms: f64,
    stage_proposal_broadcast_ms: f64,
    stage_state_sync_ms: f64,
    stage_follower_vote_ms: f64,
    stage_qc_collect_ms: f64,
    stage_commit_resync_ms: f64,
    stage_other_ms: f64,
    qc_poll_iters_total: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum D1IngressMode {
    Auto,
    OpsWireV1,
    OpsV2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NetworkTransportMode {
    InMemory,
    UdpLoopback,
}

enum NetworkHarness {
    InMemory(InMemoryTransport),
    UdpLoopback(Vec<UdpTransport>),
}

fn env_usize(name: &str, default: usize) -> Result<usize> {
    match std::env::var(name) {
        Ok(v) => {
            let parsed: usize = v
                .trim()
                .parse()
                .with_context(|| format!("invalid {name}={v}"))?;
            Ok(parsed)
        }
        Err(_) => Ok(default),
    }
}

fn env_string_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|v| {
        let t = v.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

fn ingress_mode_env() -> Result<D1IngressMode> {
    let raw = std::env::var("NOVOVM_D1_INGRESS_MODE").unwrap_or_else(|_| "auto".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(D1IngressMode::Auto),
        "ops_wire_v1" | "wire_v1" => Ok(D1IngressMode::OpsWireV1),
        "ops_v2" | "v2" => Ok(D1IngressMode::OpsV2),
        _ => bail!("invalid NOVOVM_D1_INGRESS_MODE={raw}; valid: auto|ops_wire_v1|ops_v2"),
    }
}

fn network_transport_env() -> Result<NetworkTransportMode> {
    let raw =
        std::env::var("NOVOVM_E2E_NETWORK_TRANSPORT").unwrap_or_else(|_| "inmemory".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "inmemory" | "memory" => Ok(NetworkTransportMode::InMemory),
        "udp_loopback" | "udp" => Ok(NetworkTransportMode::UdpLoopback),
        _ => bail!("invalid NOVOVM_E2E_NETWORK_TRANSPORT={raw}; valid: inmemory|udp_loopback"),
    }
}

fn pnode(id: CNodeId) -> PNodeId {
    PNodeId(id as u64)
}

impl NetworkHarness {
    fn mode_name(&self) -> &'static str {
        match self {
            Self::InMemory(_) => "inmemory",
            Self::UdpLoopback(_) => "udp_loopback",
        }
    }

    fn send(&self, from_idx: usize, to_idx: usize, msg: ProtocolMessage) -> Result<()> {
        match self {
            Self::InMemory(t) => t
                .send(pnode(to_idx as CNodeId), msg)
                .context("inmemory send failed"),
            Self::UdpLoopback(transports) => transports[from_idx]
                .send(pnode(to_idx as CNodeId), msg)
                .context("udp loopback send failed"),
        }
    }

    fn try_recv(&self, node_idx: usize) -> Result<Option<ProtocolMessage>> {
        match self {
            Self::InMemory(t) => t
                .try_recv(pnode(node_idx as CNodeId))
                .context("inmemory recv failed"),
            Self::UdpLoopback(transports) => transports[node_idx]
                .try_recv(pnode(node_idx as CNodeId))
                .context("udp loopback recv failed"),
        }
    }
}

fn quantile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let mut rank = (q * n as f64).ceil() as usize;
    if rank == 0 {
        rank = 1;
    }
    if rank > n {
        rank = n;
    }
    (sorted[rank - 1] * 100.0).round() / 100.0
}

fn batch_digest(round: usize, txs: &[TxIngressRecord]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update((round as u64).to_le_bytes());
    for tx in txs {
        hasher.update(tx.key.to_le_bytes());
        hasher.update(tx.value.to_le_bytes());
    }
    hasher.finalize().into()
}

fn main() -> Result<()> {
    let runtime_start = Instant::now();

    let load_start = Instant::now();
    let tx_wire_path = env_string_nonempty("NOVOVM_TX_WIRE_FILE")
        .context("NOVOVM_TX_WIRE_FILE is required for consensus network e2e")?;
    let txs_all = load_tx_records_from_wire_file(Path::new(&tx_wire_path))?;
    if txs_all.is_empty() {
        bail!("tx wire has zero txs");
    }
    let tx_wire_load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    let batch_size = env_usize("NOVOVM_E2E_BATCH_SIZE", 1000)?.max(1);
    let validators = env_usize("NOVOVM_E2E_VALIDATORS", 4)?.max(4);
    let max_batches = env_usize("NOVOVM_E2E_MAX_BATCHES", usize::MAX)?;
    let summary_out = env_string_nonempty("NOVOVM_E2E_SUMMARY_OUT");

    let setup_start = Instant::now();
    let runtime = AoemRuntimeConfig::from_env()?;
    let facade = novovm_exec::AoemExecFacade::open_with_runtime(&runtime)?;
    let session = facade.create_session()?;
    let requested_ingress_mode = ingress_mode_env()?;
    let supports_wire_v1 = facade.supports_ops_wire_v1();
    let use_wire_v1 = match requested_ingress_mode {
        D1IngressMode::Auto => supports_wire_v1,
        D1IngressMode::OpsWireV1 => {
            if !supports_wire_v1 {
                bail!("NOVOVM_D1_INGRESS_MODE=ops_wire_v1 requested, but loaded AOEM does not export aoem_execute_ops_wire_v1");
            }
            true
        }
        D1IngressMode::OpsV2 => false,
    };
    let selected_ingress_mode = if use_wire_v1 { "ops_wire_v1" } else { "ops_v2" };
    let input_source = "tx_wire".to_string();
    let d1_codec = if use_wire_v1 {
        "local_tx_wire_v1_write_u64le_v1".to_string()
    } else {
        "-".to_string()
    };
    let aoem_ingress_path = if use_wire_v1 {
        "ops_wire_v1".to_string()
    } else if requested_ingress_mode == D1IngressMode::Auto && !supports_wire_v1 {
        "ops_v2_fallback".to_string()
    } else {
        "ops_v2_forced".to_string()
    };

    let validator_ids: Vec<CNodeId> = (0..validators as CNodeId).collect();
    let validator_set = ValidatorSet::new_equal_weight(validator_ids.clone());

    let mut signing_keys = Vec::with_capacity(validators);
    for _ in 0..validators {
        signing_keys.push(SigningKey::generate(&mut OsRng));
    }
    let public_keys: HashMap<CNodeId, VerifyingKey> = signing_keys
        .iter()
        .enumerate()
        .map(|(i, sk)| (i as CNodeId, sk.verifying_key()))
        .collect();

    let config = BFTConfig::default();
    let mut engines = Vec::with_capacity(validators);
    for i in 0..validators {
        engines.push(BFTEngine::new(
            config.clone(),
            i as CNodeId,
            signing_keys[i].clone(),
            validator_set.clone(),
            public_keys.clone(),
        )?);
    }

    let network_transport_mode = network_transport_env()?;
    let transport = match network_transport_mode {
        NetworkTransportMode::InMemory => {
            let t = InMemoryTransport::new(1_000_000);
            for id in &validator_ids {
                t.register(pnode(*id));
            }
            NetworkHarness::InMemory(t)
        }
        NetworkTransportMode::UdpLoopback => {
            let mut transports = Vec::with_capacity(validators);
            for i in 0..validators {
                let t = UdpTransport::bind(pnode(i as CNodeId), "127.0.0.1:0")
                    .with_context(|| format!("udp bind failed for node={i}"))?;
                transports.push(t);
            }
            let mut addrs = Vec::with_capacity(validators);
            for i in 0..validators {
                let addr = transports[i]
                    .local_addr()
                    .with_context(|| format!("udp local_addr failed for node={i}"))?;
                addrs.push(addr.to_string());
            }
            for i in 0..validators {
                for j in 0..validators {
                    if i == j {
                        continue;
                    }
                    transports[i]
                        .register_peer(pnode(j as CNodeId), &addrs[j])
                        .with_context(|| {
                            format!("udp register_peer failed from={i} to={j} addr={}", addrs[j])
                        })?;
                }
            }
            NetworkHarness::UdpLoopback(transports)
        }
    };
    let setup_ms = setup_start.elapsed().as_secs_f64() * 1000.0;

    let mut latency_ms_samples = Vec::new();
    let mut e2e_tps_samples = Vec::new();
    let mut kernel_tps_samples = Vec::new();
    let mut network_message_count: u64 = 0;
    let mut network_message_bytes: u64 = 0;
    let mut qc_poll_iters_total: u64 = 0;

    let loop_start = Instant::now();
    let mut stage_batch_admission_ms = 0.0f64;
    let mut stage_ingress_pack_ms = 0.0f64;
    let mut stage_aoem_submit_ms = 0.0f64;
    let mut stage_proposal_build_ms = 0.0f64;
    let mut stage_proposal_broadcast_ms = 0.0f64;
    let mut stage_state_sync_ms = 0.0f64;
    let mut stage_follower_vote_ms = 0.0f64;
    let mut stage_qc_collect_ms = 0.0f64;
    let mut stage_commit_resync_ms = 0.0f64;

    let mut batch_index = 0usize;
    for chunk in txs_all.chunks(batch_size) {
        if batch_index >= max_batches {
            break;
        }
        let round_start = Instant::now();

        let leader_idx = 0usize;
        let leader_id = leader_idx as CNodeId;

        let stage_start = Instant::now();
        engines[leader_idx].start_epoch()?;
        let batch_id = (batch_index + 1) as u64;
        engines[leader_idx].add_batch(batch_id, chunk.len() as u64)?;
        stage_batch_admission_ms += stage_start.elapsed().as_secs_f64() * 1000.0;

        let exec_report = if use_wire_v1 {
            let pack_start = Instant::now();
            let payload = build_ops_wire_v1_from_records(chunk, |i, _| {
                (batch_id << 32).saturating_add(i as u64 + 1)
            });
            stage_ingress_pack_ms += pack_start.elapsed().as_secs_f64() * 1000.0;
            let submit_start = Instant::now();
            let report = session.submit_ops_wire_report(&payload.bytes);
            stage_aoem_submit_ms += submit_start.elapsed().as_secs_f64() * 1000.0;
            report
        } else {
            let pack_start = Instant::now();
            let exec_batch = build_exec_batch_from_records(chunk, |i, _| {
                (batch_id << 32).saturating_add(i as u64 + 1)
            });
            stage_ingress_pack_ms += pack_start.elapsed().as_secs_f64() * 1000.0;
            let submit_start = Instant::now();
            let report = session.submit_ops_report(&exec_batch.ops);
            stage_aoem_submit_ms += submit_start.elapsed().as_secs_f64() * 1000.0;
            report
        };
        if !exec_report.ok {
            let msg = exec_report
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "unknown aoem error".to_string());
            bail!("aoem submit_ops_report failed on batch {batch_id}: {msg}");
        }
        let exec_out = exec_report
            .output
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing exec output on batch {batch_id}"))?;
        if exec_out.metrics.elapsed_us > 0 {
            let ktps = exec_out.metrics.processed_ops as f64 * 1_000_000.0
                / exec_out.metrics.elapsed_us as f64;
            kernel_tps_samples.push(ktps);
        }

        let stage_start = Instant::now();
        let mut batch_results = HashMap::new();
        batch_results.insert(batch_id, batch_digest(batch_index, chunk));
        let override_root = batch_digest(batch_index + 1024, chunk);
        let proposal =
            engines[leader_idx].propose_epoch_with_state_root(&batch_results, override_root)?;
        stage_proposal_build_ms += stage_start.elapsed().as_secs_f64() * 1000.0;

        let stage_start = Instant::now();
        let proposal_payload =
            crate::bincode_compat::serialize(&BenchFinalityPayload::Proposal(proposal.clone()))
                .context("serialize proposal payload failed")?;
        for i in 0..validators {
            if i == leader_idx {
                continue;
            }
            let msg = ProtocolMessage::Finality(FinalityMessage::CheckpointPropose {
                id: CheckpointId(batch_id),
                from: pnode(leader_id as CNodeId),
                payload: proposal_payload.clone(),
            });
            let encoded = protocol_encode(&msg).context("encode proposal protocol msg failed")?;
            network_message_count = network_message_count.saturating_add(1);
            network_message_bytes = network_message_bytes.saturating_add(encoded.len() as u64);
            transport
                .send(leader_idx, i, msg)
                .context("transport send proposal failed")?;
        }
        stage_proposal_broadcast_ms += stage_start.elapsed().as_secs_f64() * 1000.0;

        let stage_start = Instant::now();
        let leader_state = engines[leader_idx].protocol_state_snapshot();
        for i in 0..validators {
            if i == leader_idx {
                continue;
            }
            engines[i].sync_protocol_state(leader_state.clone());
        }
        stage_state_sync_ms += stage_start.elapsed().as_secs_f64() * 1000.0;

        let stage_start = Instant::now();
        let mut pending_votes = Vec::new();
        pending_votes.push(engines[leader_idx].vote_for_proposal(&proposal)?);
        for i in 0..validators {
            if i == leader_idx {
                continue;
            }
            let recv = transport
                .try_recv(i)
                .context("transport recv proposal failed")?;
            let Some(ProtocolMessage::Finality(FinalityMessage::CheckpointPropose {
                payload, ..
            })) = recv
            else {
                bail!("follower {i} did not receive proposal message");
            };
            let decoded: BenchFinalityPayload = crate::bincode_compat::deserialize(&payload)
                .context("deserialize proposal payload failed")?;
            let BenchFinalityPayload::Proposal(follower_proposal) = decoded else {
                bail!("follower {i} received non-proposal payload");
            };
            let vote = engines[i].vote_for_proposal(&follower_proposal)?;
            let payload =
                crate::bincode_compat::serialize(&BenchFinalityPayload::Vote(vote.clone()))
                    .context("serialize vote payload failed")?;
            let msg = ProtocolMessage::Finality(FinalityMessage::Vote {
                id: CheckpointId(batch_id),
                from: pnode(i as CNodeId),
                sig: payload,
            });
            let encoded = protocol_encode(&msg).context("encode vote protocol msg failed")?;
            network_message_count = network_message_count.saturating_add(1);
            network_message_bytes = network_message_bytes.saturating_add(encoded.len() as u64);
            transport
                .send(i, leader_id as usize, msg)
                .context("transport send vote failed")?;
        }
        stage_follower_vote_ms += stage_start.elapsed().as_secs_f64() * 1000.0;

        let stage_start = Instant::now();
        let mut qc_opt = None;
        for vote in pending_votes {
            if let Some(qc) = engines[leader_idx].collect_vote(vote)? {
                qc_opt = Some(qc);
                break;
            }
        }
        let mut poll_iters: u64 = 0;
        while qc_opt.is_none() {
            poll_iters = poll_iters.saturating_add(1);
            if poll_iters > 2_000_000 {
                bail!("leader vote collection timed out on batch {batch_id}");
            }
            let recv = transport
                .try_recv(leader_id as usize)
                .context("transport recv vote failed")?;
            let Some(ProtocolMessage::Finality(FinalityMessage::Vote { id, sig, .. })) = recv
            else {
                std::thread::yield_now();
                continue;
            };
            if id.0 != batch_id {
                // stale vote from previous round, drop.
                continue;
            }
            let decoded: BenchFinalityPayload = crate::bincode_compat::deserialize(&sig)
                .context("deserialize vote payload failed")?;
            let BenchFinalityPayload::Vote(v) = decoded else {
                bail!("leader received non-vote payload");
            };
            if let Some(qc) = engines[leader_idx].collect_vote(v)? {
                qc_opt = Some(qc);
            }
        }
        qc_poll_iters_total = qc_poll_iters_total.saturating_add(poll_iters);
        stage_qc_collect_ms += stage_start.elapsed().as_secs_f64() * 1000.0;

        let stage_start = Instant::now();
        let qc = qc_opt.ok_or_else(|| anyhow::anyhow!("qc not formed on batch {batch_id}"))?;
        engines[leader_idx].commit_qc(qc)?;

        let mut synced_state = engines[leader_idx].protocol_state_snapshot();
        synced_state.leader_id = 0;
        synced_state.view = 0;
        synced_state.phase = novovm_consensus::Phase::Propose;
        synced_state.active_proposal = None;
        synced_state.votes.clear();
        for i in 0..validators {
            if i == leader_idx {
                continue;
            }
            engines[i].sync_protocol_state(synced_state.clone());
        }
        engines[leader_idx].sync_protocol_state(synced_state);
        stage_commit_resync_ms += stage_start.elapsed().as_secs_f64() * 1000.0;

        let latency_ms = round_start.elapsed().as_secs_f64() * 1000.0;
        let tps = if latency_ms > 0.0 {
            (chunk.len() as f64) * 1000.0 / latency_ms
        } else {
            0.0
        };
        latency_ms_samples.push(latency_ms);
        e2e_tps_samples.push(tps);

        batch_index += 1;
    }

    if batch_index == 0 {
        bail!("no batch executed (check tx wire and batch size)");
    }

    let loop_total_ms = loop_start.elapsed().as_secs_f64() * 1000.0;
    let runtime_total_ms = runtime_start.elapsed().as_secs_f64() * 1000.0;
    let measured_stage_ms = stage_batch_admission_ms
        + stage_ingress_pack_ms
        + stage_aoem_submit_ms
        + stage_proposal_build_ms
        + stage_proposal_broadcast_ms
        + stage_state_sync_ms
        + stage_follower_vote_ms
        + stage_qc_collect_ms
        + stage_commit_resync_ms;
    let stage_other_ms = (loop_total_ms - measured_stage_ms).max(0.0);

    let summary = E2eSummary {
        generated_at_utc: format!(
            "{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ),
        variant: runtime.variant.as_str().to_string(),
        d1_ingress_mode: selected_ingress_mode.to_string(),
        d1_input_source: input_source,
        d1_codec,
        aoem_ingress_path,
        network_transport: transport.mode_name().to_string(),
        repeat_count: 1,
        warm_excludes_first_repeat: false,
        txs_total_per_repeat: txs_all.len(),
        txs_total_all_repeats: txs_all.len(),
        validators,
        txs_total: txs_all.len(),
        batches: batch_index,
        batch_size,
        consensus_network_e2e_tps_p50: quantile(&e2e_tps_samples, 0.50),
        consensus_network_e2e_tps_p90: quantile(&e2e_tps_samples, 0.90),
        consensus_network_e2e_tps_p99: quantile(&e2e_tps_samples, 0.99),
        consensus_network_e2e_latency_ms_p50: quantile(&latency_ms_samples, 0.50),
        consensus_network_e2e_latency_ms_p90: quantile(&latency_ms_samples, 0.90),
        consensus_network_e2e_latency_ms_p99: quantile(&latency_ms_samples, 0.99),
        aoem_kernel_tps_p50: quantile(&kernel_tps_samples, 0.50),
        aoem_kernel_tps_p90: quantile(&kernel_tps_samples, 0.90),
        aoem_kernel_tps_p99: quantile(&kernel_tps_samples, 0.99),
        network_message_count,
        network_message_bytes,
        repeat_wall_tps_p50: quantile(&e2e_tps_samples, 0.50),
        repeat_wall_tps_p90: quantile(&e2e_tps_samples, 0.90),
        repeat_wall_tps_p99: quantile(&e2e_tps_samples, 0.99),
        repeat_loop_ms_p50: (loop_total_ms * 100.0).round() / 100.0,
        repeat_loop_ms_p90: (loop_total_ms * 100.0).round() / 100.0,
        repeat_loop_ms_p99: (loop_total_ms * 100.0).round() / 100.0,
        warm_wall_tps_p50: quantile(&e2e_tps_samples, 0.50),
        warm_wall_tps_p90: quantile(&e2e_tps_samples, 0.90),
        warm_wall_tps_p99: quantile(&e2e_tps_samples, 0.99),
        warm_loop_ms_p50: (loop_total_ms * 100.0).round() / 100.0,
        warm_loop_ms_p90: (loop_total_ms * 100.0).round() / 100.0,
        warm_loop_ms_p99: (loop_total_ms * 100.0).round() / 100.0,
        runtime_total_ms: (runtime_total_ms * 100.0).round() / 100.0,
        tx_wire_load_ms: (tx_wire_load_ms * 100.0).round() / 100.0,
        setup_ms: (setup_ms * 100.0).round() / 100.0,
        loop_total_ms: (loop_total_ms * 100.0).round() / 100.0,
        stage_batch_admission_ms: (stage_batch_admission_ms * 100.0).round() / 100.0,
        stage_ingress_pack_ms: (stage_ingress_pack_ms * 100.0).round() / 100.0,
        stage_aoem_submit_ms: (stage_aoem_submit_ms * 100.0).round() / 100.0,
        stage_proposal_build_ms: (stage_proposal_build_ms * 100.0).round() / 100.0,
        stage_proposal_broadcast_ms: (stage_proposal_broadcast_ms * 100.0).round() / 100.0,
        stage_state_sync_ms: (stage_state_sync_ms * 100.0).round() / 100.0,
        stage_follower_vote_ms: (stage_follower_vote_ms * 100.0).round() / 100.0,
        stage_qc_collect_ms: (stage_qc_collect_ms * 100.0).round() / 100.0,
        stage_commit_resync_ms: (stage_commit_resync_ms * 100.0).round() / 100.0,
        stage_other_ms: (stage_other_ms * 100.0).round() / 100.0,
        qc_poll_iters_total,
    };

    let text = serde_json::to_string_pretty(&summary)?;
    if let Some(path) = summary_out {
        fs::write(&path, &text).with_context(|| format!("write summary failed: {path}"))?;
    }
    println!("{text}");

    Ok(())
}
