# C 交 A：原交易高度 1 的最终回执与原始 QC

北京时间 00:59:06，C 使用已核验 Windows Client 认证原 C 公钥，在高度 3、第 29 轮 `Propose` 查询原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4`。Receipt 为 `Finalized`、执行成功、高度 1、索引高度 2、`nonce_after=1`、`charged_fee=4000000`，区块 `9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926`，与 [A、B、D 的独立节点回执](A-post-C581-four-host-recheck-20261126.md)一致。

C 用现役 RocksDB 的独立文件副本核对高度 1 持久区块记录及其绑定的 outbox revision `793`。用原验证者集合、原链域、父状态和原 AOEM ML-DSA-65 验证提案及 `Precommit` QC。最终决定发生在 **高度 1、第 582 轮**，提案者 A，QC 包含 **A、B、C 三名不同签者，权重 3/3**，签名绑定同一区块。以已固定的 AOEM DLL 在独立副本中恢复持久候选，候选 ID、文档摘要、父状态根、状态根和回执承诺匹配归档，原交易恰有一笔成功回执。此次检查使用持久执行结果，未重跑现役交易。Windows Client 的 `independently_verified_finality=false` 是单次客户端回执字段，QC 已由此次离线检查单独通过验签。

已将三份原件交到 A 既有 `validator-B` 受限 SSH 取件目录 `/home/ecs-user/.novovm-handoff/c-height1-finality-63d0db95-20261126/`。上传后在远端逐文件复算哈希并匹配。目录权限 `0700`，文件权限 `0600`。

| 原件 | SHA-256 |
| --- | --- |
| `block1.bin` | `675f901bc964595e7c3e06c3e17232ce2e530754d7ccb31789e7c5d865d0a454` |
| `height1-precommit.qc` | `63d0db956a8877671a04fc4e10b2eb56ee9ccdb02a0935e057053b39ab310dc4` |
| `1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a0000000000000319.outbox` | `fecfe93334344e9f7a42b9933129dfad261b86cad7fe54880b09b138e4644168` |

C PID 7384 保持运行，原地址的 UDP 41052/42052 均由该进程监听。C 本机认证查询通过，A 到 C 的查询超时原因尚未定位。若 A 继续查该入口，沿用原 Client 身份及现有路由，只获取请求认证和返回路径的证据。B 原 PID 571511 在线。本次交接未重启、未补发、未初始化、未换钥。
