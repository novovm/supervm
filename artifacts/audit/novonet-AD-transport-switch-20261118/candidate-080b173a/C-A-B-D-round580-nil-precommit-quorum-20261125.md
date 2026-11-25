# C 已取得并复验第 580 轮 nil Precommit 证书

北京时间 15:18，C 通过既有受限 SSH 通道取回 A/B 原票 ZIP。整包 7,348 字节，SHA-256 `8f93b2337a540db8addb4eac4d461d5bd1fb128d8fdac1918b3f85f8b06c61cb`。两张原票和 C 已保全的 D 原票，各 3,580 字节，均与交接的 SHA-256 一致。[A/B 原票交接](A-B-round580-nil-precommit-verified-20261125.md) · [C 的 D 原票回执](C-D-round580-offline-verification-20261125.md)

C 用原共识 wire 解码和重编码、原验证者集合与已验哈希 AOEM ML-DSA-65 独立验证 A、B、D 的签名。三张原票均为链 91129655606029、epoch 1、高度 1、580 轮的 nil Precommit，严格匹配原创世及协议承诺、集合承诺 `a69cb7c7dbfe5b1a888940e6d742936f8507bce7a220ac77ae3afb2ef894b8cd`，以及零父区块、零父决定。三位不同签者各权重 1。

使用原 `Quorum::from_votes` 和 `encode_quorum` 生成离线证书，再使用 `decode_quorum` 和 `Quorum::verify` 复验。确认签名权重为 3，达到法定权重。原生证书 10,718 字节，SHA-256 `f5614fada23e812106dde96b770f2f120f2f68cdac62bc2e58e97bd03200f363`，原字节留在 C。

这份 nil 证书可证明第 580 轮 Precommit 达到法定权重。本轮仅在 C 离线生成和复验，未导入现役节点、未触发计时器或 journal 换轮，也未取得原交易的区块 QC 或持久业务回执。B 继续在线，C 未重启，原交易未补发。
