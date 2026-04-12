# NOVOVM 安全测试意见复核结论（2026-04-08）

## 结论摘要

针对外部提出的 4 条安全意见，基于当前仓库代码复核后的正式结论如下：

1. 手工 JSON 构造：成立，但应定为中危稳健性/安全硬化问题，不宜表述成已证实高危注入。
2. EVM Gateway 外部执行路径：成立，但应定为中危配置/供应链信任边界问题，不是经典命令注入。
3. AOEM / 动态库 FFI unsafe 边界：成立，属于架构级风险面，应通过工件治理和信任边界控制。
4. `set_max_pools` 越权：当前仓库未复现，不应入账，除非补充明确入口与调用链。

---

## 1. `novovm-node` 手工 JSON 构造问题

### 结论

成立，但不应定性为已证实高危 JSON 注入，应定为中危稳健性/安全硬化问题。

### 复核说明

- 当前 `build_l1l4_anchor_record` 采用手工字符串拼接方式构造 JSON。
- 但现有实现并非完全裸拼接，字符串字段在进入拼接前已做最小转义处理，因此“已证实可被直接利用的高危 JSON 注入”这一表述并不准确。
- 真正的问题在于：该实现仍依赖人工维护转义规则，完整性与鲁棒性不如 `serde_json` 等标准序列化路径。后续字段扩展、异常输入或控制字符场景下，存在产出畸形 JSON 或遗漏边界处理的风险。

### 处理意见

- 该项应入账。
- 修复方式应为：改为 typed struct + `serde_json` 标准序列化，移除手工 JSON 拼接逻辑。

### 当前状态

- 已修复。
- `build_l1l4_anchor_record` 已改为 typed struct + `serde_json` 标准序列化。
- 已完成定向构建验证：
  - `cargo build -p novovm-node -p novovm-evm-gateway`

### 代码定位

- [novovm-node.rs:102](D:/WEB3_AI/SUPERVM/crates/novovm-node/src/bin/novovm-node.rs:102)
- [novovm-node.rs:142](D:/WEB3_AI/SUPERVM/crates/novovm-node/src/bin/novovm-node.rs:142)

---

## 2. EVM Gateway 外部执行路径问题

### 结论

成立，但不应定性为经典命令注入，应定为中危配置/供应链信任边界问题。

### 复核说明

- 当前实现通过 `Command::new(exec_path)` 直接拉起外部可执行文件。
- 代码路径未见 shell 拼接执行，因此不属于典型的 shell 命令注入问题。
- 但 `exec_path` 来源于环境变量/配置项，当前未见严格的：
  - 固定目录约束
  - `canonicalize` 规范化校验
  - basename allowlist
  - 工件签名或 hash 校验
- 因此，如果配置源被篡改，理论上可能导致网关执行非预期二进制。其本质属于执行工件信任边界不足，而不是 shell 命令注入。

### 处理意见

- 该项应入账。
- 修复方式应为：对可执行路径增加固定目录、规范化校验、basename allowlist，并视部署模型增加 hash/signature 校验。

### 当前状态

- 已完成第一层收口，但未做完全部供应链治理。
- 当前已在 `evm atomic-broadcast executor` 主路径上补入：
  - `canonicalize`
  - 必须为实际文件
  - basename allowlist
- 当前 allowlist 仅覆盖仓库内已明确存在的执行器名：
  - `evm_atomic_broadcast_executor`
  - `evm_atomic_broadcast_executor.exe`
- 该项对应代码已完成定向构建验证：
  - `cargo build -p novovm-node -p novovm-evm-gateway`
- 尚未在本轮保守修复中继续扩展到 `eth public broadcast executor` 等同类路径；该部分待合法部署约束明确后再加固，避免误伤现有配置。

### 代码定位

- [main.rs:14600](D:/WEB3_AI/SUPERVM/crates/gateways/evm-gateway/src/main.rs:14600)
- [main.rs:14609](D:/WEB3_AI/SUPERVM/crates/gateways/evm-gateway/src/main.rs:14609)
- [rpc_gateway_exec_cfg.rs:4863](D:/WEB3_AI/SUPERVM/crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs:4863)

---

## 3. AOEM / 动态库 FFI Unsafe 边界问题

### 结论

成立，属于架构级风险面。

### 复核说明

- 当前系统存在 AOEM / 插件动态库加载与 FFI 调用路径。
- Rust 语言级安全保证并不能覆盖外部动态库内部的内存安全问题。
- 只要底层动态库为 C/C++ 或其他非 Rust 安全边界内工件，则其内存安全缺陷、未定义行为或恶意逻辑都可能直接穿透 Rust 的语言安全边界。
- 该问题本质不属于单点编码缺陷，而是架构层面的信任边界与供应链治理问题。

### 处理意见

- 该项应作为架构级风险记录。
- 不建议按“代码 bug”处理，而应纳入部署与发布治理要求：
  - 仅加载受信工件
  - 记录工件 hash / version
  - 启动前做完整性校验
  - 明确动态库来源与发布链路

### 代码定位

- [aoem-bindings/lib.rs:359](D:/WEB3_AI/SUPERVM/crates/aoem-bindings/src/lib.rs:359)
- [novovm-exec/lib.rs:432](D:/WEB3_AI/SUPERVM/crates/novovm-exec/src/lib.rs:432)

---

## 4. `set_max_pools` 越权问题

### 结论

当前仓库未复现，不应入账。

### 复核说明

- 复核过程中，未在当前仓库找到所述 `set_max_pools` 对应实现。
- 已检查到的市场治理相关参数更新路径，当前体现为治理/提案执行链路内部调用，未见明确的“公开未鉴权入口直接调用全局 setter”证据。
- 因此，按当前仓库状态，无法支持“管理函数越权”这一结论。

### 处理意见

- 该项暂不入账。
- 如外部继续坚持该问题成立，应要求其补充：
  - 具体文件
  - 具体函数
  - 具体调用入口
  - 具体未鉴权调用链

### 代码定位

- [market_engine.rs:464](D:/WEB3_AI/SUPERVM/crates/novovm-consensus/src/market_engine.rs:464)
- [protocol.rs:1423](D:/WEB3_AI/SUPERVM/crates/novovm-consensus/src/protocol.rs:1423)

---

## 建议的处理顺序

建议按以下顺序推进：

1. 优先修复 `novovm-node` 手工 JSON 构造
2. 优先修复 EVM Gateway 外部执行路径约束
3. 将 AOEM / FFI unsafe 边界补入部署与发布治理规范
4. `set_max_pools` 越权问题暂不立案，等待补充证据

---

## 最终归档口径

1. 手工 JSON 构造：成立，记为中危稳健性/安全硬化问题
2. 外部执行路径：成立，记为中危配置/供应链信任边界问题
3. AOEM / 动态库 FFI 边界：成立，记为架构级风险面
4. `set_max_pools` 越权：未复现，不入账

---

## 修复进展快照（2026-04-08）

1. 手工 JSON 构造：已修复
2. 外部执行路径：
   - `evm atomic-broadcast executor` 主路径已完成第一层运行时约束
   - gateway 同类执行器路径的更强信任约束仍待后续补强
3. AOEM / 动态库 FFI 边界：未改代码，保留为治理项
4. `set_max_pools` 越权：未立案
