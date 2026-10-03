use super::super::authentication::authenticate_transfer_v3;
use super::super::wire::{
    canonical_tx_hash, decode_transfer_v3, encode_transfer_v3, signing_message, FeePolicy,
    TransferV3,
};
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

fn limits() -> ApflLimits {
    ApflLimits {
        transactions: 2048,
        transaction_bytes: 4096,
        body_bytes: 8 * 1024 * 1024,
    }
}

fn signed(seed: u8, nonce: u64) -> TransferV3 {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let public = key.verifying_key().to_bytes();
    let mut tx = TransferV3 {
        chain_id: 71,
        from: if seed.is_multiple_of(2) {
            Sha256::digest(public)[12..].to_vec()
        } else {
            public.to_vec()
        },
        to: vec![7 + seed; if seed.is_multiple_of(2) { 20 } else { 32 }],
        asset: if seed.is_multiple_of(2) {
            "NOV".into()
        } else {
            "nETH/资产".into()
        },
        amount: 17 + u128::from(nonce),
        nonce,
        fee_policy: FeePolicy {
            pay_asset: if seed.is_multiple_of(2) {
                "NOV".into()
            } else {
                "ALT".into()
            },
            max_pay_amount: 200 + u128::from(nonce),
            slippage_bps: u32::from(seed),
        },
        signature: Vec::new(),
    };
    tx.signature = public.to_vec();
    tx.signature
        .extend_from_slice(&key.sign(&signing_message(&tx).unwrap()).to_bytes());
    tx
}

fn fixture() -> Vec<Vec<u8>> {
    [2, 3, 2, 3, 2]
        .into_iter()
        .enumerate()
        .map(|(i, seed)| encode_transfer_v3(&signed(seed, i as u64)).unwrap())
        .collect()
}

#[test]
fn heterogeneous_real_v3_rows_and_original_digests_round_trip() {
    let raw = fixture();
    let batch = ApflTransferBatch::from_raw(&raw, limits()).unwrap();
    let bytes = batch.encode().unwrap();
    let decoded = ApflTransferBatch::decode(&bytes, limits()).unwrap();
    assert_eq!(batch, decoded);
    assert_eq!(decoded.encode().unwrap(), bytes);
    assert_eq!(decoded.len(), raw.len());
    assert_eq!(
        decoded.canonical_bytes(),
        raw.iter().map(Vec::len).sum::<usize>()
    );
    assert_eq!(
        decoded.max_transaction_bytes(),
        raw.iter().map(Vec::len).max().unwrap()
    );
    for (i, raw) in raw.iter().enumerate() {
        let owned = decode_transfer_v3(raw, 4096).unwrap();
        let view = decoded.row(i).unwrap();
        assert_eq!(view, owned.as_view());
        assert_eq!(view.encode().unwrap(), *raw);
        assert_eq!(decoded.canonical_raw(i).unwrap(), *raw);
        assert_eq!(view.encoded_len().unwrap(), raw.len());
        assert_eq!(
            view.canonical_tx_hash().unwrap(),
            canonical_tx_hash(&owned).unwrap()
        );
        assert_eq!(
            view.signing_message().unwrap(),
            signing_message(&owned).unwrap()
        );
        assert_eq!(view.signature, owned.signature);
        authenticate_transfer_v3(&decoded.canonical_raw(i).unwrap(), 71, 4096).unwrap();
    }
    assert!(decoded.row(raw.len()).is_err());
    assert!(Arc::ptr_eq(&batch.0, &batch.clone().0));
}

#[test]
fn shared_real_signed_fields_compress_without_a_fixture_generator() {
    let raw: Vec<_> = (0..1024)
        .map(|nonce| encode_transfer_v3(&signed(4, nonce)).unwrap())
        .collect();
    let batch = ApflTransferBatch::from_raw(&raw, limits()).unwrap();
    assert!(batch.encode().unwrap().len() < batch.canonical_bytes());
    assert_eq!(batch.0.from.values.len(), 1);
    assert_eq!(batch.0.to.values.len(), 1);
    assert_eq!(batch.0.amount.values.len(), raw.len());
    assert_eq!(batch.0.signatures.len(), raw.len() * 96);
    // Highest fields, zero amounts and distinct asset/fee strings are values,
    // not a constrained benchmark lane or regenerated business defaults.
    let mut edge = signed(2, u64::MAX);
    edge.amount = u128::MAX;
    edge.fee_policy.max_pay_amount = u128::MAX;
    edge.fee_policy.slippage_bps = u32::MAX;
    edge.asset.clear();
    edge.fee_policy.pay_asset = "arbitrary fee".into();
    let bytes = encode_transfer_v3(&edge).unwrap();
    let edge_batch = ApflTransferBatch::from_raw(std::slice::from_ref(&bytes), limits()).unwrap();
    assert_eq!(edge_batch.canonical_raw(0).unwrap(), bytes);
    // Codec preserves even semantically unacceptable input. Authentication,
    // exhausted nonce and execution policy remain the real admission boundary.
    assert!(authenticate_transfer_v3(&bytes, 71, 4096).is_err());
}

fn column_start(bytes: &[u8], selected: usize) -> usize {
    let mut reader = Reader { bytes, offset: 26 };
    for (index, kind) in [
        Kind::U64,
        Kind::Account,
        Kind::Account,
        Kind::Text,
        Kind::U128,
        Kind::U64,
        Kind::Text,
        Kind::U128,
        Kind::U32,
    ]
    .into_iter()
    .enumerate()
    {
        if index == selected {
            return reader.offset;
        }
        scan_column(&mut reader, 5, kind, limits().transaction_bytes).unwrap();
    }
    unreachable!()
}

#[test]
fn changed_shared_index_residual_and_signature_never_reuse_authority() {
    let raw = fixture();
    let original = ApflTransferBatch::from_raw(&raw, limits())
        .unwrap()
        .encode()
        .unwrap();
    let mut changed = original.clone();
    let chain = column_start(&changed, 0) + 4;
    changed[chain] = 72;
    let decoded = ApflTransferBatch::decode(&changed, limits()).unwrap();
    assert!(authenticate_transfer_v3(&decoded.canonical_raw(0).unwrap(), 72, 4096).is_err());

    // Recipient has two dictionary values (20 and 32 bytes). Switch one row
    // after both values were introduced; representation remains canonical.
    let mut changed = original.clone();
    let start = column_start(&changed, 2);
    let mut cursor = Reader {
        bytes: &changed,
        offset: start + 4,
    };
    cursor.account().unwrap();
    cursor.account().unwrap();
    let index = cursor.offset + 4 * 4;
    changed[index..index + 4].copy_from_slice(&1u32.to_le_bytes());
    // Different address width changes the declared maximum/total. Do not let
    // stale metadata through; re-encode a legitimately changed logical batch.
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    let mut logical = raw.clone();
    let mut tx = decode_transfer_v3(&logical[4], 4096).unwrap();
    tx.to = decode_transfer_v3(&logical[1], 4096).unwrap().to;
    logical[4] = encode_transfer_v3(&tx).unwrap();
    let altered = ApflTransferBatch::from_raw(&logical, limits()).unwrap();
    assert!(authenticate_transfer_v3(&altered.canonical_raw(4).unwrap(), 71, 4096).is_err());

    let mut changed = original.clone();
    let residual = column_start(&changed, 4) + 4;
    changed[residual] = 99; // unique dense amount, same V3 varint width
    let altered = ApflTransferBatch::decode(&changed, limits()).unwrap();
    assert!(authenticate_transfer_v3(&altered.canonical_raw(0).unwrap(), 71, 4096).is_err());

    let mut changed = original.clone();
    *changed.last_mut().unwrap() ^= 1;
    let altered = ApflTransferBatch::decode(&changed, limits()).unwrap();
    assert_eq!(
        altered.row(4).unwrap().canonical_tx_hash().unwrap(),
        decode_transfer_v3(&raw[4], 4096)
            .unwrap()
            .as_view()
            .canonical_tx_hash()
            .unwrap()
    );
    assert!(authenticate_transfer_v3(&altered.canonical_raw(4).unwrap(), 71, 4096).is_err());
}

#[test]
fn rejects_noncanonical_dictionaries_indexes_versions_and_truncation() {
    let original = ApflTransferBatch::from_raw(&fixture(), limits())
        .unwrap()
        .encode()
        .unwrap();
    let mut changed = original.clone();
    changed[8] = 2;
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    let mut changed = original.clone();
    changed.push(0);
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    for length in [0, 8, 25, original.len() - 1, original.len() - 96] {
        assert!(ApflTransferBatch::decode(&original[..length], limits()).is_err());
    }
    let mut changed = original.clone();
    changed[26..30].copy_from_slice(&0u32.to_le_bytes());
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    let start = column_start(&original, 1);
    let mut cursor = Reader {
        bytes: &original,
        offset: start + 4,
    };
    cursor.account().unwrap();
    cursor.account().unwrap();
    let indexes = cursor.offset;
    for value in [1u32, 2, u32::MAX] {
        let mut changed = original.clone();
        changed[indexes..indexes + 4].copy_from_slice(&value.to_le_bytes());
        assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    }
    let mut changed = original.clone();
    for index in 0..5 {
        changed[indexes + index * 4..indexes + (index + 1) * 4]
            .copy_from_slice(&0u32.to_le_bytes());
    }
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    // Dense residuals must really be distinct, not alternate encodings of a
    // shared/dictionary column. All dictionaries reject duplicated values.
    let mut changed = original.clone();
    let amount = column_start(&original, 4) + 4;
    let value = changed[amount..amount + 16].to_vec();
    changed[amount + 16..amount + 32].copy_from_slice(&value);
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
}

#[test]
fn expanded_budget_cannot_be_bypassed_by_shared_values_or_false_header() {
    let raw = fixture();
    let batch = ApflTransferBatch::from_raw(&raw, limits()).unwrap();
    let bytes = batch.encode().unwrap();
    let exact = ApflLimits {
        transactions: raw.len(),
        transaction_bytes: batch.max_transaction_bytes(),
        body_bytes: batch.canonical_bytes(),
    };
    assert!(ApflTransferBatch::decode(&bytes, exact).is_ok());
    for small in [
        ApflLimits {
            transactions: raw.len() - 1,
            ..exact
        },
        ApflLimits {
            transaction_bytes: exact.transaction_bytes - 1,
            ..exact
        },
        ApflLimits {
            body_bytes: exact.body_bytes - 1,
            ..exact
        },
    ] {
        assert!(ApflTransferBatch::from_raw(&raw, small).is_err());
        assert!(ApflTransferBatch::decode(&bytes, small).is_err());
    }
    let mut changed = bytes.clone();
    changed[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    let mut changed = bytes.clone();
    changed[14..22].copy_from_slice(&(batch.canonical_bytes() as u64 - 1).to_le_bytes());
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    let mut changed = bytes.clone();
    changed[22..26].copy_from_slice(&(batch.max_transaction_bytes() as u32 - 1).to_le_bytes());
    assert!(ApflTransferBatch::decode(&changed, limits()).is_err());
    assert!(ApflTransferBatch::from_raw(&[], limits()).is_err());
    let mut unsigned = signed(2, 0);
    unsigned.signature.clear();
    assert!(
        ApflTransferBatch::from_raw(&[encode_transfer_v3(&unsigned).unwrap()], limits()).is_err()
    );
}

#[test]
fn strict_v3_view_preserves_minimum_and_rejects_overlong_scalar() {
    let tx = TransferV3 {
        chain_id: 0,
        from: vec![0; 20],
        to: vec![0; 20],
        asset: String::new(),
        amount: 0,
        nonce: 0,
        fee_policy: FeePolicy {
            pay_asset: String::new(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: vec![0; 96],
    };
    let raw = encode_transfer_v3(&tx).unwrap();
    assert_eq!(raw.len(), MIN_RAW_BYTES);
    assert_eq!(
        decode_transfer_view_v3(&raw, raw.len()).unwrap(),
        tx.as_view()
    );
    let mut overlong = raw[..5].to_vec();
    overlong.extend_from_slice(&[0x80, 0]);
    overlong.extend_from_slice(&raw[6..]);
    assert!(decode_transfer_view_v3(&overlong, overlong.len()).is_err());
    let batch = ApflTransferBatch::from_raw(std::slice::from_ref(&raw), limits()).unwrap();
    assert_eq!(
        ApflTransferBatch::decode(&batch.encode().unwrap(), limits())
            .unwrap()
            .canonical_raw(0)
            .unwrap(),
        raw
    );
}
