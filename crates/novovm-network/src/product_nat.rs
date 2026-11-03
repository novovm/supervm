//! Routing outcomes shared by the original node overlay. Authentication and
//! observed-endpoint qualification use NativeEndpoint, not a second signed UDP
//! codec. The superseded raw Ed25519 NAT probe is deliberately not exported.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatDiagnosisV1 {
    PunchedDirect,
    UdpReachabilityBlockedOrAckReturnFailed,
    InvalidAckRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NatSelectedPathV1 {
    PunchedDirect,
    RelayNovoRudp,
    QueueFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NatPunchAttemptV1 {
    pub diagnosis: NatDiagnosisV1,
    pub selected_path_after_punch: NatSelectedPathV1,
    pub relay_fallback_selected: bool,
    pub fallback_reason: Option<String>,
    pub ack_valid: bool,
}

pub fn fallback_after_nat_failure_v1(
    diagnosis: NatDiagnosisV1,
    relay_candidate_available: bool,
) -> NatPunchAttemptV1 {
    NatPunchAttemptV1 {
        diagnosis,
        selected_path_after_punch: if relay_candidate_available {
            NatSelectedPathV1::RelayNovoRudp
        } else {
            NatSelectedPathV1::QueueFallback
        },
        relay_fallback_selected: relay_candidate_available,
        fallback_reason: Some(if relay_candidate_available {
            "NatPunchFailed".into()
        } else {
            "NoReachableRelayCandidate".into()
        }),
        ack_valid: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_direct_path_does_not_claim_authenticated_delivery() {
        for relay in [false, true] {
            let result = fallback_after_nat_failure_v1(NatDiagnosisV1::InvalidAckRejected, relay);
            assert!(!result.ack_valid);
            assert_eq!(result.relay_fallback_selected, relay);
            assert_eq!(
                result.selected_path_after_punch,
                if relay {
                    NatSelectedPathV1::RelayNovoRudp
                } else {
                    NatSelectedPathV1::QueueFallback
                }
            );
        }
    }
}
