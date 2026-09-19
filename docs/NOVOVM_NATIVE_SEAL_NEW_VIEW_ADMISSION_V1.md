# 原生封印新轮候选准入 V1

本切片把已有的 timeout certificate（TC）和 new-view certificate（NVC）
接到本机非零轮次提案、投票与持久化恢复的准入边界。它是保守的本地候选
准入规则，不是完整换主协议，不开放 Overlay 的非零轮次网络准入，不部署
服务或重启聊天节点。实现仅位于 SUPERVM Host；不修改 AOEM 内核、ABI、DLL，
也不向 AOEM 添加 NOV 专属业务。

## 必须先有证据，再签新轮候选

本机调用 `admit_local_new_view_candidate(ledger, authority, certificate, request)`，
为目标候选保存准入证据。仅推进 durable round tracking 或收集到超时法定
人数，不再足以调用既有签名接口为非零轮次生成 proposal / vote。

准入与新签名需共同满足：

1. 独立固定的 authority、账本 genesis、协议配置承诺、chain、epoch 和
   validator-set hash 一致；请求高度和轮次匹配证据上下文。
2. 本地持久化轮次已经顺序推进到该目标轮次，并持有有效前轮 TC；不能跳轮，
   也不能仅凭远端对象自报的轮次更新本地状态。
3. NVC 的观察签名与前轮 TC 分别经过严格验签、成员去重和实际权重重算。
   两者都需要超过总验证者权重的 2/3；等权四节点才等价于 3/4。
4. 如 NVC 报告最高 prepare QC，新候选必须承接其同一个不可变 subject：
   只允许 round 和由此重算的 subject hash 改变。块、父块、父 justify QC、
   状态根、交易 / receipt / body / DA 承诺等其余字段不得改变。
   选定的最高 QC、对应 proposal 和三类索引必须已通过本地匹配流程持久化；
   仅随证书携带的内联 QC 不会自动导入。恢复时继续检查这些引用，避免下一轮
   库存报告遗忘已经接纳的 QC。
5. 本地持久 QC 库存不能存在较早轮次中被证书遗漏的更高 QC，也不能存在与
   候选冲突的 QC。新签名时再次核对库存，覆盖“准入后又收到较早轮次中高于
   证书所报轮次的 QC”的情况。准入后生成的同候选、同目标轮次 QC 不阻断
   合法重试或剩余验证者投票；未来轮次 QC 仍拒绝。
6. 从本地 AOEM-owned 候选重新构造 subject，继续执行既有本地候选资格、
   ownership 和账本匹配检查。proposal 的签名者必须是 authority 对目标
   height / round 规定的 leader。

这里的最高 QC 是**当前高度**较早轮次的 prepare QC，不替代 proposal
引用的**父高度** `justify_qc_hash`。没有最高 QC 的法定人数报告，不证明
不存在本地候选锁，更不授权更换已锁定候选。

## 候选锁不解除，准入本身不签名

准入保存完整 authority、NVC、重构 subject 和独立域摘要，同步落盘并读回
验证。该位置的证据采用 first-write-wins，不能通过重复准入覆盖已固定的
候选；已有记录也不能只检查“记录存在”而跳过完整证据验证。相同候选的
有效重复请求返回 `false`、保留原记录，不覆盖原证书；若后来收到较早轮次中
高于原证书所报轮次的 QC，原证书不再满足当前签名条件，不能用重试替换它，
仍然拒绝。

准入操作本身不创建 proposal / vote，不更新签名 height lock，不写签名
outbox，也不改变 canonical 或最终性标记。实际签名仍通过原有共享写锁、
timeout 水位、round lock、height lock 与同步持久化边界。新轮证据不能
解除旧 height lock；如果本机已锁定另一个候选，仍然拒绝签名。

因此本切片选择在证据不足或冲突时停止签名，没有实现允许锁迁移的完整
活性规则。它不保证任意超时之后都能够产出下一轮区块。

## 历史恢复与新签名分开校验

新 proposal / vote 必须匹配当前活动轮次，并重新检查当前库存；历史
proposal、QC 的持久化副本与 outbox 恢复则使用证据校验，不要求历史对象
仍位于当前轮次。否则本机推进轮次后会错误地失去恢复原先合法记录的能力。

历史路径依然不能跳过目标 subject、authority、NVC 和摘要绑定，不能利用
“历史恢复”绕过缺失或篡改的准入记录。旧版本曾保存的非零轮次对象，不因
存在于磁盘就自动获得信任；缺少新规则所需证据时拒绝，不自动补造证据，
不自动修复或重签。第 0 轮既有行为保持不变。

这些保证以本地 seal store 的完整性为边界，不代表抵抗整库删除、外部回滚
或物理磁盘损坏。只读历史证据校验也不是新签名授权。

## 范围与明确未完成项

- Overlay 仍只接受第 0 轮。新增本地准入不是 new-view 网络传播或主循环接入。
- 没有自动轮询 timer、自动换主、完整 pacemaker、epoch 切换或动态验证者集。
- 没有候选解锁、fork choice、commit finality、AOEM candidate 晋升或账本指针晋升。
- `chain_canonical`、`proof_sealed`、`safe`、`finalized` 不因本切片置 true。
- QC 对执行与 body / DA 字段的承诺绑定，不等于实际下载 body、独立 AOEM
  重放、执行正确性证明或网络数据可用性验证。

本地 QC 库存核对继承离线 V1 的整库扫描限制：最多 4096 个 QC object，
超过上限即拒绝，不截断库存继续签名。生产规模仍需经认证、可恢复的逐高度
库存。这不是无限增长账本的最终扫描方案。
库存核查还复验当前高度的准入记录及其最高 QC 引用，防止 QC 对象与全部索引
同时丢失后误签空报告；每高度最多 4096 条准入记录，新记录超限在写入前拒绝。

## 验证与交付边界

新增必需 gate 字段 `test_native_seal_new_view_admission`，默认 false，仅在
以下专用过滤器成功后置 true：

```sh
cargo test -p novovm-node --lib native_seal_new_view_admission -- --test-threads=1
```

既有字段 `test_native_seal_new_view` 继续执行封印全量过滤器，新准入测试也
包含在该回归中：

```sh
cargo test -p novovm-node --lib native_block_seal -- --test-threads=1
```

producer、preflight 与节点运行时的字段顺序、lockset 和数量锁同步为 51。
字段缺失或 false 都拒绝；旧 50 字段报告只验证过观察能力，不能通过改标签
当作新签名准入门已经通过，必须重新生成。

2026-09-20 本机验收（Windows；`0820cc4` 加本切片，已同步远端两项 NAT 更新）：

- 封印全量回归 60/60 通过；专用准入过滤器另跑 20/20 通过。
- 完整 `supervm-mainline-gate` 和 preflight 通过，51/51 字段为 true；状态生成时间
  `2026-09-19T20:54:18.465224700+00:00`。
- gate 内执行计划 10 项、候选工作区 14 项、nonce 67 项通过；后两组各有 2 个
  专用 worker 标注 ignored，由父测试启动执行，不是跳过相应多进程恢复验证。
- 合并远端后的 NAT 网络 4 项、NAT runtime 3 项通过。
- `novovm-network`、`novovm-node`、`novovmctl` 全目标 Clippy `-D warnings` 通过；
  workspace fmt、额外 include 测试文件 rustfmt 与 diff 检查通过。

完整日志位于 `artifacts/audit/seal-admission-20260920/mainline-gate.log`。
前一切片的 40 项封印回归与 50/50 gate 结果不是本切片的验收凭证；以上为
本次重新执行结果。gate 输出的 L1–L4 百分比不是主网上线完成率，远端 CI 状态
也必须另行确认，不能用本机测试替代。

物理多机、公网 / NAT / CGNAT、Linux 安装实机、self-hosted nightly 长跑、
断电恢复和主网签收继续需要独立证据；本地软件回归不能代替这些项目。

前置设计见 [最高 QC / New-view 观察 V1](NOVOVM_NATIVE_SEAL_NEW_VIEW_V1.md)
与 [超时观察和轮次跟踪 V1](NOVOVM_NATIVE_TIMEOUT_OBSERVATION_V1.md)。
