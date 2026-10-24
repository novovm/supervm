//! Real packaged-runtime checks of the shared signature contract only.
//! These checks do not supply trusted UCA state or execute a native transaction.
use aoem_bindings::AoemDyn;
use novovm_exec::pq_signature::{AoemMldsaParameterSet, MldsaSigner};
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, SignedUcaSpendV1, UcaSpendIntentV1, MLDSA65_PUBLIC_KEY_BYTES,
    MLDSA65_SIGNATURE_BYTES, SPEND_CONTEXT_V1,
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

fn intent(public_key: &[u8]) -> UcaSpendIntentV1 {
    UcaSpendIntentV1 {
        chain_id: 97,
        genesis: [0x31; 32],
        account_id: "uca:test-payer".into(),
        authorization_epoch: 4,
        spend_key_ref: spend_key_ref_v1(public_key).unwrap(),
        recipient_uca: "uca:test-recipient".into(),
        amount: 12_000_000_000,
        max_fee: 100_000,
        nonce: 29,
        not_before_height: 100,
        expires_at_height: 120,
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
        .uca_spend_signer_v1()
        .expect("qualified ML-DSA-65 signer");
    let verifier = facade
        .uca_spend_verifier_v1()
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
    let decoded = SignedUcaSpendV1::decode(&encoded).unwrap();
    assert!(decoded == original);
    let checked = verifier.verify(decoded).unwrap();
    assert_eq!(checked.intent(), &original.intent);
    assert!(checked.signed() == &original);
    // Valid cryptography must not turn absent UCA state into permission.
    assert_eq!(
        checked.check_supplied_state(None, &[5; 32], 101),
        Err(novovm_adapter_api::uca_spend_policy::SpendPolicyError::StateUnavailable)
    );

    // Each changed intent is still structurally valid. The altered key reference
    // fails binding; the other altered fields must fail signature verification.
    let mutations: [fn(&mut UcaSpendIntentV1); 11] = [
        |v| v.chain_id += 1,
        |v| v.genesis[0] ^= 1,
        |v| v.account_id.push('x'),
        |v| v.authorization_epoch += 1,
        |v| v.spend_key_ref[0] ^= 1,
        |v| v.recipient_uca.push('x'),
        |v| v.amount += 1,
        |v| v.max_fee += 1,
        |v| v.nonce += 1,
        |v| v.not_before_height += 1,
        |v| v.expires_at_height += 1,
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
    for context in [b"NOVOVM/UCA/device/v1".as_slice(), b"".as_slice()] {
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
            .sign(&key, &secret.0, &message, SPEND_CONTEXT_V1)
            .unwrap();
        wrong_suite.public_key = key;
        assert!(verifier.verify(wrong_suite).is_err());
        let mut wrong_header = encoded.clone();
        wrong_header[5] = level as u8;
        assert!(SignedUcaSpendV1::decode(&wrong_header).is_err());
    }
    let mut ed_shape = original.clone();
    ed_shape.public_key = vec![7; 32];
    ed_shape.signature = vec![8; 64];
    assert!(verifier.verify(ed_shape).is_err());
    for offset in [4, 6] {
        let mut changed = encoded.clone();
        changed[offset] ^= 1;
        assert!(SignedUcaSpendV1::decode(&changed).is_err());
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
}
