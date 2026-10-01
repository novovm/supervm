use crate::types::{BFTError, BFTResult, GovernanceVote};
use ed25519_dalek::VerifyingKey;
use std::sync::Arc;

/// Minimum vote count to prefer batch verification over per-vote verification.
pub const GOVERNANCE_VOTE_VERIFY_BATCH_MIN: usize = 8;

/// 治理投票签名校验器（I-GOV-04 execute-hook）。
///
/// 默认实现为 `ed25519`，其他方案（例如 ML-DSA）通过外部注入。
pub trait GovernanceVoteVerifier: Send + Sync {
    /// 校验器名称（用于审计/调试输出）。
    fn name(&self) -> &'static str;

    /// 校验器方案标识（用于能力判定与审计输出）。
    fn scheme(&self) -> GovernanceVoteVerifierScheme;

    /// 校验单个治理投票签名。
    fn verify(&self, vote: &GovernanceVote, key: &VerifyingKey) -> BFTResult<()>;

    /// 校验并返回可审计的验签报告（默认实现）。
    fn verify_with_report(
        &self,
        vote: &GovernanceVote,
        key: &VerifyingKey,
    ) -> BFTResult<GovernanceVoteVerificationReport> {
        self.verify(vote, key)?;
        Ok(GovernanceVoteVerificationReport {
            verifier_name: self.name(),
            scheme: self.scheme(),
        })
    }

    /// 批量校验治理投票签名（默认逐条调用 `verify_with_report`）。
    fn verify_batch_with_report(
        &self,
        inputs: &[GovernanceVoteVerificationInput<'_>],
    ) -> BFTResult<Vec<GovernanceVoteVerificationReport>> {
        let mut out = Vec::with_capacity(inputs.len());
        for input in inputs {
            out.push(self.verify_with_report(input.vote, input.key)?);
        }
        Ok(out)
    }
}

/// 治理投票验签报告（用于共识层审计输出）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceVoteVerificationReport {
    pub verifier_name: &'static str,
    pub scheme: GovernanceVoteVerifierScheme,
}

/// 批量治理投票验签输入。
#[derive(Clone, Copy)]
pub struct GovernanceVoteVerificationInput<'a> {
    pub vote: &'a GovernanceVote,
    pub key: &'a VerifyingKey,
}

/// 默认治理投票签名校验器（ed25519）。
pub struct Ed25519GovernanceVoteVerifier;

impl GovernanceVoteVerifier for Ed25519GovernanceVoteVerifier {
    fn name(&self) -> &'static str {
        "ed25519"
    }

    fn scheme(&self) -> GovernanceVoteVerifierScheme {
        GovernanceVoteVerifierScheme::Ed25519
    }

    fn verify(&self, vote: &GovernanceVote, key: &VerifyingKey) -> BFTResult<()> {
        vote.verify(key)
    }
}

/// 治理验签器方案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceVoteVerifierScheme {
    Ed25519,
    MlDsa87,
}

impl GovernanceVoteVerifierScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            GovernanceVoteVerifierScheme::Ed25519 => "ed25519",
            GovernanceVoteVerifierScheme::MlDsa87 => "mldsa87",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim().to_ascii_lowercase();
        match s.as_str() {
            "ed25519" => Some(GovernanceVoteVerifierScheme::Ed25519),
            "mldsa87" | "mldsa" | "ml-dsa" | "ml-dsa-87" => {
                Some(GovernanceVoteVerifierScheme::MlDsa87)
            }
            _ => None,
        }
    }
}

/// 构造治理验签器实例。
///
/// 说明：`mldsa87` 需要由调用方注入外部 verifier（例如 AOEM-FFI 路径），
/// 因此这里仅提供默认 `ed25519` 构造。
pub fn build_governance_vote_verifier(
    scheme: GovernanceVoteVerifierScheme,
) -> BFTResult<Arc<dyn GovernanceVoteVerifier>> {
    match scheme {
        GovernanceVoteVerifierScheme::Ed25519 => Ok(Arc::new(Ed25519GovernanceVoteVerifier)),
        GovernanceVoteVerifierScheme::MlDsa87 => Err(BFTError::InvalidProposal(
            "unsupported governance vote verifier: mldsa87 (policy-gated, requires external verifier injection)"
                .to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governance_vote_verifier_scheme_parser_accepts_supported_values() {
        assert_eq!(
            GovernanceVoteVerifierScheme::parse("ed25519"),
            Some(GovernanceVoteVerifierScheme::Ed25519)
        );
        assert_eq!(
            GovernanceVoteVerifierScheme::parse("ml-dsa-87"),
            Some(GovernanceVoteVerifierScheme::MlDsa87)
        );
        assert_eq!(
            GovernanceVoteVerifierScheme::parse("mldsa87"),
            Some(GovernanceVoteVerifierScheme::MlDsa87)
        );
        assert_eq!(GovernanceVoteVerifierScheme::parse("bad"), None);
    }

    #[test]
    fn build_governance_vote_verifier_rejects_mldsa87_without_external_injection() {
        let ok = build_governance_vote_verifier(GovernanceVoteVerifierScheme::Ed25519);
        assert!(ok.is_ok());

        let err = match build_governance_vote_verifier(GovernanceVoteVerifierScheme::MlDsa87) {
            Ok(_) => panic!("mldsa87 should require external verifier injection"),
            Err(e) => e,
        };
        assert!(err
            .to_string()
            .to_lowercase()
            .contains("unsupported governance vote verifier"));
        assert!(err.to_string().to_lowercase().contains("policy-gated"));
    }
}
