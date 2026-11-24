# C 高度 1、第 580 轮提案证据

2026-11-25，北京时间。运行源码 `da72de891c11e7113e35cc273b32839123035054`。

## 现役认证回执

A 于 01:54:33 认证原 C 身份并查询交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4`。C 当前高度 1、轮次 580，索引高度 0，`recovering=false`，待处理 1 笔。原交易回执为 `Received`，`in_pending_pool=true`，`in_local_offer=true`。候选 `value=0x9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926`，`body_id=0x8038112267cb0468eee912d12ab181a15cf5038b6b9add6da26ebf828c9f7ba7`，`signed_proposal=null`。候选查询由共识 owner 匹配该轮本地候选执行包的交易回执投影，因此已确认候选登记和交易纳入候选。交易补拉来源仍未知。 [A 认证原始回执](A-to-C-authenticated-query-20261125.json) · [候选查询实现](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/progress.rs#L299-L329)

## `successor_unavailable` 的直接条件

从现役 `run.json` 的四把创世共识公钥按源码计算 `SHA256(validator-id 域前缀含末尾 NUL || 公钥字节)`。按 Validator ID 升序如下，角色按已固定的公钥指纹映射，C 的指纹与 A 认证回执一致。生效高度为 1，选主下标从 0 开始，公式为 `(height - 1 + round) % 4`。 [创世集合构造](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/service/config.rs#L394-L421) · [Validator ID](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-consensus/src/round_bft/wire.rs#L61-L75) · [排序](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-consensus/src/round_bft/wire.rs#L131-L155) · [选主公式](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-consensus/src/round_bft/wire.rs#L259-L267) · [哈希实现](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-consensus/src/round_bft/wire.rs#L688-L693)

| 下标 | 节点 | Validator ID |
| --- | --- | --- |
| 0 | C | `1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a` |
| 1 | B | `4cecd4ba4092c821333501687e7a363d829d544c1a203ea2390d76e8b2a5627f` |
| 2 | A | `b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43` |
| 3 | D | `d90f5145f324790d6cef7b592d25747d0fb21958327eaeeb2367c800d74a8f90` |

- **高度 1、第 580 轮由 C 提案，高度 2、第 0 轮由 B 提案。** C 是本轮 leader，记录该选批状态意味着当次分支判断 `has_successor_target(1)=true`，即当前高度已有匹配的本地候选工作或已提升的后继工作。该判断也要求当时 lifecycle、body owner 和 progress 未冻结且 progress 未在恢复。部署代码按四节点给 `bodies=12`、`inflight=2`，通过后继预选的前置容量检查。下一高度没有已有目标时，`successor_selection()` 在 `validators.leader(2, 0) != local_validator` 返回 `None`。这里检查排定身份，即使 B 在线，C 仍无此预选资格。 [选批分支](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/service/rpc.rs#L646-L681) · [目标判断封装](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/native_lifecycle/successor.rs#L239-L245) · [目标判断](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/native_lifecycle/successor.rs#L278-L280) · [冻结判断](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/native_lifecycle.rs#L394-L396) · [本地目标](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/bodies/successor.rs#L678-L701) · [后继预选](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/bodies/successor.rs#L241-L260) · [运行容量](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/service.rs#L586-L595)
- `record_selection_gate(..., None, ...)` 将此次后继预选结果挂到当时的高度 1、轮次 580 和 parent。认证查询分别读取此选批状态、本地 offer 和共识候选。`successor_unavailable` 说明高度 2 的预选未开始。高度 1 的签名由独立的 `RoundProgress` 推进。 [状态记账](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/service/rpc.rs#L980-L1015) · [查询投影](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/service/rpc/native_query.rs#L69-L105)

## 从候选到签名的证据边界

| 阶段 | 本轮证据 |
| --- | --- |
| 候选选择、执行包投影和本地登记 | 原交易已通过，且本轮提案资格属于 C。当前索引仍为 0。 |
| 共识推进到 `journal.propose()` | 必须在 `TimeoutStep::Propose`，无占用的 outbox、待确认转换或优先处理的已决定状态／可接受提案；`proposed_round` 需允许本轮提案。有已验证的 valid/prevote QC 时优先取其指定值对应的候选。现有查询未暴露当时的 step、待确认转换、outbox、`proposed_round` 或 valid QC 指向及对应候选可用性。 |
| 签名、持久确认和本地回读 | `journal.propose()` 先签名并暂存转换。签名日志确认 `Applied` 或 `AlreadyPresent` 后，owner 才将原消息本地验签登记并交给发送路径。现有回执只确定查询时无匹配该交易的本轮本地签名提案，无法区分尚未签名与已暂存、尚未得到持久确认或本地登记。 |

因此最靠前的未知点是第 580 轮当时的 `journal.step()` 和待确认转换，再检查 `proposed_round` 及 valid QC 指定值是否已有对应候选。没有匹配签名提案，无法沿同一区块核验发送、AOEM 执行结果、三签 QC 或持久化业务回执。现有认证状态和日志也不能判定签票数。 [推进与队列](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/progress.rs#L658-L688) · [签名前判断](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/progress.rs#L1074-L1145) · [签名暂存](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-consensus/src/round_bft/journal/mod.rs#L357-L410) · [持久确认](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-consensus/src/round_bft/journal/mod.rs#L754-L788) · [本地回读](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/progress.rs#L786-L798) · [发送状态](https://github.com/novovm/supervm/blob/da72de891c11e7113e35cc273b32839123035054/crates/novovm-node/src/native_pipeline/consensus/native_lifecycle.rs#L1036-L1072)

02:00:43 在 C 本机只读复核，`NOVOVM-Native-Validator-C` 仍为 `Running`、PID `39896`，开始于 00:30:43，UDP `41052/42052` 仍归属该进程。现役 `C:\Windows\ServiceProfiles\LocalService\AppData\Local\NOVOVM-CommonDa72de89-C-20261125` 无本轮 `progress.json` 或节点 stdout/stderr，SCM 安装、启动命令的 `.stderr` 均为 0 字节。查询的 SCM 记录只有 00:30:13 对应服务的 7040、7045，未找到 7031、7034。旧部署的 progress 属于其他 PID，保留原文件。此次未重启或更新节点、未打开原库、未补发交易、未增加采集，B 未被操作。 [现役启动回执](C-existing-scm-runtime-20261125.json)
