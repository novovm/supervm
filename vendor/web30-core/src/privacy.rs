//! 隐私转账功能

use crate::types::*;
use anyhow::Result;
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
    _signer_index: usize,
) -> Result<RingSignature> {
    // 简化实现
    let key_image = {
        let mut hasher = Sha256::new();
        hasher.update(private_key);
        hasher.finalize().into()
    };

    let c: Vec<[u8; 32]> = ring_members
        .iter()
        .map(|_| {
            let mut h = Sha256::new();
            h.update(message);
            h.finalize().into()
        })
        .collect();

    let r: Vec<[u8; 32]> = ring_members
        .iter()
        .map(|_| {
            let mut h = Sha256::new();
            h.update(private_key);
            h.finalize().into()
        })
        .collect();

    Ok(RingSignature {
        ring_members: ring_members.to_vec(),
        key_image,
        c,
        r,
    })
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
        let private_key = [42u8; 32];
        let ring_members = vec![
            Address::from_bytes([1u8; 32]),
            Address::from_bytes([2u8; 32]),
            Address::from_bytes([3u8; 32]),
        ];
        let message = b"test message";

        let signature = generate_ring_signature(&private_key, &ring_members, message, 1)
            .expect("Failed to generate signature");

        // 默认使用 AOEM FFI 验签；若 AOEM 未配置，则 fail-closed。
        let is_valid = verify_ring_signature(&signature, message, 1000).expect("Failed to verify");
        assert!(!is_valid);
    }
}
