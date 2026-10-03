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
