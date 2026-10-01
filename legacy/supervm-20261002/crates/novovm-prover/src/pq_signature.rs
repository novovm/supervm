use aoem_bindings::AoemDyn;
use serde::Deserialize;
use std::fmt;

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
    SigningUnavailable,
    RuntimeContractMismatch,
    RuntimeFailure,
    SigningFailure,
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
            Self::SigningUnavailable => "AOEM ML-DSA signing capability unavailable",
            Self::RuntimeContractMismatch => {
                "AOEM ML-DSA encoding sizes do not match host contract"
            }
            Self::RuntimeFailure => "AOEM ML-DSA verification call failed",
            Self::SigningFailure => "AOEM ML-DSA signing call failed",
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
