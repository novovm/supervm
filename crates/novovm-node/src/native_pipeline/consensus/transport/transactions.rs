//! Versioned best-effort raw transaction dissemination, not a body or decision.
//! Session/sequence provide only a bounded replay hint, never durable identity.
use super::*;

const VERSION: u16 = 1;
const FIXED_BYTES: usize = PREFIX_BYTES + 2 + 8 + 32 + 32 + 8 + 32 + 32 + 8 + 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransactionsScope {
    pub chain_id: u64,
    pub genesis: Hash,
    pub protocol: Hash,
    pub epoch: u64,
    pub validator_set_hash: Hash,
    /// Fresh random process generation, not a network session or signer log.
    pub session: Hash,
    pub sequence: u64,
}

impl TransactionsScope {
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            self.chain_id != 0 && self.sequence != 0,
            "empty input gossip domain/sequence"
        );
        for value in [
            self.genesis,
            self.protocol,
            self.validator_set_hash,
            self.session,
        ] {
            ensure!(value != [0; 32], "empty input gossip commitment/session");
        }
        Ok(())
    }
}

pub(super) fn append(
    out: &mut Vec<u8>,
    scope: &TransactionsScope,
    raw: &[Vec<u8>],
    limits: DecodeLimits,
) -> Result<()> {
    scope.validate_shape()?;
    ensure!(
        !raw.is_empty() && raw.len() <= limits.transactions,
        "input gossip count exceeds bound"
    );
    let mut bytes = 0usize;
    for tx in raw {
        ensure!(
            !tx.is_empty() && tx.len() <= limits.transaction_bytes,
            "input gossip transaction exceeds bound"
        );
        bytes = bytes
            .checked_add(tx.len())
            .context("input gossip size overflow")?;
        ensure!(
            bytes <= limits.body_bytes,
            "input gossip body exceeds bound"
        );
    }
    let size = raw
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(bytes))
        .and_then(|n| n.checked_add(FIXED_BYTES))
        .context("input gossip size overflow")?;
    ensure!(
        size <= limits.message_bytes,
        "input gossip message exceeds bound"
    );
    append_scope(out, scope);
    out.extend_from_slice(&u32::try_from(raw.len())?.to_be_bytes());
    for tx in raw {
        out.extend_from_slice(&u32::try_from(tx.len())?.to_be_bytes());
        out.extend_from_slice(tx);
    }
    Ok(())
}

fn append_scope(out: &mut Vec<u8>, scope: &TransactionsScope) {
    out.extend_from_slice(&VERSION.to_be_bytes());
    out.extend_from_slice(&scope.chain_id.to_be_bytes());
    out.extend_from_slice(&scope.genesis);
    out.extend_from_slice(&scope.protocol);
    out.extend_from_slice(&scope.epoch.to_be_bytes());
    out.extend_from_slice(&scope.validator_set_hash);
    out.extend_from_slice(&scope.session);
    out.extend_from_slice(&scope.sequence.to_be_bytes());
}

fn read_scope(reader: &mut Reader<'_>) -> Result<TransactionsScope> {
    ensure!(
        reader.take(2)? == VERSION.to_be_bytes(),
        "input gossip version mismatch"
    );
    let scope = TransactionsScope {
        chain_id: reader.u64()?,
        genesis: reader.hash()?,
        protocol: reader.hash()?,
        epoch: reader.u64()?,
        validator_set_hash: reader.hash()?,
        session: reader.hash()?,
        sequence: reader.u64()?,
    };
    scope.validate_shape()?;
    Ok(scope)
}

pub(super) fn decode(reader: &mut Reader<'_>, limits: DecodeLimits) -> Result<Message> {
    let scope = read_scope(reader)?;
    let count = reader.u32()? as usize;
    ensure!(
        count > 0 && count <= limits.transactions && count <= reader.remaining() / 5,
        "input gossip count exceeds input/budget"
    );
    // Validate every length and the complete retained charge before allocation.
    let mut scan = reader.clone();
    let mut bytes = 0usize;
    for _ in 0..count {
        let tx = scan.blob(limits.transaction_bytes)?;
        ensure!(!tx.is_empty(), "empty input gossip transaction");
        bytes = bytes
            .checked_add(tx.len())
            .context("input gossip size overflow")?;
        ensure!(
            bytes <= limits.body_bytes,
            "input gossip body exceeds bound"
        );
    }
    ensure!(scan.remaining() == 0, "input gossip trailing bytes");
    let mut raw_transactions = Vec::with_capacity(count);
    for _ in 0..count {
        raw_transactions.push(reader.blob(limits.transaction_bytes)?.to_vec());
    }
    Ok(Message::Transactions {
        scope,
        raw_transactions,
    })
}

pub(super) fn append_taken(
    out: &mut Vec<u8>,
    scope: &TransactionsScope,
    fragment_id: &Hash,
) -> Result<()> {
    scope.validate_shape()?;
    ensure!(
        *fragment_id != [0; 32],
        "empty input gossip credit identity"
    );
    append_scope(out, scope);
    out.extend_from_slice(fragment_id);
    Ok(())
}

pub(super) fn decode_taken(reader: &mut Reader<'_>) -> Result<Message> {
    let scope = read_scope(reader)?;
    let fragment_id = reader.hash()?;
    ensure!(fragment_id != [0; 32], "empty input gossip credit identity");
    Ok(Message::TransactionsTaken { scope, fragment_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn specimen() -> (Message, DecodeLimits) {
        (
            Message::Transactions {
                scope: TransactionsScope {
                    chain_id: 3,
                    genesis: [1; 32],
                    protocol: [2; 32],
                    epoch: 4,
                    validator_set_hash: [3; 32],
                    session: [4; 32],
                    sequence: 1,
                },
                raw_transactions: vec![vec![1, 2], vec![3, 4, 5]],
            },
            DecodeLimits {
                transactions: 2,
                transaction_bytes: 4,
                body_bytes: 6,
                message_bytes: 512,
            },
        )
    }
    #[test]
    fn raw_and_taken_roundtrip_are_versioned_distinct_and_exact() -> Result<()> {
        let (raw, limits) = specimen();
        let Message::Transactions { scope, .. } = raw.clone() else {
            unreachable!()
        };
        let taken = Message::TransactionsTaken {
            scope,
            fragment_id: [8; 32],
        };
        for (message, tag, lane) in [(raw, 9, 2), (taken, 10, 0)] {
            let encoded = encode(&message, limits)?;
            assert_eq!(encoded[10], tag);
            assert_eq!(&encoded[11..13], &1u16.to_be_bytes());
            assert_eq!(message_lane(&encoded[..11])?, lane);
            assert_eq!(super::super::decode(&encoded, limits)?, message);
            for len in 0..encoded.len() {
                assert!(super::super::decode(&encoded[..len], limits).is_err());
            }
            let mut wrong = encoded.clone();
            wrong[12] = 2;
            assert!(super::super::decode(&wrong, limits).is_err());
            let mut extra = encoded;
            extra.push(0);
            assert!(super::super::decode(&extra, limits).is_err());
        }
        Ok(())
    }
    #[test]
    fn raw_count_lengths_and_empty_identity_fail_closed() -> Result<()> {
        let (message, limits) = specimen();
        let encoded = encode(&message, limits)?;
        for restricted in [
            DecodeLimits {
                transactions: 1,
                ..limits
            },
            DecodeLimits {
                body_bytes: 4,
                ..limits
            },
            DecodeLimits {
                transaction_bytes: 2,
                ..limits
            },
        ] {
            assert!(encode(&message, restricted).is_err());
            assert!(super::super::decode(&encoded, restricted).is_err());
        }
        let mut forged = encoded;
        forged[FIXED_BYTES - 4..FIXED_BYTES].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(super::super::decode(&forged, limits).is_err());
        let Message::Transactions { mut scope, .. } = message else {
            unreachable!()
        };
        scope.sequence = 0;
        assert!(encode(
            &Message::TransactionsTaken {
                scope,
                fragment_id: [8; 32]
            },
            limits
        )
        .is_err());
        scope.sequence = 1;
        assert!(encode(
            &Message::TransactionsTaken {
                scope,
                fragment_id: [0; 32]
            },
            limits
        )
        .is_err());
        Ok(())
    }
}
