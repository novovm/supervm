//! Original July node startup, reusing the reviewed genesis/store/RPC contracts.
//! Native NovoRUDP owns transport; no Relay controller or alternate chain head.
pub mod client;
pub mod config;
pub mod rpc;
#[cfg(test)]
mod tests;
use super::business::nov_transfer_batch::{
    effect_contract, program_id, receipt_codec, SEMANTIC_VERSION,
};
use super::consensus::{
    bodies::{BodyLimits, CandidateBodies},
    collector::CollectorLimits,
    native::{LaneLimits, MessageDomain, NativeChannel},
    native_lifecycle::{DeliveryLimits, NativeLifecycle},
    pacemaker::TimeoutPolicy,
    progress::{ProgressLimits, RoundProgress},
    statement::ParentPoint,
    transport::DecodeLimits,
    wire::{Context as ConsensusContext, Validator, ValidatorSet},
    ValidatorJournal,
};
use super::execution::plan::{BatchContext, PlanBudget};
use super::ingress::batch::AuthenticationBudget;
use super::persistence::io::IoBudget;
use super::persistence::{CandidateStore, OpenMode, PacketBudget, StorageDomain, StoreConfig};
use super::pipeline::{CandidatePipeline, PipelineConfig};
use super::state::frontier::CaptureBudget;
use super::state::tree::{empty_root, StateNodeReader};
use anyhow::{ensure, Context, Result};
pub use config::{
    GenesisAllocation, GenesisConfig, GenesisValidator, NativePeerConfig, ResidentConfig,
};
use ed25519_dalek::SigningKey;
use novovm_exec::resident::StorageConfig;
use novovm_network::novorudp::{
    endpoint::NativePeer,
    fragments::ReassemblyLimits,
    object_link::{NativeSendLimits, NativeSendPolicy},
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use novovm_network::UdpTransport;
use novovm_protocol::NodeId;
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMode {
    CreateNew,
    Existing,
}

pub struct ResidentNode {
    pub profile: String,
    pub controller: NativeLifecycle,
    pub pipeline: CandidatePipeline,
    pub template: BatchContext,
    pub validators: Arc<ValidatorSet>,
}

impl ResidentNode {
    /// Historical convenience startup. Bind the original UDP owner exactly
    /// once, then use the same supplied-transport initialization as the node tick.
    /// The local route label is not a validator or client authentication identity.
    pub fn start(mut config: ResidentConfig, mode: StartMode) -> Result<Self> {
        config.normalize_native_paths();
        config.validate()?;
        let public = read_signer(&config)?.verifying_key().to_bytes();
        let local_route = NodeId(u64::from_le_bytes(public[..8].try_into()?));
        let transport = UdpTransport::bind_for_chain(
            local_route,
            &config.udp_addr.to_string(),
            config.genesis.chain_id,
        )?;
        Self::start_on_transport(config, mode, transport)
    }

    /// Blocking startup only. Existing mode verifies the complete pinned genesis
    /// content; never installs it, clears logs or repairs an interrupted create.
    /// Consumes the original node's socket owner; never binds another listener.
    pub fn start_on_transport(
        mut config: ResidentConfig,
        mode: StartMode,
        transport: UdpTransport,
    ) -> Result<Self> {
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
        let result = start_native(
            &config,
            &pipeline,
            template,
            validators.clone(),
            key,
            local,
            transport,
        );
        match result {
            Ok(controller) => Ok(Self {
                profile: config.profile.clone(),
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
        let wall_ms = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        self.controller.poll_io(wall_ms, now)?;
        self.controller.poll(&self.pipeline, now)
    }

    /// Administrative drain. Always drains pipeline even if network shutdown
    /// reports failure. Call this on normal exit, not inside a control poll.
    /// Under unwinding the contained owners retain their established Drop rules:
    /// channel stops/joins, pipeline disconnects/wakes without cancelling work.
    pub fn shutdown(self) -> Result<()> {
        drop(self.controller);
        self.pipeline.shutdown()
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
        execution_backend: config.execution_backend,
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

fn start_native(
    config: &ResidentConfig,
    pipeline: &CandidatePipeline,
    template: BatchContext,
    validators: Arc<ValidatorSet>,
    key: SigningKey,
    local: [u8; 32],
    transport: UdpTransport,
) -> Result<NativeLifecycle> {
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

    ensure!(
        config
            .peers
            .iter()
            .all(|p| p.public_key != key.verifying_key().to_bytes()),
        "remote route contains local signer"
    );
    ensure!(
        validators
            .members()
            .iter()
            .filter(|v| v.id() != local)
            .all(|v| config.peers.iter().any(|p| &p.public_key == v.public_key())),
        "missing pinned validator route"
    );
    let codec = DecodeLimits {
        transactions: config.batch_size,
        transaction_bytes: 1024,
        body_bytes: body_byte_limit(config.batch_size),
        message_bytes: 512 * 1024,
    };
    let progress = RoundProgress::new(
        journal,
        pipeline,
        validators.clone(),
        ProgressLimits {
            candidates: (validators.members().len() * 2 + 2).min(64),
            candidate_bytes: 256 * 1024 * 1024,
            votes: CollectorLimits {
                max_retained_rounds: 2,
                max_future_round_span: 64,
                max_votes: validators.members().len() * 7,
            },
        },
        TimeoutPolicy {
            propose: Duration::from_secs(5),
            prevote: Duration::from_secs(5),
            precommit: Duration::from_secs(5),
            round_increment: Duration::from_secs(1),
        },
    )?;
    let bodies = CandidateBodies::new(
        pipeline,
        template,
        config.genesis.policy.clone(),
        codec,
        BodyLimits {
            bodies: (validators.members().len() * 2 + 4).min(64),
            bytes: 64 * 1024 * 1024,
            inflight: 2,
        },
    )?;
    let channel = NativeChannel::from_transport(
        transport,
        key,
        config
            .peers
            .iter()
            .map(|p| NativePeer {
                key: p.public_key,
                address: p.address,
            })
            .collect(),
        config.native_client_limit,
        MessageDomain {
            chain_id: template.chain_id,
            genesis: template.genesis_config_commitment,
            protocol: template.protocol_commitment,
            validators: validators.clone(),
        },
        codec,
        std::array::from_fn(|_| LaneLimits {
            receive: ReassemblyLimits {
                max_message_bytes: codec.message_bytes,
                messages: 32,
                bytes: 16 * 1024 * 1024,
                peer_messages: 4,
                peer_bytes: 2 * 1024 * 1024,
                ttl: Duration::from_secs(30),
            },
            send: NativeSendLimits {
                messages: 32,
                bytes: 16 * 1024 * 1024,
                peer_messages: 4,
                peer_bytes: 2 * 1024 * 1024,
            },
        }),
        NativeSendPolicy {
            window: NovoRudpWindowConfig {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                no_progress_backoff: false,
                ..Default::default()
            },
            pacing: NovoRudpPacingProfile {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                no_progress_backoff: false,
            },
            ttl: Duration::from_secs(30),
        },
    )?;
    NativeLifecycle::new(
        progress,
        bodies,
        channel,
        &validators,
        DeliveryLimits {
            messages_per_peer: 32,
            bytes_per_peer: 4 * 1024 * 1024,
        },
        config.genesis.policy.clone(),
        codec,
    )
}
