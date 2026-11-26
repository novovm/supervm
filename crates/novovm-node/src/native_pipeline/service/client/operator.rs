//! Bounded operator CLI adapter over NativeApplicationClient. Configuration and
//! stdout JSON are local presentation only; network payloads retain native wire.
//! No transaction signing, account initialization, database access or validator lifecycle.
use super::NativeApplicationClient;
use crate::native_pipeline::{
    consensus::{
        native::{LaneLimits, MessageDomain, ValidatorTransportBindings},
        wire::{Validator, ValidatorSet},
    },
    service::{native_decode_limits, GenesisValidator, NativePeerConfig},
};
use anyhow::{ensure, Context, Result};
use novovm_network::novorudp::{
    endpoint::NativePeer,
    fragments::ReassemblyLimits,
    object_link::{NativeSendLimits, NativeSendPolicy},
    object_sender::ObjectSendState,
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use novovm_network::product_overlay::TransportIdentity;
use novovm_protocol::{
    native_query::{self, Answer, Frame, Query},
    uca_spend::MAX_SIGNED_SPEND_BYTES,
    uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::Read,
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_ROWS: usize = 1024;
mod authority;
pub(crate) mod receipt_proof;
pub use authority::serve_authority;
pub use receipt_proof::verified_receipt;
const MAX_CLIENT_VALIDATORS: usize = 64;
/// Local configuration budget shared with managed health inspection. A pretty
/// JSON array for 64 ML-DSA-65 public keys (1952 bytes each), separate ML-DSA-65
/// transport keys and the target identity fits with field/path headroom. This
/// does not enlarge transaction input, network/output budgets or wire::MAX_VALIDATORS.
pub const MAX_CLIENT_CONFIG_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientConfig {
    pub chain_id: u64,
    pub genesis: [u8; 32],
    pub protocol: [u8; 32],
    pub validator_epoch: u64,
    pub validators: Vec<GenesisValidator>,
    pub node: NativePeerConfig,
    pub bind_addr: SocketAddr,
    /// Existing protected nonvalidator transport-purpose key. Raw keys are rejected.
    /// Never supply a validator signer, UCA management or ML-DSA spend key.
    pub transport_key_file: PathBuf,
    pub library: PathBuf,
    /// Independently approved lowercase SHA-256; no implicit installed-library selection.
    pub library_sha256: String,
    pub timeout_ms: u64,
}

impl ClientConfig {
    fn domain(&self, public_key: &[u8]) -> Result<MessageDomain> {
        ensure!(
            public_key.len() == 1952,
            "native client transport identity must be ML-DSA-65"
        );
        ensure!(
            (1..=MAX_CLIENT_VALIDATORS).contains(&self.validators.len()),
            "native client configuration supports 1..={MAX_CLIENT_VALIDATORS} validators; this is a local resource budget, not the protocol limit"
        );
        ensure!(
            (1..=30_000).contains(&self.timeout_ms),
            "client deadline must be 1..=30000 ms"
        );
        ensure!(
            self.genesis != [0; 32] && self.protocol != [0; 32],
            "missing pinned chain domain"
        );
        ensure!(
            self.node.address.port() != 0
                && !self.node.address.ip().is_unspecified()
                && !self.node.address.ip().is_multicast(),
            "invalid pinned node endpoint"
        );
        ensure!(
            self.validators
                .iter()
                .any(|v| v.public_key == self.node.public_key),
            "target node absent from pinned validator set"
        );
        ensure!(
            self.validators
                .iter()
                .all(|v| v.public_key != public_key && v.transport_public_key != public_key),
            "client must not use a validator signing or transport key"
        );
        let validators = ValidatorSet::new(
            self.chain_id,
            self.validator_epoch,
            1,
            self.validators
                .iter()
                .map(|v| Validator::new(&v.public_key, v.weight))
                .collect::<Result<Vec<_>>>()?,
        )?;
        ValidatorTransportBindings::new(
            &validators,
            self.validators
                .iter()
                .map(|v| (v.public_key.clone(), v.transport_public_key.clone())),
        )?;
        Ok(MessageDomain {
            chain_id: self.chain_id,
            genesis: self.genesis,
            protocol: self.protocol,
            validators: Arc::new(validators),
        })
    }

    fn identity(&self, base: &Path) -> Result<(TransportIdentity, MessageDomain)> {
        let identity = novovm_relay::transport_identity::load_transport_identity(
            &base.join(&self.transport_key_file),
            self.chain_id,
            &base.join(&self.library),
            &self.library_sha256,
        )?;
        let domain = self.domain(identity.public_key())?;
        Ok((identity, domain))
    }

    fn connect(
        &self,
        key: TransportIdentity,
        domain: MessageDomain,
    ) -> Result<NativeApplicationClient> {
        let codec = native_decode_limits(MAX_ROWS, true);
        NativeApplicationClient::connect(
            UdpSocket::bind(self.bind_addr)?,
            key,
            NativePeer {
                key: self
                    .validators
                    .iter()
                    .find(|v| v.public_key == self.node.public_key)
                    .context("target node lacks approved transport binding")?
                    .transport_public_key
                    .clone(),
                address: self.node.address,
            },
            domain,
            codec,
            LaneLimits {
                receive: ReassemblyLimits {
                    max_message_bytes: codec.message_bytes,
                    messages: 2,
                    bytes: 2 * codec.message_bytes,
                    peer_messages: 2,
                    peer_bytes: 2 * codec.message_bytes,
                    ttl: Duration::from_secs(30),
                },
                send: NativeSendLimits {
                    messages: 2,
                    bytes: 2 * codec.message_bytes,
                    peer_messages: 2,
                    peer_bytes: 2 * codec.message_bytes,
                },
            },
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
            wall_ms()?,
            Instant::now(),
        )
    }
}

fn wall_ms() -> Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "input exceeds {limit} bytes");
    Ok(bytes)
}

fn read_client_config(path: &Path) -> Result<ClientConfig> {
    Ok(serde_json::from_slice(&read_bounded(
        path,
        MAX_CLIENT_CONFIG_BYTES,
    )?)?)
}

enum Operation {
    Query(Query),
    Submit(Arc<AssetSpendBatchV3>),
}

fn receipt_view(state: native_query::ReceiptState) -> Value {
    use native_query::ReceiptState;
    match state {
        ReceiptState::Finalized {
            success,
            nonce_after,
            charged_fee,
            height,
            block_hash,
        } => json!({"Finalized":{"success":success,"nonce_after":nonce_after,
                "charged_fee":charged_fee.to_string(),"height":height,"block_hash":block_hash}}),
        ReceiptState::NotRecent => json!("NotRecent"),
        ReceiptState::Received => json!("Received"),
        ReceiptState::RejectedNonce => json!("RejectedNonce"),
    }
}

fn proposal_view(
    status: native_query::ConsensusProposalStatus,
    progress: Option<native_query::ProposalProgress>,
) -> Value {
    use crate::native_pipeline::service::rpc::hex;
    let observation = status.observation.map(|observed| {
        let mut view = json!({
            "in_pending_pool": observed.in_pending_pool,
            "selection_gate": observed.selection_gate,
            "in_local_offer": observed.in_local_offer,
            "candidate": observed.candidate.map(|candidate| json!({
                "value": hex(&candidate.value),
                "body_id": hex(&candidate.body_id),
                "signed_proposal": candidate.signed_proposal.map(|proposal| json!({
                    "proposer": hex(&proposal.proposer),
                    "wire_hex": hex(&proposal.wire),
                    "dispatch": proposal.dispatch
                }))
            }))
        });
        if let Some(progress) = progress {
            let pending_journal = progress.pending_journal.map(|transition| {
                let action = match transition.action {
                    native_query::ProposalJournalAction::Proposal { round, value } => {
                        json!({"Proposal": {"round": round, "value": hex(&value)}})
                    }
                    action => json!(action),
                };
                json!({"action": action, "submitted": transition.submitted})
            });
            view["progress"] = json!({
                "step": progress.step,
                "pending_journal": pending_journal,
                "proposed_round": progress.proposed_round,
                "prevote_qc": progress.prevote_qc.map(|qc| json!({
                    "round": qc.round,
                    "value": hex(&qc.value),
                    "candidate_ready": qc.candidate_ready
                }))
            });
        }
        view
    });
    json!({"ConsensusProposal": {
        "current_height": status.current_height,
        "current_round": status.current_round,
        "observation": observation
    }})
}

// Local presentation only. Never round or narrow monetary u128s through f64/u64.
fn answer_view(answer: Answer) -> Value {
    use native_query::AssetState;
    match answer {
        Answer::Status(status) => json!({"Status":status}),
        Answer::ConsensusProposal(status) => proposal_view(status, None),
        Answer::ConsensusProposalProgress { status, progress } => proposal_view(status, progress),
        Answer::UcaIdentity(state) => json!({"UcaIdentity":state}),
        Answer::UcaApplicationGrant(state) => json!({"UcaApplicationGrant":state}),
        Answer::Error(error) => json!({"Error":error}),
        Answer::ReceiptProof(chunk) => json!({"ReceiptProofChunk": {
            "digest": crate::native_pipeline::service::rpc::hex(&chunk.digest),
            "total_bytes": chunk.total_bytes,
            "chunk_bytes": chunk.data.len(),
        }}),
        Answer::FinalizedHeader(chunk) => json!({"FinalizedHeaderChunk": {
            "digest": crate::native_pipeline::service::rpc::hex(&chunk.digest),
            "total_bytes": chunk.total_bytes,
            "chunk_bytes": chunk.data.len(),
        }}),
        Answer::Receipt {
            state,
            indexed_height,
        } => json!({"Receipt":{"state":receipt_view(state),"indexed_height":indexed_height}}),
        Answer::Receipts {
            states,
            indexed_height,
        } => {
            json!({"Receipts":{"states":states.as_slice().iter().copied().map(receipt_view).collect::<Vec<_>>(),
                "indexed_height":indexed_height}})
        }
        Answer::NovBalance(value) => json!({"NovBalance":value.map(|b|
            json!({"amount":b.amount.to_string(),"state_root":b.state_root,"height":b.height}))}),
        Answer::AssetSpendState(AssetState::Present(s)) => json!({"AssetSpendState":{"Present":{
            "authority":s.authority,
            "state_root":s.state_root,"height":s.height,"account_active":s.account_active,
            "current_spend_epoch":s.current_spend_epoch,"grant_epoch":s.grant_epoch,
            "management_key_ref":s.management_key_ref,"recovery_key_ref":s.recovery_key_ref,
            "grant_present":s.grant_present,
            "permits_nov_transfer":s.permits_nov_transfer,"revoked":s.revoked,
            "not_before_height":s.not_before_height,"expires_at_height":s.expires_at_height,
            "max_debit_per_intent":s.max_debit_per_intent.to_string(),
            "remaining_debit_budget":s.remaining_debit_budget.to_string(),"next_nonce":s.next_nonce,
            "payer_balance":s.payer_balance.to_string(),"recipient_balance":s.recipient_balance.to_string()}}}),
        Answer::AssetSpendState(AssetState::Pending) => json!({"AssetSpendState":"Pending"}),
        Answer::AssetSpendState(AssetState::Missing { state_root, height }) => {
            json!({"AssetSpendState":{"Missing":{"state_root":state_root,"height":height}}})
        }
    }
}

fn operation(action: &str, input: Option<&Path>) -> Result<Operation> {
    let codec = native_decode_limits(MAX_ROWS, true);
    match (action, input) {
        ("status", None) => Ok(Operation::Query(Query::Status)),
        ("query", Some(path)) => {
            let Frame::Request(request) =
                native_query::decode(&read_bounded(path, native_query::MAX_BYTES)?)
                    .map_err(anyhow::Error::msg)?
            else {
                anyhow::bail!("expected native query request, not response");
            };
            // The SDK supplies fresh session/sequence correlation, never replays it.
            Ok(Operation::Query(request.query))
        }
        ("submit", Some(path)) => Ok(Operation::Submit(Arc::new(AssetSpendBatchV3::decode(
            &read_bounded(path, codec.message_bytes)?,
            AssetSpendBatchLimitsV3 {
                transactions: MAX_ROWS,
                transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                body_bytes: codec.body_bytes,
            },
        )?))),
        _ => anyhow::bail!(
            "expected status (no input), query <request.nvquery>, or submit <signed-batch.nuapfl>"
        ),
    }
}

/// Read-only local files; submit is an explicit network write of already-signed
/// input. A timeout leaves business outcome UNKNOWN: keep the signed original.
pub fn run(config_path: &Path, action: &str, input: Option<&Path>) -> Result<Value> {
    run_operation(config_path, operation(action, input)?)
}

pub fn consensus_proposal(
    config_path: &Path,
    height: u64,
    round: u64,
    hash: &str,
) -> Result<Value> {
    proposal_query(config_path, height, round, hash, true)
}

pub fn consensus_proposal_legacy(
    config_path: &Path,
    height: u64,
    round: u64,
    hash: &str,
) -> Result<Value> {
    proposal_query(config_path, height, round, hash, false)
}

fn proposal_query(
    config_path: &Path,
    height: u64,
    round: u64,
    hash: &str,
    include_progress: bool,
) -> Result<Value> {
    let hash: [u8; 32] = crate::native_pipeline::service::config::decode_hex(hash)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected a 32-byte transaction hash"))?;
    ensure!(
        height > 0 && hash != [0; 32],
        "invalid consensus query coordinates"
    );
    let query = if include_progress {
        Query::ConsensusProposalProgress {
            height,
            round,
            hash,
        }
    } else {
        Query::ConsensusProposal {
            height,
            round,
            hash,
        }
    };
    run_operation(config_path, Operation::Query(query))
}

fn run_operation(config_path: &Path, operation: Operation) -> Result<Value> {
    let config_path = config_path.canonicalize()?;
    let config = read_client_config(&config_path)?;
    let (identity, domain) = config.identity(config_path.parent().context("config directory")?)?;
    let mut client = config.connect(identity, domain)?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(config.timeout_ms);
    let result = match operation {
        Operation::Query(selector) => {
            let coordinates = selector.proposal_selector();
            let answer = authority::query(&mut client, selector, deadline)?;
            let mut view = json!({"kind":"node_observation", "answer":answer_view(answer),
                "independently_verified_finality":false});
            if let Some((height, round, hash)) = coordinates {
                view["selector"] = json!({"height":height,"round":round,
                    "tx_hash":crate::native_pipeline::service::rpc::hex(&hash)});
            }
            view
        }
        Operation::Submit(batch) => {
            let rows = batch.len();
            let prepared = client.prepare_asset_spend(batch)?;
            let mut sent = false;
            loop {
                let now = Instant::now();
                ensure!(
                    now < deadline,
                    "native delivery deadline; business outcome UNKNOWN; retain signed input"
                );
                let report = client.poll(wall_ms()?, now, 64)?;
                if let Some(done) = report
                    .completed
                    .iter()
                    .find(|d| d.body.id() == prepared.id())
                {
                    ensure!(
                        done.state == ObjectSendState::Delivered,
                        "native delivery {:?}; business outcome UNKNOWN; retain signed input",
                        done.state
                    );
                    break json!({"kind":"transport_delivery_only", "object_id":prepared.id(),
                        "rows":rows, "finalized":false, "retain_signed_input":true});
                }
                if !sent {
                    sent = client.try_send(&prepared, now)?;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    };
    Ok(
        json!({"scope":"native_sdk_operator_v1", "chain_id":config.chain_id,
        "genesis":config.genesis, "protocol":config.protocol, "node":config.node,
        "elapsed_ms":started.elapsed().as_millis(), "result":result}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_client_config(count: usize) -> ClientConfig {
        let validators: Vec<_> = (0..count)
            .map(|index| {
                // Metadata-only fixture: no validator key generation/signing.
                let mut public_key = vec![255; 1952];
                public_key[..8].copy_from_slice(&(index as u64).to_be_bytes());
                GenesisValidator {
                    public_key,
                    transport_public_key: vec![(index + 16) as u8; 1952],
                    weight: 1,
                }
            })
            .collect();
        ClientConfig {
            chain_id: 7,
            genesis: [1; 32],
            protocol: [2; 32],
            validator_epoch: 1,
            node: NativePeerConfig {
                public_key: validators[0].public_key.clone(),
                address: "127.0.0.1:9000".parse().unwrap(),
            },
            validators,
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            transport_key_file: "client.key".into(),
            library: "qualified-aoem-not-loaded.dll".into(),
            library_sha256: "00".repeat(32),
            timeout_ms: 5000,
        }
    }

    #[test]
    fn native_operator_mldsa_pretty_config_budget_roundtrip_and_rejection() -> Result<()> {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/operator-tests")
            .join(format!("mldsa-config-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("client.json");
        let client = vec![2; 1952];
        for count in [4, MAX_CLIENT_VALIDATORS] {
            let config = metadata_client_config(count);
            let bytes = serde_json::to_vec_pretty(&config)?;
            assert!(
                bytes.len() > 64 * 1024,
                "must reproduce the former config failure"
            );
            assert!(bytes.len() < MAX_CLIENT_CONFIG_BYTES);
            eprintln!(
                "MLDSA client metadata: validators={count} pretty_bytes={}",
                bytes.len()
            );
            std::fs::write(&path, &bytes)?;
            let decoded = read_client_config(&path)?;
            assert_eq!(decoded.validators.len(), count);
            assert_eq!(serde_json::to_vec_pretty(&decoded)?, bytes);
            assert!(decoded.domain(&client).is_ok());
        }
        assert!(metadata_client_config(MAX_CLIENT_VALIDATORS + 1)
            .domain(&client)
            .is_err());
        std::fs::write(&path, vec![b' '; MAX_CLIENT_CONFIG_BYTES + 1])?;
        assert!(read_client_config(&path).is_err());
        // Original CLI refuses before any transport-key read or socket open.
        let error = run(&path, "status", None).unwrap_err();
        assert!(error.to_string().contains("input exceeds"));
        assert!(!directory.join("client.key").exists());
        Ok(())
    }
    #[test]
    fn native_operator_full_width_money_is_decimal_text() {
        let text = u128::MAX.to_string();
        let balance = answer_view(Answer::NovBalance(Some(native_query::Balance {
            amount: u128::MAX,
            state_root: [1; 32],
            height: 1,
        })));
        assert_eq!(balance["NovBalance"]["amount"], text);
        let receipt = receipt_view(native_query::ReceiptState::Finalized {
            success: true,
            nonce_after: 1,
            charged_fee: u128::MAX,
            height: 1,
            block_hash: [2; 32],
        });
        assert_eq!(receipt["Finalized"]["charged_fee"], text);
        let state = answer_view(Answer::AssetSpendState(native_query::AssetState::Present(
            Box::new(native_query::AssetSpendStateViewV2 {
                authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
                state_root: [3; 32],
                height: 1,
                account_active: true,
                current_spend_epoch: 1,
                management_key_ref: [6; 32],
                recovery_key_ref: None,
                grant_present: true,
                grant_epoch: 1,
                permits_nov_transfer: true,
                revoked: false,
                not_before_height: 1,
                expires_at_height: 2,
                max_debit_per_intent: u128::MAX,
                remaining_debit_budget: u128::MAX,
                next_nonce: 1,
                payer_balance: u128::MAX,
                recipient_balance: u128::MAX,
            }),
        )));
        for field in [
            "max_debit_per_intent",
            "remaining_debit_budget",
            "payer_balance",
            "recipient_balance",
        ] {
            assert_eq!(state["AssetSpendState"]["Present"][field], text);
        }
    }

    #[test]
    fn native_operator_files_are_bounded_and_wrong_wire_is_rejected() -> Result<()> {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/operator-tests")
            .join(format!("{}", rand::random::<u64>()));
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("bad-input");
        std::fs::write(&path, [1; 33])?;
        assert!(read_bounded(&path, 32).is_err());
        assert!(operation("submit", Some(&path)).is_err());
        assert!(operation("query", Some(&path)).is_err());
        let bytes = native_query::encode(Frame::Response(native_query::Response {
            request: native_query::Request {
                session: [1; 32],
                sequence: 1,
                query: Query::NovBalance {
                    account: native_query::AccountId::Bytes20([1; 20]),
                },
            },
            answer: Answer::NovBalance(None),
        }))
        .map_err(anyhow::Error::msg)?;
        std::fs::write(&path, bytes)?;
        assert!(operation("query", Some(&path)).is_err());
        Ok(())
    }
    #[test]
    fn native_operator_domain_rejects_foreign_target_and_validator_client() {
        let transport = vec![13; 1952];
        let client = vec![4; 1952];
        let public_key = vec![3; 1952]; // Metadata-only client configuration.
        let mut config = ClientConfig {
            chain_id: 7,
            genesis: [1; 32],
            protocol: [2; 32],
            validator_epoch: 1,
            validators: vec![GenesisValidator {
                public_key: public_key.clone(),
                transport_public_key: transport.clone(),
                weight: 1,
            }],
            node: NativePeerConfig {
                public_key: public_key.clone(),
                address: "127.0.0.1:9000".parse().unwrap(),
            },
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            transport_key_file: "client.key".into(),
            library: "qualified-aoem-not-loaded.dll".into(),
            library_sha256: "00".repeat(32),
            timeout_ms: 5000,
        };
        assert!(config.domain(&client).is_ok());
        assert!(config.domain(&transport).is_err());
        config.timeout_ms = 30_001;
        assert!(config.domain(&client).is_err());
        config.timeout_ms = 5000;
        config.node.public_key = client.clone();
        assert!(config.domain(&client).is_err());
        config.node.public_key = public_key;
        config.genesis = [0; 32];
        assert!(config.domain(&client).is_err());
    }

    #[test]
    fn native_operator_requires_explicit_operation_and_input() {
        assert!(operation("status", None).is_ok());
        for action in ["submit", "query", "reset", "create", "sign", "http"] {
            assert!(operation(action, None).is_err());
        }
        assert!(operation("status", Some(Path::new("unused"))).is_err());
    }
}
