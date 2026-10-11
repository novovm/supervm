//! Account-scoped Windows DPAPI. Never use LOCAL_MACHINE protection: the SCM
//! service creates and opens its keys under its dedicated virtual account.
use anyhow::{ensure, Result};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};
use zeroize::{Zeroize, Zeroizing};

pub fn protect(input: &[u8]) -> Result<Vec<u8>> {
    Ok(transform(input, false)?.to_vec())
}
pub fn unprotect(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    transform(input, true)
}
fn transform(input: &[u8], decrypt: bool) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        !input.is_empty() && input.len() <= 16384,
        "custody input size"
    );
    let data = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    // SAFETY: bounded live input, initialized output, optional arguments null.
    // DPAPI treats the input as read-only and allocates output with LocalAlloc.
    let ok = unsafe {
        if decrypt {
            CryptUnprotectData(
                &data,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &data,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    ensure!(ok != 0, "Windows account custody unavailable");
    // SAFETY: a successful DPAPI call returns cbData initialized bytes. Clear
    // even decrypted memory before freeing it; the Rust copy is also zeroizing.
    let result = unsafe {
        let bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copy = Zeroizing::new(bytes.to_vec());
        bytes.zeroize();
        LocalFree(output.pbData.cast());
        copy
    };
    ensure!(result.len() <= 16384, "custody output size");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_windows_custody_roundtrip_rejects_tampering() {
        let secret = Zeroizing::new([97u8; 64]);
        let mut sealed = protect(secret.as_ref()).unwrap();
        assert!(!sealed.windows(32).any(|w| w == [97; 32]));
        assert_eq!(&*unprotect(&sealed).unwrap(), secret.as_ref());
        let at = sealed.len() / 2;
        sealed[at] ^= 1;
        assert!(unprotect(&sealed).is_err());
    }
}
