//! Local resource admission owned by the existing node/application runtime.
//! No peer authorization, timer, worker, protocol or persistent policy lives here.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

const MIB: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourcePressureV1 {
    Normal,
    Moderate,
    Severe,
}
#[derive(Clone, Copy, Debug)]
pub struct ResourceSnapshotV1 {
    pub cpu_parallelism: usize,
    pub available_memory_bytes: u64,
    pub pressure: ResourcePressureV1,
}
impl ResourceSnapshotV1 {
    pub fn from_host() -> Self {
        // Safe OS text interface. Unsupported platforms explicitly use a small
        // fallback until their platform owner supplies an actual observation.
        #[cfg(windows)]
        let memory = {
            let mut system = sysinfo::System::new();
            system.refresh_memory_specifics(sysinfo::MemoryRefreshKind::nothing().with_ram());
            let available = system.available_memory();
            if available == 0 {
                512 * MIB
            } else {
                available
            }
        };
        #[cfg(not(windows))]
        let memory = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines().find_map(|line| {
                    let rest = line.strip_prefix("MemAvailable:")?;
                    rest.split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()?
                        .checked_mul(1024)
                })
            })
            .unwrap_or(512 * MIB);
        Self {
            cpu_parallelism: std::thread::available_parallelism().map_or(1, usize::from),
            available_memory_bytes: memory,
            pressure: ResourcePressureV1::Normal,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ServiceConcurrencySnapshotV1 {
    pub resources: ResourceSnapshotV1,
    pub admitted_work: u64,
    pub completed_work: u64,
    pub connection_limit: usize,
    pub work_limit: usize,
    pub work_ceiling: usize,
    pub waiting_watch_limit: usize,
    pub multiplex_stream_limit: usize,
    pub retained_bytes: usize,
    pub transport_bytes: usize,
    pub reserved_transport_bytes: usize,
    pub active_work: usize,
    pub reserved_bytes: usize,
}
impl ServiceConcurrencySnapshotV1 {
    /// Observed capacity of the node's shared inbound service budget. The two
    /// outbound reservations let the device keep using the network while it
    /// contributes. This is a current capacity hint, not a lifetime grant.
    pub fn inbound_connections(&self) -> usize {
        self.connection_limit
            .saturating_sub(2)
            .max(1)
            .min(usize::from(u16::MAX))
    }
}
struct State {
    admitted: u64,
    completed: u64,
    resources: ResourceSnapshotV1,
    target: usize,
    demand: bool,
    fast_completions: usize,
    active: usize,
    watches: usize,
    bytes: usize,
    request_body_bytes: usize,
    transport_bytes: usize,
    service_us: u64,
    platform_managed: bool,
    refreshed: Instant,
}
pub struct ServiceConcurrencyV1 {
    state: Mutex<State>,
}
impl ServiceConcurrencyV1 {
    pub fn from_host() -> Arc<Self> {
        Self::new(ResourceSnapshotV1::from_host())
    }
    pub fn new(resources: ResourceSnapshotV1) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                admitted: 0,
                completed: 0,
                resources,
                target: resources.cpu_parallelism.max(1),
                demand: false,
                fast_completions: 0,
                active: 0,
                watches: 0,
                bytes: 0,
                request_body_bytes: 0,
                transport_bytes: 0,
                service_us: 0,
                platform_managed: false,
                refreshed: Instant::now(),
            }),
        })
    }
    pub fn update_resources(&self, resources: ResourceSnapshotV1) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let old_ceiling = Self::ceiling(&s).0;
        let pressure_cleared = s.resources.pressure != ResourcePressureV1::Normal
            && resources.pressure == ResourcePressureV1::Normal;
        s.resources = resources;
        s.platform_managed = true;
        let capacity_recovered =
            resources.pressure == ResourcePressureV1::Normal && Self::ceiling(&s).0 > old_ceiling;
        if pressure_cleared || capacity_recovered {
            // Recover the measured CPU baseline once, not on every sample.
            // Extra outbound slots otherwise cannot produce the completions
            // needed to grow a target still clamped to one by old pressure.
            s.target = s.target.max(resources.cpu_parallelism.max(1));
        }
        s.target = s.target.min(Self::ceiling(&s).0).max(1);
    }
    /// Called by existing maintenance; at most one safe host read per 5 seconds.
    /// A mobile/platform owner supplying observations remains authoritative.
    pub fn refresh_host(&self) {
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if s.platform_managed || s.refreshed.elapsed() < Duration::from_secs(5) {
                return;
            }
            s.refreshed = Instant::now();
        }
        let observed = ResourceSnapshotV1::from_host();
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !s.platform_managed {
            let old_ceiling = Self::ceiling(&s).0;
            s.resources = observed;
            let ceiling = Self::ceiling(&s).0;
            if ceiling > old_ceiling {
                s.target = s.target.max(observed.cpu_parallelism.max(1));
            }
            s.target = s.target.min(ceiling).max(1);
        }
    }
    fn ceiling(s: &State) -> (usize, usize) {
        // Leave most memory to UI/OS/other applications. These are resource
        // estimates, never claimed hardware throughput or protocol capacities.
        let divisor = match s.resources.pressure {
            ResourcePressureV1::Normal => 4,
            ResourcePressureV1::Moderate => 8,
            ResourcePressureV1::Severe => 16,
        };
        let bytes =
            usize::try_from(s.resources.available_memory_bytes / divisor).unwrap_or(usize::MAX);
        let memory_work = (bytes / (256 * 1024)).max(1);
        let cpu_work = match s.resources.pressure {
            ResourcePressureV1::Normal => s.resources.cpu_parallelism.max(1).saturating_mul(2),
            ResourcePressureV1::Moderate => s.resources.cpu_parallelism.max(1),
            ResourcePressureV1::Severe => 1,
        };
        (memory_work.min(cpu_work).max(1), bytes)
    }
    fn snapshot_locked(s: &State) -> ServiceConcurrencySnapshotV1 {
        let (ceiling, bytes) = Self::ceiling(s);
        let work = s.target.min(ceiling).max(1);
        // Connections reserve small transport state, independently of large
        // object work. The caller applies any existing advertised wire bound.
        // A connection shares bidirectional frame retention with decoded work.
        // This is a memory estimate using the actual mailbox frame bound, not
        // a connection constant or a promise of throughput.
        let frame = crate::mailbox_structured::BULK_LIMITS.wire_bytes;
        let connection_memory = (bytes / frame.saturating_mul(4)).max(1);
        let connection_limit = work
            .saturating_mul(2)
            .saturating_add(2)
            .min(connection_memory);
        let mut snapshot = ServiceConcurrencySnapshotV1 {
            resources: s.resources,
            admitted_work: s.admitted,
            completed_work: s.completed,
            connection_limit,
            work_limit: work,
            work_ceiling: ceiling,
            waiting_watch_limit: 0,
            multiplex_stream_limit: work.saturating_add(2),
            retained_bytes: bytes / 3,
            transport_bytes: bytes.saturating_sub(bytes / 3),
            reserved_transport_bytes: s.transport_bytes,
            active_work: s.active,
            reserved_bytes: s.bytes,
        };
        // An admitted inbound connection needs its own idle subscription.
        // Waiting consumes retained memory, not a CPU execution slot. Match
        // the shared inbound budget; acquire_work still limits all Watch
        // reservations to half the retained pool so data/receipts can run.
        snapshot.waiting_watch_limit = snapshot.inbound_connections();
        snapshot
    }
    pub fn snapshot(&self) -> ServiceConcurrencySnapshotV1 {
        Self::snapshot_locked(&self.state.lock().unwrap_or_else(|e| e.into_inner()))
    }
    /// A borrowed link's send/receive buffers share one aggregate owner pool.
    /// Existing leases survive shrink; no new lease can exceed the new budget.
    pub fn try_reserve_transport(self: &Arc<Self>) -> Option<ServiceTransportPermitV1> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let limits = Self::snapshot_locked(&s);
        // One extra share belongs to the owner's shared UDP listener.
        let share = limits.transport_bytes / limits.connection_limit.saturating_add(1);
        if share < 2 || share > limits.transport_bytes.saturating_sub(s.transport_bytes) {
            s.demand = true;
            return None;
        }
        s.transport_bytes += share;
        Some(ServiceTransportPermitV1 {
            owner: self.clone(),
            bytes: share,
        })
    }
    /// Pending outbound payloads share retained memory with local execution,
    /// but remote network latency is not a local CPU/service-time observation.
    /// Half the pool remains available for incoming work and device receipts.
    pub fn try_reserve_request_body(
        self: &Arc<Self>,
        bytes: usize,
    ) -> Option<ServiceRequestBodyPermitV1> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let limits = Self::snapshot_locked(&s);
        if bytes > (limits.retained_bytes / 2).saturating_sub(s.request_body_bytes)
            || bytes > limits.retained_bytes.saturating_sub(s.bytes)
        {
            s.demand = true;
            return None;
        }
        s.request_body_bytes += bytes;
        s.bytes += bytes;
        Some(ServiceRequestBodyPermitV1 {
            owner: self.clone(),
            bytes,
        })
    }
    /// Demand alone cannot increase admission; successful measured completions
    /// must follow. Failed/cancelled operations never count as useful throughput.
    pub fn note_demand(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).demand = true;
    }
    pub fn try_acquire_work(self: &Arc<Self>, bytes: usize) -> Option<ServiceWorkPermitV1> {
        self.acquire_work(bytes, false)
    }
    pub fn try_acquire_watch(self: &Arc<Self>, bytes: usize) -> Option<ServiceWorkPermitV1> {
        self.acquire_work(bytes, true)
    }
    fn acquire_work(self: &Arc<Self>, bytes: usize, waiting: bool) -> Option<ServiceWorkPermitV1> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let limit = Self::snapshot_locked(&s);
        let slots_full = if waiting {
            s.watches >= limit.waiting_watch_limit
        } else {
            s.active.saturating_sub(s.watches) >= limit.work_limit
        };
        // Watch may reserve at most half the memory, preserving delivery/receipt
        // progress even while every subscription waits for a remote sender.
        let memory_limit = if waiting {
            limit.retained_bytes / 2
        } else {
            limit.retained_bytes
        };
        if slots_full || bytes > memory_limit.saturating_sub(s.bytes) {
            s.demand = true;
            return None;
        }
        s.active += 1;
        s.admitted = s.admitted.saturating_add(1);
        if waiting {
            s.watches += 1;
        }
        s.bytes += bytes;
        Some(ServiceWorkPermitV1 {
            owner: self.clone(),
            bytes,
            started: Instant::now(),
            complete: false,
            waiting,
        })
    }
    fn complete_work(&self, elapsed: Duration, waiting: bool) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Self::finish_execution(&mut s, elapsed, true, waiting);
    }
    fn release_work(&self, bytes: usize, unfinished: Option<bool>) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(waiting) = unfinished {
            Self::finish_execution(&mut s, Duration::ZERO, false, waiting);
        }
        s.bytes = s.bytes.saturating_sub(bytes);
    }
    fn finish_execution(s: &mut State, elapsed: Duration, success: bool, waiting: bool) {
        s.active = s.active.saturating_sub(1);
        if success {
            s.completed = s.completed.saturating_add(1);
        }
        if waiting {
            s.watches = s.watches.saturating_sub(1);
            return;
        } // idle Watch duration is not service latency
        let us = elapsed.as_micros().min(u64::MAX as u128) as u64;
        let slowed = s.service_us > 0 && us > s.service_us.saturating_mul(2) && us > 100_000;
        if !success {
            return;
        } // cancellation/network failure is not CPU pressure
        if slowed || s.resources.pressure != ResourcePressureV1::Normal {
            s.target = (s.target / 2).max(1);
            s.fast_completions = 0;
        } else {
            s.fast_completions += 1;
            if s.demand && s.fast_completions >= s.target.max(2) {
                s.target = s.target.saturating_add(1).min(Self::ceiling(s).0);
                s.demand = false;
                s.fast_completions = 0;
            }
        }
        if success {
            s.service_us = if s.service_us == 0 {
                us
            } else {
                (s.service_us.saturating_mul(7).saturating_add(us)) / 8
            };
        }
    }
}
/// Byte-only reservation: releasing a remote request is not a CPU sample.
pub struct ServiceRequestBodyPermitV1 {
    owner: Arc<ServiceConcurrencyV1>,
    bytes: usize,
}
impl Drop for ServiceRequestBodyPermitV1 {
    fn drop(&mut self) {
        let mut s = self.owner.state.lock().unwrap_or_else(|e| e.into_inner());
        s.bytes = s.bytes.saturating_sub(self.bytes);
        s.request_body_bytes = s.request_body_bytes.saturating_sub(self.bytes);
    }
}
pub struct ServiceTransportPermitV1 {
    owner: Arc<ServiceConcurrencyV1>,
    bytes: usize,
}
impl ServiceTransportPermitV1 {
    pub fn bytes_per_direction(&self) -> usize {
        self.bytes / 2
    }
}
impl Drop for ServiceTransportPermitV1 {
    fn drop(&mut self) {
        let mut s = self.owner.state.lock().unwrap_or_else(|e| e.into_inner());
        s.transport_bytes = s.transport_bytes.saturating_sub(self.bytes);
    }
}
pub struct ServiceWorkPermitV1 {
    owner: Arc<ServiceConcurrencyV1>,
    bytes: usize,
    started: Instant,
    complete: bool,
    waiting: bool,
}
impl ServiceWorkPermitV1 {
    /// Execution is finished; waiting for the reply's transport ACK retains
    /// bytes only, not CPU/Watch capacity or a remote-latency service sample.
    pub fn completed(&mut self) {
        if !self.complete {
            self.owner.complete_work(self.started.elapsed(), self.waiting);
            self.complete = true;
        }
    }
}
impl Drop for ServiceWorkPermitV1 {
    fn drop(&mut self) {
        self.owner.release_work(
            self.bytes,
            (!self.complete).then_some(self.waiting),
        );
    }
}

#[derive(Clone, Copy)]
pub(crate) enum AdmissionKind {
    Connections,
    Inbound,
    Streams,
}
/// Resizing a semaphore never revokes active permits. Shrink removes only idle
/// capacity, including newly returned permits, immediately before new admission.
pub(crate) struct AdaptiveAdmissionV1 {
    semaphore: Arc<Semaphore>,
    capacity: Mutex<usize>,
    controller: Option<Arc<ServiceConcurrencyV1>>,
    kind: AdmissionKind,
    ceiling: usize,
}
impl AdaptiveAdmissionV1 {
    pub(crate) fn new(
        fixed: usize,
        controller: Option<Arc<ServiceConcurrencyV1>>,
        kind: AdmissionKind,
        ceiling: usize,
    ) -> Arc<Self> {
        let initial = controller.as_ref().map_or(fixed, |c| {
            let s = c.snapshot();
            match kind {
                AdmissionKind::Connections => s.connection_limit,
                AdmissionKind::Inbound => s.inbound_connections(),
                AdmissionKind::Streams => s.multiplex_stream_limit,
            }
            .min(ceiling)
        });
        let gate = Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(initial)),
            capacity: Mutex::new(initial),
            controller,
            kind,
            ceiling,
        });
        gate.sync();
        gate
    }
    pub(crate) fn controller(&self) -> Option<Arc<ServiceConcurrencyV1>> {
        self.controller.clone()
    }
    pub(crate) fn limit(&self) -> usize {
        self.controller.as_ref().map_or_else(
            || *self.capacity.lock().unwrap_or_else(|e| e.into_inner()),
            |c| {
                let s = c.snapshot();
                match self.kind {
                    AdmissionKind::Connections => s.connection_limit,
                    AdmissionKind::Inbound => s.inbound_connections(),
                    AdmissionKind::Streams => s.multiplex_stream_limit,
                }
                .min(self.ceiling)
            },
        )
    }
    fn sync(&self) {
        if self.controller.is_none() {
            return;
        }
        let wanted = self.limit();
        let mut capacity = self.capacity.lock().unwrap_or_else(|e| e.into_inner());
        if wanted > *capacity {
            self.semaphore.add_permits(wanted - *capacity);
            *capacity = wanted;
        } else if wanted < *capacity {
            *capacity -= self.semaphore.forget_permits(*capacity - wanted);
        }
    }
    pub(crate) fn try_acquire_owned(
        self: Arc<Self>,
    ) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        if self.controller.is_none() {
            return self.semaphore.clone().try_acquire_owned();
        }
        let wanted = self.limit();
        let mut capacity = self.capacity.lock().unwrap_or_else(|e| e.into_inner());
        if wanted > *capacity {
            self.semaphore.add_permits(wanted - *capacity);
            *capacity = wanted;
        } else if wanted < *capacity {
            *capacity -= self.semaphore.forget_permits(*capacity - wanted);
        }
        // Hold admission lock through acquire. A concurrently dropped old
        // permit can only lower active count, never let a second acquirer pass
        // the shrunken target based on the same stale active count.
        let active = capacity.saturating_sub(self.semaphore.available_permits());
        let result = if active >= wanted {
            Err(TryAcquireError::NoPermits)
        } else {
            self.semaphore.clone().try_acquire_owned()
        };
        if result.is_err() {
            if let Some(c) = &self.controller {
                c.note_demand();
            }
        }
        result
    }
    #[cfg(test)]
    pub(crate) fn available_permits(&self) -> usize {
        self.sync();
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resources(pressure: ResourcePressureV1) -> ResourceSnapshotV1 {
        ResourceSnapshotV1 {
            cpu_parallelism: 8,
            available_memory_bytes: 8 * 1024 * MIB,
            pressure,
        }
    }
    #[test]
    fn demand_requires_success_and_pressure_reduces_new_work() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let old = c.snapshot().work_limit;
        c.note_demand();
        for _ in 0..old.max(2) {
            let mut p = c.try_acquire_work(1024).unwrap();
            p.completed();
        }
        assert!(c.snapshot().work_limit > old);
        let p = c.try_acquire_work(128 * 1024 * 1024).unwrap();
        c.update_resources(ResourceSnapshotV1 {
            available_memory_bytes: 32 * MIB,
            ..resources(ResourcePressureV1::Severe)
        });
        assert!(c.try_acquire_work(128 * 1024 * 1024).is_none());
        assert_eq!(c.snapshot().active_work, 1);
        drop(p);
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }
    #[test]
    fn shrink_does_not_revoke_active_and_returned_permits_are_withheld() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let gate =
            AdaptiveAdmissionV1::new(4, Some(c.clone()), AdmissionKind::Connections, usize::MAX);
        let permits: Vec<_> = (0..6)
            .map(|_| gate.clone().try_acquire_owned().unwrap())
            .collect();
        c.update_resources(ResourceSnapshotV1 {
            available_memory_bytes: 8 * MIB,
            ..resources(ResourcePressureV1::Severe)
        });
        assert!(gate.clone().try_acquire_owned().is_err());
        drop(permits);
        let limit = gate.limit();
        let held: Vec<_> = (0..limit)
            .map(|_| gate.clone().try_acquire_owned().unwrap())
            .collect();
        assert!(gate.clone().try_acquire_owned().is_err());
        drop(held);
    }
    #[test]
    fn low_memory_watches_leave_small_requests_and_receipts_usable() {
        let c = ServiceConcurrencyV1::new(ResourceSnapshotV1 {
            available_memory_bytes: 32 * MIB,
            ..resources(ResourcePressureV1::Moderate)
        });
        let mut watches = Vec::new();
        while let Some(p) = c.try_acquire_watch(1024) {
            watches.push(p);
        }
        assert!(!watches.is_empty());
        let work = c
            .try_acquire_work(4096)
            .expect("small delivery must progress beside watches");
        assert!(c.try_acquire_work(64 * 1024 * 1024).is_none());
        drop(work);
        drop(watches);
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }
    #[test]
    fn idle_watches_cover_admitted_connections_without_consuming_data_slots() {
        let c = ServiceConcurrencyV1::new(ResourceSnapshotV1 {
            cpu_parallelism: 2,
            available_memory_bytes: 1024 * MIB,
            pressure: ResourcePressureV1::Normal,
        });
        let limits = c.snapshot();
        assert_eq!(limits.work_limit, 2);
        assert_eq!(limits.inbound_connections(), 4);
        assert_eq!(limits.waiting_watch_limit, limits.inbound_connections());
        let watches: Vec<_> = (0..limits.inbound_connections())
            .map(|_| {
                c.try_acquire_watch(2 * crate::mailbox_structured::BULK_LIMITS.wire_bytes)
                    .expect("each admitted connection can wait for a bulk notification")
            })
            .collect();
        assert!(c.try_acquire_watch(1).is_none());
        let data: Vec<_> = (0..limits.work_limit)
            .map(|_| {
                c.try_acquire_work(4096)
                    .expect("data and receipts still progress")
            })
            .collect();
        assert!(c.try_acquire_work(1).is_none());
        drop((watches, data));
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }
    #[test]
    fn watch_memory_cap_and_pressure_shrink_preserve_existing_permits() {
        let normal = ResourceSnapshotV1 {
            cpu_parallelism: 2,
            available_memory_bytes: 1024 * MIB,
            pressure: ResourcePressureV1::Normal,
        };
        let c = ServiceConcurrencyV1::new(normal);
        let limits = c.snapshot();
        let watch = c.try_acquire_watch(limits.retained_bytes / 2).unwrap();
        assert!(
            c.try_acquire_watch(1).is_none(),
            "Watch cannot consume the data half"
        );
        let data = c
            .try_acquire_work(limits.retained_bytes - limits.retained_bytes / 2)
            .expect("the other half remains usable by delivery");
        drop((watch, data));

        let mut watches: Vec<_> = (0..limits.inbound_connections())
            .map(|_| c.try_acquire_watch(1024).unwrap())
            .collect();
        c.update_resources(ResourceSnapshotV1 {
            available_memory_bytes: 128 * MIB,
            pressure: ResourcePressureV1::Severe,
            ..normal
        });
        assert_eq!(c.snapshot().waiting_watch_limit, 1);
        assert_eq!(c.snapshot().active_work, watches.len());
        assert!(c.try_acquire_watch(1).is_none());
        let data = c.try_acquire_work(4096).expect("shrink leaves data progress");
        drop(data);
        while watches.len() > c.snapshot().waiting_watch_limit {
            drop(watches.pop());
            assert!(c.try_acquire_watch(1).is_none());
        }
        drop(watches);
        let watch = c
            .try_acquire_watch(1024)
            .expect("released permits are reusable");
        assert!(c.try_acquire_watch(1).is_none());
        c.update_resources(normal);
        let recovered: Vec<_> = (1..limits.inbound_connections())
            .map(|_| {
                c.try_acquire_watch(1024)
                    .expect("resource recovery restores admission")
            })
            .collect();
        drop((watch, recovered));
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }
    #[test]
    fn cancelled_network_operation_does_not_infer_cpu_pressure() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let before = c.snapshot().work_limit;
        let permit = c.try_acquire_work(1024).unwrap();
        drop(permit);
        assert_eq!(c.snapshot().work_limit, before);
        assert_eq!(c.snapshot().completed_work, 0);
    }
    #[test]
    fn completed_replies_release_execution_before_ack_without_double_completion() {
        let c = ServiceConcurrencyV1::new(ResourceSnapshotV1 {
            cpu_parallelism: 2,
            available_memory_bytes: 1024 * MIB,
            pressure: ResourcePressureV1::Normal,
        });
        let mut replies: Vec<_> = (0..2)
            .map(|_| c.try_acquire_work(4096).unwrap())
            .collect();
        assert!(c.try_acquire_work(1).is_none());
        for permit in &mut replies {
            // Fixed local service sample; no sleep or remote RTT in this test.
            permit.started = Instant::now() - Duration::from_secs(1);
            permit.completed();
        }
        let completed = c.snapshot();
        assert_eq!(completed.active_work, 0);
        assert_eq!(completed.completed_work, 2);
        assert_eq!(completed.reserved_bytes, 8192);
        assert_eq!(
            completed.work_limit, 3,
            "local demand feedback applies before ACK"
        );
        let next: Vec<_> = (0..completed.work_limit)
            .map(|_| c.try_acquire_work(1024).unwrap())
            .collect();
        // An old response may be reported complete more than once. It cannot
        // decrement a new handler's active count or add another success sample.
        replies[0].completed();
        assert_eq!(c.snapshot().active_work, next.len());
        assert_eq!(c.snapshot().completed_work, 2);
        drop(replies);
        assert_eq!(c.snapshot().active_work, next.len());
        assert_eq!(c.snapshot().reserved_bytes, next.len() * 1024);
        drop(next); // cancellation is not successful completion
        assert_eq!(c.snapshot().completed_work, 2);
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }
    #[test]
    fn completed_reply_memory_remains_bounded_through_pressure_shrink() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let bytes = c.snapshot().retained_bytes;
        let mut reply = c.try_acquire_work(bytes).unwrap();
        reply.completed();
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().reserved_bytes, bytes);
        assert!(
            c.try_acquire_work(1).is_none(),
            "reply bytes still await ACK"
        );
        c.update_resources(ResourceSnapshotV1 {
            available_memory_bytes: 128 * MIB,
            ..resources(ResourcePressureV1::Severe)
        });
        assert!(c.snapshot().retained_bytes < bytes);
        assert!(c.try_acquire_work(1).is_none());
        reply.completed();
        assert_eq!(c.snapshot().completed_work, 1);
        drop(reply);
        assert_eq!(c.snapshot().reserved_bytes, 0);
        let next = c.try_acquire_work(4096).unwrap();
        assert!(
            c.try_acquire_work(1).is_none(),
            "new work obeys reduced CPU budget"
        );
        drop(next);
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().completed_work, 1);
    }
    #[test]
    fn watch_completion_releases_subscription_but_retains_reply_memory() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let limits = c.snapshot();
        let mut watches: Vec<_> = (0..limits.waiting_watch_limit)
            .map(|_| c.try_acquire_watch(1024).unwrap())
            .collect();
        assert!(c.try_acquire_watch(1).is_none());
        watches[0].completed();
        assert_eq!(c.snapshot().active_work, watches.len() - 1);
        assert_eq!(c.snapshot().reserved_bytes, watches.len() * 1024);
        let next = c.try_acquire_watch(1024).unwrap();
        assert!(c.try_acquire_watch(1).is_none());
        watches[0].completed();
        assert_eq!(c.snapshot().active_work, watches.len());
        assert_eq!(c.snapshot().completed_work, 1);
        assert_eq!(c.snapshot().work_limit, limits.work_limit);
        drop(watches);
        assert_eq!(c.snapshot().active_work, 1);
        assert_eq!(c.snapshot().reserved_bytes, 1024);
        drop(next);
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().reserved_bytes, 0);
        assert_eq!(c.snapshot().completed_work, 1);
    }
    #[test]
    fn aggregate_transport_lease_preserves_text_and_typical_image_memory() {
        for available_mib in [128, 256, 512] {
            let c = ServiceConcurrencyV1::new(ResourceSnapshotV1 {
                available_memory_bytes: available_mib * MIB,
                ..resources(ResourcePressureV1::Normal)
            });
            let mut held = Vec::new();
            while let Some(p) = c.try_reserve_transport() {
                assert!(p.bytes_per_direction() >= 1_700_000);
                held.push(p);
            }
            let state = c.snapshot();
            assert!(state.reserved_transport_bytes <= state.transport_bytes);
            let text = c.try_acquire_work(4096).unwrap();
            let image = c
                .try_acquire_work(2 * crate::mailbox_structured::BULK_LIMITS.wire_bytes)
                .unwrap();
            drop((text, image, held));
            assert_eq!(c.snapshot().reserved_transport_bytes, 0);
        }
    }
    #[test]
    fn outbound_payloads_share_memory_but_not_local_latency_feedback() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let initial = c.snapshot();
        let body = c
            .try_reserve_request_body(initial.retained_bytes / 2)
            .unwrap();
        assert!(c.try_reserve_request_body(1).is_none());
        assert_eq!(c.snapshot().active_work, 0);
        let incoming = c.try_acquire_work(1024).unwrap();
        drop(incoming);
        drop(body);
        assert_eq!(c.snapshot().reserved_bytes, 0);
        assert_eq!(c.snapshot().work_limit, initial.work_limit);
        assert_eq!(c.snapshot().completed_work, 0);
    }
    #[test]
    fn normal_memory_recovery_restores_baseline_without_remote_completions() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        c.update_resources(ResourceSnapshotV1 {
            available_memory_bytes: MIB,
            ..resources(ResourcePressureV1::Normal)
        });
        assert_eq!(c.snapshot().work_limit, 1);
        c.update_resources(resources(ResourcePressureV1::Normal));
        assert_eq!(c.snapshot().work_limit, 8);
    }
}
