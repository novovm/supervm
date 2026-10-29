# 原生提案输入归因与已撤回网络实验（2026-10-30）

本轮交付是有界诊断与失败证据，不是吞吐修复。保留同一原生 SDK/APFL、
node → exec → AOEM、独立 ML-DSA 签名、账户授权、费用、四节点耐久回执终点。
没有推进 main、替换已交付 AOEM DLL 或部署候选；约 12K 恢复比较目标仍未达成，也不是最终上限。

## 1. 当前保留源码与撤回边界

保留范围只有：默认关闭的有界输入来源／查询身份诊断、既有 receiver 数字的只读快照，
以及测试驱动有界的请求完成／超时历史。诊断不是签名、授权、准入或共识输入。
`asset_ingress` 容量 8192；查询身份仅覆盖 sequence ≤128，丢失／淘汰必须显式报告。

**已撤回全部 socket 设置、prefill 行为、对应 Cargo 变更及其活动测试。**
本次核对 `crates/novovm-network/src/transport.rs` 与该 crate 的 `Cargo.toml` 均与 HEAD 相同。
因此既有 socket 配置未兑现的缺口及原 prefetch 行为并未被当前源码修复。
以下实验 patch／日志保留供复盘，不是当前活动产品路径或已合格性能补丁。
过去实验的构建／测试 PASS 不自动验证撤回后的另一份源码或最终候选 SHA。

## 2. 新 trace：定位实际提案者，而非固定复用旧高度

基线 `uca-29576-1793295495057531500` 的 65,536 行原测试通过；
测量窗口 38.466394 秒、1703.720915 TPS。此为开启诊断的负性能基线，不是稳定提升。
扫描实际提案者后，本轮最大正向“正文 ID 完成－本机父 ACK”是 **h37/node3：1914.3337 ms**；
没有 ≥2 秒样本。本轮 h52/node0 已在父 ACK 前约 1.172 秒选择 lookahead，不能当作旧慢例。

h37 最终正文 `fca33c48039996c820a1869716440be271080ca1f76229960226b38a10c05281`，
以 node3 的 h36 durable ACK 为零点：

| 观察 | 相对父 ACK，ms |
| --- | ---: |
| 相同输入完成接收／解码 | 1877.8598 |
| 验签计算开始 → 返回 | 1884.8055 → 1893.9442（9.1387） |
| 授权捕获开始 → 状态就绪 | 1894.2691 → 1907.7454（13.4763） |
| 授权排空：1024 接受结果、0 拒绝 | 1908.7841 |
| 选择正文 | 1910.3336 |
| 正文 ID 完成 | 1914.3337 |
| 执行完成／本地提案 | 1992.6142／1994.2484 |
| durable ACK | 2265.5268 |

输入完成接收至选批仅 **32.4738 ms**。此前门禁记录为 `no_ordered_rows`，pending/window 为 0；
不是已有可选正文后卡秒，也未观察到该窗口的 `nonce_pending`／`source_boundary_pending`。
`received` 是整对象重组／哈希后的观察，不是首个 UDP 包到达时间；前段等待仍不能拆归到 socket、
补包、FIFO、接收哈希或交接中的某一项。

关联链为：最终正文 → 同本地 Arc 世代的选择 → 相同 **1024 行数量＋有序交易 ID digest**
→ 原 gossip scope／fragment → node3 接收源 → 同世代验签／授权阶段。
另有原始第 236 行 `00f43612dfbf3153e8ea99cf679860fbfd279ab0ed87d33bf7ab08a7d6255da7`
的 seen／入池／选择样本。行诊断只采 `hash[0]==0`，不代表所有行时间齐全；接受结果还可含精确重复。
有序 ID digest 不包含签名，不能代替原完整字节绑定、真实验签或经济验证。
`source_ref` 只在本进程两次同地址 creation 之间有效，不能跨节点按指针连接。

证据：[h37 精确分析](height37-input.json)、[h52 分析](height52-input.json)、
[分析脚本](analyze-input.ps1)、[基线摘要](before-input-trace.json)、
[原报告压缩件](before-input-trace.raw.zip)、[原运行日志](before-65536-input-trace.log)。
旧 2026-10-29 h52／`dfcc9a1f…` 缺少同组交易关联，**没有被本轮追溯证明**。

### 撤回实验后的最终源码复现：h40/node3 在等当前 nonce 段

最终 node `a4db9b3e…`、trace 开启，65K 全程及冷恢复通过：40.484786 秒、1618.780942 TPS。
本轮最慢正向准备样本变为 h40/node3；h39 durable ACK 为零点：

| 观察 | 相对父 ACK，ms |
| --- | ---: |
| 后续批次已完成准入，池里 1024 行但无可选行 | 1374.1665 |
| 又一后续批次完成准入，池里 2048 行但无可选行 | 1584.8385 |
| 所需 1024 行输入完成接收 | 1668.1542 |
| 同输入验签开始 → 返回 | 1672.0252 → 1681.2069 |
| 同父根授权捕获开始 → 就绪 | 1682.9344 → 1701.1613 |
| 1024 个准入结果全部完成，0 拒绝 | 1702.4405 |
| 选批／正文 ID 完成 | 1705.7146／1709.9775 |
| 提案／durable ACK | 1784.0770／1813.6285 |

已入池样本的当前账户 nonce 为 **2496**，早到交易 nonce 为 **2739、2897 等**；
不能把这些后续交易当作已满足当前顺序的交易。所需组到齐后 **34.2863 ms** 完成准入，
再 **3.2741 ms** 即选批；这段不是“合法可选交易已就绪却被调度器卡住”。

同一完整有序 ID 组在 node1 早已准备过，后来 node1 的 targeted repair（scope sequence113，
fragment `162dbd400d616d2d85c64423d5cbf3806f96c2e0bad77cb6238b2bb8530cb0e0`）
才在 node3 完成上述交接。因此不能仅归咎于客户端供给晚；尚待区分原投递的排队／实际发包／
修复及接收完成。准备了字节不代表已获每个 peer 发送准入。

证据：[最终 h40 分析](accepted-height40-input.json)、[最终 trace 原报告](accepted-input-trace.raw.zip)、
[运行日志](accepted-input-trace.log)。四节点 asset trace 均未淘汰，critical dropped 均为0；
control/delivery 旧环已淘汰数千条，不能据其缺项追溯原投递被取消或丢包。
分析脚本显式区分本机父高度 ACK 与目标高度 ACK；上下文入池样本不冒充目标组成员。

## 3. socket 配置缺口与失败对照

原 owner 读取并报告 16 MiB requested，却没有设置原 UDP socket，也没有读取 effective；
本机独立反例实际读回 64 KiB。实验接上既有 requested/readback 后读回 16 MiB，
不等于已经完成负载验收，也不能由“库有默认配置”推断 OS 已采用它。

| 实验 | 结果 |
| --- | --- |
| 16 MiB＋旧 prefetch，第一次 | 9.9899833 秒失败，node0 查询 tx1024 超时；四节点 head4，输入 sent9/DONE4 |
| 同配置重复 | 13.8410018 秒失败，故障移到 node1／tx1024；同样 head4、sent9/DONE4 |
| 同二进制，配置 0／OS 默认 | PASS：41.621163 秒、1574.583584 TPS，64 批 DONE、四节点 head64 |

三个实际签名输入 hash 不同，不称同字节 A/B。失败节点完成 h1 的八组 128 行回执时，
首末组观察跨度为 3.933488／7.690233 秒；OS 默认对照 node0 为 52.214 ms。
失败请求所指交易已在 h2 执行且其他节点可查询；但后续输入也未完成交付，不能把整轮失败称为纯统计问题。

证据：[第一次 manifest](configured-failure-1.manifest.json)／[公开原件](configured-failure-1.zip)、
[重复 manifest](configured-failure-2.manifest.json)／[公开原件](configured-failure-2.zip)、
[OS 默认摘要](os-default-control.json)／[原报告](os-default-control.raw.zip)。
配置缺口反例：[原实现失败](socket-old-counterexample.log)、[实验实现读回](socket-fixed-counterexample.log)、
[0 配置](socket-zero-config.log)。这些实验实现与活动测试现已撤回。

## 4. 精确查询身份排除到哪一层

后续 `uca-30532-1793298658002545900`：请求 47，node0／tx1152，`sent=true` 后超时。
前一请求 45 同 fragment 在客户端已 Delivered（6007.095 ms），其回复也已被观察（6007.589 ms）。
node0 对请求 45 received/dispatched/prepared/admitted 为 6423.9645／6423.9676／6424.0159／6424.3326 ms，
收到完整请求到回复准入只有 0.3681 ms。

同 fragment 辅助对齐给出 server−client 约 416.8695 ms；它含 ACK／轮询差，不是严格同步时钟。
47 已获客户端对象发送准入，排除“前一请求一直占发送单槽、47 从未准入”。
node0 留存查询 trace 未淘汰、47 在 ≤128 覆盖内，却未见 47 received；节点继续轮询。
末次 progress 比对齐后的超时约早 20 ms，因此结论限于**末次快照仍未接收／解码完整请求47**。

仅有 reply45 未 completed；重建 inbox 为空，不是八个回复全满。
旧回复 busy 只能阻止新请求 dispatch，不能单独解释没有 received。
缺口位于“客户端对象准入 → node0 完整请求解码”之间；仍未区分实际发包、FIFO 消费、碎片或哈希。
证据：[查询 manifest](query-gap-failure.manifest.json)／[公开原件](query-gap-failure.zip)、
[查询运行日志](query-evidence-configured-trace.log)。

## 5. FIFO 证据、反例与整组撤回

旧 `receive_packet_v1` 每次取一条 FIFO 消息前都可再预取最多 4096 包；
endpoint 的一次 64 条消费可多次补入，队列硬上限仍为 65536。
带只读数字的失败样本实际看到 node0 max_depth=65536、enqueued=334240、dequeued=268705。
这证明该次存在积压，**不证明全部慢样本或所有失败都由 FIFO 造成**。
证据：[队列 manifest](queue-before-failure.manifest.json)／[公开原件](queue-before-failure.zip)。

“仅空队列才预取”实验将最大深度降至 4096，但约 30.093528 秒仍发生输入 `Expired`（node0）；
进一步 batch64 对照最大深度为 64，约 30.090766 秒仍 `Expired`（node1），四节点仍 head4。
因此没有把降低队列高水位写成负载恢复，更未延长 30 秒生产 TTL 或改变确认终点。
证据：[prefill 实验日志](queue-fixed-configured-trace.log)、[batch64 日志](queue-fixed-quantum64-trace.log)、
[原 drain 反例](drain-old-counterexample.log)、[实验反例通过](drain-fixed-counterexample.log)。
完整负向现场另见 [prefill manifest](refill-withdrawn-failure.manifest.json)／[原件](refill-withdrawn-failure.zip)，
[batch64 manifest](quantum64-withdrawn-failure.manifest.json)／[原件](quantum64-withdrawn-failure.zip)。

含已撤回改动的历史差异：[candidate](candidate-code.patch)、[queue-before](queue-before-code.patch)、
[fixed](fixed-code.patch)；早期诊断差异：[instrumented](instrumented-code.patch)。
它们可能混含当时其他诊断修改，仅供受控重放，不应直接视作当前候选或部署补丁。

## 6. 证据保存与未知边界

[collect-failure.ps1](collect-failure.ps1) 只允许原 report 和精确四个 `create-N.progress.json`，
记录文件／压缩件 SHA256；不打包 node 配置、数据库、密钥或签名日志。
早期 [collect.ps1](collect.ps1) 的失败 raw.zip 仅有 report，不能冒充包含 progress；使用上述新 manifest 包补足。
缺失计数、淘汰记录及未捕获时段均是 UNKNOWN，不是零等待或未发生。
当前源码只保留可观察性改动；网络候选全部撤回，不能据实验 patch、测试 PASS 或一个对照宣称性能闭环。

## 7. 保留检查点的产品负载，不宣称性能修复

最终运行 node `a4db9b3e013af2c4dd9efec1dd1f1cf5cd4c21e016c3fed3a5864f7479ca659b`；
测试 `00142d7c2989c13aedf4b5f29f818c0ffe15129af0395a9ad720f1d92b9f3289`；
client `bb469366689acaa7afde5668e13aa8db7caab2eb43b297e7137bce747073c877`。
AOEM 保持 `8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`，
业务后端始终 `cpu_callbacks`；GPU-required 只改变同一 ML-DSA 验签后端，不静默回退 CPU。
源码基于 `6008bb32`，代码专用差异 [accepted-code.patch](accepted-code.patch)，SHA256
`7d6009dd35b70b98bc896cef78e1dc29c19c5ffba340208e7e122a690221ab76`。
这只是诊断检查点，不是身份迁移批次的最终发布候选。

原持续在途：65536 行，8192 行在途，最多1024行／块、原四节点回执收齐终点；
后续签名／准备计时，每轮均完整费用／授权／nonce／余额核对及原库冷恢复。
本 NUAS 负载是 **16 个付款账户**，历史旧 V3 12K 负载为1024个付款方；
两者不是只换签名算法的严格对照，不因此免除恢复吞吐目标。

| 连续轮次 | 成功 TPS | P95／P99，秒 |
| --- | ---: | ---: |
| CPU | 1723.426 | 8.018689／9.827087 |
| GPU-required | 2940.937 | 3.385843／3.492183 |
| 同程序 CPU 重复 | 2620.043 | 3.545503／3.719430 |

同一业务／创世／制品，动态有效期窗口可能不同，不称同字节 A/B；CPU 慢轮次不删除。
证据：[持续负载摘要](accepted-continuous.json)／[原件](accepted-continuous.raw.zip)。

另用同一份预签字节、同创世／初始根，运行前轮冻结 node `0054236d…` 与保留检查点：

| 严格同字节轮次 | 成功 TPS | P95／P99，秒 |
| --- | ---: | ---: |
| before CPU | 2654.288 | 3.706726／4.107499 |
| after CPU | 2678.622 | 3.796653／4.142344 |
| before GPU-required | 2883.654 | 3.398758／3.721003 |
| after GPU-required | 2539.369 | 4.878743／5.240089 |
| before CPU 重复 | 2674.544 | 3.595223／4.132635 |

输入 SHA256 均 `169ddcbe7bd74933ab1f71f4f69e4979a8d6e0702c97c255534ff8cab5e64387`，
五轮均65536行、费用状态和冷恢复通过。静态预签准备不计时，不能替代上表持续负载。
CPU 约持平不能覆盖本组 GPU 约11.94%的负向结果；**未证明稳定提速或无性能回退**。
证据：[同字节摘要](accepted-same-corpus.json)／[原件](accepted-same-corpus.raw.zip)。

上述 P95/P99 均为首次提交尝试至四节点最慢回执查询观察，不是节点内部最终性时钟。
本轮没有采用最大值、去掉首轮或改确认终点来登记“恢复12K”。

## 8. 验证与下一处最小缺失证据

- release `--locked` node/client 构建通过；格式通过。
- 521 项原生默认回归、884 项网络默认回归通过；119／11项 ignored 不算已执行。
- trace 开启的真实 ML-DSA 来源／行失败绑定专项通过；上述最终全程 trace 也通过。
- GPU-required 原四进程晚启动、追赶中强杀、原库恢复、收款再次消费及冷恢复通过；真实 A/A/B 也通过。
- `cargo clippy -p novovm-node -p novovm-network --tests --release --locked -- -D warnings` 通过。

见 [原生回归](accepted-native-regressions.log)、[网络回归](accepted-network-regressions.log)、
[行绑定](accepted-actual-row-binding.log)、[GPU 故障恢复](accepted-fault-recovery-gpu.log)、
[A/A/B](accepted-aab-process.log)、[严格 Clippy](clippy-accepted.log)、[格式](format-accepted.log)。
故障原报告见 [压缩原件](accepted-fault.raw.zip)：1064行，104176ms 后晚启动，
durable h1／目标40途中强杀，原配置及签票 outbox 保持；四节点最终 head41／index41一致，
另外要求晚节点本地提案至64和1024行大正文归档通过。它不是65K吞吐测试。
这些是 Windows 同机四进程，不是四台机器或 Linux 资格。

本轮诊断首次构建误用未依赖的 `hex::encode`，已复用原 `hex` helper 修正并重建；
初次格式检查也发现本轮缩进差异，已修正。不是把本轮问题归为历史债，
也没有用后来 PASS 改写旧失败日志。socket／prefill 反例和整负载失败是撤回实验的原记录。

下一处只追踪已匹配 scope／fragment／peer 的原投递：是否真正进入原发送 owner、
初发与补发推进到哪里、对端何时接受碎片／完成哈希。优先复用已有 NativeTransferDiagnosticsV1，
不能把 OutgoingMessage 准备时间当成实际发包时间，也不能把队列高水位解释成所有等待。
不在此基础上继续叠加 GPU、编码、容量或 Lookahead 改动。

本轮只是开发分支诊断检查点；完整候选 CI、跨平台及多机尚未运行，C/D 不据此打包部署。
**主吞吐归因及性能恢复仍未完成，12K 不是最终上限。**
