//! Admission bounds and real original-owner crypto, not grant publication.
use super::*;
use crate::native_pipeline::ingress::batch::admission_retained_bytes;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[test]
fn application_admission_is_separate_and_charges_retained_allocations() -> Result<()> {
    assert!(SignatureAdmissionRequest::from_application(vec![]).is_err());
    assert!(SignatureAdmissionRequest::from_application(vec![
        vec![1];
        novovm_exec::pq_signature::MLDSA_VERIFY_BATCH_MAX_ITEMS
            + 1
    ])
    .is_err());
    assert!(SignatureAdmissionRequest::new(vec![
        AdmissionInput::Application(vec![1]),
        AdmissionInput::Raw(vec![1]),
    ])
    .is_err());
    let mut raw = Vec::with_capacity(8192);
    raw.extend_from_slice(&[1; 32]);
    let capacity = raw.capacity();
    let request = SignatureAdmissionRequest::from_application(vec![raw])?;
    assert_eq!(admission_retained_bytes(&request.inputs)?, capacity);
    let mut config = tests::config("unused-application-budget".into());
    config.authentication.body_bytes = capacity - 1;
    let (pipeline, receiver) = tests::inert_pipeline(config);
    let rejected = pipeline
        .try_admit_signatures_owned(request)
        .err()
        .context("retained capacity must exceed budget")?;
    assert!(receiver.try_recv().is_err());
    assert_eq!(pipeline.usage.lock().unwrap().bytes, 0);
    let raw = rejected.request.into_inputs().pop().unwrap().into_raw()?;
    assert_eq!(raw.len(), 32);
    assert_eq!(raw.capacity(), capacity);
    Ok(())
}

#[test]
fn application_backpressure_returns_original_command_and_holds_one_ingress_slot() -> Result<()> {
    let (pipeline, receiver) =
        tests::inert_pipeline(tests::config("unused-application-slot".into()));
    let request = || SignatureAdmissionRequest::from_application(vec![vec![7; 64]]);
    let SignatureAdmissionSubmission::Accepted(first) = pipeline
        .try_admit_signatures_owned(request()?)
        .map_err(|error| error.error)?
    else {
        anyhow::bail!("first application admission backpressured");
    };
    let command = receiver.try_recv()?;
    let SignatureAdmissionSubmission::Backpressured(returned) = pipeline
        .try_admit_signatures_owned(request()?)
        .map_err(|error| error.error)?
    else {
        anyhow::bail!("second application admission must wait for original slot");
    };
    assert_eq!(
        returned.into_inputs().pop().unwrap().into_raw()?,
        vec![7; 64]
    );
    assert_eq!(pipeline.usage.lock().unwrap().ingress, 1);
    assert_eq!(pipeline.usage.lock().unwrap().batches, 0);
    drop((first, command));
    assert_eq!(pipeline.usage.lock().unwrap().bytes, 0);
    assert_eq!(pipeline.application_signature_batch_calls(), 0);
    Ok(())
}

fn admit(pipeline: &CandidatePipeline, raw: Vec<Vec<u8>>) -> Result<SignatureAdmissionOutput> {
    let SignatureAdmissionSubmission::Accepted(mut ticket) = pipeline
        .try_admit_signatures_owned(SignatureAdmissionRequest::from_application(raw)?)
        .map_err(|error| error.error)?
    else {
        anyhow::bail!("unexpected application admission backpressure");
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ensure!(Instant::now() < deadline, "application admission deadline");
        if let Some(output) = ticket.try_take()? {
            return Ok(output);
        }
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "explicit qualified AOEM required; original queue/owner ML-DSA, not publication"]
fn actual_mldsa_application_admission_rejects_rows_without_asset_authority() -> Result<()> {
    use novovm_adapter_api::{uca_key_binding::derive_primary_key_ref_from_binding_v1, UcaKeyAlgo};
    use novovm_protocol::{native_query::UcaIdentityIdV1, uca_application::*};
    let library = std::path::PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted AOEM test library required")?,
    )
    .canonicalize()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "same packaged AOEM required"
    );
    let mut config = tests::config(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/resident-signature-admission-tests")
            .join(format!(
                "application-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ))
            .join("state.rocksdb"),
    );
    config.store.library = library.clone();
    config.authentication.transaction_bytes = 8192;
    std::fs::create_dir_all(config.store.database.parent().unwrap())?;
    let facade = novovm_exec::AoemExecFacade::open(&library, Default::default())?;
    let runtime =
        facade.mldsa_runtime_v1(novovm_exec::pq_signature::AoemMldsaParameterSet::MlDsa65)?;
    let key = runtime.generate_keypair()?;
    let intent = ApplicationGrantIntentV1 {
        selector: ApplicationGrantSelectorV1 {
            chain_id: config.store.domain.chain_id,
            genesis: config.store.domain.genesis_config_commitment,
            identity: UcaIdentityIdV1::new("uca:application-admission")
                .map_err(anyhow::Error::msg)?,
            application: [2; 32],
            service: [3; 32],
            device: [4; 32],
        },
        issuer_key_ref: derive_primary_key_ref_from_binding_v1(
            UcaKeyAlgo::Mldsa65,
            &key.public_key,
        )
        .try_into()
        .map_err(|_| anyhow::anyhow!("issuer key reference length"))?,
        expected_revision: 0,
        valid_from_height: 1,
        valid_until_height: 20,
        action: ApplicationGrantActionV1::Authorize {
            not_before_height: 1,
            expires_at_height: 100,
        },
    };
    let signer = facade.application_grant_signer_v1()?;
    let signed = signer.sign(intent.clone(), &key.public_key, &key.expanded_secret_key)?;
    let mut bad = signed.clone();
    bad.signature[0] ^= 1;
    let mut wrong_chain = intent.clone();
    wrong_chain.selector.chain_id += 1;
    let mut wrong_genesis = intent.clone();
    wrong_genesis.selector.genesis[0] ^= 1;
    let mut revoke = intent.clone();
    revoke.expected_revision = 1;
    revoke.action = ApplicationGrantActionV1::Revoke;
    let revoked = signer.sign(revoke, &key.public_key, &key.expanded_secret_key)?;
    let raw = vec![
        signed.encode()?,
        bad.encode()?,
        vec![0; 7],
        signer
            .sign(wrong_chain, &key.public_key, &key.expanded_secret_key)?
            .encode()?,
        signer
            .sign(wrong_genesis, &key.public_key, &key.expanded_secret_key)?
            .encode()?,
        revoked.encode()?,
    ];
    let (foreign, _receiver) = tests::inert_pipeline(config.clone());
    let pipeline = CandidatePipeline::start(config, OpenMode::CreateNew)?;
    let mut output = admit(&pipeline, raw.clone())?;
    for (index, expected) in raw.into_iter().enumerate() {
        let row = output.pop_admitted().context("missing admission row")?;
        assert_eq!(row.input.into_raw()?, expected);
        if index == 0 || index == 5 {
            let checked = row.result?;
            assert!(pipeline.owns_admitted(&checked));
            assert!(!foreign.owns_admitted(&checked));
            assert!(!checked.is_uca());
            assert!(checked.checked_uca().is_none());
            let fact = checked.checked_application().context("application fact")?;
            let original = if index == 0 {
                signed.clone()
            } else {
                revoked.clone()
            };
            assert_eq!(fact.reattach(original)?.intent(), fact.intent());
            assert!(fact.reattach(bad.clone()).is_err());
            assert!(checked.retained_metadata_bytes() < expected.len());
            let apfl = Arc::new(
                crate::native_pipeline::ingress::apfl::ApflTransferBatch::from_raw(
                    &[compute::tests::signed(1, 10)],
                    crate::native_pipeline::ingress::apfl::ApflLimits {
                        transactions: 16,
                        transaction_bytes: 4096,
                        body_bytes: 65536,
                    },
                )?,
            );
            assert!(pipeline.rebind_admitted_apfl(apfl, &[&checked]).is_err());
        } else {
            assert!(row.result.is_err(), "row {index} unexpectedly admitted");
        }
    }
    assert!(output.is_empty());
    drop(output);
    assert_eq!(pipeline.signature_counts(), (2, 0));
    assert_eq!(pipeline.signature_batch_calls(), 0);
    assert_eq!(pipeline.application_signature_batch_calls(), 1);
    let mut malformed = admit(&pipeline, vec![vec![0; 7]])?;
    assert!(malformed.pop_admitted().unwrap().result.is_err());
    drop(malformed);
    assert_eq!(pipeline.application_signature_batch_calls(), 1);
    let mut again = admit(&pipeline, vec![signed.encode()?])?;
    assert!(again.pop_admitted().unwrap().result.is_ok());
    drop(again);
    assert_eq!(pipeline.application_signature_batch_calls(), 2);
    assert_eq!(pipeline.signature_counts(), (3, 0));
    assert_eq!(pipeline.usage.lock().unwrap().batches, 0);
    assert_eq!(pipeline.usage.lock().unwrap().ingress, 0);
    let mut read = pipeline
        .try_read_consensus_metadata(vec![MetaKey::ChainHead])?
        .context("metadata query backpressure")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ensure!(Instant::now() < deadline, "metadata deadline");
        if let Some(snapshot) = read.try_take()? {
            assert_eq!(
                snapshot.values,
                vec![None],
                "signature admission published a head"
            );
            break;
        }
        std::thread::yield_now();
    }
    pipeline.shutdown()?;
    Ok(())
}
