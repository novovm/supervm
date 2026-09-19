# 原生封印最高 QC / New-view 观察 V1

本切片补充本地可恢复的最高 prepare-QC 快照与 new-view 法定人数证据。
它不是已部署的换主服务，不解除已有候选锁，不授予最终性，也不开放
Overlay 的非零轮次准入。所有实现位于 SUPERVM Host，不修改 AOEM 内核、
ABI 或 DLL，不自动激活旧链或重启聊天服务。

## 证据对象与独立网络域

`native_block_seal::newview` 提供三种对象：

- `NovNativeSealNewViewQcV1`：完整 proposal 与 prepare QC。验证每个签名、
  权重、proposal/subject 对应关系及 authority 指定的 leader；只接受当前
  目标高度、早于目标轮次的 QC。
- `NovNativeSealNewViewObservationV1`：某验证者关于本地已知最高 QC 的
  签名快照，也可以明确报告没有本地 QC。独立签名域绑定 authority commitment、
  chain、genesis、协议承诺、epoch、validator-set hash、高度、目标轮次、
  签名者，以及可选 QC hash 和 proposal hash。
- `NovNativeSealNewViewCertificateV1`：相同目标上下文下的观察集合与前一轮
  timeout certificate。重新核验唯一成员及实际权重，要求严格超过总权重
  2/3；等权四节点是 3/4，不能把三个签名一概解释为足够权重。

调用方必须提供独立固定的 expected context 和 authority，不能将收到对象
自报的域当作可信输入。authority 仍限定为现有 operator-pinned genesis epoch，
最多 64 个 direct validators。本切片没有实现动态 epoch 或验证者集合切换。

观察签名绑定目标上下文，而不是某一组 timeout 签名的哈希。证书单独验证
目标前一轮的 TC，因此不同节点收集到的有效 TC 签名子集不会阻止观察聚合。
目标 round 必须大于 0，不能为 `u64::MAX`，调度计算溢出同样拒绝。

## 最高 QC 与父 QC 不同

本切片的 highest QC 属于当前高度 `h`，用于报告较早轮次已形成的 prepare
证据。原 proposal 的 `justify_qc_hash` 仍指向父高度 `h - 1` 的 QC；二者不能
互换。genesis 的父块、父 QC 和 AOEM parent 仍为零；genesis 自己形成的 QC
可以成为该高度后续轮次的 highest QC。

证书验证返回的是本次 quorum 报告中的最高 QC，不是全网最高 QC 的证明。
选择按照 QC round 比较；同 subject 的不同有效投票子集用最小 QC hash
确定代表，避免输入顺序影响结果。任何跨 block 冲突（包括跨轮次）都拒绝；
同轮不同 subject 也拒绝。同块跨轮必须保持除 round / subject hash 之外的
全部 subject 字段相同；改变状态根或父 justify QC 同样拒绝。本版本没有
锁迁移规则，不能按哈希或较高轮次替冲突区块选赢家。

## 本地持久化与恢复

`sign_local_new_view` 仅面向显式启用的本机调度器：

1. 核对本地账本/AOEM ownership、genesis、协议域和验证者成员身份。
2. 要求 durable round tracking 已被有效前轮 TC 推进到请求的目标轮次。
3. 在 seal store 的共享写锁下读取当前高度的持久 QC；逐项检查持久 proposal、
   三类 QC 索引，并从本地 AOEM-owned 候选重构 exact subject。新签名前
   还扫描 QC object 库存并与该高度索引精确比较，防止删除或截短索引被
   当成没有 QC。
4. 由这些本地事实选择最高 QC；API 不接受调用方填入的 highest QC。
5. 将签名观察、前轮 TC、authority pin 和不可回退的签名水位放进同一同步
   RocksDB batch，写后重新读取、验签和核对绑定，成功才返回签名。

当前轮或未来轮 QC 不会被静默忽略以生成较低或空报告：这种情况拒绝签名。
同一 slot 的已签快照冻结；后来收到更多 QC 也不能改签同一 slot。
`load_local_new_view` 是只读恢复原快照的接口，不声称该快照仍是最新观察。
同 slot 重试返回已有签名；回退、丢失水位引用对象、缺失 QC/索引、域漂移或
不匹配的持久记录都拒绝生成替代签名。

超时水位继续保护签名边界：已超时的位置不能新签观察；原已持久化观察
可以通过只读恢复接口取回。原 proposal/vote 的 height lock 保持不变。
没有最高 QC 不代表没有候选锁，更不是任意换候选的许可证。

新增内部 `scheduled_leader_v1` 仅用于离线严格计算 leader。公开的
`expected_leader` 和 Overlay ingress 仍拒绝 round > 0，不因此接纳未来轮
网络对象。这里的持久化保证以现有 seal store 和本地磁盘完整性为边界，
不是对整库被删除、外部回滚或设备物理损坏的防护声明。

离线 V1 对整个 seal store 的 QC object 扫描有 4096 项上限，超过便拒绝
新签名，不截断后继续；这不是无限增长主网的扩展性方案。生产规模需补
经认证、可恢复的逐高度库存。历史签名恢复目前仍检查本地候选的活动签名
资格，后续 canonical 晋升必须补不可变历史验证器，不能沿用该资格假设。
QC 中的执行和 body/DA 字段是承诺绑定，不等于下载到真实 body、独立 AOEM
执行验证或重放证明。

## 必需 gate 与验收范围

新增必需字段 `test_native_seal_new_view`，初始为 false，仅在以下命令成功
后置 true：

```sh
cargo test -p novovm-node --lib native_block_seal -- --test-threads=1
```

该过滤器同时覆盖 new-view、既有 seal、timeout、round、Overlay 以及位于
seal 测试模块中的源 QC 夹具。该观察切片引入的 lockset 和数量锁为 50；后续
[新轮候选准入切片](NOVOVM_NATIVE_SEAL_NEW_VIEW_ADMISSION_V1.md) 新增
`test_native_seal_new_view_admission`，当前 producer、preflight 与节点运行时
同步要求 51 个字段。字段缺失或 false 必须拒绝，旧 50 字段及更早报告需
重新生成，不能改标签复用。

2026-09-20 本观察切片的历史本机验收：在 `e8c36d3` 加本切片代码的合并基线上，完整 gate 和
preflight 通过，50/50 必需字段为 true；生成时间为
`2026-09-19T19:20:27.491963700+00:00`。封印回归 40 项通过，其中新增
new-view 15 项，覆盖不足权重、重复/非法签名者、域/根/轮次/leader 篡改、
跨块及跨轮承诺冲突、TC 子集兼容、持久重启、冻结快照、缺失持久对象、
只读保护和并发幂等。额外的 relay client 13 项、node/ctl all-targets
Clippy `-D warnings`、格式和 diff 检查通过。完整日志位于
`artifacts/audit/seal-newview-20260920/mainline-gate.log`（本机忽略的验收产物）。
这些是软件回归结果，不是主网完成率或物理多机测试结果。

后续 [新轮候选准入 V1](NOVOVM_NATIVE_SEAL_NEW_VIEW_ADMISSION_V1.md) 已将本地
非零轮次 proposal / vote 约束到完整 NVC、当前轮次、本地 QC 库存及原候选锁，
并为历史持久对象恢复增加证据校验。准入不等于解锁，不开放 Overlay 非零轮次，
其验收结果独立记录，不能复用上面的历史测试数量。

尚未接入节点主循环自动换主、new-view 网络传播、完整锁迁移与活性规则、
commit finality、AOEM/账本晋升或升级激活。`chain_canonical`、`proof_sealed`、
`safe`、`finalized` 不因这些证据而晋升。物理多机、公网/NAT/CGNAT、Linux
安装、nightly 长跑、断电恢复和主网签收均需独立记录，不能由本地单测推导。
