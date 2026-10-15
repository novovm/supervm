use super::*;

#[test]
fn optional_auth_and_execution_leave_one_current_slot() {
    assert!(!background_slot_available(0, 0));
    assert!(!background_slot_available(0, 1));
    assert!(background_slot_available(0, 2));
    assert!(!background_slot_available(1, 2));
    assert!(background_slot_available(1, 3));
    assert!(!background_slot_available(2, 3));
}

fn pin() -> Pin {
    Pin {
        source: VoteContext {
            chain_id: 292,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            epoch: 1,
            validator_set_hash: [3; 32],
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
        },
        source_round: 3,
        parent: ParentPoint {
            height: 1,
            block_hash: [4; 32],
            state_root: [5; 32],
            receipt_batch_commitment: [6; 32],
            state_version: 2,
            decision_hash: [7; 32],
        },
    }
}

#[test]
fn only_exact_ack_promotes_all_parent_fields_and_context() -> Result<()> {
    let pin = pin();
    let next = pin.next()?;
    assert!(pin.live(pin.source, 3, false));
    assert!(!pin.promoted(pin.source, 3, pin.parent, None, false));
    assert!(pin.promoted(next, 0, pin.parent, Some(pin.parent), false));
    assert!(!pin.promoted(next, 1, pin.parent, Some(pin.parent), false));
    assert!(!pin.promoted(next, 0, pin.parent, None, false));
    assert!(!pin.promoted(next, 0, pin.parent, Some(pin.parent), true));
    for field in 0..6 {
        let mut parent = pin.parent;
        match field {
            0 => parent.height += 1,
            1 => parent.block_hash[0] ^= 1,
            2 => parent.state_root[0] ^= 1,
            3 => parent.receipt_batch_commitment[0] ^= 1,
            4 => parent.state_version += 1,
            _ => parent.decision_hash[0] ^= 1,
        }
        assert!(
            !pin.promoted(next, 0, parent, Some(parent), false),
            "parent field {field}"
        );
        assert!(
            !pin.promoted(next, 0, pin.parent, Some(parent), false),
            "head field {field}"
        );
    }
    for field in 0..8 {
        let mut context = next;
        match field {
            0 => context.chain_id += 1,
            1 => context.genesis_config_commitment[0] ^= 1,
            2 => context.protocol_commitment[0] ^= 1,
            3 => context.epoch += 1,
            4 => context.validator_set_hash[0] ^= 1,
            5 => context.height += 1,
            6 => context.parent_block_hash[0] ^= 1,
            _ => context.parent_decision_hash[0] ^= 1,
        }
        assert!(
            !pin.promoted(context, 0, pin.parent, Some(pin.parent), false),
            "context field {field}"
        );
    }
    Ok(())
}

#[test]
fn source_round_recovery_and_overflow_do_not_reuse_successor_hint() {
    let mut pin = pin();
    assert!(!pin.live(pin.source, pin.source_round + 1, false));
    assert!(!pin.live(pin.source, pin.source_round, true));
    let mut other = pin.source;
    other.parent_decision_hash[0] ^= 1;
    assert!(!pin.live(other, pin.source_round, false));
    pin.parent.height = u64::MAX;
    assert!(pin.next().is_err());
}

#[test]
fn successor_body_must_bind_exact_business_and_parent_but_keeps_explicit_time() -> Result<()> {
    let pin = pin();
    let context = BatchContext {
        chain_id: pin.source.chain_id,
        genesis_config_commitment: pin.source.genesis_config_commitment,
        protocol_commitment: pin.source.protocol_commitment,
        business_program: [8; 32],
        semantic_version: 1,
        effect_contract: [9; 32],
        receipt_codec: [10; 32],
        parent_block_hash: pin.parent.block_hash,
        parent_height: 1,
        parent_state_root: pin.parent.state_root,
        parent_receipt_root: pin.parent.receipt_batch_commitment,
        parent_state_version: pin.parent.state_version,
        height: 2,
        slot: 2,
        timestamp_unix_ms: 1234,
    };
    assert!(matches_successor_context(&context, &context, pin)?);
    let mut wrong = context;
    wrong.parent_state_version += 1;
    assert!(!matches_successor_context(&wrong, &context, pin)?);
    wrong = context;
    wrong.effect_contract[0] ^= 1;
    assert!(!matches_successor_context(&wrong, &context, pin)?);
    wrong = context;
    wrong.timestamp_unix_ms += 1;
    // Explicit candidate coordinates remain inputs, not wallclock substitution.
    assert!(matches_successor_context(&wrong, &context, pin)?);
    Ok(())
}
