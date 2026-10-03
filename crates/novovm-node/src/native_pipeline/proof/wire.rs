//! Lengths are checked against remaining input and fixed guest resource limits
//! before allocation. No untrusted serialization of execution-capability types.

use super::*;

const MAGIC: &[u8; 8] = b"NVEXIN01";

pub(super) struct Input<'a> {
    pub context: BatchContext,
    pub raw: Vec<Vec<u8>>,
    pub policy: DirectNovFeePolicy,
    pub witness: &'a [u8],
}

pub(super) fn encode(
    plan: &BatchPlan,
    policy: &DirectNovFeePolicy,
    witness: &[u8],
) -> Result<Vec<u8>> {
    policy.validate()?;
    ensure!(
        plan.declared_access().len() <= capture_budget().keys,
        "proof access set exceeds bound"
    );
    let policy = postcard::to_allocvec(policy)?;
    ensure!(
        policy.len() <= MAX_POLICY_BYTES,
        "proof policy size exceeds bound"
    );
    let raw = plan.raw_transactions();
    ensure!(
        !raw.is_empty() && raw.len() <= MAX_TRANSACTIONS,
        "proof transaction count exceeds bound"
    );
    let mut body_bytes = 0usize;
    for bytes in raw {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_TRANSACTION_BYTES,
            "proof raw size exceeds bound"
        );
        body_bytes = body_bytes
            .checked_add(bytes.len())
            .context("proof body overflow")?;
        ensure!(body_bytes <= MAX_BODY_BYTES, "proof body exceeds bound");
    }
    // Fixed context is 8 hashes, 6 u64s and one u32. Bounds precede allocation.
    let total = 8usize + 8 * 32 + 6 * 8 + 4 + 4;
    let total = total
        .checked_add(raw.len().checked_mul(4).context("proof framing overflow")?)
        .and_then(|n| n.checked_add(body_bytes))
        .and_then(|n| n.checked_add(4 + policy.len()))
        .and_then(|n| n.checked_add(4))
        .and_then(|n| n.checked_add(witness.len()))
        .context("proof input overflow")?;
    ensure!(total <= MAX_INPUT_BYTES, "proof input exceeds bound");
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    put_context(&mut out, plan.context());
    out.extend_from_slice(&u32::try_from(raw.len())?.to_be_bytes());
    for bytes in raw {
        framed(&mut out, bytes)?;
    }
    framed(&mut out, &policy)?;
    framed(&mut out, witness)?;
    ensure!(out.len() == total, "proof input framing mismatch");
    Ok(out)
}

fn put_context(out: &mut Vec<u8>, c: &BatchContext) {
    out.extend_from_slice(&c.chain_id.to_be_bytes());
    out.extend_from_slice(&c.genesis_config_commitment);
    out.extend_from_slice(&c.protocol_commitment);
    out.extend_from_slice(&c.business_program);
    out.extend_from_slice(&c.semantic_version.to_be_bytes());
    out.extend_from_slice(&c.effect_contract);
    out.extend_from_slice(&c.parent_block_hash);
    out.extend_from_slice(&c.parent_height.to_be_bytes());
    out.extend_from_slice(&c.parent_state_root);
    out.extend_from_slice(&c.parent_receipt_root);
    out.extend_from_slice(&c.parent_state_version.to_be_bytes());
    out.extend_from_slice(&c.receipt_codec);
    out.extend_from_slice(&c.height.to_be_bytes());
    out.extend_from_slice(&c.slot.to_be_bytes());
    out.extend_from_slice(&c.timestamp_unix_ms.to_be_bytes());
}

fn framed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

pub(super) fn decode(input: &[u8]) -> Result<Input<'_>> {
    ensure!(input.len() <= MAX_INPUT_BYTES, "proof input exceeds bound");
    let mut r = Reader { tail: input };
    ensure!(r.take(8)? == MAGIC, "proof input version mismatch");
    let context = BatchContext {
        chain_id: r.u64()?,
        genesis_config_commitment: r.hash()?,
        protocol_commitment: r.hash()?,
        business_program: r.hash()?,
        semantic_version: r.u32()?,
        effect_contract: r.hash()?,
        parent_block_hash: r.hash()?,
        parent_height: r.u64()?,
        parent_state_root: r.hash()?,
        parent_receipt_root: r.hash()?,
        parent_state_version: r.u64()?,
        receipt_codec: r.hash()?,
        height: r.u64()?,
        slot: r.u64()?,
        timestamp_unix_ms: r.u64()?,
    };
    let count = usize::try_from(r.u32()?)?;
    ensure!(
        count > 0 && count <= MAX_TRANSACTIONS && count <= r.tail.len() / 5,
        "proof transaction count exceeds bound or truncated"
    );
    let mut raw = Vec::with_capacity(count);
    let mut body_bytes = 0usize;
    for _ in 0..count {
        let bytes = r.framed(MAX_TRANSACTION_BYTES)?;
        ensure!(!bytes.is_empty(), "empty proof transaction");
        body_bytes = body_bytes
            .checked_add(bytes.len())
            .context("proof body overflow")?;
        ensure!(body_bytes <= MAX_BODY_BYTES, "proof body exceeds bound");
        raw.push(bytes.to_vec());
    }
    let policy_bytes = r.framed(MAX_POLICY_BYTES)?;
    // Postcard borrows string bytes only after checking they exist in the input;
    // this structure has no unbounded sequence/map fields.
    let (policy, rest): (DirectNovFeePolicy, _) = postcard::take_from_bytes(policy_bytes)?;
    ensure!(
        rest.is_empty() && postcard::to_allocvec(&policy)? == policy_bytes,
        "noncanonical proof policy"
    );
    policy.validate()?;
    let witness = r.framed(MAX_INPUT_BYTES)?;
    ensure!(r.tail.is_empty(), "proof input trailing bytes");
    Ok(Input {
        context,
        raw,
        policy,
        witness,
    })
}

struct Reader<'a> {
    tail: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        ensure!(count <= self.tail.len(), "truncated proof input");
        let (head, tail) = self.tail.split_at(count);
        self.tail = tail;
        Ok(head)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn hash(&mut self) -> Result<NodeHash> {
        Ok(self.take(32)?.try_into()?)
    }
    fn framed(&mut self, maximum: usize) -> Result<&'a [u8]> {
        let size = usize::try_from(self.u32()?)?;
        ensure!(size <= maximum, "proof field exceeds bound");
        self.take(size)
    }
}
