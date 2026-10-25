# ML-DSA-65 共识迁移后的 CI 进程夹具修复

固定输入 `78324b6c4fbb1d4eaebeeaa5457abe296a2dabed`。
本目录所在提交只修原进程测试的启动观察和故障注入时序，不修改生产共识、
传输、签名、经济、AOEM 或超时参数。不是握手迁移、Linux 安装或多机签收。

## 原 CI 失败是事实，不以本地通过覆盖

[主 CI 40213389839](https://github.com/novovm/supervm/actions/runs/40213389839)
已结束，`failure`；Rust job `121229299836`。格式、严格 Clippy、安全、
Windows/Linux 保留组件通过；原 `native_only_` 六项为 3 通过、3 失败。
独立 host-rules `40213389856` 成功，不替代主 CI。
原 Rust 日志保存在 `logs/ci-40213389839-rust.log`；三个失败报告分别为
`ci-failed-aab.json`、`ci-failed-highest.json`、`ci-failed-lowest.json`。

| 失败 | 原始证据 | 本轮定位与边界 |
| --- | --- | --- |
| A/A/B | 三份真实 ML-DSA prevote 为 A/nil/B | 原夹具先等全部进程、全部 session 和多次串行查询，才注入提案；同 height/round 不代表还没投 nil。生产提案期限为 5 秒。报告没有逐阶段时间，不能判定 nil 究竟在注入前还是注入后发生。 |
| 晚启动最大传输 peer 身份 | 已完成 h40、追赶 h1 强杀、恢复及 2/4 反例，最后恢复第三台时 Receipt 超时 | `spawn` 后直接进入严格 5 秒回执查询，没有等待新进程可响应；该新进程 stdout 在失败采集时为空。不能据此宣称传输、共识或 AOEM 已被证明出错。 |
| 晚启动最小传输 peer 身份 | 三健康方 h40，约 117.6 秒后启动缺席方；首次 Status 超时 | 外层虽然写 90 秒追赶期限，内层首个 5 秒 Status 超时直接退出，没复用已有 readiness。首次响应为何迟到，尚无足够阶段证据。 |

**当前原 UDP 握手仍用独立 Ed25519 传输密钥，offer/response 为 362/426 字节。**
ML-DSA 共识对象经已认证的原业务分片承载。未来传输身份迁移后的约 5.5 KB
握手对象确实需要有界续传，但不是本次三个失败已经证实的原因。

## 最小修复与保留的反例

- `native_product_aab.rs`：每个真实节点启动、原 Status 就绪、原钱包供应 B 后，
  立即向该节点注入分配的 A/B 正文和提案，并观察它自己的真实签票，再启动下一台。
  三台共用一个 90 秒启动期限；不等待全员 session，亦不重置原库抽样求绿。
  注入前已观察到该节点 prevote 就明确失败；仍要求实际三票精确 A/A/B，
  三个 nil precommit、自然换轮、三诚实签名 QC、账务和原库重启断言不变。
  首票后故意停顿 6 秒，仍不能单独换轮或发布；每节点就绪、B 投递、注入入队和
  真实 prevote 被观察到的时间进入报告，不是节点签名时刻。B 投递不是“本地 B 已执行”的额外证明。
- `native_product_recovery.rs`：晚启动观察复用原 `recovery_status`；恢复第三台时
  先获得合法 Status，再查询回执。启动、就绪与回执共用原 90 秒期限，不变成 90+90。
- `native_product_queries.rs`：抽出同一 `receipts_until`，单次查询仍最多 5 秒，
  进一步裁剪为剩余总预算。仅 readiness 的类型化 QueryTimeout 可重试；回执错误、
  子进程退出、投影错误或协议错误仍失败，不将资产查询改为无限重试。

没有改变生产 5 秒提案窗口、3/4 quorum、签票 journal、发布条件或追赶实现。
没有删测试、改 ignore、改 CI 选项、放宽权限或将失败包装成环境噪声。

## 本地验证与制品

环境：Windows x86_64 单机真实进程、debug；原入口 NovoRUDP/APFL，零 HTTP，
`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=0`。

| 验证 | 结果 |
| --- | --- |
| 原 readiness 边界单测 | 3 通过，覆盖共享总期限、仅类型化超时可重试、致命错误立即失败 |
| 原 `native_only_` 全部进程用例 | 6 通过、0 失败、0 忽略，356.80 秒 |
| 最终 A/A/B 重编后复验 | 1 通过，21.00 秒；6 秒停顿后独节点仍 height0/round0 |
| node 全目标严格 Clippy / 全仓格式检查 | 均退出 0；Clippy 保留日志，格式检查无输出 |

六项使用本轮 recovery/queries 最终代码；A/A/B 随后仅收紧为全员共享 90 秒启动期限、
增加逐阶段报告，再重编并单独通过。`windows-checkpoint-aab.json` 是六项中的中间夹具，
最终 A/A/B 依据是 `windows-final-aab.json`，不混称同一测试构建。
三个节点从 spawn 到真实 prevote 被观察到分别 988/826/1095 ms；
注入入队到该观察分别 27/28/26 ms。这些是夹具观察时间，不是设备执行或正式延迟成绩。

两种传输 peer 排序的恢复结果在 `windows-final-{highest,lowest}.json`：
健康三方均 h40；缺席方分别在 106794/106669 ms 启动；均在耐久 h1 强杀且 h40
归档尚不存在；原库恢复、独立重启和约 12 秒 2/4 不得发布之后，最终均四方 h46。
余额、nonce、费用和逐笔回执断言通过，原密钥、配置和签票 outbox 保留。
没有数据库复制或钱包向落后节点重新提交正文。两场景报告总时长 160126/163651 ms。
既有刻意延迟启动反例另记录首次查询超时后第 2 次就绪成功，5779 ms；
这不是复现了 Linux 首次响应延迟的内部原因。

生产源码未改，实际进程沿用并重新核验 `78324b6c` 的原制品：

| 制品 | SHA-256 |
| --- | --- |
| node | `730c16538b4b104ef9e19876077b41889dd6ccae328d6b35ebdef1be8a9c781e` |
| native-client | `d1ba39a9aa79128ba4e16c4987214563ebecf7060cdb4547f59abf326b24ec81` |
| 随包 Windows AOEM | `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947` |

原 CI Linux node 为 `3d4e28f9c274349c461394ccb3e6553f22f2dda7bbfe0d681a18c6be86550e1c`，
SO 为 `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309`；
平台身份分别记录，不能拿 Windows 通过替代新提交的 Linux CI。

## 明确未完成

Linux 首次响应延迟的更深层原因未归因；本轮修复的是夹具错误地将 spawn 当就绪、
及未建立 A/A/B 前置条件的时间依赖。新提交远端结果需按该提交单独核对。
原传输 ML-DSA 身份与有界预认证续传、设备/服务/回执消费者及移动端仍未迁完；
不增大 1200 字节首包、不把已认证业务分片拿来循环建立同一会话、不容许算法降级。
同一应用身份迁移不等于 PQ 密钥协商；没有新 TPS、Linux 部署或 A–D 四机资格。
本轮不改 B、不重打 D 包、不激活创世、不删旧运行数据。
