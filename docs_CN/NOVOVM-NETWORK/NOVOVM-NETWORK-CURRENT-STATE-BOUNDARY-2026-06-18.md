# NOVOVM Network Current State Boundary

日期：2026-06-18

## 一、当前边界

NOVOVM-NETWORK 现阶段已完成网络骨架、路由 / relay / availability 分层、native pending UDP 跨机器闭环验证；但尚未完成 libp2p 控制面、NovoRUDP 数据面、无 IP 身份寻址与抗审查 overlay。

当前 UDP repair 可以冻结为 diagnostic / compatibility profile；这不是签收 30min sustained PASS，也不是把裸 UDP repair 定为主网终态协议。

## 二、已完成范围

```text
1. Network module layering
2. Availability Plane
3. Routing Plane
4. Relay control / relay policy
5. Transport abstraction: TCP / UDP skeleton
6. Native pending tx broadcast over transport
7. Cross-machine UDP clean / fault / sustained soak gates
8. Runtime observability / gate / SOP
9. AOEM pending -> receipt 跨机器闭环验证
```

代码侧对应：

```text
crates/novovm-network/src/transport.rs
- Transport trait
- UdpTransport
- TcpTransport
- native pending tx broadcast dispatch
- native pending tx repair probe / remote payload observation

crates/novovm-network/src/availability/*
- queue / replay / reconcile / availability decision

crates/novovm-network/src/routing/*
- L3/L4 route selection
- relay scoring / route hints

crates/novovm-network/src/relay/*
- minimal relay frame / client / server skeleton

crates/novovm-network/src/capability/*
- capability hints / libp2p stub readiness
```

## 三、未完成范围

```text
1. libp2p PeerID / DHT / Identify / AutoNAT / Circuit Relay
2. 无 IP 身份寻址控制面
3. 生产级 relay data plane / multi-hop
4. NovoRUDP windowed reliable repair
5. 抗 DPI / camouflage / censorship-resistance transport profile
6. 内容地址化存储 overlay
7. 完整自组网主网网络层
```

## 四、下一阶段路线

```text
Phase 1：冻结当前 UDP baseline
- 保留为 diagnostics / compatibility profile
- 默认 NOVOVM_NATIVE_PIPELINE_TRANSPORT=udp
- 不继续把旧 UDP repair 做成主网终态协议

Phase 2：NovoRUDP Windowed Repair v0
- 显式 NOVOVM_NATIVE_PIPELINE_TRANSPORT=novorudp
- receiver-driven current missing window
- ACK missing window
- windowed repair
- pacing / no-progress backoff
- final missing 收敛

Phase 3：libp2p Control Plane v0
- PeerID / NodeID
- DHT
- Identify
- AutoNAT
- Circuit Relay
- capability exchange
- RouteSet discovery

Phase 4：NovoRUDP Mainnet Data Plane v0
- tx broadcast
- block fragment
- receipt repair
- AOEM-native ingress transport
- relay_udp / direct_udp

Phase 5：Anti-censorship / no-IP overlay profile
- NodeID 身份寻址
- relay registry
- route token
- multi-hop relay
- content addressing
- storage overlay
```

## 五、最终架构定位

```text
libp2p Control Plane
+ NovoRUDP Data Plane
+ Relay / Overlay Routing
+ Content / Node Identity Addressing
```

其中：

```text
libp2p = 找节点、认节点、DHT、relay、NAT traversal、能力声明
NovoRUDP = 高频交易传播、ACK bitmap、window repair、AOEM-native data path
Relay / Overlay = 中继、多路径、弱位置隐藏、抗单点封禁
```

一句话：之前做的是 NOVOVM 网络骨架和 native pipeline UDP 验证；下一阶段才是 libp2p 控制面 + NovoRUDP 主网数据面。
