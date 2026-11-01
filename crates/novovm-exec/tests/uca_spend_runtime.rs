//! Real packaged-runtime checks of the shared signature contract only.
//! These checks do not supply trusted asset state or execute a native transaction.
use aoem_bindings::AoemDyn;
use novovm_exec::pq_signature::{AoemMldsaParameterSet, MldsaSigner};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV3, SignedAssetSpendV3, MLDSA65_PUBLIC_KEY_BYTES,
    MLDSA65_SIGNATURE_BYTES, SPEND_CONTEXT_V3,
};
use std::path::PathBuf;

fn packaged_library() -> PathBuf {
    let (platform, library) = if cfg!(target_os = "windows") {
        ("windows", "aoem_ffi.dll")
    } else if cfg!(target_os = "macos") {
        ("macos", "libaoem_ffi.dylib")
    } else {
        ("linux", "libaoem_ffi.so")
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../aoem")
        .join(platform)
        .join("core/bin")
        .join(library)
}

// Ephemeral test keys are never printed or persisted. This mirrors the existing
// runtime test's fill-on-exit hygiene, not a claim of secure platform custody.
struct TestSecret(Vec<u8>);
impl Drop for TestSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

fn intent(public_key: &[u8]) -> AssetSpendIntentV3 {
    AssetSpendIntentV3 {
        authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
        chain_id: 97,
        genesis: [0x31; 32],
        payer: vec![1; 32],
        authorization_epoch: 4,
        spend_key_ref: spend_key_ref_v1(public_key).unwrap(),
        recipient: vec![2; 32],
        asset: novovm_protocol::uca_spend::NATIVE_NOV_ASSET,
        amount: 12_000_000_000,
        max_fee: 100_000,
        nonce: 29,
        not_before_height: 100,
        expires_at_height: 120,
        authority_update: None,
        recipient_creation: None,
    }
}

#[test]
#[ignore = "requires the trusted packaged AOEM runtime; run explicitly, no silent skip"]
fn packaged_facade_signs_and_checks_the_single_shared_spend_contract() {
    let path = packaged_library();
    let runtime = unsafe { AoemDyn::load(&path) }.expect("load trusted repository AOEM package");
    let facade = AoemExecFacade::open(&path, AoemExecOpenOptions::default())
        .expect("open the same packaged runtime through the product facade");
    let signer = facade
        .asset_spend_signer_v3()
        .expect("qualified ML-DSA-65 signer");
    let verifier = facade
        .asset_spend_verifier_v3()
        .expect("qualified ML-DSA-65 verifier");
    let (public_key, secret_key) = runtime
        .mldsa_keygen_v1(65)
        .expect("ephemeral ML-DSA-65 key");
    let secret_key = TestSecret(secret_key);
    let original = signer
        .sign(intent(&public_key), &public_key, &secret_key.0)
        .unwrap();
    assert_eq!(original.public_key.len(), MLDSA65_PUBLIC_KEY_BYTES);
    assert_eq!(original.signature.len(), MLDSA65_SIGNATURE_BYTES);
    let encoded = original.encode().unwrap();
    let decoded = SignedAssetSpendV3::decode(&encoded).unwrap();
    assert!(decoded == original);
    let checked = verifier.verify(decoded).unwrap();
    assert_eq!(checked.intent(), &original.intent);
    assert!(checked.signed() == &original);
    // Valid cryptography must not turn absent asset authority into permission.
    assert_eq!(
        checked.check_supplied_state(None, &[5; 32], 101),
        Err(novovm_adapter_api::uca_spend_policy::SpendPolicyError::StateUnavailable)
    );

    // Each changed intent is still structurally valid. The altered key reference
    // fails binding; the other altered fields must fail signature verification.
    let mutations: [fn(&mut AssetSpendIntentV3); 12] = [
        |v| v.chain_id += 1,
        |v| v.genesis[0] ^= 1,
        |v| v.payer[0] ^= 1,
        |v| v.authorization_epoch += 1,
        |v| v.spend_key_ref[0] ^= 1,
        |v| v.recipient[0] ^= 1,
        |v| v.amount += 1,
        |v| v.max_fee += 1,
        |v| v.nonce += 1,
        |v| v.not_before_height += 1,
        |v| v.expires_at_height += 1,
        |v| v.authority = novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller,
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut changed = original.clone();
        mutate(&mut changed.intent);
        changed.intent.validate().unwrap();
        assert!(
            verifier.verify(changed).is_err(),
            "changed intent field {index}"
        );
    }
    let mut bad_signature = original.clone();
    bad_signature.signature[0] ^= 1;
    assert!(verifier.verify(bad_signature).is_err());
    let (other_public, other_secret) = runtime.mldsa_keygen_v1(65).unwrap();
    let other_secret = TestSecret(other_secret);
    let mut replaced_key = original.clone();
    replaced_key.public_key = other_public.clone();
    assert!(verifier.verify(replaced_key.clone()).is_err());
    replaced_key.intent.spend_key_ref = spend_key_ref_v1(&other_public).unwrap();
    replaced_key.validate().unwrap();
    assert!(verifier.verify(replaced_key).is_err());

    // The exact context belongs to the upstream contract. A valid signature
    // under another purpose or raw/internal framing is not an asset signature.
    let generic = MldsaSigner::new(&runtime, AoemMldsaParameterSet::MlDsa65).unwrap();
    let message = original.intent.signing_bytes().unwrap();
    for context in [
        b"NOVOVM/UCA/device/v1".as_slice(),
        b"NOVOVM/asset/NOV-spend/v2".as_slice(),
        b"".as_slice(),
    ] {
        let mut other_context = original.clone();
        other_context.signature = generic
            .sign(&public_key, &secret_key.0, &message, context)
            .unwrap();
        assert!(verifier.verify(other_context).is_err());
    }
    let mut raw = original.clone();
    raw.signature = runtime.mldsa_sign_v1(65, &secret_key.0, &message).unwrap();
    assert!(verifier.verify(raw).is_err());

    // Neither wire metadata nor supplied lengths negotiate another suite. Even
    // otherwise valid ML-DSA-44/87 signatures cannot enter the spend facade.
    for level in [44, 87] {
        let parameters = AoemMldsaParameterSet::try_from(level).unwrap();
        let (key, secret) = runtime.mldsa_keygen_v1(level).unwrap();
        let secret = TestSecret(secret);
        let other = MldsaSigner::new(&runtime, parameters).unwrap();
        let mut wrong_suite = original.clone();
        wrong_suite.signature = other
            .sign(&key, &secret.0, &message, SPEND_CONTEXT_V3)
            .unwrap();
        wrong_suite.public_key = key;
        assert!(verifier.verify(wrong_suite).is_err());
        let mut wrong_header = encoded.clone();
        wrong_header[5] = level as u8;
        assert!(SignedAssetSpendV3::decode(&wrong_header).is_err());
    }
    let mut ed_shape = original.clone();
    ed_shape.public_key = vec![7; 32];
    ed_shape.signature = vec![8; 64];
    assert!(verifier.verify(ed_shape).is_err());
    for offset in [4, 6] {
        let mut changed = encoded.clone();
        changed[offset] ^= 1;
        assert!(SignedAssetSpendV3::decode(&changed).is_err());
    }

    // This only demonstrates stable replay-domain derivation across key/epoch
    // changes. Trusted authorization and nonce consumption belong to the node.
    let mut rotated = original.intent.clone();
    rotated.authorization_epoch += 1;
    rotated.spend_key_ref = spend_key_ref_v1(&other_public).unwrap();
    assert_eq!(
        rotated.nonce_identity().unwrap(),
        original.intent.nonce_identity().unwrap()
    );
    let rotated_signed = signer
        .sign(rotated, &other_public, &other_secret.0)
        .unwrap();
    verifier.verify(rotated_signed).unwrap();
    let mut other_chain = original.intent.clone();
    other_chain.genesis[0] ^= 1;
    assert_ne!(
        other_chain.nonce_identity().unwrap(),
        original.intent.nonce_identity().unwrap()
    );

    // The same cryptographic consumer signs explicit account-management
    // effects. Valid cryptography still does NOT grant management authority.
    let mut management = original.intent.clone();
    management.authority = novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller;
    management.amount = 0;
    management.recipient = management.payer.clone();
    management.authority_update = Some(novovm_protocol::uca_spend::AssetAuthorityUpdateV2 {
        use_recovery: false,
        next_epoch: management.authorization_epoch + 1,
        active: true,
        management_key_ref: [0x71; 32],
        recovery_key_ref: Some([0x72; 32]),
        grants: Vec::new(),
    });
    let signed_management = signer.sign(management, &public_key, &secret_key.0).unwrap();
    let checked = verifier.verify(signed_management.clone()).unwrap();
    assert!(checked.check_supplied_state(None, &[5; 32], 101).is_err());
    for field in 0..6 {
        let mut altered = signed_management.clone();
        let update = altered.intent.authority_update.as_mut().unwrap();
        match field {
            0 => update.active = false,
            1 => update.use_recovery = true,
            2 => update.management_key_ref[0] ^= 1,
            3 => update.recovery_key_ref = None,
            4 => update
                .grants
                .push(novovm_protocol::uca_spend::AssetGrantConfigV2 {
                    key_ref: [0x73; 32],
                    permits_nov_transfer: true,
                    not_before_height: 101,
                    expires_at_height: 200,
                    max_debit_per_intent: 5,
                    remaining_debit_budget: 10,
                }),
            5 => altered.intent.authority_update = None,
            _ => unreachable!(),
        }
        altered.validate().unwrap();
        assert!(
            verifier.verify(altered).is_err(),
            "management field {field}"
        );
    }
}

#[test]
#[ignore = "requires the trusted packaged AOEM runtime; run explicitly, no silent skip"]
fn packaged_uca_batch_matches_scalar_mixed_rows_and_preserves_qualified_owner() {
    let facade = AoemExecFacade::open(packaged_library(), AoemExecOpenOptions::default()).unwrap();
    let runtime = facade
        .mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)
        .unwrap();
    let key = runtime.generate_keypair().unwrap();
    let signer = facade.asset_spend_signer_v3().unwrap();
    let signed = signer
        .sign(
            intent(&key.public_key),
            &key.public_key,
            &key.expanded_secret_key,
        )
        .unwrap();
    let mut next = signed.intent.clone();
    next.nonce += 1;
    let second = signer
        .sign(next, &key.public_key, &key.expanded_secret_key)
        .unwrap();
    let mut wrong_context = signed.clone();
    wrong_context.signature = runtime
        .sign(
            &key.public_key,
            &key.expanded_secret_key,
            &signed.intent.signing_bytes().unwrap(),
            b"not-a-spend-context",
        )
        .unwrap();
    drop(runtime);
    let verifier = facade.asset_spend_verifier_v3().unwrap();
    // Repeated signed values intentionally test crypto only, not nonce or spend
    // permission. Enough well-shaped rows exercise AOEM's existing bulk path.
    let rows: Vec<_> = (0..320)
        .map(|index| {
            let mut row = if index % 11 == 7 {
                second.clone()
            } else {
                signed.clone()
            };
            match index % 11 {
                0 => row.public_key.truncate(32),
                1 => row.signature[0] ^= 1,
                2 => row.intent.amount += 1,
                3 => row = wrong_context.clone(),
                4 => row.intent.spend_key_ref[0] ^= 1,
                5 => row.signature.truncate(64),
                6 => row.intent.expires_at_height = row.intent.not_before_height,
                8 => {
                    row.intent.authority =
                        novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller
                }
                _ => {}
            }
            row
        })
        .collect();
    let expected: Vec<_> = rows
        .iter()
        .map(|row| verifier.verify(row.clone()).is_ok())
        .collect();
    let shared = verifier.clone();
    drop(verifier);
    drop(facade);
    let actual = shared
        .verify_batch(rows.clone())
        .expect("actual batch execution, no scalar fallback");
    assert_eq!(actual.len(), rows.len());
    for (index, result) in actual.into_iter().enumerate() {
        assert_eq!(
            result.is_ok(),
            expected[index],
            "original batch row {index}"
        );
        if let Ok(checked) = result {
            assert!(checked.signed() == &rows[index]);
            assert!(checked.clone().signed() == checked.signed());
            assert!(checked.check_supplied_state(None, &[0; 32], 101).is_err());
        }
    }
    assert!(shared.verify_batch(vec![signed; 1025]).is_err());
    assert!(shared.verify_batch(vec![]).unwrap().is_empty());
}
