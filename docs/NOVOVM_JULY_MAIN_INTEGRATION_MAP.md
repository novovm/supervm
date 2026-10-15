# 七月原 tick 接合回共享 main 的有界迁入表

日期：2026-10-16。来源核对：共享 `5905614b5ec9f69e0332d8ad81a60080d78d4619`
与恢复 `3ed27becae0eb23c7f13a99e533e249277617d84` 的 Git 文件/内容差异。
写本文时共享 HEAD 已推进到 `a755c814`；实施前必须复核新增差异，不覆盖另一设备提交。
本文是依赖与冲突清单，不是共享 main 已激活、构建通过或性能恢复声明；本轮没有构建。
实施方向服从[恢复路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md)，证据范围见
[验收台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

## 目标调用链与不变边界

恢复检查点已接的链为：原 `run_native_execution_tick_node_mode_v1`
→ `NativeExecutionPipelineUdpDriveV1` → 原 `UdpTransport` 的唯一认证接收 owner
→ `NativeChannel` / `NativeLifecycle` 的结构正文与协议交接
→ 原 `CandidatePipeline` / exec / bindings / AOEM
→ 原 `RoundProgress` / round_bft / `ValidatorJournal` 授权耐久发布与原生查询。

`RpcLifecycle` 在该检查点仅被复用为有界准入、选批与查询投影驱动；名称不是 HTTP
监听许可。正常 native tick 的配置必须没有 `rpc_addr`，不得新开兼容监听或调用
旧 `run_from_env` bootstrap。执行与共识分离，同一业务候选只能由一个 journal 发布。

这张图不证明原七月所有性能机制已经消费：`3ed27bec` 明确
`successor_overlap_active=false`；旧 raw fixture、opcode 114 计数与当前真实交易最终性
不是同一验收。正在进行的结构准入/选批改动须单独核对其最终提交，不能以本文冻结来源
覆盖。原 tick 接通、原结构批保留、完整最终确认 TPS 应分别报告。

## 最小产品依赖组：整组接线，但不整树覆盖

下列 node 路径前缀均为 `crates/novovm-node/src/`；每组只迁其实际差异。

| 组 | 精确路径 | 选择理由与迁接方式 |
| --- | --- | --- |
| 唯一 socket owner | `crates/novovm-network/src/transport/native_ingress.rs`、`transport/native_ingress/tests.rs`；`transport.rs` 的 owner/收包提取/分派相关 hunk | 新增认证对象适配，不复制完整 transport。保留 main 的 RLPx、原交易鉴权、限流与 Protocol 观察语义。 |
| 原生查询 wire | `crates/novovm-protocol/src/native_query.rs`；同 crate `src/lib.rs` 仅加对应模块声明 | 查询请求/响应是原生有界二进制；不覆盖 protocol 其他文件或降级交易 wire。 |
| 协议输入与证据 | `native_pipeline/consensus/message_evidence.rs`、`native.rs`、`native/query.rs`；`consensus/transport.rs` | 同一认证 peer、域与验证者集合约束；transport 仅把 fragments 引用收敛到根 `novorudp`，不改变消息域/编码。 |
| 候选及耐久推进 | `native_pipeline/consensus/bodies.rs`、`progress.rs`、`progress/recovery.rs`、`native_lifecycle.rs`、`native_lifecycle/ingress.rs` | 复用已存在的完整执行、精确正文和 journal 安全规则；这些是配套依赖，不只迁 channel 壳。 |
| 生命周期装配 | `native_pipeline/service.rs`、`service/config.rs` | `start_on_transport` 消费原 tick 已绑定的 transport；复用原创世、常驻 pipeline 与恢复。配置从 relay 改为显式 UDP peers/client 限额，不静默解释旧配置。 |
| 准入/查询交接 | `native_pipeline/service/rpc.rs`、`service/rpc/admission.rs`、`service/rpc/native_query.rs` | 客户端/验证者分队列、真实钱包鉴权、原 pending/nonce 预留、原生查询；保留 `rpc/gossip.rs`、`rpc/history.rs` 等未变实现，不能遗漏新 native query 子模块。 |
| SDK 调用与测试客户端 | `native_pipeline/service/client.rs` | 同一 NativeObjectLink 的主链结构提交/查询适配，无 validator 秘钥或执行器；不接管 NOVOCHAT 平台 SDK。 |
| 实际入口 | `bin/novovm-node.rs` 原 UdpDrive 与 tick 部分；`native_pipeline/consensus/mod.rs`；必要 Cargo feature hunk | 有选择地替换活动调用，不覆盖整个 bin、lib 或 Cargo。main 的模块文件布局维持原样。 |

### 已在共享源码中的能力：不要再迁一遍或降级

- 原 `novorudp/endpoint.rs`、`object_link.rs` 的 shared socket / bounded receiver 回调及
  当前握手、AEAD、代次、字节预算已在两棵树；本次 diff 没有这些实现差异。
  共享库同时被 NOVOCHAT 使用，不以恢复树覆盖其后续修复。
- `native_pipeline` 的 business/execution/persistence/state、BatchPlan 保留共享 APFL、
  最终归档/证明编码边界，以及 exec/bindings 的运行实现不需要整包重迁。
  不回退 CPU 对照、完整代数计划、二进制结果或当前 SDK。
- `novovm-consensus::round_bft` 原安全规则/journal 依赖无需替换；恢复树没有 main 的
  `native_seal_authority` 不等于应删除该资产。两份 Cargo 的 postcard 位置差异无功能价值。
- `native_pipeline/proof.rs` 在恢复树去掉的 `cfg(test)` receipt 诊断不是产品接合依赖；
  保留 main 的关系、反例与统一证明资产，不照搬这一删除。

### 必须手工适配的具体冲突

1. **模块文件名**：main 是 `native_pipeline/mod.rs` 与 `ingress/mod.rs`；恢复是
   `native_pipeline.rs` 与 `ingress.rs`。保留 main 的既有文件，仅增模块声明，不能两种
   文件并存。尤其保留 main 的 `#![forbid(unsafe_code)]` 和 node lib 的 recursion limit。
2. **旧 controller/channel**：main `service.rs` 构造 `HostChannel` / relay NetworkWorker /
   `Controller`；恢复改为 `NativeChannel` / `CandidateBodies` / `RoundProgress` /
   `NativeLifecycle`。不能只改 transport 后仍让旧 controller 拥有同一 signer/head。
   活动 `consensus/mod.rs` 应以新组替换 `pub mod channel/controller`；原文件及测试先保留
   可追溯来源，验收映射完成才清理，不把删除测试当通过。
3. **fragments 类型**：main channel 使用 `duplex::fragments::OutgoingMessage`；新协议
   编码器返回 `novorudp::fragments::OutgoingMessage`。两者不是同一 Rust 类型；不能在
   新编码器与旧 channel 之间仅改 import 便假定兼容。活动 controller 替换应成套进行。
4. **历史 HTTP 适配**：恢复 `service/http.rs` 是从 main 已有 `native_fresh_rpc.rs` 抽出的
   framing/deferred owner。main 仍有原实现，可继续从该原模块导入 `FreshRpcServer`、
   `DeferredRpcHandler/Reply`，保留新配置的禁监听检查；不必复制第二份 HTTP parser。
   若确需抽取，须连同旧调用者归一，不能本轮顺手删除其他功能。
5. **Cargo**：node 只按需要增加显式 `historical-http` feature，正常默认关闭；保留原
   dependencies、已有 features、所有产品 binaries 与锁文件。恢复 Cargo 的依赖删减、
   lettre 版本降级、serde_json feature 删减均不属于本次迁接。
6. **交易 wire 不回滚**：恢复 `novovm-protocol/src/tx_wire.rs` 仍是旧 `[u8;32]` signature；
   main 已保留 V3 `Vec<u8>` 公钥32+签名64、严格 canonical/trailing-byte 拒绝等保护。
   此文件及 `native_tx_unsigned_commitment_v3` 必须留在 main。新增候选路径使用
   `native_pipeline/ingress/wire.rs` 的真实签名契约；新增 native_query 不能拉回旧 tx_wire。

## 原 bin 的安全切换点

main 的 tick 比恢复多出 `seal_config_path` 参数、启动恢复、Product Overlay/旧封印
初始化与 fresh-genesis 提前返回；main bootstrap 还先调用
`native_candidate_node_mode::selected`、`native_seal_config_selection_v1`，并有
`native_resident` 提前返回。**只贴恢复 tick 循环末尾的 product.poll 分支不安全**。

必须在任何 signer、DB、旧 ledger recovery 或监听启动前完成：

- `NOVOVM_NATIVE_EXECUTION_CONFIG` 与显式 `START=create|existing`、chain/genesis/validator/
  peers/预算检查；原 tick 只 bind 一个 UdpTransport，随后一次性移交其 receive owner。
- 与旧 seal config/enable、fresh/candidate mode、旧 overlay 驱动、旧 raw ingress、历史
  fixture、旧 Host execution permit、固定 expected-tx finality 组合显式拒绝；不是静默忽略。
- configured native tick 不执行旧 pending executor、自动同步、旧 seal 发布或任何从
  transport/executed 计数推导 finality 的分支。历史比较必须显式隔离，不能与产品同身份
  同链域运行，更不能清空旧签票日志重启。
- 默认 bootstrap 不再激活 `native_resident` / `native-resident-udp-v1` 替代产品；保留
  EVM、只读与其他未替代产品能力。新的主线须留在原 tick 装配和循环，不新增早退入口。
- 配置解析完成后再激活 signer/常驻 owner；正常退出、poll 错误、报告写入失败都 drain。
  新创世不继承测试账本，不将已存在目录误当新链，不重标旧协议签名与日志。

共享 network 的 adapter 必须保留 `receive_owner` 跨 clone 唯一权、已有 drain queue、
完整接收缓冲与有界 quantum；endpoint 发送使用同一 Arc socket，不另建监听。
`observe_legacy_protocol=false` 在产品中须于旧 Protocol decoder/observer 之前拒绝非
认证家族，避免旧 pending/peer learning/自动同步副作用。普通 client 仅 application/query
streams；校验预算/流子集、AEAD/replay 与独立钱包签名，ACK 不授予执行/最终性。

## 回归依赖和旧测试保护

必须迁入新路径的真实反例，而不是只保留正常成功用例。以下前缀为
`crates/novovm-node/src/native_pipeline/consensus/`：

- `native/tests.rs`、`native_lifecycle_integration.rs`、`progress_integration.rs`、
  `progress_split_integration.rs`：新 owner/消息来源/精确正文、锁与轮次、恢复推进。
- `native_product_rpc.rs`、`native_product_client.rs`、`native_product_queries.rs`：实际
  原 tick 四进程、原生客户端、无 HTTP 的新创世/交易/查询/冷恢复和 minority 用例。
  名称带 rpc 不等于用 HTTP 验收；必须检查最终调用参数与监听事实。
- `chain_integration.rs`、`transport/tests.rs`、`tests.rs` 的新接线 hunk：保留原
  `journal_recovery_integration.rs`、`pacemaker_integration.rs`，不减少 lost-ACK、
  防双签、不同合法 QC 子集、归档篡改和连续高度的断言。
- `native_pipeline/service/tests.rs` 必须适配新配置和构造；旧 `ResidentConfig` 字面量
  不能靠省略/默认填充值把 relay 配置变成 native 配置。

结构接收/独立鉴权的回归资产可以一起迁入，但它们**不是上述产品链的另一个监听器**：

```
crates/novovm-node/src/native_structural_ingress.rs
crates/novovm-node/src/native_structural_ingress/tests.rs
crates/novovm-node/src/native_structural_socket_tests.rs
crates/novovm-node/src/native_pipeline/ingress/delivery.rs
crates/novovm-node/src/native_pipeline/ingress/native.rs
crates/novovm-node/src/native_pipeline/ingress/native/socket.rs
crates/novovm-node/src/native_pipeline/ingress/native/socket/tests.rs
crates/novovm-node/src/native_pipeline/ingress/native/tests.rs
crates/novovm-node/src/native_pipeline/ingress/authentication/tests.rs
crates/novovm-node/src/native_pipeline/pipeline/authentication/tests.rs
```

这些路径部分只被回归调用，不能为方便编译把 fixture raw consumer 激活成第二生产入口。
若迁入 authentication 的新增测试，必须带齐其 `ingress::native::socket::tests::deliver`
等依赖；只加测试文件名而漏模块声明不算迁接。

旧 `controller/tests.rs`、`controller_{integration,workload,crash,recovery,load}.rs`、
`network_integration.rs`、`resident_rpc_process.rs` 及其 aab/load/offline 子文件不得为
让编译通过直接删除。记录每个原反例对应的新入口测试：共享类型变化后可保留其原版本
为显式历史比较，但不能把不再编译执行的断言继续算在新路径已覆盖数量中。

具体缺口：`3ed27bec` 的四进程新 tick 记录覆盖 minority 和冷恢复，**未新增实际
新 tick A/A/B 分票、Linux、离线追赶或同条件 TPS 验收**；progress 级分票不替代
真实进程/传输组合故障。`NativeLifecycle` 还明确记录离线 peer 满队列可背压广播、
归档追赶/后继重叠未完成；不能因旧 Relay 路径曾测过就注销这些缺口。

## 保护范围与实施验收顺序

1. 实施前重读共享新增提交与文件状态；只对上表认领文件处理冲突。保护现有49份
   legacy/runtime 草稿、知识库、所有签票日志、数据库、冻结报告和历史 SDK；数量是
   历史保护集合标识，现场新增文件仍须核对。不重启全量备份，不覆盖恢复场外修改。
2. 迁入纯 source/协议组、接唯一 owner，再将服务和原 tick 的活动装配成套替换。
   不整仓 merge/reset/revert，不采用恢复树中的大范围 D 文件列表。
3. 共享 main 的 `novovm-network/src/lib.rs`、Cargo、mailbox/Iroh/tls/discovery/locator/
   rendezvous/service owner 和 NOVOCHAT 新接线保留；network lib 已 `pub use transport::*`，
   不需要用恢复版删掉50行模块导出。`NativeEndpoint` 等共享 SDK 以最新 main 为准。
4. 在最终共享构建重新跑 owner/流权限/无降级、结构签名/预算、经济/依赖/失败/恢复、
   四原 tick 进程 native-only 全确认与 minority；确认无旧监听/签者/DB authority 激活。
   回归失败不能靠删模块、删测试、缩 quorum、宽松签名或换观察终点修饰。
5. 同一个最终 binary/DLL/config 上测原结构负载和实际成功最终确认，分别报告传输、
   执行、发布、观察时间、P95/P99、失败交易与恢复；此前恢复树通过不等于共享构建通过。
   A/A/B、离线、Linux及高吞吐仍按实际执行范围列明。
6. 通过后按本表撤掉被替代的 ACTIVE bootstrap/controller/错误调用者；再做目录清理。
   只清理已被替代且已有验证映射的实现，不按月份删文件，不移除其他产品资产。

本表不阻塞当前恢复树内的结构准入实测；它界定下一笔共享主线切换必须一起处理的
依赖与冲突，避免把恢复树的小负载通过、已交付共享库或一个 UDP 外壳误报为上线完成。
