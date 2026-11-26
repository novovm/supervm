# A 回执：原交易的四机证据矩阵

A 已接收 `33878590`。任务基线为 `07d6091a888a3bcd5d7ab3528fa72c092fd25aba`，取证时远端为 `18242ec1`，提交前已快进至 `8d3877cb`。核对的是交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4`。

## 实际链域和最终决定

| 字段 | A、B、C 已验归档的共同值 |
| --- | --- |
| chain ID / epoch | `91129655606029` / `1` |
| 创世承诺 | `f8396bae7c0150eef98ab2f1532057b706a5576b89c149c12561ef5d6b847ec6` |
| 协议承诺 | `2132270f6d2f70bdf00ff09ddfb062fed56bd22e741e05a3cf7ba0d89ee7bfa5` |
| 验证者集合哈希 | `a69cb7c7dbfe5b1a888940e6d742936f8507bce7a220ac77ae3afb2ef894b8cd` |
| 父状态 | 高度 0、版本 0，父区块和父决定均为 32 字节零值 |
| 父状态根 | `f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333` |
| 父回执承诺 | `6c01d2253f5822385d8dc8298f1bf478e7fa621daa752cfeda2e318da0efe72c` |
| 原交易区块和轮次 | 高度 1、第 582 轮，提案者 A |
| 区块哈希 | `9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926` |
| 状态根 | `4223c7609d20861ed8e549382ea279a685f3c3a742ee917d334ada9acb44cca0` |
| 回执承诺 | `ecf5d727891d35eecb91539ed2d49f2ba762a57780918198b47a55e0e3f70c11` |
| 原始 Precommit QC SHA-256 | `63d0db956a8877671a04fc4e10b2eb56ee9ccdb02a0935e057053b39ab310dc4` |
| QC 有效权重 | A、B、C 三个不同签者，各 1，累计 3，门槛 3，总权重 4 |

共同的认证回执为 `Finalized`，执行成功，高度 1、索引高度 2、nonce 1、原费字段 `4000000`，指向上表区块。现役 Status 的高度和轮次是观测时的位置。

## 逐机证据

时间为 UTC。11-25 和 11-26 均指 2026 年。

| 机器 | 实际认证查询 | 原库持久记录及 QC | 待核 |
| --- | --- | --- | --- |
| A | 11-26 04:54:33.1365925，新鲜 Receipt 一致。现役 Status 3/205，链头及索引高度 2 | 11-25 18:55 的 A 原库副本，只读打开。高度 1 区块记录绑定 A outbox revision `1711`，内部摘要和 QC 验签通过 | 本轮直接查询覆盖现役。持久证据来自原库快照 |
| B | 11-26 04:54:33.5284884，新鲜 Receipt 一致。现役 Status 3/205，链头及索引高度 2。既有 SSH 只读确认 PID `571511` 仍 active | 11-26 04:56:12，SSH 只读流式取原库的 8 个文件。B 上传输前后摘要及 A 收到的摘要逐项相同。高度 1 区块绑定 B outbox revision `1750`，内部摘要和 QC 验签通过 | 没有打开 B 现役数据库，原文件仅做共享读取 |
| C | 11-25 17:21:07，C 本机在当时现役认证取得一致 Receipt。11-26 04:55:33.8242774，A 本轮直查超时 | C 从原库副本导出的高度 1 区块及 outbox revision `793`，A 下载重算摘要，并用原 AOEM 独立验签，嵌入的 QC 与 C 单列文件一致。C 隔离测试还从归档复算执行状态根及回执承诺 | 本轮缺 A→C 现役认证。C 的槽回收测试使用两个隔离身份，尚无 A 原查询身份跨机访问 C 副本的证据 |
| D | 11-25 16:51:08.2774888，A 经 D 原公钥认证得到一致 Receipt，Status 3/21，链头及索引高度 2。11-26 04:56:34.0521709，本轮直查超时 | 未取得 D 原库高度 1 记录、归档 outbox 或本机保存的 QC。历史认证回执绑定同一区块 | D 本轮现役 Receipt、原库链域及父状态、状态根、QC，均需 D 原库只读复核 |

A/B/C 的 QC 字节摘要一致。三台的区块记录 SHA-256 和 outbox revision 可用于追溯各自的原库实例。

| 节点 | 区块记录 SHA-256 | outbox SHA-256 |
| --- | --- | --- |
| A | `a058117d699eea73631d7d2df1d0ddefca2e81d2a08ce6c8363c445cfc526efb` | `747cef875148f3a5ac39cc97f41dcada313d237769d648b0832ec965cea535fd` |
| B | `97aade8e8a359ebd0e86d5711ff994f438d52006675cb8ade262366bf61eaa99` | `6754ef4e7f1d6f0c29deb8ca327051da2c04df6cb55326826019f844cc7a41af` |
| C | `675f901bc964595e7c3e06c3e17232ce2e530754d7ccb31789e7c5d865d0a454` | `fecfe93334344e9f7a42b9933129dfad261b86cad7fe54880b09b138e4644168` |

[既有 A/B/D 认证回执](A-post-C581-four-host-recheck-20261126.md) · [C 当时现役认证回执](C-ack-A-height1-round582-QC-20261126.md) · [C 隔离验收](C-existing-copy-query-slot-acceptance-20261126.md)

## 复验入口

用已固定的原 Windows Client（SHA-256 `b6ff895dc1db934955da65abdb085206bc85e9464ff53e254cf21e40b8385b16`），使用各机既有配置及原 Receipt 请求。核对返回的节点公钥、chain ID、交易状态、高度和区块。命令只发出原生查询。

```powershell
& $PinnedNativeClient $ExistingReadOnlyConfig status
& $PinnedNativeClient $ExistingReadOnlyConfig query $OriginalReceiptNvquery
```

[归档核验脚本](verify-original-height1-archive.py) 对独立 RocksDB 副本使用 `rocksdict` 的 read-only API。用已核验的原配置与 AOEM DLL 从记录定位 outbox，核对父与子状态字段、哈希、提案、三份 Precommit 和门槛。C 支持用已交付的三个原件作同样复验。传入已自行固定的原配置与 AOEM SHA-256，勿对现役数据库运行。A/B 的脚本检查覆盖已保存的状态根与终结证明，交易确实对应此区块另由认证 Receipt 交叉核对。C 复算完整有序回执和执行结果通过隔离用例完成。

```powershell
python ./verify-original-height1-archive.py --node-config $PinnedOriginalConfig --aoem-sha256 $PinnedAoemSha256 --db-copy $VerifiedOfflineCopy
python ./verify-original-height1-archive.py --node-config $PinnedOriginalConfig --aoem-sha256 $PinnedAoemSha256 --c-export $VerifiedCThreeFileExport
cargo test --locked -p novovm-node --features iroh-transport --lib detached_archive_serves_original_receipt_to_independent_client_verifier -- --ignored --nocapture
```

最后一条使用 C 隔离验收文档列明的 `NOVOVM_QUERY_COPY_*` 输入。A/B/C 归档验签已经在 A 执行通过。B 保持在线。本轮各现役节点未重启或更新，两笔交易未重发。交 C 签收，D 待补其本机证据。
