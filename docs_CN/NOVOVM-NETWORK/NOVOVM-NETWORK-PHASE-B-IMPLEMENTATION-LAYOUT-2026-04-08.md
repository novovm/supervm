# NOVOVM-NETWORK-PHASE-B-IMPLEMENTATION-LAYOUT-2026-04-08

## 1. 文档定位

本文不是新的 RFC，也不是直接实施代码。  
本文用于把 Phase B 候选线 `Availability Plane` 压成可执行的代码落点和边界约束。

本文服务于两个目标：

1. 防止 `availability` 与 `routing` / `transport` 混写
2. 为下一步 skeleton 和最小实现提供稳定文件落点

## 2. 实施原则

### 2.1 模块边界

`Availability Plane` 只消费 `SelectedPath`，不重新做 routing。

职责边界：

1. `routing`
   - 负责路径选择
   - 输出 `SelectedPath`

2. `availability`
   - 负责把 `SelectedPath` 转换成可用性决策
   - 决定：
     - `Normal`
     - `ReadOnly`
     - `QueueOnly`

3. `transport`
   - 不承载 queue / replay / reconcile

### 2.2 当前阶段禁止项

以下内容本阶段不得进入：

1. 不把 queue 写进 `routing`
2. 不把 replay 写进 `transport`
3. 不在 `novovm-node` 中散落 queue 逻辑
4. 不做 relay data plane
5. 不做复杂持久化
6. 不做 ACP / adaptive transport

## 3. 代码落点

建议目录：

```text
crates/novovm-network/src/
  transport/         # 已有，不动
  routing/           # Phase A，已完成
  availability/      # Phase B，新增
    mod.rs
    mode.rs
    decision.rs
    controller.rs
    queue.rs
    replay.rs
    reconcile.rs
    audit.rs
```

`novovm-node` 只作为调用入口，不承载 availability 内部实现。

## 4. 各文件职责

### 4.1 `availability/mod.rs`

职责：

1. 统一导出 availability 模块公共类型
2. 维持模块边界

建议导出：

1. `AvailabilityMode`
2. `AvailabilityDecision`
3. `AvailabilityController`
4. `QueuedRequest`
5. `ReplayResult`

### 4.2 `availability/mode.rs`

职责：

1. 定义可用性模式

建议结构：

```rust
pub enum AvailabilityMode {
    Normal,
    ReadOnly,
    QueueOnly,
}
```

### 4.3 `availability/decision.rs`

职责：

1. 定义可用性决策结构
2. 作为 `SelectedPath -> AvailabilityDecision` 的标准输出

建议结构：

```rust
pub struct AvailabilityDecision {
    pub mode: AvailabilityMode,
    pub reason: &'static str,
}
```

### 4.4 `availability/controller.rs`

职责：

1. 提供唯一入口
2. 消费 `SelectedPath`
3. 输出 `AvailabilityDecision`

建议结构：

```rust
pub struct AvailabilityController;

impl AvailabilityController {
    pub fn decide(&self, selected_path: &SelectedPath) -> AvailabilityDecision {
        match selected_path {
            SelectedPath::DirectL4(_)
            | SelectedPath::L4Relay(_)
            | SelectedPath::L3Relay(_)
            | SelectedPath::ForcedUpstream(_) => AvailabilityDecision {
                mode: AvailabilityMode::Normal,
                reason: "path_available",
            },
            SelectedPath::ReadOnlyQueue => AvailabilityDecision {
                mode: AvailabilityMode::QueueOnly,
                reason: "no_path",
            },
        }
    }
}
```

规则：

1. `availability` 不得反向修改 `SelectedPath`
2. `availability` 不得重新判断 route

### 4.5 `availability/queue.rs`

职责：

1. 提供最小本地队列抽象
2. 第一版只做内存队列或极薄持久化接口

建议结构：

```rust
pub struct QueuedRequest {
    pub request_id: String,
    pub idempotent_key: String,
    pub payload: Vec<u8>,
    pub created_unix_ms: u64,
    pub retry_count: u32,
}

pub struct SimpleQueue {
    inner: Vec<QueuedRequest>,
}
```

当前阶段不做：

1. RocksDB
2. SQLite
3. 多队列分片
4. 分布式一致性

### 4.6 `availability/replay.rs`

职责：

1. 定义 replay 结果与最小 replay 驱动接口

建议结构：

```rust
pub enum ReplayResult {
    Applied,
    DuplicateIgnored,
    RetryLater,
    PermanentlyRejected,
}
```

当前阶段不做复杂调度，只保留最小顺序 replay 能力。

### 4.7 `availability/reconcile.rs`

职责：

1. 定义本地状态与上游状态差异检查接口
2. 第一版只做最小状态分类，不做自动修复

建议输出类型至少覆盖：

1. 已提交成功
2. 待补交
3. 已过期
4. 已拒绝

### 4.8 `availability/audit.rs`

职责：

1. 记录 availability mode 切换
2. 记录 queue / replay 关键事件

第一版要求：

1. 只保留最小事件结构
2. 不做复杂审计后端

## 5. `novovm-node` 接入点

当前主路径已经有：

```text
RouteSelector -> SelectedPath
```

Phase B 接入只加一层：

```text
SelectedPath -> AvailabilityDecision
```

建议接入顺序固定为：

1. seed routing
2. `selector.select_best_path()`
3. `availability_controller.decide(...)`
4. 根据 `AvailabilityDecision.mode` 分支：
   - `Normal` -> 继续执行
   - `ReadOnly` -> 拒绝写
   - `QueueOnly` -> 入队

## 6. 第一版最小实现窗口

第一版只允许落以下内容：

1. `AvailabilityMode`
2. `AvailabilityDecision`
3. `AvailabilityController`
4. `QueuedRequest`
5. `SimpleQueue`
6. `ReplayResult`

当前阶段不要求：

1. 真实持久化
2. 完整 replay 流程
3. 完整 reconcile 流程
4. `evm-gateway` 全接线

## 7. 成功标准

若按本文进入 skeleton 或实施阶段，成功标准为：

1. `availability/` 作为独立模块落盘
2. `queue_only` 不再只是字符串，而有明确控制器输出
3. `novovm-node` 只通过 `AvailabilityController` 接 availability
4. `routing` / `transport` 不被 availability 逻辑污染
5. queue / replay / reconcile 的后续扩展位已明确

## 8. 推荐下一步

基于本文，下一步建议顺序为：

1. 先创建 `availability/` skeleton
2. 再把 `AvailabilityController` 接入 `novovm-node`
3. 再做最小内存队列
4. 最后再评估 replay / reconcile 的实现窗口

## 9. 一句话结论

> Phase B 的第一步不是立刻实现 queue / replay 全链路，  
> 而是先把 `Availability Plane` 的代码边界和唯一接入点锁住。

## 10. Rust 版节点级集成 smoke 任务单（QueueOnly -> restart -> replay）

### 10.1 任务目标

验证节点级最小生存链是否闭环：

1. `QueueOnly` 下真实写入 `FileQueueStore`
2. 重启后 pending 仍存在
3. `Normal + replay_on_start` 时 replay 消费 pending
4. `Applied` 后队列文件被删除

### 10.2 文件落点（强制）

1. `crates/novovm-node/tests/queue_replay_smoke.rs`

说明：

1. 该用例是主线能力验证，必须使用 Rust 集成测试
2. 禁止用 `ps1` 作为该能力的最终验证实现

### 10.3 最小实现步骤

1. 测试内创建临时 `QUEUE_DIR`
2. 第一阶段（入队）：
   - `NOVOVM_AVAILABILITY_QUEUE_DIR=<QUEUE_DIR>`
   - `NOVOVM_AVAILABILITY_FORCE_MODE=queue_only`
   - `NOVOVM_AVAILABILITY_REPLAY_ON_START=0`
   - 触发一条可入队请求
   - 断言：`QUEUE_DIR` 中 `*.json >= 1`
3. 第二阶段（重启回放）：
   - 复用同一 `QUEUE_DIR`
   - `NOVOVM_AVAILABILITY_FORCE_MODE=normal`
   - `NOVOVM_AVAILABILITY_REPLAY_ON_START=1`
   - 再次启动节点主流程
   - 断言：`QUEUE_DIR` 中 `*.json == 0`
4. 结果断言：
   - 至少一条请求被 `Applied`
   - 失败请求不应被误删（本用例可通过构造失败分支单独覆盖）

### 10.4 约束

1. 不改 transport 主行为
2. 不改 routing 策略逻辑
3. 不扩第三条强证据来源
4. 不引入 relay data plane

### 10.5 验收标准

1. `QueueOnly` 场景下节点真实写入文件队列
2. 进程重启后 pending 仍可见
3. replay 成功后 pending 被删除
4. 整条链不依赖内存态残留

## 11. Phase B.0 当前状态（after queue_replay_smoke validation）

当前已完成并验证的最小闭环：

1. `SelectedPath -> AvailabilityDecision` 已接入 `novovm-node` 主路径
2. `ReadOnly / QueueOnly / Normal` 已形成真实分流行为
3. `QueueOnly` 已从内存行为升级为接口化队列：
   - `QueueStore`
   - `InMemoryQueueStore`
   - `FileQueueStore`
4. Rust 集成 smoke 已落地：
   - `crates/novovm-node/tests/queue_replay_smoke.rs`
5. 节点级最小生存链已验证通过：
   - `QueueOnly` 入队
   - 重启后 pending 保留
   - `replay_on_start` 消费 pending
   - `Applied` 后队列文件删除

验证命令：

- `cargo test -p novovm-node queue_replay_smoke`

验证结果：

- `1 passed; 0 failed`
- Phase B.1：最小 replay 可观测性已落地（`applied / retry_later / rejected`），日志前缀 `availability_replay:` 已接线。
- Phase B.2：最小 reconcile 只读报告已落地（`pending / applied / unknown`），当前仅做只读汇总，不做自动修复。

当前可定性为：

> Phase B 已完成 B.0 层级验证：Availability Plane 已具备最小“入队 -> 持久化 -> 重启后 replay -> 成功删除”的节点级闭环。

当前仍未进入的范围：

1. replay 批量策略 / 重试策略增强
2. reconcile 自动修复
3. relay data plane
4. capability import
5. transport 强化
