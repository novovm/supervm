# Native seal V2：统一确认目标，不统一签名子集

## 本刀解决什么

四个验证者可能分别收集 ABC、ABD、ACD、BCD 的 prepare 签名。它们认可的
完整候选与提案相同，但 QC hash 不同。V1 确认整份 QC，无法合并这些确认票。

V2 的确认目标为：

```text
SHA256("novovm-native-seal-commit-target-v2\0" || subject_hash || proposal_hash)
```

只有 QC 的签名子集、累计权重、QC hash 不参与目标标识。每份 QC 仍先完整
验签、检查签名者唯一性和法定权重。完整 subject 包含链域、创世、协议配置、
验证者集合、height/round、父块与 justify QC、执行/状态/回执/DA 承诺。
同一个 block hash 不足以判等：不同轮次、根、提案或父 QC 都是不同目标。

V2 确认票使用独立的 schema、签名域和哈希域。证书可以用任一有效等价 QC
作为 prepare 见证，合并其他等价 QC 持有者的确认票，按验证者权重计票。
`certificate_hash` 仍绑定具体证据，会随见证/确认签名子集变化；后续驱动必须
按 `target_hash` 汇票，不能重新按 `certificate_hash` 分票。

## 本地安全与持久化

- `sign_local_commit_vote_v2` 重建本地已执行候选，检查完整持久 QC、proposal、
  索引、集合、new-view、超时和共享安全锁。当前高度出现冲突块 QC 则拒绝。
- 与 V1 共用 `(chain, epoch, height, validator)` 存储槽，但结构/schema 不同，
  两版互相拒绝解释对方记录；没有自动迁移、删锁、解锁或降级重签。
- V2 原始 prepare QC hash、签名和共享安全锁在同一 sync batch 保存并读回。
  同目标不同 QC 只能重放原签名，不换原始见证；原始见证丢失即失败。
- 同高度仍只签一个完整目标。超时后只重放旧签名，不产生新签名；其他轮次
  不属于等价目标。本刀没有解决跨轮 commit 活性或允许换轮解锁。
- `persist_local_verified_commit_certificate_v2` 按高度归档第一份有效证书。
  后来的同目标证书完整验证后返回 false，保留原始证据；不同目标/版本拒绝。
  `load_commit_certificate_by_height_v2` 重验键绑定、签名、集合和持久见证依赖。
  旧证据损坏不能被新等价证书静默覆盖。

## 验收边界

本地回归覆盖四个独立 RocksDB 测试库的不同 3/4 QC 合票、2/4 拒绝、权重
计票、V1/V2 互斥、签名域隔离、错误目标/根/轮次、损坏凭证、缺失原始 QC、
缺失安全锁、超时、重启和等价证书不覆盖。候选 finalized/safe/proof_sealed/
chain_canonical 保持 false。测试使用合成候选，不等同 AOEM 实机或网络验收。

尚未接入在线驱动、commit 网络消息/outbox、跨轮 commit 恢复、最终祖先验证、
fork choice 或 AOEM/ledger 可恢复晋升。V1 服务仍为 prepare-only，默认行为未改。
实体多机、公网、断电与长期运行均未在本刀执行。AOEM 内核/DLL/ABI未改。

下一刀：在此显式版本契约上设计并验证在线 commit 驱动和跨轮行为，再接网络；
不能因为本地 V2 凭证成立就设置 finalized。

后续已增加[固定 prepared 轮次的实验性在线确认](NOVOVM_NATIVE_SEAL_COMMIT_RUNTIME_V2.md)。
该模式显式开启，跨轮 commit 活性及最终晋升仍未完成。
