//! Codec-only recovery delivery namespace checks. Reuse existing synthetic
//! consensus/APFL/NUAS fixtures; neither the envelope tag nor its fragment ID
//! grants signature, execution, archive, spending or publication authority.
use super::*;

#[test]
fn control_reservation_matches_original_codec_not_body_ceiling() -> Result<()> {
    let limits = tests::limits();
    let mut largest = 0;
    for message in tests::specimens() {
        if matches!(
            message,
            Message::Proposal { .. } | Message::Vote(_) | Message::Decision { .. }
        ) {
            largest = largest.max(encode(&message, limits)?.len());
        }
    }
    // Original specimens use one validator and include the maximal proposal+QC.
    assert_eq!(largest, durable_control_bound(1, limits)?);
    assert_eq!(
        durable_control_bound(4, limits)?,
        17_924.min(limits.message_bytes)
    );
    let tight = DecodeLimits {
        message_bytes: 8192,
        ..limits
    };
    assert_eq!(durable_control_bound(4, tight)?, 8192);
    assert!(durable_control_bound(0, limits).is_err());
    assert!(durable_control_bound(wire::MAX_VALIDATORS + 1, limits).is_err());
    Ok(())
}

fn specimens() -> Result<Vec<(Message, DecodeLimits, u8, usize)>> {
    let mut out = Vec::new();
    for message in tests::specimens() {
        let kind = match &message {
            Message::Decision { .. } => Some((16, 0)),
            Message::Body { .. } => Some((17, 1)),
            _ => None,
        };
        if let Some((tag, lane)) = kind {
            out.push((message, tests::limits(), tag, lane));
        }
    }
    let (message, limits) = apfl::tests::recovery_specimen();
    out.push((message, limits, 18, 1));
    let (message, limits) = uca::tests::recovery_specimen()?;
    out.push((message, limits, 19, 1));
    ensure!(out.len() == 4);
    Ok(out)
}

#[test]
fn recovery_reply_namespace_preserves_canonical_payload_lane_and_body_identity() -> Result<()> {
    for (message, limits, tag, lane) in specimens()? {
        let original = encode(&message, limits)?;
        let recovery = encode_recovery_reply(&message, limits)?;
        assert_eq!(recovery[10], tag);
        assert_eq!(&recovery[..10], &original[..10]);
        assert_eq!(&recovery[PREFIX_BYTES..], &original[PREFIX_BYTES..]);
        assert_eq!(message_lane(&original)?, lane);
        assert_eq!(message_lane(&recovery)?, lane);
        let decoded = decode(&recovery, limits)?;
        assert_eq!(decoded, message);
        assert_eq!(encode(&decoded, limits)?, original);
        assert_eq!(encode_recovery_reply(&decoded, limits)?, recovery);
        if matches!(message, Message::Decision { .. }) {
            let Message::Decision {
                proposal,
                certificate,
                body_id,
            } = &message
            else {
                unreachable!()
            };
            let Message::Decision {
                proposal: recovered_proposal,
                certificate: recovered_certificate,
                body_id: recovered_body_id,
            } = &decoded
            else {
                unreachable!()
            };
            assert_eq!(
                wire::encode_proposal(proposal)?,
                wire::encode_proposal(recovered_proposal)?
            );
            assert_eq!(
                wire::encode_quorum(certificate)?,
                wire::encode_quorum(recovered_certificate)?
            );
            assert_eq!(body_id, recovered_body_id);
        } else {
            assert_eq!(
                candidate_body_id(&message, limits)?,
                candidate_body_id(&decoded, limits)?
            );
        }
        let domain = [0x97; 32];
        let original = OutgoingMessage::new(domain, original, limits.message_bytes)?;
        let recovery = OutgoingMessage::new(domain, recovery, limits.message_bytes)?;
        assert_ne!(
            original.id(),
            recovery.id(),
            "delivery purposes share a completion identity"
        );
        assert_eq!(original.domain(), recovery.domain());
        assert_eq!(original.byte_len(), recovery.byte_len());
    }
    Ok(())
}

#[test]
fn recovery_reply_namespace_rejects_requests_votes_proposals_and_input() -> Result<()> {
    for message in tests::specimens() {
        if !matches!(message, Message::Decision { .. } | Message::Body { .. }) {
            assert!(encode(&message, tests::limits()).is_ok());
            assert!(encode_recovery_reply(&message, tests::limits()).is_err());
        }
    }
    let scope = TransactionsScope {
        chain_id: 77,
        genesis: [1; 32],
        protocol: [2; 32],
        epoch: 1,
        validator_set_hash: [3; 32],
        session: [4; 32],
        sequence: 1,
    };
    let (apfl, apfl_limits) = apfl::tests::recovery_specimen();
    let Message::ApflBody { batch: apfl, .. } = apfl else {
        unreachable!()
    };
    let (uca, uca_limits) = uca::tests::recovery_specimen()?;
    let Message::AssetBody { batch: uca, .. } = uca else {
        unreachable!()
    };
    for (message, limits) in [
        (
            Message::Transactions {
                scope,
                raw_transactions: vec![vec![1, 2]],
            },
            tests::limits(),
        ),
        (
            Message::TransactionsTaken {
                scope,
                fragment_id: [7; 32],
            },
            tests::limits(),
        ),
        (
            Message::ApflTransactions { scope, batch: apfl },
            apfl_limits,
        ),
        (Message::AssetTransactions { scope, batch: uca }, uca_limits),
    ] {
        assert!(encode(&message, limits).is_ok());
        assert!(encode_recovery_reply(&message, limits).is_err());
    }
    Ok(())
}

#[test]
fn recovery_reply_namespace_preserves_limits_and_rejects_truncation_or_trailing_bytes() -> Result<()>
{
    for (message, limits, _, _) in specimens()? {
        let recovery = encode_recovery_reply(&message, limits)?;
        for end in [0, 8, 10, PREFIX_BYTES, recovery.len() - 1] {
            assert!(decode(&recovery[..end], limits).is_err());
        }
        let mut trailing = recovery.clone();
        trailing.push(0);
        assert!(decode(&trailing, limits).is_err());
        let mut wrong_version = recovery.clone();
        wrong_version[9] = 2;
        assert!(decode(&wrong_version, limits).is_err());
        assert!(message_lane(&wrong_version).is_err());

        // Small body ceilings keep the configuration itself valid, including
        // for a Decision whose evidence is smaller than the original body cap.
        let restricted = DecodeLimits {
            transactions: limits.transactions,
            transaction_bytes: 1,
            body_bytes: 1,
            message_bytes: recovery.len() - 1,
        };
        restricted.validate()?;
        assert!(encode_recovery_reply(&message, restricted).is_err());
        assert!(decode(&recovery, restricted).is_err());
        assert!(encode_recovery_reply(
            &message,
            DecodeLimits {
                transactions: 0,
                ..limits
            }
        )
        .is_err());
    }
    Ok(())
}
