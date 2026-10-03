//! Binding to AOEM's real portable receipt ABI. Never falls back to a trace probe.
use super::AoemDyn;
use anyhow::{anyhow, bail, Result};
use std::ptr;

pub(super) type ProveFn = unsafe extern "C" fn(
    *const u8,
    usize,
    *const u8,
    usize,
    *const u32,
    *mut *mut u8,
    *mut usize,
) -> i32;
pub(super) type VerifyFn =
    unsafe extern "C" fn(*const u8, usize, *const u32, *const u8, usize) -> i32;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"AORCP001";

fn validate_prove(elf_len: usize, input_len: usize) -> Result<()> {
    if elf_len == 0 || elf_len > 64 * 1024 * 1024 || input_len > MAX_BYTES {
        bail!("portable RISC0 ELF/input exceeds supported bounds");
    }
    Ok(())
}

fn validate_verify(proof: &[u8], journal_len: usize) -> Result<()> {
    if proof.len() > MAX_BYTES || !proof.starts_with(MAGIC) || journal_len > MAX_BYTES {
        bail!("invalid portable RISC0 receipt envelope or journal bounds");
    }
    Ok(())
}

impl AoemDyn {
    /// Export presence only, NOT backend readiness or transaction validity.
    /// Older libraries remain loadable and report false here.
    pub fn has_risc0_portable_exports_v1(&self) -> bool {
        self.risc0_prove_v1.is_some() && self.risc0_verify_v1.is_some() && self.free.is_some()
    }

    /// Trusted local guest execution; the host must provide isolation/resource budgets.
    /// `image` is the host-pinned RISC0 image ID, not an ELF SHA256.
    pub fn risc0_prove_v1(&self, elf: &[u8], input: &[u8], image: &[u32; 8]) -> Result<Vec<u8>> {
        validate_prove(elf.len(), input.len())?;
        let prove = self
            .risc0_prove_v1
            .ok_or_else(|| anyhow!("aoem_risc0_prove_v1 not found in loaded library"))?;
        // Resolve free BEFORE calling a producer that may allocate.
        let free = self
            .free
            .ok_or_else(|| anyhow!("aoem_free not found in loaded library"))?;
        let mut p = ptr::null_mut();
        let mut n = 0;
        let rc = unsafe {
            prove(
                elf.as_ptr(),
                elf.len(),
                input.as_ptr(),
                input.len(),
                image.as_ptr(),
                &mut p,
                &mut n,
            )
        };
        if rc != 0 || p.is_null() || n == 0 || n > MAX_BYTES {
            if !p.is_null() {
                unsafe { free(p, n) };
            }
            bail!("aoem_risc0_prove_v1 failed or returned invalid buffer: rc={rc}, len={n}");
        }
        let receipt = self.copy_aoem_owned_bytes(p, n, "aoem_risc0_prove_v1")?;
        validate_verify(&receipt, 0)?;
        Ok(receipt)
    }

    /// Verifies cryptographic validity and exact host-pinned journal equality.
    /// Pins must come from trusted host policy, not the supplied receipt.
    /// Empty expected journal means exactly empty; never skip output verification.
    pub fn risc0_verify_v1(
        &self,
        proof: &[u8],
        image: &[u32; 8],
        expected_journal: &[u8],
    ) -> Result<()> {
        validate_verify(proof, expected_journal.len())?;
        let verify = self
            .risc0_verify_v1
            .ok_or_else(|| anyhow!("aoem_risc0_verify_v1 not found in loaded library"))?;
        let rc = unsafe {
            verify(
                proof.as_ptr(),
                proof.len(),
                image.as_ptr(),
                expected_journal.as_ptr(),
                expected_journal.len(),
            )
        };
        if rc != 0 {
            bail!("aoem_risc0_verify_v1 rejected or unavailable: rc={rc}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_receipt_input_bounds() {
        assert!(validate_prove(0, 0).is_err());
        assert!(validate_prove(64 * 1024 * 1024, MAX_BYTES).is_ok());
        assert!(validate_prove(64 * 1024 * 1024 + 1, 0).is_err());
        assert!(validate_prove(1, MAX_BYTES + 1).is_err());
    }
    #[test]
    fn portable_receipt_envelope_is_not_cryptographic_verification() {
        assert!(validate_verify(b"", 0).is_err());
        assert!(validate_verify(b"trace-digest", 0).is_err());
        assert!(validate_verify(MAGIC, MAX_BYTES + 1).is_err());
        // Envelope acceptance still requires a real backend verification call.
        assert!(validate_verify(MAGIC, 0).is_ok());
    }
}
