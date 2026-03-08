//! 隐私转账功能

use crate::types::*;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::env;

const PRIVACY_RING_SIG_PLACEHOLDER_ALLOW_ENV: &str =
    "NOVOVM_WEB30_PRIVACY_RING_SIG_PLACEHOLDER_ALLOW";

fn placeholder_ring_sig_allowed() -> bool {
    env::var(PRIVACY_RING_SIG_PLACEHOLDER_ALLOW_ENV)
        .map(|v| {
            let v = v.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
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
    // 安全默认：占位实现在生产环境禁用，避免“永真”验签被利用。
    if !placeholder_ring_sig_allowed() {
        return Ok(false);
    }

    if signature.ring_members.is_empty() {
        return Ok(false);
    }

    if signature.c.len() != signature.ring_members.len() {
        return Ok(false);
    }

    if signature.r.len() != signature.ring_members.len() {
        return Ok(false);
    }

    // 占位校验：至少约束 challenge 与消息/金额绑定，避免任意垃圾输入直接通过。
    // 注意：这不是完整环签名验证，仅用于受控测试场景。
    let mut challenge_hasher = Sha256::new();
    challenge_hasher.update(message);
    challenge_hasher.update(amount.to_le_bytes());
    let expected_c: [u8; 32] = challenge_hasher.finalize().into();
    if signature.c.iter().any(|c| c != &expected_c) {
        return Ok(false);
    }

    Ok(true)
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

        // 默认禁用占位验签，防止生产误用。
        let is_valid = verify_ring_signature(&signature, message, 1000).expect("Failed to verify");
        assert!(!is_valid);
    }

    #[test]
    fn test_ring_signature_placeholder_requires_explicit_env_enable() {
        let private_key = [9u8; 32];
        let ring_members = vec![
            Address::from_bytes([11u8; 32]),
            Address::from_bytes([12u8; 32]),
        ];
        let message = b"msg";
        let amount = 123u128;

        let signature = generate_ring_signature(&private_key, &ring_members, message, 0)
            .expect("Failed to generate signature");
        std::env::set_var(PRIVACY_RING_SIG_PLACEHOLDER_ALLOW_ENV, "1");
        let enabled_result =
            verify_ring_signature(&signature, message, amount).expect("verify failed");
        std::env::remove_var(PRIVACY_RING_SIG_PLACEHOLDER_ALLOW_ENV);
        // generate_ring_signature() uses challenge=hash(message), while verifier binds (message, amount).
        // This ensures placeholder verifier is never trivially "always true".
        assert!(!enabled_result);
    }
}
