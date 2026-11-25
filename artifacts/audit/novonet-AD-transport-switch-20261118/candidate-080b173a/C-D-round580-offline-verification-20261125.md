# C 离线验签通过：D 原签票

北京时间 14:53，C 从 E 盘把 D 原 ZIP 复制到内盘。整包 11,772 字节，SHA-256 为 `7d491d42897f883cda3f0e6adcc6c7cf8e66c56b5210d30d55b0a99c261107d2`。三张 3,580 字节的 wire 文件均与 [D 交接清单](D-ROUND-580-581-WIRE-RECOVERED.md) 的 SHA-256 一致。

C 基于现役配置的原验证者公钥和权重重建集合，用原 `round_bft::wire::decode_vote`、`Vote::verify` 和 SHA-256 已固定的 AOEM ML-DSA-65 实现离线验签。逐张核对链 ID 91129655606029、原创世与协议承诺、epoch、验证者集合承诺 `a69cb7c7dbfe5b1a888940e6d742936f8507bce7a220ac77ae3afb2ef894b8cd`、高度 1、父区块和父决定均为零、D 的原签者 ID。验证者 D 权重 1，集合总权重 4、法定权重 3。

| 高度 / 轮次 | 原签票 | C 离线结果 |
| --- | --- | --- |
| 1 / 580 | D nil Prevote | 通过 |
| 1 / 580 | D nil Precommit | 通过 |
| 1 / 581 | D nil Prevote | 通过 |

A 现有验签快照记录第 580 轮 nil Prevote 已计入 A、C，nil Precommit 已计入 A、B。D 的两张对应原票在各自阶段都是可用的第三份权重。上述结论结合 A 已验票快照和 C 对 D 原件的验签，C 未持有并重验 A、B、C 其他签者的原始字节，也未给现役 collector 注入任何票。第 581 轮仅取得 D 一张票，仍缺同轮另一位签者或能解释 D 实际换轮的证据。[A 现役签者快照](A-round580-third-weight-20261125.json)

本轮只完成离线取证。未生成 QC、未起计或推进轮次、未验收交易最终性。C 仍以 PID 24256 运行，B 仍在线；未重启 C、未补发交易。
