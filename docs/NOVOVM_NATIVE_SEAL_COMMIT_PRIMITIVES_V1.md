# Native seal 第二轮确认基础接口 v1

## 本轮交付

此前 prepare QC 仅证明验证者认可同一个候选。现在增加独立的 commit 签名与
证书类型，以及先同步持久化、再向调用者返回签名的本地 Host 接口。
**它们尚未接入在线节点驱动，不代表已经实现主网最终性。**

代码位于 `crates/novovm-node/src/native_block_seal_commit.rs`：

- `NovNativeSealCommitVoteV1`：使用独立 Ed25519 签名域，绑定完整 prepare QC hash
  和签名者。prepare 签名不可用作 commit 签名；链、创世、验证者集合、轮次、
  区块及执行根等字段通过经过完整验证的 QC 间接绑定。
- `NovNativeSealCommitCertificateV1`：携带 prepare QC 和按验证者排序的确认票。
  校验唯一签名者、完整签名与严格超过 2/3 的权重，不信任调用者填写的权重。
  四个等权验证者需要三票；不同权重时按权重计算。
- `sign_local_commit_vote`：仅供可信本地 Host 显式调用。重建本地候选 subject，
  检查已持久化 QC、proposal、索引、验证者集合与 new-view 准入；同高度有
  冲突块的 QC 时拒绝，包括不同轮次。新签名还必须通过超时和既有安全锁检查。

确认票及防重复签名记录与共享 round/height 安全锁采用一个 RocksDB sync batch
写入并读回验证，之后才返回签名。重启调用可返回相同的旧签名；即使已超时也
不会制造新签名。损坏记录、丢失 QC 或丢失共享安全锁时拒绝继续。
这是正常重启与故障注入测试，不是断电或底层磁盘故障签收。

## 必须保留的协议限制

本版本每个 `(chain, epoch, height, validator)` 只允许确认**一份完整的 QC**。
即使是同一块、同一轮，但 prepare 签名子集不同，QC hash 也可能不同，不能
再次确认。不能直接在在线驱动中看到任意 QC 就调用此接口，否则可能分散确认票，
损害活性。自动启用前必须明确 QC 选择、传播、换轮及确认锁规则并完成协议验证；
若改变签名目标或解锁规则，需要显式版本演进，不能悄悄放宽这个锁。

当前未增加 commit 网络消息、outbox、自动投票、最终祖先验证、
fork choice 或 AOEM/ledger 跨库晋升恢复。证书 `verify` 只是密码学证据校验，
不是运行时授权、当前数据可用性或 canonical/finalized 判定接口。
所有区块的 `proof_sealed`、`chain_canonical`、`safe`、`finalized` 均保持原状。
没有改 AOEM 内核、DLL、ABI、聊天服务或现有操作员数据库。

## 确认凭证持久化

`persist_local_verified_commit_certificate` 重建本地候选，验证确认权重与签名、
持久化验证者集合、完整 prepare QC、proposal、QC 索引及 new-view 准入，
拒绝同高度冲突块 QC。按 `(chain, epoch, height)` 保存一个完整证书，使用
RocksDB sync batch 写入并读回；证书与高度定位共用一个记录，没有跨键半写窗口。
相同证书重复提交返回 false；不同证书（包括同 QC 的不同确认签名子集）报错，
不覆盖。这是保守的证据归档约定，不是在线 fork choice 或共识锁协议。

`load_commit_certificate_by_height` 重启后检查高度键绑定，重新验签并检查
持久 QC、proposal、集合及索引依赖；损坏记录不自动修复或覆盖。读取只返回
历史证据，不证明当前 DA、无竞争分支或已 finalized。正常重启测试不等于断电验证。

## 回归

```sh
cargo test -p novovm-node --lib native_block_seal::tests::commit -- --test-threads=2
cargo test -p novovm-node --lib native_block_seal -- --test-threads=2
cargo clippy -p novovm-node -p novovmctl --all-targets -- -D warnings
```

测试使用隔离的合成执行候选和测试密钥，覆盖 2/4 拒绝、3/4 成证、权重计票、
重复签名者、签名域混用、篡改与错误链、QC 混用、跨轮冲突 QC、重启、防重复
签名、超时和损坏持久记录。现有主线门禁的 `native_block_seal` 测试过滤已包含
本组；没有增加依赖 443 端口或实体设备的默认 CI 前置条件。

下一阶段：先解决确认目标与换轮的在线协议，再接确认消息与证书归档接口，最后
补最终祖先/fork choice 与可恢复晋升。不要把本轮证书类型直接用来改 finalized。

后续新增 [V2 统一确认目标](NOVOVM_NATIVE_SEAL_COMMIT_TARGET_V2.md)，解决同 subject/
proposal 的 prepare 签名子集分票问题。V1 语义保持不变；两版同一签名槽互斥，
没有自动迁移。V2 仍是本地接口，不表示在线确认和跨轮活性已经完成。
