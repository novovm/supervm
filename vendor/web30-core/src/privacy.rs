//! 隐私转账功能

use crate::types::*;
use anyhow::{bail, Result};
use sha2::{Digest, Sha256};
use std::env;

#[cfg(feature = "aoem-ring-ffi")]
use aoem_bindings::AoemDyn;

const PRIVACY_RING_SIG_BACKEND_ENV: &str = "NOVOVM_WEB30_PRIVACY_RING_SIG_BACKEND";

fn ring_sig_backend() -> &'static str {
    match env::var(PRIVACY_RING_SIG_BACKEND_ENV) {
        Ok(raw) if raw.trim().eq_ignore_ascii_case("none") => "none",
        _ => "aoem_ffi",
    }
}

#[cfg(feature = "aoem-ring-ffi")]
fn resolve_aoem_dll_path() -> Option<String> {
    for key in ["NOVOVM_AOEM_DLL", "AOEM_DLL", "AOEM_FFI_DLL"] {
        if let Ok(value) = env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(feature = "aoem-ring-ffi")]
fn verify_ring_signature_via_aoem(
    signature: &RingSignature,
    message: &[u8],
    amount: u128,
) -> Result<bool> {
    let Some(dll_path) = resolve_aoem_dll_path() else {
        return Ok(false);
    };
    let dynlib = unsafe { AoemDyn::load(&dll_path) }?;
    if !dynlib.supports_ring_signature_verify() {
        return Ok(false);
    }
    if dynlib.ring_signature_supported_flag() != Some(true) {
        return Ok(false);
    }
    let payload = serde_json::to_vec(signature)?;
    dynlib.ring_signature_verify_web30_v1(payload.as_slice(), message, amount)
}

#[cfg(feature = "aoem-ring-ffi")]
fn generate_ring_signature_via_aoem(
    private_key: &[u8; 32],
    ring_members: &[Address],
    message: &[u8],
    amount: u128,
    signer_index: usize,
) -> Result<RingSignature> {
    if signer_index >= ring_members.len() {
        bail!("signer_index out of range for ring_members");
    }
    let Some(dll_path) = resolve_aoem_dll_path() else {
        bail!("AOEM DLL path is not configured");
    };
    let dynlib = unsafe { AoemDyn::load(&dll_path) }?;
    if !dynlib.supports_ring_signature_sign_web30_v1() {
        bail!("loaded AOEM DLL does not export ring-signature sign ABI");
    }
    if dynlib.ring_signature_supported_flag() != Some(true) {
        bail!("loaded AOEM DLL reports ring-signature capability disabled");
    }
    let ring_json = serde_json::to_vec(
        &ring_members
            .iter()
            .map(|member| *member.as_bytes())
            .collect::<Vec<[u8; 32]>>(),
    )?;
    let public_key = ring_members[signer_index].as_bytes();
    let signature_json = dynlib.ring_signature_sign_web30_v1(
        ring_json.as_slice(),
        signer_index as u32,
        private_key,
        public_key,
        message,
        amount,
    )?;
    serde_json::from_slice::<RingSignature>(signature_json.as_slice())
        .map_err(|e| anyhow::anyhow!("decode AOEM ring-signature payload failed: {e}"))
}

#[cfg(feature = "aoem-ring-ffi")]
fn generate_ring_keypair_via_aoem() -> Result<([u8; 32], [u8; 32])> {
    let Some(dll_path) = resolve_aoem_dll_path() else {
        bail!("AOEM DLL path is not configured");
    };
    let dynlib = unsafe { AoemDyn::load(&dll_path) }?;
    if !dynlib.supports_ring_signature_keygen_v1() {
        bail!("loaded AOEM DLL does not export ring-signature keygen ABI");
    }
    if dynlib.ring_signature_supported_flag() != Some(true) {
        bail!("loaded AOEM DLL reports ring-signature capability disabled");
    }
    let (public_key, secret_key) = dynlib.ring_signature_keygen_v1()?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("AOEM ring public key length must be 32 bytes"))?;
    let secret_key: [u8; 32] = secret_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("AOEM ring secret key length must be 32 bytes"))?;
    Ok((public_key, secret_key))
}

/// 生成隐身地址
pub fn generate_stealth_address(
    view_key: &[u8; 32],
    spend_key: &[u8; 32],
    sender_ephemeral: &[u8; 32],
) -> StealthAddress {
    // 简化实现，实际需要椭圆曲线运算
    let mut hasher = Sha256::new();
    hasher.update(view_key);
    hasher.update(spend_key);
    hasher.update(sender_ephemeral);

    let combined = hasher.finalize();
    let mut new_view = [0u8; 32];
    let mut new_spend = [0u8; 32];
    new_view.copy_from_slice(&combined[..32]);
    new_spend.copy_from_slice(view_key);

    StealthAddress {
        view_key: new_view,
        spend_key: new_spend,
    }
}

/// 验证环签名
pub fn verify_ring_signature(
    signature: &RingSignature,
    message: &[u8],
    amount: u128,
) -> Result<bool> {
    if signature.ring_members.is_empty() {
        return Ok(false);
    }

    if signature.c.len() != signature.ring_members.len() {
        return Ok(false);
    }

    if signature.r.len() != signature.ring_members.len() {
        return Ok(false);
    }

    if ring_sig_backend() == "none" {
        return Ok(false);
    }

    #[cfg(feature = "aoem-ring-ffi")]
    {
        verify_ring_signature_via_aoem(signature, message, amount)
    }

    #[cfg(not(feature = "aoem-ring-ffi"))]
    {
        let _ = (signature, message, amount);
        Ok(false)
    }
}

/// 生成环签名
pub fn generate_ring_signature(
    private_key: &[u8; 32],
    ring_members: &[Address],
    message: &[u8],
    signer_index: usize,
) -> Result<RingSignature> {
    let _ = (private_key, ring_members, message, signer_index);
    bail!("ring signature generation requires amount; use generate_ring_signature_with_amount")
}

/// 生成环签名（显式绑定 amount，供 AOEM Web30 FFI 使用）
pub fn generate_ring_signature_with_amount(
    private_key: &[u8; 32],
    ring_members: &[Address],
    message: &[u8],
    amount: u128,
    signer_index: usize,
) -> Result<RingSignature> {
    if ring_members.is_empty() {
        bail!("ring_members must not be empty");
    }
    if signer_index >= ring_members.len() {
        bail!("signer_index out of range for ring_members");
    }
    if ring_sig_backend() == "none" {
        bail!("ring-signature backend disabled");
    }

    #[cfg(feature = "aoem-ring-ffi")]
    {
        generate_ring_signature_via_aoem(private_key, ring_members, message, amount, signer_index)
    }

    #[cfg(not(feature = "aoem-ring-ffi"))]
    {
        let _ = (private_key, ring_members, message, amount, signer_index);
        bail!("ring signature generation backend is unavailable in this build")
    }
}

/// 生成环签名密钥对
pub fn generate_ring_keypair() -> Result<([u8; 32], [u8; 32])> {
    if ring_sig_backend() == "none" {
        bail!("ring-signature backend disabled");
    }

    #[cfg(feature = "aoem-ring-ffi")]
    {
        generate_ring_keypair_via_aoem()
    }

    #[cfg(not(feature = "aoem-ring-ffi"))]
    {
        bail!("ring signature keygen backend is unavailable in this build")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stealth_address_generation() {
        let view_key = [1u8; 32];
        let spend_key = [2u8; 32];
        let ephemeral = [3u8; 32];

        let stealth = generate_stealth_address(&view_key, &spend_key, &ephemeral);
        assert_ne!(stealth.view_key, view_key);
    }

    #[test]
    fn test_ring_signature() {
        let ring_members = vec![
            Address::from_bytes([1u8; 32]),
            Address::from_bytes([2u8; 32]),
            Address::from_bytes([3u8; 32]),
        ];
        let message = b"test message";
        let signature = RingSignature {
            ring_members: ring_members.clone(),
            key_image: [0u8; 32],
            c: vec![[0u8; 32]; ring_members.len()],
            r: vec![[0u8; 32]; ring_members.len()],
        };

        // 默认使用 AOEM FFI 验签；若 AOEM 未配置，则 fail-closed。
        let is_valid = verify_ring_signature(&signature, message, 1000).expect("Failed to verify");
        assert!(!is_valid);
    }

    #[test]
    fn test_ring_signature_generation_is_disabled_without_backend() {
        let private_key = [42u8; 32];
        let ring_members = vec![Address::from_bytes([1u8; 32])];
        let message = b"test message";
        assert!(generate_ring_signature(&private_key, &ring_members, message, 0).is_err());
    }

    #[test]
    fn test_ring_signature_with_amount_rejects_invalid_signer_index() {
        let private_key = [42u8; 32];
        let ring_members = vec![Address::from_bytes([1u8; 32])];
        let message = b"test message";
        let err = generate_ring_signature_with_amount(&private_key, &ring_members, message, 1, 1)
            .expect_err("invalid signer index must fail");
        assert!(err.to_string().contains("signer_index"));
    }

    #[test]
    fn test_ring_signature_keygen_is_disabled_without_backend() {
        assert!(generate_ring_keypair().is_err());
    }
}
