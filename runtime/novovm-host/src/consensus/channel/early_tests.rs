//! Owner/codec tests only: intentionally unverified raw bytes cannot establish
//! AOEM authentication, exact-parent admission, persistence or consensus safety.

use super::*;
use crate::business::nov_transfer_batch::{
    effect_contract, program_id, receipt_codec, SEMANTIC_VERSION,
};
use crate::consensus::wire::{Context as WireContext, Validator};
use crate::state::tree::empty_root;
use ed25519_dalek::SigningKey;
use novovm_network::fragments::CompletedMessage;
use novovm_network::peer_id_from_ed25519_public_key_v1;

fn key(index: u8) -> SigningKey {
    SigningKey::from_bytes(&[121 + index; 32])
}

fn peer() -> String {
    peer_id_from_ed25519_public_key_v1(&key(1).verifying_key().to_bytes())
}

fn config() -> ChannelConfig {
    let validators = Arc::new(
        ValidatorSet::new(
            292,
            1,
            1,
            (0..4)
                .map(|index| Validator::new(key(index).verifying_key().to_bytes(), 1))
                .collect::<Result<_>>()
                .unwrap(),
        )
        .unwrap(),
    );
    let policy = DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    };
    let codec = DecodeLimits {
        transactions: 8,
        transaction_bytes: 16 * 1024,
        body_bytes: 32 * 1024,
        message_bytes: 64 * 1024,
    };
    let charge = codec.message_bytes * 4 + 4096;
    let lanes = LaneBudget {
        control: QueueBudget {
            messages: 8,
            bytes: charge * 8,
        },
        body: QueueBudget {
            messages: 8,
            bytes: charge * 8,
        },
    };
    ChannelConfig {
        chain_id: 292,
        genesis: [0x71; 32],
        protocol: [0x72; 32],
        peers: vec![peer()],
        validators,
        policy,
        codec,
        reassembly: ReassemblyLimits {
            max_message_bytes: codec.message_bytes,
            messages: 8,
            bytes: 512 * 1024,
            peer_messages: 8,
            peer_bytes: 512 * 1024,
            ttl: Duration::from_secs(20),
        },
        prepare: lanes,
        send: lanes,
        receive: lanes,
        ttl: Duration::from_secs(20),
    }
}

fn unstarted(config: &ChannelConfig) -> HostChannel {
    let (_, charge) = validate(config).unwrap();
    HostChannel {
        shared: Arc::new(Mutex::new(Shared::default())),
        stop: Arc::new(AtomicBool::new(false)),
        scope: Arc::new(()),
        peers: config.peers.iter().cloned().collect(),
        prepare_budget: config.prepare,
        send_budget: config.send,
        charge,
        ttl: config.ttl,
        worker: None,
    }
}

fn scope(config: &ChannelConfig) -> EarlyBodyScope {
    EarlyBodyScope {
        source: WireContext {
            chain_id: config.chain_id,
            genesis_config_commitment: config.genesis,
            protocol_commitment: config.protocol,
            epoch: 1,
            validator_set_hash: config.validators.hash(),
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
        },
        source_round: 7,
        target_height: 2,
    }
}

fn context(config: &ChannelConfig) -> BatchContext {
    BatchContext {
        chain_id: config.chain_id,
        genesis_config_commitment: config.genesis,
        protocol_commitment: config.protocol,
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&config.policy).unwrap(),
        parent_block_hash: [0x43; 32],
        parent_height: 1,
        parent_state_root: empty_root(),
        parent_receipt_root: empty_root(),
        parent_state_version: 0,
        receipt_codec: receipt_codec(),
        height: 2,
        slot: 0,
        timestamp_unix_ms: 19,
    }
}

fn raw() -> Vec<Vec<u8>> {
    vec![vec![1, 2, 3], vec![4, 5, 6, 7]]
}

fn early(config: &ChannelConfig) -> Arc<Message> {
    Arc::new(Message::EarlyBody {
        scope: scope(config),
        raw_transactions: raw(),
    })
}

fn prepared(channel: &HostChannel, config: &ChannelConfig, message: Arc<Message>) -> Ready {
    let encoded = transport::encode(&message, config.codec).unwrap();
    prepare(message, encoded, config, &channel.scope, channel.charge).unwrap()
}

fn submit(channel: &HostChannel, token: u64, input: PrepareInput) {
    assert!(matches!(
        channel
            .try_prepare(PrepareRequest { token, input })
            .unwrap(),
        PrepareAdmission::Accepted
    ));
}

fn advance(channel: &HostChannel, config: &ChannelConfig, selected: usize) {
    assert!(prepare_one(
        config,
        &channel.scope,
        channel.charge,
        &channel.shared,
        selected
    )
    .unwrap());
}

fn reply(channel: &HostChannel, expected: u64) -> std::result::Result<Ready, String> {
    let Some(ChannelEvent::Prepared { token, result }) = channel.try_recv().unwrap() else {
        panic!("missing local owner reply");
    };
    assert_eq!(token, expected);
    result
}

#[test]
fn early_owner_preserves_raw_identity_and_regenerates_only_unverified_request() {
    let config = config();
    let channel = unstarted(&config);
    let message = early(&config);
    submit(&channel, 1, PrepareInput::New(message.clone()));
    assert_eq!(channel.shared.lock().unwrap().prepare_usage[1].messages, 1);
    advance(&channel, &config, 1);
    let mut ready = reply(&channel, 1).unwrap();
    let expected = transport::early_body_id(&scope(&config), &raw(), config.codec).unwrap();
    assert!(Arc::ptr_eq(&ready.message, &message));
    assert_eq!(ready.prepared.early_id(), Some(expected));
    assert_eq!(ready.prepared.body_id(), None);
    assert!(ready.body.is_none() && ready.bound_early.is_none());
    assert!(matches!(ready.evidence.as_ref(), VerifiedEvidence::None));
    let input = ready.early.as_mut().unwrap();
    assert_eq!(input.id(), expected);
    assert_eq!(input.scope(), &scope(&config));
    let request = input.take_request().unwrap();
    assert!(input.take_request().is_none());
    submit(&channel, 2, PrepareInput::Cached(ready.prepared.clone()));
    advance(&channel, &config, 1);
    let mut cached = reply(&channel, 2).unwrap();
    assert!(Arc::ptr_eq(&cached.message, &message));
    assert_eq!(cached.prepared.early_id(), Some(expected));
    assert!(cached.early.as_mut().unwrap().take_request().is_some());
    assert_eq!(channel.status().unwrap().encoded_messages, 1);
    assert!(matches!(
        channel
            .try_retire(Retirement::AuthenticationRequest(request))
            .unwrap(),
        RetireAdmission::Accepted
    ));
    assert_eq!(channel.shared.lock().unwrap().retire_usage[1].messages, 1);
    assert!(retire_one(&channel.shared, 1).unwrap());
    assert_eq!(channel.shared.lock().unwrap().retire_usage[1].bytes, 0);
}

#[test]
fn bind_owner_produces_exact_original_body_id_bytes_and_request() {
    let config = config();
    let channel = unstarted(&config);
    let early = prepared(&channel, &config, early(&config)).prepared;
    let expected_early = early.early_id().unwrap();
    let context = context(&config);
    let canonical = prepared(
        &channel,
        &config,
        Arc::new(Message::Body {
            context,
            raw_transactions: raw(),
        }),
    );
    submit(&channel, 1, PrepareInput::BindEarly { early, context });
    advance(&channel, &config, 1);
    let mut ready = reply(&channel, 1).unwrap();
    assert_eq!(ready.bound_early, Some((scope(&config), expected_early)));
    assert!(ready.early.is_none());
    assert_eq!(ready.prepared.early_id(), None);
    assert_eq!(ready.prepared.body_id(), canonical.prepared.body_id());
    assert_eq!(
        ready.prepared.fragment_id(),
        canonical.prepared.fragment_id()
    );
    assert_eq!(
        transport::encode(&ready.message, config.codec).unwrap(),
        transport::encode(&canonical.message, config.codec).unwrap()
    );
    assert_eq!(ready.prepared.retained_bytes(), channel.charge);
    let body = ready.body.as_mut().unwrap();
    assert_eq!(body.id(), canonical.body.unwrap().id());
    assert_eq!(body.context(), &context);
    assert!(body.take_request().is_some());
    assert!(body.take_request().is_none());
    assert!(matches!(ready.evidence.as_ref(), VerifiedEvidence::None));
    // A canonical cached body cannot manufacture early-binding provenance.
    // Controller retries must retain their own local purpose/token relation.
    submit(&channel, 2, PrepareInput::Cached(ready.prepared.clone()));
    advance(&channel, &config, 1);
    let cached = reply(&channel, 2).unwrap();
    assert!(cached.bound_early.is_none() && cached.early.is_none());
    assert_eq!(cached.prepared.body_id(), ready.prepared.body_id());
}

#[test]
fn bind_rejects_foreign_owner_and_non_early_handle_before_body_rebuild() {
    let config = config();
    let first = unstarted(&config);
    let second = unstarted(&config);
    let early = prepared(&first, &config, early(&config)).prepared;
    let original = early.clone();
    let PrepareAdmission::Rejected { request, .. } = second
        .try_prepare(PrepareRequest {
            token: 1,
            input: PrepareInput::BindEarly {
                early,
                context: context(&config),
            },
        })
        .unwrap()
    else {
        panic!("foreign owner accepted");
    };
    let PrepareInput::BindEarly { early, context } = request.input else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(&early.0, &original.0));
    // Exercise the owner guard too, not just the nonblocking admission guard.
    assert!(bind_early(early, context, &config, &second.scope, second.charge).is_err());
    assert_eq!(second.status().unwrap().pending_prepare, 0);
    let body = prepared(
        &first,
        &config,
        Arc::new(Message::Body {
            context,
            raw_transactions: raw(),
        }),
    )
    .prepared;
    submit(
        &first,
        2,
        PrepareInput::BindEarly {
            early: body,
            context,
        },
    );
    advance(&first, &config, 1);
    assert!(reply(&first, 2).err().unwrap().contains("early-body"));
    assert_eq!(first.status().unwrap().encoded_messages, 0);
    assert_eq!(first.shared.lock().unwrap().prepare_usage[1].bytes, 0);
}

#[test]
fn owner_rejects_wrong_early_scope_domain_and_binding_shape() {
    let config = config();
    let channel = unstarted(&config);
    let valid_scope = scope(&config);
    let mut invalid_scopes = Vec::new();
    let mut altered = valid_scope;
    altered.target_height += 1;
    invalid_scopes.push(altered);
    let mut altered = valid_scope;
    altered.source.chain_id += 1;
    invalid_scopes.push(altered);
    let mut altered = valid_scope;
    altered.source.genesis_config_commitment[0] ^= 1;
    invalid_scopes.push(altered);
    let mut altered = valid_scope;
    altered.source.protocol_commitment[0] ^= 1;
    invalid_scopes.push(altered);
    let mut altered = valid_scope;
    altered.source.validator_set_hash[0] ^= 1;
    invalid_scopes.push(altered);
    let mut altered = valid_scope;
    altered.source.epoch += 1;
    invalid_scopes.push(altered);
    for scope in invalid_scopes {
        submit(
            &channel,
            1,
            PrepareInput::New(Arc::new(Message::EarlyBody {
                scope,
                raw_transactions: raw(),
            })),
        );
        advance(&channel, &config, 1);
        assert!(reply(&channel, 1).is_err());
    }
    let early = prepared(&channel, &config, early(&config)).prepared;
    let valid = context(&config);
    let mut invalid_contexts = Vec::new();
    let mut altered = valid;
    altered.height += 1;
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.parent_height += 1;
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.chain_id += 1;
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.genesis_config_commitment[0] ^= 1;
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.protocol_commitment[0] ^= 1;
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.semantic_version = 0;
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.parent_block_hash = [0; 32];
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.parent_state_root = [0; 32];
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.parent_receipt_root = [0; 32];
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.business_program = [0; 32];
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.effect_contract = [0; 32];
    invalid_contexts.push(altered);
    let mut altered = valid;
    altered.receipt_codec = [0; 32];
    invalid_contexts.push(altered);
    for context in invalid_contexts {
        submit(
            &channel,
            2,
            PrepareInput::BindEarly {
                early: early.clone(),
                context,
            },
        );
        advance(&channel, &config, 1);
        assert!(reply(&channel, 2).is_err());
        submit(
            &channel,
            3,
            PrepareInput::New(Arc::new(Message::BindBody {
                scope: valid_scope,
                announcement_id: early.early_id().unwrap(),
                context,
            })),
        );
        advance(&channel, &config, 0);
        assert!(reply(&channel, 3).is_err());
    }
    assert_eq!(channel.status().unwrap().pending_prepare, 0);
}

#[test]
fn binding_reserves_old_allocation_plus_reply_until_consumed_without_blocking_control() {
    let mut config = config();
    let charge = config.codec.message_bytes * 4 + 4096;
    config.prepare.body = QueueBudget {
        messages: 1,
        bytes: 2 * charge,
    };
    let channel = unstarted(&config);
    let early = prepared(&channel, &config, early(&config)).prepared;
    let weak = Arc::downgrade(&early.0);
    let id = early.early_id().unwrap();
    submit(
        &channel,
        1,
        PrepareInput::BindEarly {
            early,
            context: context(&config),
        },
    );
    assert!(weak.upgrade().is_some());
    {
        let shared = channel.shared.lock().unwrap();
        assert_eq!(shared.prepare_usage[1].bytes, 2 * charge);
        assert_eq!(shared.prepare[1][0].charge, 2 * charge);
    }
    submit(
        &channel,
        2,
        PrepareInput::New(Arc::new(Message::BindBody {
            scope: scope(&config),
            announcement_id: id,
            context: context(&config),
        })),
    );
    advance(&channel, &config, 1);
    assert!(weak.upgrade().is_none()); // Original input was destroyed on owner.
    {
        let shared = channel.shared.lock().unwrap();
        assert_eq!(shared.prepare_usage[1].bytes, 2 * charge);
        assert_eq!(shared.local[1][0].charge, 2 * charge);
    }
    assert!(matches!(
        channel
            .try_prepare(PrepareRequest {
                token: 3,
                input: PrepareInput::New(early_message(&config)),
            })
            .unwrap(),
        PrepareAdmission::Backpressure(_)
    ));
    advance(&channel, &config, 0);
    let control = reply(&channel, 2).unwrap();
    assert!(control.body.is_none() && control.early.is_none() && control.bound_early.is_none());
    assert!(matches!(control.evidence.as_ref(), VerifiedEvidence::None));
    assert_eq!(
        channel.shared.lock().unwrap().prepare_usage[1].bytes,
        2 * charge
    );
    let body = reply(&channel, 1).unwrap();
    assert!(body.body.is_some());
    assert_eq!(channel.shared.lock().unwrap().prepare_usage[1].bytes, 0);
}

// Keeps local names for prepared handles unambiguous in the budget tests.
fn early_message(config: &ChannelConfig) -> Arc<Message> {
    early(config)
}

#[test]
fn too_small_bind_budget_returns_original_and_can_retire_without_second_reply_charge() {
    let mut config = config();
    let charge = config.codec.message_bytes * 4 + 4096;
    config.prepare.body = QueueBudget {
        messages: 1,
        bytes: charge,
    };
    let channel = unstarted(&config); // This is a valid existing configuration.
    let early = prepared(&channel, &config, early(&config)).prepared;
    let weak = Arc::downgrade(&early.0);
    let PrepareAdmission::Rejected { request, .. } = channel
        .try_prepare(PrepareRequest {
            token: 1,
            input: PrepareInput::BindEarly {
                early,
                context: context(&config),
            },
        })
        .unwrap()
    else {
        panic!("under-reserved bind accepted");
    };
    assert!(weak.upgrade().is_some());
    assert_eq!(channel.status().unwrap().pending_prepare, 0);
    assert!(matches!(
        channel
            .try_retire(Retirement::Input(request.input))
            .unwrap(),
        RetireAdmission::Accepted
    ));
    assert!(weak.upgrade().is_some());
    assert_eq!(channel.shared.lock().unwrap().retire_usage[1].bytes, charge);
    assert!(retire_one(&channel.shared, 1).unwrap());
    assert!(weak.upgrade().is_none());
    assert_eq!(channel.shared.lock().unwrap().retire_usage[1].bytes, 0);
}

#[test]
fn early_receive_and_send_use_body_lane_while_bind_has_no_body_authority() {
    let mut config = config();
    let charge = config.codec.message_bytes * 4 + 4096;
    config.receive.body = QueueBudget {
        messages: 1,
        bytes: charge,
    };
    config.send.body = QueueBudget {
        messages: 1,
        bytes: charge,
    };
    let channel = unstarted(&config);
    let body = early(&config);
    let id = transport::early_body_id(&scope(&config), &raw(), config.codec).unwrap();
    let bind = Arc::new(Message::BindBody {
        scope: scope(&config),
        announcement_id: id,
        context: context(&config),
    });
    for message in [&body, &body, &bind] {
        receive_one(
            CompletedMessage {
                peer: peer(),
                id: [1; 32],
                chunks: vec![transport::encode(message, config.codec).unwrap()],
            },
            &config,
            &channel.scope,
            channel.charge,
            &channel.shared,
        )
        .unwrap();
    }
    {
        let shared = channel.shared.lock().unwrap();
        assert_eq!(shared.receive_usage[1].messages, 1);
        assert_eq!(shared.receive_usage[0].messages, 1);
        assert_eq!(shared.status.dropped_received, 1);
        assert_eq!(shared.status.invalid_received, 0);
    }
    let Some(ChannelEvent::Received(control)) = channel.try_recv().unwrap() else {
        panic!("missing bind");
    };
    assert!(matches!(
        control.ready.message.as_ref(),
        Message::BindBody { .. }
    ));
    assert!(control.ready.body.is_none() && control.ready.early.is_none());
    assert!(matches!(
        control.ready.evidence.as_ref(),
        VerifiedEvidence::None
    ));
    let Some(ChannelEvent::Received(body)) = channel.try_recv().unwrap() else {
        panic!("missing early body");
    };
    assert!(body.ready.body.is_none());
    assert_eq!(body.ready.early.as_ref().unwrap().id(), id);
    assert!(matches!(
        channel
            .try_send(Outbound {
                peer: peer(),
                message: body.ready.prepared.clone()
            })
            .unwrap(),
        SendAdmission::Accepted
    ));
    assert!(matches!(
        channel
            .try_send(Outbound {
                peer: peer(),
                message: body.ready.prepared
            })
            .unwrap(),
        SendAdmission::Backpressure(_)
    ));
    assert!(matches!(
        channel
            .try_send(Outbound {
                peer: peer(),
                message: control.ready.prepared
            })
            .unwrap(),
        SendAdmission::Accepted
    ));
    let shared = channel.shared.lock().unwrap();
    assert_eq!(shared.send_usage[0].messages, 1);
    assert_eq!(shared.send_usage[1].messages, 1);
    assert_eq!(
        shared.receive_usage[0].bytes + shared.receive_usage[1].bytes,
        0
    );
}
