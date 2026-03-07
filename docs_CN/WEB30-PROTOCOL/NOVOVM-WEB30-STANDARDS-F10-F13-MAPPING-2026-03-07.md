# NOVOVM WEB30 Standards 到 F-10~F-13 映射矩阵（2026-03-07）

## 1. 目的

将 `SVM2026/standards` 作为 WEB30 协议设计权威源，收敛到 `SUPERVM` 当前迁移主线（`F-10~F-13`），给出：

- 标准 -> 功能域映射
- 当前迁移进度（文档/代码/门禁）
- 下一步最小可执行项

## 2. 评估口径

状态口径（本文件专用）：

- `SnapshotDone`：标准文档已迁入 `docs_CN/WEB30-PROTOCOL/SVM2026-REFERENCE/standards/`
- `SpecMapped`：已完成到 `F-10~F-13` 的语义映射
- `CodeNotStarted`：主链路代码尚未落地
- `GateNotStarted`：专项门禁尚未落地

## 3. 标准映射矩阵（15 项）

| 标准 | 文件 | 主语义 | 目标功能域 | 优先级 | 当前状态 | 备注 |
| --- | --- | --- | --- | --- | --- | --- |
| WEB30 | `WEB30-TOKEN-STANDARD.md` | 主资产/转账/授权基础 | F-12 DeFi Core | P0 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 作为 F-12 基础标准 |
| WEB3001 | `WEB3001-NFT-STANDARD.md` | NFT 资产语义 | F-12 DeFi Core | P1 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 建议在 Token 基础后接入 |
| WEB3002 | `WEB3002-MULTI-TOKEN-STANDARD.md` | 多代币统一模型 | F-12 DeFi Core | P1 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 与 WEB3001 共用资产层 |
| WEB3003 | `WEB3003-DAO-GOVERNANCE-STANDARD.md` | DAO 治理业务规则 | F-12 DeFi Core | P1 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 与 I-GOV 共识治理分层对齐 |
| WEB3004 | `WEB3004-DEFI-PROTOCOL-STANDARD.md` | DeFi 协议接口 | F-12 DeFi Core | P0 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | F-12 直接主目标 |
| WEB3005 | `WEB3005-IDENTITY-REPUTATION-STANDARD.md` | 身份与信誉 | Cross-Domain（UA/账户体系） | P2 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 不直接归 F-10~F-13，需和 UNIFIEDACCOUNT 联动 |
| WEB3006 | `WEB3006-DECENTRALIZED-STORAGE-STANDARD.md` | 去中心化存储 | F-10 Web3 Storage | P0 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | F-10 直接主目标 |
| WEB3007 | `WEB3007-CROSS-CHAIN-MESSAGING-STANDARD.md` | 跨链消息协议 | F-13 Multi-chain Plugin | P1 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 需与 adapter/plugin 路由联动 |
| WEB3008 | `WEB3008-DNS-STANDARD.md` | 域名服务 | F-11 DNS | P0 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | F-11 直接主目标 |
| WEB3009 | `WEB3009-DEX-STANDARD.md` | DEX 交易协议 | F-12 DeFi Core | P0 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | F-12 高优先子域 |
| WEB3010 | `WEB3010-ORACLE-STANDARD.md` | 预言机接口 | F-12 DeFi Core | P0 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | F-12 基础依赖域 |
| WEB3011 | `WEB3011-AI-INTERFACE-STANDARD.md` | AI 接口 | Cross-Domain（扩展应用层） | P2 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 建议后置，不阻塞 F-10~F-13 |
| WEB3012 | `WEB3012-IOT-SENSOR-STANDARD.md` | IoT 感知接口 | Cross-Domain（扩展应用层） | P2 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 建议后置，不阻塞 F-10~F-13 |
| WEB3013 | `WEB3013-DEVICE-CONTROL-STANDARD.md` | 设备控制接口 | Cross-Domain（扩展应用层） | P2 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 建议后置，不阻塞 F-10~F-13 |
| WEB3014 | `WEB3014-DECENTRALIZED-MESSAGING-STANDARD.md` | 去中心化消息协议 | F-13 Multi-chain Plugin | P1 | SnapshotDone + SpecMapped + CodeNotStarted + GateNotStarted | 与 WEB3007 共同构成跨链消息面 |

## 4. 按功能域的进度汇总

| 功能域 | 对应标准数 | 文档快照 | 主链代码 | 门禁 | 阶段结论 |
| --- | ---: | --- | --- | --- | --- |
| F-10 Web3 Storage | 1 | 1/1 完成 | 0/1 | 0/1 | 设计已就绪，工程未开始 |
| F-11 DNS | 1 | 1/1 完成 | 0/1 | 0/1 | 设计已就绪，工程未开始 |
| F-12 DeFi Core | 7 | 7/7 完成 | 0/7 | 0/7 | 标准覆盖充分，需分波次落地 |
| F-13 Multi-chain Plugin | 2 | 2/2 完成 | 0/2 | 0/2 | 需和 Adapter/EVM 主线对齐 |
| Cross-Domain（不直接归 F-10~F-13） | 4 | 4/4 完成 | 0/4 | 0/4 | 建议后置并与 UA/扩展域联动 |

## 5. 下一步（最小可执行）

1. 先做 P0 三域骨架：F-10（WEB3006）/ F-11（WEB3008）/ F-12（WEB30 + WEB3004 + WEB3009 + WEB3010）。
2. 为每个 P0 子域新增 1 条正向 + 1 条负向 gate，并接入统一 acceptance 汇总。
3. F-13（WEB3007/WEB3014）按 `adapter/plugin` 路线推进，避免绕开现有 registry/ABI/caps/hash 治理。
4. Cross-Domain 四项（WEB3005/3011/3012/3013）暂不阻塞 F-10~F-13，进入后续扩展波次。

## 6. 证据与来源

- 权威来源：`D:\WEB3_AI\SVM2026\standards`
- 快照目录：`D:\WEB3_AI\SUPERVM\docs_CN\WEB30-PROTOCOL\SVM2026-REFERENCE\standards`
- 快照索引：`SVM2026-REFERENCE/STANDARDS-INDEX.md`
