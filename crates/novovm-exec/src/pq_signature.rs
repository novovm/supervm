//! Shared AOEM ML-DSA signing and verification facade.
//!
//! Reused from the original prover bridge without changing its strict parameter,
//! external-pure framing, runtime compatibility or sign-then-verify contracts.
//! Protocol consumers select their suite and canonical transcript separately.

use aoem_bindings::{AoemDyn, AoemMldsaVerifyItemRef};
use serde::Deserialize;
use std::{fmt, sync::Arc};
use zeroize::Zeroizing;

/// Generated with the existing AOEM primitive, validated with this facade's
/// qualified sign/verify path, and kept zeroizing across every error path.
/// Intentionally not Debug, Clone or Serialize: consumers transfer custody.
pub struct MldsaKeyPair {
    pub public_key: Vec<u8>,
    pub expanded_secret_key: Zeroizing<Vec<u8>>,
}

/// Long-lived qualified handle over the same loaded AOEM owner. Construction
/// performs the KAT and capability checks once; sign/verify never rerun the KAT.
/// Consumers select the parameter set and own message/domain policy.
#[derive(Clone)]
pub struct MldsaRuntime {
    runtime: Arc<AoemDyn>,
    parameters: AoemMldsaParameterSet,
}

impl MldsaRuntime {
    pub fn parameters(&self) -> AoemMldsaParameterSet {
        self.parameters
    }

    pub fn sign(
        &self,
        trusted_public_key: &[u8],
        expanded_secret_key: &[u8],
        message: &[u8],
        context: &[u8],
    ) -> Result<Vec<u8>, PqVerificationError> {
        sign_with_backend(
            self.runtime.as_ref(),
            self.parameters,
            trusted_public_key,
            expanded_secret_key,
            message,
            context,
        )
    }

    pub fn verify(
        &self,
        trusted_public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<(), PqVerificationError> {
        check_lengths(self.parameters, trusted_public_key, signature)?;
        let framed = frame_pure_message(message, context)?;
        verify_with_backend(
            self.runtime.as_ref(),
            self.parameters,
            trusted_public_key,
            &framed,
            signature,
        )
    }

    pub fn generate_keypair(&self) -> Result<MldsaKeyPair, PqVerificationError> {
        if !self.runtime.supports_mldsa_keygen_v1() {
            return Err(PqVerificationError::KeyGenerationUnavailable);
        }
        let (public_key, expanded_secret_key) = self
            .runtime
            .mldsa_keygen_v1(self.parameters.level())
            .map_err(|_| PqVerificationError::KeyGenerationFailure)?;
        let key = MldsaKeyPair {
            public_key,
            expanded_secret_key: Zeroizing::new(expanded_secret_key),
        };
        self.sign(
            &key.public_key,
            &key.expanded_secret_key,
            &self.parameters.level().to_be_bytes(),
            b"AOEM/shared-MLDSA/key-generation/v1",
        )?;
        Ok(key)
    }
}

impl crate::AoemExecFacade {
    /// Borrow the already loaded runtime, retaining the existing suite and
    /// known-answer qualification. Consumers own canonical messages/purposes.
    pub fn mldsa_verifier_v1(
        &self,
        parameters: AoemMldsaParameterSet,
    ) -> Result<MldsaVerifier<'_>, PqVerificationError> {
        MldsaVerifier::new(&self.dynlib, parameters)
    }

    pub fn mldsa_signer_v1(
        &self,
        parameters: AoemMldsaParameterSet,
    ) -> Result<MldsaSigner<'_>, PqVerificationError> {
        MldsaSigner::new(&self.dynlib, parameters)
    }

    /// Shares the already trusted loaded library without reopening it. KAT and
    /// signing qualification occur once at this explicit owner creation point.
    pub fn mldsa_runtime_v1(
        &self,
        parameters: AoemMldsaParameterSet,
    ) -> Result<MldsaRuntime, PqVerificationError> {
        ensure_compatible_backend(self.dynlib.as_ref(), parameters)?;
        ensure_signing_backend(self.dynlib.as_ref(), parameters)?;
        Ok(MldsaRuntime {
            runtime: Arc::clone(&self.dynlib),
            parameters,
        })
    }

    /// Key generation is an offline caller-owned operation, not a chain action.
    /// No alternative algorithm/parameter set is selected when unavailable.
    pub fn mldsa_keygen_v1(
        &self,
        parameters: AoemMldsaParameterSet,
    ) -> Result<MldsaKeyPair, PqVerificationError> {
        self.mldsa_runtime_v1(parameters)?.generate_keypair()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AoemMldsaParameterSet {
    MlDsa44,
    MlDsa65,
    MlDsa87,
}

impl AoemMldsaParameterSet {
    pub const fn level(self) -> u32 {
        match self {
            Self::MlDsa44 => 44,
            Self::MlDsa65 => 65,
            Self::MlDsa87 => 87,
        }
    }

    pub const fn public_key_bytes(self) -> usize {
        match self {
            Self::MlDsa44 => 1312,
            Self::MlDsa65 => 1952,
            Self::MlDsa87 => 2592,
        }
    }

    pub const fn signature_bytes(self) -> usize {
        match self {
            Self::MlDsa44 => 2420,
            Self::MlDsa65 => 3309,
            Self::MlDsa87 => 4627,
        }
    }

    /// Expanded secret-key encoding sizes from FIPS 204, Table 2; not seed sizes.
    pub const fn secret_key_bytes(self) -> usize {
        match self {
            Self::MlDsa44 => 2560,
            Self::MlDsa65 => 4032,
            Self::MlDsa87 => 4896,
        }
    }
}

impl TryFrom<u32> for AoemMldsaParameterSet {
    type Error = PqVerificationError;

    fn try_from(level: u32) -> Result<Self, Self::Error> {
        match level {
            44 => Ok(Self::MlDsa44),
            65 => Ok(Self::MlDsa65),
            87 => Ok(Self::MlDsa87),
            _ => Err(PqVerificationError::UnsupportedParameterSet),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PqVerificationError {
    UnsupportedParameterSet,
    InvalidPublicKeyLength,
    InvalidSignatureLength,
    InvalidSecretKeyLength,
    RuntimeUnavailable,
    BatchUnavailable,
    BatchTooLarge,
    SigningUnavailable,
    KeyGenerationUnavailable,
    RuntimeContractMismatch,
    RuntimeFailure,
    SigningFailure,
    KeyGenerationFailure,
    RuntimeIncompatible,
    InvalidContextLength,
    MessageTooLarge,
    CompatibilityVectorInvalid,
    InvalidSignature,
}

impl fmt::Display for PqVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedParameterSet => "unsupported explicit AOEM ML-DSA parameter set",
            Self::InvalidPublicKeyLength => {
                "public key length does not match expected parameter set"
            }
            Self::InvalidSignatureLength => {
                "signature length does not match expected parameter set"
            }
            Self::InvalidSecretKeyLength => {
                "expanded secret key length does not match expected parameter set"
            }
            Self::RuntimeUnavailable => "AOEM ML-DSA verification capability unavailable",
            Self::BatchUnavailable => "AOEM ML-DSA batch verification capability unavailable",
            Self::BatchTooLarge => "ML-DSA verification batch exceeds the bounded facade contract",
            Self::SigningUnavailable => "AOEM ML-DSA signing capability unavailable",
            Self::KeyGenerationUnavailable => "AOEM ML-DSA key generation capability unavailable",
            Self::RuntimeContractMismatch => {
                "AOEM ML-DSA encoding sizes do not match host contract"
            }
            Self::RuntimeFailure => "AOEM ML-DSA verification call failed",
            Self::SigningFailure => "AOEM ML-DSA signing call failed",
            Self::KeyGenerationFailure => "AOEM ML-DSA key generation call failed",
            Self::RuntimeIncompatible => {
                "AOEM runtime failed ML-DSA known-answer compatibility check"
            }
            Self::InvalidContextLength => "ML-DSA context exceeds 255 bytes",
            Self::MessageTooLarge => "ML-DSA framed message allocation exceeds available capacity",
            Self::CompatibilityVectorInvalid => "invalid bundled ML-DSA compatibility vector",
            Self::InvalidSignature => "invalid AOEM ML-DSA signature",
        })
    }
}

impl std::error::Error for PqVerificationError {}

pub struct MldsaVerifier<'runtime> {
    runtime: &'runtime AoemDyn,
    parameters: AoemMldsaParameterSet,
}

/// A bounded work unit, not a chain transaction or consensus capacity limit.
/// Callers with larger streams must retain their original bounded batching.
pub const MLDSA_VERIFY_BATCH_MAX_ITEMS: usize = 1024;
const MLDSA_VERIFY_BATCH_MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Unframed canonical inputs. The facade applies external-pure framing once,
/// just as scalar verification does; neither lengths nor failures select suites.
#[derive(Clone, Copy)]
pub(crate) struct MldsaVerifyInput<'a> {
    pub public_key: &'a [u8],
    pub message: &'a [u8],
    pub context: &'a [u8],
    pub signature: &'a [u8],
}

/// Verification-only qualified view of the existing loaded owner. Unlike the
/// signing runtime this does not require sign/keygen capabilities. Construction
/// qualifies both scalar and batch once; clones neither load nor re-qualify it.
#[derive(Clone)]
pub(crate) struct OwnedMldsaVerifier {
    runtime: Arc<AoemDyn>,
    parameters: AoemMldsaParameterSet,
}

impl OwnedMldsaVerifier {
    pub(crate) fn new(
        runtime: Arc<AoemDyn>,
        parameters: AoemMldsaParameterSet,
    ) -> Result<Self, PqVerificationError> {
        ensure_compatible_backend(runtime.as_ref(), parameters)?;
        ensure_batch_compatible_backend(runtime.as_ref(), parameters)?;
        Ok(Self {
            runtime,
            parameters,
        })
    }

    pub(crate) fn verify(&self, item: MldsaVerifyInput<'_>) -> Result<(), PqVerificationError> {
        check_lengths(self.parameters, item.public_key, item.signature)?;
        let message = frame_pure_message(item.message, item.context)?;
        verify_with_backend(
            self.runtime.as_ref(),
            self.parameters,
            item.public_key,
            &message,
            item.signature,
        )
    }

    pub(crate) fn verify_batch(
        &self,
        items: &[MldsaVerifyInput<'_>],
    ) -> Result<Vec<Result<(), PqVerificationError>>, PqVerificationError> {
        verify_batch_with_backend(self.runtime.as_ref(), self.parameters, items)
    }
}

impl<'runtime> MldsaVerifier<'runtime> {
    pub fn new(
        runtime: &'runtime AoemDyn,
        expected_parameters: AoemMldsaParameterSet,
    ) -> Result<Self, PqVerificationError> {
        ensure_compatible_backend(runtime, expected_parameters)?;
        Ok(Self {
            runtime,
            parameters: expected_parameters,
        })
    }

    pub fn verify(
        &self,
        trusted_public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<(), PqVerificationError> {
        check_lengths(self.parameters, trusted_public_key, signature)?;
        let signing_message = frame_pure_message(message, context)?;
        verify_with_backend(
            self.runtime,
            self.parameters,
            trusted_public_key,
            &signing_message,
            signature,
        )
    }
}

/// Deterministic external-pure ML-DSA signing paired with [`MldsaVerifier`].
///
/// The parameter set is always explicit. Construction requires the verifier's
/// official positive-vector and tampering-negative checks and matching signing
/// capabilities. This wrapper does not select a chain signature policy or imply
/// end-to-end post-quantum security for a transaction or consensus protocol.
pub struct MldsaSigner<'runtime> {
    verifier: MldsaVerifier<'runtime>,
}

impl<'runtime> MldsaSigner<'runtime> {
    pub fn new(
        runtime: &'runtime AoemDyn,
        expected_parameters: AoemMldsaParameterSet,
    ) -> Result<Self, PqVerificationError> {
        let verifier = MldsaVerifier::new(runtime, expected_parameters)?;
        ensure_signing_backend(runtime, expected_parameters)?;
        Ok(Self { verifier })
    }

    /// Signs the unframed message with a borrowed, expanded secret key.
    ///
    /// The caller must establish the public key's trusted identity binding and
    /// supply a valid expanded secret key from trusted key generation/storage,
    /// not a seed. The secret is neither copied nor retained by this wrapper;
    /// ownership, protection and erasure of it remain the caller's responsibility.
    /// Contexts are limited to 255 bytes. The external-pure prefix is added once,
    /// and a signature is returned only after the existing verification path
    /// accepts it for the supplied trusted public key, message and context.
    pub fn sign(
        &self,
        trusted_public_key: &[u8],
        expanded_secret_key: &[u8],
        message: &[u8],
        context: &[u8],
    ) -> Result<Vec<u8>, PqVerificationError> {
        sign_with_backend(
            self.verifier.runtime,
            self.verifier.parameters,
            trusted_public_key,
            expanded_secret_key,
            message,
            context,
        )
    }
}

fn frame_pure_message(message: &[u8], context: &[u8]) -> Result<Vec<u8>, PqVerificationError> {
    let context_len =
        u8::try_from(context.len()).map_err(|_| PqVerificationError::InvalidContextLength)?;
    let size = message
        .len()
        .checked_add(context.len() + 2)
        .ok_or(PqVerificationError::MessageTooLarge)?;
    let mut framed = Vec::new();
    framed
        .try_reserve_exact(size)
        .map_err(|_| PqVerificationError::MessageTooLarge)?;
    framed.extend_from_slice(&[0, context_len]);
    framed.extend_from_slice(context);
    framed.extend_from_slice(message);
    Ok(framed)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatibilityVector {
    level: u32,
    tg_id: u32,
    tc_id: u32,
    public_key: String,
    message: String,
    context: String,
    signature: String,
}

fn vector_bytes(hex: &str) -> Result<Vec<u8>, PqVerificationError> {
    if !hex.len().is_multiple_of(2) {
        return Err(PqVerificationError::CompatibilityVectorInvalid);
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)
                .map_err(|_| PqVerificationError::CompatibilityVectorInvalid)?;
            u8::from_str_radix(text, 16)
                .map_err(|_| PqVerificationError::CompatibilityVectorInvalid)
        })
        .collect()
}

fn compatibility_vector(
    parameters: AoemMldsaParameterSet,
) -> Result<CompatibilityVector, PqVerificationError> {
    let (json, group, case) = match parameters {
        AoemMldsaParameterSet::MlDsa44 => {
            (include_str!("pq_signature_fixtures/mldsa44.json"), 1, 11)
        }
        AoemMldsaParameterSet::MlDsa65 => {
            (include_str!("pq_signature_fixtures/mldsa65.json"), 3, 43)
        }
        AoemMldsaParameterSet::MlDsa87 => {
            (include_str!("pq_signature_fixtures/mldsa87.json"), 5, 70)
        }
    };
    let vector: CompatibilityVector =
        serde_json::from_str(json).map_err(|_| PqVerificationError::CompatibilityVectorInvalid)?;
    if (vector.level, vector.tg_id, vector.tc_id) != (parameters.level(), group, case) {
        return Err(PqVerificationError::CompatibilityVectorInvalid);
    }
    Ok(vector)
}

fn ensure_compatible_backend(
    runtime: &impl MldsaBackend,
    parameters: AoemMldsaParameterSet,
) -> Result<(), PqVerificationError> {
    let vector = compatibility_vector(parameters)?;
    let public_key = vector_bytes(&vector.public_key)?;
    let signature = vector_bytes(&vector.signature)?;
    check_lengths(parameters, &public_key, &signature)?;
    let message = frame_pure_message(
        &vector_bytes(&vector.message)?,
        &vector_bytes(&vector.context)?,
    )?;
    match verify_with_backend(runtime, parameters, &public_key, &message, &signature) {
        Err(PqVerificationError::InvalidSignature) => {
            return Err(PqVerificationError::RuntimeIncompatible)
        }
        result => result?,
    }
    let mut invalid = signature;
    invalid[0] ^= 1;
    match verify_with_backend(runtime, parameters, &public_key, &message, &invalid) {
        Err(PqVerificationError::InvalidSignature) => Ok(()),
        Ok(()) => Err(PqVerificationError::RuntimeIncompatible),
        Err(error) => Err(error),
    }
}

trait MldsaBackend {
    fn available(&self) -> bool;
    fn sizes(&self, level: u32) -> Result<(usize, usize), PqVerificationError>;
    fn verify(
        &self,
        level: u32,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, PqVerificationError>;
}

impl MldsaBackend for AoemDyn {
    fn available(&self) -> bool {
        self.supports_mldsa_size_v1()
            && self.supports_mldsa_verify_v1()
            && self.mldsa_supported_flag() == Some(true)
    }

    fn sizes(&self, level: u32) -> Result<(usize, usize), PqVerificationError> {
        let public_key = self
            .mldsa_pubkey_size_v1(level)
            .map_err(|_| PqVerificationError::RuntimeFailure)?;
        let signature = self
            .mldsa_signature_size_v1(level)
            .map_err(|_| PqVerificationError::RuntimeFailure)?;
        Ok((public_key, signature))
    }

    fn verify(
        &self,
        level: u32,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, PqVerificationError> {
        self.mldsa_verify_v1(level, public_key, message, signature)
            .map_err(|_| PqVerificationError::RuntimeFailure)
    }
}

trait MldsaBatchBackend: MldsaBackend {
    fn batch_available(&self) -> bool;
    fn verify_batch(
        &self,
        items: &[AoemMldsaVerifyItemRef<'_>],
    ) -> Result<Vec<bool>, PqVerificationError>;
}

impl MldsaBatchBackend for AoemDyn {
    fn batch_available(&self) -> bool {
        self.supports_mldsa_verify_batch_v1()
    }

    fn verify_batch(
        &self,
        items: &[AoemMldsaVerifyItemRef<'_>],
    ) -> Result<Vec<bool>, PqVerificationError> {
        self.mldsa_verify_batch_v1(items)
            .map_err(|_| PqVerificationError::RuntimeFailure)
    }
}

fn ensure_batch_compatible_backend(
    runtime: &impl MldsaBatchBackend,
    parameters: AoemMldsaParameterSet,
) -> Result<(), PqVerificationError> {
    if !runtime.available() {
        return Err(PqVerificationError::RuntimeUnavailable);
    }
    if !runtime.batch_available() {
        return Err(PqVerificationError::BatchUnavailable);
    }
    if runtime.sizes(parameters.level())?
        != (parameters.public_key_bytes(), parameters.signature_bytes())
    {
        return Err(PqVerificationError::RuntimeContractMismatch);
    }
    let vector = compatibility_vector(parameters)?;
    let public_key = vector_bytes(&vector.public_key)?;
    let signature = vector_bytes(&vector.signature)?;
    check_lengths(parameters, &public_key, &signature)?;
    let message = vector_bytes(&vector.message)?;
    let context = vector_bytes(&vector.context)?;
    let mut bad_signature = signature.clone();
    bad_signature[0] ^= 1;
    let mut bad_message = message.clone();
    bad_message.push(1);
    let good = MldsaVerifyInput {
        public_key: &public_key,
        message: &message,
        context: &context,
        signature: &signature,
    };
    let results = verify_batch_with_backend(
        runtime,
        parameters,
        &[
            good,
            MldsaVerifyInput {
                signature: &bad_signature,
                ..good
            },
            good,
            MldsaVerifyInput {
                message: &bad_message,
                ..good
            },
        ],
    )?;
    if results
        != [
            Ok(()),
            Err(PqVerificationError::InvalidSignature),
            Ok(()),
            Err(PqVerificationError::InvalidSignature),
        ]
    {
        return Err(PqVerificationError::RuntimeIncompatible);
    }
    Ok(())
}

fn verify_batch_with_backend(
    runtime: &impl MldsaBatchBackend,
    parameters: AoemMldsaParameterSet,
    items: &[MldsaVerifyInput<'_>],
) -> Result<Vec<Result<(), PqVerificationError>>, PqVerificationError> {
    if items.len() > MLDSA_VERIFY_BATCH_MAX_ITEMS {
        return Err(PqVerificationError::BatchTooLarge);
    }
    let mut results = vec![Ok(()); items.len()];
    let mut framed = Vec::with_capacity(items.len());
    let mut bytes = 0usize;
    for (index, item) in items.iter().enumerate() {
        let shape = check_lengths(parameters, item.public_key, item.signature).and({
            if item.context.len() > u8::MAX as usize {
                Err(PqVerificationError::InvalidContextLength)
            } else {
                Ok(())
            }
        });
        if let Err(error) = shape {
            results[index] = Err(error);
            framed.push(None);
            continue;
        }
        bytes = bytes
            .checked_add(item.message.len())
            .and_then(|size| size.checked_add(item.context.len() + 2))
            .filter(|size| *size <= MLDSA_VERIFY_BATCH_MAX_MESSAGE_BYTES)
            .ok_or(PqVerificationError::BatchTooLarge)?;
        framed.push(Some(frame_pure_message(item.message, item.context)?));
    }
    let rows: Vec<_> = items
        .iter()
        .zip(&framed)
        .filter_map(|(item, message)| {
            message.as_ref().map(|message| AoemMldsaVerifyItemRef {
                level: parameters.level(),
                pubkey: item.public_key,
                message,
                signature: item.signature,
            })
        })
        .collect();
    if rows.is_empty() {
        return Ok(results);
    }
    // Capability and encoding widths were qualified once, not re-queried for
    // every row. AOEM owns its existing internal parallel chunk scheduling.
    let verified = runtime.verify_batch(&rows)?;
    if verified.len() != rows.len() {
        return Err(PqVerificationError::RuntimeContractMismatch);
    }
    let mut verified = verified.into_iter();
    for (index, message) in framed.iter().enumerate() {
        if message.is_some() {
            results[index] = if verified.next() == Some(true) {
                Ok(())
            } else {
                Err(PqVerificationError::InvalidSignature)
            };
        }
    }
    Ok(results)
}

trait MldsaSigningBackend: MldsaBackend {
    fn signing_available(&self) -> bool;
    fn secret_key_size(&self, level: u32) -> Result<usize, PqVerificationError>;
    fn sign(
        &self,
        level: u32,
        expanded_secret_key: &[u8],
        signing_message: &[u8],
    ) -> Result<Vec<u8>, PqVerificationError>;
}

impl MldsaSigningBackend for AoemDyn {
    fn signing_available(&self) -> bool {
        self.supports_mldsa_sign_v1()
    }

    fn secret_key_size(&self, level: u32) -> Result<usize, PqVerificationError> {
        self.mldsa_secret_key_size_v1(level)
            .map_err(|_| PqVerificationError::SigningFailure)
    }

    fn sign(
        &self,
        level: u32,
        expanded_secret_key: &[u8],
        signing_message: &[u8],
    ) -> Result<Vec<u8>, PqVerificationError> {
        self.mldsa_sign_v1(level, expanded_secret_key, signing_message)
            .map_err(|_| PqVerificationError::SigningFailure)
    }
}

fn ensure_signing_backend(
    runtime: &impl MldsaSigningBackend,
    parameters: AoemMldsaParameterSet,
) -> Result<(), PqVerificationError> {
    if !runtime.available() {
        return Err(PqVerificationError::RuntimeUnavailable);
    }
    if !runtime.signing_available() {
        return Err(PqVerificationError::SigningUnavailable);
    }
    let level = parameters.level();
    if runtime.sizes(level)? != (parameters.public_key_bytes(), parameters.signature_bytes())
        || runtime.secret_key_size(level)? != parameters.secret_key_bytes()
    {
        return Err(PqVerificationError::RuntimeContractMismatch);
    }
    Ok(())
}

fn sign_with_backend(
    runtime: &impl MldsaSigningBackend,
    parameters: AoemMldsaParameterSet,
    trusted_public_key: &[u8],
    expanded_secret_key: &[u8],
    message: &[u8],
    context: &[u8],
) -> Result<Vec<u8>, PqVerificationError> {
    if trusted_public_key.len() != parameters.public_key_bytes() {
        return Err(PqVerificationError::InvalidPublicKeyLength);
    }
    if expanded_secret_key.len() != parameters.secret_key_bytes() {
        return Err(PqVerificationError::InvalidSecretKeyLength);
    }
    let signing_message = frame_pure_message(message, context)?;
    ensure_signing_backend(runtime, parameters)?;
    let signature = runtime.sign(parameters.level(), expanded_secret_key, &signing_message)?;
    verify_with_backend(
        runtime,
        parameters,
        trusted_public_key,
        &signing_message,
        &signature,
    )?;
    Ok(signature)
}

fn verify_with_backend(
    runtime: &impl MldsaBackend,
    expected_parameters: AoemMldsaParameterSet,
    trusted_public_key: &[u8],
    signing_message: &[u8],
    signature: &[u8],
) -> Result<(), PqVerificationError> {
    check_lengths(expected_parameters, trusted_public_key, signature)?;
    let expected_sizes = (
        expected_parameters.public_key_bytes(),
        expected_parameters.signature_bytes(),
    );
    if !runtime.available() {
        return Err(PqVerificationError::RuntimeUnavailable);
    }
    let level = expected_parameters.level();
    if runtime.sizes(level)? != expected_sizes {
        return Err(PqVerificationError::RuntimeContractMismatch);
    }
    match runtime.verify(level, trusted_public_key, signing_message, signature)? {
        true => Ok(()),
        false => Err(PqVerificationError::InvalidSignature),
    }
}

fn check_lengths(
    parameters: AoemMldsaParameterSet,
    public_key: &[u8],
    signature: &[u8],
) -> Result<(), PqVerificationError> {
    if public_key.len() != parameters.public_key_bytes() {
        return Err(PqVerificationError::InvalidPublicKeyLength);
    }
    if signature.len() != parameters.signature_bytes() {
        return Err(PqVerificationError::InvalidSignatureLength);
    }
    Ok(())
}

#[cfg(test)]
#[path = "pq_signature_tests.rs"]
mod tests;

#[cfg(test)]
mod owned_runtime_tests {
    use super::*;

    #[test]
    #[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; actual owned AOEM ML-DSA"]
    fn actual_owned_mldsa_runtime_keygen_pair_and_lifetime() -> anyhow::Result<()> {
        let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .ok_or_else(|| anyhow::anyhow!("explicit NOVOVM_AOEM_TEST_LIBRARY required"))?;
        let facade = crate::AoemExecFacade::open(library, crate::AoemExecOpenOptions::default())?;
        let runtime = facade.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
        let shared = runtime.clone();
        let key = runtime.generate_keypair()?;
        assert_eq!(key.public_key.len(), 1952);
        assert_eq!(key.expanded_secret_key.len(), 4032);
        drop(facade);
        drop(runtime);
        let context = b"test/owned-runtime-keypair";
        let message = b"an isolated fixture, not a vote";
        let mut signature =
            shared.sign(&key.public_key, &key.expanded_secret_key, message, context)?;
        assert_eq!(signature.len(), 3309);
        shared.verify(&key.public_key, message, context, &signature)?;
        assert!(shared
            .verify(&key.public_key, b"changed", context, &signature)
            .is_err());
        assert!(shared
            .verify(&key.public_key, message, b"other context", &signature)
            .is_err());
        signature[0] ^= 1;
        assert!(shared
            .verify(&key.public_key, message, context, &signature)
            .is_err());
        let other = shared.generate_keypair()?;
        assert!(shared
            .sign(
                &key.public_key,
                &other.expanded_secret_key,
                message,
                context
            )
            .is_err());
        assert_eq!(
            shared.sign(&[0; 32], &key.expanded_secret_key, message, context),
            Err(PqVerificationError::InvalidPublicKeyLength)
        );
        assert_eq!(
            shared.sign(&key.public_key, &[0; 32], message, context),
            Err(PqVerificationError::InvalidSecretKeyLength)
        );
        Ok(())
    }
}
