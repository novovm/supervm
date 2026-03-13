// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

#![forbid(unsafe_code)]
#![allow(clippy::large_enum_variant)]

use anyhow::{bail, Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use novovm_consensus::{
    BFTConfig, BFTEngine, BFTProposal, NodeId as CNodeId, Phase, ProtocolState, ValidatorSet, Vote,
};
use novovm_exec::AoemRuntimeConfig;
use novovm_network::{TcpTransport, Transport, UdpTransport};
use novovm_node::tx_ingress::{
    build_exec_batch_from_records, build_ops_wire_v1_from_records, load_tx_records_from_wire_file,
};
use novovm_protocol::{
    encode as protocol_encode, CheckpointId, FinalityMessage, NodeId as PNodeId, ProtocolMessage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum D1IngressMode {
    Auto,
    OpsWireV1,
    OpsV2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClusterTransportMode {
    Udp,
    Tcp,
}

enum ClusterTransport {
    Udp(UdpTransport),
    Tcp(TcpTransport),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ClusterWirePayload {
    Proposal {
        proposal: BFTProposal,
        leader_state: ProtocolState,
    },
    Vote(Vote),
}

#[derive(Debug, Clone, Serialize)]
struct ClusterNodeSummary {
    generated_at_utc: String,
    node_id: usize,
    leader_id: usize,
    role: String,
    transport: String,
    validators: usize,
    expected_batches: usize,
    processed_batches: usize,
    d1_ingress_mode: String,
    d1_codec: String,
    aoem_ingress_path: String,
    variant: String,
    consensus_tps_p50: f64,
    consensus_tps_p90: f64,
    consensus_tps_p99: f64,
    consensus_latency_ms_p50: f64,
    consensus_latency_ms_p90: f64,
    consensus_latency_ms_p99: f64,
    aoem_kernel_tps_p50: f64,
    aoem_kernel_tps_p90: f64,
    aoem_kernel_tps_p99: f64,
    runtime_total_ms: f64,
    network_message_count: u64,
    network_message_bytes: u64,
    process_exit_code: i32,
}

fn env_usize(name: &str, default: usize) -> Result<usize> {
    match std::env::var(name) {
        Ok(v) => {
            let parsed = v
                .trim()
                .parse::<usize>()
                .with_context(|| format!("invalid {name}={v}"))?;
            Ok(parsed)
        }
        Err(_) => Ok(default),
    }
}

fn env_u64(name: &str, default: u64) -> Result<u64> {
    match std::env::var(name) {
        Ok(v) => {
            let parsed = v
                .trim()
                .parse::<u64>()
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

fn transport_mode_env() -> Result<ClusterTransportMode> {
    let raw = std::env::var("NOVOVM_CLUSTER_TRANSPORT").unwrap_or_else(|_| "udp".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "udp" => Ok(ClusterTransportMode::Udp),
        "tcp" => Ok(ClusterTransportMode::Tcp),
        _ => bail!("invalid NOVOVM_CLUSTER_TRANSPORT={raw}; valid: udp|tcp"),
    }
}

fn pnode(id: CNodeId) -> PNodeId {
    PNodeId(id as u64)
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

fn deterministic_signing_key(node_id: usize) -> SigningKey {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-cluster-bench-signing-key-v1");
    hasher.update((node_id as u64).to_le_bytes());
    let seed: [u8; 32] = hasher.finalize().into();
    SigningKey::from_bytes(&seed)
}

fn parse_peers() -> Result<Vec<(usize, String)>> {
    let raw = env_string_nonempty("NOVOVM_CLUSTER_PEERS")
        .context("NOVOVM_CLUSTER_PEERS is required; format: id=host:port,id=host:port")?;
    let mut out = Vec::new();
    for part in raw.split(',') {
        let entry = part.trim();
        if entry.is_empty() {
            continue;
        }
        let mut it = entry.splitn(2, '=');
        let id_raw = it
            .next()
            .context("peer parse failed: missing id in NOVOVM_CLUSTER_PEERS")?;
        let addr = it
            .next()
            .context("peer parse failed: missing addr in NOVOVM_CLUSTER_PEERS")?
            .trim()
            .to_string();
        let id = id_raw
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid peer id in NOVOVM_CLUSTER_PEERS: {id_raw}"))?;
        out.push((id, addr));
    }
    if out.is_empty() {
        bail!("NOVOVM_CLUSTER_PEERS parsed empty");
    }
    Ok(out)
}

impl ClusterTransport {
    fn mode_name(&self) -> &'static str {
        match self {
            Self::Udp(_) => "udp",
            Self::Tcp(_) => "tcp",
        }
    }

    fn send(&self, to: usize, msg: ProtocolMessage) -> Result<()> {
        match self {
            Self::Udp(t) => t.send(pnode(to as CNodeId), msg).context("udp send failed"),
            Self::Tcp(t) => t.send(pnode(to as CNodeId), msg).context("tcp send failed"),
        }
    }

    fn try_recv(&self, me: usize) -> Result<Option<ProtocolMessage>> {
        match self {
            Self::Udp(t) => t.try_recv(pnode(me as CNodeId)).context("udp recv failed"),
            Self::Tcp(t) => t.try_recv(pnode(me as CNodeId)).context("tcp recv failed"),
        }
    }
}

fn main() -> Result<()> {
    let runtime_start = Instant::now();
    let node_id = env_usize("NOVOVM_CLUSTER_NODE_ID", usize::MAX)?;
    let validators = env_usize("NOVOVM_CLUSTER_VALIDATORS", 4)?.max(4);
    let leader_id = env_usize("NOVOVM_CLUSTER_LEADER_ID", 0)?;
    let batch_size = env_usize("NOVOVM_E2E_BATCH_SIZE", 1000)?.max(1);
    let expected_batches = env_usize("NOVOVM_CLUSTER_EXPECTED_BATCHES", 1)?.max(1);
    let max_batches = env_usize("NOVOVM_E2E_MAX_BATCHES", usize::MAX)?;
    let timeout_sec = env_u64("NOVOVM_CLUSTER_TIMEOUT_SEC", 1200)?;
    let summary_out = env_string_nonempty("NOVOVM_E2E_SUMMARY_OUT");
    let listen_addr = env_string_nonempty("NOVOVM_CLUSTER_LISTEN_ADDR")
        .context("NOVOVM_CLUSTER_LISTEN_ADDR is required")?;
    let peers = parse_peers()?;

    if node_id >= validators {
        bail!("NOVOVM_CLUSTER_NODE_ID out of range: node_id={node_id}, validators={validators}");
    }
    if leader_id >= validators {
        bail!(
            "NOVOVM_CLUSTER_LEADER_ID out of range: leader_id={leader_id}, validators={validators}"
        );
    }

    let transport = match transport_mode_env()? {
        ClusterTransportMode::Udp => {
            let t = UdpTransport::bind(pnode(node_id as CNodeId), &listen_addr)
                .with_context(|| format!("udp bind failed: {}", listen_addr))?;
            for (id, addr) in &peers {
                t.register_peer(pnode(*id as CNodeId), addr)
                    .with_context(|| format!("udp register_peer failed: id={id} addr={addr}"))?;
            }
            ClusterTransport::Udp(t)
        }
        ClusterTransportMode::Tcp => {
            let t = TcpTransport::bind(pnode(node_id as CNodeId), &listen_addr)
                .with_context(|| format!("tcp bind failed: {}", listen_addr))?;
            for (id, addr) in &peers {
                t.register_peer(pnode(*id as CNodeId), addr)
                    .with_context(|| format!("tcp register_peer failed: id={id} addr={addr}"))?;
            }
            ClusterTransport::Tcp(t)
        }
    };

    let validator_ids: Vec<CNodeId> = (0..validators as CNodeId).collect();
    let validator_set = ValidatorSet::new_equal_weight(validator_ids);
    let mut signing_keys = Vec::with_capacity(validators);
    for i in 0..validators {
        signing_keys.push(deterministic_signing_key(i));
    }
    let public_keys: HashMap<CNodeId, VerifyingKey> = signing_keys
        .iter()
        .enumerate()
        .map(|(i, sk)| (i as CNodeId, sk.verifying_key()))
        .collect();
    let config = BFTConfig::default();
    let mut engine = BFTEngine::new(
        config,
        node_id as CNodeId,
        signing_keys[node_id].clone(),
        validator_set,
        public_keys,
    )?;

    let mut latency_samples = Vec::new();
    let mut tps_samples = Vec::new();
    let mut kernel_tps_samples = Vec::new();
    let mut network_message_count = 0u64;
    let mut network_message_bytes = 0u64;
    let started = Instant::now();

    let requested_ingress_mode = ingress_mode_env()?;
    let mut variant = "-".to_string();
    let mut d1_codec = "-".to_string();
    let mut aoem_ingress_path = "-".to_string();
    let mut processed_batches = 0usize;

    if node_id == leader_id {
        let tx_wire_path = env_string_nonempty("NOVOVM_TX_WIRE_FILE")
            .context("leader requires NOVOVM_TX_WIRE_FILE")?;
        let txs_all = load_tx_records_from_wire_file(Path::new(&tx_wire_path))?;
        if txs_all.is_empty() {
            bail!("tx wire has zero txs");
        }

        let runtime = AoemRuntimeConfig::from_env()?;
        variant = runtime.variant.as_str().to_string();
        let facade = novovm_exec::AoemExecFacade::open_with_runtime(&runtime)?;
        let session = facade.create_session()?;
        let supports_wire_v1 = facade.supports_ops_wire_v1();
        let use_wire_v1 = match requested_ingress_mode {
            D1IngressMode::Auto => supports_wire_v1,
            D1IngressMode::OpsWireV1 => {
                if !supports_wire_v1 {
                    bail!("requested ops_wire_v1 but AOEM DLL does not export aoem_execute_ops_wire_v1");
                }
                true
            }
            D1IngressMode::OpsV2 => false,
        };
        d1_codec = if use_wire_v1 {
            "local_tx_wire_v1_write_u64le_v1".to_string()
        } else {
            "-".to_string()
        };
        aoem_ingress_path = if use_wire_v1 {
            "ops_wire_v1".to_string()
        } else if requested_ingress_mode == D1IngressMode::Auto && !supports_wire_v1 {
            "ops_v2_fallback".to_string()
        } else {
            "ops_v2_forced".to_string()
        };

        for (batch_index, chunk) in txs_all
            .chunks(batch_size)
            .take(expected_batches.min(max_batches))
            .enumerate()
        {
            if started.elapsed() > Duration::from_secs(timeout_sec) {
                bail!("leader timeout: exceeded {} sec", timeout_sec);
            }
            let round_start = Instant::now();
            let batch_id = (batch_index + 1) as u64;

            engine.start_epoch()?;
            engine.add_batch(batch_id, chunk.len() as u64)?;

            let exec_report = if use_wire_v1 {
                let payload = build_ops_wire_v1_from_records(chunk, |i, _| {
                    (batch_id << 32).saturating_add(i as u64 + 1)
                });
                session.submit_ops_wire_report(&payload.bytes)
            } else {
                let payload = build_exec_batch_from_records(chunk, |i, _| {
                    (batch_id << 32).saturating_add(i as u64 + 1)
                });
                session.submit_ops_report(&payload.ops)
            };
            if !exec_report.ok {
                let msg = exec_report
                    .error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "unknown aoem error".to_string());
                bail!("leader aoem submit failed on batch {batch_id}: {msg}");
            }
            let exec_out = exec_report
                .output
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("leader missing exec output"))?;
            if exec_out.metrics.elapsed_us > 0 {
                let ktps = exec_out.metrics.processed_ops as f64 * 1_000_000.0
                    / exec_out.metrics.elapsed_us as f64;
                kernel_tps_samples.push(ktps);
            }

            let mut batch_results = HashMap::new();
            batch_results.insert(batch_id, [0u8; 32]);
            let proposal = engine.propose_epoch_with_state_root(&batch_results, [0u8; 32])?;
            let leader_state = engine.protocol_state_snapshot();
            let payload = bincode::serialize(&ClusterWirePayload::Proposal {
                proposal: proposal.clone(),
                leader_state,
            })?;
            for peer in 0..validators {
                if peer == node_id {
                    continue;
                }
                let msg = ProtocolMessage::Finality(FinalityMessage::CheckpointPropose {
                    id: CheckpointId(batch_id),
                    from: pnode(node_id as CNodeId),
                    payload: payload.clone(),
                });
                let encoded = protocol_encode(&msg).context("encode proposal failed")?;
                network_message_count = network_message_count.saturating_add(1);
                network_message_bytes = network_message_bytes.saturating_add(encoded.len() as u64);
                transport.send(peer, msg)?;
            }

            let mut qc_opt = engine.collect_vote(engine.vote_for_proposal(&proposal)?)?;
            while qc_opt.is_none() {
                if started.elapsed() > Duration::from_secs(timeout_sec) {
                    bail!("leader timeout while waiting votes on batch {}", batch_id);
                }
                let Some(msg) = transport.try_recv(node_id)? else {
                    std::thread::yield_now();
                    continue;
                };
                let ProtocolMessage::Finality(FinalityMessage::Vote { id, sig, .. }) = msg else {
                    continue;
                };
                if id.0 != batch_id {
                    continue;
                }
                let decoded: ClusterWirePayload =
                    bincode::deserialize(&sig).context("decode vote payload failed")?;
                let ClusterWirePayload::Vote(vote) = decoded else {
                    continue;
                };
                if let Some(qc) = engine.collect_vote(vote)? {
                    qc_opt = Some(qc);
                }
            }

            let qc = qc_opt.ok_or_else(|| anyhow::anyhow!("qc missing for batch {}", batch_id))?;
            engine.commit_qc(qc)?;
            let mut synced_state = engine.protocol_state_snapshot();
            synced_state.leader_id = leader_id as CNodeId;
            synced_state.view = 0;
            synced_state.phase = Phase::Propose;
            synced_state.active_proposal = None;
            synced_state.votes.clear();
            engine.sync_protocol_state(synced_state);

            let latency_ms = round_start.elapsed().as_secs_f64() * 1000.0;
            let tps = if latency_ms > 0.0 {
                (chunk.len() as f64) * 1000.0 / latency_ms
            } else {
                0.0
            };
            latency_samples.push(latency_ms);
            tps_samples.push(tps);
            processed_batches += 1;
        }

        drop(session);
        std::mem::forget(facade);
    } else {
        while processed_batches < expected_batches.min(max_batches) {
            if started.elapsed() > Duration::from_secs(timeout_sec) {
                bail!(
                    "follower {} timeout waiting proposals ({}/{})",
                    node_id,
                    processed_batches,
                    expected_batches
                );
            }
            let Some(msg) = transport.try_recv(node_id)? else {
                std::thread::yield_now();
                continue;
            };
            let ProtocolMessage::Finality(FinalityMessage::CheckpointPropose {
                id, payload, ..
            }) = msg
            else {
                continue;
            };
            let decoded: ClusterWirePayload =
                bincode::deserialize(&payload).context("decode proposal payload failed")?;
            let ClusterWirePayload::Proposal {
                proposal,
                leader_state,
            } = decoded
            else {
                continue;
            };
            engine.sync_protocol_state(leader_state);
            let vote = engine.vote_for_proposal(&proposal)?;
            let sig = bincode::serialize(&ClusterWirePayload::Vote(vote.clone()))
                .context("encode follower vote payload failed")?;
            let msg = ProtocolMessage::Finality(FinalityMessage::Vote {
                id,
                from: pnode(node_id as CNodeId),
                sig,
            });
            let encoded =
                protocol_encode(&msg).context("encode follower vote protocol msg failed")?;
            network_message_count = network_message_count.saturating_add(1);
            network_message_bytes = network_message_bytes.saturating_add(encoded.len() as u64);
            transport.send(leader_id, msg)?;
            processed_batches += 1;
        }
    }

    let runtime_total_ms = runtime_start.elapsed().as_secs_f64() * 1000.0;
    let summary = ClusterNodeSummary {
        generated_at_utc: format!(
            "{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ),
        node_id,
        leader_id,
        role: if node_id == leader_id {
            "leader".to_string()
        } else {
            "follower".to_string()
        },
        transport: transport.mode_name().to_string(),
        validators,
        expected_batches,
        processed_batches,
        d1_ingress_mode: match requested_ingress_mode {
            D1IngressMode::Auto => "auto".to_string(),
            D1IngressMode::OpsWireV1 => "ops_wire_v1".to_string(),
            D1IngressMode::OpsV2 => "ops_v2".to_string(),
        },
        d1_codec,
        aoem_ingress_path,
        variant,
        consensus_tps_p50: quantile(&tps_samples, 0.50),
        consensus_tps_p90: quantile(&tps_samples, 0.90),
        consensus_tps_p99: quantile(&tps_samples, 0.99),
        consensus_latency_ms_p50: quantile(&latency_samples, 0.50),
        consensus_latency_ms_p90: quantile(&latency_samples, 0.90),
        consensus_latency_ms_p99: quantile(&latency_samples, 0.99),
        aoem_kernel_tps_p50: quantile(&kernel_tps_samples, 0.50),
        aoem_kernel_tps_p90: quantile(&kernel_tps_samples, 0.90),
        aoem_kernel_tps_p99: quantile(&kernel_tps_samples, 0.99),
        runtime_total_ms: (runtime_total_ms * 100.0).round() / 100.0,
        network_message_count,
        network_message_bytes,
        process_exit_code: 0,
    };

    let text = serde_json::to_string_pretty(&summary)?;
    if let Some(path) = summary_out {
        fs::write(&path, &text).with_context(|| format!("write summary failed: {path}"))?;
    }
    println!("{text}");
    Ok(())
}
