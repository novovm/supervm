use aoem_bindings::AoemDyn;
use novovm_prover::pq_signature::{
    AoemMldsaParameterSet, MldsaSigner, MldsaVerifier, PqVerificationError,
};
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

#[test]
#[ignore = "requires the trusted packaged AOEM runtime; run explicitly, no silent skip"]
fn packaged_signer_pairs_with_standard_verifier_for_all_parameter_sets() {
    let runtime = packaged_runtime();
    for parameters in [
        AoemMldsaParameterSet::MlDsa44,
        AoemMldsaParameterSet::MlDsa65,
        AoemMldsaParameterSet::MlDsa87,
    ] {
        verify_signer_pair(&runtime, parameters);
    }
}

fn verify_signer_pair(runtime: &AoemDyn, parameters: AoemMldsaParameterSet) {
    let signer = MldsaSigner::new(runtime, parameters).expect("qualified signing capability");
    let verifier = MldsaVerifier::new(runtime, parameters).expect("known-answer qualification");
    let level = parameters.level();
    // Ephemeral keys only: never print, serialize or write their secret bytes.
    let (public_key, mut secret_key) = runtime.mldsa_keygen_v1(level).unwrap();
    let (other_key, mut other_secret_key) = runtime.mldsa_keygen_v1(level).unwrap();
    assert_eq!(public_key.len(), parameters.public_key_bytes());
    assert_eq!(secret_key.len(), parameters.secret_key_bytes());
    assert_eq!(other_secret_key.len(), parameters.secret_key_bytes());
    assert_ne!(public_key, other_key);

    for context in [
        b"".as_slice(),
        b"test-only signer domain".as_slice(),
        &[7; 255],
    ] {
        for message in [
            b"".as_slice(),
            b"test-only:chain=17:nonce=4:amount=7".as_slice(),
        ] {
            let signature = signer
                .sign(&public_key, &secret_key, message, context)
                .unwrap();
            verifier
                .verify(&public_key, message, context, &signature)
                .unwrap();
            assert_eq!(signature.len(), parameters.signature_bytes());
            assert_eq!(
                signer
                    .sign(&public_key, &secret_key, message, context)
                    .unwrap(),
                signature
            );
            // Interoperate with the existing raw/internal ABI: external-pure
            // framing is added exactly once, including the empty context.
            let once = framed(message, context);
            assert_eq!(
                runtime.mldsa_sign_v1(level, &secret_key, &once).unwrap(),
                signature
            );
            assert!(runtime
                .mldsa_verify_v1(level, &public_key, &once, &signature)
                .unwrap());
            assert!(!runtime
                .mldsa_verify_v1(level, &public_key, message, &signature)
                .unwrap());
            let raw = runtime.mldsa_sign_v1(level, &secret_key, message).unwrap();
            let twice = runtime
                .mldsa_sign_v1(level, &secret_key, &framed(&once, context))
                .unwrap();
            for incompatible in [&raw, &twice] {
                assert_eq!(
                    verifier.verify(&public_key, message, context, incompatible),
                    Err(PqVerificationError::InvalidSignature)
                );
            }

            let mut changed_message = message.to_vec();
            changed_message.push(1);
            assert_eq!(
                verifier.verify(&public_key, &changed_message, context, &signature),
                Err(PqVerificationError::InvalidSignature)
            );
            let mut changed_context = context.to_vec();
            if changed_context.is_empty() {
                changed_context.push(1);
            } else {
                changed_context[0] ^= 1;
            }
            assert_eq!(
                verifier.verify(&public_key, message, &changed_context, &signature),
                Err(PqVerificationError::InvalidSignature)
            );
            let mut changed_signature = signature.clone();
            changed_signature[0] ^= 1;
            assert_eq!(
                verifier.verify(&public_key, message, context, &changed_signature),
                Err(PqVerificationError::InvalidSignature)
            );
            assert_eq!(
                verifier.verify(&other_key, message, context, &signature),
                Err(PqVerificationError::InvalidSignature)
            );
        }
    }

    let message = b"test-only borrowed expanded key";
    let context = b"test-only signer domain";
    // Both pairs really sign. The cross-pair failure must come from the output
    // self-verification, not an artificial malformed secret sent into the FFI.
    let other_signature = signer
        .sign(&other_key, &other_secret_key, message, context)
        .unwrap();
    verifier
        .verify(&other_key, message, context, &other_signature)
        .unwrap();
    assert_eq!(
        signer.sign(&other_key, &secret_key, message, context),
        Err(PqVerificationError::InvalidSignature)
    );

    for invalid_public_key in [b"".as_slice(), &public_key[..public_key.len() - 1]] {
        assert_eq!(
            signer.sign(invalid_public_key, &secret_key, message, context),
            Err(PqVerificationError::InvalidPublicKeyLength)
        );
    }
    let mut extended_public_key = public_key.clone();
    extended_public_key.push(0);
    assert_eq!(
        signer.sign(&extended_public_key, &secret_key, message, context),
        Err(PqVerificationError::InvalidPublicKeyLength)
    );
    // Length failures must be rejected by the Host before invoking the runtime.
    // In particular do not test a same-length all-zero malformed secret key.
    for invalid_secret_key in [b"".as_slice(), &secret_key[..secret_key.len() - 1]] {
        assert_eq!(
            signer.sign(&public_key, invalid_secret_key, message, context),
            Err(PqVerificationError::InvalidSecretKeyLength)
        );
    }
    // Wrong length is rejected by the Host; this is not a malformed equal-length key.
    let mut extended_secret_key = vec![0; parameters.secret_key_bytes() + 1];
    assert_eq!(
        signer.sign(&public_key, &extended_secret_key, message, context),
        Err(PqVerificationError::InvalidSecretKeyLength)
    );
    extended_secret_key.fill(0);
    assert_eq!(
        signer.sign(&public_key, &secret_key, message, &[0; 256]),
        Err(PqVerificationError::InvalidContextLength)
    );
    secret_key.fill(0);
    other_secret_key.fill(0);
}
