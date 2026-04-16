use crate::availability::AvailabilityMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AvailabilityAuditEventKind {
    ModeChanged,
    Queued,
    Replayed,
    ReconcileChecked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityAuditEvent {
    pub kind: AvailabilityAuditEventKind,
    pub at_unix_ms: u64,
    pub mode: AvailabilityMode,
    pub request_id: Option<String>,
    pub detail: String,
}

#[derive(Debug, Default, Clone)]
pub struct AvailabilityAuditLog {
    events: Vec<AvailabilityAuditEvent>,
}

impl AvailabilityAuditLog {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn record(&mut self, event: AvailabilityAuditEvent) {
        self.events.push(event);
    }

    pub fn drain(&mut self) -> Vec<AvailabilityAuditEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}
