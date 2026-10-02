//! Bounded optional announcement codec. Shape checks do not authenticate the
//! sender, pin its live round, approve a parent, or verify any transaction.

use super::*;

const VERSION: u16 = 1;
const SCOPE_BYTES: usize = 3 * 8 + 5 * 32 + 2 * 8;
const BODY_FIXED_BYTES: usize = PREFIX_BYTES + 2 + SCOPE_BYTES + 4;
const BIND_BYTES: usize = PREFIX_BYTES + 2 + SCOPE_BYTES + 32 + CONTEXT_BYTES;
const ID_DOMAIN: &[u8] = b"novovm-round-bft-transport/v1/early-body/v1\0";

/// One source consensus generation and its immediately following height. The
/// target round is always zero; neither a future range nor a parent root is
/// implied. This carries no E2E session or cached execution authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EarlyBodyScope {
    pub source: wire::Context,
    pub source_round: u64,
    pub target_height: u64,
}

impl EarlyBodyScope {
    pub fn validate_shape(&self) -> Result<()> {
        self.source.validate_shape()?;
        ensure!(
            self.target_height
                == self
                    .source
                    .height
                    .checked_add(1)
                    .context("early body source height exhausted")?,
            "early body must target the immediately following height"
        );
        Ok(())
    }

    /// Check only the reference's static shape and source domain. This does
    /// not authenticate a sender or approve the claimed parent/state/program.
    pub fn validate_binding(&self, id: &Hash, context: &BatchContext) -> Result<()> {
        self.validate_shape()?;
        ensure!(*id != [0; 32], "empty early announcement identity");
        ensure!(
            context.chain_id == self.source.chain_id
                && context.genesis_config_commitment == self.source.genesis_config_commitment
                && context.protocol_commitment == self.source.protocol_commitment,
            "early binding changes its source chain domain"
        );
        ensure!(
            context.height == self.target_height && context.parent_height == self.source.height,
            "early binding height differs from announcement scope"
        );
        // Parent hashes/roots, business/effect pins and slot/time are still claims.
        // Only the controller and exact-parent compiler may authorize them.
        Ok(())
    }
}

fn body_size(scope: &EarlyBodyScope, raw: &[Vec<u8>], limits: DecodeLimits) -> Result<usize> {
    scope.validate_shape()?;
    ensure!(
        !raw.is_empty() && raw.len() <= limits.transactions,
        "early transaction count exceeds budget or empty"
    );
    let mut total = 0usize;
    for transaction in raw {
        ensure!(
            !transaction.is_empty() && transaction.len() <= limits.transaction_bytes,
            "early raw transaction exceeds size budget or empty"
        );
        total = total
            .checked_add(transaction.len())
            .context("early body size overflow")?;
        ensure!(
            total <= limits.body_bytes,
            "early raw body exceeds byte budget"
        );
    }
    let size = raw
        .len()
        .checked_mul(4)
        .and_then(|bytes| bytes.checked_add(BODY_FIXED_BYTES))
        .and_then(|bytes| bytes.checked_add(total))
        .context("early body envelope size overflow")?;
    ensure!(
        size <= limits.message_bytes,
        "early body envelope exceeds message budget"
    );
    Ok(size)
}

/// Commits the version, complete scope, ordered count/lengths and exact raw
/// bytes. Valid local budgets do not change identity. Full-body work belongs
/// on the assembly owner; an ID is not a signature or a state-admission token.
pub fn early_body_id(
    scope: &EarlyBodyScope,
    raw: &[Vec<u8>],
    limits: DecodeLimits,
) -> Result<Hash> {
    limits.validate()?;
    let size = body_size(scope, raw, limits)?;
    let mut scope_bytes = Vec::with_capacity(SCOPE_BYTES);
    append_scope(&mut scope_bytes, scope);
    let mut digest = Sha256::new();
    digest.update(ID_DOMAIN);
    digest.update(u64::try_from(size - PREFIX_BYTES)?.to_be_bytes());
    digest.update(VERSION.to_be_bytes());
    digest.update(scope_bytes);
    digest.update(u32::try_from(raw.len())?.to_be_bytes());
    for transaction in raw {
        digest.update(u32::try_from(transaction.len())?.to_be_bytes());
        digest.update(transaction);
    }
    Ok(digest.finalize().into())
}

pub(super) fn append_body(
    out: &mut Vec<u8>,
    scope: &EarlyBodyScope,
    raw: &[Vec<u8>],
    limits: DecodeLimits,
) -> Result<()> {
    // Includes this message's actual prefix/version/scope/count, not the
    // unrelated full Body context width. Check before any body allocation.
    body_size(scope, raw, limits)?;
    out.extend_from_slice(&VERSION.to_be_bytes());
    append_scope(out, scope);
    out.extend_from_slice(&u32::try_from(raw.len())?.to_be_bytes());
    for transaction in raw {
        out.extend_from_slice(&u32::try_from(transaction.len())?.to_be_bytes());
        out.extend_from_slice(transaction);
    }
    Ok(())
}

pub(super) fn append_bind(
    out: &mut Vec<u8>,
    scope: &EarlyBodyScope,
    id: &Hash,
    context: &BatchContext,
    limits: DecodeLimits,
) -> Result<()> {
    scope.validate_binding(id, context)?;
    ensure!(
        BIND_BYTES <= limits.message_bytes,
        "early binding exceeds message budget"
    );
    out.extend_from_slice(&VERSION.to_be_bytes());
    append_scope(out, scope);
    out.extend_from_slice(id);
    append_context(out, context);
    Ok(())
}

fn append_scope(out: &mut Vec<u8>, scope: &EarlyBodyScope) {
    let source = &scope.source;
    out.extend_from_slice(&source.chain_id.to_be_bytes());
    out.extend_from_slice(&source.genesis_config_commitment);
    out.extend_from_slice(&source.protocol_commitment);
    out.extend_from_slice(&source.epoch.to_be_bytes());
    out.extend_from_slice(&source.validator_set_hash);
    out.extend_from_slice(&source.height.to_be_bytes());
    out.extend_from_slice(&source.parent_block_hash);
    out.extend_from_slice(&source.parent_decision_hash);
    out.extend_from_slice(&scope.source_round.to_be_bytes());
    out.extend_from_slice(&scope.target_height.to_be_bytes());
}

fn read_scope(reader: &mut Reader<'_>) -> Result<EarlyBodyScope> {
    ensure!(
        reader.u16()? == VERSION,
        "early body payload version mismatch"
    );
    let scope = EarlyBodyScope {
        source: wire::Context {
            chain_id: reader.u64()?,
            genesis_config_commitment: reader.hash()?,
            protocol_commitment: reader.hash()?,
            epoch: reader.u64()?,
            validator_set_hash: reader.hash()?,
            height: reader.u64()?,
            parent_block_hash: reader.hash()?,
            parent_decision_hash: reader.hash()?,
        },
        source_round: reader.u64()?,
        target_height: reader.u64()?,
    };
    scope.validate_shape()?;
    Ok(scope)
}

pub(super) fn decode_body(reader: &mut Reader<'_>, limits: DecodeLimits) -> Result<Message> {
    let scope = read_scope(reader)?;
    let count = reader.u32()? as usize;
    ensure!(
        count > 0 && count <= limits.transactions && count <= reader.remaining() / 5,
        "early transaction count exceeds input or budget"
    );
    // Preflight every field, its aggregate budget and the complete tail before
    // allocating the transaction vector or a single transaction's raw bytes.
    let mut scan = reader.clone();
    let mut total = 0usize;
    for _ in 0..count {
        let raw = scan.blob(limits.transaction_bytes)?;
        ensure!(!raw.is_empty(), "empty early raw transaction");
        total = total
            .checked_add(raw.len())
            .context("early body length overflow")?;
        ensure!(
            total <= limits.body_bytes,
            "early raw body exceeds byte budget"
        );
    }
    ensure!(scan.remaining() == 0, "early body has trailing bytes");
    let mut raw_transactions = Vec::with_capacity(count);
    for _ in 0..count {
        raw_transactions.push(reader.blob(limits.transaction_bytes)?.to_vec());
    }
    Ok(Message::EarlyBody {
        scope,
        raw_transactions,
    })
}

pub(super) fn decode_bind(reader: &mut Reader<'_>) -> Result<Message> {
    let scope = read_scope(reader)?;
    let announcement_id = reader.hash()?;
    let context = read_context(reader)?;
    scope.validate_binding(&announcement_id, &context)?;
    Ok(Message::BindBody {
        scope,
        announcement_id,
        context,
    })
}
