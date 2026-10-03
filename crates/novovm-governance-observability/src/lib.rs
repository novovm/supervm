#![forbid(unsafe_code)]

pub mod collectors;
pub mod events;
pub mod export;
pub mod metrics;

pub use collectors::pr_gate::{collect_gate_metrics, GateMetrics};
pub use collectors::runtime::{collect_runtime_signals, RuntimeSignals};
pub use collectors::trigger::{collect_trigger_metrics, TriggerMetrics};
pub use events::{GovernanceEvent, GovernanceEventEnvelope};
pub use export::jsonl::{
    append_governance_event, append_governance_event_auto, default_governance_events_dir,
    default_governance_events_path, discover_governance_event_files,
    load_governance_events_from_file, load_governance_events_from_paths,
};
pub use metrics::{
    build_governance_assessment, GovernanceAssessment, GovernanceThresholds, Phase4Decision,
    Phase4DecisionReport, Phase4DecisionThresholds,
};
