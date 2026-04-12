# NOVOVM-NETWORK-PHASE-B-CANDIDATE-AVAILABILITY-2026-04-08

## 1. 文档定位

本文不是正式实施方案，也不是 transport 强化方案。  
本文用于定义 NOVOVM Network 在 Phase A 完成后的下一阶段候选主线之一：

> Availability Plane（可用性面）

其目标是在受限或不稳定网络环境下，使系统具备：

1. 可降级
2. 可排队
3. 可回放
4. 可对账
5. 可审计

本文与 RFC-0 的关系：

1. RFC-0 已冻结网络主线，明确当前顺序为：
   - Topology Plane
   - Availability Plane
   - Relay Data Plane
   - Capability Import
   - Transport 强化
2. 本文聚焦 Availability Plane，不扩展 relay data plane，不讨论 ACP / camouflage / anti-DPI。

## 2. 目标

Phase B 候选目标：

1. 将当前已出现于主线路由语义中的 `queue_only`，落成真实可运行的 availability 行为
2. 将 `ReadOnly` / `QueueOnly` / `Store-and-Forward` / `replay` / `reconcile` / `audit` 从文档概念推进为主线实现窗口
3. 在不扩大 transport 边界的前提下，让系统在网络受限时继续可信运行

## 3. 非目标

本阶段明确不做：

1. 不做 relay data plane
2. 不做 ACP / adaptive transport
3. 不做 camouflage / anti-DPI
4. 不做 discovery / mesh / NAT assist
5. 不做 `SVM2026/l4-network` capability import
6. 不重写 transport 主干

## 4. 与 Phase A 的衔接点

Phase A 已完成：

1. `routing/` 落盘
2. `L4LocalRoutingTable`
3. `L3RegionalRoutingTable`
4. `RouteSelector`
5. `selector -> overlay_route_*` 接线
6. `LocalObserved` 白名单两条成立
7. freshness 已成立
8. 定向 `check` / `test` 已通过

当前与 Availability Plane 直接相关的衔接点有：

1. `SelectedPath::ReadOnlyQueue`
2. `OverlayAvailabilityMode`
3. `queue_only` 已进入主线路由 / 落标语义，但尚未成为完整执行链

因此，Phase B 的作用不是引入新概念，而是把已有语义变成真实能力。

## 5. 基础原则

### 5.1 Availability Plane 不改变 Topology Plane

路由仍由以下组件决定：

1. `L4LocalRoutingTable`
2. `L3RegionalRoutingTable`
3. `RouteSelector`

Availability Plane 只回答：

> 当路径不可用、路径不足、或系统主动降级时，如何继续运行。

### 5.2 先行为闭环，后体验优化

先保证：

1. 正确降级
2. 正确入队
3. 正确回放
4. 正确对账

不追求一开始就做复杂 UI 或控制面。

### 5.3 先 node / gateway 主路径，后扩展

优先绑定：

1. `novovm-node`
2. `evm-gateway`

不扩到更多客户端侧变体。

## 6. Phase B 最小实现窗口

### 6.1 ReadOnly

硬语义：

1. 允许：读操作
2. 禁止：新写执行
3. 行为：
   - 写请求拒绝
   - 返回明确降级原因
   - 不进入上游执行路径

### 6.2 QueueOnly

硬语义：

1. 允许：接收写请求
2. 不允许：立即提交上游
3. 行为：
   - 写请求进入本地持久队列
   - 记录幂等键
   - 等待 replay

### 6.3 Store-and-Forward

最小要求：

1. 本地持久队列
2. 幂等键
3. 可枚举 pending items
4. 可按顺序 replay
5. replay 失败可重试

### 6.4 Replay

最小要求：

1. 出队前幂等查重
2. 成功后标记 completed
3. 失败可重试
4. 具备最大重试 / 退避策略

### 6.5 Reconcile

最小要求：

1. 本地状态 vs 上游状态差异检查
2. 能识别：
   - 已提交成功
   - 待补交
   - 已过期 / 已拒绝
3. 不要求第一阶段就做复杂自动修复

### 6.6 Audit

最小要求：

1. 记录 availability mode 切换
2. 记录 queue / replay 关键事件
3. 记录最终处理结果
4. 保持脱敏与最小元数据原则

## 7. 建议代码落点

建议新增但保持极简：

### 7.1 `novovm-node`

负责：

1. 根据 `SelectedPath` / availability 状态，决定：
   - 读
   - 拒绝
   - 入队
   - replay 触发

### 7.2 `novovm-network`

本阶段不扩 transport，只允许承载：

1. availability mode 枚举
2. 极少量与 queue / replay 对接的辅助类型

### 7.3 `evm-gateway`

若当前已有合适入口，可作为：

1. QueueOnly 的写请求入口
2. replay 的执行出口

## 8. 最小数据结构建议

### 8.1 AvailabilityMode

```rust
enum OverlayAvailabilityMode {
    Normal,
    ReadOnly,
    QueueOnly,
}
```

### 8.2 QueuedRequest

```rust
struct QueuedRequest {
    request_id: String,
    idempotent_key: String,
    created_unix_ms: u64,
    payload: Vec<u8>,
    retry_count: u32,
}
```

### 8.3 ReplayResult

```rust
enum ReplayResult {
    Applied,
    DuplicateIgnored,
    RetryLater,
    PermanentlyRejected,
}
```

## 9. 成功标准

本阶段若推进，成功标准只认以下几条：

1. `ReadOnly` 与 `QueueOnly` 成为真实行为，而不只是语义标签
2. `queue_only` 可驱动本地持久队列写入
3. replay 能完成最小幂等回放
4. reconcile 能识别最小成功 / 失败 / 待补交状态
5. audit 可记录 mode 切换与 queue / replay 关键事件

## 10. 当前未做项

明确保留到后续阶段：

1. relay data plane
2. discovery / mesh
3. NAT assist
4. libp2p capability import
5. transport 强化
6. ACP / camouflage

## 11. 推荐顺序

如果 Phase B 选择 Availability Plane 作为下一主线，则建议顺序为：

1. `ReadOnly` / `QueueOnly` 行为落地
2. 本地持久队列
3. replay
4. reconcile
5. audit
6. 再评估是否进入 relay data plane

## 12. 一句话结论

> Phase A 解决了“怎么选路径”，  
> Phase B（Availability Plane）要解决“路径不足时系统怎么继续可信运行”。
