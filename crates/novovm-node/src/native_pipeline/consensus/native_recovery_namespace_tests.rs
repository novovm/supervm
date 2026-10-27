//! Real authenticated UDP/object delivery counterexamples, not consensus or
//! execution acceptance. The fixture signs a structurally valid decision; its
//! value is not an executed BlockStatement. Recovery must retain that exact
//! evidence while obtaining a distinct transport-only delivery identity.
use super::*;
use crate::native_pipeline::consensus::wire::Quorum;
use crate::native_pipeline::execution::plan::BatchContext;

fn fixture_body(context: VoteContext) -> transport::Message {
    transport::Message::Body {
        context: BatchContext {
            chain_id: context.chain_id,
            genesis_config_commitment: context.genesis_config_commitment,
            protocol_commitment: context.protocol_commitment,
            business_program: [0x81; 32],
            semantic_version: 1,
            effect_contract: [0x82; 32],
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: [0x83; 32],
            parent_receipt_root: [0x84; 32],
            parent_state_version: 0,
            receipt_codec: [0x85; 32],
            height: context.height,
            slot: 0,
            timestamp_unix_ms: 1,
        },
        raw_transactions: vec![vec![0x41; 1000], vec![0x42; 1000]],
    }
}

/// Send through the original authenticated object owner and consume its real
/// application handoff. Completion is the real transport DONE, never injected.
fn deliver(
    mesh: &mut Mesh,
    prepared: &PreparedMessage,
    expected: &transport::Message,
    handoffs: usize,
) -> Result<()> {
    ensure!(mesh.channels[0].try_send(0, prepared, Instant::now())?);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut completed = 0;
    let mut received = 0;
    loop {
        ensure!(
            Instant::now() < deadline,
            "transport completed {completed} send(s), but application received {received}/{handoffs} exact replies before the unchanged 30-second history TTL"
        );
        completed += mesh.pump()?;
        ensure!(completed <= 1, "duplicate transport completion");
        // Poll all three existing lanes. No receive/history limit is changed,
        // and this drains READY objects before judging a deduplicated resend.
        for (node, channel) in mesh.channels.iter_mut().enumerate() {
            for _ in 0..3 {
                if let Some(event) = channel.poll_receive(Instant::now(), 64)? {
                    match event {
                        ReceiveEvent::Message(value) => {
                            ensure!(node == 1, "unexpected receiving peer");
                            ensure!(
                                value.fragment_id == prepared.fragment_id(),
                                "delivery changed transport object identity"
                            );
                            ensure!(
                                &value.message == expected,
                                "recovery changed the signed evidence or original body"
                            );
                            if matches!(expected, transport::Message::Decision { .. }) {
                                ensure!(
                                    matches!(value.evidence, VerifiedEvidence::Decision { .. }),
                                    "recovery bypassed original decision verification"
                                );
                            }
                            received += 1;
                        }
                        ReceiveEvent::Rejected { error, .. } => {
                            anyhow::bail!("valid recovery fixture rejected: {error}")
                        }
                    }
                }
            }
        }
        ensure!(
            received <= handoffs,
            "same-purpose retransmission redelivered"
        );
        if completed == 1 && received == handoffs {
            break;
        }
        std::thread::yield_now();
    }
    Ok(())
}

fn check_realtime_then_recovery(
    keys: &[FixtureSigningKey],
    domain: MessageDomain,
    message: transport::Message,
) -> Result<()> {
    let mut mesh = Mesh::new(keys, domain)?;
    let started = Instant::now();
    let original = transport::encode(&message, codec())?;
    let realtime = mesh.channels[0].prepare(&message)?;
    deliver(&mut mesh, &realtime, &message, 1)?;
    // Exact same-purpose retransmits still receive DONE without a second
    // application handoff. This is the original bounded tombstone contract.
    deliver(&mut mesh, &realtime, &message, 0)?;
    let recovery = mesh.channels[0].prepare_recovery_reply(&message)?;
    deliver(&mut mesh, &recovery, &message, 1)?;
    ensure!(
        realtime.fragment_id() != recovery.fragment_id(),
        "recovery delivery responsibility still aliases realtime history"
    );
    let repeated = mesh.channels[0].prepare_recovery_reply(&message)?;
    ensure!(repeated.fragment_id() == recovery.fragment_id());
    deliver(&mut mesh, &repeated, &message, 0)?;
    ensure!(
        transport::encode(&message, codec())? == original,
        "canonical signed/body encoding changed"
    );
    ensure!(
        started.elapsed() < Duration::from_secs(30),
        "test must not wait out or renew the existing completion-history TTL"
    );
    Ok(())
}

#[test]
fn native_recovery_namespace_decision_redelivers_exact_evidence_before_history_expiry() -> Result<()>
{
    let (keys, _, domain, context) = separated_material()?;
    let body = fixture_body(context);
    let body_id = transport::candidate_body_id(&body, codec())?;
    let leader = domain.validators.leader(context.height, 0)?;
    let signing = keys
        .iter()
        .find(|key| {
            Validator::new(key.verifying_key().to_bytes(), 1)
                .is_ok_and(|validator| validator.id() == leader)
        })
        .context("missing fixture leader")?;
    let proposal =
        test_vectors::sign_proposal(context, 0, body_id, None, &domain.validators, signing)?;
    let votes = keys
        .iter()
        .take(3)
        .map(|key| {
            test_vectors::sign_vote(
                context,
                0,
                Phase::Precommit,
                Some(proposal.value),
                &domain.validators,
                key,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let certificate = Quorum::from_votes(&domain.validators, votes)?;
    check_realtime_then_recovery(
        &keys,
        domain,
        transport::Message::Decision {
            proposal,
            certificate,
            body_id,
        },
    )
}

#[test]
fn native_recovery_namespace_body_redelivers_exact_bytes_before_history_expiry() -> Result<()> {
    let (keys, _, domain, context) = separated_material()?;
    check_realtime_then_recovery(&keys, domain, fixture_body(context))
}
