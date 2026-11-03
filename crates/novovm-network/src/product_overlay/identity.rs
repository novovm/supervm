//! ML-DSA-65 identity operations supplied by the existing qualified crypto owner.
//! The network neither opens AOEM libraries nor owns another signing algorithm.

use anyhow::{ensure, Result};
use std::sync::Arc;

pub const MLDSA65_PUBLIC_KEY_BYTES: usize = 1952;
pub const MLDSA65_SIGNATURE_BYTES: usize = 3309;
pub const MLDSA65_SUITE: u16 = 65;

/// Read-only signature verification does not require a local private key.
pub trait TransportVerifier: Send + Sync {
    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()>;
}

/// A long-lived signer/verifier over one independently held purpose key.
/// Implementations must use the qualified ML-DSA-65 runtime, not negotiate an
/// algorithm from packet sizes or retry a different suite after failure.
pub trait TransportSignatureProvider: Send + Sync {
    fn public_key(&self) -> &[u8];
    fn sign(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>>;
    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()>;
}

impl<T: TransportSignatureProvider + ?Sized> TransportVerifier for T {
    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        TransportSignatureProvider::verify(self, public_key, message, context, signature)
    }
}

impl TransportVerifier for Arc<dyn TransportVerifier> {
    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        self.as_ref()
            .verify(public_key, message, context, signature)
    }
}

/// Clones share the original runtime/key owner; no secret bytes are copied by
/// reconnect or by a new handshake. The public identity is fixed at creation.
#[derive(Clone)]
pub struct TransportIdentity {
    public_key: Arc<[u8]>,
    provider: Arc<dyn TransportSignatureProvider>,
}

impl TransportIdentity {
    pub fn new(provider: Arc<dyn TransportSignatureProvider>) -> Result<Self> {
        ensure!(
            provider.public_key().len() == MLDSA65_PUBLIC_KEY_BYTES,
            "transport identity requires an ML-DSA-65 public key"
        );
        Ok(Self {
            public_key: Arc::from(provider.public_key()),
            provider,
        })
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    pub fn peer_id(&self) -> String {
        super::peer_id_from_mldsa65_public_key_v1(self.public_key())
    }

    pub fn sign(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>> {
        let signature = self.provider.sign(message, context)?;
        ensure!(
            signature.len() == MLDSA65_SIGNATURE_BYTES,
            "transport signer returned an invalid ML-DSA-65 signature size"
        );
        Ok(signature)
    }

    pub fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        ensure!(
            public_key.len() == MLDSA65_PUBLIC_KEY_BYTES
                && signature.len() == MLDSA65_SIGNATURE_BYTES,
            "transport ML-DSA-65 key/signature size mismatch"
        );
        TransportSignatureProvider::verify(
            self.provider.as_ref(),
            public_key,
            message,
            context,
            signature,
        )
    }
}

impl std::fmt::Debug for TransportIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportIdentity")
            .field("peer_id", &self.peer_id())
            .finish_non_exhaustive()
    }
}

impl TransportVerifier for TransportIdentity {
    fn verify(
        &self,
        public_key: &[u8],
        message: &[u8],
        context: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        TransportIdentity::verify(self, public_key, message, context, signature)
    }
}
