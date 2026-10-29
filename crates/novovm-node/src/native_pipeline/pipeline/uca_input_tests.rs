//! Bounds without a library plus opt-in real same-owner ML-DSA admission.
use super::*;
use crate::native_pipeline::state::tree::empty_root;

#[test]
fn asset_request_charges_signed_accounts_once_in_raw_bytes() {
    let reference = tests::request(empty_root());
    let raw = vec![vec![0x55; 4000]];
    let request =
        BatchRequest::from_uca(raw, reference.request.context, reference.request.policy).unwrap();
    assert_eq!(request.body_bytes, 4000);
    assert_eq!(request.max_transaction_bytes, 4000);
    assert_eq!(request.transaction_count, 1);
    let mut config = tests::config("unused-uca-budget".into());
    config.authentication.body_bytes = 3999;
    assert!(request.reservation(&config).is_err());
    config.authentication.body_bytes = 4000;
    assert_eq!(
        request.reservation(&config).unwrap(),
        BatchRequest::retained_reservation(4000, &config).unwrap()
    );
    // Accounting creates no signature, authority, permit or durable result.
    assert!(matches!(
        request.request.raw_transactions,
        compute::CandidateInput::Uca { .. }
    ));
}

#[test]
fn asset_request_rejects_empty_and_oversized_wire_before_acceptance() {
    let reference = tests::request(empty_root());
    assert!(BatchRequest::from_uca(
        vec![],
        reference.request.context,
        reference.request.policy.clone(),
    )
    .is_err());
    let request = BatchRequest::from_uca(
        vec![vec![1; 4097]],
        reference.request.context,
        reference.request.policy,
    )
    .unwrap();
    let config = tests::config("unused-uca-oversized".into());
    assert!(request.reservation(&config).is_err());
}

#[test]
#[ignore = "actual packaged AOEM; exact NUAS signature reuse, not asset authority"]
fn actual_mldsa_uca_rebind_checks_full_rows_order_domain_and_owner() -> Result<()> {
    use novovm_protocol::uca_spend::{
        spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2, MAX_SIGNED_SPEND_BYTES,
    };
    use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};
    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM test library required")?,
    )
    .canonicalize()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "same packaged AOEM required"
    );
    let facade = novovm_exec::AoemExecFacade::open(&library, Default::default())?;
    let keys =
        facade.mldsa_runtime_v1(novovm_exec::pq_signature::AoemMldsaParameterSet::MlDsa65)?;
    let keypair = keys.generate_keypair()?;
    let pk = &keypair.public_key;
    let signer = facade.asset_spend_signer_v2()?;
    let mut signed = Vec::new();
    for nonce in 0..2 {
        signed.push(signer.sign(
            AssetSpendIntentV2 {
                asset: 1,
                authority_update: None,
                chain_id: 717,
                genesis: [1; 32],
                payer: vec![1; 20],
                authorization_epoch: 1,
                spend_key_ref: spend_key_ref_v1(pk)?,
                recipient: vec![2; 20],
                amount: 30,
                max_fee: 50,
                nonce,
                not_before_height: 1,
                expires_at_height: 50,
            },
            pk,
            &keypair.expanded_secret_key,
        )?);
    }
    let limits = AssetSpendBatchLimitsV2 {
        transactions: 16,
        transaction_bytes: MAX_SIGNED_SPEND_BYTES,
        body_bytes: 65536,
    };
    let make = |rows: &[SignedAssetSpendV2]| -> Result<_> {
        Ok(Arc::new(AssetSpendBatchV2::from_raw(
            &rows
                .iter()
                .map(SignedAssetSpendV2::encode)
                .collect::<std::result::Result<Vec<_>, _>>()?,
            limits,
        )?))
    };
    let source = make(&signed)?;
    let weak = Arc::downgrade(&source);
    let mut config = tests::config("unused-uca-rebind".into());
    config.authentication.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.plan.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    let (pipeline, _receiver) = tests::inert_pipeline(config.clone());
    let (foreign, _foreign_receiver) = tests::inert_pipeline(config);
    let first = admitted_uca_for_test(pipeline.identity.clone(), &facade, source.clone(), 0)?;
    let second = admitted_uca_for_test(pipeline.identity.clone(), &facade, source.clone(), 1)?;
    assert!(foreign
        .rebind_admitted_uca(make(&signed)?, &[&first, &second])
        .is_err());
    assert!(pipeline
        .rebind_admitted_uca(make(&signed)?, &[&second, &first])
        .is_err());
    let reversed = vec![signed[1].clone(), signed[0].clone()];
    assert!(pipeline
        .rebind_admitted_uca(make(&reversed)?, &[&second, &first])
        .is_ok());
    assert!(pipeline
        .rebind_admitted_uca(
            make(&[signed[0].clone(), signed[0].clone()])?,
            &[&first, &first]
        )
        .is_err());
    for change in 0..8 {
        let mut altered = signed.clone();
        match change {
            0 => altered[0].signature[0] ^= 1,
            1 => {
                altered[0].public_key[0] ^= 1;
                altered[0].intent.spend_key_ref = spend_key_ref_v1(&altered[0].public_key)?;
            }
            2 => altered[0].intent.genesis[0] ^= 1,
            3 => altered[0].intent.chain_id += 1,
            4 => altered[0].intent.amount += 1,
            5 => altered[0].intent.authorization_epoch += 1,
            6 => altered[0].intent.payer[0] ^= 1,
            _ => altered[0].intent.recipient[0] ^= 1,
        }
        assert!(
            pipeline
                .rebind_admitted_uca(make(&altered)?, &[&first, &second])
                .is_err(),
            "mutation {change}"
        );
    }
    let mut wrong_suite = signed[0].encode()?;
    wrong_suite[5] = 87;
    assert!(SignedAssetSpendV2::decode(&wrong_suite).is_err());
    let selected = make(&signed)?;
    let bytes = selected.canonical_bytes();
    let candidate = pipeline.rebind_admitted_uca(selected, &[&first, &second])?;
    let reference = tests::request(empty_root());
    let request = BatchRequest::from_authenticated(
        candidate,
        reference.request.context,
        reference.request.policy,
    )?;
    assert_eq!(request.body_bytes, bytes);
    assert!(request
        .request
        .raw_transactions
        .ensure_owner(&foreign.identity)
        .is_err());
    request
        .request
        .raw_transactions
        .ensure_owner(&pipeline.identity)?;
    drop(source);
    drop(first);
    drop(second);
    assert!(
        weak.upgrade().is_none(),
        "candidate cannot pin the retired pool dictionary"
    );
    assert_eq!(
        pipeline.signature_counts(),
        (0, 0),
        "rebinding alone never runs business or verifier"
    );
    Ok(())
}

#[test]
#[ignore = "actual packaged AOEM ML-DSA admission; not spend authority or finality"]
fn actual_mldsa_uca_async_admission_keeps_source_owner_and_row_failures() -> Result<()> {
    use novovm_protocol::uca_spend::{
        spend_key_ref_v1, AssetSpendIntentV2, MAX_SIGNED_SPEND_BYTES,
    };
    use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM test library required")?,
    )
    .canonicalize()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "test keygen and pipeline must use same packaged AOEM"
    );
    let mut config = tests::config(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/resident-signature-admission-tests")
            .join(format!(
                "uca-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ))
            .join("state.rocksdb"),
    );
    config.store.library = library.clone();
    config.prepare_asset_signatures = true;
    config.authentication.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.plan.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    std::fs::create_dir_all(config.store.database.parent().unwrap())?;
    let facade =
        novovm_exec::AoemExecFacade::open(&library, novovm_exec::AoemExecOpenOptions::default())?;
    let (pk, sk) = aoem_bindings::mldsa_keygen_v1_auto(65)?.context("ML-DSA-65 unavailable")?;
    let signed = facade.asset_spend_signer_v2()?.sign(
        AssetSpendIntentV2 {
            asset: 1,
            authority_update: None,
            chain_id: config.store.domain.chain_id,
            genesis: config.store.domain.genesis_config_commitment,
            payer: vec![1; 20],
            authorization_epoch: 1,
            spend_key_ref: spend_key_ref_v1(&pk)?,
            recipient: vec![2; 20],
            amount: 80,
            max_fee: 10,
            nonce: 0,
            not_before_height: 1,
            expires_at_height: 50,
        },
        &pk,
        &sk,
    )?;
    let mut tampered = signed.clone();
    tampered.signature[0] ^= 1;
    let mut wrong_domain = signed.clone();
    wrong_domain.intent.genesis[0] ^= 1;
    let raws = vec![signed.encode()?, tampered.encode()?, wrong_domain.encode()?];
    let limits = AssetSpendBatchLimitsV2 {
        transactions: 16,
        transaction_bytes: MAX_SIGNED_SPEND_BYTES,
        body_bytes: 65536,
    };
    let batch = Arc::new(AssetSpendBatchV2::from_raw(&raws, limits)?);
    assert!(SignatureAdmissionRequest::new(vec![
        AdmissionInput::Raw(vec![1]),
        AdmissionInput::Uca {
            batch: batch.clone(),
            index: 0
        },
    ])
    .is_err());
    assert!(SignatureAdmissionRequest::new(vec![AdmissionInput::Uca {
        batch: batch.clone(),
        index: 3
    }])
    .is_err());
    let (foreign, _receiver) = tests::inert_pipeline(config.clone());
    let pipeline = CandidatePipeline::start(config, OpenMode::CreateNew)?;
    let startup = pipeline.signature_startup();
    assert!(
        startup.0 && startup.1 > 0,
        "ready preceded verifier qualification"
    );
    assert_eq!(pipeline.signature_counts(), (0, 0));
    assert_eq!(
        pipeline.signature_batch_calls(),
        0,
        "KAT is not a business batch"
    );
    // Repeated rows are individual authentication facts, not nonce reservations;
    // the one control-owned pool must reject duplicate/same-nonce admissions.
    let request = SignatureAdmissionRequest::new(vec![
        AdmissionInput::Uca {
            batch: batch.clone(),
            index: 0,
        },
        AdmissionInput::Uca {
            batch: batch.clone(),
            index: 1,
        },
        AdmissionInput::Uca {
            batch: batch.clone(),
            index: 2,
        },
        AdmissionInput::Uca {
            batch: batch.clone(),
            index: 0,
        },
    ])?;
    let SignatureAdmissionSubmission::Accepted(mut ticket) = pipeline
        .try_admit_signatures_owned(request)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("unexpected UCA admission backpressure");
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut output = loop {
        ensure!(Instant::now() < deadline, "UCA signature admission timeout");
        if let Some(output) = ticket.try_take()? {
            break output;
        }
        std::thread::yield_now();
    };
    let first = output.pop_admitted().unwrap();
    assert_eq!(first.input.into_raw()?, raws[0]);
    let checked = first.result?;
    assert!(checked.is_uca());
    assert!(pipeline.owns_admitted(&checked));
    assert!(!foreign.owns_admitted(&checked));
    assert_eq!(checked.checked_uca().unwrap().signed().encode()?, raws[0]);
    assert!(checked.matches_uca_source(&batch, 0));
    assert!(!checked.matches_uca_source(&batch, 1));
    let equal_but_distinct = Arc::new(AssetSpendBatchV2::from_raw(&raws, limits)?);
    assert!(!checked.matches_uca_source(&equal_but_distinct, 0));
    assert!(checked.retained_metadata_bytes() >= raws[0].len());
    assert!(output.pop_admitted().unwrap().result.is_err());
    assert!(output.pop_admitted().unwrap().result.is_err());
    assert!(output
        .pop_admitted()
        .unwrap()
        .result?
        .matches_uca_source(&batch, 0));
    assert!(output.is_empty());
    drop(output);
    assert_eq!(pipeline.signature_counts(), (2, 0));
    assert_eq!(pipeline.signature_batch_calls(), 1);
    assert_eq!(pipeline.usage.lock().unwrap().ingress, 0);
    // A bad external signature has not latched failure on the resident owner.
    let SignatureAdmissionSubmission::Accepted(mut again) = pipeline
        .try_admit_signatures_owned(SignatureAdmissionRequest::new(vec![AdmissionInput::Uca {
            batch,
            index: 0,
        }])?)
        .map_err(|rejected| rejected.error)?
    else {
        anyhow::bail!("bad row poisoned admission");
    };
    loop {
        ensure!(Instant::now() < deadline, "UCA owner reuse timeout");
        if let Some(mut output) = again.try_take()? {
            assert!(output.pop_admitted().unwrap().result.is_ok());
            break;
        }
        std::thread::yield_now();
    }
    assert_eq!(pipeline.signature_batch_calls(), 2);
    assert_eq!(
        pipeline.signature_startup(),
        startup,
        "batch reopened verifier"
    );
    pipeline.shutdown()?;
    Ok(())
}
