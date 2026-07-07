# NOVOVM Network Production Overlay Audit

Date: 2026-07-03

Status: `NEXT PHASE LOCKED`

Scope:

```text
NOVOVM network layer after APFL/AOEM NativeTransfer performance signoff.
Focus shifts from raw TPS chasing to production reachability, weak-network survival,
relay overlay, no-IP identity routing, and anti-censorship transport paths.
```

## Executive Conclusion

The previous goal should be updated.

Old focus:

```text
Freeze UDP baseline.
Improve NovoRUDP repair/window profiles.
Gradually plan libp2p control plane and anti-censorship overlay.
```

New focus:

```text
Build NOVOVM Production Overlay Network v1:
identity-first addressing,
auto reachability detection,
relay data plane,
multi-hop route selection,
weak-network survival,
non-fixed-port operation,
and anti-censorship transport profiles.
```

Reason:

```text
APFL/AOEM NativeTransfer software path is no longer the immediate bottleneck.
Cross-machine tests are network-link limited.
The next product risk is reachability and network resilience, not higher local TPS.
```

## What Is Already Proven

NativeTransfer execution path:

```text
NOVORUDP
  -> APFL compact native transfer bytes
  -> SUPERVM bulk handoff
  -> aoem_execute_ops_wire_v1
  -> opcode 114 / compute.apfl_native_transfer_v1
  -> AOEM structural hot plans
  -> compact commit
  -> AOEM state surfaces / OCCC evidence
```

Signed properties:

```text
614400 native transfers closed cross-machine.
ledger/hash/signature correctness closed.
canonical materialization removed from the hot path.
structural bulk route validated.
compact commit validated.
```

Performance boundary:

```text
Low-bandwidth A/B link produced roughly 120k-150k NativeTransfer TPS.
Raw UDP and iperf3 showed A/B network bandwidth in the tens of Mbit/s.
The current cross-machine ceiling is the network path, not APFL/AOEM/NOVORUDP internals.
```

Therefore:

```text
Do not keep optimizing APFL/AOEM/NOVORUDP for this test network.
Move to production network reachability and survivability.
```

## Current Code Audit

### Existing foundations

`crates/novovm-network` already contains the correct conceptual layers:

```text
control_plane.rs
  PeerId
  Libp2pControlPlaneConfig
  CapabilityAdvertisement
  ControlPlaneRegistry
  RouteSet resolution

overlay.rs
  no-IP identity routing policy
  relay-required route decisions
  multi-hop max policy
  camouflage profile hook

routing/
  L4 local routing table
  L3 regional routing table
  route selector
  relay health / score / cooldown / runtime feedback

relay/
  relay frame model
  multi-hop relay frame model
  relay server/client semantic model

rollout-policy overlay tools
  overlay auto-profile
  relay discovery merge
  relay health refresh
```

Runtime configuration already contains production-shaped profiles:

```text
config/runtime/lifecycle/overlay.route.runtime.json
  prod
  prod_privacy
  prod-cn
  prod-eu
  prod-us
  relay buckets
  relay set size
  relay rotation
  region failover
  discovery seed policy
  source reputation policy
```

This means the architecture direction is not empty. The route policy, relay scoring,
profile switching, and no-IP route semantics are already modeled.

### Current gaps

The audit found no actual `libp2p` dependency in the workspace `Cargo.toml` files.

Current `Libp2pControlPlaneConfig` is a semantic/config model, not a live libp2p
runtime. The relay module is also a semantic relay frame model, not a deployed
network relay daemon.

Missing production runtime pieces:

```text
real libp2p control-plane runtime
real peer discovery
real identify/autonat/circuit-relay integration
real relay data-plane forwarding
real node identity key binding
real no-fixed-port endpoint advertisement
real NAT reachability probe
real weak-network route failover
real overlay route runtime attached to NOVORUDP data send
real anti-censorship transport profiles
```

The current high-performance A/B NativeTransfer path is still a direct UDP test
path with explicit IP/port configuration. That is correct for performance signoff,
but it is not yet a production anti-blocking network.

## Updated Target

Recommended Codex goal text:

```text
推进 NOVOVM Production Overlay Network v1：
冻结 APFL/AOEM NativeTransfer 性能基线，不再围绕当前低带宽 A/B 链路追 TPS；
将网络主线切换到 identity-first addressing、自动可达性探测、无固定端口运行、
relay data plane、multi-hop route selection、弱网/抗封锁 overlay 与生产级观测，
直到形成可运行的 direct->relay->multi-hop->queue fallback 网络路径。
```

English short form:

```text
Build NOVOVM Production Overlay Network v1:
identity-first control plane,
automatic reachability,
floating-port operation,
relay data plane,
multi-hop routing,
weak-network survival,
anti-censorship transport profiles,
and production observability.
```

## Architecture Boundary

Keep these boundaries locked:

```text
AOEM owns execution semantics.
SUPERVM owns host orchestration and network handoff.
NOVORUDP remains the high-throughput data plane.
Overlay control plane resolves peer identity to route sets.
Relay/multi-hop transports are fallback/availability layers, not ledger semantics.
APFL wire is unchanged.
Opcode 114 path is unchanged.
```

Do not turn overlay work into:

```text
another APFL rewrite
another AOEM rewrite
a generic VPN dependency
a fixed relay-only network
a hardcoded IP/port list
a browser-style proxy layer that changes ledger semantics
```

## Production Overlay v1 Plan

### Phase 1: Overlay Runtime Contract

Goal:

```text
Define one runtime contract that converts identity -> selected data-plane route.
```

Required output:

```text
OverlayRuntimeDecision {
  target_peer_id
  selected_path
  route_set
  direct_endpoint_candidates
  relay_candidates
  multi_hop_candidates
  reachability_class
  reason
}
```

Acceptance:

```text
direct route selected when peer is reachable
relay route selected when direct is unavailable
multi-hop selected when relay policy requires it
queue fallback selected when no route is safe
report exposes why a route was selected
```

### Phase 2: Reachability Probe and Floating Port

Goal:

```text
Stop assuming fixed IP/port reachability.
```

Required behavior:

```text
node starts on configured or random UDP port
node reports observed local endpoint
node probes target direct path
node records reachable / relay-only / lan-only / unreachable
node updates L4LocalRoutingTable
```

Acceptance:

```text
same two-machine setup works with non-fixed sender port
receiver report includes source endpoint observations
route selector can choose direct only after reachability proof
```

### Phase 3: Relay Data Plane v0

Goal:

```text
Make relay more than a policy model.
```

Required behavior:

```text
relay node listens for NOVORUDP relay frames
client wraps DATA/REPAIR/ACK frames for relay target
relay forwards opaque NOVORUDP bytes
relay does not inspect APFL/AOEM payload semantics
receiver unwraps or receives forwarded data through relay path
```

Acceptance:

```text
A -> relay -> B NativeTransfer smoke passes
final_missing = 0
ledger/hash/signature correctness closed
report shows selected_path = relay
direct path can be disabled and relay path still works
```

### Phase 4: Multi-Hop and Route Rotation

Goal:

```text
Support multi-hop relay route sets and rotation without changing NOVORUDP wire.
```

Required behavior:

```text
relay path can include 2-3 hops
TTL prevents relay loops
route tokens bind hop authorization
relay health feedback penalizes failing hops
route rotation respects overlay.route.runtime.json profile
```

Acceptance:

```text
A -> relay1 -> relay2 -> B smoke passes
failed relay triggers fallback to alternate route
health/cooldown state is persisted
```

### Phase 5: libp2p Control Plane

Goal:

```text
Use libp2p for peer identity, discovery, identify, autonat, and circuit-relay control-plane duties.
```

Important boundary:

```text
libp2p is the control plane first.
NOVORUDP remains the high-throughput data plane for APFL native transfer.
```

Required behavior:

```text
libp2p PeerId binds to NOVOVM node identity
identify advertises capabilities
DHT or configured bootstrap discovers peers
AutoNAT classifies reachability
CircuitRelay is available as fallback control/data route where appropriate
RouteSetDiscovery publishes direct/relay/multi-hop route sets
```

Acceptance:

```text
node can discover target by PeerId instead of IP address
node can choose direct or relay based on reachability
NOVORUDP APFL path still executes through AOEM opcode 114 unchanged
```

### Phase 6: Anti-Censorship Transport Profiles

Goal:

```text
Survive port blocking, UDP degradation, NAT, and unstable consumer networks.
```

Transport profiles:

```text
direct_novorudp
relay_novorudp
libp2p_circuit_relay
webrtc_relay
webtransport_cover
operator_vpn_profile
queue_only_safe_mode
```

Acceptance:

```text
operator can force secure profile
node can auto-switch profile when direct path fails
network remains correct under blocked UDP / random ports / degraded relays
```

## Auto VPN Position

Do not make VPN mandatory.

Correct positioning:

```text
VPN / WireGuard / Tailscale / operator tunnel can be one transport profile.
It must not be the core NOVOVM network architecture.
The core architecture is identity-first routing plus relay/multi-hop fallback.
```

Reason:

```text
Mandatory VPN creates centralized operational dependency.
Overlay relay and libp2p control plane preserve a more native decentralized path.
```

## Next Concrete Cut

Recommended next implementation cut:

```text
NOVOVM Overlay Runtime Decision v0
```

Scope:

```text
No APFL changes.
No AOEM changes.
No NOVORUDP frame changes.
No real libp2p dependency yet.
Only connect existing route policy models into one runtime decision/report surface.
```

Files likely involved:

```text
crates/novovm-network/src/routing/*
crates/novovm-network/src/overlay.rs
crates/novovm-network/src/control_plane.rs
config/runtime/lifecycle/overlay.route.runtime.json
```

Acceptance:

```text
cargo test -q -p novovm-network routing overlay control_plane
route decision report includes direct/relay/multihop/queue reason
no fixed IP route is accepted when no-IP profile is enforced
relay path is selected when direct reachability is unavailable
```

This cut turns the existing scattered route policy models into a single product
decision point. After that, the relay data plane can be implemented against the
same decision contract.

## Implementation Progress

### Cut 1: Overlay Runtime Decision v0

Implemented in:

```text
crates/novovm-network/src/overlay_runtime.rs
```

This cut adds one product decision surface:

```text
decide_overlay_runtime_route_v0(
  ControlPlaneRegistry,
  target_peer_id
) -> OverlayRuntimeDecision
```

It reports:

```text
selected_path = direct_novorudp | relay_novorudp | multi_hop_relay | queue_fallback
route_set
direct_endpoint_candidates
relay_candidates
multi_hop_candidates
reachability_class
reason
```

Validation:

```text
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1
cargo check -q -p novovm-network
```

### Cut 2: Reachability Probe + Floating Port v0

Implemented in:

```text
crates/novovm-network/src/reachability.rs
```

This cut adds the pure runtime model that converts probe observations into L4
reachability state:

```text
direct_probe_ack + public endpoint  -> Reachable
direct_probe_ack + private endpoint -> LanOnly
direct probe failed + relay exists  -> RelayOnly
direct probe failed + no relay      -> Unreachable
no probe evidence                   -> Unknown
```

It also detects floating-port operation:

```text
local bind port = 0
or observed remote port != configured port
```

The model updates:

```text
L4LocalRoutingTable
L4PeerRef.addr_hint
L4PeerRef.reachability
L4PeerRef.latency_ms
L4PeerRef.last_seen_unix_ms
```

Validation:

```text
cargo test -q -p novovm-network reachability -- --test-threads=1
cargo check -q -p novovm-network
```

### Cut 3: Relay Data Plane v0

Implemented in:

```text
crates/novovm-network/src/relay/data_plane.rs
```

This cut connects overlay runtime route decisions to an opaque NOVORUDP payload
forwarding model.

Boundary:

```text
No APFL changes.
No AOEM changes.
No NOVORUDP frame changes.
Relay forwards opaque bytes only.
Relay does not inspect native transfer payload semantics.
Queue fallback preserves payload but does not deliver.
```

Supported paths:

```text
OverlayRuntimeSelectedPath::DirectNovoRudp
  -> direct data-plane result, no relay hops

OverlayRuntimeSelectedPath::RelayNovoRudp
  -> MultiHopRelayFrame with one relay hop

OverlayRuntimeSelectedPath::MultiHopRelay
  -> MultiHopRelayFrame with ordered relay hop chain

OverlayRuntimeSelectedPath::QueueFallback
  -> queued=true, delivered=false
```

Validation:

```text
cargo test -q -p novovm-network relay::data_plane -- --test-threads=1
cargo check -q -p novovm-network
```

### Cut 4: NOVORUDP Relay Data-Plane Smoke v0

Implemented in:

```text
crates/novovm-network/src/relay/data_plane.rs
```

This cut proves that the relay data-plane model carries actual
`NovoRudpTransportFrameV0` encoded bytes, not just arbitrary test strings.

Boundary:

```text
Network owns network communication only.
Relay forwards opaque NOVORUDP frame bytes.
Relay does not parse APFL.
Relay does not call AOEM.
Relay does not inspect ledger, signature, hash, or NativeTransfer semantics.
No NOVORUDP frame format change.
No ACK/repair semantic change.
```

Added smoke surface:

```text
run_novorudp_relay_data_plane_smoke_v0(
  RelayServer,
  OverlayRuntimeDecision,
  NovoRudpRelaySmokeInput
) -> NovoRudpRelaySmokeReport
```

The smoke builds a real `NovoRudpTransportFrameV0`, encodes it, forwards the
encoded bytes through the selected overlay path, and decodes only the delivered
frame bytes to verify transport preservation.

Covered paths:

```text
direct_novorudp:
  delivered=true
  visited_hops=[]
  decoded frame kind/sequence/payload match

relay_novorudp:
  delivered=true
  visited_hops=[relay]
  decoded frame kind/sequence/payload match

multi_hop_relay:
  delivered=true
  visited_hops=[relay_a, relay_b]
  decoded frame kind/sequence/payload match

queue_fallback:
  delivered=false
  queued=true
  encoded NOVORUDP frame bytes preserved
```

Validation:

```text
cargo test -q -p novovm-network relay::data_plane -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1
cargo test -q -p novovm-network reachability -- --test-threads=1
cargo check -q -p novovm-network
```

### Cut 5: NOVORUDP Relay UDP Loopback Runtime Gate v0

Implemented in:

```text
crates/novovm-network/src/relay/loopback.rs
crates/novovm-network/src/relay/mod.rs
```

This cut moves from pure in-memory forwarding to an actual local UDP runtime
gate. It starts loopback UDP sockets, sends real `NovoRudpTransportFrameV0`
encoded bytes, and verifies that the target receives decodable frame bytes.

Boundary:

```text
Network communication only.
No APFL payload interpretation.
No AOEM call.
No opcode 114 call.
No ledger/hash/signature semantics.
No NOVORUDP wire change.
No ACK/repair semantic change.
No production relay daemon claim yet.
```

Runtime smoke paths:

```text
direct:
  sender UDP socket -> target UDP socket
  target decodes NovoRudpTransportFrameV0

relay:
  sender UDP socket -> relay UDP socket -> target UDP socket
  relay unwraps only relay envelope, forwards opaque NOVORUDP frame bytes

multi-hop:
  sender UDP socket -> relay-a UDP socket -> relay-b UDP socket -> target UDP socket
  relays forward opaque NOVORUDP frame bytes through ordered hops

queue_fallback:
  no socket delivery
  encoded NOVORUDP frame is preserved as queued payload
```

Report surface:

```text
NovoRudpRelayUdpLoopbackReport {
  request_id
  path
  delivered
  queued
  encoded_frame_bytes
  target_received_bytes
  queued_payload_bytes
  relay_hop_count
  relay_hops
  frame_decode_ok
  decoded_kind
  decoded_sequence
  payload_match
  queued_payload_preserved
}
```

Validation:

```text
cargo test -q -p novovm-network relay::loopback -- --test-threads=1
cargo test -q -p novovm-network relay::data_plane -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1
cargo test -q -p novovm-network reachability -- --test-threads=1
cargo check -q -p novovm-network
```

### Cut 6: Overlay Decision -> UDP Loopback Runtime Gate v0

Implemented in:

```text
crates/novovm-network/src/relay/loopback.rs
```

This cut connects the identity/route decision surface to the UDP loopback
runtime gate. The runtime path no longer has to be selected only by a manual
test parameter; it can be derived from `OverlayRuntimeDecision`.

Mapping:

```text
OverlayRuntimeSelectedPath::DirectNovoRudp
  -> RelayUdpLoopbackPath::Direct

OverlayRuntimeSelectedPath::RelayNovoRudp
  -> RelayUdpLoopbackPath::Relay

OverlayRuntimeSelectedPath::MultiHopRelay
  -> RelayUdpLoopbackPath::MultiHop

OverlayRuntimeSelectedPath::QueueFallback
  -> RelayUdpLoopbackPath::QueueFallback
```

Added surfaces:

```text
relay_udp_loopback_path_from_overlay_decision_v0(
  OverlayRuntimeDecision
) -> RelayUdpLoopbackPath

run_novorudp_overlay_relay_udp_loopback_smoke_v0(
  OverlayRuntimeDecision,
  NovoRudpRelayUdpLoopbackInput
) -> NovoRudpRelayUdpLoopbackReport
```

Acceptance covered:

```text
identity route with direct RouteSet
  -> direct UDP loopback delivery

identity route with one relay hop
  -> relay UDP loopback delivery

identity route with two relay/circuit hops
  -> multi-hop UDP loopback delivery

missing identity route
  -> queue fallback without socket delivery
```

Boundary:

```text
Overlay/control plane chooses the path.
Relay runtime only moves NOVORUDP frame bytes.
No APFL/AOEM/ledger execution is introduced.
```

Validation:

```text
cargo test -q -p novovm-network relay::loopback -- --test-threads=1
cargo check -q -p novovm-network
```

### Cut 7: Network Overlay Gate CLI v0

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
crates/novovm-node/Cargo.toml
```

This cut exposes the overlay runtime gate as an executable report-producing
binary. It builds a minimal identity/control-plane registry, derives
`OverlayRuntimeDecision`, runs the UDP loopback relay gate, and writes a JSON
report.

Boundary:

```text
Network overlay verification only.
No APFL decode.
No AOEM call.
No opcode 114.
No ledger/hash/signature execution.
No business payload interpretation.
```

Environment:

```text
NOVOVM_OVERLAY_GATE_ROUTE=direct|relay|multihop|queue
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/<route>.json
NOVOVM_OVERLAY_GATE_REQUEST_ID=<optional>
NOVOVM_OVERLAY_GATE_TARGET_PEER_ID=<optional>
NOVOVM_OVERLAY_GATE_LOCAL_PEER_ID=<optional>
```

Validation commands:

```text
NOVOVM_OVERLAY_GATE_ROUTE=direct \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/direct.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_ROUTE=relay \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/relay.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_ROUTE=multihop \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/multihop.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_ROUTE=queue \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/queue.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Observed local gate result:

```text
direct:
  accepted=true
  selected_path=DirectNovoRudp
  delivered=true
  frame_decode_ok=true
  payload_match=true
  relay_hop_count=0

relay:
  accepted=true
  selected_path=RelayNovoRudp
  delivered=true
  frame_decode_ok=true
  payload_match=true
  relay_hop_count=1

multihop:
  accepted=true
  selected_path=MultiHopRelay
  delivered=true
  frame_decode_ok=true
  payload_match=true
  relay_hop_count=2

queue:
  accepted=true
  selected_path=QueueFallback
  delivered=false
  queued=true
  queued_payload_preserved=true
```

This is the first executable `direct -> relay -> multi-hop -> queue fallback`
network-overlay gate. It is still loopback/local and not yet a production relay
daemon, but it validates the route decision and NOVORUDP frame lifecycle.

### Cut 8: Network Overlay Three-Process Runtime Gate v0

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut extends the overlay gate from self-contained loopback mode into
separate process roles:

```text
NOVOVM_OVERLAY_GATE_MODE=receiver
NOVOVM_OVERLAY_GATE_MODE=relay
NOVOVM_OVERLAY_GATE_MODE=sender
```

Boundary:

```text
Network communication only.
Receiver decodes NOVORUDP frame bytes only.
Relay decodes only the relay envelope and forwards opaque NOVORUDP frame bytes.
Sender constructs one NOVORUDP frame or relay envelope.
No APFL.
No AOEM.
No opcode 114.
No ledger/hash/signature execution.
No business payload semantics.
```

Mode behavior:

```text
receiver:
  binds NOVOVM_OVERLAY_GATE_BIND_ADDR
  receives one UDP datagram
  decodes NovoRudpTransportFrameV0
  writes receiver JSON report

relay:
  binds NOVOVM_OVERLAY_GATE_BIND_ADDR
  receives one relay envelope
  forwards to target_addr or next hop
  writes relay JSON report

sender:
  route=direct
    sends encoded NOVORUDP frame to target addr

  route=relay
    sends relay envelope to relay addr
    relay forwards NOVORUDP frame to receiver

  route=multihop
    sends relay envelope to relay-a
    relay-a forwards envelope to relay-b
    relay-b forwards NOVORUDP frame to receiver

  route=queue
    does not send socket data
    writes queued sender JSON report
```

Environment:

```text
NOVOVM_OVERLAY_GATE_MODE=receiver|relay|sender|loopback
NOVOVM_OVERLAY_GATE_ROUTE=direct|relay|multihop|queue
NOVOVM_OVERLAY_GATE_BIND_ADDR=127.0.0.1:<port>
NOVOVM_OVERLAY_GATE_TARGET_ADDR=127.0.0.1:<receiver_port>
NOVOVM_OVERLAY_GATE_RELAY_ADDR=127.0.0.1:<relay_a_port>
NOVOVM_OVERLAY_GATE_NEXT_HOP_ADDR=127.0.0.1:<relay_b_port>
NOVOVM_OVERLAY_GATE_REPORT_PATH=<json report path>
```

Observed local process-gate result:

```text
direct:
  sender accepted=true
  receiver accepted=true
  receiver frame_decode_ok=true
  receiver received_bytes=137

relay:
  sender accepted=true
  relay accepted=true
  receiver accepted=true
  relay forwarded_bytes=137
  receiver frame_decode_ok=true

multihop:
  sender accepted=true
  relay-a accepted=true
  relay-b accepted=true
  receiver accepted=true
  relay-a forwarded_to=127.0.0.1:40116
  relay-b forwarded_to=127.0.0.1:40114
  receiver frame_decode_ok=true

queue:
  sender accepted=true
  queued=true
  sent_bytes=0
  selected_path=QueueFallback
```

Validation:

```text
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
target/debug/supervm-network-overlay-gate.exe with receiver/relay/sender modes
```

This is the first runnable multi-process `direct -> relay -> multi-hop -> queue`
fallback network path. It is still local-loopback, but it validates process
boundaries and route fallback without involving business execution.

### Cut 9: Reachability-Driven Auto Route Gate v0

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut adds `NOVOVM_OVERLAY_GATE_ROUTE=auto`. The gate now evaluates the
reachability probe model, records the probe decision, derives an effective route,
then runs the same network-only NOVORUDP frame gate.

Boundary:

```text
Reachability probe drives route selection only.
Probe does not inspect APFL.
Probe does not call AOEM.
Probe does not change NOVORUDP wire.
Probe does not change ledger or execution semantics.
```

Environment:

```text
NOVOVM_OVERLAY_GATE_ROUTE=auto
NOVOVM_OVERLAY_GATE_DIRECT_PROBE_SENT=1|0
NOVOVM_OVERLAY_GATE_DIRECT_PROBE_ACK=1|0
NOVOVM_OVERLAY_GATE_RELAY_AVAILABLE=1|0
NOVOVM_OVERLAY_GATE_AUTO_RELAY_HOPS=1|2
NOVOVM_OVERLAY_GATE_CONFIGURED_ADDR_HINT=<addr>
NOVOVM_OVERLAY_GATE_OBSERVED_ADDR=<addr>
NOVOVM_OVERLAY_GATE_LOCAL_BIND_ADDR=<addr>
NOVOVM_OVERLAY_GATE_FLOATING_PORT_MODE=fixed|ephemeral
NOVOVM_OVERLAY_GATE_PROBE_RTT_MS=<optional>
```

Auto route mapping:

```text
DirectReachable or LanReachable
  -> effective_route=direct

RelayOnly and AUTO_RELAY_HOPS < 2
  -> effective_route=relay

RelayOnly and AUTO_RELAY_HOPS >= 2
  -> effective_route=multihop

Unreachable or Unknown
  -> effective_route=queue
```

Observed local auto-gate result:

```text
auto-direct:
  accepted=true
  probe=LanReachable
  reachability=LanOnly
  floating_port_active=true
  effective_route=direct
  selected_path=DirectNovoRudp
  delivered=true

auto-relay:
  accepted=true
  probe=RelayOnly
  reachability=RelayOnly
  floating_port_active=true
  effective_route=relay
  selected_path=RelayNovoRudp
  delivered=true
  relay_hop_count=1

auto-multihop:
  accepted=true
  probe=RelayOnly
  reachability=RelayOnly
  floating_port_active=true
  effective_route=multihop
  selected_path=MultiHopRelay
  delivered=true
  relay_hop_count=2

auto-queue:
  accepted=true
  probe=Unreachable
  reachability=Unreachable
  floating_port_active=true
  effective_route=queue
  selected_path=QueueFallback
  delivered=false
  queued=true
```

Validation:

```text
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

This is the first executable gate where route selection can be driven by
reachability evidence instead of a fixed route parameter.

### Cut 10: Runtime Probe/Ack Auto Route Gate v0

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut upgrades `route=auto` from simulated probe inputs to an optional real
UDP probe/ack flow.

Boundary:

```text
Probe uses NOVORUDP Endpoint frame.
Receiver replies with NOVORUDP Ack frame.
Sender uses Ack presence/absence only for route selection.
No APFL decode.
No AOEM call.
No opcode 114.
No ledger/hash/signature execution.
No NOVORUDP data wire change.
```

New sender behavior:

```text
NOVOVM_OVERLAY_GATE_ROUTE=auto
NOVOVM_OVERLAY_GATE_RUNTIME_PROBE=1

sender:
  binds local UDP socket
  sends NovoRudpTransportFrameKindV0::Endpoint to target addr
  waits for NovoRudpTransportFrameKindV0::Ack
  feeds ack result into ReachabilityProbeDecision
  selects effective route
  sends DATA direct, relay, multihop, or queue fallback
```

Receiver behavior:

```text
receiver:
  if Endpoint probe is received:
    sends Ack to source addr
    continues waiting for DATA

  if DATA is received:
    decodes NovoRudpTransportFrameV0
    writes receiver report
```

Additional environment:

```text
NOVOVM_OVERLAY_GATE_RUNTIME_PROBE=1
NOVOVM_OVERLAY_GATE_PROBE_TIMEOUT_MS=<ms>
NOVOVM_OVERLAY_GATE_RELAY_TARGET_ADDR=<relay-reachable target addr>
```

Observed local process-gate result:

```text
probe-direct:
  sender accepted=true
  runtime_probe_report.ack_received=true
  effective_route=direct
  selected_path=DirectNovoRudp
  receiver accepted=true
  receiver probe_ack_sent=true
  receiver frame_decode_ok=true

probe-relay-fallback:
  sender accepted=true
  runtime_probe_report.ack_received=false
  effective_route=relay
  selected_path=RelayNovoRudp
  relay accepted=true
  receiver accepted=true
  receiver frame_decode_ok=true

probe-queue-fallback:
  sender accepted=true
  runtime_probe_report.ack_received=false
  effective_route=queue
  selected_path=QueueFallback
  queued=true
  sent_bytes=0
```

Important design note:

```text
NOVOVM_OVERLAY_GATE_TARGET_ADDR is the direct probe target.
NOVOVM_OVERLAY_GATE_RELAY_TARGET_ADDR is the relay-reachable delivery target.

These are intentionally separate because a direct endpoint can be unreachable
while a relay has an alternate reachable target address.
```

Validation:

```text
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
target/debug/supervm-network-overlay-gate.exe with receiver/relay/sender modes
```

### Cut 11: Runtime Probe Multi-Hop Fallback + Route Plan Observability v0

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut completes the runtime-probe fallback matrix for multi-hop relay routes
and adds route-plan observability fields to the sender/loopback reports.

New report fields:

```text
route_plan_source:
  manual
  simulated_probe
  runtime_probe

runtime_probe_used:
  true | false

auto_relay_hops:
  0 for manual route
  1 for relay fallback
  2+ for multi-hop fallback
```

Observed local process-gate result:

```text
probe-multihop-fallback:
  sender accepted=true
  runtime_probe_report.ack_received=false
  route_plan_source=runtime_probe
  auto_relay_hops=2
  effective_route=multihop
  selected_path=MultiHopRelay
  relay-a accepted=true
  relay-b accepted=true
  receiver accepted=true
  receiver frame_decode_ok=true
```

Boundary:

```text
Multi-hop fallback is still network-only.
Relay-a and relay-b forward opaque NOVORUDP frame bytes.
No APFL.
No AOEM.
No opcode 114.
No ledger/hash/signature execution.
```

This closes the local executable route matrix:

```text
direct probe ok
  -> direct data path

direct probe failed + relay available + 1 hop
  -> relay data path

direct probe failed + relay available + 2 hops
  -> multi-hop data path

direct probe failed + relay unavailable
  -> queue fallback
```

### Cut 12: Overlay Gate Matrix Report v0

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut adds a one-command matrix report for production-overlay audit replay.

Mode:

```text
NOVOVM_OVERLAY_GATE_MODE=matrix
```

Output:

```text
artifacts/network-overlay-gate/matrix.json
```

Matrix coverage:

```text
manual-direct
manual-relay
manual-multihop
manual-queue
auto-direct
auto-relay
auto-multihop
auto-queue
```

Observed result:

```text
matrix accepted=true
case_count=8

manual-direct:
  selected=DirectNovoRudp
  delivered=true
  queued=false
  relay_hop_count=0

manual-relay:
  selected=RelayNovoRudp
  delivered=true
  queued=false
  relay_hop_count=1

manual-multihop:
  selected=MultiHopRelay
  delivered=true
  queued=false
  relay_hop_count=2

manual-queue:
  selected=QueueFallback
  delivered=false
  queued=true

auto-direct:
  route_plan_source=simulated_probe
  selected=DirectNovoRudp
  delivered=true

auto-relay:
  route_plan_source=simulated_probe
  selected=RelayNovoRudp
  delivered=true
  relay_hop_count=1

auto-multihop:
  route_plan_source=simulated_probe
  selected=MultiHopRelay
  delivered=true
  relay_hop_count=2

auto-queue:
  route_plan_source=simulated_probe
  selected=QueueFallback
  delivered=false
  queued=true
```

Validation:

```text
NOVOVM_OVERLAY_GATE_MODE=matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/matrix.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

This gives auditors a compact fallback matrix replay without touching APFL,
AOEM, ledger, opcode 114, or business payload semantics.

### Cut 13: Overlay Gate Multi-Frame Data Plane v0

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut upgrades the local three-process / four-process overlay gate from
single-frame smoke to configurable multi-frame data-plane replay.

New env:

```text
NOVOVM_OVERLAY_GATE_MAX_FRAMES
```

Default:

```text
1
```

So existing one-frame smoke behavior remains unchanged.

Multi-frame behavior:

```text
sender:
  sends N opaque NOVORUDP DATA frames
  or queues N frames for queue fallback

relay:
  forwards N relay envelopes before exiting
  preserves opaque NOVORUDP frame bytes

receiver:
  receives and decodes N NOVORUDP DATA frames
  still replies to runtime Endpoint probe frames
```

Observed local process-gate result with:

```text
NOVOVM_OVERLAY_GATE_MAX_FRAMES=4
```

```text
direct:
  sender accepted=true
  sent_frame_count=4
  receiver accepted=true
  data_frames_received=4

relay:
  sender accepted=true
  sent_frame_count=4
  relay accepted=true
  frames_received=4
  receiver accepted=true
  data_frames_received=4

multihop:
  sender accepted=true
  sent_frame_count=4
  relay-a accepted=true
  relay-a frames_received=4
  relay-b accepted=true
  relay-b frames_received=4
  receiver accepted=true
  data_frames_received=4

queue:
  sender accepted=true
  queued_count=4
```

Validation:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo test -q -p novovm-network relay::loopback -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1
```

Boundary:

```text
network_only=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

This is still a local gate, not yet a full production relay daemon. Its value is
that relay and multi-hop paths now prove repeated opaque frame forwarding
instead of a one-shot packet.

### Cut 14: Overlay Route Health / Cooldown Matrix v0

Implemented in:

```text
crates/novovm-network/src/overlay_runtime.rs
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut adds an optional health-aware runtime route decision layer.

Default API remains unchanged:

```text
decide_overlay_runtime_route_v0
```

New optional API:

```text
decide_overlay_runtime_route_with_health_v0
```

New health model:

```text
OverlayRouteHealthSnapshot
OverlayHopHealth
OverlayRouteHealthState:
  Healthy
  Degraded
  CoolingDown
  Failed
```

The decision layer is still network-only. It does not know or inspect APFL,
AOEM, opcode 114, ledger state, signatures, receipts, or transaction semantics.

Gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=health-matrix
```

Observed result:

```text
accepted=true
case_count=4

health-direct:
  selected=DirectNovoRudp
  reason=DirectAllowed
  delivered=true
  queued=false
  relay_hop_count=0

health-direct-cooldown-multihop:
  selected=MultiHopRelay
  reason=MultiHopRelayRequired
  delivered=true
  queued=false
  relay_hop_count=2

health-single-relay-fallback:
  selected=RelayNovoRudp
  reason=DirectCoolingDown
  delivered=true
  queued=false
  relay_hop_count=1

health-queue-fallback:
  selected=QueueFallback
  reason=RouteHealthExhausted
  delivered=false
  queued=true
```

Validation:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

NOVOVM_OVERLAY_GATE_MODE=health-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/health-matrix.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

This gives the overlay runtime a path to avoid recently failed or cooling-down
hops before retrying business-level payloads. The fallback decision remains in
the network layer.

### Cut 15: Overlay Observation to Health Feedback v0

Implemented in:

```text
crates/novovm-network/src/overlay_runtime.rs
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut connects runtime route observations to the health-aware route decision
layer.

New model:

```text
OverlayRouteAttemptObservation
```

New conversion:

```text
overlay_route_health_from_observations_v0
```

Semantics:

```text
delivered=true:
  no cooldown is created

queued=true:
  no network-hop cooldown is created

delivered=false and queued=false:
  selected network path hops are marked CoolingDown
```

Gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=observation-matrix
```

Observed result:

```text
accepted=true
case_count=3

observation-direct-success:
  selected=DirectNovoRudp
  reason=DirectAllowed
  delivered=true
  queued=false
  health_hops=0

observation-direct-failure:
  selected=MultiHopRelay
  reason=MultiHopRelayRequired
  delivered=true
  queued=false
  health_hops=1

observation-direct-and-multihop-failure:
  selected=QueueFallback
  reason=RouteHealthExhausted
  delivered=false
  queued=true
  health_hops=3
```

Validation:

```text
cargo fmt --check
cargo check -q -p novovm-network
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

NOVOVM_OVERLAY_GATE_MODE=observation-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/observation-matrix.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

This creates the first closed network feedback loop:

```text
route attempt observation
  -> hop cooldown snapshot
  -> health-aware route decision
  -> direct / relay / multi-hop / queue fallback
```

### Cut 16: Direct -> Relay -> Multi-Hop -> Queue Fallback Chain v0

Implemented in:

```text
crates/novovm-network/src/overlay_runtime.rs
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

This cut adds an ordered fallback-chain decision path for production overlay
routing.

New API:

```text
decide_overlay_runtime_fallback_chain_v0
```

Candidate order:

```text
1. direct NOVORUDP
2. single relay NOVORUDP
3. multi-hop relay
4. queue fallback
```

The chain consumes the same `OverlayRouteHealthSnapshot` produced from runtime
observations. It skips cooling-down hops and selects the next viable network
path.

Gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=fallback-chain
```

Observed result:

```text
accepted=true
case_count=4

fallback-direct:
  selected=DirectNovoRudp
  reason=DirectAllowed
  delivered=true
  queued=false
  relay_hop_count=0
  health_hops=0

fallback-relay-after-direct-failure:
  selected=RelayNovoRudp
  reason=DirectCoolingDown
  delivered=true
  queued=false
  relay_hop_count=1
  health_hops=1

fallback-multihop-after-direct-relay-failure:
  selected=MultiHopRelay
  reason=MultiHopRelayRequired
  delivered=true
  queued=false
  relay_hop_count=2
  health_hops=2

fallback-queue-after-all-failure:
  selected=QueueFallback
  reason=RouteHealthExhausted
  delivered=false
  queued=true
  health_hops=4
```

Validation:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

NOVOVM_OVERLAY_GATE_MODE=fallback-chain \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/fallback-chain.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

This is the first executable local fallback-chain proof for:

```text
direct -> relay -> multi-hop -> queue
```

## Cut 17: Fallback Chain Multi-Process Runner v0

Commit scope:

```text
network overlay process gate only
APFL unchanged
AOEM unchanged
ledger unchanged
NOVORUDP frame format unchanged
payload treated as opaque bytes
```

New runner:

```text
scripts/novovm-overlay-fallback-chain-process-gate.ps1
```

Purpose:

```text
Turn the single-process fallback-chain proof into a scriptable multi-process gate.

The runner starts independent gate processes for:
1. direct receiver + sender
2. relay receiver + relay + sender
3. multihop receiver + relay-b + relay-c + sender
4. queue sender

It then writes one aggregate report:
artifacts/network-overlay-gate/fallback-chain-process/report.json
```

Validation command:

```powershell
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-overlay-fallback-chain-process-gate.ps1 `
  -MaxFrames 4 `
  -BasePort 39420
```

Observed result:

```text
accepted=true
case_count=4
max_frames=4

direct:
  sender_sent=4
  receiver_frames=4

relay:
  sender_sent=4
  relay_frames=4
  receiver_frames=4

multihop:
  sender_sent=4
  relay_b_frames=4
  relay_c_frames=4
  receiver_frames=4

queue:
  sender_sent=0
  queued=4
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Significance:

```text
This cut proves the production overlay fallback chain can be exercised as
real independent network processes, not only as an in-process route decision.

It remains strictly a network communication gate:
the runner only verifies opaque NOVORUDP frame delivery/forwarding/queuing.
It does not inspect APFL, call AOEM, or execute ledger semantics.
```

## Cut 18: Configurable Cross-Machine Overlay Process Gate v0

Commit scope:

```text
network overlay process gate only
config-driven node addresses
APFL unchanged
AOEM unchanged
ledger unchanged
NOVORUDP frame format unchanged
payload treated as opaque bytes
```

New files:

```text
configs/network-overlay/cross-machine-loopback.example.json
scripts/novovm-overlay-cross-machine-process-gate.ps1
```

Purpose:

```text
Move the process runner from hardcoded localhost ports to config-driven node
addresses. The same script can now be used in two modes:

1. all-local:
   Start sender / relay / receiver processes on one machine for CI and audit.

2. cross-machine role mode:
   Run one role per machine using the same config:
   receiver, relay, sender, or queue.
```

Example config shape:

```json
{
  "max_frames": 4,
  "timeout_ms": 10000,
  "sender": {
    "node_id": "node-a",
    "bind_addr": "127.0.0.1:0"
  },
  "receiver": {
    "node_id": "node-b",
    "bind_addr": "127.0.0.1:39520",
    "public_addr": "127.0.0.1:39520"
  },
  "relays": [
    {
      "node_id": "relay-1",
      "bind_addr": "127.0.0.1:39530",
      "public_addr": "127.0.0.1:39530"
    },
    {
      "node_id": "relay-2",
      "bind_addr": "127.0.0.1:39540",
      "public_addr": "127.0.0.1:39540"
    }
  ]
}
```

Local validation command:

```powershell
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-overlay-cross-machine-process-gate.ps1 `
  -ConfigPath configs\network-overlay\cross-machine-loopback.example.json `
  -Role all-local `
  -Route all
```

Observed result:

```text
accepted=true
scope=network_overlay_cross_machine_process_gate_v0
case_count=4
max_frames=4

direct:
  sender_sent=4
  receiver_frames=4

relay:
  sender_sent=4
  relay_frames=4
  receiver_frames=4

multihop:
  sender_sent=4
  relay_0_frames=4
  relay_1_frames=4
  receiver_frames=4

queue:
  sender_sent=0
  queued=4
```

Cross-machine usage pattern:

```powershell
# On receiver node:
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-overlay-cross-machine-process-gate.ps1 `
  -ConfigPath <shared-or-local-config.json> `
  -Role receiver `
  -Route direct

# On relay node 0:
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-overlay-cross-machine-process-gate.ps1 `
  -ConfigPath <shared-or-local-config.json> `
  -Role relay `
  -RelayIndex 0 `
  -Route relay

# On relay node 1 for multihop:
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-overlay-cross-machine-process-gate.ps1 `
  -ConfigPath <shared-or-local-config.json> `
  -Role relay `
  -RelayIndex 1 `
  -Route multihop

# On sender node:
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-overlay-cross-machine-process-gate.ps1 `
  -ConfigPath <shared-or-local-config.json> `
  -Role sender `
  -Route direct|relay|multihop
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Significance:

```text
This cut creates the first config-driven bridge from local overlay gates to
real A / Relay / B network smoke tests.

It still does not claim production daemon readiness. It only proves that the
overlay data plane can be launched by role with external node address config.
The network layer remains isolated from APFL/AOEM/ledger semantics.
```

## Cut 19: Real Four-Node Overlay Process Gate v0

Status:

```text
PASS
```

Commit/runtime baseline:

```text
SUPERVM HEAD = 1bc4747
Binary = target/debug/supervm-network-overlay-gate
Scope = real four-device overlay process gate
```

Topology:

```text
A  = 192.168.71.118
B  = 192.168.71.56:41020
R1 = 192.168.71.9:41030
R2 = 192.168.71.54:41040
```

Covered real network paths:

```text
Direct:
  A -> B
  4/4 opaque NOVORUDP frames delivered and decoded.

Relay:
  A -> R1 -> B
  4/4 opaque NOVORUDP frames delivered and decoded.

Multihop:
  A -> R1 -> R2 -> B
  4/4 opaque NOVORUDP frames delivered and decoded.

Queue:
  A local queue fallback
  4/4 opaque NOVORUDP frames queued.
  sent_frame_count=0
  sent_bytes_total=0
```

Direct observed result:

```text
A sender:
  accepted=true
  target=192.168.71.56:41020
  bind_addr_effective=0.0.0.0:45077
  sent_frame_count=4
  sent_bytes_total=556

B receiver:
  accepted=true
  data_frames_received=4
  source_addr=192.168.71.118:45077
  frame_decode_ok=true
```

Relay observed result:

```text
A -> R1:
  sender accepted=true
  sent_frame_count=4

R1 -> B:
  frames_received=4
  delivered_to_target=4/4

B receiver:
  accepted=true
  data_frames_received=4
  source_addr=192.168.71.9:41030
  frame_decode_ok=true
```

Multihop observed result:

```text
A -> R1 -> R2 -> B:
  R2 frames_received=4
  R2 delivered_to_target=4/4

B receiver:
  accepted=true
  data_frames_received=4
  source_addr=192.168.71.54:41040
  frame_decode_ok=true
```

Queue observed result:

```text
accepted=true
requested_route=queue
effective_route=queue
queued_count=4
sent_frame_count=0
sent_bytes_total=0

frames=4/4 queued
sent_to=null
encoded_frame_bytes=139
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Operational findings:

```text
1. Linux sender must bind 0.0.0.0:0 for cross-machine UDP.

   If sender defaults to 127.0.0.1:0 and sends to a non-loopback peer,
   Linux may return Invalid argument (os error 22) or fail to send on the
   expected outbound interface.

2. B reachable endpoint for this four-node run is:
   192.168.71.56:41020

   The B WLAN address 192.168.71.117 was not the effective A -> B path in
   this run.

3. The real four-node run validates network overlay reachability only.
   It does not execute APFL, call AOEM, or touch ledger semantics.
```

Significance:

```text
This cut moves Production Overlay validation out of localhost and into a real
four-device network:

direct -> relay -> multi-hop -> queue fallback

It validates that the overlay data plane can survive role separation across
physical machines while preserving strict network/business separation.
```

## Cut 20: Adaptive Overlay Node Runtime Decision Core v0

Status:

```text
PASS
```

Implemented in:

```text
crates/novovm-network/src/adaptive_overlay.rs
crates/novovm-network/src/lib.rs
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Move from manually assigned A / R1 / R2 / B roles toward fixed node identity
with dynamic runtime role selection.

This cut does not introduce a long-running production daemon yet. It adds the
decision core needed by such a daemon:

- zero-config bind policy
- node capability records
- relay budget policy
- endpoint records
- adaptive candidate route generation
- health-aware direct -> relay -> multihop -> queue route decision
```

New core types:

```text
AdaptiveOverlayBindPolicy
AdaptiveOverlayRelayBudget
AdaptiveOverlayNodeCapabilities
AdaptiveOverlayEndpointRecord
AdaptiveOverlayNodeConfig
AdaptiveOverlayRoutePlan
```

Zero-config default:

```text
bind_policy = Floating
effective_bind_candidates = ["0.0.0.0:0"]
```

This encodes the operational finding from Cut 19:

```text
Linux cross-machine sender must not default to 127.0.0.1:0.
```

Gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=adaptive-node-matrix
```

Validation command:

```powershell
$env:NOVOVM_OVERLAY_GATE_MODE="adaptive-node-matrix"
$env:NOVOVM_OVERLAY_GATE_REPORT_PATH="artifacts/network-overlay-gate/adaptive-node-matrix.json"
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Observed result:

```text
accepted=true
scope=adaptive_overlay_node_matrix_gate_v0
fixed_identity_dynamic_role=true
bind_candidates=["0.0.0.0:0"]
bootstrap_peer_count=4

adaptive-direct-healthy:
  expected_path=DirectNovoRudp
  selected_path=DirectNovoRudp
  reason=DirectAllowed

adaptive-relay-after-direct-cooldown:
  expected_path=RelayNovoRudp
  selected_path=RelayNovoRudp
  reason=DirectCoolingDown

adaptive-multihop-after-direct-relay-cooldown:
  expected_path=MultiHopRelay
  selected_path=MultiHopRelay
  reason=MultiHopRelayRequired

adaptive-queue-after-all-cooldown:
  expected_path=QueueFallback
  selected_path=QueueFallback
  reason=RouteHealthExhausted
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Design note:

```text
Relay is modeled as a capability, not a fixed machine identity.

The same node can be a sender, receiver, relay candidate, or queue fallback
participant depending on runtime request direction, reachability, policy, and
health state.
```

Known v0 limitation:

```text
Health is still node-level, not edge-level.

If a relay node is cooling down, all routes using that relay are skipped. A
future edge-level health model should distinguish:

A -> R1 failure
R1 -> B failure
R1 -> R2 failure
R2 -> B failure
```

Significance:

```text
Cut 19 proved the real four-node data plane.
Cut 20 begins the zero-config product shape:

fixed identity
dynamic endpoint
capability-based relay
health-aware automatic route selection
direct -> relay -> multihop -> queue

without mixing network transport with APFL, AOEM, or ledger semantics.
```

## Cut 21: Adaptive Overlay Node Process Gate v0

Status:

```text
PASS
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
scripts/novovm-adaptive-overlay-node-process-matrix.ps1
```

Goal:

```text
Move the adaptive overlay from an in-process decision matrix to real process
execution where every participant runs the same adaptive-node gate.

This cut removes the fixed test identity shape from the local process gate:
nodes are not launched as "sender", "receiver", "relay", or "queue" modes.
They are launched as adaptive overlay nodes with capabilities, endpoints,
peer records, route health, and a target peer when sending is required.
```

Gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=adaptive-node
```

Adaptive node inputs:

```text
NOVOVM_OVERLAY_ADAPTIVE_NODE_ID
NOVOVM_OVERLAY_ADAPTIVE_BIND_ADDR
NOVOVM_OVERLAY_ADAPTIVE_RELAY_ENABLED
NOVOVM_OVERLAY_ADAPTIVE_QUEUE_ENABLED
NOVOVM_OVERLAY_ADAPTIVE_TARGET_PEER_ID
NOVOVM_OVERLAY_ADAPTIVE_PEERS_JSON
NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_PEERS
```

Default bind behavior:

```text
NOVOVM_OVERLAY_ADAPTIVE_BIND_ADDR defaults to 0.0.0.0:0
```

This keeps the Cut 19 operational rule in the executable process path:

```text
cross-machine sender must not default to 127.0.0.1:0
```

Process matrix runner:

```text
scripts/novovm-adaptive-overlay-node-process-matrix.ps1
```

Validation command:

```powershell
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-adaptive-overlay-node-process-matrix.ps1 `
  -MaxFrames 4 `
  -BasePort 39720
```

Observed aggregate result:

```text
accepted=true
scope=adaptive_overlay_node_process_matrix_v0
max_frames=4
peer_count=4

direct:
  sender selected_path=DirectNovoRudp
  reason=DirectAllowed
  sender_sent=4
  receiver_frames=4

relay:
  sender selected_path=RelayNovoRudp
  reason=DirectCoolingDown
  sender_sent=4
  relay_frames_forwarded=4
  receiver_frames=4

multihop:
  sender selected_path=MultiHopRelay
  reason=MultiHopRelayRequired
  sender_sent=4
  relay_2_frames_forwarded=4
  relay_3_frames_forwarded=4
  receiver_frames=4

queue:
  sender selected_path=QueueFallback
  reason=RouteHealthExhausted
  sender_sent=0
  queued_count=4
```

Per-node report fields include:

```text
node_id
bind_policy
bind_addr_requested
bind_addr_effective
interface_summary
endpoint_record
bootstrap_peer_count
selected_path
decision_reason
relay_budget
queue_enabled
target_peer_id
sent_frame_count
queued_count
direct_frames_received
relay_envelopes_received
relay_frames_forwarded
probe_ack_sent
```

Execution detail:

```text
The adaptive-node process binds its UDP socket before running interface
inventory. This prevents receiver startup races where interface inspection
delays socket binding and the sender transmits before the listener exists.
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Significance:

```text
Cut 21 is the first executable process proof of the Adaptive Overlay Node
product shape:

fixed node identity
floating bind endpoint
endpoint record generation
interface inventory reporting
capability-based relay eligibility
health-aware route decision
automatic direct -> relay -> multihop -> queue selection

Each process still remains a network-only gate. It forwards opaque NOVORUDP
bytes and never interprets APFL, calls AOEM, or mutates ledger state.
```

Known v0 limitation:

```text
This is still a local process matrix, not a long-running production daemon and
not yet a real cross-machine autonomous discovery network.

Peer records are supplied through NOVOVM_OVERLAY_ADAPTIVE_PEERS_JSON.
Interface inventory is reported for audit, but endpoint publication,
external observed-address discovery, signed peer records, abuse policy,
and persistent health gossip remain future cuts.
```

## Cut 22: Real Cross-Machine Adaptive Overlay Node Smoke v0

Status:

```text
PASS
```

Implemented in:

```text
crates/novovm-network/src/adaptive_overlay.rs
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
configs/network-overlay/adaptive-cross-machine-4node.example.json
scripts/novovm-adaptive-overlay-cross-machine-smoke.ps1
```

Goal:

```text
Run the Cut 19 real four-device topology through adaptive-node mode instead
of manually assigned sender / receiver / relay routes.

The sender only provides:
target_peer_id=node-b

The route is selected by:
peer records
capabilities
route health
route-family cooldown state
```

Topology config:

```text
A  = node-a   = 192.168.71.118
B  = node-b   = 192.168.71.56:41020
R1 = relay-1  = 192.168.71.9:41030
R2 = relay-2  = 192.168.71.54:41040
```

Config file:

```text
configs/network-overlay/adaptive-cross-machine-4node.example.json
```

Runner:

```powershell
powershell -ExecutionPolicy Bypass `
  -File scripts\novovm-adaptive-overlay-cross-machine-smoke.ps1 `
  -Action commands `
  -ConfigPath configs\network-overlay\adaptive-cross-machine-4node.example.json
```

This prints per-machine commands for:

```text
adaptive-direct
adaptive-relay
adaptive-multihop
adaptive-queue
```

No command uses:

```text
NOVOVM_OVERLAY_GATE_ROUTE=direct|relay|multihop
```

All active processes use:

```text
NOVOVM_OVERLAY_GATE_MODE=adaptive-node
```

Route-family health input:

```text
adaptive-direct:
  cooldown_route_families=[]

adaptive-relay:
  cooldown_route_families=[direct]

adaptive-multihop:
  cooldown_route_families=[direct, relay]

adaptive-queue:
  cooldown_route_families=[direct, relay, multihop]
```

Why route-family cooldown exists:

```text
Cut 20/21 hop health is node-level.

In a two-relay topology, marking R1 itself as cooling down would also remove
the A -> R1 -> R2 -> B multihop path. Route-family cooldown represents the
production condition "direct path failed" or "single-relay path failed" while
keeping relay nodes themselves eligible for multihop.
```

New adaptive report fields:

```text
route_plan_source
candidate_route_count
candidate_direct_count
candidate_relay_count
candidate_multihop_count
cooldown_hop_count
cooldown_hops
cooldown_route_families
received_frame_count
```

Validated locally:

```text
cargo fmt --check
cargo check -q -p novovm-network
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo test -q -p novovm-network adaptive_overlay -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

adaptive-node process matrix:
  direct    PASS
  relay     PASS
  multihop  PASS
  queue     PASS

cross-machine runner queue case:
  selected_path=QueueFallback
  decision_reason=RouteHealthExhausted
  queued_count=4
  sent_frame_count=0
```

Real-machine observed result:

```text
adaptive-direct:
  accepted=true
  selected_path=DirectNovoRudp
  decision_reason=DirectAllowed
  route_plan_source=adaptive_runtime_peer_records_health
  candidate_direct_count=1
  candidate_relay_count=1
  candidate_multihop_count=1
  A sent_frame_count=4
  B received_frame_count=4

adaptive-relay:
  accepted=true
  selected_path=RelayNovoRudp
  decision_reason=DirectCoolingDown
  route_plan_source=adaptive_runtime_peer_records_health
  cooldown_route_families=[Direct]
  candidate_direct_count=0
  candidate_relay_count=1
  candidate_multihop_count=1
  A sent_frame_count=4
  R1 relay_frames_forwarded=4
  B received_frame_count=4

adaptive-multihop:
  accepted=true
  selected_path=MultiHopRelay
  decision_reason=MultiHopRelayRequired
  route_plan_source=adaptive_runtime_peer_records_health
  cooldown_route_families=[Direct, Relay]
  candidate_direct_count=0
  candidate_relay_count=0
  candidate_multihop_count=1
  A sent_frame_count=4
  R1 relay_frames_forwarded=4
  R2 relay_frames_forwarded=4
  B received_frame_count=4

adaptive-queue:
  accepted=true
  selected_path=QueueFallback
  decision_reason=RouteHealthExhausted
  route_plan_source=adaptive_runtime_peer_records_health
  cooldown_route_families=[Direct, Relay, Multihop]
  candidate_direct_count=0
  candidate_relay_count=0
  candidate_multihop_count=0
  queued_count=4
  sent_frame_count=0
  sent_bytes_total=0
```

Aggregate reports:

```text
artifacts/network-overlay-gate/real-adaptive-overlay-4node-20260703/adaptive-direct/aggregate.json
artifacts/network-overlay-gate/real-adaptive-overlay-4node-20260703/adaptive-relay/aggregate.json
artifacts/network-overlay-gate/real-adaptive-overlay-4node-20260703/adaptive-multihop/aggregate.json
artifacts/network-overlay-gate/real-adaptive-overlay-4node-20260703/adaptive-queue/aggregate.json
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Significance:

```text
Cut 22 is the first real cross-machine adaptive-node smoke where node identity
is fixed, endpoints are dynamic, relay is a capability, and route selection is
driven by peer records, capability, and route health rather than manual route
selection.

The sender never specifies:

NOVOVM_OVERLAY_GATE_ROUTE=direct|relay|multihop

It only specifies:

target_peer_id=node-b

The runtime automatically selects:

DirectNovoRudp
RelayNovoRudp
MultiHopRelay
QueueFallback
```

Operational findings:

```text
1. advertised_endpoint cannot remain 0.0.0.0:port in production records.
   It must be replaced with a reachable endpoint selected by probe result,
   for example 192.168.71.x:port or an observed public endpoint.

2. B's effective reachable endpoint in this run is:
   192.168.71.56:41020

   The WLAN address 192.168.71.117 is not the endpoint to publish for this
   topology.

3. direct_frames_received is ambiguous in relay/multihop receiver reports.
   It means "decoded NOVORUDP data frames received by this node", not that the
   route was direct. A future report field should rename or alias it as:

   novorudp_frames_received

4. relay delivered_to_target=false on an intermediate hop can be misread.
   It means the relay forwarded to the next hop, not to final target. Future
   reports should separate:

   forwarded_to_next_hop
   delivered_to_final_target
```

Next frontier:

```text
Cut 23: Endpoint Advertisement + Interface Selection Fix v0

Fix advertised endpoint publication, interface scoring, VPN/virtual adapter
avoidance, and reachable endpoint selection before NAT traversal / UDP hole
punching.
```

## Cut 23: Endpoint Advertisement + Interface Selection Fix v0

Status:

```text
LOCAL / CONFIG VALIDATION PASS
REAL FOUR-MACHINE REGRESSION PASS
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Stop publishing bind endpoints such as 0.0.0.0:port in adaptive endpoint
records.

Separate:

bind_addr_requested
bind_addr_effective
advertised_endpoint
observed_endpoint

Cut 23 v0 implements the first three. observed_endpoint remains a future NAT /
external observer feature.
```

New report field:

```text
endpoint_selection
```

It contains:

```text
advertised_endpoint
endpoint_selection_reason
bind_addr_effective
candidates
rejected_candidates
policy
```

Selection order:

```text
1. NOVOVM_OVERLAY_ADAPTIVE_ADVERTISED_ENDPOINT
2. self peer record endpoint from NOVOVM_OVERLAY_ADAPTIVE_PEERS_JSON
3. bind_addr_effective, only if it is publishable
```

Default rejection policy:

```text
reject_unspecified=true       # rejects 0.0.0.0 / ::
reject_loopback_by_default=true
reject_link_local_by_default=true
```

Observed validation:

```text
A:
  bind_addr_effective=0.0.0.0:<dynamic-port>
  advertised_endpoint=192.168.1.246:<dynamic-port>
  endpoint_selection_reason=manually_configured_public_addr
  rejected bind candidate:
    endpoint=0.0.0.0:<dynamic-port>
    reason=reject_unspecified_ip

B:
  bind_addr_effective=0.0.0.0:41020
  advertised_endpoint=192.168.1.245:41020
  endpoint_selection_reason=manually_configured_public_addr

R1:
  bind_addr_effective=0.0.0.0:41030
  advertised_endpoint=192.168.1.178:41030
  endpoint_selection_reason=manually_configured_public_addr

R2:
  bind_addr_effective=0.0.0.0:41040
  advertised_endpoint=192.168.1.11:41040
  endpoint_selection_reason=manually_configured_public_addr
```

Real four-machine regression:

```text
Topology:
  A  = 192.168.1.246
  B  = 192.168.1.245:41020
  R1 = 192.168.1.178:41030
  R2 = 192.168.1.11:41040

adaptive-direct:
  selected_path=DirectNovoRudp
  A -> B
  delivered=4/4

adaptive-relay:
  selected_path=RelayNovoRudp
  A -> R1 -> B
  delivered=4/4

adaptive-multihop:
  selected_path=MultiHopRelay
  A -> R1 -> R2 -> B
  delivered=4/4

adaptive-queue:
  selected_path=QueueFallback
  queued_count=4
  sent_bytes_total=0
```

Endpoint advertisement regression:

```text
advertised_endpoint no longer publishes 0.0.0.0:port.
0.0.0.0:* candidates are rejected with reject_unspecified_ip.

A  advertised_endpoint=192.168.1.246:<dynamic-port>
B  advertised_endpoint=192.168.1.245:41020
R1 advertised_endpoint=192.168.1.178:41030
R2 advertised_endpoint=192.168.1.11:41040
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo test -q -p novovm-network adaptive_overlay -- --test-threads=1

adaptive-node process matrix:
  direct    PASS
  relay     PASS
  multihop  PASS
  queue     PASS

cut23 endpoint selection sample:
  adaptive-queue PASS
  advertised_endpoint=192.168.1.246:<dynamic-port>
  bind candidate 0.0.0.0:<dynamic-port> rejected

real four-machine adaptive regression:
  adaptive-direct    PASS
  adaptive-relay     PASS
  adaptive-multihop  PASS
  adaptive-queue     PASS
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Known v0 limitation:

```text
Cut 23 v0 uses configured peer-record endpoints as the source of publishable
addresses.

It does not yet run active peer reachability probes, does not derive observed
public endpoints, and does not perform NAT traversal / UDP hole punching.
```

Next frontier:

```text
Cut 24: Reachability Probe + Observed Endpoint Record v0

Use peer observations and probe replies to decide whether the configured
advertised endpoint is actually reachable, then record observed source
addresses for later NAT traversal.
```

## Cut 24: Reachability Probe + Observed Endpoint Record v0

Status:

```text
LOCAL / ROLE VALIDATION PASS
REAL FOUR-MACHINE OBSERVED ENDPOINT SMOKE PASS
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Record what a peer actually observes as the source endpoint of a probe.

Separate:

bind_addr_effective
advertised_endpoint
observed_endpoint
ack_source_endpoint

This is the required step before NAT traversal / UDP hole punching.
```

New gate modes:

```text
NOVOVM_OVERLAY_GATE_MODE=observed-endpoint-matrix
NOVOVM_OVERLAY_GATE_MODE=observed-endpoint
```

Observed endpoint role mode:

```text
NOVOVM_OVERLAY_OBSERVED_ROLE=observer
NOVOVM_OVERLAY_OBSERVED_ROLE=prober
```

Probe / ack behavior:

```text
Probe:
  NOVORUDP frame kind = Endpoint
  payload = ObservedEndpointProbePayloadV0

Ack:
  NOVORUDP frame kind = Ack
  payload = ObservedEndpointAckPayloadV0

Ack must echo the probe_nonce.
Nonce mismatch is rejected and cannot mark the endpoint reachable.
```

New report fields:

```text
observed_endpoint
observed_by_peer_id
observed_at_ms
ack_source_endpoint
observed_endpoint_changed
observed_endpoint_stable
reachability_probe_result
probe_nonce
ack_nonce
probe_ack_valid
probe_reject_reason
probe_rtt_ms
```

Local matrix validation:

```text
observed-endpoint-matrix accepted=true

lan-observed-endpoint:
  probe_ack_valid=true
  reachability_probe_result=reachable
  observed_endpoint=127.0.0.1:<dynamic-port>
  ack_source_endpoint=127.0.0.1:<observer-port>

nonce-mismatch-rejected:
  probe_ack_valid=false
  reachability_probe_result=rejected
  probe_reject_reason=probe_nonce_mismatch
```

Role validation:

```text
observer accepted=true
prober accepted=true
probe_ack_valid=true
observed_endpoint=127.0.0.1:<prober-port>
ack_source_endpoint=127.0.0.1:41120
```

Real four-machine observed endpoint smoke:

```text
Topology:
  A  = 192.168.1.246
  B  = 192.168.1.245:41020
  R1 = 192.168.1.178:41030
  R2 = 192.168.1.11:41040

A->B observed endpoint:
  accepted=true
  probe_ack_valid=true
  probe_nonce=a-to-b-cut24-001
  ack_nonce=a-to-b-cut24-001
  observed_by_peer_id=node-b
  ack_source_endpoint=192.168.1.245:41020
  observed_endpoint=192.168.1.246:58693
  reachability_probe_result=reachable

R1->B observed endpoint:
  accepted=true
  probe_ack_valid=true
  probe_nonce=r1-to-b-cut24-001
  ack_nonce=r1-to-b-cut24-001
  observed_by_peer_id=node-b
  ack_source_endpoint=192.168.1.245:41020
  observed_endpoint=192.168.1.178:65108
  reachability_probe_result=reachable

R2->B observed endpoint:
  accepted=true
  probe_ack_valid=true
  probe_nonce=r2-to-b-cut24-001
  ack_nonce=r2-to-b-cut24-001
  observed_by_peer_id=node-b
  ack_source_endpoint=192.168.1.245:41020
  observed_endpoint=192.168.1.11:65525
  reachability_probe_result=reachable

stale nonce rejection:
  accepted=false
  probe_ack_valid=false
  probe_nonce=a-to-b-cut24-stale-001
  ack_nonce=wrong-a-to-b-cut24-001
  probe_reject_reason=probe_nonce_mismatch
  reachability_probe_result=rejected
  observed_endpoint=192.168.1.246:38857
  ack_source_endpoint=192.168.1.245:41020
```

Observed smoke result:

```text
A->B observed endpoint      = PASS
R1->B observed endpoint     = PASS
R2->B observed endpoint     = PASS
stale nonce rejection       = PASS

Cut 24 real four-machine observed endpoint smoke = PASS
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo check -q -p novovm-network
cargo test -q -p novovm-network adaptive_overlay -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

NOVOVM_OVERLAY_GATE_MODE=observed-endpoint-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/observed-endpoint-matrix-cut24.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
nat_hole_punch=false
```

Known v0 limitation:

```text
Cut 24 v0 records observed endpoints and rejects stale / mismatched probe ack.

It does not yet perform NAT hole punching, simultaneous open, relay-assisted
punch orchestration, endpoint stability windows, or public observer quorum.
```

Next frontier:

```text
Cut 25:
  NAT Traversal Probe + UDP Hole Punch v0
```

## Cut 25: NAT Traversal Probe + UDP Hole Punch v0

Status:

```text
LOCAL NAT-PUNCH LOGIC SMOKE PASS
REAL CROSS-NAT SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Add the first NAT punch control-plane smoke path after Cut 24 observed endpoint
records.

This cut validates local punch control logic:
1. punch probe / ack success
2. punch nonce mismatch rejection
3. punch failure -> RelayNovoRudp fallback selection

It does not claim real NAT traversal success yet.
```

New gate modes:

```text
NOVOVM_OVERLAY_GATE_MODE=nat-punch-matrix
NOVOVM_OVERLAY_GATE_MODE=nat-punch
```

NAT punch role mode:

```text
NOVOVM_OVERLAY_OBSERVED_ROLE=observer
NOVOVM_OVERLAY_OBSERVED_ROLE=prober
```

Probe / ack behavior:

```text
Punch probe:
  NOVORUDP frame kind = Endpoint
  payload = NatPunchProbePayloadV0

Punch ack:
  NOVORUDP frame kind = Ack
  payload = NatPunchAckPayloadV0

Ack must echo punch_nonce.
Nonce mismatch is rejected and cannot mark punch reachable.
```

New / relevant report fields:

```text
punch_nonce
ack_nonce
punch_target_peer_id
punch_target_observed_endpoint
punch_attempt_sent
punch_ack_valid
punch_reject_reason
punch_result
selected_path_after_punch
relay_fallback_selected
fallback_reason
observed_endpoint
observed_by_peer_id
ack_source_endpoint
```

Local NAT punch matrix validation:

```text
nat-punch-matrix accepted=true

success:
  punch_ack_valid=true
  punch_result=punched_direct
  selected_path_after_punch=PunchedDirect

nonce mismatch:
  punch_ack_valid=false
  punch_reject_reason=punch_nonce_mismatch

fallback:
  punch_ack_valid=false
  relay_fallback_selected=true
  fallback_reason=NatPunchFailed
  selected_path_after_punch=RelayNovoRudp
```

Local role validation:

```text
observer accepted=true
prober accepted=true
punch_ack_valid=true
punch_result=punched_direct
selected_path_after_punch=PunchedDirect
observed_endpoint=127.0.0.1:<prober-port>
ack_source_endpoint=127.0.0.1:<observer-port>
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo check -q -p novovm-network
cargo test -q -p novovm-network adaptive_overlay -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

NOVOVM_OVERLAY_GATE_MODE=nat-punch-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/nat-punch-matrix-cut25.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
```

Real cross-NAT smoke remains pending:

```text
Cut 25 has not yet proven:
REAL NAT PUNCH PASS
REAL CROSS-NAT UDP HOLE PUNCH PASS
REAL CROSS-NAT RELAY FALLBACK PASS

To run the real smoke, A must be able to send UDP to:
B_OBSERVED_OR_PUBLIC_ADDR=<B public IP / DDNS / port mapping>:41020

If punch succeeds:
  selected_path_after_punch=PunchedDirect

If punch fails but fallback works:
  selected_path_after_punch=RelayNovoRudp
  fallback_reason=NatPunchFailed
```

Next frontier:

```text
Cut 26:
  Relay-First Zero-Config Overlay v0
```

## Cut 26: Relay-First Zero-Config Overlay v0

Status:

```text
LOCAL PRODUCT-POLICY MATRIX PASS
REAL PUBLIC RELAY SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Make the product connectivity contract explicit:

The user must not need to know IPs, open ports, configure routers, or understand
NAT/VPN/firewall state for baseline communication.

Baseline connectivity is relay-first over outbound-friendly transports.
Direct UDP / NAT punch is an optimization only.
If punch fails, the node stays on relay.
If relay is unavailable, the node queues rather than falsely claiming delivery.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=relay-first-zero-config-matrix
```

Local policy matrix:

```text
vpn-tun-or-cgnat-no-inbound-udp:
  selected_path=RelayNovoRudp
  outbound_transport=QUIC_OR_TLS_OR_WEBSOCKET_443
  udp_inbound_required=false
  user_network_configuration_required=false

observed-endpoint-and-punch-success-upgrades-path:
  initial_path=RelayNovoRudp
  punch_ack_valid=true
  selected_path_after_punch=PunchedDirect

punch-fails-stays-on-relay:
  initial_path=RelayNovoRudp
  punch_ack_valid=false
  selected_path_after_punch=RelayNovoRudp
  fallback_reason=NatPunchFailed

relay-unavailable-queues-without-data-loss-claim:
  selected_path=QueueFallback
  queued=true
```

Privileged node service policy:

```text
Dedicated NOVOVM node deployments may install an explicitly authorized local
service with highest local privilege.

Allowed under explicit install / node ownership:
  manage local firewall rules
  manage local services and startup
  inspect interfaces, VPN/TUN routes, and route metrics
  attempt UPnP / NAT-PMP / PCP mappings
  choose relay / direct / punch / queue paths automatically

Not allowed / not claimed:
  bypass external firewalls
  bypass VPN provider policy
  bypass ISP CGNAT
  bypass cloud security groups
  bypass OS privilege requirements without authorization
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo check -q -p novovm-network
cargo test -q -p novovm-network adaptive_overlay -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

NOVOVM_OVERLAY_GATE_MODE=relay-first-zero-config-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/relay-first-zero-config-matrix-cut26.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
real_public_relay_smoke=false
```

Next frontier:

```text
Cut 27:
  Real Public Relay Bootstrap Smoke v0
```

## Cut 27: Public Relay Bootstrap Smoke v0

Status:

```text
LOCAL PUBLIC RELAY BOOTSTRAP MATRIX PASS
REAL PUBLIC VPS RELAY SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Prove the relay-first zero-config data path shape before using a real public
VPS relay.

A and B both actively bootstrap to public relay R.
A sends to target_peer_id=node-b through R.
R forwards by peer_id using B's registered relay session endpoint.
B does not need a public inbound endpoint.
```

New gate modes:

```text
NOVOVM_OVERLAY_GATE_MODE=public-relay-bootstrap-matrix
NOVOVM_OVERLAY_GATE_MODE=public-relay-bootstrap
```

Role mode:

```text
NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE=relay
NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE=client-register
NOVOVM_OVERLAY_PUBLIC_RELAY_ROLE=client-send
```

Local bootstrap matrix:

```text
public-relay-bootstrap-matrix accepted=true

A:
  selected_path=RelayNovoRudp
  route_plan_source=relay_first_zero_config_policy
  target_peer_id=node-b
  selected_relay_peer_id=public-relay-1
  sent_frame_count=4
  queued_count=0

Public relay:
  node_id=public-relay-1
  relay_enabled=true
  bootstrap_sessions_established=2
  session_peer_ids=[node-a,node-b]
  relay_envelopes_received=4
  relay_frames_forwarded=4
  forwarded_to_peer_id=node-b

B:
  inbound_public_endpoint_required=false
  received_frame_count=4
  frame_decode_ok=true
  source_peer_id=node-a
  via_relay_peer_id=public-relay-1
```

Real public VPS relay smoke remains pending:

```text
Required:
  R_PUBLIC_ADDR=<public-vps-ip-or-ddns>:<relay-port>
  public relay inbound UDP allowed for this smoke

Expected:
  A and B do not require public inbound endpoints.
  A and B both send outbound bootstrap registration to R.
  A sends only target_peer_id=node-b through R.
  R forwards to node-b's registered relay session endpoint.
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate
cargo check -q -p novovm-network
cargo test -q -p novovm-network adaptive_overlay -- --test-threads=1
cargo test -q -p novovm-network overlay_runtime -- --test-threads=1

NOVOVM_OVERLAY_GATE_MODE=public-relay-bootstrap-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/public-relay-bootstrap-matrix-cut27.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
real_public_vps_relay_smoke=false
```

Next frontier:

```text
Cut 28:
  Relay Endpoint Candidate Selection v0
```

## Cut 28: Relay Endpoint Candidate Selection v0

Status:

```text
LOCAL RELAY ENDPOINT CANDIDATE MATRIX PASS
REAL MULTI-TRANSPORT PUBLIC RELAY SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Remove fixed relay port assumptions from the product path.

Cut 27 proved the local relay bootstrap semantics.
Cut 28 proves that production relay bootstrap must select from endpoint
candidates instead of requiring one hard-coded P2P port.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=relay-endpoint-candidates-matrix
```

Port policy:

```text
443:
  Preferred zero-config relay bootstrap path.
  Used for WSS / TLS / QUIC candidates.

80:
  Allowed only as plain HTTP/WebSocket compatibility fallback.
  Not a UDP default.
  Not the preferred path because it is often intercepted or modified by proxy,
  captive portal, ISP, or enterprise middleware.

41030:
  Test-only UDP smoke example.
  Not a product requirement.

Dynamic/high UDP ports:
  Allowed as performance candidates when the network permits them.
  Never required for user availability.
```

Local candidate matrix:

```text
relay-endpoint-candidates-matrix accepted=true

candidate_selection_order:
  1. wss_443
  2. quic_443
  3. tls_443
  4. ws_80
  5. udp_dynamic_or_configured
  6. queue_fallback

fixed_relay_port_required=false
fixed_41030_used_as_requirement=false
user_router_configuration_required=false
user_firewall_configuration_required=false
nat_punch_is_optimization=true
direct_path_is_optimization=true
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=relay-endpoint-candidates-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/relay-endpoint-candidates-matrix-cut28.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
real_multi_transport_public_relay_smoke=false
```

Next frontier:

```text
Cut 29:
  443 Outbound Relay Transport v0
```

## Cut 29: 443 Outbound Relay Transport v0

Status:

```text
LOCAL WSS 443 OUTBOUND RELAY MATRIX PASS
REAL WSS/TLS PUBLIC RELAY SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Make the default zero-config availability path explicit:

A and B do not need public inbound endpoints.
A and B actively connect outbound to relay R over the product default
WSS/TLS 443 candidate.
R forwards opaque NOVORUDP frames by target_peer_id over node-b's relay
session.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=wss-443-outbound-relay-matrix
```

Local WSS 443 relay matrix:

```text
wss-443-outbound-relay-matrix accepted=true

outer_transport:
  selected_transport=wss
  selected_endpoint=wss://relay.example.com:443/novovm
  selected_port=443
  direction=client_outbound
  tls_expected=true
  requires_user_port_forward=false
  requires_public_client_inbound=false

NOVORUDP:
  novorudp_carriage=NOVORUDP-over-WSS-443
  novorudp_wire_changed=false
  payload_treated_opaque=true
```

Expected local matrix fields:

```text
A:
  selected_transport=wss
  selected_endpoint=wss://relay.example.com:443/novovm
  selected_path=RelayNovoRudp
  target_peer_id=node-b
  sent_frame_count=4
  inbound_public_endpoint_required=false
  nat_punch_required=false

R:
  listener=0.0.0.0:443
  transport=wss
  bootstrap_sessions_established=2
  session_peer_ids=[node-a,node-b]
  relay_frames_forwarded=4
  forwarded_to_peer_id=node-b
  forwards_by_peer_id=true

B:
  transport=wss
  received_frame_count=4
  frame_decode_ok=true
  via_relay_peer_id=public-relay-1
  inbound_public_endpoint_required=false
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=wss-443-outbound-relay-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/wss-443-outbound-relay-matrix-cut29.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
real_wss_tls_public_relay_smoke=false
```

Architecture clarification:

```text
WSS/TLS 443 relay is the default zero-config outbound transport.
It is not a centralized NOVOVM control plane.

Relay nodes are replaceable reachability helpers.
They forward opaque NOVORUDP frames by peer identity.
They are not trusted authorities and do not own peer identity, payload,
routing rights, or execution semantics.
```

Next frontier:

```text
Cut 30:
  Decentralized Bootstrap Constraint Matrix v0
```

## Cut 30: Decentralized Bootstrap Constraint Matrix v0

Status:

```text
LOCAL DECENTRALIZED BOOTSTRAP CONSTRAINT MATRIX PASS
REAL FEDERATED RELAY / LAN / CELLULAR MIXED TOPOLOGY SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Define the terminal product network model without pretending that physical
network constraints do not exist.

NOVOVM must be decentralized.
But decentralization does not mean two devices behind unrelated NAT / CGNAT /
VPN / TUN networks can always discover each other without any shared reachable
medium.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=decentralized-bootstrap-constraint-matrix
```

Core constraints:

```text
LAN broadcast / mDNS:
  Useful for same-LAN discovery.
  Does not cross router / NAT / carrier CGNAT / VPN TUN boundaries.

Cellular node outside LAN:
  Cannot be discovered by LAN broadcast.
  If it has no public reachable endpoint and no shared rendezvous / relay
  candidate, direct discovery is not guaranteed.

Relay / rendezvous:
  Required as a reachability role for many real-world topologies.
  Not required to be centralized.
  Any NOVOVM node can become a relay candidate.
```

Local constraint matrix:

```text
decentralized-bootstrap-constraint-matrix accepted=true

centralized_control_plane_required=false
single_official_relay_required=false
single_official_domain_required=false
relay_is_trusted_authority=false
peer_identity_source=novovm_key
routing_subject=target_peer_id
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
```

Terminal strategy:

```text
1. LAN-first discovery:
   mDNS / UDP local broadcast / local peer cache

2. Direct when physically possible:
   IPv6 / public endpoint / observed endpoint probe

3. Federated relay candidates:
   Multiple replaceable NOVOVM relay nodes, not one official server

4. Peer-signed relay endpoint records:
   Relay reachability records are attached to NOVOVM identity

5. Multi-relay rotation:
   Fail over between relay candidates without trusting one relay

6. NAT punch as optimization:
   Improves latency/cost when possible, not required for availability

7. WSS/TLS 443 as default outbound transport:
   Real-world zero-config carrier across VPN/TUN/CGNAT/enterprise networks

8. Queue fallback:
   If no candidate path exists, do not fake connectivity
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=decentralized-bootstrap-constraint-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/decentralized-bootstrap-constraint-matrix-cut30.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
real_federated_relay_smoke=false
```

Next frontier:

```text
Cut 31:
  Multi-relay Candidate Selection / Rotation v0
```

## Cut 31: Multi-relay Candidate Selection / Rotation v0

Status:

```text
LOCAL MULTI-RELAY CANDIDATE ROTATION MATRIX PASS
REAL MULTI-RELAY FEDERATED SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Prove the client does not depend on a single relay.
The node selects from multiple peer-signed relay candidates, skips invalid
or cooled-down relays, rotates on send failure, and falls back to queue when
no reachable relay exists.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=multi-relay-candidate-rotation-matrix
```

Relay candidate fields:

```text
relay_peer_id
endpoint
transport
port
priority
last_seen_ms
last_success_ms
failure_count
cooldown_until_ms
observed_reachable
supports_wss_443
supports_quic_443
supports_udp
record_signature_valid
```

Selection policy:

```text
require_record_signature_valid=true
skip_cooldown_relays=true
skip_unreachable_relays=true
prefer_wss_443_over_udp_fixed_port=true
rotate_on_send_failure=true
all_relays_failed_fallback=QueueFallback
relay_is_trusted_authority=false
centralized_control_plane_required=false
single_official_relay_required=false
peer_identity_source=novovm_key
routing_subject=target_peer_id
```

Local rotation matrix:

```text
case 1: single healthy relay
  selected_relay_peer_id=relay-a
  selected_path_after_relay_selection=RelayNovoRudp

case 2: primary relay cooldown
  relay-a skipped
  selected_relay_peer_id=relay-b

case 3: primary relay send failure
  relay-a failure_count increments
  relay-a enters cooldown
  relay-b selected

case 4: invalid relay signature
  relay-invalid rejected
  reject_reason=relay_record_signature_invalid
  relay-b selected

case 5: all relays unavailable
  selected_path_after_relay_selection=QueueFallback
  fallback_reason=NoReachableRelayCandidate

case 6: transport priority
  wss://relay:443 preferred over udp://relay:41030
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=multi-relay-candidate-rotation-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/multi-relay-candidate-rotation-matrix-cut31.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
apfl_interpreted=false
aoem_called=false
opcode114_called=false
ledger_semantics=false
novorudp_wire_changed=false
relay_is_trusted_authority=false
real_multi_relay_federated_smoke=false
```

Next frontier:

```text
Cut 32:
  Peer-signed Relay Endpoint Record v0
```

## Cut 32: Peer-signed Relay Endpoint Record v0

Status:

```text
LOCAL PEER-SIGNED RELAY RECORD MATRIX PASS
REAL FEDERATED SIGNED RELAY RECORD SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Turn relay_record_signature_valid from a policy boolean into a real
signature verification step.

Relay candidates are not accepted from a trusted central table.
Each relay endpoint record must be signed by the NOVOVM key corresponding
to relay_peer_id, and clients independently verify the record before using
it for relay selection.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=peer-signed-relay-record-matrix
```

Record shape:

```text
PeerSignedRelayEndpointRecord:
  record_version
  relay_peer_id
  relay_public_key
  endpoints:
    transport
    uri
    port
    priority
    capabilities
  issued_at_ms
  expires_at_ms
  nonce_or_record_id
  signature_scheme=ed25519
  signature
```

Canonical payload covered by signature:

```text
record_version
relay_peer_id
relay_public_key
endpoints
issued_at_ms
expires_at_ms
nonce_or_record_id
capabilities
```

Local signed record matrix:

```text
valid signed relay record:
  signature_valid=true
  record_accepted=true
  selected_transport=wss
  selected_path_after_relay_selection=RelayNovoRudp

invalid signature:
  signature_valid=false
  reject_reason=relay_record_signature_invalid

expired record:
  signature_valid=true
  reject_reason=relay_record_expired

peer_id / public_key mismatch:
  signature_valid=true
  reject_reason=relay_record_identity_mismatch

endpoint tamper after signing:
  signature_valid=false
  reject_reason=relay_record_signature_invalid

unsupported transport:
  signature_valid=true
  reject_reason=relay_transport_unsupported

multiple valid records:
  wss://relay:443 preferred over udp://relay:41030
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=peer-signed-relay-record-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/peer-signed-relay-record-matrix-cut32.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
relay_is_trusted_authority=false
centralized_control_plane_required=false
single_official_relay_required=false
peer_identity_source=novovm_key
routing_subject=target_peer_id
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
real_federated_signed_relay_record_smoke=false
```

Next frontier:

```text
Cut 33:
  Privacy-preserving Node Discovery / Blinded Relay Directory v0
```

## Cut 33: Privacy-preserving Node Discovery / Blinded Relay Directory v0

Status:

```text
LOCAL PRIVACY-PRESERVING NODE DISCOVERY MATRIX PASS
REAL FEDERATED BLINDED DIRECTORY SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Prevent NOVOVM discovery from becoming a full raw IP relay list sync system.

Nodes must receive only a minimal necessary relay candidate set.
Directory responses expose blinded / encrypted endpoint hints, signed record
hashes, classes, score buckets, and expiry metadata rather than the full
raw endpoint inventory.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=privacy-preserving-node-discovery-matrix
```

Directory entry shape:

```text
BlindedRelayDirectoryEntry:
  relay_peer_id
  relay_record_hash
  transport_class
  region_hint
  capability_class
  score_bucket
  expires_at_ms
  encrypted_or_blinded_endpoint_hint
  relay_record_signature
```

Local privacy matrix:

```text
full raw IP directory exposure rejected:
  raw_ip_directory_exposed=false
  reject_reason=full_directory_sync_forbidden

minimal candidate set issued:
  node_receives_minimal_candidate_set=true
  candidate_set_size <= candidate_set_policy_limit

valid signed blinded candidate accepted:
  candidate_record_signed=true
  candidate_signature_valid_count > 0
  candidate_endpoint_encrypted_or_blinded=true

tampered candidate rejected:
  reject_reason=relay_record_signature_invalid

expired candidate rejected:
  reject_reason=relay_record_expired

excessive directory sync rejected:
  full_relay_ip_list_synced=false
  reject_reason=full_directory_sync_forbidden

routing remains peer based:
  routing_subject=target_peer_id

relay remains non-authority:
  relay_is_trusted_authority=false
  business_semantics_interpreted_by_relay=false
```

Non-goals in v0:

```text
tor_grade_anonymity_claimed=false
os_router_isp_visibility_hidden=false
economic_penalty_or_chain_market=false
full_dht_implemented=false
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=privacy-preserving-node-discovery-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/privacy-preserving-node-discovery-matrix-cut33.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
raw_ip_directory_exposed=false
full_relay_ip_list_synced=false
relay_is_trusted_authority=false
centralized_control_plane_required=false
single_official_relay_required=false
peer_identity_source=novovm_key
routing_subject=target_peer_id
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
real_federated_blinded_directory_smoke=false
```

Next frontier:

```text
Cut 34:
  Signed Bootstrap Manifest v0
```

## Cut 34: Signed Bootstrap Manifest v0

Status:

```text
LOCAL SIGNED BOOTSTRAP MANIFEST MATRIX PASS
REAL MULTI-SOURCE BOOTSTRAP SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Define how a first-install / first-start NOVOVM node obtains initial bootstrap
seed information without turning the bootstrap source into a centralized
control plane.

Bootstrap manifests may come from installer bundles, official downloads,
QR invites, friend invites, or history cache, but clients must verify the
manifest signature, expiry, policy flags, and seed relay records before use.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=signed-bootstrap-manifest-matrix
```

Manifest policy:

```text
signature_required=true
expiry_required=true
full_raw_ip_directory_forbidden=true
single_official_relay_forbidden=true
single_official_domain_forbidden=true
seed_candidates_forwarded_to_cut33_directory_policy=true
```

Local signed bootstrap matrix:

```text
valid signed bootstrap manifest:
  bootstrap_manifest_signature_valid=true
  client_accepts_manifest=true

invalid manifest signature rejected:
  client_accepts_manifest=false
  client_reject_reason=bootstrap_manifest_signature_invalid

expired manifest rejected:
  bootstrap_manifest_expired=true
  client_reject_reason=bootstrap_manifest_expired

manifest with full raw IP directory rejected:
  full_raw_ip_directory_embedded=true
  client_reject_reason=full_raw_ip_directory_forbidden

manifest requiring single official relay rejected:
  manifest_requires_single_official_relay=true
  client_reject_reason=single_official_relay_forbidden

manifest requiring single official domain rejected:
  manifest_requires_single_official_domain=true
  client_reject_reason=single_official_domain_forbidden

manifest seed candidates handed to Cut 33 policy:
  node_receives_minimal_candidate_set=true
  candidate_endpoint_encrypted_or_blinded=true
  raw_ip_directory_exposed=false
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=signed-bootstrap-manifest-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/signed-bootstrap-manifest-matrix-cut34.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
bootstrap_manifest_signature_valid=true
full_raw_ip_directory_embedded=false
full_raw_ip_directory_exposed=false
centralized_control_plane_required=false
single_official_relay_required=false
single_official_domain_required=false
relay_is_trusted_authority=false
peer_identity_source=novovm_key
routing_subject=target_peer_id
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
real_multi_source_bootstrap_smoke=false
```

Next frontier:

```text
Cut 35:
  Bootstrap Source Resolver / Cache Fallback v0
```

## Cut 35: Bootstrap Source Resolver / Cache Fallback v0

Status:

```text
LOCAL BOOTSTRAP SOURCE RESOLVER MATRIX PASS
REAL MULTI-SOURCE BOOTSTRAP RESOLVER SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Define a deterministic first-start bootstrap source resolver.

Signed bootstrap manifests can come from cache, installer bundle, QR invite,
friend invite, official signed source, community signed source, or a discovered
blinded directory source. The resolver must prefer fresh local cache when safe,
skip expired or invalid sources, avoid making official sources mandatory, and
merge valid sources through the Cut 33 blinded directory policy.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=bootstrap-source-resolver-matrix
```

Resolver fallback order:

```text
1. local_cache
2. embedded_install_manifest
3. qr_invite_manifest
4. friend_invite_manifest
5. official_signed_bootstrap_manifest
6. community_signed_bootstrap_manifest
7. discovered_blinded_directory_source
```

Local resolver matrix:

```text
valid cache preferred when fresh:
  selected_bootstrap_manifest_source=local_cache

expired cache skipped:
  reject_reason=bootstrap_manifest_expired
  selected_bootstrap_manifest_source=embedded_install_manifest

invalid signature source rejected:
  reject_reason=bootstrap_manifest_signature_invalid

official source not mandatory:
  official_source_required=false
  selected source may be friend/community invite

multi-source merge does not expose raw IP directory:
  multi_source_merge_exposes_raw_ip_directory=false
  merged candidates remain blinded

fallback order deterministic:
  local_cache -> embedded_install_manifest -> qr_invite_manifest -> official_signed_bootstrap_manifest

no reachable bootstrap source:
  selected_path_after_bootstrap=QueueFallback
  fallback_reason=NoReachableBootstrapSource
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=bootstrap-source-resolver-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/bootstrap-source-resolver-matrix-cut35.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
bootstrap_source_resolver_enabled=true
valid_cache_preferred_when_fresh=true
expired_cache_skipped=true
invalid_signature_source_rejected=true
official_source_required=false
multi_source_merge_exposes_raw_ip_directory=false
fallback_order_deterministic=true
centralized_control_plane_required=false
single_official_relay_required=false
single_official_domain_required=false
relay_is_trusted_authority=false
peer_identity_source=novovm_key
routing_subject=target_peer_id
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
real_multi_source_bootstrap_resolver_smoke=false
```

Next frontier:

```text
Cut 36:
  NAT Auto Diagnosis + Safe Fallback v0
```

## Cut 36: NAT Auto Diagnosis + Safe Fallback v0

Status:

```text
LOCAL NAT AUTO DIAGNOSIS SAFE FALLBACK MATRIX PASS
REAL MIXED NAT / VPN / CELLULAR FALLBACK SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
```

Goal:

```text
Do not treat NAT punch as a basic connectivity prerequisite.

NAT punch remains an optimization path. If punch succeeds, upgrade to
PunchedDirect. If punch fails, classify the failure and safely fall back to
RelayNovoRudp when a relay candidate exists, or QueueFallback when no healthy
relay candidate exists. Never misclassify timeout, VPN/TUN, CGNAT, or nonce
mismatch as reachable direct connectivity.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=nat-auto-adaptive-matrix
```

Runtime prober behavior update:

```text
nat-punch prober timeout / recv failure:
  punch_ack_valid=false
  punch_result=failed
  nat_failure_classification=UdpReachabilityBlockedOrAckReturnFailed
  selected_path_after_punch=RelayNovoRudp if relay candidate exists
  selected_path_after_punch=QueueFallback if no relay candidate exists
  process exits accepted=true for safe fallback reports
```

Local auto-adaptive matrix:

```text
punch success upgrades to direct:
  selected_path_after_punch=PunchedDirect

UDP timeout with relay:
  nat_failure_classification=UdpReachabilityBlockedOrAckReturnFailed
  selected_path_after_punch=RelayNovoRudp

UDP timeout without relay:
  selected_path_after_punch=QueueFallback

nonce mismatch:
  nat_failure_classification=StaleOrMismatchedPunchAck
  reachability_misclassified_as_direct=false

VPN/TUN or CGNAT:
  punch_required_for_connectivity=false
  selected_path_after_punch=RelayNovoRudp when relay candidate exists

relay unavailable after NAT failure:
  selected_path_after_punch=QueueFallback
  fallback_reason=NoHealthyNetworkPath
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=nat-punch-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/nat-punch-matrix-cut25-regression.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=nat-auto-adaptive-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/nat-auto-adaptive-matrix-cut36.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
adaptive_auto_networking_complete=false
nat_punch_is_availability_prerequisite=false
nat_punch_is_optimization_path=true
relay_first_zero_config_required=true
manual_user_port_forward_required=false
vpn_tun_supported_by_policy=true
failure_is_diagnosed_before_route_selection=true
safe_fallback_without_false_reachable=true
novorudp_wire_changed=false
real_mixed_nat_vpn_cellular_fallback_smoke=false
```

Next frontier:

```text
Cut 37:
  Headless Public Relay Deploy Package v0
```

## Cut 37: Headless Public Relay Deploy Package v0

Status:

```text
LOCAL HEADLESS PUBLIC RELAY DEPLOY PACKAGE MATRIX PASS
REAL PUBLIC VPS RELAY RUNTIME SMOKE PENDING
```

Implemented in:

```text
crates/novovm-node/src/bin/supervm-network-overlay-gate.rs
scripts/novovm-headless-public-relay-package.ps1
```

Goal:

```text
Separate public relay runtime from the development environment.

The development machine builds and packages the relay binary. A public VPS only
needs the binary, relay.config.json, run scripts, checksums, and a reports
directory. It does not need VS Code, Codex, Rust toolchain, or a full git
workspace.
```

New gate mode:

```text
NOVOVM_OVERLAY_GATE_MODE=headless-public-relay-deploy-package-matrix
```

Package layout:

```text
novovm-public-relay-v0/
  supervm-network-overlay-gate(.exe)
  relay.config.json
  run-relay.sh
  run-relay.ps1
  README.md
  CHECKSUMS.txt
  reports/
```

relay.config.json:

```text
mode=public-relay-bootstrap
role=relay
node_id=public-relay-1
bind_addr=0.0.0.0:41030
report_path=reports/public-relay-1.json
payload_treated_opaque=true
relay_is_trusted_authority=false
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
```

Local package matrix:

```text
package_created=true
binary_present=true
config_present=true
run_relay_sh_present=true
run_relay_ps1_present=true
readme_present=true
checksum_written=true
reports_dir_present=true
rust_toolchain_required=false
vscode_required=false
codex_required=false
full_git_workspace_required=false
relay_start_command_documented=true
boundary_fields_preserved=true
```

Validation commands:

```text
cargo fmt --check
cargo check -q -p novovm-node --bin supervm-network-overlay-gate

NOVOVM_OVERLAY_GATE_MODE=headless-public-relay-deploy-package-matrix \
NOVOVM_OVERLAY_GATE_REPORT_PATH=artifacts/network-overlay-gate/headless-public-relay-deploy-package-matrix-cut37.json \
cargo run -q -p novovm-node --bin supervm-network-overlay-gate

powershell -ExecutionPolicy Bypass -File scripts/novovm-headless-public-relay-package.ps1
```

Boundary:

```text
network_only=true
payload_treated_opaque=true
headless_deploy_package=true
relay_is_trusted_authority=false
business_semantics_interpreted_by_relay=false
novorudp_wire_changed=false
real_public_vps_relay_runtime_smoke=false
```

## Product Readout

Current product status:

```text
NativeTransfer performance path: signed.
Network performance bottleneck: external link confirmed.
Production network resilience: next active frontier.
```

Short conclusion:

```text
NOVOVM is past the "can it execute fast" question for NativeTransfer.
The next product-grade problem is "can nodes find and reach each other under real networks,
NAT, weak links, unstable ports, and blocking".
```
