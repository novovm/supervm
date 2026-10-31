//! Channel tests cover bounds/ownership only. The opt-in test below exercises
//! the existing real AOEM owner; no synthetic callback is claimed as execution.
use super::*;
use crate::native_pipeline::ingress::apfl::ApflLimits;
use crate::native_pipeline::pipeline::compute::tests::{policy, signed};
use crate::native_pipeline::pipeline::tests::{config, inert_pipeline, request};
use crate::native_pipeline::state::tree::empty_root;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn cfg() -> PipelineConfig {
    let mut cfg = config("unused-signature-admission".into());
    cfg.max_retained_bytes = 512 * 1024 * 1024;
    cfg
}

fn input() -> SignatureAdmissionRequest {
    SignatureAdmissionRequest::from_raw(vec![signed(1, 10), signed(2, 20)]).unwrap()
}

fn usage(pipeline: &CandidatePipeline) -> (usize, usize, usize, usize) {
    let used = pipeline.usage.lock().unwrap();
    (used.batches, used.bytes, used.background, used.ingress)
}

fn all_usage(pipeline: &CandidatePipeline) -> (usize, usize, usize, usize, usize) {
    let used = pipeline.usage.lock().unwrap();
    (
        used.batches,
        used.bytes,
        used.background,
        used.ingress,
        used.authority,
    )
}

fn rejected_output(
    pipeline: &CandidatePipeline,
    command: AdmissionCommand,
) -> (
    SignatureAdmissionOutput,
    mpsc::Sender<Result<SignatureAdmissionOutput>>,
) {
    let AdmissionCommand {
        request,
        reply,
        permit,
    } = command;
    // Channel/accounting fixture only: never manufactures signature evidence.
    let rows = request
        .inputs
        .into_iter()
        .map(|input| AdmissionRow {
            input,
            result: Err(anyhow::anyhow!("fixture rejection")),
        })
        .collect();
    (
        SignatureAdmissionOutput {
            rows: AdmissionRows {
                rows,
                peak_callbacks: 0,
            },
            _permit: permit,
            owner: pipeline.identity.clone(),
        },
        reply,
    )
}

fn completed(
    pipeline: &CandidatePipeline,
    receiver: &mpsc::Receiver<DriverMessage>,
) -> SignatureAdmissionOutput {
    let mut ticket = admitted(pipeline);
    let (output, reply) = rejected_output(pipeline, next(receiver));
    assert!(reply.send(Ok(output)).is_ok());
    ticket.try_take().unwrap().unwrap()
}

fn admitted(pipeline: &CandidatePipeline) -> SignatureAdmissionTicket {
    let SignatureAdmissionSubmission::Accepted(ticket) =
        pipeline.try_admit_signatures_owned(input()).unwrap()
    else {
        panic!("signature admission unexpectedly backpressured")
    };
    ticket
}

fn next(receiver: &mpsc::Receiver<DriverMessage>) -> AdmissionCommand {
    let DriverMessage::Admission(command) = receiver.try_recv().unwrap() else {
        panic!("expected signature admission command")
    };
    command
}

#[test]
fn admission_has_one_independent_slot_and_preserves_maximum_candidate_budget() {
    let mut config = cfg();
    let bytes = input().reservation(&config).unwrap();
    let current = BatchRequest::retained_reservation(
        config.authentication.body_bytes.min(config.plan.body_bytes),
        &config,
    )
    .unwrap();
    config.max_retained_bytes = bytes + current - 1;
    let (short, _) = inert_pipeline(config.clone());
    assert!(matches!(
        short.try_admit_signatures_owned(input()).unwrap(),
        SignatureAdmissionSubmission::Backpressured(_)
    ));
    assert_eq!(usage(&short), (0, 0, 0, 0));
    config.max_retained_bytes += 1;
    let (pipeline, receiver) = inert_pipeline(config);
    let ticket = admitted(&pipeline);
    let command = next(&receiver);
    assert_eq!(usage(&pipeline), (0, bytes, 0, 1));
    assert!(matches!(
        pipeline.try_admit_signatures_owned(input()).unwrap(),
        SignatureAdmissionSubmission::Backpressured(_)
    ));
    let Submission::Accepted(current) = pipeline.try_submit_owned(request(empty_root())).unwrap()
    else {
        panic!("ingress consumed the sole current candidate slot")
    };
    let ordinary = receiver.try_recv().unwrap();
    assert_eq!(usage(&pipeline).0, 1);
    drop((ticket, command, current, ordinary));
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
}

#[test]
fn dropped_ticket_does_not_cancel_admitted_work_and_error_reply_retains_budget() {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let ticket = admitted(&pipeline);
    drop(ticket);
    assert_eq!(usage(&pipeline).3, 1);
    let command = next(&receiver);
    drop(command);
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
    let mut ticket = admitted(&pipeline);
    assert!(ticket.try_take().unwrap().is_none());
    let command = next(&receiver);
    assert!(command
        .reply
        .send(Err(anyhow::anyhow!("owner failure")))
        .is_ok());
    drop(command);
    assert_eq!(usage(&pipeline).3, 1);
    assert!(ticket.try_take().is_err());
    assert!(ticket.try_take().is_err());
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
}

#[test]
fn unconsumed_output_keeps_ingress_permit_until_consumed_or_dropped() {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let mut ticket = admitted(&pipeline);
    let AdmissionCommand {
        request,
        reply,
        permit,
    } = next(&receiver);
    // This is a channel-only fixture, deliberately all errors, not fake auth.
    let rows = request
        .inputs
        .into_iter()
        .map(|input| AdmissionRow {
            input,
            result: Err(anyhow::anyhow!("fixture rejection")),
        })
        .collect();
    assert!(reply
        .send(Ok(SignatureAdmissionOutput {
            rows: AdmissionRows {
                rows,
                peak_callbacks: 0
            },
            _permit: permit,
            owner: pipeline.identity.clone(),
        }))
        .is_ok());
    let mut output = ticket.try_take().unwrap().unwrap();
    assert_eq!(output.len(), 2);
    assert_eq!(usage(&pipeline).3, 1);
    assert!(output.pop_front().unwrap().result.is_err());
    assert_eq!(usage(&pipeline).3, 1);
    drop(output);
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
}

#[test]
fn authority_parking_hands_off_only_one_slot_and_retains_bytes_through_last_arc() {
    let mut config = cfg();
    let bytes = input().reservation(&config).unwrap();
    let reserve = BatchRequest::retained_reservation(
        config.authentication.body_bytes.min(config.plan.body_bytes),
        &config,
    )
    .unwrap();
    config.max_retained_bytes = 2 * bytes + reserve;
    let (pipeline, receiver) = inert_pipeline(config);
    let mut first = completed(&pipeline, &receiver);
    assert_eq!(all_usage(&pipeline), (0, bytes, 0, 1, 0));
    // A result held by its caller still occupies active ingress until the
    // explicit stage handoff, even though resident compute has completed.
    assert!(matches!(
        pipeline.try_admit_signatures_owned(input()).unwrap(),
        SignatureAdmissionSubmission::Backpressured(_)
    ));
    let retained = first._permit.clone();
    first.park_for_authority().unwrap();
    assert_eq!(all_usage(&pipeline), (0, bytes, 0, 0, 1));
    let mut second = completed(&pipeline, &receiver);
    assert_eq!(all_usage(&pipeline), (0, 2 * bytes, 0, 1, 1));
    let before = all_usage(&pipeline);
    assert!(first.park_for_authority().is_err());
    assert!(second.park_for_authority().is_err());
    assert_eq!(all_usage(&pipeline), before);
    assert!(matches!(
        pipeline.try_admit_signatures_owned(input()).unwrap(),
        SignatureAdmissionSubmission::Backpressured(_)
    ));
    let Submission::Accepted(candidate) = pipeline.try_submit_owned(request(empty_root())).unwrap()
    else {
        panic!("overlapping admissions consumed the reserved candidate quota")
    };
    let command = receiver.try_recv().unwrap();
    let candidate_bytes = all_usage(&pipeline).1 - 2 * bytes;
    assert_eq!(
        all_usage(&pipeline),
        (1, 2 * bytes + candidate_bytes, 0, 1, 1)
    );
    drop(first);
    assert!(second.park_for_authority().is_err());
    assert_eq!(all_usage(&pipeline).4, 1);
    // The final Arc, not output/ticket consumption, retires the byte and role
    // charge. Moving to authority must never drop an active signature charge.
    drop(retained);
    assert_eq!(all_usage(&pipeline), (1, bytes + candidate_bytes, 0, 1, 0));
    second.park_for_authority().unwrap();
    assert_eq!(all_usage(&pipeline), (1, bytes + candidate_bytes, 0, 0, 1));
    drop((second, candidate, command));
    assert_eq!(all_usage(&pipeline), (0, 0, 0, 0, 0));
}

#[test]
fn source_parking_preserves_bytes_and_reclaims_only_the_unique_authority_slot() {
    let mut config = cfg();
    let bytes = input().reservation(&config).unwrap();
    let reserve = BatchRequest::retained_reservation(
        config.authentication.body_bytes.min(config.plan.body_bytes),
        &config,
    )
    .unwrap();
    config.max_retained_bytes = 3 * bytes + reserve;
    let (pipeline, receiver) = inert_pipeline(config);
    let mut first = completed(&pipeline, &receiver);
    first.park_for_source().unwrap();
    assert_eq!(all_usage(&pipeline), (0, bytes, 0, 0, 0));
    assert!(first.park_for_authority().is_err());
    let mut second = completed(&pipeline, &receiver);
    second.park_for_authority().unwrap();
    let mut third = completed(&pipeline, &receiver);
    let before = all_usage(&pipeline);
    assert_eq!(before, (0, 3 * bytes, 0, 1, 1));
    assert!(!first.resume_authority().unwrap());
    first.park_for_source().unwrap();
    assert_eq!(all_usage(&pipeline), before);
    third.park_for_source().unwrap();
    assert_eq!(all_usage(&pipeline), (0, 3 * bytes, 0, 0, 1));
    second.park_for_source().unwrap();
    assert_eq!(all_usage(&pipeline), (0, 3 * bytes, 0, 0, 0));
    assert!(first.resume_authority().unwrap());
    assert!(!second.resume_authority().unwrap());
    assert!(!third.resume_authority().unwrap());
    assert!(first.resume_authority().is_err());
    let retained = first._permit.clone();
    drop((second, third, first));
    assert_eq!(all_usage(&pipeline), (0, bytes, 0, 0, 1));
    drop(retained);
    assert_eq!(all_usage(&pipeline), (0, 0, 0, 0, 0));
}

#[test]
fn parked_source_charge_still_reserves_the_maximum_candidate_bytes() {
    let mut config = cfg();
    let bytes = input().reservation(&config).unwrap();
    let reserve = BatchRequest::retained_reservation(
        config.authentication.body_bytes.min(config.plan.body_bytes),
        &config,
    )
    .unwrap();
    config.max_retained_bytes = 2 * bytes + reserve - 1;
    let (pipeline, receiver) = inert_pipeline(config);
    let mut output = completed(&pipeline, &receiver);
    output.park_for_source().unwrap();
    assert!(matches!(
        pipeline.try_admit_signatures_owned(input()).unwrap(),
        SignatureAdmissionSubmission::Backpressured(_)
    ));
    assert_eq!(all_usage(&pipeline), (0, bytes, 0, 0, 0));
    let Submission::Accepted(candidate) = pipeline.try_submit_owned(request(empty_root())).unwrap()
    else {
        panic!("parked bytes prevented the reserved candidate")
    };
    drop((output, candidate, receiver.try_recv().unwrap()));
    assert_eq!(all_usage(&pipeline), (0, 0, 0, 0, 0));
}

#[test]
fn parked_reply_abandonment_releases_its_original_owner_only() {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let (other, _) = inert_pipeline(cfg());
    let ticket = admitted(&pipeline);
    let (mut output, reply) = rejected_output(&pipeline, next(&receiver));
    output.park_for_authority().unwrap();
    assert_eq!(all_usage(&pipeline).4, 1);
    assert_eq!(all_usage(&other), (0, 0, 0, 0, 0));
    assert!(reply.send(Ok(output)).is_ok());
    drop(ticket);
    assert_eq!(all_usage(&pipeline), (0, 0, 0, 0, 0));
    assert_eq!(all_usage(&other), (0, 0, 0, 0, 0));
}

#[test]
fn authority_handoff_rejects_non_ingress_role_without_changing_accounting() {
    let (pipeline, _) = inert_pipeline(cfg());
    let permit = pipeline.reserve(23, false).unwrap().unwrap();
    assert_eq!(all_usage(&pipeline), (1, 23, 0, 0, 0));
    assert!(permit.park_for_authority().is_err());
    assert!(permit.park_for_source().is_err());
    assert!(permit.resume_authority().is_err());
    assert_eq!(all_usage(&pipeline), (1, 23, 0, 0, 0));
    drop(permit);
    assert_eq!(all_usage(&pipeline), (0, 0, 0, 0, 0));
}

#[test]
#[ignore = "real trusted AOEM; single-row APFL projection preserves exact signature fact and original owner"]
fn admitted_asset_row_projection_retains_exact_fact_without_pin_or_downgrade() -> Result<()> {
    use novovm_protocol::uca_spend::{spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2};
    use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};
    let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM test library required")?;
    let facade =
        novovm_exec::AoemExecFacade::open(std::path::Path::new(&library), Default::default())?;
    let runtime =
        facade.mldsa_runtime_v1(novovm_exec::pq_signature::AoemMldsaParameterSet::MlDsa65)?;
    let key = runtime.generate_keypair()?;
    let mut config = cfg();
    config.authentication.transaction_bytes = 8192;
    config.plan.transaction_bytes = 8192;
    let signer = facade.asset_spend_signer_v2()?;
    let rows = (0..2)
        .map(|nonce| {
            signer.sign(
                AssetSpendIntentV2 {
                    chain_id: config.store.domain.chain_id,
                    genesis: config.store.domain.genesis_config_commitment,
                    payer: vec![1; 20],
                    recipient: vec![2; 20],
                    asset: 1,
                    authorization_epoch: 1,
                    spend_key_ref: spend_key_ref_v1(&key.public_key)?,
                    amount: 1,
                    max_fee: 50,
                    nonce,
                    not_before_height: 1,
                    expires_at_height: 50,
                    authority_update: None,
                    recipient_creation: None,
                },
                &key.public_key,
                &key.expanded_secret_key,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let raw = rows
        .iter()
        .map(SignedAssetSpendV2::encode)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let source = Arc::new(AssetSpendBatchV2::from_raw(
        &raw,
        AssetSpendBatchLimitsV2 {
            transactions: 2,
            transaction_bytes: 8192,
            body_bytes: 16 * 1024,
        },
    )?);
    let weak = Arc::downgrade(&source);
    let (pipeline, _) = inert_pipeline(config.clone());
    let (foreign, _) = inert_pipeline(config);
    let original = admitted_uca_for_test(pipeline.identity.clone(), &facade, source.clone(), 1)?;
    assert!(foreign.project_admitted_uca_row(&original).is_err());
    let v3 = admitted_for_test(&pipeline, structured(&[signed(1, 10)]), 0)?;
    assert!(pipeline.project_admitted_uca_row(&v3).is_err());
    let (input, projected) = pipeline.project_admitted_uca_row(&original)?;
    let AdmissionInput::Uca { batch, index } = &input else {
        anyhow::bail!("asset projection changed contract");
    };
    assert_eq!(*index, 0);
    assert_eq!(batch.len(), 1);
    assert!(!Arc::ptr_eq(batch, &source));
    assert_eq!(batch.view(0)?, source.view(1)?);
    assert!(projected.matches_uca_source(batch, 0));
    assert!(!projected.matches_uca_source(&source, 1));
    assert!(!original.matches_uca_source(batch, 0));
    assert!(
        projected
            .checked_uca()
            .unwrap()
            .reattach(batch.row(0)?)?
            .signed()
            == &rows[1]
    );
    assert!(projected
        .checked_uca()
        .unwrap()
        .reattach(rows[0].clone())
        .is_err());
    assert!(batch.retained_bytes()? + projected.retained_metadata_bytes() <= 29_184);
    assert!(pipeline
        .rebind_admitted_uca(batch.clone(), &[&projected])
        .is_ok());
    assert!(foreign
        .rebind_admitted_uca(batch.clone(), &[&projected])
        .is_err());
    assert_eq!(
        pipeline.signature_counts(),
        (0, 0),
        "projection/rebind must not rerun crypto"
    );
    drop(original);
    drop(source);
    assert!(
        weak.upgrade().is_none(),
        "single-row fact still pins the full original dictionary"
    );
    Ok(())
}

#[test]
fn backpressure_and_closed_pipeline_return_the_exact_original_raw_allocation() {
    let (mut pipeline, receiver) = inert_pipeline(cfg());
    let ticket = admitted(&pipeline);
    let mut raw = vec![signed(3, 30)];
    let pointer = raw[0].as_ptr();
    let request = SignatureAdmissionRequest::from_raw(std::mem::take(&mut raw)).unwrap();
    let SignatureAdmissionSubmission::Backpressured(returned) =
        pipeline.try_admit_signatures_owned(request).unwrap()
    else {
        panic!("second signature request was admitted")
    };
    let returned = returned.into_inputs().pop().unwrap().into_raw().unwrap();
    assert_eq!(returned.as_ptr(), pointer);
    drop((ticket, next(&receiver)));
    pipeline.sender.take();
    let pointer = returned.as_ptr();
    let rejected = pipeline
        .try_admit_signatures_owned(SignatureAdmissionRequest::from_raw(vec![returned]).unwrap())
        .err()
        .unwrap();
    let returned = rejected
        .request
        .into_inputs()
        .pop()
        .unwrap()
        .into_raw()
        .unwrap();
    assert_eq!(returned.as_ptr(), pointer);
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
}

#[test]
fn structured_subrows_charge_each_whole_source_once_and_check_indices() {
    let raw = vec![signed(1, 10), signed(2, 20)];
    let batch = Arc::new(
        ApflTransferBatch::from_raw(
            &raw,
            ApflLimits {
                transactions: 16,
                transaction_bytes: 4096,
                body_bytes: 65_536,
            },
        )
        .unwrap(),
    );
    let one = SignatureAdmissionRequest::new(vec![AdmissionInput::Apfl {
        batch: batch.clone(),
        index: 1,
    }])
    .unwrap();
    assert_eq!(one.body_bytes, raw[1].len());
    assert_eq!(one.retained_bytes, batch.canonical_bytes());
    let repeated = SignatureAdmissionRequest::new(vec![
        AdmissionInput::Apfl {
            batch: batch.clone(),
            index: 0,
        },
        AdmissionInput::Apfl {
            batch: batch.clone(),
            index: 0,
        },
    ])
    .unwrap();
    assert_eq!(repeated.retained_bytes, batch.canonical_bytes());
    assert!(
        SignatureAdmissionRequest::new(vec![AdmissionInput::Apfl { batch, index: 2 }]).is_err()
    );
    let mut narrow = cfg();
    narrow.authentication.body_bytes = raw[1].len();
    assert!(one.reservation(&narrow).is_err());
}

fn structured(raw: &[Vec<u8>]) -> Arc<ApflTransferBatch> {
    Arc::new(
        ApflTransferBatch::from_raw(
            raw,
            ApflLimits {
                transactions: 16,
                transaction_bytes: 4096,
                body_bytes: 65_536,
            },
        )
        .unwrap(),
    )
}

#[test]
fn admitted_regrouping_uses_exact_signatures_and_releases_old_dictionaries() -> Result<()> {
    let (pipeline, _) = inert_pipeline(cfg());
    let raw = [signed(1, 10), signed(2, 20), signed(3, 30)];
    let original = structured(&raw);
    let old = Arc::downgrade(&original);
    let first = admitted_for_test(&pipeline, original.clone(), 2)?;
    let second = admitted_for_test(&pipeline, original.clone(), 0)?;
    assert!(first.matches_apfl_source(&original, 2));
    assert!(!first.matches_apfl_source(&original, 0));
    let selected = structured(&[raw[2].clone(), raw[0].clone()]);
    assert!(!Arc::ptr_eq(&original, &selected));
    assert!(!first.matches_apfl_source(&selected, 0));
    let checked = pipeline.rebind_admitted_apfl(selected.clone(), &[&first, &second])?;
    assert!(
        checked.retained_metadata_bytes() > 2 * std::mem::size_of::<SignatureCheckedTransfer>()
    );
    let AdmittedSignatures::V3(rows) = &checked.checked else {
        panic!("V3 admission contract changed");
    };
    assert_eq!(
        rows.transactions()[0].transfer().signature,
        first.checked().transfer().signature
    );
    assert_eq!(rows.transactions()[1].tx_hash(), second.checked().tx_hash());
    drop((original, first, second));
    assert!(
        old.upgrade().is_none(),
        "candidate retained the old unbudgeted dictionary"
    );
    let BatchSource::Apfl(source) = checked.source() else {
        panic!("structured source expanded");
    };
    assert!(Arc::ptr_eq(source, &selected));
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
    Ok(())
}

#[test]
fn admitted_rebinding_rejects_same_hash_changed_signature_duplicates_and_foreign_owner(
) -> Result<()> {
    let (pipeline, _) = inert_pipeline(cfg());
    let (foreign, _) = inert_pipeline(cfg());
    let raw = signed(1, 10);
    let original = structured(std::slice::from_ref(&raw));
    let checked = admitted_for_test(&pipeline, original.clone(), 0)?;
    let mut changed = raw.clone();
    *changed.last_mut().unwrap() ^= 1;
    let forged = structured(&[changed]);
    assert_eq!(
        forged.row(0)?.canonical_tx_hash()?,
        checked.checked().tx_hash()
    );
    assert!(pipeline.rebind_admitted_apfl(forged, &[&checked]).is_err());
    assert!(foreign
        .rebind_admitted_apfl(original.clone(), &[&checked])
        .is_err());
    let repeated = structured(&[raw.clone(), raw]);
    assert!(pipeline
        .rebind_admitted_apfl(repeated, &[&checked, &checked])
        .is_err());
    assert!(pipeline.rebind_admitted_apfl(original, &[]).is_err());
    let raw_cap = AdmittedTransfer {
        checked: Arc::new(CheckedAdmission::V3(
            crate::native_pipeline::ingress::authentication::authenticate_transfer_v3(
                &signed(1, 10),
                pipeline.storage_domain().chain_id,
                4096,
            )?,
        )),
        owner: pipeline.identity.clone(),
    };
    assert!(pipeline
        .rebind_admitted_apfl(structured(&[signed(1, 10)]), &[&raw_cap])
        .is_err());
    Ok(())
}

#[test]
fn admitted_request_owner_is_rechecked_at_both_real_enqueue_boundaries() -> Result<()> {
    let mut config = cfg();
    config.max_batches = 3;
    let (pipeline, _) = inert_pipeline(config.clone());
    let (foreign, incoming) = inert_pipeline(config);
    let batch = structured(&[signed(1, 10)]);
    let cap = admitted_for_test(&pipeline, batch.clone(), 0)?;
    let checked = pipeline.rebind_admitted_apfl(batch.clone(), &[&cap])?;
    let request =
        BatchRequest::from_authenticated(checked, compute::tests::context(empty_root()), policy())?;
    let rejected = foreign
        .try_submit_owned(request)
        .err()
        .expect("foreign ordinary owner accepted");
    assert!(rejected.error.to_string().contains("different pipeline"));
    let checked = pipeline.rebind_admitted_apfl(batch, &[&cap])?;
    let request = AuthenticationRequest::from_authenticated(checked, policy())?;
    assert!(foreign.can_authenticate_background(&request).is_err());
    let rejected = foreign
        .try_authenticate_owned(request)
        .err()
        .expect("foreign background owner accepted");
    assert!(rejected.error.to_string().contains("different pipeline"));
    assert!(matches!(
        incoming.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(usage(&foreign), (0, 0, 0, 0));
    Ok(())
}

#[test]
fn consumed_admitted_capability_does_not_pin_the_single_ingress_permit() -> Result<()> {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let batch = structured(&[signed(1, 10)]);
    let request = SignatureAdmissionRequest::from_apfl(batch.clone())?;
    let SignatureAdmissionSubmission::Accepted(mut ticket) =
        pipeline.try_admit_signatures_owned(request).unwrap()
    else {
        panic!("ingress unavailable");
    };
    let command = next(&receiver);
    let checked = crate::native_pipeline::ingress::authentication::authenticate_apfl_row(
        batch,
        0,
        pipeline.storage_domain().chain_id,
    )?;
    let row = AdmissionRow {
        input: command.request.into_inputs().pop().unwrap(),
        result: Ok(CheckedAdmission::V3(checked)),
    };
    assert!(command
        .reply
        .send(Ok(SignatureAdmissionOutput {
            rows: AdmissionRows {
                rows: [row].into(),
                peak_callbacks: 0
            },
            _permit: command.permit,
            owner: pipeline.identity.clone(),
        }))
        .is_ok());
    let mut output = ticket.try_take()?.expect("completed ingress");
    let row = output.pop_admitted().unwrap();
    let retained = row.result?;
    drop((row.input, output));
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
    // The real successful cap survives, yet a new request can use the sole slot.
    let again = admitted(&pipeline);
    assert_eq!(usage(&pipeline).3, 1);
    drop((again, next(&receiver), retained));
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
    Ok(())
}

#[test]
fn admitted_requests_keep_original_current_and_background_budget_rules() -> Result<()> {
    let (pipeline, receiver) = inert_pipeline(cfg());
    let batch = structured(&[signed(1, 10)]);
    let cap = admitted_for_test(&pipeline, batch.clone(), 0)?;
    let checked = pipeline.rebind_admitted_apfl(batch.clone(), &[&cap])?;
    let background = AuthenticationRequest::from_authenticated(checked, policy())?;
    // A max_batches=1 pipeline has no spare background slot, even for an
    // already verified body. Reuse must not bypass the current-height reserve.
    let AuthenticationSubmission::Backpressured(background) =
        pipeline.try_authenticate_owned(background).unwrap()
    else {
        panic!("reused background consumed sole ordinary slot");
    };
    drop(background);
    let checked = pipeline.rebind_admitted_apfl(batch, &[&cap])?;
    let current =
        BatchRequest::from_authenticated(checked, compute::tests::context(empty_root()), policy())?;
    let expected_bytes = current.reservation(&pipeline.config)?;
    let Submission::Accepted(ticket) = pipeline.try_submit_owned(current).unwrap() else {
        panic!("ordinary slot unavailable");
    };
    assert_eq!(usage(&pipeline), (1, expected_bytes, 0, 0));
    drop(ticket);
    assert_eq!(
        usage(&pipeline).0,
        1,
        "dropped reply must not cancel accepted work"
    );
    drop(receiver.try_recv().unwrap());
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
    Ok(())
}

#[test]
#[ignore = "requires explicit trusted AOEM; actual admission/preparation owner and counters, not finalized TPS"]
fn real_admitted_rows_reuse_strict_checks_without_trusting_peer_claims() -> Result<()> {
    let mut config = cfg();
    config.max_batches = 3;
    config.store.library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM test library required")?
        .into();
    config.store.database = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/resident-signature-admission-tests")
        .join(format!(
            "reuse-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ))
        .join("state.rocksdb");
    std::fs::create_dir_all(config.store.database.parent().unwrap())?;
    let pipeline = CandidatePipeline::start(config, OpenMode::CreateNew)?;
    let raw = [signed(1, 10), signed(2, 20)];
    let original = structured(&raw);
    let SignatureAdmissionSubmission::Accepted(mut ticket) = pipeline
        .try_admit_signatures_owned(SignatureAdmissionRequest::from_apfl(original)?)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("ingress unexpectedly full");
    };
    let mut output = wait(&mut ticket)?;
    let mut rows = Vec::new();
    while let Some(row) = output.pop_admitted() {
        rows.push(row.result?);
    }
    drop(output);
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
    assert_eq!(pipeline.signature_counts(), (2, 0));
    let selected = structured(&[raw[1].clone(), raw[0].clone()]);
    let checked = pipeline.rebind_admitted_apfl(selected.clone(), &[&rows[1], &rows[0]])?;
    let request = AuthenticationRequest::from_authenticated(checked, policy())?;
    let AuthenticationSubmission::Accepted(mut prepared) = pipeline
        .try_authenticate_owned(request)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("reuse unexpectedly full");
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let body = loop {
        if let Some(body) = prepared.try_take()? {
            break body;
        }
        ensure!(Instant::now() < deadline, "reuse preparation timeout");
        thread::yield_now();
    };
    assert_eq!(pipeline.signature_counts(), (2, 2));
    drop(body);
    // A body known only from the wire has no local capabilities: its original
    // strict verifier still runs, even when the canonical hashes are familiar.
    let AuthenticationSubmission::Accepted(mut peer) = pipeline
        .try_authenticate_owned(AuthenticationRequest::from_apfl(selected, policy())?)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("peer work unexpectedly full");
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let body = loop {
        if let Some(body) = peer.try_take()? {
            break body;
        }
        ensure!(Instant::now() < deadline, "peer authentication timeout");
        thread::yield_now();
    };
    assert_eq!(pipeline.signature_counts(), (4, 2));
    drop(body);
    pipeline.shutdown()
}

fn wait(ticket: &mut SignatureAdmissionTicket) -> Result<SignatureAdmissionOutput> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(output) = ticket.try_take()? {
            return Ok(output);
        }
        ensure!(
            Instant::now() < deadline,
            "signature admission completion timeout"
        );
        thread::yield_now();
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; resident signature graph, not state admission or TPS"]
fn real_resident_admission_keeps_good_bad_duplicate_rows_and_reuses_same_owner() -> Result<()> {
    let mut config = cfg();
    config.max_batches = 2;
    config.store.library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM test library required")?
        .into();
    config.store.database = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/resident-signature-admission-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ))
        .join("state.rocksdb");
    std::fs::create_dir_all(config.store.database.parent().unwrap())?;
    let pipeline = CandidatePipeline::start(config, OpenMode::CreateNew)?;
    let good = signed(1, 10);
    let mut bad = good.clone();
    *bad.last_mut().unwrap() ^= 1;
    let structured = Arc::new(ApflTransferBatch::from_raw(
        &[bad.clone(), signed(2, 20)],
        ApflLimits {
            transactions: 16,
            transaction_bytes: 4096,
            body_bytes: 65_536,
        },
    )?);
    let inputs = vec![
        AdmissionInput::Raw(good.clone()),
        AdmissionInput::Apfl {
            batch: structured.clone(),
            index: 0,
        },
        AdmissionInput::Raw(good.clone()),
        AdmissionInput::Apfl {
            batch: structured,
            index: 1,
        },
    ];
    let SignatureAdmissionSubmission::Accepted(mut ticket) = pipeline
        .try_admit_signatures_owned(SignatureAdmissionRequest::new(inputs)?)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("unexpected backpressure")
    };
    let mut output = wait(&mut ticket)?;
    assert_eq!(output.len(), 4);
    let first = output.pop_front().unwrap();
    let first_hash = first.result?.tx_hash();
    assert_eq!(first.input.into_raw()?, good);
    let rejected = output.pop_front().unwrap();
    assert!(rejected.result.is_err());
    assert_eq!(rejected.input.into_raw()?, bad);
    assert_eq!(output.pop_front().unwrap().result?.tx_hash(), first_hash);
    let fourth = output.pop_front().unwrap().result?;
    assert!(fourth.is_apfl_view());
    assert!(output.is_empty());
    drop(output);
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));

    // The candidate API still rejects the WHOLE duplicated batch. Only RPC
    // admission exposes per-row decisions; it cannot manufacture a candidate.
    let AuthenticationSubmission::Accepted(mut duplicate) = pipeline
        .try_authenticate_owned(AuthenticationRequest::new(
            vec![good.clone(), good],
            policy(),
        )?)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("candidate backpressured")
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match duplicate.try_take() {
            Err(error) => {
                ensure!(
                    error.to_string().contains("duplicate canonical"),
                    "wrong duplicate failure: {error:#}"
                );
                break;
            }
            Ok(None) => {
                ensure!(Instant::now() < deadline, "candidate duplicate timeout");
                thread::yield_now();
            }
            Ok(Some(_)) => anyhow::bail!("duplicated candidate was accepted"),
        }
    }
    let mut again = admitted(&pipeline);
    let mut checked = wait(&mut again)?;
    assert_eq!(checked.len(), 2);
    while let Some(row) = checked.pop_front() {
        assert!(row.result.is_ok());
    }
    drop(checked);
    assert_eq!(usage(&pipeline), (0, 0, 0, 0));
    pipeline.shutdown()
}
