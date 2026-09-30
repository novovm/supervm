use aoem_bindings::AoemDyn;
use novovm_prover::pq_signature::{AoemMldsaParameterSet, MldsaVerifier, PqVerificationError};
use std::path::PathBuf;

fn packaged_runtime() -> AoemDyn {
    let (platform, library) = if cfg!(target_os = "windows") {
        ("windows", "aoem_ffi.dll")
    } else if cfg!(target_os = "macos") {
        ("macos", "libaoem_ffi.dylib")
    } else {
        ("linux", "libaoem_ffi.so")
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../aoem")
        .join(platform)
        .join("core/bin")
        .join(library);
    unsafe { AoemDyn::load(path) }.expect("load trusted repository AOEM package")
}

fn bytes(hex: &str) -> Vec<u8> {
    assert!(hex.len().is_multiple_of(2));
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn framed(message: &[u8], context: &[u8]) -> Vec<u8> {
    let mut result = vec![0, u8::try_from(context.len()).unwrap()];
    result.extend_from_slice(context);
    result.extend_from_slice(message);
    result
}

#[test]
#[ignore = "requires the trusted packaged AOEM runtime; run explicitly, no silent skip"]
fn packaged_runtime_requires_all_standard_mldsa_parameters() {
    let runtime = packaged_runtime();
    for (parameters, json) in [
        (
            AoemMldsaParameterSet::MlDsa44,
            include_str!("../src/pq_signature_fixtures/mldsa44.json"),
        ),
        (
            AoemMldsaParameterSet::MlDsa65,
            include_str!("../src/pq_signature_fixtures/mldsa65.json"),
        ),
        (
            AoemMldsaParameterSet::MlDsa87,
            include_str!("../src/pq_signature_fixtures/mldsa87.json"),
        ),
    ] {
        let level = parameters.level();
        let (own_key, mut secret_key) = runtime.mldsa_keygen_v1(level).expect("ephemeral test key");
        let own_signature = runtime
            .mldsa_sign_v1(level, &secret_key, b"self-test")
            .unwrap();
        assert!(runtime
            .mldsa_verify_v1(level, &own_key, b"self-test", &own_signature)
            .unwrap());
        secret_key.fill(0);

        let vector: serde_json::Value = serde_json::from_str(json).unwrap();
        let public_key = bytes(vector["public_key"].as_str().unwrap());
        let message = bytes(vector["message"].as_str().unwrap());
        let context = bytes(vector["context"].as_str().unwrap());
        let signature = bytes(vector["signature"].as_str().unwrap());
        let compatible = runtime
            .mldsa_verify_v1(level, &public_key, &framed(&message, &context), &signature)
            .unwrap();
        assert!(
            compatible,
            "packaged ML-DSA-{level} must accept the official positive"
        );
        let verifier = MldsaVerifier::new(&runtime, parameters)
            .expect("all packaged parameter sets must pass standard qualification");
        verifier
            .verify(&public_key, &message, &context, &signature)
            .unwrap();
        let mut changed_signature = signature.clone();
        changed_signature[0] ^= 1;
        assert_eq!(
            verifier.verify(&public_key, &message, &context, &changed_signature),
            Err(PqVerificationError::InvalidSignature)
        );
        println!("level={level}: self_roundtrip=true official_positive={compatible} usable_for_standard_profile={compatible}; not main-chain or FIPS certification");
    }
}

#[test]
#[ignore = "requires the trusted packaged AOEM runtime; run explicitly, no silent skip"]
fn standard_parameters_reject_tampering_wrong_context_and_legacy_raw_signatures() {
    let runtime = packaged_runtime();
    for parameters in [
        AoemMldsaParameterSet::MlDsa44,
        AoemMldsaParameterSet::MlDsa65,
        AoemMldsaParameterSet::MlDsa87,
    ] {
        verify_parameter_rejections(&runtime, parameters);
    }
}

fn verify_parameter_rejections(runtime: &AoemDyn, parameters: AoemMldsaParameterSet) {
    let verifier = MldsaVerifier::new(runtime, parameters).expect("known-answer qualification");
    let level = parameters.level();
    let (public_key, mut secret_key) = runtime.mldsa_keygen_v1(level).unwrap();
    let (other_key, mut other_secret_key) = runtime.mldsa_keygen_v1(level).unwrap();
    other_secret_key.fill(0);
    for context in [
        b"".as_slice(),
        b"test-only transaction domain".as_slice(),
        &[7; 255],
    ] {
        for message in [
            b"".as_slice(),
            b"test-only:chain=17:nonce=4:amount=7".as_slice(),
        ] {
            let signature = runtime
                .mldsa_sign_v1(level, &secret_key, &framed(message, context))
                .unwrap();
            for _ in 0..2 {
                verifier
                    .verify(&public_key, message, context, &signature)
                    .unwrap();
            }
            let mut changed_message = message.to_vec();
            changed_message.push(1);
            assert_eq!(
                verifier.verify(&public_key, &changed_message, context, &signature),
                Err(PqVerificationError::InvalidSignature)
            );
            let wrong_context = if context.is_empty() {
                b"different".as_slice()
            } else {
                b"".as_slice()
            };
            assert_eq!(
                verifier.verify(&public_key, message, wrong_context, &signature),
                Err(PqVerificationError::InvalidSignature)
            );
            assert_eq!(
                verifier.verify(&other_key, message, context, &signature),
                Err(PqVerificationError::InvalidSignature)
            );
            let mut changed_signature = signature.clone();
            changed_signature[0] ^= 1;
            assert_eq!(
                verifier.verify(&public_key, message, context, &changed_signature),
                Err(PqVerificationError::InvalidSignature)
            );
            assert_eq!(
                verifier.verify(
                    &public_key,
                    message,
                    context,
                    &signature[..signature.len() - 1]
                ),
                Err(PqVerificationError::InvalidSignatureLength)
            );
            let mut extended = signature.clone();
            extended.push(0);
            assert_eq!(
                verifier.verify(&public_key, message, context, &extended),
                Err(PqVerificationError::InvalidSignatureLength)
            );
            assert_eq!(
                verifier.verify(
                    &public_key[..public_key.len() - 1],
                    message,
                    context,
                    &signature
                ),
                Err(PqVerificationError::InvalidPublicKeyLength)
            );
            let mut extended_key = public_key.clone();
            extended_key.push(0);
            assert_eq!(
                verifier.verify(&extended_key, message, context, &signature),
                Err(PqVerificationError::InvalidPublicKeyLength)
            );
            assert_eq!(
                verifier.verify(&public_key, message, &[0; 256], &signature),
                Err(PqVerificationError::InvalidContextLength)
            );
            let raw_signature = runtime.mldsa_sign_v1(level, &secret_key, message).unwrap();
            assert_eq!(
                verifier.verify(&public_key, message, context, &raw_signature),
                Err(PqVerificationError::InvalidSignature)
            );
        }
    }
    let message = b"test-only:chain=17:nonce=4:amount=7";
    let context = b"test-only transaction domain";
    let signature = runtime
        .mldsa_sign_v1(level, &secret_key, &framed(message, context))
        .unwrap();
    for changed in [
        b"test-only:chain=18:nonce=4:amount=7",
        b"test-only:chain=17:nonce=5:amount=7",
    ] {
        assert_eq!(
            verifier.verify(&public_key, changed, context, &signature),
            Err(PqVerificationError::InvalidSignature)
        );
    }
    assert_eq!(
        verifier.verify(
            &public_key,
            message,
            b"test-only block-seal domain",
            &signature
        ),
        Err(PqVerificationError::InvalidSignature)
    );
    secret_key.fill(0);
}
