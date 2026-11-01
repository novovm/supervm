//! Real AOEM + original in-process services. Independent-process acceptance
//! remains with the original native-product load owner.
use super::*;
use novovm_protocol::uca_spend::SignedAssetSpendV3;
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};
use sha2::{Digest, Sha256};

#[test]
fn checked_source_carrier_stays_inside_original_signature_header_reservation() {
    assert!(
        std::mem::size_of::<CheckedTransactionSource>()
            + std::mem::size_of::<Box<CheckedTransactionSource>>()
            <= 4096
    );
    assert!(std::mem::size_of::<TransactionSource>() <= 64);
}

#[test]
fn maximum_shape_single_row_projection_fits_original_bridge_allowance() -> Result<()> {
    use novovm_protocol::uca_spend::{
        spend_key_ref_v1, AssetAccountCreationV2, AssetAuthorityUpdateV2, AssetGrantConfigV2,
        AssetSpendIntentV3, MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
    };
    // Codec/allocation bound only: these synthetic bytes confer no signature
    // fact and are never submitted as verified or executed transactions.
    let public_key = vec![73; MLDSA65_PUBLIC_KEY_BYTES];
    let grants = (1..=8)
        .map(|n| AssetGrantConfigV2 {
            key_ref: [n; 32],
            permits_nov_transfer: true,
            not_before_height: 0,
            expires_at_height: u64::MAX,
            max_debit_per_intent: u128::MAX,
            remaining_debit_budget: u128::MAX,
        })
        .collect::<Vec<_>>();
    for action in 0..3 {
        let mut intent = AssetSpendIntentV3 {
            authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
            chain_id: 1,
            genesis: [2; 32],
            payer: vec![3; 32],
            recipient: vec![4; 32],
            asset: 1,
            amount: u128::MAX - 1,
            max_fee: 1,
            nonce: u64::MAX - 1,
            authorization_epoch: 1,
            spend_key_ref: spend_key_ref_v1(&public_key)?,
            not_before_height: 0,
            expires_at_height: 256,
            authority_update: None,
            recipient_creation: None,
        };
        if action == 1 {
            intent.amount = 0;
            intent.recipient = intent.payer.clone();
            intent.authority = novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller;
            intent.authority_update = Some(AssetAuthorityUpdateV2 {
                use_recovery: true,
                next_epoch: 2,
                active: true,
                management_key_ref: [40; 32],
                recovery_key_ref: Some([41; 32]),
                grants: grants.clone(),
            });
        } else if action == 2 {
            let descriptor = AssetAccountCreationV2 {
                management_key_ref: [40; 32],
                recovery_key_ref: Some([41; 32]),
                grants: grants.clone(),
            };
            intent.recipient = descriptor
                .account(intent.chain_id, &intent.genesis)?
                .to_vec();
            intent.recipient_creation = Some(descriptor);
        }
        let raw = SignedAssetSpendV3 {
            intent,
            public_key: public_key.clone(),
            signature: vec![91; MLDSA65_SIGNATURE_BYTES],
        }
        .encode()?;
        let decoded = SignedAssetSpendV3::decode(&raw)?;
        let source = batch(std::slice::from_ref(&decoded))?;
        let projected = AssetSpendBatchV3::from_views(
            &[source.view(0)?],
            AssetSpendBatchLimitsV3 {
                transactions: 1,
                transaction_bytes: 8192,
                body_bytes: 8192,
            },
        )?;
        let metadata = std::mem::size_of::<AdmittedTransfer>()
            + std::mem::size_of::<crate::native_pipeline::ingress::batch::CheckedAdmission>()
            + 2 * std::mem::size_of::<usize>()
            + decoded.intent.retained_heap_bytes();
        ensure!(projected.raw_transaction(0)? == raw);
        ensure!(projected.retained_bytes()? + metadata <= 3 * 8192 + 512 + 4096);
    }
    Ok(())
}

fn batch(rows: &[SignedAssetSpendV3]) -> Result<Arc<AssetSpendBatchV3>> {
    let raw = rows
        .iter()
        .map(SignedAssetSpendV3::encode)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Arc::new(AssetSpendBatchV3::from_raw(
        &raw,
        AssetSpendBatchLimitsV3 {
            transactions: 1024,
            transaction_bytes: 8192,
            body_bytes: 8 * 1024 * 1024,
        },
    )?))
}

fn poll_admission_until(
    rpc: &mut RpcLifecycle,
    done: impl Fn(&RpcLifecycle) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(rpc) {
        ensure!(
            Instant::now() < deadline,
            "bounded source admission stalled: {}",
            rpc.status()
        );
        rpc.poll_admission()?;
        std::thread::yield_now();
    }
    Ok(())
}

fn admit_future_prefix(rpc: &mut RpcLifecycle, signed: &[SignedAssetSpendV3]) -> Result<usize> {
    let source = batch(&signed[1..2])?; // nonce 1 before nonce 0
    rpc.incoming_from_client = true;
    rpc.incoming = Some((rpc.node.controller.uca_transactions_message(source)?, 0));
    poll_admission_until(rpc, |rpc| {
        rpc.incoming.is_none() && !rpc.admission.has_pending_native_admission()
    })?;
    ensure!(rpc.pending.len() == 1 && rpc.reservations.len() == 1);
    ensure!(rpc.pending.values().next().unwrap().nonce == 1);
    ensure!(rpc.node.controller.head().is_none());
    Ok(rpc.pending_retention.bytes)
}

fn queue_source(rpc: &mut RpcLifecycle, source: Arc<AssetSpendBatchV3>) -> Result<()> {
    let message = rpc.node.controller.uca_transactions_message(source)?;
    rpc.node
        .controller
        .queue_client_transactions_for_test(message)
}

#[test]
#[ignore = "real trusted AOEM; original pool_wait counterexample, run alone"]
fn future_nonce_pool_wait_without_source_return_holds_late_bridge() -> Result<()> {
    super::super::super::uca_native_tests::with_uca_gossip_service_fixture(|nodes, signed| {
        let rpc = &mut nodes[0];
        let prefix_bytes = admit_future_prefix(rpc, signed)?;
        rpc.admission.pool_byte_limit = Some(prefix_bytes);
        rpc.admission.bridge_reserve_bytes = Some(0);
        // The old cursor/authority owner has no source-return handoff. Its
        // full pool contains only a future nonce, not a publishable prefix.
        rpc.incoming = Some((
            rpc.node
                .controller
                .uca_transactions_message(batch(&signed[2..3])?)?,
            0,
        ));
        poll_admission_until(rpc, |rpc| {
            rpc.admission
                .uca
                .as_ref()
                .is_some_and(|work| work.pool_wait.is_some())
        })?;
        let bridge = batch(&signed[..1])?;
        queue_source(rpc, bridge.clone())?;
        let checked = rpc.admission.checked_rows;
        let captured = rpc.admission.uca_capture.batches;
        for _ in 0..32 {
            rpc.poll_admission()?;
        }
        ensure!(rpc.pending.len() == 1 && rpc.pending.values().next().unwrap().nonce == 1);
        ensure!(
            rpc.admission.checked_rows == checked && rpc.admission.uca_capture.batches == captured
        );
        ensure!(rpc.admission.uca.as_ref().unwrap().rows.len() == 1);
        // The existing next-signature stage may complete, but it still cannot
        // enter the occupied authority slot or publish the missing nonce.
        ensure!(rpc
            .admission
            .work
            .as_ref()
            .is_some_and(|work| work.rows == 1));
        ensure!(Arc::strong_count(&bridge) >= 2);
        ensure!(bridge.raw_transaction(0)? == signed[0].encode()?);
        ensure!(
            rpc.node.controller.head().is_none(),
            "no nonce gap may become finality"
        );
        Ok(())
    })
}

#[test]
#[ignore = "real trusted AOEM; original two-slot source exchange, exact bridge projection and four services, run alone"]
fn checked_future_sources_yield_late_bridge_without_rechecking_signatures_or_losing_rows(
) -> Result<()> {
    super::super::super::uca_native_tests::with_uca_gossip_service_fixture(|nodes, signed| {
        let hashes = signed[..4]
            .iter()
            .map(uca::hash)
            .collect::<Result<Vec<_>>>()?;
        let rpc = &mut nodes[0];
        let prefix_bytes = admit_future_prefix(rpc, signed)?;
        let prefix = &rpc.pending[&hashes[1]];
        let metadata = prefix.retained_metadata_bytes();
        let bridge = batch(&[signed[0].clone(), signed[1].clone()])?;
        let single = AssetSpendBatchV3::from_views(
            &[bridge.view(0)?],
            AssetSpendBatchLimitsV3 {
                transactions: 1,
                transaction_bytes: 8192,
                body_bytes: 8192,
            },
        )?;
        let single_retained = single.retained_bytes()? + metadata;
        ensure!(single_retained <= 3 * 8192 + 512 + 4096);
        rpc.admission.pool_byte_limit = Some(prefix_bytes + single_retained);
        rpc.admission.bridge_reserve_bytes = Some(single_retained);
        queue_source(rpc, batch(&signed[2..3])?)?;
        poll_admission_until(rpc, |rpc| rpc.admission.source_returns == 1)?;
        ensure!(rpc.admission.uca.is_none() && rpc.admission.work.is_none());
        ensure!(rpc.pending.len() == 1);
        // Fill the second source slot, resume the checked future, then fill
        // its vacated slot with the bridge BEFORE it encounters pressure again.
        queue_source(rpc, batch(&signed[3..4])?)?;
        rpc.poll_admission()?;
        ensure!(rpc.admission.uca.is_some());
        queue_source(rpc, bridge.clone())?;
        let checked_before_rotation = rpc.admission.checked_rows;
        let captures_before_rotation = rpc.admission.uca_capture.batches;
        rpc.poll_admission()?;
        ensure!(rpc.admission.source_returns == 2);
        ensure!(rpc.admission.checked_rows == checked_before_rotation);
        ensure!(
            rpc.admission.uca_capture.batches == captures_before_rotation,
            "same-parent/same-pool source rotation must not recapture authority"
        );
        poll_admission_until(rpc, |rpc| rpc.pending.contains_key(&hashes[0]))?;
        ensure!(rpc.pending_retention.bytes <= rpc.admission.pool_byte_limit.unwrap());
        ensure!(rpc.pending.len() == 2 && rpc.reservations.len() == 2);
        let AdmissionInput::Uca {
            batch: projected,
            index,
        } = &rpc.pending[&hashes[0]].input
        else {
            unreachable!()
        };
        ensure!(projected.len() == 1 && *index == 0 && !Arc::ptr_eq(projected, &bridge));
        ensure!(projected.raw_transaction(0)? == signed[0].encode()?);
        ensure!(
            rpc.admission.checked_rows == 4,
            "each distinct signed row is checked once, not again on source resume"
        );
        // Both blocked future rows remain in the original source/authority
        // handoffs, with their original indices and immutable checked facts.
        let mut owned = BTreeSet::<Hash>::new();
        if let Some(work) = &rpc.admission.uca {
            for (_, admitted) in &work.rows {
                owned.insert(
                    Sha256::digest(admitted.checked_uca().unwrap().intent().signing_bytes()?)
                        .into(),
                );
            }
        }
        let mut sources = Vec::new();
        while let Some(source) = rpc.node.controller.take_client_transactions() {
            sources.push(source);
        }
        ensure!(sources.len() <= 2);
        for source in sources {
            let TransactionSource::Checked { checked, origin } = source else {
                anyhow::bail!("checked future source lost its fact owner");
            };
            for (_, admitted) in &checked.rows {
                let intent = admitted
                    .checked_uca()
                    .context("checked source fact")?
                    .intent();
                owned.insert(Sha256::digest(intent.signing_bytes()?).into());
            }
            rpc.node
                .controller
                .rotate_taken_checked_transactions(origin, checked);
        }
        ensure!(owned == BTreeSet::from([hashes[2], hashes[3]]));
        let deadline = Instant::now() + Duration::from_secs(30);
        while !nodes.iter().all(|rpc| {
            hashes.iter().all(|id| {
                matches!(
                    rpc.receipts.get(id),
                    Some(ReceiptState::Finalized { success: true, .. })
                )
            })
        }) {
            ensure!(
                Instant::now() < deadline,
                "late bridge did not publish all original rows: {:?}",
                nodes.iter().map(RpcLifecycle::status).collect::<Vec<_>>()
            );
            for rpc in nodes.iter_mut() {
                rpc.poll(Instant::now())?;
            }
            std::thread::yield_now();
        }
        for rpc in nodes.iter_mut() {
            let mut fees = 0;
            for (index, id) in hashes.iter().enumerate() {
                let Some(ReceiptState::Finalized {
                    success: true,
                    nonce_after,
                    charged_fee,
                    ..
                }) = rpc.receipts.get(id)
                else {
                    unreachable!()
                };
                ensure!(*nonce_after == index as u64 + 1 && *charged_fee <= 50);
                fees += charged_fee;
            }
            let root = rpc
                .node
                .controller
                .head()
                .context("published bridge head")?
                .state_root;
            let mut read = native_query::UcaBatchStateRead::new(
                root,
                vec![(
                    signed[0].intent.clone(),
                    Account::try_from(signed[0].intent.payer.as_slice())
                        .map_err(anyhow::Error::msg)?,
                    Account::try_from(signed[0].intent.recipient.as_slice())
                        .map_err(anyhow::Error::msg)?,
                )],
                rpc.node.pipeline.capture_budget(),
            )?;
            let state = loop {
                ensure!(
                    Instant::now() < deadline,
                    "published bridge state read timeout"
                );
                if let Some(mut states) = read.poll(&rpc.node.pipeline)? {
                    break states.pop().context("bridge state result")??;
                }
                std::thread::yield_now();
            };
            let native_query::UcaReadResult::Present(state) = state else {
                anyhow::bail!("bridge did not retain original account state");
            };
            let amount: u128 = signed[..4].iter().map(|row| row.intent.amount).sum();
            ensure!(state.next_nonce() == 4 && state.balances() == (1000 - amount - fees, amount));
            ensure!(
                state
                    .grant()
                    .context("published original grant")?
                    .remaining_debit_budget
                    == 1000 - amount - fees
            );
            ensure!(rpc.pending.is_empty() && rpc.reservations.is_empty());
            ensure!(rpc.pending_retention.bytes == 0 && rpc.pending_retention.owners.is_empty());
        }
        Ok(())
    })
}
