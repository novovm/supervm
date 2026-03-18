# NOVOVM MEV 封闭扩展架构与复用清单（2026-03-18）

## 1. 目标与边界

目标：在不污染 `SUPERVM` 开源主仓库的前提下，落地可实盘演进的 MEV 闭环（机会 -> 决策 -> 执行 -> 回执 -> 风控）。

硬边界：

1. `SUPERVM` / `EVM 插件` 保持基础设施定位，不写入 MEV 业务逻辑。
2. MEV 逻辑只存在于独立私有仓：`D:\WEB3_AI\SUPERVM-MEV`。
3. 通过网关公开 RPC/能力接口耦合，不在主链路代码中嵌入策略痕迹。
4. 私钥、策略参数、收益规则只在 `SUPERVM-MEV` 的私有配置目录中管理。

## 2. 推荐目录结构（封闭、可高性能贴合）

建议保持“双仓隔离”：

1. 开源基础设施仓：`D:\WEB3_AI\SUPERVM`
2. 私有 MEV 仓：`D:\WEB3_AI\SUPERVM-MEV`

在 `SUPERVM-MEV` 内按职责收口：

1. `mev/novovm-mev-runtime/`
2. `mev/contracts/`
3. `scripts/mev/`
4. `mev/config/`
5. `mev/private/`（建议新增，仅私有策略）

`mev/private/` 建议拆分：

1. `mev/private/strategies/`：策略权重、排序、白名单、收益阈值。
2. `mev/private/submitters/`：私有通道提交器（relay/bundle）。
3. `mev/private/secrets/`：私钥、API Key、签名盐（永不进 Git）。
4. `mev/private/profiles/`：实盘/影子/回测配置。

## 3. 已扫描可直接复用能力（SUPERVM-MEV）

本次扫描与本地编译确认：

1. `cargo test -p novovm-mev-runtime --manifest-path D:\WEB3_AI\SUPERVM-MEV\Cargo.toml --no-run` 已通过。

可复用模块：

1. 机会准备：
   - `mev/novovm-mev-runtime/src/m6a_triangle_prep.rs`
   - `mev/novovm-mev-runtime/src/m6b_backrun_prep.rs`
2. 策略调度：
   - `mev/novovm-mev-runtime/src/m6_strategy_dispatch.rs`
   - `mev/novovm-mev-runtime/src/bin/m6_strategy_dispatch_runner.rs`
3. 执行网关（本机闭环）：
   - `mev/novovm-mev-runtime/src/execution_gateway.rs`
   - `mev/novovm-mev-runtime/src/internal/eth_tx_request_builder.rs`
4. 机会输入与上游桥接：
   - `mev/novovm-mev-runtime/src/upstream_evm.rs`
5. 合约与执行计划：
   - `mev/contracts/NOVOVMTriangleFlashSwapReceiverV1.sol`
   - `mev/novovm-mev-runtime/src/flash_swap_executor.rs`
   - `mev/novovm-mev-runtime/src/bin/m8_internal_flash_swap_deploy_runner.rs`
   - `mev/novovm-mev-runtime/src/bin/m9_internal_flash_swap_canary_runner.rs`
6. 运行编排：
   - `scripts/mev/run_fullnode_mev_stack.sh`
   - `scripts/mev/live_fl9e_internal_hotloop.sh`
   - `scripts/mev/run_fullnode_mev_stack_external_peer_profile.sh`

## 4. 当前缺口（实盘前必须补）

已识别主要缺口：

1. 私有通道未形成独立路径：
   - 当前 `SubmitPath` 仅有 `InternalQueue/LocalNode/PublicBroadcast`。
   - 文件：`mev/novovm-mev-runtime/src/contracts.rs`
2. 未看到私有 bundle 提交接口实现：
   - 未发现 `eth_sendBundle` / `mev_sendBundle` / builder relay 适配层。
3. 提交一致性评估仍按三路径模型：
   - 文件：`mev/novovm-mev-runtime/src/submit_path_consistency.rs`

结论：你现有代码已具备“机会到执行”的主干，但“私有通道/打包能力”仍是不完整项。

## 5. 最短落地方案（不动 SUPERVM 主干）

### 第一步（当天可完成）

1. 在 `SUPERVM-MEV` 扩展 `SubmitPath` 新值：`PrivateRelay`。
2. 新增 `private_relay_gateway.rs`（只放在 `SUPERVM-MEV/mev`）。
3. 在 `m6_strategy_dispatch_runner` 增加 `submit_path=private_relay` 支持。
4. 在 `submit_path_consistency` 增加私有通道路由检查。

### 第二步（次日可完成）

1. 实现 `relay adapter`：
   - `eth_sendBundle`（或目标 relay 的 HTTP API）。
2. 增加“私有优先、公网回退”策略位：
   - 仅在 `SUPERVM-MEV` 生效。
3. 把私有提交结果纳入 `ack/status/receipt` 统一回执结构。

### 第三步（封盘验收）

1. 跑长窗并输出：
   - 机会数、提交数、上链数、失败码分布、净收益。
2. 验收通过后仅发布开源仓“通用接口文档”，不发布私有策略细节。

## 6. 高性能贴合做法（不留主仓痕迹）

1. 同机部署：`SUPERVM gateway` + `SUPERVM-MEV runtime`，走 `127.0.0.1`。
2. 只使用网关通用接口，不改 `crates/plugins/evm/plugin` 的策略逻辑。
3. 策略参数热更新只在 `SUPERVM-MEV/mev/private/profiles/`。
4. 开源仓只保留能力面，不记录策略权重、私有地址、收益阈值。

## 7. 开源与保密实践建议

1. `SUPERVM-MEV` 使用独立私有 remote，不与开源仓混推。
2. 私有目录加入忽略：
   - `mev/private/secrets/*`
   - `mev/private/profiles/live/*.env`
   - `artifacts/migration/mev/live-*`
3. 不在 `SUPERVM` 提交任何私有 relay 地址、钱包、收益参数。

## 8. 当前结论

1. 你已经具备“机会 -> 执行”的可运行主干。
2. 为了进入实盘，最短板是“私有通道/打包器”缺位。
3. 正确做法是：继续在 `SUPERVM-MEV` 封闭扩展，不改 `SUPERVM` 主干代码。
