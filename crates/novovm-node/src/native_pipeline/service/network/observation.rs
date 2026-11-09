//! Opt-in public control-plane observations, owned by the original reactor.
//! No file IO, blocking lock, secret material or authority decisions here.
use super::*;
use novovm_network::iroh_transport::IrohInboundObservationV1;
use std::{cell::Cell, collections::VecDeque};

#[derive(Default)]
pub(super) struct CarrierObservations {
    pub(super) enabled: AtomicBool,
    next: AtomicU64,
    omitted: AtomicU64,
    events: Mutex<VecDeque<serde_json::Value>>,
}
impl CarrierObservations {
    pub(super) fn event(&self, make: impl FnOnce() -> serde_json::Value) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut events) = self.events.try_lock() else {
            self.omitted.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if events.len() == 64 {
            events.pop_front();
            self.omitted.fetch_add(1, Ordering::Relaxed);
        }
        let mut event = make();
        event["wall_ms"] = serde_json::json!(wall_ms().ok());
        events.push_back(event);
    }
    pub(super) fn inbound(&self, event: IrohInboundObservationV1) {
        self.event(|| {
            serde_json::json!({"direction":"inbound", "attempt":event.attempt,
            "phase":format!("{:?}",event.phase), "outcome":format!("{:?}",event.outcome),
            "elapsed_ms":event.elapsed_ms})
        });
    }
    pub(super) fn report(&self) -> serde_json::Value {
        let events = self.events.try_lock().ok();
        serde_json::json!({"enabled":self.enabled.load(Ordering::Relaxed),
            "available":events.is_some(), "events":events.as_deref(),
            "omitted":self.omitted.load(Ordering::Relaxed),
            "attempt_scope":"process_local_per_direction_not_wire_id"})
    }
    pub(super) fn start<'a>(
        &'a self,
        source: &'static str,
        sequence: u64,
        route: &IrohAddressV1,
    ) -> DialObservation<'a> {
        let attempt = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.event(|| {
            serde_json::json!({"direction":"outbound", "attempt":attempt,
            "phase":"candidate", "source":source, "authoritative_sequence":sequence,
            "endpoint_key":route.endpoint_key, "direct_candidates":route.direct,
            "relay_candidates":route.relays, "deadline_ms":8000,
            "selected_remote":null})
        });
        DialObservation {
            owner: self,
            attempt,
            started: Instant::now(),
            phase: Cell::new("carrier connect"),
            ended: false,
        }
    }
}

pub(super) struct DialObservation<'a> {
    pub(super) owner: &'a CarrierObservations,
    pub(super) attempt: u64,
    pub(super) started: Instant,
    pub(super) phase: Cell<&'static str>,
    ended: bool,
}
impl DialObservation<'_> {
    pub(super) fn stage(&self, phase: &'static str) {
        self.phase.set(phase);
        self.owner.event(|| {
            serde_json::json!({"direction":"outbound", "attempt":self.attempt,
            "phase":phase, "elapsed_ms":self.started.elapsed().as_millis()})
        });
    }
    pub(super) fn finish(&mut self, outcome: &'static str, error: Option<&anyhow::Error>) {
        self.ended = true;
        self.owner.event(|| serde_json::json!({"direction":"outbound", "attempt":self.attempt,
            "phase":self.phase.get(), "outcome":outcome, "elapsed_ms":self.started.elapsed().as_millis(),
            "carrier_failure":error.and_then(novovm_network::iroh_transport::iroh_connection_failure_kind_v1).map(|kind|format!("{kind:?}")),
            "capacity_rejected":error.is_some_and(|e|e.is::<novovm_network::iroh_transport::IrohConnectionCapacityV1>()),
            "terminal_auth_or_domain_rejection":error.is_some_and(terminal_session_rejection)}));
    }
}
impl Drop for DialObservation<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.finish("cancelled", None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observations_are_opt_in_bounded_nonblocking_and_record_cancellation() {
        let owner = CarrierObservations::default();
        owner.event(|| panic!("disabled observation evaluated projection"));
        owner.enabled.store(true, Ordering::Relaxed);
        for i in 0..100 {
            owner.event(|| serde_json::json!({"i":i}));
        }
        let report = owner.report();
        assert_eq!(report["events"].as_array().unwrap().len(), 64);
        assert_eq!(report["omitted"], 36);
        let lock = owner.events.lock().unwrap();
        owner.event(|| panic!("contended observation evaluated projection"));
        assert_eq!(owner.report()["available"], false);
        drop(lock);
        let route = IrohAddressV1 {
            endpoint_key: [1; 32],
            direct: vec![],
            relays: vec![],
        };
        {
            let attempt = owner.start("configured_seed", 4166, &route);
            attempt.stage("bound authentication");
        }
        let report = owner.report();
        assert_eq!(
            report["events"].as_array().unwrap().last().unwrap()["outcome"],
            "cancelled"
        );
        assert_eq!(
            report["events"].as_array().unwrap().last().unwrap()["phase"],
            "bound authentication"
        );
    }
}
