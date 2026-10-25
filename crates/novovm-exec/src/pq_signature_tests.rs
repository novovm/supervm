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
    for (parameters, level, key_bytes, signature_bytes, secret_key_bytes) in [
        (AoemMldsaParameterSet::MlDsa44, 44, 1312, 2420, 2560),
        (AoemMldsaParameterSet::MlDsa65, 65, 1952, 3309, 4032),
        (AoemMldsaParameterSet::MlDsa87, 87, 2592, 4627, 4896),
    ] {
        assert_eq!(AoemMldsaParameterSet::try_from(level), Ok(parameters));
        assert_eq!(parameters.level(), level);
        assert_eq!(parameters.public_key_bytes(), key_bytes);
        assert_eq!(parameters.signature_bytes(), signature_bytes);
        assert_eq!(parameters.secret_key_bytes(), secret_key_bytes);
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

struct SigningBackend {
    verifier: Backend,
    signing_available: bool,
    secret_size: Result<usize, PqVerificationError>,
    sign_result: Result<usize, PqVerificationError>,
    sign_calls: Cell<usize>,
    secret_pointer: Cell<*const u8>,
}

impl SigningBackend {
    fn new(parameters: AoemMldsaParameterSet) -> Self {
        Self {
            verifier: Backend::new(parameters),
            signing_available: true,
            secret_size: Ok(parameters.secret_key_bytes()),
            sign_result: Ok(parameters.signature_bytes()),
            sign_calls: Cell::new(0),
            secret_pointer: Cell::new(std::ptr::null()),
        }
    }
}

fn assert_signing_message(message: &[u8]) {
    assert_eq!(
        message,
        b"\x00\x0ctest-contextcaller-provided canonical message"
    );
}

impl MldsaBackend for SigningBackend {
    fn available(&self) -> bool {
        self.verifier.available()
    }

    fn sizes(&self, level: u32) -> Result<(usize, usize), PqVerificationError> {
        self.verifier.sizes(level)
    }

    fn verify(
        &self,
        level: u32,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, PqVerificationError> {
        assert_signing_message(message);
        self.verifier.verify(
            level,
            public_key,
            b"caller-provided canonical message",
            signature,
        )
    }
}

impl MldsaSigningBackend for SigningBackend {
    fn signing_available(&self) -> bool {
        self.signing_available
    }

    fn secret_key_size(&self, level: u32) -> Result<usize, PqVerificationError> {
        assert!(matches!(level, 44 | 65 | 87));
        self.secret_size
    }

    fn sign(
        &self,
        level: u32,
        expanded_secret_key: &[u8],
        signing_message: &[u8],
    ) -> Result<Vec<u8>, PqVerificationError> {
        let parameters = AoemMldsaParameterSet::try_from(level).unwrap();
        assert_eq!(expanded_secret_key.len(), parameters.secret_key_bytes());
        assert!(std::ptr::eq(
            expanded_secret_key.as_ptr(),
            self.secret_pointer.get()
        ));
        assert_signing_message(signing_message);
        self.sign_calls.set(self.sign_calls.get() + 1);
        self.sign_result.map(|size| vec![0x26; size])
    }
}

fn check_signing(
    backend: &SigningBackend,
    parameters: AoemMldsaParameterSet,
    key_len: usize,
    secret_len: usize,
    context: &[u8],
) -> Result<Vec<u8>, PqVerificationError> {
    // Synthetic bytes are used only by this mock, never by a real runtime.
    let secret_key = vec![0x37; secret_len];
    backend.secret_pointer.set(secret_key.as_ptr());
    sign_with_backend(
        backend,
        parameters,
        &vec![0x15; key_len],
        &secret_key,
        b"caller-provided canonical message",
        context,
    )
}

#[test]
fn malformed_signing_inputs_are_rejected_before_the_backend() {
    for parameters in PARAMETER_SETS {
        let backend = SigningBackend::new(parameters);
        let secret_size = parameters.secret_key_bytes();
        let key_size = parameters.public_key_bytes();
        for secret_len in [0, 32, secret_size - 1, secret_size + 1, secret_size * 2] {
            assert_eq!(
                check_signing(&backend, parameters, key_size, secret_len, b"test-context"),
                Err(PqVerificationError::InvalidSecretKeyLength)
            );
        }
        for key_len in [0, 32, key_size - 1, key_size + 1] {
            assert_eq!(
                check_signing(&backend, parameters, key_len, secret_size, b"test-context"),
                Err(PqVerificationError::InvalidPublicKeyLength)
            );
        }
        assert_eq!(
            check_signing(&backend, parameters, key_size, secret_size, &[0; 256]),
            Err(PqVerificationError::InvalidContextLength)
        );
        assert_eq!(backend.sign_calls.get(), 0);
        assert_eq!(backend.verifier.calls.get(), 0);
    }
}

#[test]
fn signing_capabilities_and_all_encoding_sizes_fail_closed() {
    for parameters in PARAMETER_SETS {
        let mut backend = SigningBackend::new(parameters);
        let check = |backend: &SigningBackend| {
            check_signing(
                backend,
                parameters,
                parameters.public_key_bytes(),
                parameters.secret_key_bytes(),
                b"test-context",
            )
        };
        backend.verifier.available = false;
        assert_eq!(
            check(&backend),
            Err(PqVerificationError::RuntimeUnavailable)
        );
        backend.verifier.available = true;
        backend.signing_available = false;
        assert_eq!(
            check(&backend),
            Err(PqVerificationError::SigningUnavailable)
        );
        backend.signing_available = true;
        for sizes in [(0, 0), (32, 64), (parameters.public_key_bytes(), 1)] {
            backend.verifier.sizes = Ok(sizes);
            assert_eq!(
                check(&backend),
                Err(PqVerificationError::RuntimeContractMismatch)
            );
        }
        backend.verifier.sizes = Err(PqVerificationError::RuntimeFailure);
        assert_eq!(check(&backend), Err(PqVerificationError::RuntimeFailure));
        backend.verifier.sizes = Ok((parameters.public_key_bytes(), parameters.signature_bytes()));
        for secret_size in [
            0,
            32,
            parameters.secret_key_bytes() - 1,
            parameters.secret_key_bytes() + 1,
        ] {
            backend.secret_size = Ok(secret_size);
            assert_eq!(
                check(&backend),
                Err(PqVerificationError::RuntimeContractMismatch)
            );
        }
        backend.secret_size = Err(PqVerificationError::SigningFailure);
        assert_eq!(check(&backend), Err(PqVerificationError::SigningFailure));
        assert_eq!(backend.sign_calls.get(), 0);
        assert_eq!(backend.verifier.calls.get(), 0);
    }
}

#[test]
fn failed_or_malformed_signing_output_is_never_returned_or_retried() {
    for parameters in PARAMETER_SETS {
        let mut backend = SigningBackend::new(parameters);
        for (result, expected) in [
            (
                Err(PqVerificationError::SigningFailure),
                PqVerificationError::SigningFailure,
            ),
            (Ok(0), PqVerificationError::InvalidSignatureLength),
            (
                Ok(parameters.signature_bytes() - 1),
                PqVerificationError::InvalidSignatureLength,
            ),
            (
                Ok(parameters.signature_bytes() + 1),
                PqVerificationError::InvalidSignatureLength,
            ),
        ] {
            backend.sign_result = result;
            let before = backend.sign_calls.get();
            assert_eq!(
                check_signing(
                    &backend,
                    parameters,
                    parameters.public_key_bytes(),
                    parameters.secret_key_bytes(),
                    b"test-context",
                ),
                Err(expected)
            );
            assert_eq!(backend.sign_calls.get(), before + 1);
            assert_eq!(backend.verifier.calls.get(), 0);
        }
    }
}

#[test]
fn signing_borrows_the_secret_frames_once_and_requires_output_self_verification() {
    for parameters in PARAMETER_SETS {
        let mut backend = SigningBackend::new(parameters);
        for (result, expected) in [
            (Ok(true), Ok(vec![0x26; parameters.signature_bytes()])),
            (Ok(false), Err(PqVerificationError::InvalidSignature)),
            (
                Err(PqVerificationError::RuntimeFailure),
                Err(PqVerificationError::RuntimeFailure),
            ),
        ] {
            backend.verifier.result = result;
            let before = backend.sign_calls.get();
            assert_eq!(
                check_signing(
                    &backend,
                    parameters,
                    parameters.public_key_bytes(),
                    parameters.secret_key_bytes(),
                    b"test-context",
                ),
                expected
            );
            assert_eq!(backend.sign_calls.get(), before + 1);
            assert_eq!(backend.verifier.calls.get(), before + 1);
        }
    }
}

struct BatchBackend {
    parameters: AoemMldsaParameterSet,
    expected_key: Vec<u8>,
    expected_message: Vec<u8>,
    expected_signature: Vec<u8>,
    mode: u8,
    calls: Cell<usize>,
    rows: Cell<usize>,
}

impl BatchBackend {
    fn known_answer(parameters: AoemMldsaParameterSet) -> Self {
        let vector = compatibility_vector(parameters).unwrap();
        Self {
            parameters,
            expected_key: vector_bytes(&vector.public_key).unwrap(),
            expected_message: frame_pure_message(
                &vector_bytes(&vector.message).unwrap(),
                &vector_bytes(&vector.context).unwrap(),
            )
            .unwrap(),
            expected_signature: vector_bytes(&vector.signature).unwrap(),
            mode: 0,
            calls: Cell::new(0),
            rows: Cell::new(0),
        }
    }
}

impl MldsaBackend for BatchBackend {
    fn available(&self) -> bool {
        self.mode != 1
    }
    fn sizes(&self, level: u32) -> Result<(usize, usize), PqVerificationError> {
        assert_eq!(level, self.parameters.level());
        if self.mode == 2 {
            Ok((32, 64))
        } else {
            Ok((
                self.parameters.public_key_bytes(),
                self.parameters.signature_bytes(),
            ))
        }
    }
    fn verify(&self, _: u32, _: &[u8], _: &[u8], _: &[u8]) -> Result<bool, PqVerificationError> {
        panic!("batch must not retry/fall back to scalar verification")
    }
}

impl MldsaBatchBackend for BatchBackend {
    fn batch_available(&self) -> bool {
        self.mode != 3
    }
    fn verify_batch(
        &self,
        items: &[AoemMldsaVerifyItemRef<'_>],
    ) -> Result<Vec<bool>, PqVerificationError> {
        self.calls.set(self.calls.get() + 1);
        self.rows.set(self.rows.get() + items.len());
        if self.mode == 4 {
            return Err(PqVerificationError::RuntimeFailure);
        }
        let mut results: Vec<_> = items
            .iter()
            .map(|item| {
                assert_eq!(item.level, self.parameters.level());
                match self.mode {
                    5 => true,
                    6 => false,
                    _ => {
                        item.pubkey == self.expected_key
                            && item.message == self.expected_message
                            && item.signature == self.expected_signature
                    }
                }
            })
            .collect();
        if self.mode == 7 {
            results.pop();
        }
        if self.mode == 8 {
            results.push(true);
        }
        Ok(results)
    }
}

#[test]
fn batch_qualification_requires_explicit_suite_mixed_kat_and_exact_result_count() {
    for parameters in PARAMETER_SETS {
        for (mode, expected) in [
            (0, Ok(())),
            (1, Err(PqVerificationError::RuntimeUnavailable)),
            (2, Err(PqVerificationError::RuntimeContractMismatch)),
            (3, Err(PqVerificationError::BatchUnavailable)),
            (4, Err(PqVerificationError::RuntimeFailure)),
            (5, Err(PqVerificationError::RuntimeIncompatible)),
            (6, Err(PqVerificationError::RuntimeIncompatible)),
            (7, Err(PqVerificationError::RuntimeContractMismatch)),
            (8, Err(PqVerificationError::RuntimeContractMismatch)),
        ] {
            let mut backend = BatchBackend::known_answer(parameters);
            backend.mode = mode;
            assert_eq!(
                ensure_batch_compatible_backend(&backend, parameters),
                expected
            );
            assert_eq!(backend.calls.get(), usize::from(!matches!(mode, 1..=3)));
        }
    }
}

#[test]
fn batch_frames_once_and_keeps_invalid_input_and_signature_rows_in_original_order() {
    let parameters = AoemMldsaParameterSet::MlDsa65;
    let backend = BatchBackend::known_answer(parameters);
    let vector = compatibility_vector(parameters).unwrap();
    let message = vector_bytes(&vector.message).unwrap();
    let context = vector_bytes(&vector.context).unwrap();
    let mut bad_signature = backend.expected_signature.clone();
    bad_signature[0] ^= 1;
    let good = MldsaVerifyInput {
        public_key: &backend.expected_key,
        message: &message,
        context: &context,
        signature: &backend.expected_signature,
    };
    let inputs = [
        good,
        MldsaVerifyInput {
            public_key: &[0; 32],
            ..good
        },
        MldsaVerifyInput {
            signature: &bad_signature,
            ..good
        },
        MldsaVerifyInput {
            context: &[0; 256],
            ..good
        },
        good,
        MldsaVerifyInput {
            signature: &[0; 64],
            ..good
        },
        MldsaVerifyInput {
            message: b"wrong canonical message",
            ..good
        },
    ];
    assert_eq!(
        verify_batch_with_backend(&backend, parameters, &inputs).unwrap(),
        [
            Ok(()),
            Err(PqVerificationError::InvalidPublicKeyLength),
            Err(PqVerificationError::InvalidSignature),
            Err(PqVerificationError::InvalidContextLength),
            Ok(()),
            Err(PqVerificationError::InvalidSignatureLength),
            Err(PqVerificationError::InvalidSignature),
        ]
    );
    assert_eq!(backend.calls.get(), 1);
    assert_eq!(
        backend.rows.get(),
        4,
        "malformed shapes never enter the FFI"
    );
}

#[test]
fn batch_runtime_failure_has_no_partial_success_and_work_units_are_bounded() {
    let parameters = AoemMldsaParameterSet::MlDsa65;
    let mut backend = BatchBackend::known_answer(parameters);
    let public_key = backend.expected_key.clone();
    let signature = backend.expected_signature.clone();
    let input = MldsaVerifyInput {
        public_key: &public_key,
        message: b"message",
        context: b"context",
        signature: &signature,
    };
    assert_eq!(
        verify_batch_with_backend(&backend, parameters, &[]).unwrap(),
        []
    );
    assert_eq!(
        verify_batch_with_backend(
            &backend,
            parameters,
            &vec![input; MLDSA_VERIFY_BATCH_MAX_ITEMS + 1]
        ),
        Err(PqVerificationError::BatchTooLarge)
    );
    let oversized_message = vec![0; MLDSA_VERIFY_BATCH_MAX_MESSAGE_BYTES];
    assert_eq!(
        verify_batch_with_backend(
            &backend,
            parameters,
            &[MldsaVerifyInput {
                message: &oversized_message,
                ..input
            }]
        ),
        Err(PqVerificationError::BatchTooLarge)
    );
    assert_eq!(backend.calls.get(), 0);
    backend.mode = 4;
    assert_eq!(
        verify_batch_with_backend(
            &backend,
            parameters,
            &[
                input,
                MldsaVerifyInput {
                    public_key: &[0; 32],
                    ..input
                }
            ]
        ),
        Err(PqVerificationError::RuntimeFailure)
    );
    assert_eq!(backend.calls.get(), 1);
}
