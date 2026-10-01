//! Bounded local journal encoding. Hash pins catch damaged/mixed records; they
//! are not a replacement for filesystem protection against rollback of an
//! entire database. Consensus signatures and retained quorum witnesses are
//! independently verified on reopen, before the signer becomes usable.

use super::*;
use sha2::{Digest, Sha256};

const SNAP: &[u8; 8] = b"NVSIGN01";
const OUT: &[u8; 8] = b"NVOUT001";
const MAX_RECORD: usize = 512 * 1024;

fn digest(bytes: &[u8]) -> Hash {
    let mut hash = Sha256::new();
    hash.update(b"novovm/replacement/signing-journal/v1\0");
    hash.update(bytes);
    hash.finalize().into()
}

fn parent_digest(parent: &ParentPoint) -> Hash {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&parent.height.to_be_bytes());
    bytes.extend_from_slice(&parent.block_hash);
    bytes.extend_from_slice(&parent.state_root);
    bytes.extend_from_slice(&parent.receipt_batch_commitment);
    bytes.extend_from_slice(&parent.state_version.to_be_bytes());
    bytes.extend_from_slice(&parent.decision_hash);
    digest(&bytes)
}

fn framed(bytes: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    bytes.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

fn seal(mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() <= MAX_RECORD - 32,
        "signing record exceeds byte budget"
    );
    bytes.extend_from_slice(&digest(&bytes));
    Ok(bytes)
}

pub(super) fn encode_snapshot(identity: &Identity, snapshot: &Snapshot) -> Result<Vec<u8>> {
    ensure!(
        snapshot.revision > 0,
        "durable signing snapshot requires revision"
    );
    ensure!(
        snapshot.state.context() == &identity.context,
        "snapshot context differs from pinned signer"
    );
    let mut bytes = SNAP.to_vec();
    bytes.extend_from_slice(&identity.validator);
    bytes.extend_from_slice(&parent_digest(&identity.parent));
    bytes.extend_from_slice(&snapshot.revision.to_be_bytes());
    match snapshot.proposed {
        None => bytes.push(0),
        Some((round, value, valid)) => {
            bytes.push(1);
            bytes.extend_from_slice(&round.to_be_bytes());
            bytes.extend_from_slice(&value);
            bytes.push(u8::from(valid.is_some()));
            bytes.extend_from_slice(&valid.unwrap_or(0).to_be_bytes());
        }
    }
    framed(&mut bytes, &snapshot.state.encode()?)?;
    let witness = snapshot
        .witness
        .as_ref()
        .map(wire::encode_quorum)
        .transpose()?
        .unwrap_or_default();
    framed(&mut bytes, &witness)?;
    seal(bytes)
}

pub(super) fn decode_snapshot(identity: &Identity, bytes: &[u8]) -> Result<Snapshot> {
    let mut reader = Reader::open(bytes, SNAP)?;
    ensure!(
        reader.hash()? == identity.validator && reader.hash()? == parent_digest(&identity.parent),
        "signing snapshot identity/parent mismatch"
    );
    let revision = reader.u64()?;
    ensure!(revision != 0, "invalid signing snapshot revision");
    let proposed = match reader.byte()? {
        0 => None,
        1 => {
            let round = reader.u64()?;
            let value = reader.hash()?;
            let has_valid = reader.byte()?;
            let valid = reader.u64()?;
            ensure!(
                value != [0; 32] && has_valid <= 1 && (has_valid != 0 || valid == 0),
                "invalid saved proposal"
            );
            let valid = (has_valid == 1).then_some(valid);
            ensure!(
                valid.is_none_or(|valid| valid < round),
                "saved proposal valid round is not earlier"
            );
            Some((round, value, valid))
        }
        _ => anyhow::bail!("unknown saved proposal tag"),
    };
    let state = State::restore(reader.frame(4096)?, &identity.context)?;
    ensure!(
        proposed.is_none_or(|(round, _, _)| round <= state.round()),
        "saved proposal is from future round"
    );
    let witness_bytes = reader.frame(wire::MAX_WIRE_BYTES)?;
    let witness = if witness_bytes.is_empty() {
        None
    } else {
        Some(wire::decode_quorum(witness_bytes)?)
    };
    match (state.valid(), &witness) {
        (None, None) => {}
        (Some((round, value)), Some(quorum)) => {
            let quorum = quorum.verify(&identity.set)?;
            ensure!(
                quorum.context() == &identity.context
                    && quorum.round() == round
                    && quorum.value() == Some(value)
                    && quorum.phase() == wire::Phase::Prevote,
                "saved valid value lacks its exact prevote quorum"
            );
        }
        _ => anyhow::bail!("saved valid value and quorum witness disagree"),
    }
    reader.finish()?;
    let snapshot = Snapshot {
        state,
        revision,
        proposed,
        witness,
    };
    ensure!(
        encode_snapshot(identity, &snapshot)? == bytes,
        "noncanonical signing snapshot"
    );
    Ok(snapshot)
}

pub(super) fn encode_outbox(
    snapshot: &Snapshot,
    saved: &[u8],
    message: Option<&DurableMessage>,
) -> Result<Vec<u8>> {
    let mut bytes = OUT.to_vec();
    bytes.extend_from_slice(&snapshot.revision.to_be_bytes());
    bytes.extend_from_slice(&digest(saved));
    match message {
        None => bytes.push(0),
        Some(DurableMessage::Proposal(proposal)) => {
            bytes.push(1);
            framed(&mut bytes, &wire::encode_proposal(proposal)?)?;
        }
        Some(DurableMessage::Vote(vote)) => {
            bytes.push(2);
            framed(&mut bytes, &wire::encode_vote(vote)?)?;
        }
        Some(DurableMessage::Decision {
            proposal,
            certificate,
        }) => {
            bytes.push(3);
            framed(&mut bytes, &wire::encode_proposal(proposal)?)?;
            framed(&mut bytes, &wire::encode_quorum(certificate)?)?;
        }
    }
    seal(bytes)
}

pub(super) fn decode_outbox(
    identity: &Identity,
    snapshot: &Snapshot,
    saved: &[u8],
    bytes: &[u8],
) -> Result<Option<DurableMessage>> {
    let mut reader = Reader::open(bytes, OUT)?;
    ensure!(
        reader.u64()? == snapshot.revision && reader.hash()? == digest(saved),
        "outbox/snapshot binding mismatch"
    );
    let message = match reader.byte()? {
        0 => {
            ensure!(
                snapshot.state.decided().is_none()
                    && (snapshot.state.step() == Step::Propose
                        || (snapshot.state.step() == Step::Precommit
                            && snapshot
                                .state
                                .valid()
                                .is_some_and(|(round, _)| round == snapshot.state.round()))),
                "state-only outbox lacks an eligible round/late-quorum transition"
            );
            None
        }
        1 => {
            let proposal = wire::decode_proposal(reader.frame(wire::MAX_WIRE_BYTES)?)?;
            proposal.verify(&identity.set)?;
            ensure!(
                proposal.context == identity.context
                    && proposal.proposer_id == identity.validator
                    && proposal.round == snapshot.state.round()
                    && snapshot.state.step() == Step::Propose
                    && snapshot.state.decided().is_none()
                    && snapshot.proposed
                        == Some((proposal.round, proposal.value, proposal.valid_round)),
                "saved proposal differs from signing state"
            );
            Some(DurableMessage::Proposal(proposal))
        }
        2 => {
            let vote = wire::decode_vote(reader.frame(wire::MAX_WIRE_BYTES)?)?;
            vote.verify(&identity.set)?;
            ensure!(
                vote.context == identity.context
                    && vote.validator_id == identity.validator
                    && vote.round == snapshot.state.round()
                    && snapshot.state.decided().is_none(),
                "saved vote differs from signer context/round"
            );
            match vote.phase {
                wire::Phase::Prevote => ensure!(
                    snapshot.state.step() == Step::Prevote,
                    "saved prevote step mismatch"
                ),
                wire::Phase::Precommit => {
                    ensure!(
                        snapshot.state.step() == Step::Precommit,
                        "saved precommit step mismatch"
                    );
                    if let Some(value) = vote.value {
                        ensure!(
                            snapshot.state.locked() == Some((vote.round, value)),
                            "saved precommit lock mismatch"
                        );
                    }
                }
            }
            Some(DurableMessage::Vote(vote))
        }
        3 => {
            let proposal = wire::decode_proposal(reader.frame(wire::MAX_WIRE_BYTES)?)?;
            let certificate = wire::decode_quorum(reader.frame(wire::MAX_WIRE_BYTES)?)?;
            proposal.verify(&identity.set)?;
            let verified = certificate.verify(&identity.set)?;
            ensure!(
                proposal.context == identity.context
                    && verified.context() == &identity.context
                    && verified.round() == proposal.round
                    && verified.phase() == wire::Phase::Precommit
                    && verified.value() == Some(proposal.value)
                    && snapshot.state.decided() == Some(proposal.value),
                "saved decision evidence mismatch"
            );
            Some(DurableMessage::Decision {
                proposal,
                certificate,
            })
        }
        _ => anyhow::bail!("unknown signing outbox kind"),
    };
    reader.finish()?;
    ensure!(
        encode_outbox(snapshot, saved, message.as_ref())? == bytes,
        "noncanonical signing outbox"
    );
    Ok(message)
}

struct Reader<'a> {
    bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    fn open(bytes: &'a [u8], magic: &[u8; 8]) -> Result<Self> {
        ensure!(
            (40..=MAX_RECORD).contains(&bytes.len()),
            "invalid signing record size"
        );
        let (body, pin) = bytes.split_at(bytes.len() - 32);
        ensure!(
            digest(body).as_slice() == pin && body.starts_with(magic),
            "signing record digest/version mismatch"
        );
        Ok(Self { bytes: &body[8..] })
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        ensure!(count <= self.bytes.len(), "truncated signing record");
        let (head, tail) = self.bytes.split_at(count);
        self.bytes = tail;
        Ok(head)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn hash(&mut self) -> Result<Hash> {
        Ok(self.take(32)?.try_into()?)
    }
    fn frame(&mut self, max: usize) -> Result<&'a [u8]> {
        let count = u32::from_be_bytes(self.take(4)?.try_into()?) as usize;
        ensure!(count <= max, "signing record field exceeds bound");
        self.take(count)
    }
    fn finish(&self) -> Result<()> {
        ensure!(self.bytes.is_empty(), "trailing signing record bytes");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
