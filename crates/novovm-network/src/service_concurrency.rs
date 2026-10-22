//! Local resource admission owned by the existing node/application runtime.
//! No peer authorization, timer, worker, protocol or persistent policy lives here.
use crate::novorudp::wire_budget::{NativeWireUsage, MAX_BYTES_PER_MINUTE, MIN_BYTES_PER_MINUTE};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

#[path = "service_request_rate.rs"]
mod request_rate;
pub(crate) use request_rate::{RequestRateLimitV1, RequestRateObservationV1};

const MIB: u64 = 1024 * 1024;
/// At most one native wire allowance decision per interval; observations in
/// between only report usage. Short enough that doubling reaches a capable
/// unmetered allowance within seconds of sustained refusals.
const WIRE_DECISION_INTERVAL: Duration = Duration::from_secs(2);

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
    /// Local attempt counters, not unique clients or remote service receipts.
    pub connection_admission_rejections: u64,
    pub work_admission_rejections: u64,
    pub watch_admission_rejections: u64,
    pub store_quota_rejections: u64,
    pub store_full_rejections: u64,
    /// Last local backoff observation; no request identity or retained history.
    pub execution_backoffs: u64,
    pub last_backoff_execution_us: u64,
    pub last_backoff_baseline_us: u64,
    pub last_backoff_active_work: usize,
    /// Last observation of the native UDP listener, never a carrier claim.
    pub native_peer_count: usize,
    pub native_peer_capacity: usize,
    pub native_authenticated_waiting: bool,
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
    /// Current native wire allowance and its last observed usage; zero/None
    /// until a native listener with a configured budget reports.
    pub wire_bytes_per_minute: u64,
    pub wire_usage: Option<NativeWireUsage>,
    /// Platform network-cost observation; metered until reported otherwise.
    pub network_unmetered: bool,
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
    connection_admission_rejections: u64,
    work_admission_rejections: u64,
    watch_admission_rejections: u64,
    store_quota_rejections: u64,
    store_full_rejections: u64,
    execution_backoffs: u64,
    last_backoff_execution_us: u64,
    last_backoff_baseline_us: u64,
    last_backoff_active_work: usize,
    native_peer_count: usize,
    native_peer_capacity: usize,
    native_authenticated_waiting: bool,
    resources: ResourceSnapshotV1,
    target: usize,
    demand: bool,
    fast_completions: usize,
    active: usize,
    watches: usize,
    saturation_generation: u64,
    bytes: usize,
    request_body_bytes: usize,
    transport_bytes: usize,
    service_us: u64,
    platform_managed: bool,
    refreshed: Instant,
    wire: WireState,
    request_rate: request_rate::RequestRateState,
}
/// Native wire allowance owned by the same resource controller. The listener's
/// startup budget is the metered ceiling; an unmetered network may grow beyond
/// it while every other resource stays healthy.
struct WireState {
    startup: u64,
    limit: u64,
    unmetered: bool,
    decided: Instant,
    refusals: u64,
    backoffs: u64,
    usage: Option<NativeWireUsage>,
}
pub struct ServiceConcurrencyV1 {
    state: Mutex<State>,
    work_changed: tokio::sync::watch::Sender<()>,
}
impl ServiceConcurrencyV1 {
    pub fn from_host() -> Arc<Self> {
        Self::new(ResourceSnapshotV1::from_host())
    }
    pub fn new(resources: ResourceSnapshotV1) -> Arc<Self> {
        Arc::new(Self {
            work_changed: tokio::sync::watch::channel(()).0,
            state: Mutex::new(State {
                admitted: 0,
                completed: 0,
                connection_admission_rejections: 0,
                work_admission_rejections: 0,
                watch_admission_rejections: 0,
                store_quota_rejections: 0,
                store_full_rejections: 0,
                execution_backoffs: 0,
                last_backoff_execution_us: 0,
                last_backoff_baseline_us: 0,
                last_backoff_active_work: 0,
                native_peer_count: 0,
                native_peer_capacity: 0,
                native_authenticated_waiting: false,
                resources,
                target: resources.cpu_parallelism.max(1),
                demand: false,
                fast_completions: 0,
                active: 0,
                watches: 0,
                saturation_generation: 0,
                bytes: 0,
                request_body_bytes: 0,
                transport_bytes: 0,
                service_us: 0,
                platform_managed: false,
                refreshed: Instant::now(),
                request_rate: request_rate::RequestRateState::new(),
                wire: WireState {
                    startup: 0,
                    limit: 0,
                    unmetered: false,
                    decided: Instant::now(),
                    refusals: 0,
                    backoffs: 0,
                    usage: None,
                },
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
        self.work_changed.send_replace(());
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
    /// Platform network-cost observation. Until an unmetered network is
    /// reported, the native wire allowance never exceeds its startup value.
    pub fn observe_network_cost(&self, unmetered: bool) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.wire.unmetered = unmetered;
        if !unmetered && s.wire.startup > 0 {
            s.wire.limit = s.wire.limit.min(s.wire.startup);
        }
    }
    /// The native listener reports its configured allowance usage and receives
    /// the allowance to enforce now. At most one decision per interval: halve
    /// under platform pressure; double while the allowance itself refused
    /// traffic and execution stayed healthy. Execution backoff vetoes growth,
    /// not bandwidth: work/request admission already handles that pressure.
    /// Metered networks keep the startup value as ceiling; the floor keeps
    /// ordinary messages usable under pressure.
    pub(crate) fn observe_native_wire(&self, usage: NativeWireUsage, now: Instant) -> u64 {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let refusals = usage.send_deferred.saturating_add(usage.receive_dropped);
        s.wire.usage = Some(usage);
        if s.wire.startup == 0 {
            s.wire.startup = usage.limit;
            s.wire.limit = usage.limit;
            s.wire.decided = now;
            s.wire.refusals = refusals;
            s.wire.backoffs = s.execution_backoffs;
        }
        let ceiling = if s.wire.unmetered {
            MAX_BYTES_PER_MINUTE
        } else {
            s.wire.startup
        };
        let floor = (s.wire.startup / 8).clamp(MIN_BYTES_PER_MINUTE, ceiling);
        if now.saturating_duration_since(s.wire.decided) >= WIRE_DECISION_INTERVAL {
            let pressured = s.resources.pressure != ResourcePressureV1::Normal;
            let execution_backoff = s.execution_backoffs > s.wire.backoffs;
            if pressured {
                s.wire.limit /= 2;
            } else if !execution_backoff && refusals > s.wire.refusals {
                s.wire.limit = s.wire.limit.saturating_mul(2);
            }
            s.wire.decided = now;
            s.wire.refusals = refusals;
            s.wire.backoffs = s.execution_backoffs;
        }
        s.wire.limit = s.wire.limit.clamp(floor, ceiling);
        s.wire.limit
    }
    /// Default request policy shares this owner's resource and wire feedback.
    /// Called by the existing admission path, never a new timer or task.
    pub(crate) fn mailbox_request_rate(
        &self,
        observation: RequestRateObservationV1,
        now: Instant,
    ) -> RequestRateLimitV1 {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let limits = Self::snapshot_locked(&s);
        let signals = request_rate::RateSignals {
            pressure: s.resources.pressure,
            memory_busy: s.bytes > limits.retained_bytes.saturating_mul(3) / 4,
            execution_backoffs: s.execution_backoffs,
            wire_refusals: s
                .wire
                .usage
                .map_or(0, |v| v.send_deferred.saturating_add(v.receive_dropped)),
            service_us: s.service_us,
            work: limits.work_limit,
        };
        s.request_rate.observe(observation, signals, now)
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
        // a connection constant or a promise of throughput. Keep this budget
        // tied to observed resources, not the execution-latency target: an idle
        // authenticated connection does not execute a handler. Otherwise one
        // slow handler can strand new handshakes waiting for work completions
        // that those not-yet-connected clients cannot produce.
        let frame = crate::mailbox_structured::BULK_LIMITS.wire_bytes;
        let connection_memory = (bytes / frame.saturating_mul(4)).max(1);
        let connection_limit = ceiling
            .saturating_mul(2)
            .saturating_add(2)
            .min(connection_memory);
        let mut snapshot = ServiceConcurrencySnapshotV1 {
            resources: s.resources,
            admitted_work: s.admitted,
            completed_work: s.completed,
            connection_admission_rejections: s.connection_admission_rejections,
            work_admission_rejections: s.work_admission_rejections,
            watch_admission_rejections: s.watch_admission_rejections,
            store_quota_rejections: s.store_quota_rejections,
            store_full_rejections: s.store_full_rejections,
            execution_backoffs: s.execution_backoffs,
            last_backoff_execution_us: s.last_backoff_execution_us,
            last_backoff_baseline_us: s.last_backoff_baseline_us,
            last_backoff_active_work: s.last_backoff_active_work,
            native_peer_count: s.native_peer_count,
            native_peer_capacity: s.native_peer_capacity,
            native_authenticated_waiting: s.native_authenticated_waiting,
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
            wire_bytes_per_minute: s.wire.limit,
            wire_usage: s.wire.usage,
            network_unmetered: s.wire.unmetered,
        };
        // An admitted inbound connection needs its own idle subscription.
        // Waiting consumes retained memory, not a CPU execution slot. Match
        // the shared inbound budget; acquire_work still limits all Watch
        // reservations to half the retained pool so data/receipts can run.
        snapshot.waiting_watch_limit = snapshot.inbound_connections();
        snapshot
    }
    pub(crate) fn subscribe_work_capacity(&self) -> tokio::sync::watch::Receiver<()> {
        self.work_changed.subscribe()
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
    pub(crate) fn observe_native_admission(&self, peers: usize, capacity: usize, waiting: bool) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.native_peer_count = peers;
        s.native_peer_capacity = capacity;
        s.native_authenticated_waiting = waiting;
        if waiting {
            // Only a verified, unexpired handshake supplies this demand. It
            // still cannot grow capacity without measured useful completions.
            s.demand = true;
        }
    }
    pub(crate) fn note_watch_admission_rejection(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.watch_admission_rejections = s.watch_admission_rejections.saturating_add(1);
    }
    pub fn try_acquire_work(self: &Arc<Self>, bytes: usize) -> Option<ServiceWorkPermitV1> {
        self.acquire_work(bytes, false, true)
    }
    pub fn try_acquire_watch(self: &Arc<Self>, bytes: usize) -> Option<ServiceWorkPermitV1> {
        self.acquire_work(bytes, true, true)
    }
    pub(crate) fn try_acquire_ready_work(
        self: &Arc<Self>,
        bytes: usize,
    ) -> Option<ServiceWorkPermitV1> {
        self.acquire_work(bytes, false, false)
    }
    pub(crate) fn note_work_admission_rejection(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.work_admission_rejections = s.work_admission_rejections.saturating_add(1);
    }
    fn acquire_work(
        self: &Arc<Self>,
        bytes: usize,
        waiting: bool,
        count_rejection: bool,
    ) -> Option<ServiceWorkPermitV1> {
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
            if count_rejection {
                if waiting {
                    s.watch_admission_rejections = s.watch_admission_rejections.saturating_add(1);
                } else {
                    s.work_admission_rejections = s.work_admission_rejections.saturating_add(1);
                }
            }
            s.demand = true;
            return None;
        }
        s.active += 1;
        s.admitted = s.admitted.saturating_add(1);
        if waiting {
            s.watches += 1;
        }
        let admitted_at_capacity =
            !waiting && s.active.saturating_sub(s.watches) >= limit.work_limit;
        if admitted_at_capacity {
            // Local evidence only, not an identity/authorization sequence. A
            // generation lets earlier work remember overlap even if all other
            // handlers finish before that work's result is observed.
            s.saturation_generation = s.saturation_generation.wrapping_add(1);
        }
        s.bytes += bytes;
        Some(ServiceWorkPermitV1 {
            owner: self.clone(),
            bytes,
            started: Instant::now(),
            timing: ServiceExecutionTimingV1::for_owner(self),
            complete: false,
            waiting,
            saturation_generation: s.saturation_generation,
            admitted_at_capacity,
        })
    }
    fn complete_work(
        &self,
        elapsed: Duration,
        waiting: bool,
        saturation_generation: u64,
        admitted_at_capacity: bool,
        finished_generation: Option<u64>,
    ) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let observed_saturation = admitted_at_capacity
            || saturation_generation != finished_generation.unwrap_or(s.saturation_generation);
        Self::finish_execution(&mut s, elapsed, true, waiting, observed_saturation);
        self.work_changed.send_replace(());
    }
    fn release_work(&self, bytes: usize, unfinished: Option<bool>) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(waiting) = unfinished {
            Self::finish_execution(&mut s, Duration::ZERO, false, waiting, false);
        }
        s.bytes = s.bytes.saturating_sub(bytes);
        self.work_changed.send_replace(());
    }
    fn finish_execution(
        s: &mut State,
        elapsed: Duration,
        success: bool,
        waiting: bool,
        observed_saturation: bool,
    ) {
        let active_work = s.active.saturating_sub(s.watches);
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
          // Heterogeneous requests have different useful work: a single large
          // commit being slower than metadata does not prove concurrency pressure.
          // Only a saturated interval supplies latency-based backoff evidence;
          // actual platform resource pressure remains independently authoritative.
        if (slowed && observed_saturation) || s.resources.pressure != ResourcePressureV1::Normal {
            s.execution_backoffs = s.execution_backoffs.saturating_add(1);
            s.last_backoff_execution_us = us;
            s.last_backoff_baseline_us = s.service_us;
            s.last_backoff_active_work = active_work;
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
impl ServiceRequestBodyPermitV1 {
    /// Transfer already-accounted retained bytes into execution. Waiting itself
    /// is not an execution sample or rejection. Shrink never admits new work
    /// above the current budget; existing reservations survive until dropped.
    pub(crate) fn try_start_work(&mut self) -> Option<ServiceWorkPermitV1> {
        let mut s = self.owner.state.lock().unwrap_or_else(|e| e.into_inner());
        let limit = ServiceConcurrencyV1::snapshot_locked(&s);
        if self.bytes == 0
            || s.active.saturating_sub(s.watches) >= limit.work_limit
            || s.bytes > limit.retained_bytes
        {
            s.demand = true;
            return None;
        }
        let bytes = std::mem::take(&mut self.bytes);
        s.request_body_bytes = s.request_body_bytes.saturating_sub(bytes);
        s.active += 1;
        s.admitted = s.admitted.saturating_add(1);
        let admitted_at_capacity = s.active.saturating_sub(s.watches) >= limit.work_limit;
        if admitted_at_capacity {
            s.saturation_generation = s.saturation_generation.wrapping_add(1);
        }
        Some(ServiceWorkPermitV1 {
            owner: self.owner.clone(),
            bytes,
            started: Instant::now(),
            timing: ServiceExecutionTimingV1::for_owner(&self.owner),
            complete: false,
            waiting: false,
            saturation_generation: s.saturation_generation,
            admitted_at_capacity,
        })
    }
}
impl Drop for ServiceRequestBodyPermitV1 {
    fn drop(&mut self) {
        let mut s = self.owner.state.lock().unwrap_or_else(|e| e.into_inner());
        s.bytes = s.bytes.saturating_sub(self.bytes);
        s.request_body_bytes = s.request_body_bytes.saturating_sub(self.bytes);
        self.owner.work_changed.send_replace(());
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
/// Per-work local timing only. Admission, transport and memory ownership stay
/// with the existing permit. Explicit policy/queue waits are excluded from
/// execution feedback; this is service elapsed time, not a CPU-time counter.
#[derive(Clone, Default)]
pub(crate) struct ServiceExecutionTimingV1(Arc<Mutex<ExecutionTiming>>);
#[derive(Default)]
struct ExecutionTiming {
    owner: std::sync::Weak<ServiceConcurrencyV1>,
    excluded: Duration,
    elapsed: Option<Duration>,
    finished_saturation_generation: Option<u64>,
}
impl ServiceExecutionTimingV1 {
    fn for_owner(owner: &Arc<ServiceConcurrencyV1>) -> Self {
        Self(Arc::new(Mutex::new(ExecutionTiming {
            owner: Arc::downgrade(owner),
            ..Default::default()
        })))
    }
    pub(crate) fn note_store_rejection(&self, disk_full: bool) {
        let owner = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .owner
            .upgrade();
        if let Some(owner) = owner {
            let mut s = owner.state.lock().unwrap_or_else(|e| e.into_inner());
            let counter = if disk_full {
                &mut s.store_full_rejections
            } else {
                &mut s.store_quota_rejections
            };
            *counter = counter.saturating_add(1);
        }
    }
    pub(crate) fn wait(&self) -> ServiceExecutionWaitV1 {
        ServiceExecutionWaitV1 {
            timing: self.clone(),
            started: Instant::now(),
        }
    }
    pub(crate) async fn measure<T>(&self, work: impl std::future::Future<Output = T>) -> T {
        // Begin on first poll, not when the owner queues the future. Finish
        // before the owner observes the result, not at a later network tick.
        let started = Instant::now();
        let result = work.await;
        self.finish(started.elapsed());
        result
    }
    fn finish(&self, wall: Duration) {
        let owner = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .owner
            .upgrade();
        let finished_saturation_generation = owner.map(|owner| {
            owner
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .saturation_generation
        });
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.elapsed = Some(wall.saturating_sub(state.excluded));
        state.finished_saturation_generation = finished_saturation_generation;
    }
    fn elapsed(&self) -> Option<Duration> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).elapsed
    }
    fn finished_saturation_generation(&self) -> Option<u64> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .finished_saturation_generation
    }
}
pub(crate) struct ServiceExecutionWaitV1 {
    timing: ServiceExecutionTimingV1,
    started: Instant,
}
impl Drop for ServiceExecutionWaitV1 {
    fn drop(&mut self) {
        let mut state = self.timing.0.lock().unwrap_or_else(|e| e.into_inner());
        state.excluded = state.excluded.saturating_add(self.started.elapsed());
    }
}

pub struct ServiceWorkPermitV1 {
    owner: Arc<ServiceConcurrencyV1>,
    bytes: usize,
    started: Instant,
    timing: ServiceExecutionTimingV1,
    complete: bool,
    waiting: bool,
    saturation_generation: u64,
    admitted_at_capacity: bool,
}
impl ServiceWorkPermitV1 {
    pub(crate) fn execution_timing(&self) -> ServiceExecutionTimingV1 {
        self.timing.clone()
    }
    /// Execution is finished; waiting for the reply's transport ACK retains
    /// bytes only, not CPU/Watch capacity or a remote-latency service sample.
    pub fn completed(&mut self) {
        if !self.complete {
            self.owner.complete_work(
                self.timing
                    .elapsed()
                    .unwrap_or_else(|| self.started.elapsed()),
                self.waiting,
                self.saturation_generation,
                self.admitted_at_capacity,
                self.timing.finished_saturation_generation(),
            );
            self.complete = true;
        }
    }
}
impl Drop for ServiceWorkPermitV1 {
    fn drop(&mut self) {
        self.owner
            .release_work(self.bytes, (!self.complete).then_some(self.waiting));
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
                let mut s = c.state.lock().unwrap_or_else(|e| e.into_inner());
                s.demand = true;
                if matches!(
                    self.kind,
                    AdmissionKind::Connections | AdmissionKind::Inbound
                ) {
                    s.connection_admission_rejections =
                        s.connection_admission_rejections.saturating_add(1);
                }
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
    fn slow_execution_does_not_shrink_authenticated_connection_capacity() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let initial = c.snapshot();
        let gate = AdaptiveAdmissionV1::new(
            1,
            Some(c.clone()),
            AdmissionKind::Inbound,
            usize::from(u16::MAX),
        );
        let held: Vec<_> = (0..initial.inbound_connections())
            .map(|_| gate.clone().try_acquire_owned().unwrap())
            .collect();
        assert!(gate.clone().try_acquire_owned().is_err());
        let watch = c.try_acquire_watch(1024).unwrap();
        c.state.lock().unwrap().service_us = 10_000;
        let mut slow = c.try_acquire_work(4096).unwrap();
        let concurrent: Vec<_> = (1..initial.work_limit)
            .map(|_| c.try_acquire_work(4096).unwrap())
            .collect();
        drop(concurrent);
        slow.execution_timing().finish(Duration::from_millis(1010));
        slow.completed();
        let after = c.snapshot();
        assert_eq!(after.work_limit, initial.work_limit / 2);
        assert_eq!(after.connection_limit, initial.connection_limit);
        assert_eq!(gate.limit(), initial.inbound_connections());
        assert_eq!(after.execution_backoffs, 1);
        assert_eq!(after.last_backoff_execution_us, 1_010_000);
        assert_eq!(after.last_backoff_baseline_us, 10_000);
        assert_eq!(after.last_backoff_active_work, 1, "idle Watch is excluded");
        slow.completed();
        assert_eq!(c.snapshot().execution_backoffs, 1, "no duplicate feedback");
        drop((held, slow, watch));
        let restored: Vec<_> = (0..initial.inbound_connections())
            .map(|_| gate.clone().try_acquire_owned().unwrap())
            .collect();
        assert!(gate.clone().try_acquire_owned().is_err());
        drop(restored);
    }
    #[test]
    fn connection_capacity_tracks_resources_and_preserves_active_leases() {
        let normal = resources(ResourcePressureV1::Normal);
        let c = ServiceConcurrencyV1::new(normal);
        let initial = c.snapshot();
        let gate = AdaptiveAdmissionV1::new(
            1,
            Some(c.clone()),
            AdmissionKind::Inbound,
            usize::from(u16::MAX),
        );
        let held: Vec<_> = (0..initial.inbound_connections())
            .map(|_| gate.clone().try_acquire_owned().unwrap())
            .collect();
        c.update_resources(ResourceSnapshotV1 {
            pressure: ResourcePressureV1::Moderate,
            ..normal
        });
        let moderate = c.snapshot();
        assert!(moderate.connection_limit < initial.connection_limit);
        assert!(gate.clone().try_acquire_owned().is_err());
        assert_eq!(held.len(), initial.inbound_connections());
        c.update_resources(ResourceSnapshotV1 {
            available_memory_bytes: 32 * MIB,
            ..normal
        });
        let scarce = c.snapshot();
        let memory_bound = (32 * MIB as usize
            / 4
            / crate::mailbox_structured::BULK_LIMITS
                .wire_bytes
                .saturating_mul(4))
        .max(1);
        assert!(scarce.connection_limit <= memory_bound);
        assert!(scarce.connection_limit < moderate.connection_limit);
        assert!(gate.clone().try_acquire_owned().is_err());
        drop(held);
        let bounded: Vec<_> = (0..gate.limit())
            .map(|_| gate.clone().try_acquire_owned().unwrap())
            .collect();
        assert!(gate.clone().try_acquire_owned().is_err());
        drop(bounded);
        c.update_resources(normal);
        assert_eq!(gate.limit(), initial.inbound_connections());
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
        assert_eq!(limits.inbound_connections(), 8);
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
        let data = c
            .try_acquire_work(4096)
            .expect("shrink leaves data progress");
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
        let mut replies: Vec<_> = (0..2).map(|_| c.try_acquire_work(4096).unwrap()).collect();
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
    fn policy_and_queue_waits_do_not_reduce_execution_feedback() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let before = c.snapshot().work_limit;
        c.state.lock().unwrap().service_us = 10_000;
        let mut permit = c.try_acquire_work(4096).unwrap();
        let timing = permit.execution_timing();
        // Fixed phase accounting, independent of wall-clock scheduler speed:
        // 600 ms token wait + 400 ms store/worker queue + 10 ms useful work.
        timing.0.lock().unwrap().excluded = Duration::from_secs(1);
        timing.finish(Duration::from_millis(1010));
        permit.started = Instant::now() - Duration::from_secs(30);
        permit.completed();
        assert_eq!(c.snapshot().work_limit, before);
        assert_eq!(c.state.lock().unwrap().service_us, 10_000);
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().reserved_bytes, 4096, "reply still awaits ACK");
        drop(permit);
        assert_eq!(c.snapshot().reserved_bytes, 0);

        // Equally long actual service under real concurrent saturation still
        // produces backoff. A heterogeneous single request alone is not load.
        let mut slow = c.try_acquire_work(4096).unwrap();
        let concurrent: Vec<_> = (1..before)
            .map(|_| c.try_acquire_work(4096).unwrap())
            .collect();
        slow.execution_timing().finish(Duration::from_millis(1010));
        slow.completed();
        assert_eq!(c.snapshot().work_limit, (before / 2).max(1));
        drop(concurrent);
    }

    #[test]
    fn isolated_slow_request_does_not_infer_concurrency_pressure() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let initial = c.snapshot();
        c.state.lock().unwrap().service_us = 14_920;
        let mut work = c.try_acquire_work(4096).unwrap();
        work.execution_timing()
            .finish(Duration::from_micros(628_899));
        work.completed();
        assert_eq!(c.snapshot().work_limit, initial.work_limit);
        assert_eq!(c.snapshot().execution_backoffs, 0);
        assert_eq!(c.snapshot().completed_work, 1);
        assert_eq!(c.snapshot().reserved_bytes, 4096);
        drop(work);
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }

    #[test]
    fn completed_overlap_remains_saturation_evidence_for_slow_work() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let initial = c.snapshot();
        c.state.lock().unwrap().service_us = 10_000;
        let mut slow = c.try_acquire_work(4096).unwrap();
        let mut concurrent: Vec<_> = (1..initial.work_limit)
            .map(|_| c.try_acquire_work(4096).unwrap())
            .collect();
        for work in &mut concurrent {
            work.execution_timing().finish(Duration::from_millis(10));
            work.completed();
        }
        assert_eq!(c.snapshot().active_work, 1);
        slow.execution_timing()
            .finish(Duration::from_micros(628_899));
        slow.completed();
        assert_eq!(c.snapshot().work_limit, initial.work_limit / 2);
        assert_eq!(c.snapshot().execution_backoffs, 1);
        assert_eq!(c.snapshot().last_backoff_active_work, 1);
        drop((slow, concurrent));
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }

    #[test]
    fn saturation_after_future_completion_does_not_change_its_feedback() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let initial = c.snapshot();
        c.state.lock().unwrap().service_us = 14_920;
        let mut finished = c.try_acquire_work(4096).unwrap();
        finished
            .execution_timing()
            .finish(Duration::from_micros(628_899));
        // Owner observation is delayed. Later admissions cannot retroactively
        // turn the already-finished unsaturated operation into load evidence.
        let later: Vec<_> = (1..initial.work_limit)
            .map(|_| c.try_acquire_work(4096).unwrap())
            .collect();
        finished.completed();
        assert_eq!(c.snapshot().work_limit, initial.work_limit);
        assert_eq!(c.snapshot().execution_backoffs, 0);
        drop((finished, later));
    }

    #[test]
    fn platform_pressure_backs_off_without_saturated_or_slow_work() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let mut work = c.try_acquire_work(4096).unwrap();
        c.update_resources(resources(ResourcePressureV1::Moderate));
        let before = c.snapshot().work_limit;
        work.execution_timing().finish(Duration::from_millis(1));
        work.completed();
        assert_eq!(c.snapshot().work_limit, before / 2);
        assert_eq!(c.snapshot().execution_backoffs, 1);
        drop(work);
        assert_eq!(c.snapshot().reserved_bytes, 0);
    }

    #[test]
    fn waiting_work_keeps_admission_memory_and_cancellation_semantics() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let before = c.snapshot().work_limit;
        let permits: Vec<_> = (0..before)
            .map(|_| c.try_acquire_work(4096).unwrap())
            .collect();
        let waiting = permits[0].execution_timing().wait();
        assert!(c.try_acquire_work(1).is_none(), "waiting is still bounded");
        assert_eq!(c.snapshot().reserved_bytes, before * 4096);
        drop(waiting);
        drop(permits);
        assert_eq!(c.snapshot().active_work, 0);
        assert_eq!(c.snapshot().reserved_bytes, 0);
        assert_eq!(c.snapshot().completed_work, 0, "cancel is not useful work");
        assert_eq!(c.snapshot().work_limit, before);
    }

    #[tokio::test]
    async fn execution_measurement_finishes_before_owner_observation() {
        let timing = ServiceExecutionTimingV1::default();
        assert_eq!(timing.measure(async { 7 }).await, 7);
        let measured = timing.elapsed().expect("future result fixes its sample");
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let mut permit = c.try_acquire_work(4096).unwrap();
        permit.timing = timing;
        permit.started = Instant::now() - Duration::from_secs(30);
        permit.completed();
        assert_eq!(
            c.state.lock().unwrap().service_us,
            measured.as_micros() as u64
        );
    }

    #[test]
    fn rejection_counters_separate_admission_and_store_causes() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let limits = c.snapshot();
        assert!(c.try_acquire_work(limits.retained_bytes + 1).is_none());
        assert!(c.try_acquire_watch(limits.retained_bytes / 2 + 1).is_none());
        let gate = AdaptiveAdmissionV1::new(1, Some(c.clone()), AdmissionKind::Inbound, 1);
        let held = gate.clone().try_acquire_owned().unwrap();
        assert!(gate.try_acquire_owned().is_err());
        let permit = c.try_acquire_work(4096).unwrap();
        permit.execution_timing().note_store_rejection(false);
        permit.execution_timing().note_store_rejection(true);
        let snapshot = c.snapshot();
        assert_eq!(snapshot.connection_admission_rejections, 1);
        assert_eq!(snapshot.work_admission_rejections, 1);
        assert_eq!(snapshot.watch_admission_rejections, 1);
        assert_eq!(snapshot.store_quota_rejections, 1);
        assert_eq!(snapshot.store_full_rejections, 1);
        assert_eq!(snapshot.completed_work, 0);
        drop((held, permit));
    }

    #[test]
    fn authenticated_udp_demand_requires_real_work_before_execution_growth() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let old = c.snapshot();
        c.observe_native_admission(old.inbound_connections(), old.inbound_connections(), true);
        assert_eq!(c.snapshot().work_limit, old.work_limit);
        assert!(c.snapshot().native_authenticated_waiting);
        for _ in 0..old.work_limit.max(2) {
            let mut p = c.try_acquire_work(4096).unwrap();
            p.execution_timing().finish(Duration::from_millis(1));
            p.completed();
        }
        assert!(c.snapshot().work_limit > old.work_limit);
        c.observe_native_admission(
            old.inbound_connections() + 1,
            c.snapshot().inbound_connections(),
            false,
        );
        assert!(!c.snapshot().native_authenticated_waiting);
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

    #[test]
    fn queued_body_is_accounted_without_execution_or_false_rejection() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let initial = c.snapshot();
        let running: Vec<_> = (0..initial.work_limit)
            .map(|_| c.try_acquire_work(1).unwrap())
            .collect();
        let mut queued = c.try_reserve_request_body(4096).unwrap();
        let reserved = c.snapshot().reserved_bytes;
        for _ in 0..8 {
            assert!(queued.try_start_work().is_none());
        }
        assert_eq!(c.snapshot().reserved_bytes, reserved);
        assert_eq!(c.snapshot().work_admission_rejections, 0);
        assert_eq!(c.snapshot().completed_work, 0);
        drop(running);
        let work = queued.try_start_work().unwrap();
        assert!(
            queued.try_start_work().is_none(),
            "one reservation transfers only once"
        );
        drop(queued);
        assert_eq!(c.snapshot().reserved_bytes, 4096);
        assert_eq!(c.snapshot().active_work, 1);
        drop(work);
        assert_eq!(c.snapshot().reserved_bytes, 0);
        assert_eq!(c.snapshot().work_limit, initial.work_limit);
    }
    #[test]
    fn queued_large_body_preserves_small_work_and_survives_shrink_without_new_admission() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let initial = c.snapshot();
        let mut queued = c
            .try_reserve_request_body(initial.retained_bytes / 2)
            .unwrap();
        assert!(c.try_reserve_request_body(1).is_none());
        let small = c.try_acquire_work(1024).unwrap();
        drop(small);
        c.update_resources(ResourceSnapshotV1 {
            available_memory_bytes: 128 * MIB,
            ..resources(ResourcePressureV1::Severe)
        });
        assert!(queued.try_start_work().is_none());
        assert_eq!(c.snapshot().active_work, 0);
        drop(queued);
        assert_eq!(c.snapshot().reserved_bytes, 0);
        assert!(c.try_acquire_work(1024).is_some());
    }

    #[test]
    fn execution_backoff_vetoes_wire_growth_without_halving_bandwidth() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        c.observe_network_cost(true);
        let start = Instant::now();
        let usage = |refused| NativeWireUsage {
            limit: 64 * MIB,
            send_deferred: refused,
            ..NativeWireUsage::default()
        };
        assert_eq!(c.observe_native_wire(usage(0), start), 64 * MIB);
        for i in 1..=3 {
            c.state.lock().unwrap().execution_backoffs += 1;
            // The existing work controller still owns execution pressure.
            // Repeated slow execution must not manufacture a bandwidth cut.
            assert_eq!(
                c.observe_native_wire(usage(i), start + Duration::from_secs(i * 5)),
                64 * MIB
            );
        }
        // New wire refusals with recovered execution can grow the allowance.
        assert_eq!(
            c.observe_native_wire(usage(4), start + Duration::from_secs(20)),
            128 * MIB
        );
        // Real platform pressure still takes effect immediately at the next
        // original decision boundary, regardless of observed request success.
        c.update_resources(resources(ResourcePressureV1::Severe));
        assert_eq!(
            c.observe_native_wire(usage(4), start + Duration::from_secs(25)),
            64 * MIB
        );
    }

    #[test]
    fn native_wire_allowance_follows_cost_pressure_and_actual_refusals() {
        let c = ServiceConcurrencyV1::new(resources(ResourcePressureV1::Normal));
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let usage = |limit: u64, refused: u64| NativeWireUsage {
            limit,
            send_deferred: refused,
            ..NativeWireUsage::default()
        };
        let startup = 64 * MIB;
        assert_eq!(c.observe_native_wire(usage(startup, 0), at(0)), startup);
        // Metered (default): refusals never raise the allowance above startup.
        assert_eq!(c.observe_native_wire(usage(startup, 9), at(5)), startup);
        // Unmetered and healthy: grows only while the allowance refuses traffic.
        c.observe_network_cost(true);
        assert_eq!(c.observe_native_wire(usage(startup, 20), at(10)), 128 * MIB);
        // No new refusal: no growth. Within one interval: no decision at all.
        assert_eq!(
            c.observe_native_wire(usage(128 * MIB, 20), at(15)),
            128 * MIB
        );
        assert_eq!(
            c.observe_native_wire(usage(128 * MIB, 21), at(16)),
            128 * MIB
        );
        assert_eq!(
            c.observe_native_wire(usage(128 * MIB, 30), at(20)),
            256 * MIB
        );
        // Platform pressure halves it every decision, down to startup / 8.
        c.update_resources(resources(ResourcePressureV1::Severe));
        let mut limit = 256 * MIB;
        for step in 0..6 {
            limit = c.observe_native_wire(usage(limit, 30), at(25 + step * 5));
        }
        assert_eq!(limit, 8 * MIB);
        // Back to metered caps immediately at the startup value.
        c.update_resources(resources(ResourcePressureV1::Normal));
        c.observe_network_cost(true);
        for step in 0..8 {
            limit = c.observe_native_wire(usage(limit, 40 + step), at(60 + step * 5));
        }
        assert!(limit > startup);
        c.observe_network_cost(false);
        assert_eq!(c.snapshot().wire_bytes_per_minute, startup);
        assert_eq!(c.observe_native_wire(usage(limit, 99), at(200)), startup);
        assert!(!c.snapshot().network_unmetered);
    }
}
