//! Original-node assembly of the fixed fast pipeline, not another node/runtime.
//! Experimental fresh profile only: no production activation, legacy opt-in,
//! fixture wallet, implicit key creation, or second authority head.
pub mod config;
pub mod rpc;
pub use config::{GenesisAllocation, GenesisConfig, GenesisValidator, ResidentConfig};

use super::business::nov_transfer_batch::{
    effect_contract, program_id, receipt_codec, SEMANTIC_VERSION,
};
use super::consensus::channel::{ChannelConfig, HostChannel, LaneBudget, QueueBudget};
use super::consensus::collector::CollectorLimits;
use super::consensus::controller::{Controller, ControllerConfig, ControllerLimits};
use super::consensus::pacemaker::TimeoutPolicy;
use super::consensus::statement::ParentPoint;
use super::consensus::transport::DecodeLimits;
use super::consensus::wire::{Context as ConsensusContext, Validator, ValidatorSet};
use super::consensus::ValidatorJournal;
use super::execution::plan::{BatchContext, PlanBudget};
use super::ingress::batch::AuthenticationBudget;
use super::persistence::io::IoBudget;
use super::persistence::{CandidateStore, OpenMode, PacketBudget, StorageDomain, StoreConfig};
use super::pipeline::{CandidatePipeline, PipelineConfig};
use super::state::frontier::CaptureBudget;
use super::state::tree::{empty_root, StateNodeReader};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_exec::resident::StorageConfig;
use novovm_network::duplex::fragments::ReassemblyLimits;
use novovm_network::duplex::peer_id_from_ed25519_public_key_v1;
use novovm_network::duplex::worker::{NetworkWorker, NetworkWorkerConfig, WorkerLimits};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMode {
    CreateNew,
    Existing,
}

pub struct ResidentNode {
    pub controller: Controller,
    pub pipeline: CandidatePipeline,
    pub template: BatchContext,
    pub validators: Arc<ValidatorSet>,
}

impl ResidentNode {
    /// Blocking startup only. Existing mode verifies the complete pinned genesis
    /// content; never installs it, clears logs or repairs an interrupted create.
    pub fn start(mut config: ResidentConfig, mode: StartMode) -> Result<Self> {
        config.normalize_native_paths();
        config.validate()?;
        let (validators, initial) = config.genesis.prepare()?;
        let key = read_signer(&config)?;
        let local = Validator::new(key.verifying_key().to_bytes(), 1)?.id();
        ensure!(
            validators.member(&local).is_some(),
            "local signer is absent from configured validators"
        );
        let pipeline_config = pipeline_config(&config);
        {
            let store = CandidateStore::open(
                pipeline_config.store.clone(),
                match mode {
                    StartMode::CreateNew => OpenMode::CreateNew,
                    StartMode::Existing => OpenMode::Existing,
                },
            )?;
            if mode == StartMode::CreateNew {
                store.install_unpublished_state(&initial)?;
            }
            for (hash, expected) in initial.nodes() {
                ensure!(
                    store.read_node(hash)?.as_ref() == Some(expected),
                    "pinned genesis content missing or altered; explicit recovery required"
                );
            }
        }
        let template = BatchContext {
            chain_id: config.genesis.chain_id,
            genesis_config_commitment: config.genesis.genesis_config_commitment,
            protocol_commitment: config.genesis.protocol_commitment,
            business_program: program_id(),
            semantic_version: SEMANTIC_VERSION,
            effect_contract: effect_contract(&config.genesis.policy)?,
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: initial.root(),
            parent_receipt_root: empty_root(),
            parent_state_version: 0,
            receipt_codec: receipt_codec(),
            height: 1,
            slot: 0,
            timestamp_unix_ms: config.genesis.timestamp_unix_ms,
        };
        let pipeline = CandidatePipeline::start(pipeline_config, OpenMode::Existing)?;
        let result = start_controller(&config, &pipeline, template, validators.clone(), key, local);
        match result {
            Ok(controller) => Ok(Self {
                controller,
                pipeline,
                template,
                validators,
            }),
            Err(error) => {
                let drain = pipeline.shutdown();
                Err(error.context(format!(
                    "resident startup failed (pipeline drain: {drain:?})"
                )))
            }
        }
    }

    pub fn poll(&mut self, now: Instant) -> Result<()> {
        self.controller.poll(&self.pipeline, now)
    }

    /// Administrative drain. Always drains pipeline even if network shutdown
    /// reports failure. Call this on normal exit, not inside a control poll.
    /// Under unwinding the contained owners retain their established Drop rules:
    /// channel stops/joins, pipeline disconnects/wakes without cancelling work.
    pub fn shutdown(mut self) -> Result<()> {
        let network = self.controller.shutdown();
        drop(self.controller);
        let pipeline = self.pipeline.shutdown();
        network.and(pipeline)
    }
}

fn read_signer(config: &ResidentConfig) -> Result<SigningKey> {
    ensure!(
        std::fs::metadata(&config.signing_key_file)?.len() <= 128,
        "signing-key file exceeds explicit key format"
    );
    let bytes =
        std::fs::read(&config.signing_key_file).context("read explicit local signing-key file")?;
    let decoded = if bytes.len() == 32 {
        bytes
    } else {
        config::decode_hex(
            std::str::from_utf8(&bytes)
                .context("signing-key file must be 32 raw bytes or 64 hex digits")?
                .trim(),
        )?
    };
    let secret: [u8; 32] = decoded
        .try_into()
        .map_err(|_| anyhow::anyhow!("signing-key file must contain exactly 32 bytes"))?;
    Ok(SigningKey::from_bytes(&secret))
}

fn body_byte_limit(batch_size: usize) -> usize {
    (batch_size * 1024).min(384 * 1024)
}

fn pipeline_config(config: &ResidentConfig) -> PipelineConfig {
    let size = config.batch_size;
    // Arbitrary RPC transfers may have three distinct keys each (payer,
    // recipient and signer nonce), not the load fixture's shared recipient.
    let keys = 3 * size + 128;
    PipelineConfig {
        store: StoreConfig {
            library: config.library.clone(),
            database: config.database.clone(),
            domain: StorageDomain {
                chain_id: config.genesis.chain_id,
                genesis_config_commitment: config.genesis.genesis_config_commitment,
                protocol_commitment: config.genesis.protocol_commitment,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        workers: config.workers,
        max_batches: 2,
        max_retained_bytes: 256 * 1024 * 1024,
        authentication: AuthenticationBudget {
            transactions: size,
            transaction_bytes: 1024,
            body_bytes: size * 1024,
        },
        plan: PlanBudget {
            transactions: size,
            transaction_bytes: 1024,
            body_bytes: size * 1024,
            access_keys: keys,
        },
        capture: CaptureBudget {
            keys,
            nodes: 65_536,
            bytes: 16 * 1024 * 1024,
        },
        compute_timeout: Duration::from_secs(30),
        io: IoBudget {
            requests: 1,
            ..IoBudget::default()
        },
        capture_edge_quantum: 64,
    }
}

fn start_controller(
    config: &ResidentConfig,
    pipeline: &CandidatePipeline,
    template: BatchContext,
    validators: Arc<ValidatorSet>,
    key: SigningKey,
    local: [u8; 32],
) -> Result<Controller> {
    let context = ConsensusContext {
        chain_id: template.chain_id,
        genesis_config_commitment: template.genesis_config_commitment,
        protocol_commitment: template.protocol_commitment,
        epoch: config.genesis.validator_epoch,
        validator_set_hash: validators.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: template.parent_state_root,
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let mut opening =
        ValidatorJournal::open(pipeline, context, parent, validators.clone(), key.clone())?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let journal = loop {
        if let Some(journal) = opening.poll(pipeline)? {
            break journal;
        }
        ensure!(
            Instant::now() < deadline,
            "resident journal opening timeout; no signer activated"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    let peers: BTreeMap<_, _> = validators
        .members()
        .iter()
        .filter(|v| v.id() != local)
        .map(|v| (v.id(), peer_id_from_ed25519_public_key_v1(v.public_key())))
        .collect();
    let channel = channel(
        config,
        validators.clone(),
        peers.values().cloned().collect(),
        key,
    )?;
    let mut limits = ControllerLimits::default();
    let reserve = validators.members().len() * 2 + limits.max_inflight + 2;
    limits.max_body_bytes = limits
        .max_body_bytes
        .max(channel.preparation_charge() * reserve);
    Controller::new(
        ControllerConfig {
            validators: validators.clone(),
            local_validator: local,
            peers,
            execution: template,
            collector: CollectorLimits {
                max_retained_rounds: 2,
                max_future_round_span: 64,
                max_votes: validators.members().len() * 7,
            },
            timeouts: TimeoutPolicy {
                propose: Duration::from_secs(5),
                prevote: Duration::from_secs(5),
                precommit: Duration::from_secs(5),
                round_increment: Duration::from_secs(1),
            },
            limits,
            retransmit: Duration::from_millis(100),
        },
        journal,
        channel,
    )
}

fn channel(
    config: &ResidentConfig,
    validators: Arc<ValidatorSet>,
    peers: Vec<String>,
    key: SigningKey,
) -> Result<HostChannel> {
    let network = NetworkWorker::start(
        NetworkWorkerConfig {
            chain_id: config.genesis.chain_id,
            relay: config.relay.clone(),
            peers: peers.clone(),
            limits: WorkerLimits::default(),
            handshake_timeout_ms: 5_000,
            reconnect_delay_ms: 50,
            heartbeat_interval_ms: 1_000,
            queue_ttl_ms: 30_000,
        },
        key,
    )?;
    let codec = DecodeLimits {
        transactions: config.batch_size,
        transaction_bytes: 1024,
        body_bytes: body_byte_limit(config.batch_size),
        message_bytes: 512 * 1024,
    };
    let peer_reservation = peers.len() * (codec.message_bytes * 4 + 4096) * 2;
    let lane = LaneBudget {
        control: QueueBudget {
            messages: 32.max(peers.len() * 4),
            bytes: (64 * 1024 * 1024).max(peer_reservation),
        },
        body: QueueBudget {
            messages: 8.max(peers.len() * 2),
            bytes: (32 * 1024 * 1024).max(peer_reservation),
        },
    };
    HostChannel::start(
        network,
        ChannelConfig {
            chain_id: config.genesis.chain_id,
            genesis: config.genesis.genesis_config_commitment,
            protocol: config.genesis.protocol_commitment,
            peers,
            validators,
            policy: config.genesis.policy.clone(),
            codec,
            reassembly: ReassemblyLimits {
                max_message_bytes: codec.message_bytes,
                messages: 32,
                bytes: 8 * 1024 * 1024,
                peer_messages: 8,
                peer_bytes: 2 * 1024 * 1024,
                ttl: Duration::from_secs(30),
            },
            prepare: lane,
            send: lane,
            receive: lane,
            ttl: Duration::from_secs(30),
        },
    )
}

#[cfg(test)]
#[path = "service/tests.rs"]
mod tests;
