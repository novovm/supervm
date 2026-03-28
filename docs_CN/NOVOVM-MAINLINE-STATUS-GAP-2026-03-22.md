# NOVOVM 主线现状-目标-差距清单（打勾版，更新于 2026-03-24）

## 1. 范围说明

本文只描述仓库主线的功能落地状态，不讨论营销叙事。  
口径以“真实生产链路可运行”为准，不以模拟环境和表演型测试作为完成标准。

## 2. 主线完成度总览

1. [x] 单一可运维入口已收口到 `scripts/novovm-up.ps1`。  
2. [x] 统一账户持久化主线已收口为 `rocksdb`（gateway/plugin 双侧生产硬约束）。  
3. [x] 统一账户一键生产操作命令已具备（backup/restore/migrate）。  
4. [x] `novovm-node` 常驻消费模式已具备（watch + daemon + lean I/O）。  
5. [x] 四层角色化运行已具备（`-RoleProfile full|l1|l2|l3`，同一程序不同角色）。  
6. [x] 四层最小闭环已具备：L4/L3/L2 真实消费计量写入 L1 锚点文件。  
7. [ ] 公网常驻节点生命周期产品化仍未完成（热更新、平滑升级、运行时治理收口）。  
8. [x] L1/L2/L3 多机部署参数模板已完成（角色矩阵脚本+文档）。  
9. [x] 覆盖层寻址（NodeID/SessionID）最小可用版本已完成（gateway 入站记录 + node 锚点双侧落标识，底层仍兼容 IP 传输）。  
10. [x] 周期化收益结算最小版本已完成（按锚点汇总生成 voucher 凭据）。
11. [x] 自动收益发放最小版本已完成（消费 voucher 自动产出发放指令）。
12. [x] 链上到账执行最小版本已完成（消费 dispatch 自动产出 executed 到账状态）。
13. [x] 外部链回执确认最小版本已完成（RPC 回执落库 + 重放）。
14. [x] 真实签名与广播最小版本已完成（消费 dispatch 调用 RPC 提交交易）。
15. [x] 广播提交-回执确认强一致回补最小版本已完成（统一状态机 + 自动重放）。
16. [x] 回补状态机服务化常驻最小版本已完成（daemon 循环执行）。
17. [x] 回补状态机主入口一体化最小版本已完成（`novovm-up` 同生命周期拉起）。
18. [x] 回补状态机 gateway 二进制生命周期内嵌最小版本已完成（由 `novovm-evm-gateway` 进程内拉起并守护）。
19. [x] 回补状态机“纯二进制逻辑化”已完成（移除对 `powershell` 回补脚本执行器的主路径依赖）。
20. [x] 回补配置口径统一已完成（支持 `NOVOVM_RECONCILE_*`，兼容 `NOVOVM_GATEWAY_RECONCILE_*`）。

## 3. 已完成项（主线）

1. 统一入口与生产运行手册：`docs_CN/NOVOVM-UNIFIED-ENTRYPOINT-AND-RUNBOOK-2026-03-22.md`。  
2. 四层路线图与角色化入口：`docs_CN/NOVOVM-L1-L4-ROADMAP-v1-2026-03-22.md`。  
3. 角色运行手册：`docs_CN/NOVOVM-L1-L4-ROLE-PROFILES-RUNBOOK-2026-03-23.md`。  
4. UA 生产操作命令手册：`docs_CN/NOVOVM-UA-PROD-OPS-CMDS-2026-03-23.md`。  
5. 脚本入口支持角色参数：`scripts/novovm-up.ps1`。  
6. 节点侧四层锚点写入：`crates/novovm-node/src/bin/novovm-node.rs`（`NOVOVM_L1L4_ANCHOR_PATH`）。  
7. 节点侧四层锚点已接入统一账本键空间：`NOVOVM_L1L4_ANCHOR_LEDGER_*`。  
8. 多机部署模板脚本与文档：`scripts/novovm-generate-role-matrix.ps1`、`docs_CN/NOVOVM-L1-L3-MULTI-NODE-PROD-MATRIX-2026-03-23.md`。  
9. 收益结算周期脚本与手册：`scripts/novovm-l1l4-settlement-cycle.ps1`、`docs_CN/NOVOVM-L1L4-SETTLEMENT-CYCLE-RUNBOOK-2026-03-23.md`。  
10. 自动收益发放脚本与手册：`scripts/novovm-l1l4-auto-payout.ps1`、`docs_CN/NOVOVM-L1L4-AUTO-PAYOUT-RUNBOOK-2026-03-23.md`。  
11. 到账执行脚本与手册：`scripts/novovm-l1l4-payout-execute.ps1`、`docs_CN/NOVOVM-L1L4-PAYOUT-EXECUTE-RUNBOOK-2026-03-23.md`。  
12. 外部链确认脚本与手册：`scripts/novovm-l1l4-external-confirm.ps1`、`docs_CN/NOVOVM-L1L4-EXTERNAL-CONFIRM-RUNBOOK-2026-03-23.md`。  
13. 真实签名广播脚本与手册：`scripts/novovm-l1l4-real-broadcast.ps1`、`docs_CN/NOVOVM-L1L4-REAL-BROADCAST-RUNBOOK-2026-03-23.md`。  
14. 强一致回补脚本与手册：`scripts/novovm-l1l4-reconcile.ps1`、`docs_CN/NOVOVM-L1L4-RECONCILE-RUNBOOK-2026-03-23.md`。  
15. 回补 daemon 脚本与手册：`scripts/novovm-l1l4-reconcile-daemon.ps1`、`docs_CN/NOVOVM-L1L4-RECONCILE-DAEMON-RUNBOOK-2026-03-23.md`。  
16. 主入口一体化回补参数：`scripts/novovm-up.ps1`、`scripts/migration/run_gateway_node_pipeline.ps1`。  
17. gateway 二进制生命周期内嵌回补：`crates/gateways/evm-gateway/src/main.rs`（`NOVOVM_GATEWAY_EMBED_RECONCILE_DAEMON`）。  

## 4. 未完成项（差距）

## Gap-A：节点服务体系仍是“可运行”，还不是“完整运维产品”
现状：已有 daemon/watch 机制，但缺统一热更新与升级编排。  
目标：形成稳定的公网节点生命周期管理（启动、平滑重启、升级、回滚）。

## Gap-B：四层闭环已跑通“计量/锚点/周期凭据/自动发放/到账执行/外部确认/真实广播/强一致回补/主入口一体化/gateway 生命周期内嵌/纯 Rust 回补”
现状：主路径已由 gateway 内嵌 Rust 循环执行回补，不再依赖外部 powershell 守护脚本。  
目标：继续做参数治理收口与回补配置模板化，降低跨环境运维差异。

## Gap-C：覆盖层寻址已具备最小可用版本，仍待强化
现状：已具备 NodeID/SessionID 覆盖层标识主链路，网络传输层仍兼容传统 IP。  
目标：在现有标识层基础上推进更强的路由隔离与抗分析增强。

## 5. 下一步执行顺序（功能优先）

1. P1：公网常驻节点生命周期产品化（升级与回滚编排）。  
2. P1：覆盖层寻址增强版（在 NodeID/SessionID 基础上继续强化路由与抗分析）。  
