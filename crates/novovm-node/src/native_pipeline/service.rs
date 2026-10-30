//! Original July node startup, reusing the reviewed genesis/store/RPC contracts.
//! Native NovoRUDP owns transport; no Relay controller or alternate chain head.
pub mod client;
pub mod config;
pub mod rpc;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod uca_native_tests;
use super::business::nov_transfer_batch::{
    effect_contract, program_id, receipt_codec, uca_effect_contract, uca_program_id,
    uca_receipt_codec, SEMANTIC_VERSION,
};
use super::consensus::{
    bodies::{BodyLimits, CandidateBodies},
    collector::CollectorLimits,
    crypto::{AoemConsensusCrypto, AoemConsensusSigner},
    native::{LaneLimits, MessageDomain, NativeChannel, ValidatorTransportBindings},
    native_lifecycle::{DeliveryLimits, NativeLifecycle},
    pacemaker::TimeoutPolicy,
    progress::{ProgressLimits, RoundProgress},
    statement::ParentPoint,
    transport::DecodeLimits,
    wire::{Context as ConsensusContext, ValidatorSet},
    ValidatorJournal,
};
use super::execution::plan::{BatchContext, PlanBudget};
use super::ingress::batch::AuthenticationBudget;
use super::persistence::io::IoBudget;
use super::persistence::{OpenMode, PacketBudget, StorageDomain, StoreConfig};
use super::pipeline::{CandidatePipeline, PipelineConfig};
use super::state::frontier::CaptureBudget;
use super::state::tree::empty_root;
use anyhow::{ensure, Context, Result};
pub use config::{
    GenesisAllocation, GenesisConfig, GenesisValidator, NativePeerConfig, NodeKeyStorage,
    ResidentConfig,
};
use ed25519_dalek::SigningKey;
use novovm_consensus::round_bft::crypto::ConsensusSigner;
use novovm_exec::resident::StorageConfig;
use novovm_network::novorudp::{
    endpoint::NativePeer,
    fragments::ReassemblyLimits,
    object_link::{NativeSendLimits, NativeSendPolicy},
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use novovm_network::UdpTransport;
use novovm_protocol::NodeId;
use novovm_relay::purpose_keys::{self, PurposeV1};
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
        let prepared = prepare_startup(&config)?;
        let public = prepared.identities.transport.verifying_key().to_bytes();
        let local_route = NodeId(u64::from_le_bytes(public[..8].try_into()?));
        let transport = UdpTransport::bind_for_chain(
            local_route,
            &config.udp_addr.to_string(),
            config.genesis.chain_id,
        )?;
        Self::start_prepared(config, mode, transport, prepared)
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
        let prepared = prepare_startup(&config)?;
        Self::start_prepared(config, mode, transport, prepared)
    }

    fn start_prepared(
        config: ResidentConfig,
        mode: StartMode,
        transport: UdpTransport,
        prepared: StartupPreparation,
    ) -> Result<Self> {
        let StartupPreparation {
            validators,
            initial,
            identities,
        } = prepared;
        let pipeline_config = pipeline_config(&config);
        let template = BatchContext {
            chain_id: config.genesis.chain_id,
            genesis_config_commitment: config.genesis.genesis_config_commitment,
            protocol_commitment: config.genesis.protocol_commitment,
            business_program: if config.uca_initialization.is_some() {
                uca_program_id()
            } else {
                program_id()
            },
            semantic_version: SEMANTIC_VERSION,
            effect_contract: if config.uca_initialization.is_some() {
                uca_effect_contract(&config.genesis.policy)?
            } else {
                effect_contract(&config.genesis.policy)?
            },
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: initial.root(),
            parent_receipt_root: empty_root(),
            parent_state_version: 0,
            receipt_codec: if config.uca_initialization.is_some() {
                uca_receipt_codec()
            } else {
                receipt_codec()
            },
            height: 1,
            slot: 0,
            timestamp_unix_ms: config.genesis.timestamp_unix_ms,
        };
        let pipeline = CandidatePipeline::start_with_initial_state(
            pipeline_config,
            match mode {
                StartMode::CreateNew => OpenMode::CreateNew,
                StartMode::Existing => OpenMode::Existing,
            },
            initial,
        )?;
        let result = start_native(
            &config,
            &pipeline,
            template,
            validators.clone(),
            identities,
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
        {
            let _span = crate::native_pipeline::profile::span(
                crate::native_pipeline::profile::Phase::NativeIo,
            );
            self.controller.poll_io(wall_ms, now)?;
        }
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::NativeAssembly,
        );
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

struct NodeIdentities {
    validator: Arc<AoemConsensusSigner>,
    transport: SigningKey,
    bindings: ValidatorTransportBindings,
}

struct StartupPreparation {
    validators: Arc<ValidatorSet>,
    initial: super::state::tree::StagedStateUpdate,
    identities: NodeIdentities,
}

fn prepare_startup(config: &ResidentConfig) -> Result<StartupPreparation> {
    let (metadata, reference_initial) = if config.uca_initialization.is_some() {
        // The approved initializer below constructs the complete asset tree.
        // Validate the same allocations here without staging a discarded copy.
        (config.genesis.validated_allocations()?.0, None)
    } else {
        let (metadata, initial) = config.genesis.prepare()?;
        (metadata, Some(initial))
    };
    // Authenticate the independent transport key and complete routing binding
    // before loading AOEM. A credential error cannot activate a signer or store.
    let transport = read_key(
        &config.transport_key_file,
        config.key_storage,
        PurposeV1::Transport,
        config.genesis.chain_id,
    )?;
    let bindings = config.genesis.transport_bindings(&metadata)?;
    let local_transport = transport.verifying_key().to_bytes();
    let public = config
        .genesis
        .validators
        .iter()
        .find(|v| v.transport_public_key == local_transport)
        .context("local transport key differs from committed validator binding")?
        .public_key
        .as_slice();
    ensure!(
        config.peers.iter().all(|p| p.public_key != public)
            && metadata
                .members()
                .iter()
                .filter(|v| v.public_key() != public)
                .all(|v| config.peers.iter().any(|p| p.public_key == v.public_key())),
        "missing remote validator route or route targets local signer"
    );
    // One qualified crypto owner; no per-vote load/KAT or Ed25519 fallback.
    let runtime = novovm_exec::AoemExecFacade::open(
        &config.library,
        novovm_exec::AoemExecOpenOptions::default(),
    )?;
    let crypto = AoemConsensusCrypto::new(&runtime)?;
    let validators = Arc::new((*metadata).clone().with_verifier(Arc::new(crypto.clone())));
    ensure!(
        validators.has_verifier() && !validators.is_test_only(),
        "original node requires qualified ML-DSA-65 consensus verification"
    );
    let identities = read_identities(config, &validators, &crypto, transport, bindings)?;
    // All pins and management signatures precede opening the state owner.
    let initial = if let Some(material) = &config.uca_initialization {
        material.verify(&config.genesis, &runtime)?.into_state()
    } else {
        reference_initial.expect("reference initial state prepared with validator metadata")
    };
    Ok(StartupPreparation {
        validators,
        initial,
        identities,
    })
}

fn read_identities(
    config: &ResidentConfig,
    validators: &ValidatorSet,
    crypto: &AoemConsensusCrypto,
    transport: SigningKey,
    bindings: ValidatorTransportBindings,
) -> Result<NodeIdentities> {
    ensure!(config.key_storage == NodeKeyStorage::ProtectedV1,
        "ML-DSA-65 validator requires protected purpose custody; raw Ed25519 keys are not a fallback");
    let material = purpose_keys::load_validator(
        &config.signing_key_file,
        config.genesis.chain_id,
        |public, secret, challenge| crypto.check_key_pair(public, secret, challenge),
    )?;
    let validator = crypto.signer(material)?;
    let public = validator.public_key();
    ensure!(
        bindings.transport_public_key(public)? == transport.verifying_key().to_bytes(),
        "local transport key differs from committed validator binding"
    );
    ensure!(
        config.peers.iter().all(|p| p.public_key != public)
            && validators
                .members()
                .iter()
                .filter(|v| v.public_key() != public)
                .all(|v| config.peers.iter().any(|p| p.public_key == v.public_key())),
        "missing remote validator route or route targets local signer"
    );
    Ok(NodeIdentities {
        validator,
        transport,
        bindings,
    })
}

fn read_key(
    path: &std::path::Path,
    storage: NodeKeyStorage,
    purpose: PurposeV1,
    chain_id: u64,
) -> Result<SigningKey> {
    ensure!(
        storage == NodeKeyStorage::ProtectedV1,
        "node startup requires protected purpose custody; raw keys are retired"
    );
    purpose_keys::load(path, purpose, chain_id).context("read protected node purpose custody")
}

fn body_byte_limit(batch_size: usize) -> usize {
    (batch_size * 1024).min(384 * 1024)
}

// Expanded signed inputs, distinct from the APFL encoded network object.
const NATIVE_ASSET_MAX_ROWS: usize = 1024;
const NATIVE_ASSET_ROW_BYTES: usize = 8192;

fn uca_body_byte_limit(batch_size: usize) -> usize {
    batch_size * NATIVE_ASSET_ROW_BYTES
}

/// Shared original node/client acceptance capacity. An asset node's local
/// proposal target must not reject a larger valid candidate from another node.
/// The historical V3 reference keeps its configured bounds.
pub(crate) fn native_decode_limits(batch_size: usize, asset_accounts: bool) -> DecodeLimits {
    let rows = if asset_accounts {
        NATIVE_ASSET_MAX_ROWS
    } else {
        batch_size
    };
    let body_bytes = if asset_accounts {
        uca_body_byte_limit(rows)
    } else {
        body_byte_limit(rows)
    };
    DecodeLimits {
        transactions: rows,
        transaction_bytes: if asset_accounts {
            NATIVE_ASSET_ROW_BYTES
        } else {
            1024
        },
        body_bytes,
        // Fourteen APFL columns add at most two u32 fields per cell, plus
        // dictionary counts and the existing frame/context header. At 1024
        // rows that overhead is below 128 KiB, even without shared values.
        message_bytes: if asset_accounts {
            (body_bytes + 128 * 1024).max(512 * 1024)
        } else {
            512 * 1024
        },
    }
}

fn native_lane_limits(message_bytes: usize) -> LaneLimits {
    LaneLimits {
        receive: ReassemblyLimits {
            max_message_bytes: message_bytes,
            messages: 32,
            bytes: (16 * message_bytes).max(16 * 1024 * 1024),
            peer_messages: 4,
            peer_bytes: 4 * message_bytes,
            ttl: Duration::from_secs(30),
        },
        send: NativeSendLimits {
            messages: 32,
            bytes: (16 * message_bytes).max(16 * 1024 * 1024),
            peer_messages: 4,
            peer_bytes: 4 * message_bytes,
        },
    }
}

fn native_body_retention_limit(codec: DecodeLimits) -> usize {
    // Previous decided input + current body + successor retain encoded and
    // expanded inputs/signature facts. Keep the existing ordinary-body reserve
    // too; increasing wire capacity alone must not disable lookahead.
    (16 * codec.message_bytes).max(64 * 1024 * 1024)
}

fn pipeline_config(config: &ResidentConfig) -> PipelineConfig {
    // Arbitrary RPC transfers may have three distinct keys each (payer,
    // recipient and signer nonce), not the load fixture's shared recipient.
    let uca = config.uca_initialization.is_some();
    let size = if uca {
        NATIVE_ASSET_MAX_ROWS
    } else {
        config.batch_size
    };
    let row_bytes = if uca { NATIVE_ASSET_ROW_BYTES } else { 1024 };
    let body_bytes = if uca {
        uca_body_byte_limit(size)
    } else {
        size * 1024
    };
    // Asset transactions do not read UCA identity mappings. The largest
    // Ordinary account-management fits 31 keys; policy/fee adds 42 per batch.
    // Rich opening rows select fewer rows within this SAME common key budget.
    // Charge actual supported access, not 128 old identity pages per row: the
    // latter would unnecessarily consume the second candidate's reservation.
    let keys = if uca {
        rpc::ASSET_PLAN_ACCESS_KEYS
    } else {
        3 * size + 128
    };
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
        signature_backend: config.signature_backend,
        prepare_asset_signatures: uca,
        max_batches: 2,
        max_retained_bytes: 256 * 1024 * 1024,
        authentication: AuthenticationBudget {
            transactions: size,
            transaction_bytes: row_bytes,
            body_bytes,
        },
        plan: PlanBudget {
            transactions: size,
            transaction_bytes: row_bytes,
            body_bytes,
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
    identities: NodeIdentities,
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
    let mut opening = ValidatorJournal::open(
        pipeline,
        context,
        parent,
        validators.clone(),
        identities.validator,
    )?;
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

    let codec = native_decode_limits(config.batch_size, config.uca_initialization.is_some());
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
            bytes: native_body_retention_limit(codec),
            inflight: 2,
        },
    )?;
    let channel = NativeChannel::from_transport(
        transport,
        identities.transport,
        identities.bindings.clone(),
        config
            .peers
            .iter()
            .map(|p| {
                Ok(NativePeer {
                    key: identities.bindings.transport_public_key(&p.public_key)?,
                    address: p.address,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        config.native_client_limit,
        MessageDomain {
            chain_id: template.chain_id,
            genesis: template.genesis_config_commitment,
            protocol: template.protocol_commitment,
            validators: validators.clone(),
        },
        codec,
        // Control evidence does not grow with transaction bodies. Keep its
        // original lane ceiling; body/ingress lanes share the larger policy.
        std::array::from_fn(|lane| {
            native_lane_limits(if lane == 0 {
                512 * 1024
            } else {
                codec.message_bytes
            })
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
            bytes_per_peer: 8 * codec.message_bytes,
        },
        config.genesis.policy.clone(),
        codec,
    )
}
