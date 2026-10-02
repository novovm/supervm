//! Bounded local journal encoding. Hash pins catch damaged/mixed records; they
//! are not a replacement for filesystem protection against rollback of an
//! entire database. Consensus signatures and retained quorum witnesses are
//! independently verified on reopen, before the signer becomes usable.

use super::*;
use sha2::{Digest, Sha256};

const SNAP: &[u8; 8] = b"NVSIGN02";
const OUT: &[u8; 8] = b"NVOUT002";
const ARCHIVED_OUT: &[u8; 8] = b"NVOUT001";
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
    encode_snapshot_at(identity, identity.context, identity.parent, snapshot)
}

pub(super) fn encode_snapshot_at(
    identity: &Identity,
    context: ConsensusContext,
    parent: ParentPoint,
    snapshot: &Snapshot,
) -> Result<Vec<u8>> {
    ensure!(
        snapshot.revision > 0,
        "durable signing snapshot requires revision"
    );
    ensure!(
        snapshot.state.context() == &context,
        "snapshot context differs from pinned signer"
    );
    let mut bytes = SNAP.to_vec();
    bytes.extend_from_slice(&identity.validator);
    bytes.extend_from_slice(&parent_digest(&parent));
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
    for reference in references(&snapshot.replay) {
        bytes.push(u8::from(reference.is_some()));
        if let Some(reference) = reference {
            bytes.extend_from_slice(&reference.revision.to_be_bytes());
            bytes.extend_from_slice(&reference.digest);
        }
    }
    seal(bytes)
}

pub(super) fn decode_snapshot(identity: &Identity, bytes: &[u8]) -> Result<Snapshot> {
    ensure!(
        !bytes.starts_with(b"NVSIGN01"),
        "legacy signing snapshot lacks the required replay index; explicit migration required"
    );
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
    let mut read_reference = || -> Result<Option<ReplayRef>> {
        match reader.byte()? {
            0 => Ok(None),
            1 => {
                let reference = ReplayRef {
                    revision: reader.u64()?,
                    digest: reader.hash()?,
                };
                ensure!(
                    reference.revision > 0
                        && reference.revision <= revision
                        && reference.digest != [0; 32],
                    "invalid or future replay reference"
                );
                Ok(Some(reference))
            }
            _ => anyhow::bail!("unknown replay reference tag"),
        }
    };
    let replay = ReplayIndex {
        proposal: read_reference()?,
        prevote: read_reference()?,
        precommit: read_reference()?,
        locked: read_reference()?,
        valid: read_reference()?,
    };
    reader.finish()?;
    let snapshot = Snapshot {
        state,
        revision,
        proposed,
        witness,
        replay,
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
    record: &ReplayRecord,
) -> Result<Vec<u8>> {
    ensure!(
        record.revision == snapshot.revision,
        "outbox revision mismatch"
    );
    let mut bytes = OUT.to_vec();
    bytes.extend_from_slice(&record.revision.to_be_bytes());
    bytes.extend_from_slice(&digest(saved));
    encode_payload(&mut bytes, record)?;
    seal(bytes)
}

fn references(index: &ReplayIndex) -> [Option<ReplayRef>; MAX_REPLAY_RECORDS] {
    [
        index.proposal,
        index.prevote,
        index.precommit,
        index.locked,
        index.valid,
    ]
}

pub(super) fn replay_digest(record: &ReplayRecord) -> Result<Hash> {
    ensure!(record.revision > 0, "replay record requires revision");
    let mut bytes = b"novovm/replacement/signing-replay/v2\0".to_vec();
    bytes.extend_from_slice(&record.revision.to_be_bytes());
    encode_payload(&mut bytes, record)?;
    ensure!(
        bytes.len() <= MAX_RECORD,
        "replay payload exceeds byte budget"
    );
    Ok(digest(&bytes))
}

fn encode_payload(bytes: &mut Vec<u8>, record: &ReplayRecord) -> Result<()> {
    match &record.message {
        None => bytes.push(0),
        Some(DurableMessage::Proposal(proposal)) => {
            bytes.push(1);
            framed(bytes, &wire::encode_proposal(proposal)?)?;
        }
        Some(DurableMessage::Vote(vote)) => {
            bytes.push(2);
            framed(bytes, &wire::encode_vote(vote)?)?;
        }
        Some(DurableMessage::Decision {
            proposal,
            certificate,
        }) => {
            bytes.push(3);
            framed(bytes, &wire::encode_proposal(proposal)?)?;
            framed(bytes, &wire::encode_quorum(certificate)?)?;
        }
    }
    // A decision's QC and an evidence QC must never coexist in one record.
    ensure!(
        !matches!(record.message, Some(DurableMessage::Decision { .. }))
            || matches!(record.evidence, ReplayEvidence::None),
        "decision cannot carry another replay certificate"
    );
    match &record.evidence {
        ReplayEvidence::None => bytes.push(0),
        ReplayEvidence::Proposal {
            proposal,
            justification,
            candidate,
        } => {
            bytes.push(1);
            framed(bytes, &wire::encode_proposal(proposal.proposal())?)?;
            let proof = justification
                .as_ref()
                .map(|proof| wire::encode_quorum(proof.quorum()))
                .transpose()?
                .unwrap_or_default();
            framed(bytes, &proof)?;
            encode_locator(bytes, candidate);
        }
        ReplayEvidence::Certified {
            proposal,
            certificate,
            candidate,
        } => {
            bytes.push(2);
            framed(bytes, &wire::encode_proposal(proposal.proposal())?)?;
            framed(bytes, &wire::encode_quorum(certificate.quorum())?)?;
            encode_locator(bytes, candidate);
        }
    }
    Ok(())
}

fn encode_locator(bytes: &mut Vec<u8>, candidate: &CandidateLocator) {
    bytes.extend_from_slice(&candidate.value);
    bytes.extend_from_slice(&candidate.candidate_id);
    bytes.extend_from_slice(&candidate.document_digest);
}

fn decode_locator(reader: &mut Reader<'_>) -> Result<CandidateLocator> {
    let candidate = CandidateLocator {
        value: reader.hash()?,
        candidate_id: reader.hash()?,
        document_digest: reader.hash()?,
    };
    ensure!(
        candidate.value != [0; 32]
            && candidate.candidate_id != [0; 32]
            && candidate.document_digest != [0; 32],
        "zero replay candidate locator"
    );
    Ok(candidate)
}

pub(super) fn decode_outbox(
    identity: &Identity,
    snapshot: &Snapshot,
    saved: &[u8],
    bytes: &[u8],
) -> Result<ReplayRecord> {
    let mut reader = Reader::open(bytes, OUT)?;
    ensure!(
        reader.u64()? == snapshot.revision && reader.hash()? == digest(saved),
        "outbox/snapshot binding mismatch"
    );
    let record = decode_payload(identity, snapshot.revision, &mut reader)?;
    reader.finish()?;
    let reference = Some(ReplayRef {
        revision: record.revision,
        digest: replay_digest(&record)?,
    });
    match &record.message {
        None => match &record.evidence {
            ReplayEvidence::None => ensure!(
                snapshot.state.decided().is_none()
                    && snapshot.state.step() == Step::Propose
                    && snapshot.replay.proposal.is_none()
                    && snapshot.replay.prevote.is_none()
                    && snapshot.replay.precommit.is_none(),
                "state-only outbox lacks an eligible round transition"
            ),
            ReplayEvidence::Certified { certificate, .. } => ensure!(
                snapshot.state.decided().is_none()
                    && snapshot.state.step() == Step::Precommit
                    && certificate.round() == snapshot.state.round()
                    && snapshot.state.valid()
                        == certificate.value().map(|v| (certificate.round(), v))
                    && snapshot.replay.valid == reference,
                "state-only outbox lacks its late-quorum transition"
            ),
            _ => anyhow::bail!("state-only proposal evidence"),
        },
        Some(DurableMessage::Proposal(proposal)) => {
            ensure!(
                proposal.round == snapshot.state.round()
                    && snapshot.state.step() == Step::Propose
                    && snapshot.state.decided().is_none()
                    && snapshot.proposed
                        == Some((proposal.round, proposal.value, proposal.valid_round))
                    && snapshot.replay.proposal == reference,
                "saved proposal differs from signing state"
            );
        }
        Some(DurableMessage::Vote(vote)) => {
            ensure!(
                vote.round == snapshot.state.round() && snapshot.state.decided().is_none(),
                "saved vote differs from signer context/round"
            );
            match vote.phase {
                wire::Phase::Prevote => ensure!(
                    snapshot.state.step() == Step::Prevote && snapshot.replay.prevote == reference,
                    "saved prevote step mismatch"
                ),
                wire::Phase::Precommit => {
                    ensure!(
                        snapshot.state.step() == Step::Precommit
                            && snapshot.replay.precommit == reference,
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
        }
        Some(DurableMessage::Decision { proposal, .. }) => {
            ensure!(
                snapshot.state.decided() == Some(proposal.value)
                    && references(&snapshot.replay).iter().all(Option::is_none),
                "saved decision evidence mismatch"
            );
        }
    }
    ensure!(
        encode_outbox(snapshot, saved, &record)? == bytes,
        "noncanonical signing outbox"
    );
    Ok(record)
}

fn decode_payload(
    identity: &Identity,
    revision: u64,
    reader: &mut Reader<'_>,
) -> Result<ReplayRecord> {
    ensure!(revision > 0, "zero replay revision");
    let message = match reader.byte()? {
        0 => None,
        1 => Some(DurableMessage::Proposal(wire::decode_proposal(
            reader.frame(wire::MAX_WIRE_BYTES)?,
        )?)),
        2 => Some(DurableMessage::Vote(wire::decode_vote(
            reader.frame(wire::MAX_WIRE_BYTES)?,
        )?)),
        3 => Some(DurableMessage::Decision {
            proposal: wire::decode_proposal(reader.frame(wire::MAX_WIRE_BYTES)?)?,
            certificate: wire::decode_quorum(reader.frame(wire::MAX_WIRE_BYTES)?)?,
        }),
        _ => anyhow::bail!("unknown replay message kind"),
    };
    let evidence = match reader.byte()? {
        0 => ReplayEvidence::None,
        1 => {
            ensure!(
                !matches!(message, Some(DurableMessage::Decision { .. })),
                "multiple replay certificates"
            );
            let proposal = wire::decode_proposal(reader.frame(wire::MAX_WIRE_BYTES)?)?
                .verify(&identity.set)?;
            let bytes = reader.frame(wire::MAX_WIRE_BYTES)?;
            let justification = if bytes.is_empty() {
                None
            } else {
                Some(wire::decode_quorum(bytes)?.verify(&identity.set)?)
            };
            ReplayEvidence::Proposal {
                proposal,
                justification,
                candidate: decode_locator(reader)?,
            }
        }
        2 => {
            ensure!(
                !matches!(message, Some(DurableMessage::Decision { .. })),
                "multiple replay certificates"
            );
            let proposal = wire::decode_proposal(reader.frame(wire::MAX_WIRE_BYTES)?)?
                .verify(&identity.set)?;
            let certificate =
                wire::decode_quorum(reader.frame(wire::MAX_WIRE_BYTES)?)?.verify(&identity.set)?;
            ReplayEvidence::Certified {
                proposal,
                certificate,
                candidate: decode_locator(reader)?,
            }
        }
        _ => anyhow::bail!("unknown replay evidence kind"),
    };
    let record = ReplayRecord {
        revision,
        message,
        evidence,
    };
    validate_record(identity, &record)?;
    Ok(record)
}

fn exact_quorum(
    identity: &Identity,
    quorum: &VerifiedQuorum,
    round: u64,
    value: Hash,
    phase: wire::Phase,
) -> Result<()> {
    ensure!(
        quorum.context() == &identity.context
            && quorum.round() == round
            && quorum.value() == Some(value)
            && quorum.phase() == phase,
        "replay certificate domain/round/phase/value mismatch"
    );
    Ok(())
}

fn validate_record(identity: &Identity, record: &ReplayRecord) -> Result<()> {
    ensure!(record.revision > 0, "zero replay revision");
    match &record.evidence {
        ReplayEvidence::None => {}
        ReplayEvidence::Proposal {
            proposal,
            justification,
            candidate,
        } => {
            let p = proposal.proposal();
            validate_locator(identity, p, candidate)?;
            match (p.valid_round, justification) {
                (None, None) => {}
                (Some(round), Some(proof)) => {
                    ensure!(round < p.round, "replay justification is not earlier");
                    exact_quorum(identity, proof, round, p.value, wire::Phase::Prevote)?;
                }
                _ => anyhow::bail!("replay proposal justification missing or unexpected"),
            }
        }
        ReplayEvidence::Certified {
            proposal,
            certificate,
            candidate,
        } => {
            let p = proposal.proposal();
            validate_locator(identity, p, candidate)?;
            exact_quorum(
                identity,
                certificate,
                p.round,
                p.value,
                wire::Phase::Prevote,
            )?;
        }
    }
    match (&record.message, &record.evidence) {
        (None, ReplayEvidence::None | ReplayEvidence::Certified { .. }) => {}
        (Some(DurableMessage::Proposal(message)), ReplayEvidence::Proposal { proposal, .. }) => {
            message.verify(&identity.set)?;
            ensure!(
                message == proposal.proposal() && message.proposer_id == identity.validator,
                "replay local proposal differs from evidence/signer"
            );
        }
        (Some(DurableMessage::Vote(vote)), evidence) => {
            vote.verify(&identity.set)?;
            ensure!(
                vote.context == identity.context && vote.validator_id == identity.validator,
                "replay vote differs from local signer/domain"
            );
            match (vote.phase, vote.value, evidence) {
                (_, None, ReplayEvidence::None) => {}
                (wire::Phase::Prevote, value, ReplayEvidence::Proposal { proposal, .. }) => {
                    let p = proposal.proposal();
                    ensure!(
                        p.round == vote.round && value.is_none_or(|value| p.value == value),
                        "replay prevote differs from executed proposal"
                    );
                }
                (
                    wire::Phase::Precommit,
                    Some(value),
                    ReplayEvidence::Certified { proposal, .. },
                ) => {
                    ensure!(
                        proposal.proposal().round == vote.round
                            && proposal.proposal().value == value,
                        "replay precommit differs from certified execution"
                    );
                }
                _ => anyhow::bail!("replay vote has missing or wrong evidence kind"),
            }
        }
        (
            Some(DurableMessage::Decision {
                proposal,
                certificate,
            }),
            ReplayEvidence::None,
        ) => {
            proposal.verify(&identity.set)?;
            ensure!(
                proposal.context == identity.context,
                "replay decision context mismatch"
            );
            exact_quorum(
                identity,
                &certificate.verify(&identity.set)?,
                proposal.round,
                proposal.value,
                wire::Phase::Precommit,
            )?;
        }
        _ => anyhow::bail!("replay message/evidence kind mismatch"),
    }
    Ok(())
}

fn validate_locator(
    identity: &Identity,
    proposal: &Proposal,
    candidate: &CandidateLocator,
) -> Result<()> {
    ensure!(
        proposal.context == identity.context
            && candidate.value == proposal.value
            && candidate.value != [0; 32]
            && candidate.candidate_id != [0; 32]
            && candidate.document_digest != [0; 32],
        "replay candidate/domain mismatch"
    );
    Ok(())
}

/// Historical evidence is pinned by its own immutable revision/payload digest,
/// not by the latest snapshot's step or its mutable valid witness.
pub(super) fn decode_replay(
    identity: &Identity,
    reference: &ReplayRef,
    bytes: &[u8],
) -> Result<ReplayRecord> {
    let mut reader = Reader::open(bytes, OUT)?;
    let revision = reader.u64()?;
    ensure!(
        revision == reference.revision && revision > 0,
        "replay reference revision mismatch"
    );
    let _historical_snapshot_pin = reader.hash()?;
    let record = decode_payload(identity, revision, &mut reader)?;
    reader.finish()?;
    ensure!(
        replay_digest(&record)? == reference.digest,
        "replay reference payload digest mismatch"
    );
    Ok(record)
}

pub(super) fn validate_replay_index(
    identity: &Identity,
    snapshot: &Snapshot,
    records: &[ReplayRecord],
) -> Result<()> {
    ensure!(
        records.len() <= MAX_REPLAY_RECORDS,
        "too many replay records"
    );
    let refs = snapshot.replay.references()?;
    ensure!(
        refs.len() == records.len(),
        "missing or unreferenced replay records"
    );
    for (index, record) in records.iter().enumerate() {
        ensure!(
            record.revision > 0
                && record.revision <= snapshot.revision
                && !records[..index]
                    .iter()
                    .any(|old| old.revision == record.revision),
            "duplicate or future replay record"
        );
        validate_record(identity, record)?;
        ensure!(
            refs.iter()
                .any(|reference| reference.revision == record.revision
                    && replay_digest(record).is_ok_and(|digest| digest == reference.digest)),
            "replay record is not exactly pinned"
        );
    }
    if snapshot.state.decided().is_some() {
        ensure!(
            refs.is_empty(),
            "decided snapshot retains active replay roles"
        );
        return Ok(());
    }
    let get = |reference: Option<ReplayRef>| -> Result<Option<&ReplayRecord>> {
        reference
            .map(|reference| {
                records
                    .iter()
                    .find(|record| record.revision == reference.revision)
                    .context("replay role record missing")
            })
            .transpose()
    };
    let round = snapshot.state.round();
    match (
        snapshot.proposed.filter(|(r, _, _)| *r == round),
        get(snapshot.replay.proposal)?,
    ) {
        (None, None) => {}
        (
            Some(expected),
            Some(ReplayRecord {
                message: Some(DurableMessage::Proposal(proposal)),
                ..
            }),
        ) => {
            ensure!(
                (proposal.round, proposal.value, proposal.valid_round) == expected,
                "proposal replay role differs from snapshot"
            );
        }
        _ => anyhow::bail!("proposal replay role missing or wrong kind"),
    }
    for (phase, reference, required) in [
        (
            wire::Phase::Prevote,
            snapshot.replay.prevote,
            snapshot.state.step() != Step::Propose,
        ),
        (
            wire::Phase::Precommit,
            snapshot.replay.precommit,
            snapshot.state.step() == Step::Precommit,
        ),
    ] {
        match (required, get(reference)?) {
            (false, None) => {}
            (
                true,
                Some(ReplayRecord {
                    message: Some(DurableMessage::Vote(vote)),
                    evidence,
                    ..
                }),
            ) => {
                ensure!(
                    vote.round == round && vote.phase == phase,
                    "vote replay role round/phase mismatch"
                );
                if phase == wire::Phase::Prevote {
                    if let (Some(value), Some((locked_round, locked_value))) =
                        (vote.value, snapshot.state.locked())
                    {
                        // A later current-round QC can lock a different value
                        // after this vote. Only a retained older lock constrains
                        // the proposal that authorized the original prevote.
                        // Equal-round different-value QCs conflict with that
                        // lock, so this justification must be strictly newer.
                        if locked_round < round && locked_value != value {
                            ensure!(
                                matches!(evidence, ReplayEvidence::Proposal {
                                    proposal, justification: Some(proof), ..
                                } if proposal.proposal().valid_round
                                    .is_some_and(|valid_round| valid_round > locked_round)
                                    && proposal.proposal().valid_round == Some(proof.round())),
                                "prevote against retained lock lacks sufficient proposal justification"
                            );
                            // validate_record already checks the genuine QC's
                            // exact proposal domain, round, phase and value.
                        }
                    }
                }
                if phase == wire::Phase::Precommit {
                    if let Some(value) = vote.value {
                        ensure!(
                            snapshot.state.locked() == Some((round, value)),
                            "replayed precommit lock mismatch"
                        );
                    } else {
                        ensure!(
                            snapshot
                                .state
                                .locked()
                                .is_none_or(|(locked, _)| locked < round),
                            "nil precommit cannot create a current-round lock"
                        );
                    }
                }
            }
            _ => anyhow::bail!("vote replay role missing or unexpected"),
        }
    }
    for (checkpoint, reference, valid) in [
        (snapshot.state.locked(), snapshot.replay.locked, false),
        (snapshot.state.valid(), snapshot.replay.valid, true),
    ] {
        match (checkpoint, get(reference)?) {
            (None, None) => {}
            (
                Some((round, value)),
                Some(ReplayRecord {
                    message,
                    evidence: ReplayEvidence::Certified { certificate, .. },
                    ..
                }),
            ) => {
                exact_quorum(identity, certificate, round, value, wire::Phase::Prevote)?;
                if valid {
                    ensure!(
                        snapshot.witness.as_ref() == Some(certificate.quorum()),
                        "valid replay certificate differs from snapshot witness"
                    );
                } else {
                    ensure!(
                        matches!(message, Some(DurableMessage::Vote(vote))
                            if vote.phase == wire::Phase::Precommit
                                && vote.round == round && vote.value == Some(value)),
                        "locked replay role requires its local non-nil precommit"
                    );
                }
            }
            _ => anyhow::bail!("certified checkpoint replay role missing or wrong kind"),
        }
    }
    Ok(())
}

/// Parse an immutable decision archive without pretending it is a current
/// signer snapshot. The caller verifies the full QC/set, block statement and
/// exact outbox digest/locator; this decoder alone grants no authority.
pub(crate) fn decode_archived_decision(bytes: &[u8], revision: u64) -> Result<(Proposal, Quorum)> {
    let legacy = bytes.starts_with(ARCHIVED_OUT);
    let mut reader = Reader::open(bytes, if legacy { ARCHIVED_OUT } else { OUT })?;
    ensure!(
        reader.u64()? == revision && revision != 0,
        "archived decision revision mismatch"
    );
    let _snapshot_digest = reader.hash()?;
    ensure!(reader.byte()? == 3, "archive is not a decision outbox");
    let proposal = wire::decode_proposal(reader.frame(wire::MAX_WIRE_BYTES)?)?;
    let certificate = wire::decode_quorum(reader.frame(wire::MAX_WIRE_BYTES)?)?;
    if !legacy {
        ensure!(
            reader.byte()? == 0,
            "archived decision has unexpected replay evidence"
        );
    }
    reader.finish()?;
    Ok((proposal, certificate))
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
