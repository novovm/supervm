// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use novovm_consensus::{BFTConfig, BFTEngine, BFTProposal, NodeId as CNodeId, ValidatorSet, Vote};
use novovm_exec::AoemRuntimeConfig;
use novovm_network::{InMemoryTransport, Transport};
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
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::Instant;

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum D1IngressMode {
    Auto,
    OpsWireV1,
    OpsV2,
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
    let tx_wire_path = env_string_nonempty("NOVOVM_TX_WIRE_FILE")
        .context("NOVOVM_TX_WIRE_FILE is required for consensus network e2e")?;
    let txs_all = load_tx_records_from_wire_file(Path::new(&tx_wire_path))?;
    if txs_all.is_empty() {
        bail!("tx wire has zero txs");
    }

    let batch_size = env_usize("NOVOVM_E2E_BATCH_SIZE", 1000)?.max(1);
    let validators = env_usize("NOVOVM_E2E_VALIDATORS", 4)?.max(4);
    let max_batches = env_usize("NOVOVM_E2E_MAX_BATCHES", usize::MAX)?;
    let summary_out = env_string_nonempty("NOVOVM_E2E_SUMMARY_OUT");

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

    let transport = InMemoryTransport::new(1_000_000);
    for id in &validator_ids {
        transport.register(pnode(*id));
    }

    let mut latency_ms_samples = Vec::new();
    let mut e2e_tps_samples = Vec::new();
    let mut kernel_tps_samples = Vec::new();
    let mut network_message_count: u64 = 0;
    let mut network_message_bytes: u64 = 0;

    let mut batch_index = 0usize;
    for chunk in txs_all.chunks(batch_size) {
        if batch_index >= max_batches {
            break;
        }
        let round_start = Instant::now();

        let leader_idx = 0usize;
        let leader_id = leader_idx as CNodeId;

        engines[leader_idx].start_epoch()?;
        let batch_id = (batch_index + 1) as u64;
        engines[leader_idx].add_batch(batch_id, chunk.len() as u64)?;

        let exec_report = if use_wire_v1 {
            let payload = build_ops_wire_v1_from_records(chunk, |i, _| {
                (batch_id << 32).saturating_add(i as u64 + 1)
            });
            session.submit_ops_wire_report(&payload.bytes)
        } else {
            let exec_batch = build_exec_batch_from_records(chunk, |i, _| {
                (batch_id << 32).saturating_add(i as u64 + 1)
            });
            session.submit_ops_report(&exec_batch.ops)
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

        let mut batch_results = HashMap::new();
        batch_results.insert(batch_id, batch_digest(batch_index, chunk));
        let override_root = batch_digest(batch_index + 1024, chunk);
        let proposal =
            engines[leader_idx].propose_epoch_with_state_root(&batch_results, override_root)?;

        let proposal_payload = bincode::serialize(&BenchFinalityPayload::Proposal(proposal.clone()))
            .context("serialize proposal payload failed")?;
        for i in 0..validators {
            if i == leader_idx {
                continue;
            }
            let msg = ProtocolMessage::Finality(FinalityMessage::CheckpointPropose {
                id: CheckpointId(batch_id),
                payload: proposal_payload.clone(),
            });
            let encoded = protocol_encode(&msg).context("encode proposal protocol msg failed")?;
            network_message_count = network_message_count.saturating_add(1);
            network_message_bytes = network_message_bytes.saturating_add(encoded.len() as u64);
            transport
                .send(pnode(i as CNodeId), msg)
                .context("transport send proposal failed")?;
        }

        let leader_state = engines[leader_idx].protocol_state_snapshot();
        for i in 0..validators {
            if i == leader_idx {
                continue;
            }
            engines[i].sync_protocol_state(leader_state.clone());
        }

        let mut pending_votes = Vec::new();
        pending_votes.push(engines[leader_idx].vote_for_proposal(&proposal)?);
        for i in 0..validators {
            if i == leader_idx {
                continue;
            }
            let recv = transport
                .try_recv(pnode(i as CNodeId))
                .context("transport recv proposal failed")?;
            let Some(ProtocolMessage::Finality(FinalityMessage::CheckpointPropose { payload, .. })) = recv else {
                bail!("follower {i} did not receive proposal message");
            };
            let decoded: BenchFinalityPayload =
                bincode::deserialize(&payload).context("deserialize proposal payload failed")?;
            let BenchFinalityPayload::Proposal(follower_proposal) = decoded else {
                bail!("follower {i} received non-proposal payload");
            };
            let vote = engines[i].vote_for_proposal(&follower_proposal)?;
            let payload = bincode::serialize(&BenchFinalityPayload::Vote(vote.clone()))
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
                .send(pnode(leader_id), msg)
                .context("transport send vote failed")?;
        }

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
                .try_recv(pnode(leader_id))
                .context("transport recv vote failed")?;
            let Some(ProtocolMessage::Finality(FinalityMessage::Vote { id, sig, .. })) = recv else {
                std::thread::yield_now();
                continue;
            };
            if id.0 != batch_id {
                // stale vote from previous round, drop.
                continue;
            }
            let decoded: BenchFinalityPayload =
                bincode::deserialize(&sig).context("deserialize vote payload failed")?;
            let BenchFinalityPayload::Vote(v) = decoded else {
                bail!("leader received non-vote payload");
            };
            if let Some(qc) = engines[leader_idx].collect_vote(v)? {
                qc_opt = Some(qc);
            }
        }
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
    };

    let text = serde_json::to_string_pretty(&summary)?;
    if let Some(path) = summary_out {
        fs::write(&path, &text).with_context(|| format!("write summary failed: {path}"))?;
    }
    println!("{text}");
    Ok(())
}
