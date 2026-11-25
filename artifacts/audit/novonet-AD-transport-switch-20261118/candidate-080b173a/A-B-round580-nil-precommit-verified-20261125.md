# A/B 原始 nil Precommit 已取得并验签

北京时间 15:14，A 收齐并离线验签。两张原票逐字节通过原 round_bft wire 解码和重编码，使用原验证者公钥、权重和已固定哈希的 AOEM ML-DSA-65，通过 Vote::verify。

| 签者 | 高度 / 轮次 | 阶段和值 | 字节 | SHA-256 |
| --- | --- | --- | ---: | --- |
| A | 1 / 580 | nil Precommit | 3580 | bb73ec93d8aa84a70c4464ad313bf00179b47f012eedc11b78d3d84b5befa8ad |
| B | 1 / 580 | nil Precommit | 3580 | 3e76bd6beb58d225d57c730606a862c4dfa33d9925cc25ca426b67db2a4a3314 |

两张票均核对链 ID 91129655606029、原始创世及协议承诺、epoch 1、验证者集合承诺 a69cb7c7dbfe5b1a888940e6d742936f8507bce7a220ac77ae3afb2ef894b8cd，以及零父区块和零父决定。每张权重 1，法定权重 3。A 原票来自 A 的停机快照。B 原票来自六个既有存储文件在 A 上的临时只读副本，每个文件在复制前后哈希一致，B 原进程 PID 571511 保持在线。Git 仅记录此回执，未提交原票字节。

C 使用既有受限 B SSH 交接取件。文件 7,348 字节，ZIP SHA-256 8f93b2337a540db8addb4eac4d461d5bd1fb128d8fdac1918b3f85f8b06c61cb，B 端已复核相同哈希。

```text
validator-B:/home/ecs-user/.novovm-handoff/ab-round580-8f93b233-20261125/A-B-height1-round580-nil-precommit-20261125.zip
```

请用 C 已保全的 D 原 nil Precommit，与此两张 A/B 原票放在同一原验证者集合和父状态下独立复核。[C 的 D 原票验签回执](C-D-round580-offline-verification-20261125.md) 三张同轮 nil Precommit 可用于原共识的轮次推进，无法证明应用区块或交易最终性。本轮没有向现役 collector 导入票据，没有改变任何节点运行或补发交易。
