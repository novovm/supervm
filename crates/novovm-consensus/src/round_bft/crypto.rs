//! Domain-neutral adapter boundary for the original AOEM ML-DSA-65 owner.
//!
//! Consensus supplies the complete versioned transcript, not a prehash labelled
//! as pure ML-DSA. Product adapters must use the one approved pure-context
//! framing below. There is no algorithm negotiation or fallback here.
use anyhow::Result;
use std::sync::Arc;

pub const MLDSA65_PUBLIC_KEY_BYTES: usize = 1952;
pub const MLDSA65_SIGNATURE_BYTES: usize = 3309;
pub const MLDSA65_SUITE: u16 = 65;
pub const MLDSA65_CONTEXT: &[u8] = b"NOVOVM/consensus/ML-DSA-65/v2";

pub trait ConsensusVerifier: Send + Sync {
    fn verify_mldsa65(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()>;
    /// Explicit component-fixture branding; product startup must reject it.
    fn is_test_only(&self) -> bool {
        false
    }
}

pub trait ConsensusSigner: Send + Sync {
    fn public_key(&self) -> &[u8];
    fn sign_mldsa65(&self, message: &[u8]) -> Result<Vec<u8>>;
    fn is_test_only(&self) -> bool {
        false
    }
}

impl<T: ConsensusSigner + ?Sized> ConsensusSigner for Arc<T> {
    fn public_key(&self) -> &[u8] {
        (**self).public_key()
    }
    fn sign_mldsa65(&self, message: &[u8]) -> Result<Vec<u8>> {
        (**self).sign_mldsa65(message)
    }
    fn is_test_only(&self) -> bool {
        (**self).is_test_only()
    }
}
