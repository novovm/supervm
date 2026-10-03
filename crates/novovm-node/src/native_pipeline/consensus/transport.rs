//! `round-bft-transport/v1`: NEW development-only opaque network messages.
//!
//! Call encoding, body hashing and decoding on the assembly/ingress owner, NOT
//! the consensus poll loop. These operations deliberately inspect a whole body.
//! The bounded fragment layer moves bytes; neither its digest nor `body_id`
//! authenticates business inputs. A proposal's body reference is untrusted and
//! unsigned metadata. Before voting the host must independently pin its context,
//! construct BatchRequest with LOCAL fee policy, execute the original bytes and
//! compare the resulting BlockStatement. This codec grants no such authority.

use super::wire::{self, Hash, Proposal, Quorum, Vote};
use crate::native_pipeline::execution::plan::BatchContext;
use anyhow::{ensure, Context, Result};
use novovm_network::duplex::fragments::{OutgoingMessage, MAX_MESSAGE_BYTES};
use sha2::{Digest, Sha256};

pub const PROTOCOL: &str = "round-bft-transport/v1";
const MAGIC: &[u8; 8] = b"NVHOSTN1";
const PREFIX_BYTES: usize = 11;
const CONTEXT_BYTES: usize = 308;

mod early;
pub use early::{early_body_id, EarlyBodyScope};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Body {
        context: BatchContext,
        raw_transactions: Vec<Vec<u8>>,
    },
    Proposal {
        proposal: Proposal,
        valid_quorum: Option<Quorum>,
        body_id: Hash,
    },
    Vote(Vote),
    Decision {
        proposal: Proposal,
        certificate: Quorum,
        body_id: Hash,
    },
    RequestBody {
        body_id: Hash,
    },
    /// Ask for an already decided block extending THIS exact parent. This is
    /// a development-v1 message extension (tag 6); older decoders reject it.
    /// A request grants neither archive authority nor permission to sign.
    RequestDecision {
        context: wire::Context,
    },
    /// Optional parent-independent raw input, never a candidate or a vote.
    /// Tags 7/8 carry their own explicit payload version; old decoders reject.
    EarlyBody {
        scope: EarlyBodyScope,
        raw_transactions: Vec<Vec<u8>>,
    },
    /// A small reference to one immutable announcement. The controller must
    /// check the origin/round and authorize this exact parent independently.
    BindBody {
        scope: EarlyBodyScope,
        announcement_id: Hash,
        context: BatchContext,
    },
}

/// Operational decoder ceilings, not activated production block parameters.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub transactions: usize,
    pub transaction_bytes: usize,
    pub body_bytes: usize,
    pub message_bytes: usize,
}

impl DecodeLimits {
    fn validate(self) -> Result<()> {
        ensure!(
            (1..=65_536).contains(&self.transactions)
                && self.transaction_bytes > 0
                && self.transaction_bytes <= self.body_bytes
                && self.body_bytes <= self.message_bytes
                && (PREFIX_BYTES..=MAX_MESSAGE_BYTES).contains(&self.message_bytes),
            "invalid host network codec limits"
        );
        Ok(())
    }
}

/// Derive once from the locally pinned chain, never from an incoming message.
pub fn fragment_domain(chain_id: u64, genesis: Hash, protocol: Hash) -> Hash {
    let mut digest = Sha256::new();
    digest.update(b"novovm-round-bft-transport/v1/fragments\0");
    digest.update(chain_id.to_be_bytes());
    digest.update(genesis);
    digest.update(protocol);
    digest.finalize().into()
}

/// Full-body work; call once on the assembly owner and retain the immutable
/// prepared message for bounded frame retries. Its ID is a fragment ID, not a
/// BodyId, executed candidate, block value or consensus decision identity.
pub fn prepare_message(
    domain: Hash,
    message: &Message,
    limits: DecodeLimits,
) -> Result<OutgoingMessage> {
    OutgoingMessage::new(domain, encode(message, limits)?, limits.message_bytes)
}

/// Commits every context field and every ordered original transaction byte.
/// Transport budgets do not enter the identity; no peer-supplied tx hash does.
pub fn body_id(
    context: &BatchContext,
    raw_transactions: &[Vec<u8>],
    limits: DecodeLimits,
) -> Result<Hash> {
    limits.validate()?;
    let mut bytes = Vec::new();
    append_body(&mut bytes, context, raw_transactions, limits)?;
    let mut digest = Sha256::new();
    digest.update(b"novovm-round-bft-transport/v1/body\0");
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
    Ok(digest.finalize().into())
}

pub fn encode(message: &Message, limits: DecodeLimits) -> Result<Vec<u8>> {
    limits.validate()?;
    let mut out = Vec::from(MAGIC.as_slice());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.push(match message {
        Message::Body { .. } => 1,
        Message::Proposal { .. } => 2,
        Message::Vote(_) => 3,
        Message::Decision { .. } => 4,
        Message::RequestBody { .. } => 5,
        Message::RequestDecision { .. } => 6,
        Message::EarlyBody { .. } => 7,
        Message::BindBody { .. } => 8,
    });
    match message {
        Message::Body {
            context,
            raw_transactions,
        } => append_body(&mut out, context, raw_transactions, limits)?,
        Message::Proposal {
            proposal,
            valid_quorum,
            body_id,
        } => {
            append_blob(&mut out, &wire::encode_proposal(proposal)?, limits)?;
            out.push(u8::from(valid_quorum.is_some()));
            if let Some(quorum) = valid_quorum {
                append_blob(&mut out, &wire::encode_quorum(quorum)?, limits)?;
            }
            out.extend_from_slice(body_id);
        }
        Message::Vote(vote) => append_blob(&mut out, &wire::encode_vote(vote)?, limits)?,
        Message::Decision {
            proposal,
            certificate,
            body_id,
        } => {
            append_blob(&mut out, &wire::encode_proposal(proposal)?, limits)?;
            append_blob(&mut out, &wire::encode_quorum(certificate)?, limits)?;
            out.extend_from_slice(body_id);
        }
        Message::RequestBody { body_id } => out.extend_from_slice(body_id),
        Message::RequestDecision { context } => {
            context.validate_shape()?;
            out.extend_from_slice(&context.chain_id.to_be_bytes());
            out.extend_from_slice(&context.genesis_config_commitment);
            out.extend_from_slice(&context.protocol_commitment);
            out.extend_from_slice(&context.epoch.to_be_bytes());
            out.extend_from_slice(&context.validator_set_hash);
            out.extend_from_slice(&context.height.to_be_bytes());
            out.extend_from_slice(&context.parent_block_hash);
            out.extend_from_slice(&context.parent_decision_hash);
        }
        Message::EarlyBody {
            scope,
            raw_transactions,
        } => {
            early::append_body(&mut out, scope, raw_transactions, limits)?;
        }
        Message::BindBody {
            scope,
            announcement_id,
            context,
        } => {
            early::append_bind(&mut out, scope, announcement_id, context, limits)?;
        }
    }
    ensure!(
        out.len() <= limits.message_bytes,
        "host network message exceeds byte budget"
    );
    Ok(out)
}

/// Allocation-free lane classification for the ingress owner's reservation.
/// This checks only the wire prefix; it grants no decode or signing authority.
pub(crate) fn body_prefix(bytes: &[u8]) -> Result<bool> {
    ensure!(bytes.len() >= PREFIX_BYTES, "truncated host network prefix");
    ensure!(
        &bytes[..8] == MAGIC && bytes[8..10] == 1u16.to_be_bytes(),
        "host network protocol/version mismatch"
    );
    match bytes[10] {
        1 | 7 => Ok(true),
        2..=6 | 8 => Ok(false),
        _ => anyhow::bail!("unknown host network message kind"),
    }
}

pub fn decode(bytes: &[u8], limits: DecodeLimits) -> Result<Message> {
    limits.validate()?;
    ensure!(
        bytes.len() >= PREFIX_BYTES && bytes.len() <= limits.message_bytes,
        "invalid host network message length"
    );
    let mut reader = Reader { bytes, offset: 0 };
    ensure!(
        reader.take(8)? == MAGIC && reader.take(2)? == 1u16.to_be_bytes(),
        "host network protocol/version mismatch"
    );
    let message = match reader.byte()? {
        1 => {
            let context = read_context(&mut reader)?;
            let count = reader.u32()? as usize;
            ensure!(
                count > 0 && count <= limits.transactions && count <= reader.remaining() / 5,
                "transaction count exceeds input or budget"
            );
            // Preflight the entire body BEFORE allocating a transaction vector
            // or any raw transaction. A forged count cannot reserve huge memory.
            let mut scan = reader.clone();
            let mut total = 0usize;
            for _ in 0..count {
                let raw = scan.blob(limits.transaction_bytes)?;
                ensure!(!raw.is_empty(), "empty raw transaction");
                total = total
                    .checked_add(raw.len())
                    .context("body length overflow")?;
                ensure!(total <= limits.body_bytes, "raw body exceeds byte budget");
            }
            ensure!(scan.remaining() == 0, "host body has trailing bytes");
            let mut raw_transactions = Vec::with_capacity(count);
            for _ in 0..count {
                raw_transactions.push(reader.blob(limits.transaction_bytes)?.to_vec());
            }
            Message::Body {
                context,
                raw_transactions,
            }
        }
        2 => {
            let proposal = wire::decode_proposal(reader.blob(wire::MAX_WIRE_BYTES)?)?;
            let valid_quorum = match reader.byte()? {
                0 => None,
                1 => Some(wire::decode_quorum(reader.blob(wire::MAX_WIRE_BYTES)?)?),
                _ => anyhow::bail!("noncanonical optional quorum tag"),
            };
            Message::Proposal {
                proposal,
                valid_quorum,
                body_id: reader.hash()?,
            }
        }
        3 => Message::Vote(wire::decode_vote(reader.blob(wire::MAX_WIRE_BYTES)?)?),
        4 => Message::Decision {
            proposal: wire::decode_proposal(reader.blob(wire::MAX_WIRE_BYTES)?)?,
            certificate: wire::decode_quorum(reader.blob(wire::MAX_WIRE_BYTES)?)?,
            body_id: reader.hash()?,
        },
        5 => Message::RequestBody {
            body_id: reader.hash()?,
        },
        6 => {
            let context = wire::Context {
                chain_id: reader.u64()?,
                genesis_config_commitment: reader.hash()?,
                protocol_commitment: reader.hash()?,
                epoch: reader.u64()?,
                validator_set_hash: reader.hash()?,
                height: reader.u64()?,
                parent_block_hash: reader.hash()?,
                parent_decision_hash: reader.hash()?,
            };
            context.validate_shape()?;
            Message::RequestDecision { context }
        }
        7 => early::decode_body(&mut reader, limits)?,
        8 => early::decode_bind(&mut reader)?,
        _ => anyhow::bail!("unknown host network message kind"),
    };
    ensure!(
        reader.remaining() == 0,
        "host network message has trailing bytes"
    );
    Ok(message)
}

fn append_blob(out: &mut Vec<u8>, bytes: &[u8], limits: DecodeLimits) -> Result<()> {
    ensure!(
        bytes.len() <= wire::MAX_WIRE_BYTES,
        "signed evidence exceeds wire bound"
    );
    let size = out
        .len()
        .checked_add(4)
        .and_then(|n| n.checked_add(bytes.len()))
        .context("message size overflow")?;
    ensure!(
        size <= limits.message_bytes,
        "host network message exceeds byte budget"
    );
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn append_body(
    out: &mut Vec<u8>,
    context: &BatchContext,
    raw: &[Vec<u8>],
    limits: DecodeLimits,
) -> Result<()> {
    ensure!(
        !raw.is_empty() && raw.len() <= limits.transactions,
        "transaction count exceeds budget or empty"
    );
    let total = raw.iter().try_fold(0usize, |sum, tx| {
        ensure!(
            !tx.is_empty() && tx.len() <= limits.transaction_bytes,
            "raw transaction exceeds size budget or empty"
        );
        sum.checked_add(tx.len()).context("raw body size overflow")
    })?;
    ensure!(total <= limits.body_bytes, "raw body exceeds byte budget");
    let size = raw
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(CONTEXT_BYTES + 4 + PREFIX_BYTES))
        .and_then(|n| n.checked_add(total))
        .context("body envelope size overflow")?;
    ensure!(
        size <= limits.message_bytes,
        "body envelope exceeds message budget"
    );
    append_context(out, context);
    out.extend_from_slice(&u32::try_from(raw.len())?.to_be_bytes());
    for tx in raw {
        out.extend_from_slice(&u32::try_from(tx.len())?.to_be_bytes());
        out.extend_from_slice(tx);
    }
    Ok(())
}

fn append_context(out: &mut Vec<u8>, c: &BatchContext) {
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

fn read_context(reader: &mut Reader<'_>) -> Result<BatchContext> {
    Ok(BatchContext {
        chain_id: reader.u64()?,
        genesis_config_commitment: reader.hash()?,
        protocol_commitment: reader.hash()?,
        business_program: reader.hash()?,
        semantic_version: reader.u32()?,
        effect_contract: reader.hash()?,
        parent_block_hash: reader.hash()?,
        parent_height: reader.u64()?,
        parent_state_root: reader.hash()?,
        parent_receipt_root: reader.hash()?,
        parent_state_version: reader.u64()?,
        receipt_codec: reader.hash()?,
        height: reader.u64()?,
        slot: reader.u64()?,
        timestamp_unix_ms: reader.u64()?,
    })
}

#[derive(Clone)]
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        ensure!(length <= self.remaining(), "truncated host network message");
        let start = self.offset;
        self.offset += length;
        Ok(&self.bytes[start..self.offset])
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn hash(&mut self) -> Result<Hash> {
        Ok(self.take(32)?.try_into()?)
    }
    fn blob(&mut self, max: usize) -> Result<&'a [u8]> {
        let length = self.u32()? as usize;
        ensure!(length <= max, "host network field exceeds byte budget");
        self.take(length)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "transport/early_tests.rs"]
mod early_tests;
