//! APFL transfer batches on the existing opaque transport. The structured wire
//! identity is NOT the canonical transaction/body identity. Local decoding only
//! checks representation and bounds; every receiver still verifies signatures.
use super::*;

const VERSION: u16 = 1;

fn batch_limits(limits: DecodeLimits) -> ApflLimits {
    ApflLimits {
        transactions: limits.transactions,
        transaction_bytes: limits.transaction_bytes,
        body_bytes: limits.body_bytes,
    }
}

fn validate_batch(batch: &ApflTransferBatch, limits: DecodeLimits) -> Result<()> {
    limits.validate()?;
    ensure!(
        !batch.is_empty() && batch.len() <= limits.transactions,
        "APFL transaction count exceeds budget or empty"
    );
    ensure!(
        batch.max_transaction_bytes() <= limits.transaction_bytes
            && batch.canonical_bytes() <= limits.body_bytes,
        "APFL canonical body exceeds byte budget"
    );
    Ok(())
}

fn canonical_size(batch: &ApflTransferBatch, fixed: usize) -> Result<usize> {
    batch
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(batch.canonical_bytes()))
        .and_then(|n| n.checked_add(fixed))
        .context("APFL canonical envelope size overflow")
}

/// Match old Body's exact ordered V3 commitment by streaming its original wire
/// bytes directly from borrowed rows, without a canonical transaction Vec.
pub fn apfl_body_id(
    context: &BatchContext,
    batch: &ApflTransferBatch,
    limits: DecodeLimits,
) -> Result<Hash> {
    validate_batch(batch, limits)?;
    let size = canonical_size(batch, CONTEXT_BYTES + 4)?;
    ensure!(
        size.checked_add(PREFIX_BYTES)
            .context("body size overflow")?
            <= limits.message_bytes,
        "body envelope exceeds message budget"
    );
    let mut context_bytes = Vec::with_capacity(CONTEXT_BYTES);
    append_context(&mut context_bytes, context);
    let mut digest = Sha256::new();
    digest.update(b"novovm-round-bft-transport/v1/body\0");
    digest.update(u64::try_from(size)?.to_be_bytes());
    digest.update(context_bytes);
    digest.update(u32::try_from(batch.len())?.to_be_bytes());
    canonical_rows(&mut digest, batch)?;
    Ok(digest.finalize().into())
}

pub fn apfl_early_body_id(
    scope: &EarlyBodyScope,
    batch: &ApflTransferBatch,
    limits: DecodeLimits,
) -> Result<Hash> {
    validate_batch(batch, limits)?;
    scope.validate_shape()?;
    let size = canonical_size(batch, 2 + early::SCOPE_BYTES + 4)?;
    ensure!(
        size.checked_add(PREFIX_BYTES)
            .context("early body size overflow")?
            <= limits.message_bytes,
        "early body envelope exceeds message budget"
    );
    let mut scope_bytes = Vec::with_capacity(early::SCOPE_BYTES);
    early::append_scope(&mut scope_bytes, scope);
    let mut digest = Sha256::new();
    digest.update(early::ID_DOMAIN);
    digest.update(u64::try_from(size)?.to_be_bytes());
    digest.update(VERSION.to_be_bytes());
    digest.update(scope_bytes);
    digest.update(u32::try_from(batch.len())?.to_be_bytes());
    canonical_rows(&mut digest, batch)?;
    Ok(digest.finalize().into())
}

fn canonical_rows(digest: &mut Sha256, batch: &ApflTransferBatch) -> Result<()> {
    for index in 0..batch.len() {
        let row = batch.row(index)?;
        digest.update(u32::try_from(row.encoded_len()?)?.to_be_bytes());
        row.update_canonical_digest(digest)?;
    }
    Ok(())
}

fn append_batch(out: &mut Vec<u8>, batch: &ApflTransferBatch, limits: DecodeLimits) -> Result<()> {
    validate_batch(batch, limits)?;
    let encoded = batch.encode()?;
    ensure!(
        out.len()
            .checked_add(encoded.len())
            .context("APFL message size overflow")?
            <= limits.message_bytes,
        "APFL message exceeds byte budget"
    );
    out.extend_from_slice(&encoded);
    Ok(())
}

pub(super) fn append_body(
    out: &mut Vec<u8>,
    context: &BatchContext,
    batch: &ApflTransferBatch,
    limits: DecodeLimits,
) -> Result<()> {
    out.extend_from_slice(&VERSION.to_be_bytes());
    append_context(out, context);
    append_batch(out, batch, limits)
}

pub(super) fn append_early(
    out: &mut Vec<u8>,
    scope: &EarlyBodyScope,
    batch: &ApflTransferBatch,
    limits: DecodeLimits,
) -> Result<()> {
    scope.validate_shape()?;
    out.extend_from_slice(&VERSION.to_be_bytes());
    early::append_scope(out, scope);
    append_batch(out, batch, limits)
}

pub(super) fn append_transactions(
    out: &mut Vec<u8>,
    scope: &TransactionsScope,
    batch: &ApflTransferBatch,
    limits: DecodeLimits,
) -> Result<()> {
    scope.validate_shape()?;
    // Existing scope codec already includes the explicit u16 payload version.
    transactions::append_scope(out, scope);
    append_batch(out, batch, limits)
}

fn read_batch(reader: &mut Reader<'_>, limits: DecodeLimits) -> Result<Arc<ApflTransferBatch>> {
    let batch = ApflTransferBatch::decode(reader.take(reader.remaining())?, batch_limits(limits))?;
    validate_batch(&batch, limits)?;
    Ok(Arc::new(batch))
}

pub(super) fn decode(tag: u8, reader: &mut Reader<'_>, limits: DecodeLimits) -> Result<Message> {
    match tag {
        11 => {
            ensure!(
                reader.u16()? == VERSION,
                "APFL body payload version mismatch"
            );
            let context = read_context(reader)?;
            Ok(Message::ApflBody {
                context,
                batch: read_batch(reader, limits)?,
            })
        }
        12 => {
            let scope = early::read_scope(reader)?;
            Ok(Message::ApflEarlyBody {
                scope,
                batch: read_batch(reader, limits)?,
            })
        }
        13 => {
            let scope = transactions::read_scope(reader)?;
            Ok(Message::ApflTransactions {
                scope,
                batch: read_batch(reader, limits)?,
            })
        }
        _ => anyhow::bail!("unknown APFL host message kind"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
    use crate::native_pipeline::ingress::wire::{
        encode_transfer_v3, signing_message, FeePolicy, TransferV3,
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn fixture() -> (
        BatchContext,
        EarlyBodyScope,
        Vec<Vec<u8>>,
        Arc<ApflTransferBatch>,
        DecodeLimits,
    ) {
        let context = BatchContext {
            chain_id: 77,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            business_program: [3; 32],
            semantic_version: 4,
            effect_contract: [5; 32],
            parent_block_hash: [6; 32],
            parent_height: 1,
            parent_state_root: [8; 32],
            parent_receipt_root: [9; 32],
            parent_state_version: 10,
            receipt_codec: [11; 32],
            height: 2,
            slot: 12,
            timestamp_unix_ms: 13,
        };
        let scope = EarlyBodyScope {
            source: wire::Context {
                chain_id: 77,
                genesis_config_commitment: [1; 32],
                protocol_commitment: [2; 32],
                epoch: 1,
                validator_set_hash: [3; 32],
                height: 1,
                parent_block_hash: [0; 32],
                parent_decision_hash: [0; 32],
            },
            source_round: 4,
            target_height: 2,
        };
        let limits = DecodeLimits {
            transactions: 8,
            transaction_bytes: 1024,
            body_bytes: 8192,
            message_bytes: 16384,
        };
        let raw = (0u8..4)
            .map(|index| {
                let key = SigningKey::from_bytes(&[11 + index; 32]);
                let mut tx = TransferV3 {
                    chain_id: 77,
                    from: key.verifying_key().to_bytes().to_vec(),
                    to: vec![21 + index; if index % 2 == 0 { 20 } else { 32 }],
                    asset: if index % 2 == 0 { "NOV" } else { "USD" }.into(),
                    amount: if index == 3 {
                        u128::MAX
                    } else {
                        7 + u128::from(index)
                    },
                    nonce: 129 + u64::from(index) * 997,
                    fee_policy: FeePolicy {
                        pay_asset: if index % 2 == 0 { "NOV" } else { "USD" }.into(),
                        max_pay_amount: u128::MAX - u128::from(index),
                        slippage_bps: u32::from(index) * 23,
                    },
                    signature: Vec::new(),
                };
                tx.signature
                    .extend_from_slice(key.verifying_key().as_bytes());
                tx.signature
                    .extend_from_slice(&key.sign(&signing_message(&tx).unwrap()).to_bytes());
                encode_transfer_v3(&tx).unwrap()
            })
            .collect::<Vec<_>>();
        let batch = Arc::new(ApflTransferBatch::from_raw(&raw, batch_limits(limits)).unwrap());
        (context, scope, raw, batch, limits)
    }

    #[test]
    fn structured_wire_preserves_raw_body_and_early_commitments_with_distinct_fragment_ids(
    ) -> Result<()> {
        let (context, scope, raw, batch, limits) = fixture();
        assert_eq!(
            apfl_body_id(&context, &batch, limits)?,
            body_id(&context, &raw, limits)?
        );
        assert_eq!(
            apfl_early_body_id(&scope, &batch, limits)?,
            early_body_id(&scope, &raw, limits)?
        );
        let domain = fragment_domain(
            context.chain_id,
            context.genesis_config_commitment,
            context.protocol_commitment,
        );
        let old = Message::Body {
            context,
            raw_transactions: raw.clone(),
        };
        let structured = Message::ApflBody {
            context,
            batch: batch.clone(),
        };
        assert_ne!(
            prepare_message(domain, &old, limits)?.id(),
            prepare_message(domain, &structured, limits)?.id()
        );
        for (index, expected) in raw.iter().enumerate() {
            assert_eq!(batch.canonical_raw(index)?, *expected);
            // Codec fields are not substituted from a performance fixture.
            authenticate_transfer_v3(expected, 77, 1024)?;
        }
        let mut changed = raw.clone();
        changed.swap(0, 1);
        let reordered = ApflTransferBatch::from_raw(&changed, batch_limits(limits))?;
        assert_ne!(
            apfl_body_id(&context, &batch, limits)?,
            apfl_body_id(&context, &reordered, limits)?
        );
        let mut forged = raw;
        *forged[0].last_mut().unwrap() ^= 1;
        let unauthenticated = ApflTransferBatch::from_raw(&forged, batch_limits(limits))?;
        assert_ne!(
            apfl_body_id(&context, &batch, limits)?,
            apfl_body_id(&context, &unauthenticated, limits)?
        );
        assert!(authenticate_transfer_v3(&unauthenticated.canonical_raw(0)?, 77, 1024).is_err());
        Ok(())
    }

    #[test]
    fn structured_tags_versions_truncation_trailing_and_canonical_budgets_are_strict() -> Result<()>
    {
        let (context, scope, _raw, batch, limits) = fixture();
        let messages = [
            (
                11,
                1,
                Message::ApflBody {
                    context,
                    batch: batch.clone(),
                },
            ),
            (
                12,
                1,
                Message::ApflEarlyBody {
                    scope,
                    batch: batch.clone(),
                },
            ),
            (
                13,
                2,
                Message::ApflTransactions {
                    scope: TransactionsScope {
                        chain_id: 77,
                        genesis: [1; 32],
                        protocol: [2; 32],
                        epoch: 1,
                        validator_set_hash: [3; 32],
                        session: [4; 32],
                        sequence: 1,
                    },
                    batch: batch.clone(),
                },
            ),
        ];
        for (tag, lane, message) in messages {
            let encoded = encode(&message, limits)?;
            assert_eq!(encoded[10], tag);
            assert_eq!(&encoded[11..13], &1u16.to_be_bytes());
            assert_eq!(message_lane(&encoded)?, lane);
            assert_eq!(super::super::decode(&encoded, limits)?, message);
            for len in 0..encoded.len() {
                assert!(super::super::decode(&encoded[..len], limits).is_err());
            }
            let mut version = encoded.clone();
            version[12] = 2;
            assert!(super::super::decode(&version, limits).is_err());
            let mut trailing = encoded.clone();
            trailing.push(0);
            assert!(super::super::decode(&trailing, limits).is_err());
            for restricted in [
                DecodeLimits {
                    transactions: 3,
                    ..limits
                },
                DecodeLimits {
                    transaction_bytes: batch.max_transaction_bytes() - 1,
                    ..limits
                },
                DecodeLimits {
                    body_bytes: batch.canonical_bytes() - 1,
                    ..limits
                },
            ] {
                assert!(encode(&message, restricted).is_err());
                assert!(super::super::decode(&encoded, restricted).is_err());
            }
        }
        Ok(())
    }
}
