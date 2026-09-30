use super::*;
use std::cell::Cell;

struct Backend {
    available: bool,
    sizes: Result<(usize, usize), PqVerificationError>,
    result: Result<bool, PqVerificationError>,
    calls: Cell<usize>,
}

impl Backend {
    fn new(parameters: AoemMldsaParameterSet) -> Self {
        Self {
            available: true,
            sizes: Ok((parameters.public_key_bytes(), parameters.signature_bytes())),
            result: Ok(true),
            calls: Cell::new(0),
        }
    }
}

impl MldsaBackend for Backend {
    fn available(&self) -> bool {
        self.available
    }

    fn sizes(&self, level: u32) -> Result<(usize, usize), PqVerificationError> {
        assert!(matches!(level, 44 | 65 | 87));
        self.sizes
    }

    fn verify(
        &self,
        level: u32,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, PqVerificationError> {
        let parameters = AoemMldsaParameterSet::try_from(level).unwrap();
        assert_eq!(public_key, vec![0x15; parameters.public_key_bytes()]);
        assert_eq!(signature, vec![0x26; parameters.signature_bytes()]);
        assert_eq!(message, b"caller-provided canonical message");
        self.calls.set(self.calls.get() + 1);
        self.result
    }
}

const PARAMETER_SETS: [AoemMldsaParameterSet; 3] = [
    AoemMldsaParameterSet::MlDsa44,
    AoemMldsaParameterSet::MlDsa65,
    AoemMldsaParameterSet::MlDsa87,
];

fn check(
    backend: &Backend,
    parameters: AoemMldsaParameterSet,
    key_len: usize,
    signature_len: usize,
) -> Result<(), PqVerificationError> {
    verify_with_backend(
        backend,
        parameters,
        &vec![0x15; key_len],
        b"caller-provided canonical message",
        &vec![0x26; signature_len],
    )
}

#[test]
fn only_explicit_canonical_parameter_sets_are_accepted() {
    for (parameters, level, key_bytes, signature_bytes) in [
        (AoemMldsaParameterSet::MlDsa44, 44, 1312, 2420),
        (AoemMldsaParameterSet::MlDsa65, 65, 1952, 3309),
        (AoemMldsaParameterSet::MlDsa87, 87, 2592, 4627),
    ] {
        assert_eq!(AoemMldsaParameterSet::try_from(level), Ok(parameters));
        assert_eq!(parameters.level(), level);
        assert_eq!(parameters.public_key_bytes(), key_bytes);
        assert_eq!(parameters.signature_bytes(), signature_bytes);
    }
    for unsupported in [0, 1, 2, 3, 5, 43, 64, 86, 88, u32::MAX] {
        assert_eq!(
            AoemMldsaParameterSet::try_from(unsupported),
            Err(PqVerificationError::UnsupportedParameterSet)
        );
    }
}

#[test]
fn malformed_keys_never_reach_crypto_backend() {
    for parameters in PARAMETER_SETS {
        let backend = Backend::new(parameters);
        let expected = parameters.public_key_bytes();
        for key_len in [0, 32, expected - 1, expected + 1, expected * 2] {
            assert_eq!(
                check(&backend, parameters, key_len, parameters.signature_bytes()),
                Err(PqVerificationError::InvalidPublicKeyLength)
            );
        }
        assert_eq!(backend.calls.get(), 0);
    }
}

#[test]
fn malformed_signatures_never_reach_crypto_backend() {
    for parameters in PARAMETER_SETS {
        let backend = Backend::new(parameters);
        let expected = parameters.signature_bytes();
        for signature_len in [0, 64, expected - 1, expected + 1, expected * 2] {
            assert_eq!(
                check(
                    &backend,
                    parameters,
                    parameters.public_key_bytes(),
                    signature_len
                ),
                Err(PqVerificationError::InvalidSignatureLength)
            );
        }
        assert_eq!(backend.calls.get(), 0);
    }
}

#[test]
fn parameter_set_is_not_inferred_from_input_lengths() {
    for expected in PARAMETER_SETS {
        let backend = Backend::new(expected);
        for supplied in PARAMETER_SETS {
            if supplied != expected {
                assert_eq!(
                    check(
                        &backend,
                        expected,
                        supplied.public_key_bytes(),
                        supplied.signature_bytes()
                    ),
                    Err(PqVerificationError::InvalidPublicKeyLength)
                );
            }
        }
        assert_eq!(backend.calls.get(), 0);
    }
}

#[test]
fn unavailable_or_incompatible_backend_fails_closed() {
    for parameters in PARAMETER_SETS {
        let mut backend = Backend::new(parameters);
        backend.available = false;
        assert_eq!(
            check(
                &backend,
                parameters,
                parameters.public_key_bytes(),
                parameters.signature_bytes()
            ),
            Err(PqVerificationError::RuntimeUnavailable)
        );
        backend.available = true;
        for sizes in [(0, 0), (32, 64), (parameters.public_key_bytes(), 1)] {
            backend.sizes = Ok(sizes);
            assert_eq!(
                check(
                    &backend,
                    parameters,
                    parameters.public_key_bytes(),
                    parameters.signature_bytes()
                ),
                Err(PqVerificationError::RuntimeContractMismatch)
            );
        }
        backend.sizes = Err(PqVerificationError::RuntimeFailure);
        assert_eq!(
            check(
                &backend,
                parameters,
                parameters.public_key_bytes(),
                parameters.signature_bytes()
            ),
            Err(PqVerificationError::RuntimeFailure)
        );
        assert_eq!(backend.calls.get(), 0);
    }
}

#[test]
fn explicit_verifier_result_is_required_without_retry_or_fallback() {
    for parameters in PARAMETER_SETS {
        let mut backend = Backend::new(parameters);
        for (result, expected) in [
            (Ok(true), Ok(())),
            (Ok(false), Err(PqVerificationError::InvalidSignature)),
            (
                Err(PqVerificationError::RuntimeFailure),
                Err(PqVerificationError::RuntimeFailure),
            ),
        ] {
            backend.result = result;
            let before = backend.calls.get();
            assert_eq!(
                check(
                    &backend,
                    parameters,
                    parameters.public_key_bytes(),
                    parameters.signature_bytes()
                ),
                expected
            );
            assert_eq!(backend.calls.get(), before + 1);
        }
    }
}

#[test]
fn external_pure_mode_has_explicit_context_framing() {
    assert_eq!(frame_pure_message(b"", b"").unwrap(), [0, 0]);
    assert_eq!(
        frame_pure_message(b"bc", b"a").unwrap(),
        [0, 1, b'a', b'b', b'c']
    );
    assert_ne!(
        frame_pure_message(b"bc", b"a").unwrap(),
        frame_pure_message(b"c", b"ab").unwrap()
    );
    let context = vec![7; 255];
    let framed = frame_pure_message(b"payload", &context).unwrap();
    assert_eq!(&framed[..2], &[0, 255]);
    assert_eq!(&framed[2..257], context);
    assert_eq!(&framed[257..], b"payload");
    assert_eq!(
        frame_pure_message(b"payload", &[0; 256]),
        Err(PqVerificationError::InvalidContextLength)
    );
}

struct SentinelBackend {
    parameters: AoemMldsaParameterSet,
    override_result: Option<bool>,
    calls: Cell<usize>,
}

impl MldsaBackend for SentinelBackend {
    fn available(&self) -> bool {
        true
    }

    fn sizes(&self, level: u32) -> Result<(usize, usize), PqVerificationError> {
        assert_eq!(level, self.parameters.level());
        Ok((
            self.parameters.public_key_bytes(),
            self.parameters.signature_bytes(),
        ))
    }

    fn verify(
        &self,
        level: u32,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, PqVerificationError> {
        assert_eq!(level, self.parameters.level());
        let vector = compatibility_vector(self.parameters).unwrap();
        assert_eq!(public_key, vector_bytes(&vector.public_key).unwrap());
        assert_eq!(
            message,
            frame_pure_message(
                &vector_bytes(&vector.message).unwrap(),
                &vector_bytes(&vector.context).unwrap()
            )
            .unwrap()
        );
        self.calls.set(self.calls.get() + 1);
        Ok(self
            .override_result
            .unwrap_or(signature == vector_bytes(&vector.signature).unwrap()))
    }
}

#[test]
fn compatibility_gate_requires_both_positive_and_negative_sentinels() {
    for parameters in PARAMETER_SETS {
        for (override_result, expected, calls) in [
            (None, Ok(()), 2),
            (
                Some(false),
                Err(PqVerificationError::RuntimeIncompatible),
                1,
            ),
            (Some(true), Err(PqVerificationError::RuntimeIncompatible), 2),
        ] {
            let backend = SentinelBackend {
                parameters,
                override_result,
                calls: Cell::new(0),
            };
            assert_eq!(ensure_compatible_backend(&backend, parameters), expected);
            assert_eq!(backend.calls.get(), calls);
        }
    }
}

#[test]
fn bundled_vectors_have_exact_profile_and_encoding_sizes() {
    for parameters in PARAMETER_SETS {
        let vector = compatibility_vector(parameters).unwrap();
        let public_key = vector_bytes(&vector.public_key).unwrap();
        let signature = vector_bytes(&vector.signature).unwrap();
        let message = vector_bytes(&vector.message).unwrap();
        let context = vector_bytes(&vector.context).unwrap();
        assert_eq!(check_lengths(parameters, &public_key, &signature), Ok(()));
        assert_eq!(
            frame_pure_message(&message, &context).unwrap().len(),
            message.len() + context.len() + 2
        );
    }
    for invalid in ["f", "gg", "☺"] {
        assert_eq!(
            vector_bytes(invalid),
            Err(PqVerificationError::CompatibilityVectorInvalid)
        );
    }
}
