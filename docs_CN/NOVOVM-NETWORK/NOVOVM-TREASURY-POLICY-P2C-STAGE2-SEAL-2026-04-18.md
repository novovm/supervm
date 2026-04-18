# NOVOVM Treasury Policy P2-C 第二段封盘（2026-04-18）

## 目的

本文件用于封盘 `P2-C Stage2`，固定当前已进入主线的 policy 层可执行边界。

## 阶段状态

- `P0`：已签收
- `P1-A`：已签收
- `P1-B`：已签收
- `P1-C`：已签收
- `P2-A`：已签收
- `P2-B1`：已签收
- `P2-B2`：已签收
- `P2-C Stage1`：已签收
- `P2-C Stage2`：已签收
- `P2-C overall`：进行中

## P2-C Stage2 已完成范围（代码事实）

### 1. `policy_version` 进入一等可查询状态

- `policy_version` 已进入 policy/summary 查询面。
- settlement journal 条目已携带 `policy_version`，可追踪账务版本。
- governance policy apply 已具备版本更新与回退防护。

### 2. `policy_source` 已标准化并可查询

- 当前生效来源通过 `policy_source` 对外可见。
- 来源路径稳定区分：
  - `config_path`
  - `governance_path`
- 历史 `default` 值已兼容归一为 `config_path`。

### 3. `threshold state` 分级行为已执行化

clearing 风险状态已稳定分级并可查询：

- `healthy`
- `constrained`
- `blocked`

行为差异已落地：

- `healthy`：允许正常非 NOV clearing。
- `constrained`：对非 NOV clearing 启用更紧的 slippage 限制。
- `blocked`：由 policy gate 拒绝非 NOV clearing。

### 4. `governance disabled` 先拒绝边界保持不变

- `governance disabled` 仍先于授权检查触发拒绝。
- Stage2 未放松该生产语义。

### 5. 风险查询与失败摘要口径稳定

- `treasury.get_clearing_risk_summary` 可见：
  - `policy_version`
  - `policy_source`
  - `current_threshold_state`
  - `last_trigger`
  - `failure_summary`

## 本地验收矩阵

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node --quiet`
- `cargo run -p novovm-node --bin supervm-mainline-gate`

上述验收在本次封盘通过。

## 明确非声明（本阶段不包含）

- 高级金融策略自动化
- 收益分配类产品
- staking / 分红机制
- multi-hop 或拆单 clearing

## 稳定对外口径（建议）

`P2-C Stage2 已将 Treasury policy 推进为“可版本化、可追踪来源、可执行分级行为”的主线；P2-C 整体仍在后续策略层增强中。`
