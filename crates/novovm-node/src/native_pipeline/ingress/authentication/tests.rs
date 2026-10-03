use super::*;
use crate::native_pipeline::ingress::wire::{encode_transfer_v3, signing_message, FeePolicy};
use ed25519_dalek::{Signer, SigningKey, Verifier};

const CHAIN: u64 = 71;
const MAX_BYTES: usize = 4096;

fn account(key: &[u8; 32], width: usize) -> Vec<u8> {
    match width {
        20 => Sha256::digest(key)[12..32].to_vec(),
        32 => key.to_vec(),
        _ => panic!("unsupported fixture account width"),
    }
}

fn resign(transaction: &mut TransferV3, key: &SigningKey) {
    transaction.signature.clear();
    let signature = key.sign(&signing_message(transaction).unwrap());
    transaction
        .signature
        .extend_from_slice(key.verifying_key().as_bytes());
    transaction
        .signature
        .extend_from_slice(&signature.to_bytes());
}

fn signed(seed: u8, width: usize, nonce: u64) -> (SigningKey, TransferV3) {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut transaction = TransferV3 {
        chain_id: CHAIN,
        from: account(key.verifying_key().as_bytes(), width),
        to: account(
            SigningKey::from_bytes(&[91; 32]).verifying_key().as_bytes(),
            width,
        ),
        asset: "NOV".to_owned(),
        amount: 19,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".to_owned(),
            max_pay_amount: 300,
            slippage_bps: 25,
        },
        signature: Vec::new(),
    };
    resign(&mut transaction, &key);
    (key, transaction)
}

fn checked(seed: u8, width: usize, nonce: u64) -> SignatureCheckedTransfer {
    let (_, transaction) = signed(seed, width, nonce);
    authenticate_transfer_v3(&encode_transfer_v3(&transaction).unwrap(), CHAIN, MAX_BYTES).unwrap()
}

fn rejection(transaction: &TransferV3, chain: u64) -> String {
    let raw = encode_transfer_v3(transaction).unwrap();
    let error = authenticate_transfer_v3(&raw, chain, MAX_BYTES)
        .err()
        .expect("the transaction must be rejected");
    format!("{error:#}")
}

#[test]
fn real_signatures_bind_both_account_widths_to_one_chain_separated_nonce_identity() {
    let first = checked(11, 20, 0);
    let second = checked(11, 32, 1);
    assert_ne!(first.transfer().from, second.transfer().from);
    assert_eq!(first.public_key(), second.public_key());
    assert_eq!(first.nonce_identity(), second.nonce_identity());
    assert_eq!(
        first.tx_hash(),
        first.transfer().canonical_tx_hash().unwrap()
    );
    assert_eq!(
        second.tx_hash(),
        second.transfer().canonical_tx_hash().unwrap()
    );

    // Independently spell out the legacy chain-separated identity. A balance
    // alias changes the signed transaction, not the signer's nonce bucket.
    let mut identity = Sha256::new();
    identity.update(b"novovm-native-auth-nonce-identity-v1");
    identity.update(CHAIN.to_be_bytes());
    identity.update(b"novovm-native-auth/ed25519-public-key/v2");
    identity.update([0]);
    identity.update(first.public_key());
    assert_eq!(
        first.nonce_identity(),
        <[u8; 32]>::from(identity.finalize())
    );

    let (key, mut other_chain) = signed(11, 20, 0);
    other_chain.chain_id += 1;
    resign(&mut other_chain, &key);
    let other = authenticate_transfer_v3(
        &encode_transfer_v3(&other_chain).unwrap(),
        other_chain.chain_id,
        MAX_BYTES,
    )
    .unwrap();
    assert_ne!(first.nonce_identity(), other.nonce_identity());
    assert_ne!(first.nonce_identity(), checked(12, 20, 0).nonce_identity());
}

#[test]
fn cryptographically_valid_signature_cannot_authorize_another_payer() {
    for width in [20, 32] {
        let (key, mut transaction) = signed(13, width, 0);
        let other = SigningKey::from_bytes(&[14; 32]);
        transaction.from = account(other.verifying_key().as_bytes(), width);
        resign(&mut transaction, &key);
        let signature = Signature::from_slice(&transaction.signature[32..]).unwrap();
        key.verifying_key()
            .verify_strict(&signing_message(&transaction).unwrap(), &signature)
            .unwrap();
        assert!(rejection(&transaction, CHAIN).contains("does not authorize payer"));
    }
}

#[test]
fn every_signed_transfer_field_is_authenticated() {
    type Mutation = (&'static str, fn(&mut TransferV3));
    let mutations: [Mutation; 9] = [
        ("chain", |tx| tx.chain_id += 1),
        ("from", |tx| tx.from[0] ^= 1),
        ("to", |tx| tx.to[0] ^= 1),
        ("asset", |tx| tx.asset.push('X')),
        ("amount", |tx| tx.amount += 1),
        ("nonce", |tx| tx.nonce += 1),
        ("fee asset", |tx| tx.fee_policy.pay_asset.push('X')),
        ("maximum fee", |tx| tx.fee_policy.max_pay_amount += 1),
        ("slippage", |tx| tx.fee_policy.slippage_bps += 1),
    ];
    let (key, original) = signed(15, 20, 0);
    let message = signing_message(&original).unwrap();
    let signature = Signature::from_slice(&original.signature[32..]).unwrap();
    for (name, mutate) in mutations {
        let mut altered = original.clone();
        mutate(&mut altered);
        let altered_message = signing_message(&altered).unwrap();
        assert_ne!(message, altered_message, "{name}");
        assert!(
            key.verifying_key()
                .verify_strict(&altered_message, &signature)
                .is_err(),
            "{name}"
        );
        // Use the changed chain as the configured domain so this particular
        // negative also exercises crypto, rather than only the domain gate.
        assert!(
            rejection(&altered, altered.chain_id).contains("invalid native V3 signature"),
            "{name}"
        );
    }

    let mut altered_signature = original.clone();
    altered_signature.signature[45] ^= 1;
    assert!(rejection(&altered_signature, CHAIN).contains("invalid native V3 signature"));
    let mut altered_key = original;
    altered_key.signature[..32]
        .copy_from_slice(SigningKey::from_bytes(&[16; 32]).verifying_key().as_bytes());
    assert!(rejection(&altered_key, CHAIN).contains("invalid native V3 signature"));
}

#[test]
fn chain_domain_nonce_exhaustion_and_input_budget_are_hard_gates() {
    let (_, transaction) = signed(17, 20, 0);
    assert!(rejection(&transaction, 0).contains("nonzero"));
    assert!(rejection(&transaction, CHAIN + 1).contains("domain mismatch"));
    let (key, mut zero_chain) = signed(17, 20, 0);
    zero_chain.chain_id = 0;
    resign(&mut zero_chain, &key);
    assert!(rejection(&zero_chain, 0).contains("nonzero"));
    assert!(rejection(&zero_chain, CHAIN).contains("domain mismatch"));

    let (_, exhausted) = signed(17, 32, u64::MAX);
    assert!(rejection(&exhausted, CHAIN).contains("nonce exhausted"));
    let final_nonce = checked(17, 32, u64::MAX - 1);
    let parent = BTreeMap::from([(final_nonce.nonce_identity(), u64::MAX - 1)]);
    assert_eq!(
        check_nonce_sequence(&[final_nonce], &parent).unwrap()[0].after,
        u64::MAX
    );

    let raw = encode_transfer_v3(&transaction).unwrap();
    assert!(authenticate_transfer_v3(&raw, CHAIN, raw.len()).is_ok());
    assert!(authenticate_transfer_v3(&raw, CHAIN, raw.len() - 1).is_err());
}

#[test]
fn weak_identity_key_passes_ordinary_verify_but_is_rejected_by_strict_admission() {
    // A=identity, R=identity, S=0 satisfies the ordinary verification equation
    // for every message. This is a concrete weak-key counterexample, not merely
    // a randomly damaged signature that both verification methods reject.
    let mut identity = [0; 32];
    identity[0] = 1;
    let key = VerifyingKey::from_bytes(&identity).unwrap();
    assert!(key.is_weak());
    let mut signature_bytes = [0; 64];
    signature_bytes[..32].copy_from_slice(&identity);
    let signature = Signature::from_bytes(&signature_bytes);
    for width in [20, 32] {
        let (_, mut transaction) = signed(18, width, 0);
        transaction.from = account(&identity, width);
        transaction.signature.clear();
        transaction.signature.extend_from_slice(&identity);
        transaction.signature.extend_from_slice(&signature_bytes);
        let message = signing_message(&transaction).unwrap();
        key.verify(&message, &signature).unwrap();
        assert!(key.verify_strict(&message, &signature).is_err());
        assert!(rejection(&transaction, CHAIN).contains("invalid native V3 signature"));
    }
}

#[test]
fn malformed_public_key_and_noncanonical_signature_encodings_are_rejected() {
    let (_, original) = signed(19, 32, 0);
    // Select a definitely non-decompressible encoding with the actual decoder,
    // rather than assuming that an all-ones Edwards encoding is invalid.
    let invalid_key = (0..=u8::MAX)
        .map(|byte| [byte; 32])
        .find(|bytes| VerifyingKey::from_bytes(bytes).is_err())
        .expect("fixture contains a non-decompressible Edwards encoding");
    let mut invalid = original.clone();
    invalid.from = invalid_key.to_vec();
    invalid.signature[..32].copy_from_slice(&invalid_key);
    assert!(rejection(&invalid, CHAIN).contains("invalid Ed25519 public key"));

    // Scalar S=L (the subgroup order), not a canonical scalar in [0,L).
    let order = [
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
        0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
    ];
    let mut noncanonical_s = original.clone();
    noncanonical_s.signature[64..96].copy_from_slice(&order);
    assert!(rejection(&noncanonical_s, CHAIN).contains("invalid native V3 signature"));

    // Noncanonical compressed R with y=p+1, an alternative identity encoding.
    let mut noncanonical_r = original;
    let mut encoded_r = [0xff; 32];
    encoded_r[0] = 0xee;
    encoded_r[31] = 0x7f;
    noncanonical_r.signature[32..64].copy_from_slice(&encoded_r);
    assert!(rejection(&noncanonical_r, CHAIN).contains("invalid native V3 signature"));
}

#[test]
fn admission_rejects_every_tested_non_96_byte_signature_payload() {
    let (_, mut transaction) = signed(20, 20, 0);
    transaction.signature.clear();
    let mut prefix = encode_transfer_v3(&transaction).unwrap();
    // Signature is the last postcard field. Replace only its canonical empty
    // slice with a canonical wrong-sized slice; do not bypass auth by merely
    // asserting the public encoder's own length check.
    assert_eq!(prefix.pop(), Some(0));
    for length in [0, 1, 32, 64, 95, 97, 128] {
        let mut raw = prefix.clone();
        raw.extend(postcard::to_allocvec(&vec![0_u8; length]).unwrap());
        let error = authenticate_transfer_v3(&raw, CHAIN, MAX_BYTES)
            .err()
            .expect("wrong signature payload length must fail");
        assert!(
            error.to_string().contains("96-byte"),
            "length {length}: {error:#}"
        );
    }
}

#[test]
fn ordered_nonce_planning_shares_aliases_and_preserves_independent_signers() {
    let transactions = [
        checked(21, 20, 0),
        checked(22, 32, 7),
        checked(21, 32, 1),
        checked(22, 20, 8),
    ];
    let a = transactions[0].nonce_identity();
    let b = transactions[1].nonce_identity();
    let parent = BTreeMap::from([(a, 0), (b, 7)]);
    let original = parent.clone();
    let transitions = check_nonce_sequence(&transactions, &parent).unwrap();
    assert_eq!(
        transitions,
        vec![
            NonceTransition {
                identity: a,
                before: 0,
                after: 1
            },
            NonceTransition {
                identity: b,
                before: 7,
                after: 8
            },
            NonceTransition {
                identity: a,
                before: 1,
                after: 2
            },
            NonceTransition {
                identity: b,
                before: 8,
                after: 9
            },
        ]
    );
    assert_eq!(parent, original);
    assert!(check_nonce_sequence(&[], &BTreeMap::new())
        .unwrap()
        .is_empty());
}

#[test]
fn replay_gap_and_missing_parent_fail_the_entire_nonce_plan_without_reserving_prefix() {
    let identity = checked(23, 20, 0).nonce_identity();
    let parent = BTreeMap::from([(identity, 0)]);
    let original = parent.clone();
    for nonces in [[0, 0], [0, 2], [1, 2]] {
        let transactions = [checked(23, 20, nonces[0]), checked(23, 32, nonces[1])];
        let result = check_nonce_sequence(&transactions, &parent);
        assert!(result.is_err(), "alias replay or gap {nonces:?}");
        assert_eq!(parent, original);
    }
    // The first signer has a valid prefix; the second lacks an explicit parent.
    let transactions = [checked(23, 20, 0), checked(24, 32, 0)];
    assert!(check_nonce_sequence(&transactions, &parent)
        .unwrap_err()
        .to_string()
        .contains("parent input missing"));
    assert_eq!(parent, original);
    assert!(check_nonce_sequence(&transactions[..1], &BTreeMap::new())
        .unwrap_err()
        .to_string()
        .contains("parent input missing"));
    assert_eq!(
        check_nonce_sequence(&transactions[..1], &parent).unwrap()[0].before,
        0
    );
    // A failed earlier attempt left no hidden reservation or partially advanced
    // nonce: the complete correct sequence can still start at the same parent.
    let correct = [checked(23, 20, 0), checked(23, 32, 1)];
    assert_eq!(check_nonce_sequence(&correct, &parent).unwrap().len(), 2);
    let consumed_parent = BTreeMap::from([(identity, 1)]);
    assert!(check_nonce_sequence(&correct, &consumed_parent).is_err());
}

#[test]
fn authenticated_values_are_owned_send_sync_inputs_not_live_handles() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SignatureCheckedTransfer>();
    assert_send_sync::<NonceTransition>();
    let transaction = {
        let (_, unsigned_lifetime) = signed(25, 32, 0);
        let raw = encode_transfer_v3(&unsigned_lifetime).unwrap();
        authenticate_transfer_v3(&raw, CHAIN, MAX_BYTES).unwrap()
    };
    let expected = transaction.tx_hash();
    let joined = std::thread::spawn(move || {
        assert_eq!(transaction.transfer().nonce, 0);
        transaction.tx_hash()
    })
    .join()
    .unwrap();
    assert_eq!(joined, expected);
}
