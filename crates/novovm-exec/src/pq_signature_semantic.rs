//! Thin consumer of compute.crypto.mldsa_verify_v1. No cryptographic fallback.
use super::{MldsaBackend, MldsaBatchBackend, PqVerificationError};
use crate::{OpsWireOp, OpsWireV1Builder};
use aoem_bindings::{AoemDyn, AoemMldsaVerifyItemRef, AoemSharedHandle};
use std::sync::{Arc, Mutex};

const REQUEST_ID: &str = "novovm-mldsa-verify";
const SCHEMA: &[u8] = b"compute.crypto.mldsa_verify_v1";
const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;

/// Explicit execution policy, not an algorithm or authority negotiation.
/// Auto retains AOEM's current CPU policy. GpuRequired must never fall back.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MldsaVerificationBackendV1 {
    #[default]
    Auto,
    Cpu,
    GpuRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MldsaVerificationExecutionV1 {
    pub sequence: u64,
    pub backend_kind: u32,
    pub rows: u32,
    pub valid_count: u32,
}

struct Session {
    handle: AoemSharedHandle,
    sequence: u64,
    failed: bool,
    last: Option<MldsaVerificationExecutionV1>,
}

// The opaque AOEM context is an owned, thread-independent C handle. Only this
// mutex-owned session moves across threads; no raw pointer or borrowed result
// escapes, and submission/take/destruction cannot race with another caller.
unsafe impl Send for Session {}

pub(super) struct SemanticMldsaBackend {
    runtime: Arc<AoemDyn>,
    policy: MldsaVerificationBackendV1,
    session: Mutex<Session>,
}

impl SemanticMldsaBackend {
    pub(super) fn new(
        runtime: Arc<AoemDyn>,
        policy: MldsaVerificationBackendV1,
    ) -> Result<Self, PqVerificationError> {
        if !runtime.supports_execute_ops_wire_v1() || !runtime.supports_compute_result_take_v1() {
            return Err(PqVerificationError::BatchUnavailable);
        }
        let handle =
            AoemDyn::create_shared_handle_with_ingress_workers(Arc::clone(&runtime), Some(1))
                .map_err(|_| PqVerificationError::RuntimeFailure)?;
        Ok(Self {
            runtime,
            policy,
            session: Mutex::new(Session {
                handle,
                sequence: 0,
                failed: false,
                last: None,
            }),
        })
    }

    pub(super) fn last_execution_v1(&self) -> Option<MldsaVerificationExecutionV1> {
        self.session.lock().ok().and_then(|session| session.last)
    }
}

impl MldsaBackend for SemanticMldsaBackend {
    fn available(&self) -> bool {
        self.runtime.supports_mldsa_size_v1()
            && self.runtime.mldsa_supported_flag() == Some(true)
            && self.runtime.supports_execute_ops_wire_v1()
            && self.runtime.supports_compute_result_take_v1()
    }

    fn sizes(&self, level: u32) -> Result<(usize, usize), PqVerificationError> {
        Ok((
            self.runtime
                .mldsa_pubkey_size_v1(level)
                .map_err(|_| PqVerificationError::RuntimeFailure)?,
            self.runtime
                .mldsa_signature_size_v1(level)
                .map_err(|_| PqVerificationError::RuntimeFailure)?,
        ))
    }

    fn verify(
        &self,
        level: u32,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, PqVerificationError> {
        let values = self.verify_batch(&[AoemMldsaVerifyItemRef {
            level,
            pubkey: public_key,
            message,
            signature,
        }])?;
        values
            .first()
            .copied()
            .ok_or(PqVerificationError::RuntimeContractMismatch)
    }
}

impl MldsaBatchBackend for SemanticMldsaBackend {
    fn batch_available(&self) -> bool {
        self.available()
    }

    fn verify_batch(
        &self,
        items: &[AoemMldsaVerifyItemRef<'_>],
    ) -> Result<Vec<bool>, PqVerificationError> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| PqVerificationError::RuntimeFailure)?;
        session.last = None;
        if session.failed {
            return Err(PqVerificationError::RuntimeFailure);
        }
        let sequence = session
            .sequence
            .checked_add(1)
            .ok_or(PqVerificationError::RuntimeFailure)?;
        let wire = encode_request(items, sequence, self.policy)?;
        session.sequence = sequence;
        // A failed completion/take/contract permanently faults this context.
        // Pending results are context-owned and released with the context; they
        // are never reused or retried under another sequence/backend.
        session.failed = true;
        let completed = session
            .handle
            .execute_ops_wire_v1(&wire)
            .map_err(|_| PqVerificationError::RuntimeFailure)?;
        if completed.processed != 1
            || completed.success != 1
            || completed.failed_index != u32::MAX
            || completed.total_writes != 1
        {
            return Err(PqVerificationError::RuntimeContractMismatch);
        }
        let bytes = session
            .handle
            .compute_result_take_v1(REQUEST_ID, sequence, 32 + items.len())
            .map_err(|_| PqVerificationError::RuntimeFailure)?;
        let (metadata, results) = decode_result(&bytes, sequence, items.len(), self.policy)?;
        session.last = Some(metadata);
        session.failed = false;
        Ok(results)
    }
}

fn encode_request(
    items: &[AoemMldsaVerifyItemRef<'_>],
    sequence: u64,
    policy: MldsaVerificationBackendV1,
) -> Result<Vec<u8>, PqVerificationError> {
    if items.is_empty() || items.len() > super::MLDSA_VERIFY_BATCH_MAX_ITEMS || sequence == 0 {
        return Err(PqVerificationError::BatchTooLarge);
    }
    let mut length = 32usize + REQUEST_ID.len();
    for item in items {
        length = length
            .checked_add(16)
            .and_then(|n| n.checked_add(item.pubkey.len()))
            .and_then(|n| n.checked_add(item.message.len()))
            .and_then(|n| n.checked_add(item.signature.len()))
            .filter(|n| *n <= MAX_REQUEST_BYTES)
            .ok_or(PqVerificationError::BatchTooLarge)?;
    }
    let mut payload = Vec::with_capacity(length);
    payload.extend_from_slice(b"AOMV0001");
    payload.extend_from_slice(&sequence.to_le_bytes());
    payload.extend_from_slice(&(REQUEST_ID.len() as u32).to_le_bytes());
    payload.extend_from_slice(&(items.len() as u32).to_le_bytes());
    let flags: u32 = match policy {
        MldsaVerificationBackendV1::Auto => 0,
        MldsaVerificationBackendV1::Cpu => 1,
        MldsaVerificationBackendV1::GpuRequired => 2,
    };
    payload.extend_from_slice(&flags.to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(REQUEST_ID.as_bytes());
    for item in items {
        for field in [
            item.level,
            item.pubkey.len() as u32,
            item.message.len() as u32,
            item.signature.len() as u32,
        ] {
            payload.extend_from_slice(&field.to_le_bytes());
        }
        payload.extend_from_slice(item.pubkey);
        // Already externally pure-framed exactly once by the original facade.
        payload.extend_from_slice(item.message);
        payload.extend_from_slice(item.signature);
    }
    let mut wire = OpsWireV1Builder::new();
    wire.push(OpsWireOp {
        opcode: 115,
        flags: 0,
        reserved: 0,
        key: SCHEMA,
        value: &payload,
        delta: 0,
        expect_version: None,
        plan_id: 0,
    })
    .map_err(|_| PqVerificationError::RuntimeContractMismatch)?;
    Ok(wire.finish().bytes)
}

fn decode_result(
    bytes: &[u8],
    sequence: u64,
    rows: usize,
    policy: MldsaVerificationBackendV1,
) -> Result<(MldsaVerificationExecutionV1, Vec<bool>), PqVerificationError> {
    let mismatch = PqVerificationError::RuntimeContractMismatch;
    if bytes.len() != 32 + rows || bytes.get(..8) != Some(b"AOMR0001") {
        return Err(mismatch);
    }
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded header"));
    let actual_sequence = u64::from_le_bytes(bytes[8..16].try_into().expect("bounded header"));
    let metadata = MldsaVerificationExecutionV1 {
        sequence: actual_sequence,
        rows: u32_at(16),
        valid_count: u32_at(20),
        backend_kind: u32_at(24),
    };
    let expected_backend = match policy {
        MldsaVerificationBackendV1::Auto | MldsaVerificationBackendV1::Cpu => 1,
        MldsaVerificationBackendV1::GpuRequired => 4,
    };
    if actual_sequence != sequence
        || metadata.rows as usize != rows
        || metadata.backend_kind != expected_backend
        || u32_at(28) != 1
        || bytes[32..].iter().any(|value| *value > 1)
        || bytes[32..].iter().filter(|value| **value == 1).count() != metadata.valid_count as usize
    {
        return Err(mismatch);
    }
    Ok((
        metadata,
        bytes[32..].iter().map(|value| *value == 1).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actual_semantic_backend(policy: MldsaVerificationBackendV1) -> anyhow::Result<()> {
        use super::super::{
            compatibility_vector, vector_bytes, AoemMldsaParameterSet, MldsaVerifyInput,
            OwnedMldsaVerifier,
        };
        let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .ok_or_else(|| anyhow::anyhow!("explicit NOVOVM_AOEM_TEST_LIBRARY required"))?;
        let facade = crate::AoemExecFacade::open(library, crate::AoemExecOpenOptions::default())?;
        for parameters in [
            AoemMldsaParameterSet::MlDsa44,
            AoemMldsaParameterSet::MlDsa65,
            AoemMldsaParameterSet::MlDsa87,
        ] {
            let verifier = OwnedMldsaVerifier::new_with_backend(
                Arc::clone(&facade.dynlib),
                parameters,
                policy,
            )?;
            let vector = compatibility_vector(parameters)?;
            let pk = vector_bytes(&vector.public_key)?;
            let sig = vector_bytes(&vector.signature)?;
            let message = vector_bytes(&vector.message)?;
            let context = vector_bytes(&vector.context)?;
            let good = MldsaVerifyInput {
                public_key: &pk,
                signature: &sig,
                message: &message,
                context: &context,
            };
            let mut bad_signature = sig.clone();
            bad_signature[0] ^= 1;
            let mut other_context = context.clone();
            other_context.push(1);
            let mut other_message = message.clone();
            other_message.push(1);
            let mut other_pk = pk.clone();
            other_pk[0] ^= 1;
            let rows = [
                good,
                MldsaVerifyInput {
                    signature: &bad_signature,
                    ..good
                },
                MldsaVerifyInput {
                    public_key: &pk[..32],
                    ..good
                },
                MldsaVerifyInput {
                    context: &other_context,
                    ..good
                },
                good,
                MldsaVerifyInput {
                    message: &other_message,
                    ..good
                },
                MldsaVerifyInput {
                    public_key: &other_pk,
                    ..good
                },
            ];
            let checked = verifier.verify_batch(&rows)?;
            assert_eq!(
                checked,
                [
                    Ok(()),
                    Err(PqVerificationError::InvalidSignature),
                    Err(PqVerificationError::InvalidPublicKeyLength),
                    Err(PqVerificationError::InvalidSignature),
                    Ok(()),
                    Err(PqVerificationError::InvalidSignature),
                    Err(PqVerificationError::InvalidSignature)
                ]
            );
            let metadata = verifier.last_execution_v1().unwrap();
            assert_eq!(metadata.rows, 6); // malformed row never enters semantic payload
            assert_eq!(metadata.valid_count, 2);
            assert_eq!(
                metadata.backend_kind,
                if policy == MldsaVerificationBackendV1::GpuRequired {
                    4
                } else {
                    1
                }
            );
            verifier.verify(good)?;
            assert!(verifier.last_execution_v1().unwrap().sequence > metadata.sequence);
            std::thread::scope(|scope| {
                let first = verifier.clone();
                let second = verifier.clone();
                let a = scope.spawn(move || first.verify_batch(&[good, good]));
                let b = scope.spawn(move || second.verify_batch(&[good]));
                assert_eq!(a.join().unwrap().unwrap(), [Ok(()), Ok(())]);
                assert_eq!(b.join().unwrap().unwrap(), [Ok(())]);
            });
            let oversize = vec![0; super::super::MLDSA_VERIFY_BATCH_MAX_MESSAGE_BYTES];
            assert_eq!(
                verifier.verify_batch(&[MldsaVerifyInput {
                    message: &oversize,
                    ..good
                }]),
                Err(PqVerificationError::BatchTooLarge)
            );
            verifier.verify(good)?;

            // Result ownership is context + request identity + sequence, consumed once.
            let first = AoemDyn::create_shared_handle_with_ingress_workers(
                Arc::clone(&facade.dynlib),
                Some(1),
            )?;
            let other = AoemDyn::create_shared_handle_with_ingress_workers(
                Arc::clone(&facade.dynlib),
                Some(1),
            )?;
            let framed = super::super::frame_pure_message(&message, &context)?;
            let item = AoemMldsaVerifyItemRef {
                level: parameters.level(),
                pubkey: &pk,
                message: &framed,
                signature: &sig,
            };
            first.execute_ops_wire_v1(&encode_request(&[item], 1, policy)?)?;
            assert!(other.compute_result_take_v1(REQUEST_ID, 1, 33).is_err());
            assert!(first.compute_result_take_v1("wrong-owner", 1, 33).is_err());
            assert!(first.compute_result_take_v1(REQUEST_ID, 2, 33).is_err());
            let bytes = first.compute_result_take_v1(REQUEST_ID, 1, 33)?;
            assert_eq!(decode_result(&bytes, 1, 1, policy)?.1, [true]);
            assert!(first.compute_result_take_v1(REQUEST_ID, 1, 33).is_err());
        }
        Ok(())
    }

    #[test]
    #[ignore = "requires pinned NOVOVM_AOEM_TEST_LIBRARY with unified ML-DSA semantics"]
    fn actual_semantic_mldsa_cpu_contract() -> anyhow::Result<()> {
        actual_semantic_backend(MldsaVerificationBackendV1::Cpu)
    }

    #[test]
    #[ignore = "requires pinned NOVOVM_AOEM_TEST_LIBRARY and qualified explicit GPU"]
    fn actual_semantic_mldsa_gpu_contract() -> anyhow::Result<()> {
        actual_semantic_backend(MldsaVerificationBackendV1::GpuRequired)
    }

    fn result() -> Vec<u8> {
        let mut bytes = b"AOMR0001".to_vec();
        bytes.extend_from_slice(&7u64.to_le_bytes());
        for field in [3u32, 2, 4, 1] {
            bytes.extend_from_slice(&field.to_le_bytes());
        }
        bytes.extend_from_slice(&[1, 0, 1]);
        bytes
    }

    #[test]
    fn result_rejects_wrong_identity_shape_backend_flags_and_stale_bytes() {
        let original = result();
        let (_, rows) =
            decode_result(&original, 7, 3, MldsaVerificationBackendV1::GpuRequired).unwrap();
        assert_eq!(rows, [true, false, true]);
        for at in [0, 8, 16, 20, 24, 28, 32] {
            let mut bad = original.clone();
            bad[at] ^= 2;
            assert!(
                decode_result(&bad, 7, 3, MldsaVerificationBackendV1::GpuRequired).is_err(),
                "offset {at}"
            );
        }
        assert!(decode_result(&original, 8, 3, MldsaVerificationBackendV1::GpuRequired).is_err());
        assert!(decode_result(&original, 7, 3, MldsaVerificationBackendV1::Cpu).is_err());
        assert!(decode_result(
            &original[..34],
            7,
            3,
            MldsaVerificationBackendV1::GpuRequired
        )
        .is_err());
        let mut extra = original;
        extra.push(0);
        assert!(decode_result(&extra, 7, 3, MldsaVerificationBackendV1::GpuRequired).is_err());
    }

    #[test]
    fn semantic_wire_preserves_framed_message_exactly_once() {
        let framed = super::super::frame_pure_message(b"message", b"purpose").unwrap();
        let row = AoemMldsaVerifyItemRef {
            level: 65,
            pubkey: &[11; 1952],
            message: &framed,
            signature: &[22; 3309],
        };
        let wire = encode_request(&[row], 1, MldsaVerificationBackendV1::Cpu).unwrap();
        // AOV2 header 13, op header 36, schema, AOMV header 32, id, item header 16, pk.
        let offset = 13 + 36 + SCHEMA.len() + 32 + REQUEST_ID.len() + 16 + 1952;
        assert_eq!(&wire[offset..offset + framed.len()], &framed);
        assert_eq!(wire[13], 115);
        assert!(encode_request(&[], 1, MldsaVerificationBackendV1::Auto).is_err());
        assert!(encode_request(&[row], 0, MldsaVerificationBackendV1::Auto).is_err());
    }
}
