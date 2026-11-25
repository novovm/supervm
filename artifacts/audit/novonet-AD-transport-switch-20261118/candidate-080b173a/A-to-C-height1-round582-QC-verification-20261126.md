# A 交 C：高度 1、第 582 轮 QC 独立验签与持久记录核对

A 已接收 C 在 `a228f81f` 交付的三份原件，并以 A 自己固定的链域、原验证者集合及 AOEM DLL 离线复验。原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4` 在 C 的认证回执为 `Finalized`、执行成功、落在高度 1，与 A、B、D 的既有认证回执指向同一区块 `9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926`。[C 的交付](C-to-A-height1-finality-handoff-20261126.md) · [A、B、D 的认证回执](A-post-C581-four-host-recheck-20261126.md)

## 验签结果

A 以现役启动清单固定的 A 配置和 AOEM DLL 复验 ML-DSA-65。A 的第 582 轮提案签名通过，提案者与该轮排程一致。QC 内三票均为高度 1、第 582 轮、同一区块的非 nil `Precommit`。链 ID、创世和协议承诺、纪元、验证者集合哈希及父区块和父决定均与本次核对的链域和持久记录一致。

| 验证者 | 原验证者 ID | 权重 | 验签 |
| --- | --- | ---: | --- |
| A | `b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43` | 1 | 通过 |
| B | `4cecd4ba4092c821333501687e7a363d829d544c1a203ea2390d76e8b2a5627f` | 1 | 通过 |
| C | `1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a` | 1 | 通过 |

三个签者不同，累计权重 3，达到门槛 3。原集合总权重 4。

## 持久记录核对

| C 交付原件 | A 下载后复算的 SHA-256 |
| --- | --- |
| 高度 1 区块记录 `block1.bin` | `675f901bc964595e7c3e06c3e17232ce2e530754d7ccb31789e7c5d865d0a454` |
| `height1-precommit.qc` | `63d0db956a8877671a04fc4e10b2eb56ee9ccdb02a0935e057053b39ab310dc4` |
| C 决定 outbox，revision `793` | `fecfe93334344e9f7a42b9933129dfad261b86cad7fe54880b09b138e4644168` |

三项 SHA-256 均与 C 交付值一致。A 验证区块记录内部校验、决定 ID、outbox 定位和域分离摘要，以及 outbox 内部校验。记录指向 C 的 revision `793`，其归档提案和 QC 绑定同一区块，内嵌 QC 与单独原件逐字节一致。

区块记录中的状态根为 `4223c7609d20861ed8e549382ea279a685f3c3a742ee917d334ada9acb44cca0`，回执承诺为 `ecf5d727891d35eecb91539ed2d49f2ba762a57780918198b47a55e0e3f70c11`。持久候选及 AOEM 执行结果由 C 从 RocksDB 副本复核。A 本次独立验证交付的区块记录、outbox 和签名，并对照 A、B、D 的交易认证回执。

**请 C 在 `main` 回传签收**，注明本交接提交、同一区块哈希和 QC SHA-256。

北京时间 2026-11-26 01:15:04，A 经原认证入口确认 B 在线，共识高度 3、第 41 轮，索引高度 2。各节点均未重启或更新，两笔原交易均未重发。
