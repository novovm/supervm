//! Diagnostic receipt adapter for the SAME NOV relation and candidate outputs.
//! Reuses the existing complete guest input and AOEM AORCP002 adapter; it does
//! not build another prover, infer an image from a receipt, or alter consensus.
//! No product RPC or node startup route activates this direct-backend adapter.

use super::{wire, ExecutionJournalV1, JOURNAL_BYTES, MAX_INPUT_BYTES};
use crate::native_pipeline::business::nov_transfer_batch::ExecutedNovBatch;
use crate::native_pipeline::pipeline::DurableCandidate;
use anyhow::{ensure, Result};
use novovm_exec::resident::ReceiptSession;

/// An independently configured, trusted zkVM image ID (eight native u32 words,
/// NOT an ELF SHA256). This pin must come from the reviewed guest build, never
/// a remote receipt or peer's suggestion. No default or image discovery exists.
#[derive(Clone, Copy, Debug)]
pub struct ExecutionProofPins {
    image: [u32; 8],
}

/// Successful backend verification of only the pinned NOV relation. Contains no
/// signing/publishing capability. The physical candidate document, chain parent
/// authority, consensus finality and GPU backend are NOT certified by this type.
pub struct VerifiedExecutionReceipt {
    image: [u32; 8],
    journal: [u8; JOURNAL_BYTES],
    receipt: Vec<u8>,
}

impl VerifiedExecutionReceipt {
    pub fn image_id(&self) -> &[u32; 8] {
        &self.image
    }
    pub fn journal(&self) -> &[u8; JOURNAL_BYTES] {
        &self.journal
    }
    pub fn receipt(&self) -> &[u8] {
        &self.receipt
    }
}

impl ExecutionProofPins {
    pub fn new(image: [u32; 8]) -> Result<Self> {
        ensure!(image != [0; 8], "execution proof image is not configured");
        Ok(Self { image })
    }

    /// Blocking, non-cancellable native calls: run on a designated proof owner,
    /// not in the control/consensus poll or CPU execution owner. The input comes
    /// from the existing `execution_proof_input()` capture export. No witness is
    /// generated or retained on the default fast path by this optional API.
    pub fn prove_executed(
        &self,
        session: &mut ReceiptSession,
        elf: &[u8],
        input: &[u8],
        batch: &ExecutedNovBatch,
    ) -> Result<VerifiedExecutionReceipt> {
        self.prove(
            session,
            elf,
            input,
            &ExecutionJournalV1::from_executed(batch)?,
        )
    }

    pub fn verify_executed(
        &self,
        session: &mut ReceiptSession,
        receipt: &[u8],
        batch: &ExecutedNovBatch,
    ) -> Result<VerifiedExecutionReceipt> {
        self.verify(session, receipt, &ExecutionJournalV1::from_executed(batch)?)
    }

    pub fn prove_candidate(
        &self,
        session: &mut ReceiptSession,
        elf: &[u8],
        input: &[u8],
        candidate: &DurableCandidate,
    ) -> Result<VerifiedExecutionReceipt> {
        self.prove(
            session,
            elf,
            input,
            &ExecutionJournalV1::from_candidate(candidate)?,
        )
    }

    pub fn verify_candidate(
        &self,
        session: &mut ReceiptSession,
        receipt: &[u8],
        candidate: &DurableCandidate,
    ) -> Result<VerifiedExecutionReceipt> {
        self.verify(
            session,
            receipt,
            &ExecutionJournalV1::from_candidate(candidate)?,
        )
    }

    fn prove(
        &self,
        session: &mut impl Backend,
        elf: &[u8],
        input: &[u8],
        expected: &ExecutionJournalV1,
    ) -> Result<VerifiedExecutionReceipt> {
        ensure!(!elf.is_empty(), "execution proof guest is not configured");
        let framed = frame_input(input)?;
        let receipt = session.prove(elf, &framed, &self.image)?;
        // Prove's output is untrusted until independently pinned verification.
        // Expected bytes are never taken from input, prover output or receipt.
        self.verify(session, &receipt, expected)
    }

    fn verify(
        &self,
        session: &mut impl Backend,
        receipt: &[u8],
        expected: &ExecutionJournalV1,
    ) -> Result<VerifiedExecutionReceipt> {
        let journal = expected.encode();
        session.verify(receipt, &self.image, &journal)?;
        Ok(VerifiedExecutionReceipt {
            image: self.image,
            journal,
            receipt: receipt.to_vec(),
        })
    }
}

fn frame_input(input: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        input.len() <= MAX_INPUT_BYTES,
        "execution proof input exceeds bound"
    );
    // Resource/schema checks only; the guest authenticates the actual relation.
    let _ = wire::decode(input)?;
    let mut framed = Vec::with_capacity(4 + input.len());
    framed.extend_from_slice(&u32::try_from(input.len())?.to_le_bytes());
    framed.extend_from_slice(input);
    Ok(framed)
}

// Private test seam; product callers can only use the actual AOEM session.
trait Backend {
    fn prove(&mut self, elf: &[u8], input: &[u8], image: &[u32; 8]) -> Result<Vec<u8>>;
    fn verify(&mut self, receipt: &[u8], image: &[u32; 8], expected: &[u8]) -> Result<()>;
}
impl Backend for ReceiptSession {
    fn prove(&mut self, elf: &[u8], input: &[u8], image: &[u32; 8]) -> Result<Vec<u8>> {
        ReceiptSession::prove(self, elf, input, image)
    }
    fn verify(&mut self, receipt: &[u8], image: &[u32; 8], expected: &[u8]) -> Result<()> {
        ReceiptSession::verify(self, receipt, image, expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::persistence::{PacketBudget, PreparedCandidate};

    // Wiring double, NOT a zkVM proof or cryptographic validation.
    struct RecordingBackend {
        journal: Vec<u8>,
        input: Vec<u8>,
        reject: bool,
        verified: usize,
    }
    impl Backend for RecordingBackend {
        fn prove(&mut self, _: &[u8], input: &[u8], _: &[u32; 8]) -> Result<Vec<u8>> {
            self.input = input.to_vec();
            Ok(b"AORCP002-wiring-only".to_vec())
        }
        fn verify(&mut self, _: &[u8], image: &[u32; 8], expected: &[u8]) -> Result<()> {
            self.verified += 1;
            ensure!(
                !self.reject && image == &[7; 8] && expected == self.journal,
                "backend rejected pinned statement"
            );
            Ok(())
        }
    }

    #[test]
    fn prepared_candidate_and_executed_have_identical_independent_journal() {
        let fixture = super::super::tests::fixture();
        let expected = ExecutionJournalV1::from_executed(&fixture.executed).unwrap();
        let packet =
            PreparedCandidate::from_executed(fixture.executed, PacketBudget::default()).unwrap();
        assert_eq!(ExecutionJournalV1::from_packet(&packet).unwrap(), expected);
    }

    #[test]
    fn proof_output_is_verified_with_independent_image_and_all_journal_bytes() {
        let fixture = super::super::tests::fixture();
        let expected = ExecutionJournalV1::from_executed(&fixture.executed).unwrap();
        let mut backend = RecordingBackend {
            journal: expected.encode().to_vec(),
            input: Vec::new(),
            reject: false,
            verified: 0,
        };
        let pins = ExecutionProofPins::new([7; 8]).unwrap();
        let result = pins
            .prove(&mut backend, b"trusted-elf", &fixture.input, &expected)
            .unwrap();
        assert_eq!(result.journal(), &expected.encode());
        assert_eq!(backend.verified, 1);
        assert_eq!(
            &backend.input[..4],
            &u32::try_from(fixture.input.len()).unwrap().to_le_bytes()
        );
        assert_eq!(&backend.input[4..], fixture.input);
        backend.reject = true;
        assert!(pins
            .prove(&mut backend, b"trusted-elf", &fixture.input, &expected)
            .is_err());
        backend.reject = false;
        assert!(ExecutionProofPins::new([9; 8])
            .unwrap()
            .verify(&mut backend, result.receipt(), &expected)
            .is_err());
        for index in [0, 8, 40, 72, 104, 136, 144] {
            backend.journal[index] ^= 1;
            assert!(pins
                .verify(&mut backend, result.receipt(), &expected)
                .is_err());
            backend.journal[index] ^= 1;
        }
    }

    #[test]
    fn missing_image_guest_and_malformed_inputs_fail_closed() {
        assert!(ExecutionProofPins::new([0; 8]).is_err());
        assert!(frame_input(b"bad").is_err());
        assert!(frame_input(&vec![0; MAX_INPUT_BYTES + 1]).is_err());
        let fixture = super::super::tests::fixture();
        let expected = ExecutionJournalV1::from_executed(&fixture.executed).unwrap();
        let mut backend = RecordingBackend {
            journal: expected.encode().to_vec(),
            input: Vec::new(),
            reject: false,
            verified: 0,
        };
        assert!(ExecutionProofPins::new([7; 8])
            .unwrap()
            .prove(&mut backend, &[], &fixture.input, &expected)
            .is_err());
        assert_eq!(backend.verified, 0);
    }
}
