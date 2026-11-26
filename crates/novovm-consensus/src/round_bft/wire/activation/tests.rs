use super::*;
use crate::round_bft::{crypto::ConsensusSigner, test_vectors::FixtureSigningKey, wire::Validator};

#[test]
fn business_activation_requires_original_weighted_quorum_on_exact_anchor() -> Result<()> {
    let keys = [1, 2, 3, 4].map(|value| FixtureSigningKey::from_bytes(&[value; 32]));
    let members = keys
        .iter()
        .zip([2, 1, 1, 1])
        .map(|(key, weight)| Validator::new(key.public_key(), weight))
        .collect::<Result<Vec<_>>>()?;
    let set = ValidatorSet::new_for_test(7, 1, 1, members.clone())?;
    let rules = |version| BusinessRules {
        program: [version; 32],
        semantic_version: u32::from(version),
        effects: [10; 32],
        receipts: [11; 32],
    };
    let subject = BusinessActivation {
        context: Context {
            chain_id: 7,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            epoch: 1,
            validator_set_hash: set.hash(),
            height: 8,
            parent_block_hash: [3; 32],
            parent_decision_hash: [4; 32],
        },
        parent_state_root: [5; 32],
        parent_receipt_root: [6; 32],
        parent_state_version: 7,
        previous: rules(3),
        next: rules(5),
    };
    assert_eq!(set.quorum_weight(), 4);
    let message = subject.signing_bytes()?;
    let approvals = |indices: &[usize]| {
        let mut result: Vec<_> = indices
            .iter()
            .map(|&i| ActivationApproval {
                signer: members[i].id(),
                signature: keys[i].sign(&message).to_bytes(),
            })
            .collect();
        result.sort_by_key(|row| row.signer);
        result
    };
    assert!(VerifiedBusinessActivation::verify(subject, &approvals(&[1, 2, 3]), &set).is_err());
    assert!(VerifiedBusinessActivation::verify(subject, &approvals(&[0, 0, 1]), &set).is_err());
    let quorum = approvals(&[0, 1, 2]);
    assert_eq!(
        VerifiedBusinessActivation::verify(subject, &quorum, &set)?.subject(),
        &subject
    );
    let mut changed = subject;
    changed.context.parent_decision_hash[0] ^= 1;
    assert!(VerifiedBusinessActivation::verify(changed, &quorum, &set).is_err());
    let mut changed = subject;
    changed.next.effects[0] ^= 1;
    assert!(VerifiedBusinessActivation::verify(changed, &quorum, &set).is_err());
    let mut changed = subject;
    changed.context.height += 1;
    assert!(VerifiedBusinessActivation::verify(changed, &quorum, &set).is_err());
    let mut reversed = quorum;
    reversed.reverse();
    assert!(VerifiedBusinessActivation::verify(subject, &reversed, &set).is_err());
    Ok(())
}
