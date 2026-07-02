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
