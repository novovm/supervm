#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityBackend {
    Native,
    Libp2pStub,
}

impl CapabilityBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Libp2pStub => "libp2p_stub",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReport {
    pub backend: CapabilityBackend,
    pub enabled: bool,
    pub reason: String,
}

impl CapabilityReport {
    pub fn new(backend: CapabilityBackend, enabled: bool, reason: impl Into<String>) -> Self {
        Self {
            backend,
            enabled,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityReadiness {
    Ready,
    Limited,
    Disabled,
}

impl CapabilityReadiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Limited => "limited",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReadOnlyImpact {
    pub readiness: CapabilityReadiness,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityRouteHint {
    Standard,
    PreferL3Relay,
    DegradedOnly,
}

impl CapabilityRouteHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::PreferL3Relay => "prefer_l3_relay",
            Self::DegradedOnly => "degraded_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityAvailabilityHint {
    NormalPreferred,
    QueueOnlyTolerant,
    QueueOnlyPreferred,
    ReadOnlyRecommended,
}

impl CapabilityAvailabilityHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NormalPreferred => "normal_preferred",
            Self::QueueOnlyTolerant => "queue_only_tolerant",
            Self::QueueOnlyPreferred => "queue_only_preferred",
            Self::ReadOnlyRecommended => "read_only_recommended",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityAdvisory {
    pub route_hint: CapabilityRouteHint,
    pub availability_hint: CapabilityAvailabilityHint,
    pub binding: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityPolicyMode {
    AdvisoryFirst,
}

impl CapabilityPolicyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdvisoryFirst => "advisory_first",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityPolicyEvaluation {
    pub mode: CapabilityPolicyMode,
    pub route_adopted: bool,
    pub availability_adopted: bool,
}

pub fn assess_read_only_impact(report: &CapabilityReport) -> CapabilityReadOnlyImpact {
    match (report.backend, report.enabled) {
        (CapabilityBackend::Native, true) => CapabilityReadOnlyImpact {
            readiness: CapabilityReadiness::Ready,
            summary: "native_backend_active",
        },
        (CapabilityBackend::Native, false) => CapabilityReadOnlyImpact {
            readiness: CapabilityReadiness::Disabled,
            summary: "native_backend_disabled",
        },
        (CapabilityBackend::Libp2pStub, true) => CapabilityReadOnlyImpact {
            readiness: CapabilityReadiness::Limited,
            summary: "libp2p_stub_enabled_read_only",
        },
        (CapabilityBackend::Libp2pStub, false) => CapabilityReadOnlyImpact {
            readiness: CapabilityReadiness::Disabled,
            summary: "libp2p_stub_disabled",
        },
    }
}

pub fn derive_advisory(report: &CapabilityReport) -> CapabilityAdvisory {
    match (report.backend, report.enabled) {
        (CapabilityBackend::Native, true) => CapabilityAdvisory {
            route_hint: CapabilityRouteHint::Standard,
            availability_hint: CapabilityAvailabilityHint::NormalPreferred,
            binding: false,
        },
        (CapabilityBackend::Native, false) => CapabilityAdvisory {
            route_hint: CapabilityRouteHint::DegradedOnly,
            availability_hint: CapabilityAvailabilityHint::ReadOnlyRecommended,
            binding: false,
        },
        (CapabilityBackend::Libp2pStub, true) => CapabilityAdvisory {
            route_hint: CapabilityRouteHint::PreferL3Relay,
            availability_hint: CapabilityAvailabilityHint::QueueOnlyTolerant,
            binding: false,
        },
        (CapabilityBackend::Libp2pStub, false) => CapabilityAdvisory {
            route_hint: CapabilityRouteHint::DegradedOnly,
            availability_hint: CapabilityAvailabilityHint::QueueOnlyPreferred,
            binding: false,
        },
    }
}

pub fn capability_state_token(report: &CapabilityReport) -> String {
    let impact = assess_read_only_impact(report);
    let advisory = derive_advisory(report);
    format!(
        "backend={};enabled={};readiness={};route_hint={};availability_hint={}",
        report.backend.as_str(),
        if report.enabled { "1" } else { "0" },
        impact.readiness.as_str(),
        advisory.route_hint.as_str(),
        advisory.availability_hint.as_str()
    )
}

pub fn evaluate_advisory_first(
    advisory: CapabilityAdvisory,
    selected_strategy: &str,
    selected_availability_mode: &str,
) -> CapabilityPolicyEvaluation {
    let route_adopted = match advisory.route_hint {
        CapabilityRouteHint::Standard => true,
        CapabilityRouteHint::PreferL3Relay => selected_strategy == "l3_relay",
        CapabilityRouteHint::DegradedOnly => selected_strategy == "queue_only",
    };

    let availability_adopted = match advisory.availability_hint {
        CapabilityAvailabilityHint::NormalPreferred => selected_availability_mode == "normal",
        CapabilityAvailabilityHint::QueueOnlyTolerant => {
            selected_availability_mode == "normal" || selected_availability_mode == "queue_only"
        }
        CapabilityAvailabilityHint::QueueOnlyPreferred => {
            selected_availability_mode == "queue_only"
        }
        CapabilityAvailabilityHint::ReadOnlyRecommended => {
            selected_availability_mode == "read_only"
        }
    };

    CapabilityPolicyEvaluation {
        mode: CapabilityPolicyMode::AdvisoryFirst,
        route_adopted,
        availability_adopted,
    }
}

pub fn detect_capabilities() -> CapabilityReport {
    let backend = match std::env::var("NOVOVM_CAPABILITY_BACKEND")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("libp2p_stub") => CapabilityBackend::Libp2pStub,
        _ => CapabilityBackend::Native,
    };

    match backend {
        CapabilityBackend::Native => {
            CapabilityReport::new(CapabilityBackend::Native, true, "baseline_v0_native_stack")
        }
        CapabilityBackend::Libp2pStub => {
            let enabled = bool_env("NOVOVM_LIBP2P_STUB_ENABLED");
            if enabled {
                CapabilityReport::new(
                    CapabilityBackend::Libp2pStub,
                    true,
                    "phase_d0_capability_stub_enabled",
                )
            } else {
                CapabilityReport::new(
                    CapabilityBackend::Libp2pStub,
                    false,
                    "phase_d0_capability_stub_disabled",
                )
            }
        }
    }
}

fn bool_env(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        assess_read_only_impact, capability_state_token, derive_advisory, detect_capabilities,
        evaluate_advisory_first, CapabilityAvailabilityHint, CapabilityBackend,
        CapabilityReadiness, CapabilityReport, CapabilityRouteHint,
    };
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn set_or_remove(name: &str, value: Option<&str>) {
        match value {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
    }

    #[test]
    fn detect_defaults_to_native_enabled() {
        let _g = env_lock().lock().expect("env lock");
        let old_backend = std::env::var("NOVOVM_CAPABILITY_BACKEND").ok();
        let old_stub = std::env::var("NOVOVM_LIBP2P_STUB_ENABLED").ok();

        std::env::remove_var("NOVOVM_CAPABILITY_BACKEND");
        std::env::remove_var("NOVOVM_LIBP2P_STUB_ENABLED");

        let report = detect_capabilities();
        assert_eq!(report.backend, CapabilityBackend::Native);
        assert!(report.enabled);
        assert_eq!(report.reason, "baseline_v0_native_stack");

        set_or_remove("NOVOVM_CAPABILITY_BACKEND", old_backend.as_deref());
        set_or_remove("NOVOVM_LIBP2P_STUB_ENABLED", old_stub.as_deref());
    }

    #[test]
    fn detect_libp2p_stub_enabled() {
        let _g = env_lock().lock().expect("env lock");
        let old_backend = std::env::var("NOVOVM_CAPABILITY_BACKEND").ok();
        let old_stub = std::env::var("NOVOVM_LIBP2P_STUB_ENABLED").ok();

        std::env::set_var("NOVOVM_CAPABILITY_BACKEND", "libp2p_stub");
        std::env::set_var("NOVOVM_LIBP2P_STUB_ENABLED", "1");

        let report = detect_capabilities();
        assert_eq!(report.backend, CapabilityBackend::Libp2pStub);
        assert!(report.enabled);
        assert_eq!(report.reason, "phase_d0_capability_stub_enabled");

        set_or_remove("NOVOVM_CAPABILITY_BACKEND", old_backend.as_deref());
        set_or_remove("NOVOVM_LIBP2P_STUB_ENABLED", old_stub.as_deref());
    }

    #[test]
    fn detect_libp2p_stub_disabled() {
        let _g = env_lock().lock().expect("env lock");
        let old_backend = std::env::var("NOVOVM_CAPABILITY_BACKEND").ok();
        let old_stub = std::env::var("NOVOVM_LIBP2P_STUB_ENABLED").ok();

        std::env::set_var("NOVOVM_CAPABILITY_BACKEND", "libp2p_stub");
        std::env::set_var("NOVOVM_LIBP2P_STUB_ENABLED", "0");

        let report = detect_capabilities();
        assert_eq!(report.backend, CapabilityBackend::Libp2pStub);
        assert!(!report.enabled);
        assert_eq!(report.reason, "phase_d0_capability_stub_disabled");

        set_or_remove("NOVOVM_CAPABILITY_BACKEND", old_backend.as_deref());
        set_or_remove("NOVOVM_LIBP2P_STUB_ENABLED", old_stub.as_deref());
    }

    #[test]
    fn read_only_impact_native_enabled_is_ready() {
        let report = CapabilityReport::new(CapabilityBackend::Native, true, "test");
        let impact = assess_read_only_impact(&report);
        assert_eq!(impact.readiness, CapabilityReadiness::Ready);
        assert_eq!(impact.summary, "native_backend_active");
    }

    #[test]
    fn read_only_impact_stub_enabled_is_limited() {
        let report = CapabilityReport::new(CapabilityBackend::Libp2pStub, true, "test");
        let impact = assess_read_only_impact(&report);
        assert_eq!(impact.readiness, CapabilityReadiness::Limited);
        assert_eq!(impact.summary, "libp2p_stub_enabled_read_only");
    }

    #[test]
    fn read_only_impact_stub_disabled_is_disabled() {
        let report = CapabilityReport::new(CapabilityBackend::Libp2pStub, false, "test");
        let impact = assess_read_only_impact(&report);
        assert_eq!(impact.readiness, CapabilityReadiness::Disabled);
        assert_eq!(impact.summary, "libp2p_stub_disabled");
    }

    #[test]
    fn advisory_native_enabled_prefers_normal() {
        let report = CapabilityReport::new(CapabilityBackend::Native, true, "test");
        let advisory = derive_advisory(&report);
        assert_eq!(advisory.route_hint, CapabilityRouteHint::Standard);
        assert_eq!(
            advisory.availability_hint,
            CapabilityAvailabilityHint::NormalPreferred
        );
        assert!(!advisory.binding);
    }

    #[test]
    fn advisory_stub_enabled_prefers_l3_relay() {
        let report = CapabilityReport::new(CapabilityBackend::Libp2pStub, true, "test");
        let advisory = derive_advisory(&report);
        assert_eq!(advisory.route_hint, CapabilityRouteHint::PreferL3Relay);
        assert_eq!(
            advisory.availability_hint,
            CapabilityAvailabilityHint::QueueOnlyTolerant
        );
        assert!(!advisory.binding);
    }

    #[test]
    fn advisory_stub_disabled_prefers_queue_only() {
        let report = CapabilityReport::new(CapabilityBackend::Libp2pStub, false, "test");
        let advisory = derive_advisory(&report);
        assert_eq!(advisory.route_hint, CapabilityRouteHint::DegradedOnly);
        assert_eq!(
            advisory.availability_hint,
            CapabilityAvailabilityHint::QueueOnlyPreferred
        );
        assert!(!advisory.binding);
    }

    #[test]
    fn state_token_native_enabled() {
        let report = CapabilityReport::new(CapabilityBackend::Native, true, "t");
        let token = capability_state_token(&report);
        assert_eq!(
            token,
            "backend=native;enabled=1;readiness=ready;route_hint=standard;availability_hint=normal_preferred"
        );
    }

    #[test]
    fn state_token_stub_disabled() {
        let report = CapabilityReport::new(CapabilityBackend::Libp2pStub, false, "t");
        let token = capability_state_token(&report);
        assert_eq!(
            token,
            "backend=libp2p_stub;enabled=0;readiness=disabled;route_hint=degraded_only;availability_hint=queue_only_preferred"
        );
    }

    #[test]
    fn policy_eval_prefers_l3_relay_and_queue_tolerant() {
        let advisory = derive_advisory(&CapabilityReport::new(
            CapabilityBackend::Libp2pStub,
            true,
            "t",
        ));
        let eval = evaluate_advisory_first(advisory, "l3_relay", "normal");
        assert!(eval.route_adopted);
        assert!(eval.availability_adopted);
    }

    #[test]
    fn policy_eval_prefers_queue_only_when_stub_disabled() {
        let advisory = derive_advisory(&CapabilityReport::new(
            CapabilityBackend::Libp2pStub,
            false,
            "t",
        ));
        let eval = evaluate_advisory_first(advisory, "queue_only", "queue_only");
        assert!(eval.route_adopted);
        assert!(eval.availability_adopted);
    }

    #[test]
    fn policy_eval_can_detect_non_adopted() {
        let advisory = derive_advisory(&CapabilityReport::new(
            CapabilityBackend::Libp2pStub,
            true,
            "t",
        ));
        let eval = evaluate_advisory_first(advisory, "direct_l4", "read_only");
        assert!(!eval.route_adopted);
        assert!(!eval.availability_adopted);
    }
}
