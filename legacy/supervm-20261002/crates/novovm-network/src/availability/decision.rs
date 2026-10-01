use crate::availability::AvailabilityMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvailabilityDecision {
    pub mode: AvailabilityMode,
    pub reason: &'static str,
}

impl AvailabilityDecision {
    pub fn normal(reason: &'static str) -> Self {
        Self {
            mode: AvailabilityMode::Normal,
            reason,
        }
    }

    pub fn read_only(reason: &'static str) -> Self {
        Self {
            mode: AvailabilityMode::ReadOnly,
            reason,
        }
    }

    pub fn queue_only(reason: &'static str) -> Self {
        Self {
            mode: AvailabilityMode::QueueOnly,
            reason,
        }
    }
}
