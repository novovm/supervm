use super::*;
use ed25519_dalek::{Signer, SigningKey};

// Frozen owned schema and preimages copied locally from the reviewed old V3
// implementation, independently of the production borrowed codec/specialized
// digest helpers. No test imports or builds the isolated legacy repository.
#[derive(Serialize)]
struct FrozenFee {
    pay_asset: String,
    max_pay_amount: u128,
    slippage_bps: u32,
}

#[derive(Serialize)]
struct FrozenTransfer {
    from: Vec<u8>,
    to: Vec<u8>,
    asset: String,
    amount: u128,
    nonce: u64,
    fee_policy: FrozenFee,
}

#[derive(Serialize)]
enum FrozenKind {
    Transfer(FrozenTransfer),
}

#[derive(Serialize)]
struct FrozenNativeWire {
    chain_id: u64,
    kind: FrozenKind,
    signature: Vec<u8>,
}

fn frozen_wire(tx: &TransferV3, signature: &[u8]) -> Vec<u8> {
    let old = FrozenNativeWire {
        chain_id: tx.chain_id,
        kind: FrozenKind::Transfer(FrozenTransfer {
            from: tx.from.clone(),
            to: tx.to.clone(),
            asset: tx.asset.clone(),
            amount: tx.amount,
            nonce: tx.nonce,
            fee_policy: FrozenFee {
                pay_asset: tx.fee_policy.pay_asset.clone(),
                max_pay_amount: tx.fee_policy.max_pay_amount,
                slippage_bps: tx.fee_policy.slippage_bps,
            },
        }),
        signature: signature.to_vec(),
    };
    let mut out = b"NNX1\x03".to_vec();
    out.extend(postcard::to_allocvec(&old).unwrap());
    out
}

fn framed(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value);
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn frozen_digests(tx: &TransferV3) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let unsigned = frozen_wire(tx, &[]);
    let mut intent_bytes = b"novovm-native-tx-unsigned-commitment-v3".to_vec();
    intent_bytes.extend_from_slice(&(unsigned.len() as u64).to_le_bytes());
    intent_bytes.extend_from_slice(&unsigned);
    let intent = digest(&intent_bytes);
    let mut data = b"novovm-native-signed-intent-v3\0".to_vec();
    data.extend_from_slice(&intent);
    data.extend_from_slice(tx.asset.as_bytes());

    // Old ir.rs::TxIR::compute_hash, after node tx_ingress added that envelope.
    let mut hash_input = tx.from.clone();
    hash_input.extend_from_slice(&tx.to);
    hash_input.extend_from_slice(&tx.amount.to_le_bytes());
    hash_input.extend_from_slice(&tx.nonce.to_le_bytes());
    hash_input.extend_from_slice(&data);
    let tx_hash = digest(&hash_input);

    // Old shared adapter signing encoder, with every Transfer TxIR field fixed
    // exactly as in nov_native_tx_to_adapter_tx_ir_v1 (including None markers).
    let mut message = b"novovm_adapter_tx_sig_v2".to_vec();
    message.extend_from_slice(&tx.chain_id.to_le_bytes());
    message.push(0); // Transfer
    message.extend_from_slice(&tx.nonce.to_le_bytes());
    message.extend_from_slice(&tx.amount.to_le_bytes());
    message.extend_from_slice(&21_000_u64.to_le_bytes());
    message.extend_from_slice(&1_u64.to_le_bytes());
    framed(&mut message, &tx.from);
    message.push(0); // account_id None
    message.push(0); // fee_owner_account_id None
    message.push(0); // nonce_owner_account_id None
    message.push(1); // to Some
    framed(&mut message, &tx.to);
    framed(&mut message, &data);
    message.push(0); // Standard execution policy
    message.extend_from_slice(&0_u64.to_le_bytes()); // access list length
    message.push(0); // source_chain None
    message.push(0); // target_chain None
    framed(&mut message, &tx_hash);
    (intent, tx_hash, digest(&message))
}

fn fixture() -> TransferV3 {
    TransferV3 {
        chain_id: 1,
        from: vec![0x11; 20],
        to: vec![0x22; 32],
        asset: "NOV".into(),
        amount: 128,
        nonce: 0,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 128,
            slippage_bps: 0,
        },
        signature: vec![0xab; 96],
    }
}

fn mutations(tx: &TransferV3) -> Vec<TransferV3> {
    let mut variants = Vec::new();
    macro_rules! change {
        ($field:ident, $value:expr) => {{
            let mut changed = tx.clone();
            changed.$field = $value;
            variants.push(changed);
        }};
    }
    change!(chain_id, tx.chain_id + 1);
    change!(from, vec![0x45; 20]);
    change!(to, vec![0x46; 32]);
    change!(asset, "USDT".into());
    change!(amount, tx.amount + 1);
    change!(nonce, tx.nonce + 1);
    let mut changed = tx.clone();
    changed.fee_policy.pay_asset = "USDT".into();
    variants.push(changed);
    let mut changed = tx.clone();
    changed.fee_policy.max_pay_amount += 1;
    variants.push(changed);
    let mut changed = tx.clone();
    changed.fee_policy.slippage_bps += 1;
    variants.push(changed);
    variants
}

#[test]
fn existing_postcard_transfer_golden_bytes_are_unchanged() {
    let mut tx = fixture();
    tx.signature.clear();
    // Handwritten existing postcard layout: header, chain=1, kind=0,
    // from length20, to length32, then asset/amount/nonce/fee/signature.
    let mut expected = b"NNX1\x03\x01\x00\x14".to_vec();
    expected.extend_from_slice(&[0x11; 20]);
    expected.push(32);
    expected.extend_from_slice(&[0x22; 32]);
    expected.extend_from_slice(b"\x03NOV\x80\x01\x00\x03NOV\x80\x01\x00\x00");
    assert_eq!(encode_transfer_v3(&tx).unwrap(), expected);
    assert_eq!(frozen_wire(&tx, &[]), expected);
    // An unsigned wallet encoding cannot cross the signed admission boundary.
    assert!(decode_transfer_v3(&expected, expected.len()).is_err());
    expected.pop();
    expected.push(96);
    expected.extend_from_slice(&[0xab; 96]);
    tx.signature = vec![0xab; 96];
    assert_eq!(encode_transfer_v3(&tx).unwrap(), expected);
    assert_eq!(decode_transfer_v3(&expected, expected.len()).unwrap(), tx);
}

#[test]
fn frozen_old_wire_and_all_three_preimages_match_at_integer_boundaries() {
    for from_len in [20, 32] {
        for to_len in [20, 32] {
            for amount in [0, 1, 127, 128, u128::from(u64::MAX) + 1, u128::MAX] {
                for nonce in [0, 127, 128, u64::MAX] {
                    let mut tx = fixture();
                    tx.from.resize(from_len, 0x31);
                    tx.to.resize(to_len, 0x32);
                    tx.chain_id = u64::MAX;
                    tx.amount = amount;
                    tx.nonce = nonce;
                    tx.fee_policy.max_pay_amount = amount;
                    tx.fee_policy.slippage_bps = u32::MAX;
                    let expected = frozen_wire(&tx, &tx.signature);
                    let (intent, hash, message) = frozen_digests(&tx);
                    assert_eq!(encode_transfer_v3(&tx).unwrap(), expected);
                    assert_eq!(decode_transfer_v3(&expected, expected.len()).unwrap(), tx);
                    assert_eq!(unsigned_commitment(&tx).unwrap(), intent);
                    assert_eq!(canonical_tx_hash(&tx).unwrap(), hash);
                    assert_eq!(signing_message(&tx).unwrap(), message);
                }
            }
        }
    }
}

#[test]
fn every_transfer_field_is_in_the_complete_signed_intent() {
    let tx = fixture();
    let initial = frozen_digests(&tx);
    for changed in mutations(&tx) {
        let old = frozen_digests(&changed);
        assert_ne!(old.0, initial.0);
        assert_ne!(old.1, initial.1);
        assert_ne!(old.2, initial.2);
        assert_eq!(unsigned_commitment(&changed).unwrap(), old.0);
        assert_eq!(canonical_tx_hash(&changed).unwrap(), old.1);
        assert_eq!(signing_message(&changed).unwrap(), old.2);
    }
}

#[test]
fn genuine_old_oracle_signatures_verify_new_messages_and_reject_tampering() {
    let signer = SigningKey::from_bytes(&[7; 32]);
    for public_key_alias in [false, true] {
        let mut tx = fixture();
        let public_key = signer.verifying_key().to_bytes();
        tx.from = if public_key_alias {
            public_key.to_vec()
        } else {
            Sha256::digest(public_key)[12..32].to_vec()
        };
        let old_message = frozen_digests(&tx).2;
        let signature = signer.sign(&old_message);
        tx.signature = public_key.to_vec();
        tx.signature.extend_from_slice(&signature.to_bytes());
        let old_wire = frozen_wire(&tx, &tx.signature);
        let migrated = decode_transfer_v3(&old_wire, old_wire.len()).unwrap();
        signer
            .verifying_key()
            .verify_strict(&signing_message(&migrated).unwrap(), &signature)
            .unwrap();
        assert_eq!(encode_transfer_v3(&migrated).unwrap(), old_wire);
        for changed in mutations(&migrated) {
            assert!(signer
                .verifying_key()
                .verify_strict(&signing_message(&changed).unwrap(), &signature)
                .is_err());
        }
    }
}

#[test]
fn signature_bytes_are_excluded_but_not_confused_with_a_signed_admission() {
    let tx = fixture();
    let expected = frozen_digests(&tx);
    for signature in [vec![], vec![0; 96], vec![0xff; 96]] {
        let mut changed = tx.clone();
        changed.signature = signature;
        assert_eq!(unsigned_commitment(&changed).unwrap(), expected.0);
        assert_eq!(canonical_tx_hash(&changed).unwrap(), expected.1);
        assert_eq!(signing_message(&changed).unwrap(), expected.2);
    }
    assert_ne!(expected.0, expected.1);
    assert_ne!(expected.0, expected.2);
    assert_ne!(expected.1, expected.2);
    assert_ne!(digest(&frozen_wire(&tx, &tx.signature)), expected.1);
}

#[test]
fn borrowed_fields_preserve_utf8_empty_and_long_symbols_without_normalization() {
    for asset in [String::new(), "nOv\0资产".into(), "x".repeat(2048)] {
        let mut tx = fixture();
        tx.asset = asset;
        tx.fee_policy.pay_asset = " \0NOV/费用 ".into();
        let raw = frozen_wire(&tx, &tx.signature);
        assert_eq!(decode_transfer_v3(&raw, raw.len()).unwrap(), tx);
        assert!(decode_transfer_v3(&raw, raw.len() - 1).is_err());
        assert_eq!(signing_message(&tx).unwrap(), frozen_digests(&tx).2);
    }
}

#[test]
fn decoder_rejects_wrong_versions_kinds_magic_and_trailing_bytes() {
    let raw = frozen_wire(&fixture(), &[0xab; 96]);
    for version in [0, 1, 2, 4, 255] {
        let mut bad = raw.clone();
        bad[4] = version;
        assert!(decode_transfer_v3(&bad, bad.len()).is_err());
    }
    for tag in [1, 2, 3, 127, 255] {
        let mut bad = raw.clone();
        bad[6] = tag; // chain_id=1 occupies one byte; tag follows.
        assert!(decode_transfer_v3(&bad, bad.len()).is_err());
    }
    let mut bad = raw.clone();
    bad[0] ^= 1;
    assert!(decode_transfer_v3(&bad, bad.len()).is_err());
    let mut bad = raw.clone();
    bad.push(0);
    assert!(decode_transfer_v3(&bad, bad.len()).is_err());
    for end in 0..raw.len() {
        assert!(decode_transfer_v3(&raw[..end], raw.len()).is_err());
    }
    assert!(decode_transfer_v3(&raw, 0).is_err());
}

#[test]
fn noncanonical_varints_are_rejected_not_given_a_second_raw_identity() {
    let raw = frozen_wire(&fixture(), &[0xab; 96]);
    for offset in [5, 6, 7] {
        let mut bad = raw.clone();
        // Overlong chain id, Transfer enum tag, or from-vector length.
        bad[offset] |= 0x80;
        bad.insert(offset + 1, 0);
        assert!(decode_transfer_v3(&bad, bad.len()).is_err());
    }
}

#[test]
fn hostile_declared_lengths_do_not_require_owned_field_allocation() {
    // Input body promises a from field of usize::MAX bytes, but contains none.
    let mut bad = b"NNX1\x03\x01\x00".to_vec();
    bad.extend(postcard::to_allocvec(&usize::MAX).unwrap());
    assert!(decode_transfer_v3(&bad, bad.len()).is_err());
    // A nonterminating/overflowing integer is rejected too.
    bad.extend_from_slice(&[0xff; 32]);
    assert!(decode_transfer_v3(&bad, bad.len()).is_err());
}

#[test]
fn invalid_account_or_signature_sizes_cannot_cross_decode() {
    for len in [0, 19, 21, 31, 33, 1024] {
        let mut tx = fixture();
        tx.from = vec![1; len];
        let raw = frozen_wire(&tx, &tx.signature);
        assert!(decode_transfer_v3(&raw, raw.len()).is_err());
        assert!(encode_transfer_v3(&tx).is_err());
        tx = fixture();
        tx.to = vec![2; len];
        let raw = frozen_wire(&tx, &tx.signature);
        assert!(decode_transfer_v3(&raw, raw.len()).is_err());
    }
    for len in [0, 32, 64, 95, 97, 1024] {
        let tx = fixture();
        let raw = frozen_wire(&tx, &vec![3; len]);
        assert!(decode_transfer_v3(&raw, raw.len()).is_err());
    }
    for len in [32, 64, 95, 97] {
        let mut tx = fixture();
        tx.signature.resize(len, 3);
        assert!(encode_transfer_v3(&tx).is_err());
    }
}

#[test]
fn invalid_utf8_is_rejected_in_borrowed_decode() {
    let tx = fixture();
    let mut raw = frozen_wire(&tx, &tx.signature);
    let offset = raw.windows(3).position(|part| part == b"NOV").unwrap();
    raw[offset] = 0xff;
    assert!(decode_transfer_v3(&raw, raw.len()).is_err());
}

#[test]
fn bounded_native_v3_seeded_malformed_input_never_panics() {
    let raw = frozen_wire(&fixture(), &[0xab; 96]);
    let mut state = 0x921a_e117_932b_f131_u64;
    for sample in 0..512 {
        let mut bytes = if sample % 2 == 0 {
            raw.clone()
        } else {
            vec![0; sample % 256]
        };
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            if state & 7 == 0 {
                *byte = state as u8;
            }
        }
        let outcome = std::panic::catch_unwind(|| decode_transfer_v3(&bytes, 1024));
        assert!(outcome.is_ok());
    }
}
