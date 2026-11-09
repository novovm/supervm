//! The original opt-in observer also owns local relay evidence. No payload,
//! secret, address, disk writer, route selection or independent completion.
use super::*;
use crate::novorudp::carrier::VerifiedCarrierRoute;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NativeRelayRouteEvidenceV1 {
    pub route_digest: [u8; 32],
    pub circuit: [u8; 16],
    /// Original ML-DSA transport peer IDs, not validator or asset IDs.
    pub origin: String,
    pub path: Vec<String>,
    pub local_peer: String,
    pub role: &'static str,
    pub cursor: usize,
    pub first_hop: VerifiedCarrierRoute,
}

#[cfg(all(test, feature = "iroh-transport"))]
mod tests {
    use super::*;

    fn route(id: u8) -> NativeRelayRouteEvidenceV1 {
        // Component fixture only; production obtains this from actual TLS.
        let key = |seed| {
            ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_bytes()
        };
        let binding =
            crate::transport_binding::VerifiedTransportBinding::from_authenticated_transport(
                key(1),
                key(2),
                [3; 32],
                crate::transport_binding::BOUND_TRANSPORT_ALPN_V1,
                true,
            )
            .unwrap();
        NativeRelayRouteEvidenceV1 {
            route_digest: [id; 32],
            circuit: [id; 16],
            origin: "component-origin".into(),
            path: vec!["component-relay".into(), "component-target".into()],
            local_peer: "component-relay".into(),
            role: "relay",
            cursor: 0,
            first_hop: VerifiedCarrierRoute::from_binding(&binding),
        }
    }

    #[test]
    fn relay_observer_counts_only_completed_writes_and_never_object_done() {
        let observer = NativeTransferDiagnosticsV1::new();
        observer.relay_authenticated(route(1), false);
        observer.relay_ready([1; 32]);
        observer.relay_forwarded([1; 32], false, 100, true);
        observer.relay_forwarded([1; 32], false, 500, false);
        observer.relay_forwarded([1; 32], true, 20, true);
        observer.relay_dropped([1; 32]);
        let snapshot = observer.snapshot().unwrap();
        let record = &snapshot.relay_circuits[0];
        assert_eq!(
            (record.forwarded[0].packets, record.forwarded[0].bytes),
            (1, 100)
        );
        assert_eq!(
            (record.forwarded[1].packets, record.forwarded[1].bytes),
            (1, 20)
        );
        assert_eq!(record.failed_writes, [1, 0]);
        assert!(record.downstream_ready_us.is_some() && record.stream_dropped_us.is_some());
        assert!(record.native_session.is_none() && snapshot.objects.is_empty());
    }

    #[test]
    fn relay_observer_bounds_latest_circuits_and_preserves_missing_evidence() {
        let observer = NativeTransferDiagnosticsV1::new();
        for id in 1..=66 {
            observer.relay_authenticated(route(id), false);
        }
        observer.relay_forwarded([1; 32], false, 100, true);
        observer.relay_authenticated(route(66), true);
        observer.relay_native_session([66; 32], [7; 16]);
        let snapshot = observer.snapshot().unwrap();
        assert_eq!(snapshot.relay_circuits.len(), 64);
        assert_eq!(snapshot.relay_evicted_records, 2);
        assert_eq!(snapshot.relay_omitted_observations, 2);
        let last = snapshot.relay_circuits.last().unwrap();
        assert!(last.downstream_ready_us.is_none());
        assert_eq!(last.native_session, Some([7; 16]));
    }

    #[test]
    fn relay_observer_contention_drops_diagnostic_not_io() {
        let observer = NativeTransferDiagnosticsV1::new();
        observer.relay_authenticated(route(1), false);
        let held = observer.state.lock().unwrap();
        observer.relay_ready([1; 32]);
        observer.relay_forwarded([1; 32], false, 100, true);
        drop(held);
        let snapshot = observer.snapshot().unwrap();
        assert_eq!(snapshot.contended_observations, 2);
        assert_eq!(snapshot.relay_circuits[0].forwarded[0].packets, 0);
        assert!(snapshot.relay_circuits[0].downstream_ready_us.is_none());
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct NativeRelayEvidenceV1 {
    pub route: NativeRelayRouteEvidenceV1,
    pub authenticated_us: u64,
    pub last_progress_us: u64,
    pub downstream_ready_us: Option<u64>,
    /// Actual independently authenticated next carrier at an intermediate hop.
    pub downstream: Option<VerifiedCarrierRoute>,
    /// Only origin/final endpoint can observe the native end-to-end session.
    pub native_session: Option<[u8; 16]>,
    /// 0=origin towards target, 1=target towards origin. These count only
    /// completed local downstream writes of opaque frames, including E2E
    /// handshake/feedback; never physical bytes, object count or final receipt.
    pub forwarded: [PacketBytes; 2],
    /// Returned write errors only; cancellation/read errors are not counted.
    pub failed_writes: [u64; 2],
    pub stream_dropped_us: Option<u64>,
}

#[cfg(feature = "iroh-transport")]
impl NativeTransferDiagnosticsV1 {
    pub(crate) fn relay_authenticated(&self, route: NativeRelayRouteEvidenceV1, ready: bool) {
        let Ok(mut state) = self.lock_state() else {
            return;
        };
        let snapshot = &mut state.snapshot;
        if snapshot
            .relay_circuits
            .iter()
            .any(|r| r.route.route_digest == route.route_digest)
        {
            snapshot.relay_omitted_observations =
                snapshot.relay_omitted_observations.saturating_add(1);
            return;
        }
        if snapshot.relay_circuits.len() == 64 {
            snapshot.relay_circuits.remove(0);
            snapshot.relay_evicted_records = snapshot.relay_evicted_records.saturating_add(1);
        }
        let at = self.micros(Instant::now());
        snapshot.relay_circuits.push(NativeRelayEvidenceV1 {
            route,
            authenticated_us: at,
            last_progress_us: at,
            downstream_ready_us: ready.then_some(at),
            downstream: None,
            native_session: None,
            forwarded: [PacketBytes::default(); 2],
            failed_writes: [0; 2],
            stream_dropped_us: None,
        });
    }

    fn relay_progress(
        &self,
        digest: [u8; 32],
        update: impl FnOnce(&mut NativeRelayEvidenceV1, u64),
    ) {
        let Ok(mut state) = self.lock_state() else {
            return;
        };
        let at = self.micros(Instant::now());
        let snapshot = &mut state.snapshot;
        if let Some(record) = snapshot
            .relay_circuits
            .iter_mut()
            .find(|r| r.route.route_digest == digest)
        {
            record.last_progress_us = at;
            update(record, at);
        } else {
            snapshot.relay_omitted_observations =
                snapshot.relay_omitted_observations.saturating_add(1);
        }
    }
    pub(crate) fn relay_ready(&self, digest: [u8; 32]) {
        self.relay_progress(digest, |r, at| {
            r.downstream_ready_us.get_or_insert(at);
        });
    }
    pub(crate) fn relay_native_session(&self, digest: [u8; 32], session: [u8; 16]) {
        self.relay_progress(digest, |r, _| {
            r.native_session = Some(session);
        });
    }
    pub(crate) fn relay_downstream(&self, digest: [u8; 32], route: VerifiedCarrierRoute) {
        self.relay_progress(digest, |r, _| {
            r.downstream = Some(route);
        });
    }
    pub(crate) fn relay_forwarded(
        &self,
        digest: [u8; 32],
        reverse: bool,
        bytes: usize,
        completed: bool,
    ) {
        self.relay_progress(digest, |r, _| {
            let direction = usize::from(reverse);
            if completed {
                r.forwarded[direction].record(bytes);
            } else {
                r.failed_writes[direction] = r.failed_writes[direction].saturating_add(1);
            }
        });
    }
    pub(crate) fn relay_dropped(&self, digest: [u8; 32]) {
        self.relay_progress(digest, |r, at| {
            r.stream_dropped_us = Some(at);
        });
    }
}
