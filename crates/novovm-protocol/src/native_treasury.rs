//! Existing treasury deposit module transition, not a full transaction verifier.
//! Asset must already be normalized by the production argument decoder.
//! Fee settlement, caller authority, clock/parent provenance and root/receipt
//! binding remain the caller's obligations. No IO, environment or AOEM dependency.
pub struct ReserveProofViewV1<'a> {
    pub status: &'a str,
    pub expires_at_unix_ms: u128,
    pub reserve_amount: u128,
    pub proof_type: &'a str,
    pub proof_source: &'a str,
    pub proof_reference: &'a str,
}
#[derive(Debug, PartialEq, Eq)]
pub struct DepositRejectionV1 {
    pub code: &'static str,
    pub reason: String,
}
/// Computes the new reserve without mutating any state. Preserves legacy
/// saturation and policy (including absent proof and unknown-status handling).
pub fn deposit_reserve_transition_v1(
    asset: &str,
    current: u128,
    amount: u128,
    proof: Option<ReserveProofViewV1<'_>>,
    now_ms: u128,
) -> Result<u128, DepositRejectionV1> {
    let after = current.saturating_add(amount);
    if asset != "NOV" {
        if let Some(proof) = proof {
            let mut status = match proof.status.trim().to_ascii_lowercase().as_str() {
                "active" | "valid" => "active",
                "constrained" | "review" | "under_review" => "constrained",
                "revoked" | "disabled" => "revoked",
                "expired" => "expired",
                _ => "active",
            };
            if status != "revoked"
                && proof.expires_at_unix_ms > 0
                && now_ms > proof.expires_at_unix_ms
            {
                status = "expired";
            }
            if status != "active" {
                return Err(DepositRejectionV1 {
                    code: "reserve_proof_not_active",
                    reason: format!("asset={} reserve_proof_effective_status={} proof_type={} proof_source={} proof_reference={}",
                        asset, status, proof.proof_type, proof.proof_source, proof.proof_reference),
                });
            }
            if after > proof.reserve_amount {
                return Err(DepositRejectionV1 {
                    code: "reserve_proof_capacity_exceeded",
                    reason: format!("asset={} projected_reserve_after={} proof_reserve_amount={} proof_type={} proof_source={} proof_reference={}",
                        asset, after, proof.reserve_amount, proof.proof_type, proof.proof_source, proof.proof_reference),
                });
            }
        }
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_preserves_full_u128_saturating_arithmetic() {
        for (current, amount) in [(u128::MAX - 1, 2), (u128::MAX, u128::MAX)] {
            assert_eq!(
                deposit_reserve_transition_v1("NOV", current, amount, None, 10),
                Ok(u128::MAX)
            );
            assert_eq!(
                deposit_reserve_transition_v1("USDT", current, amount, None, 10),
                Ok(u128::MAX)
            );
        }
    }
}
