//! Compatibility import for existing prover diagnostic and App consumers.
//!
//! The only implementation lives in `novovm_exec::pq_signature`, on the shared
//! exec -> aoem-bindings -> AOEM path. The prover runtime integration test keeps
//! exercising this import while those consumers migrate. Remove this adapter
//! after the external App and diagnostic callers use the exec facade directly.

pub use novovm_exec::pq_signature::{
    AoemMldsaParameterSet, MldsaSigner, MldsaVerifier, PqVerificationError,
};
