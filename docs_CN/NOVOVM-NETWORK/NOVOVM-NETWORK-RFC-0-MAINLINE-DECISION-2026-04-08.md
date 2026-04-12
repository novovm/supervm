# NOVOVM RFC-0
# Network Mainline Decision（网络主线决策）

## 1. 背景与现状

当前 NOVOVM / SUPERVM 网络能力来自三条历史路径：

1. 现行主干（SUPERVM）
   - `crates/novovm-network`
   - 最小 transport / gossip / route 分类
   - `overlay_route_*` 元数据（当前主要来自参数与策略）

2. SVM2026 真实基础
   - `src/l4-network`
   - `libp2p / DHT / gossipsub / CID / gateway`
   - 具备真实 P2P backplane 能力

3. SVM2026 原型层
   - `src/network/communication/*`
   - 多通讯选择 / mesh / 邻居发现（demo / prototype）

同时存在一套四层自组网设计文档（L1-L4 routing table / relay / NAT / fallback），但未形成代码闭环。

当前状态：

> 真实最小网络主干 + 原型化 mesh + 文档化四层自组网设计，并存但未收敛。

## 2. 核心问题

当前网络系统的本质缺口不是 transport，而是：

> 缺少“自组网拓扑闭环”。

具体表现为：

1. 无真实 `L4LocalRoutingTable`
2. 无真实 `L3RegionalRoutingTable`
3. relay 存在控制面但缺数据面
4. `overlay_route_*` 不来源于真实路径选择
5. fallback hierarchy 未落地
6. NAT assist 未实现
7. `l4-network` 未被吸收为主干能力

结论：

> 问题不是“连不出去”，而是“系统不知道该连谁、怎么连”。

## 3. 不采用项

以下方向暂不进入主线：

1. 新开 ACP / Anti-Censorship 主线
2. `TLS mimic / camouflage / anti-DPI` 实现
3. 复活 `network/communication` 作为主实现入口
4. 多通讯驱动集成（LoRa / Starlink 等）
5. 构建平行 transport 栈
6. 将“神经网络式自组织通信”当作当前已实现能力

所有不采用项在 Phase E 之前不得进入主线。

原则：

> 先补拓扑与可用性闭环，再谈 transport 强化。

## 4. 唯一网络主干原则

### 4.1 主干

> 唯一网络主干：`crates/novovm-network`

所有网络能力必须围绕它收敛。

### 4.2 能力吸收策略

#### 保留（SUPERVM）

1. transport（TCP / UDP）
2. gossip
3. `overlay_route_*` 元数据
4. relay 控制面（`novovm-rollout-policy/overlay`）

#### 吸收（SVM2026）

1. `l4-network`
   - `libp2p`
   - `kad (DHT)`
   - `gossipsub`
   - `mdns`
   - `request-response`
   - `CID announce / fetch`

原则：

> 吸收能力，不回迁 crate，不复制语义。

`SVM2026/l4-network` 只做 capability import，不做 crate 级回迁。

### 4.3 冻结（仅参考）

1. `network/communication/*`
2. `mesh.rs`
3. `hardware.rs`

定性：

> 仅作为邻居发现 / route hint 的设计参考，不再作为主线实现。

## 5. 网络分层模型

### 5.1 Data Plane（数据面）

- 由 `novovm-network/transport.rs` 承担
- 后续按 feature 吸收 libp2p backplane

### 5.2 Topology Plane（拓扑面）

新增：

- `routing/`
  - `L4LocalRoutingTable`
  - `L3RegionalRoutingTable`
  - `RouteSelector`
  - `RoutingSummary / PathHint`

职责：

1. 路由记忆
2. 区域视图
3. relay 选择
4. fallback 决策

Topology Plane 是当前主线开发重点。

### 5.3 Control Plane（控制面）

复用：

- `novovm-rollout-policy/overlay`
  - relay discovery
  - relay health
  - region policy

输出：

- feed -> `L3RegionalRoutingTable`

### 5.4 Availability Plane（可用性面）

来自：

- 《受限网络下的可用性设计》

实现：

1. `ReadOnly`
2. `QueueOnly`
3. `Store-and-Forward`
4. 幂等回放
5. 对账
6. 审计

### 5.5 Overlay Semantics Plane

当前：

- `overlay_route_*` 由参数生成

目标：

> 必须改为由 `RouteSelector` 输出驱动。

## 6. overlay_route_* 生成优先级

当前 `overlay_route_*` 字段不再单一来源于 env 或 deterministic 逻辑，必须按以下优先级生成：

1. `RouteSelector` 输出（主来源）
2. `operator forced override`（显式人工或策略强制）
3. `legacy env / deterministic fallback`（最后兜底）

说明：

1. Phase A 初期 routing 状态不完整，必须保留 fallback
2. 禁止直接切断 legacy 路径来源

## 7. 可用性降级语义（硬定义）

### ReadOnly

1. 允许：读操作
2. 禁止：新写执行
3. 行为：写请求直接拒绝或返回降级提示

### QueueOnly

1. 允许：接收写请求
2. 行为：
   - 写请求落本地持久队列
   - 不立即提交上游
3. 后续：通过 replay 机制恢复

说明：

1. 两者语义必须在代码层显式区分
2. 不允许混用或模糊实现

## 8. Routing 数据来源可信度分级

routing 信息必须标注来源可信度，至少区分：

1. `local_observed`
   - 本节点直接观测（连接 / 延迟 / 成功路径）

2. `peer_hinted`
   - 其他节点提供的 hint（L4-L4）

3. `regional_announced`
   - L3 区域汇总广播

4. `operator_forced`
   - 人工或策略强制注入

要求：

1. routing table 中必须保留来源字段
2. selector 在路径选择时可考虑来源权重

## 9. 术语说明：神经网络式自组织通信

该术语仅用于描述长期设计目标：

1. 作为 `design goal / research direction`
2. 不代表当前系统能力
3. 不作为任何模块命名或功能承诺

当前实现仅覆盖：

1. 局部路由记忆
2. 区域视图
3. 路径选择

## 10. 四层网络在代码中的落点

| 层 | 职责 |
| --- | --- |
| L4 | 本地路由记忆 / L4-L4 hint / queue / degrade |
| L3 | 区域 relay pool / routing summary / NAT assist（后续） |
| L2 | 稳定执行与同步 backbone |
| L1 | 路由治理与审计锚点 |

## 11. 实施阶段

Phase A 为唯一当前主线，其它阶段不得提前进入。

### Phase A：拓扑闭环（当前阶段）

目标：

> 让代码中第一次出现真实四层 routing 结构。

实施：

- 新增：
  - `routing/types.rs`
  - `routing/l4_local.rs`
  - `routing/l3_regional.rs`
  - `routing/selector.rs`
  - `routing/sync.rs`

- 实现：
  - `L4LocalRoutingTable`
  - `L3RegionalRoutingTable`
  - `SelectedPath`

成功标准：

1. 不再只靠 env 决定路径
2. `overlay_route_*` 可以绑定 selector 输出
3. `direct / relay / upstream / queue` 有统一决策结构

### Phase B：可用性闭环

目标：

> 网络受限时系统继续可信运行。

实施：

- 落地：
  - `ReadOnly`
  - `QueueOnly`
  - `Store-and-Forward`
  - 幂等回放
  - 对账

成功标准：

1. 网络中断不再等于失败
2. `queue + replay + reconcile` 可运行

### Phase C：L3 relay 数据面

目标：

> 让 relay 从“控制面”变成“可用路径”。

实施：

- 基于现有 relay policy
- 增加最小 relay forwarding（direct + single relay）

成功标准：

1. `SelectedPath::L3Relay` 可真实走通
2. relay health 影响选路

### Phase D：吸收 libp2p backplane

目标：

> 升级 `novovm-network` 为真实 P2P 网络。

实施：

- capability import：
  - `libp2p`
  - `kad`
  - `gossipsub`
  - `mdns`

成功标准：

1. peer discovery / DHT / pubsub 可运行
2. 不产生第二套网络语义

### Phase E：transport 强化（最后）

目标：

> 仅作为优化层，不作为主线。

实施：

1. adaptive transport prototype
2. `direct + relay` 优化

暂不进入：

1. camouflage
2. anti-DPI

## 12. 第一刀代码落点

```text
crates/novovm-network/src/routing/
  types.rs
  l4_local.rs
  l3_regional.rs
  selector.rs
  sync.rs
```

优先实现：

1. `L4LocalRoutingTable`
2. `L3RegionalRoutingTable`
3. `RouteSelector`

## 13. 成功标准

1. 代码里第一次有真实 `L4/L3 routing table`
2. `overlay_route_*` 不再只靠 env
3. 路径选择首次进入主执行路径
4. transport 栈没有分叉
5. `SVM2026` 的真实网络能力有明确吸收挂点
6. routing 来源可信度字段已落入代码结构

## 14. 一句话主线

> NOVOVM 当前网络主线不是“抗封 transport”，而是以 `novovm-network` 为唯一主干，吸收 `l4-network` 的真实 P2P 能力，先补“路由记忆 + 区域视图 + fallback 闭环”，再逐步构建 relay 与 transport 能力。

## 15. 最终判断

这份 RFC-0 的意义不是描述未来，而是：

> 冻结“现在该做什么、不该做什么”的网络主线。

它解决的不是技术细节，而是：

1. 不再分叉网络路线
2. 不再重复造轮子
3. 不再被 ACP / demo / 愿景牵着走

## 16. AI 执行规范绑定（2026-04-08）

网络主线验证方式绑定以下规范文档：

1. `docs_CN/NOVOVM-NETWORK/NOVOVM-NETWORK-AI-BEHAVIOR-SPEC-2026-04-08.md`

硬约束：

1. 生产级行为验证必须使用 Rust（`tests/*.rs` 或 `src/bin/*.rs`）
2. `ps1` 可用于辅助运维，不可作为主线能力成立证据
3. Phase B `QueueOnly -> restart -> replay` 节点级 smoke 必须落在 Rust 集成测试

## Phase A 当前状态（2026-04-08 after validation）

当前已完成：

1. `crates/novovm-network/src/routing/` 第一版已落盘：
   - `types.rs`
   - `l4_local.rs`
   - `l3_regional.rs`
   - `selector.rs`
   - `sync.rs`
2. `RouteSelector` 已接入 `novovm-node` 的 `overlay_route_*` 决策链。
3. `overlay_route_*` 生成优先级已切成：
   - `operator override`
   - `selector`
   - legacy 配置层兼容输入
4. `L4/L3` 最小状态灌入已成立：
   - `L3RegionalRoutingTable` 可由 region / relay candidate seed
   - `L4LocalRoutingTable` 可由 `NOVOVM_NET_PEERS` hint seed
5. `LocalObserved` 两条白名单已成立：
   - `peer_addr_index` 精确命中
   - `decoded sender id + exact registered addr`
6. `hint -> evidence` 升级链已成立：
   - `OperatorForced + Unknown`
   - `LocalObserved + Reachable`
7. freshness 已成立：
   - `LocalObserved + Reachable` 超过 freshness window 后降为 `LocalObserved + Unknown`

当前验证结果：

1. `cargo check -p novovm-network` 通过
2. `cargo check -p novovm-node` 通过
3. `cargo test -p novovm-network --lib` 通过（`72 passed; 0 failed`）
4. 最小行为验证已覆盖并通过：
   - hint-only 不会误造 `DirectL4`
   - `LocalObserved + Reachable` 可驱动 `DirectL4`
   - freshness 过期后 `DirectL4` 退出
   - 两条白名单都能进入 `LocalObserved`
   - 非精确地址不会误升为 `LocalObserved`

当前仍未进入主线的事项：

1. 第三条及以后强证据来源
2. discovery / mesh / NAT assist
3. relay data plane
4. availability plane 执行链
5. `SVM2026/l4-network` capability import

## Phase B.0 当前状态（after queue_replay_smoke validation）

在 Phase A（拓扑与路径选择闭环）完成并冻结后，Phase B 已进入最小可恢复能力验证阶段。

当前已完成：

1. Availability 主路径接线
   - `SelectedPath -> AvailabilityDecision` 已接入 `novovm-node`
2. 分流行为成立
   - `ReadOnly / QueueOnly / Normal` 已形成真实执行路径分支
3. 队列接口化完成
   - `QueueStore`
   - `InMemoryQueueStore`
   - `FileQueueStore`
4. Rust 集成 smoke 已落地
   - `crates/novovm-node/tests/queue_replay_smoke.rs`
5. 节点级最小生存链验证通过
   - `QueueOnly 入队`
   - `重启后 pending 保留`
   - `replay_on_start 消费 pending`
   - `Applied 后队列文件删除`

验证命令：

- `cargo test -p novovm-node queue_replay_smoke`

验证结果：

- `1 passed; 0 failed`
- Phase B.1：最小 replay 可观测性已落地（`applied / retry_later / rejected`），并已接入 `novovm-node` 日志输出。
- Phase B.2：最小 reconcile 只读报告已落地（`pending / applied / unknown`），并已接入 `novovm-node` 日志输出。
- Phase B.3：replay + reconcile 最小增强已落地（细分统计与状态汇总日志）。
- Phase C.0：最小 L3 relay data plane 行为已验证（single relay roundtrip、`l3_relay` 分支执行、`availability_relay` 日志可观察）。
- Phase C.1：relay 最小增强已落地（`l3_relay` 失败时支持受控 `direct_once` 回退，`availability_relay_path` 日志可观察）。
- Phase C.2：relay 质量反馈可解释性已验证（`availability_relay_score` 输出 `selected / health / score / delta / final_score`，并与 selector 排序一致）。
- Phase D.0：capability import 最小接线已落地（backend detect + `availability_capability` 日志可观察）。
- Phase D.1：capability 只读影响面已落地（`availability_capability_state` 与 `availability_state` 增加 `capability_readiness / capability_summary`，不改变主行为）。
- Phase D.2：capability advisory 只读影响面已落地（`availability_capability_advisory` 与 `availability_state` 增加 `capability_route_hint / capability_availability_hint`，`binding=false` 不改变主行为）。
- Phase D.3：capability 只读导出已落地（L1/L4 anchor record 增加 `capability_readiness / capability_summary / capability_route_hint / capability_availability_hint`，不改变主行为）。
- Phase D.4：capability 只读增强已落地（`capability_token` 已接入日志、`availability_state` 与 L1/L4 anchor record）。
- Phase E.0：capability-policy 只读解释接线已落地（`availability_capability_policy` + `availability_state` + L1/L4 anchor record 增加 `capability_policy_mode / capability_route_adopted / capability_availability_adopted`，`advisory_first` 且不改变主行为）。
- Baseline v5：A + B + C.2 + D.4 + E.0 为当前稳定基线，后续改动不得破坏该闭环。

---

### 阶段定性

- Phase A：completed / validated / frozen
- Phase B：进入 B.0（最小可恢复闭环已验证）

---

### 当前未进入范围（明确边界）

1. replay 批量/重试策略增强
2. reconcile 自动修复
3. L3 relay data plane
4. capability import（libp2p 等）
5. transport 强化（QUIC / camouflage / ACP）

---

## 单一系统依赖口径（2026-04-11）

1. `SuperVM` 四层网络是唯一网络层：`L4 -> L3 -> L2 -> L1`，禁止并行“第二套网络主线”。  
2. `relay / multi-hop / overlay_route_*` 统一归属 L3 能力线，只能在四层主线上增量演进。  
3. 运维入口（`novovmctl` 与兼容壳）是消费层，不得定义或覆盖四层网络策略语义。  
4. 节点运行时必须维持统一状态源：`availability_state`、`availability_l3_readonly`、L1/L4 anchor 同版本同锁口径导出。  
