# 旧完整CI的高度4超时：保留失败，补足失败现场

完整CI41757929427（facd175b）的Rust任务126338555224也失败了，不能只记录该轮
Windows mirror失败。原`fresh_genesis_main_nodes_continue_three_heights_without_restart`
运行332.81秒，在`continuous height 4 deadline`退出。

## 原证据能确定什么

原上传的172份公开进程证据仍在。三在线节点均确认同一高度3证书
`2b71b0d52d367a248ac0a70e006adae8380611ac09f595d39898901d8ebf0491`。
高度4提交已返回queued，否则会先失败于入队断言。没有随后120秒内的实时状态。

确认日志只在高度变化时打印；高度3时的round0、空池与空闲candidate不是高度4超时
现场。fresh入口没有消费通用progress文件开关，缺文件也不证明节点卡死。
离线第四peer握手超时符合该测试拓扑，不能单独解释三在线节点为何没确认高度4。
**旧失败的具体等待点与根因尚未确定。**

## 只补失败观测，不猜测修复生产逻辑

保留原120秒高度期限、三在线/一离线、逐高度入队、原交易/费用/nonce/共识和重启核对。
仅在原期限已经失败后，对各在线节点原`nov_chainStatus`及对应
`nov_getTransactionStatus`各请求一次，每次总超时1秒，无重试。
原响应/错误、node_index、时间和请求耗时写入同目录
`continuous-deadline.process.json`并输出stderr；原PID仍可由既有process.json关联。
随后仍执行原panic，写文件失败也不吞原失败。

这是失败后的顺序采样，不是跨节点原子快照。期间节点可能继续推进；即使后来已
finalized，也不能倒改原用例失败结果。正常成功路径没有新增请求或等待。
复用既有CI公开进程证据allowlist，不上传配置、用途钥、数据库或原始签名载荷。

## 限定复现和验证

同一Linux原测试二进制、独立网络namespace、CPU0/1、实际候选AOEM库，预先限定三轮：

| 轮次 | 实际结果 | 耗时 |
| --- | --- | --- |
| 1 | 1通过/0失败 | 301.99秒 |
| 2 | 1通过/0失败 | 293.81秒 |
| 3 | 1通过/0失败 | 294.85秒 |

三轮均未复现，不继续重跑到满意、不计为旧超时已修复；新失败快照分支尚未在该
超时反例下实际触发。Windows/Linux原CLI测试目标严格Clippy、格式及diff检查通过。
生产代码、超时、认证与重试规则未改，C/D和B既有服务无新操作。

[原CI日志、172份公开证据及本次原日志ZIP](continuous-recovery-evidence.zip)，SHA256：
`f2a0e6a2be1d74fedf04405ce7482a1cac9c8d3e13662696266e9ff5e0157ac2`。
不含下载失败stderr中的临时URL或任何数据库/密钥。

## 完整CI的版本边界

[41759543306](https://github.com/novovm/supervm/actions/runs/41759543306)已成功，
固定源码为f5a866d825b7180fe3be7acc9340188d5a0dc05a，其中该连续恢复步骤也通过。
这是完整CI通过事实，不是旧超时根因已修复；也不能覆盖后续镜像测试修正和本次
失败诊断。最终包含这些差异的main SHA仍须运行原完整门禁。
四机认证仍使用原d7ef793b制品，不重跑、不启链、不登记新TPS或候选正式发布。

## 2026-11-05：cf1469b3再次失败，不能以本地通过销账

[41913255051](https://github.com/novovm/supervm/actions/runs/41913255051)
固定源码`cf1469b388192eaf1a5389ae67574f2025f63a99`，最终`completed/failure`。
Rust作业126866362203的原连续进程用例运行351.30秒，仍失败于
`continuous height 4 deadline`；之后的检查没有执行，不登记完整CI通过。
Windows/Linux实际运行库与所有者作业分别成功，不替代此失败或真实SCM验收。

### 原现场已知与未知

失败快照是顺序采样，不是同时取得的全局状态：

| 用例内节点 | 快照高度/阶段 | 候选提交/完成/失败 | 仍缺的关键证据 |
| --- | --- | --- | --- |
| 0 | 3 / PublishedCertificateRelay，等待后继正文 | 1/1/0 | 高度4正文对象的发送、接纳/拒绝记录 |
| 1 | 4 / round0 / CollectingVotes | 2/2/0 | 已计入票的独立身份、完整proposal/subject身份 |
| 2 | 4 / round0 / CollectingVotes | 2/2/0 | 同上 |

节点1/2固定的区块哈希均为
`6387780ae06a0b7dc71f8304a9b0a51c4e8736dba63b58e45c1b06d4c1185d8e`。
旧快照没有proposal_hash或逐身份票清单，不能据此反推出全部接纳过程，更不能
直接断言只缺节点0的一票。`queued_egress`在原实现中累加的是提交次数，不是当前
积压队列长度；918/279/630不作为背压或送达证据。离线peer握手超时及TLS关闭提示
也不足以归因本次高度4停滞。

原夹具固定四个等权验证者、门槛3；连续阶段在线索引0/1/2、离线索引3，
高度3/4/5的round0提案者依次为1/2/0。索引不是物理A/B/C/D。
尚未找到失败现场中“第一个本应发生却未发生的动作”，不登记生产修复。

### 只补原所有者中的有界交接观测

在原`native_fresh_timing`诊断模块及原发送器、生命周期、签票driver中增加
`NOVOVM_NATIVE_FRESH_HANDOFF_TRACE=1`显式开关；默认关闭，不新增线程、队列、
缓存、网络或执行路线。记录公开peer/validator、对象和proposal/subject身份、
当前高度/轮次、正文完成/拒绝原因、本地签票和已收集的有效票身份。
不输出原始交易、公钥签名字节或用途秘密。

`body.queue`明确只是原发送器的队列准入，不是系统发送、接收ACK或最终交付。
`lifecycle.receive`是原认证Overlay交给生命周期的入口，不是首包到达时间。
若下次缺口落在两者之间，仍需沿原传输事件核对，不能凭入队记录认定已送达。

每种已列交接阶段最多64条，共8种、每进程最多512条。正常对照中大量旧高度
归档拒绝会重复出现，分阶段配额避免其耗尽正文完成和有效票的观察配额；
达到某阶段上限之后，缺日志不能解释为没有发生。第二版起同时保存本机Unix微秒
及单调相对时间。关闭开关时不构造观察JSON，观察写出失败不改变原业务结果。

仅给原CI这一个忽略用例的命令开启该开关；不增加云端任务，不放宽原120秒
高度期限或生产超时，不修改检查选择/门槛，不关闭必检。此差异尚未推送，
截至本记录没有触发新的CI。

### 限定三次本地自然复现：未复现，不是已修复

WSL Linux、同一候选AOEM库、独立网络namespace、CPU亲和性0/1；保留原全部前置、
逐高度入队、三在线/一离线、确认、强杀、原库重启及历史追赶。没有把本机环境
冒充GitHub runner的硬件/负载条件。AOEM实际SO SHA256与失败CI一致：
`d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`。

| 次数 / 父PID | 原用例结果 | 用例耗时 | 观测版本 |
| --- | --- | --- | --- |
| 1 / 4383 | 1通过、0失败 | 230.19秒 | 初版全局512条配额 |
| 2 / 3921 | 1通过、0失败 | 225.83秒 | 加绝对时间、过滤无关接收、保留parent拒绝 |
| 3 / 39604 | 1通过、0失败 | 232.79秒 | 与第2次同制品 |

第2/3次测试程序SHA256为
`89002ea128ec9a791a8b59fbd98b7c2077b657d5e49885169b1f86ede65eb97b`，
实际节点为`f822e8a4cf3be0a686fbeac4ef1b6c55c4bf0f44e25aa5a65558c9b6f50f40ef`。
三次都是带观测运行，不能当成性能对照；未继续加跑直到复现或满意。

正常对照中三节点均取得相同高度4 proposal、完整subject及三个有效签票身份。
例如第3次proposal为`8837087829bde88e2766035060ccdae449d24f1b5c0215a54f4c98ae0280e1b0`，
共同subject_hash为`08ec04d0d7c0b9579f8a1113a87069a395a90f82025c45dd1a18f40113bcb888`。
三份有效身份分别为：

- `31f33ebd37000a0c33cd665ced281cce0f900e5317d876574c4446527cfaa0e4`
- `c908414e716885534474a488401e0348fa4524f1537500dec16f8edcd976a8ca`
- `e4827ea533c4451ab2df66cbc5878a98410b48a38c418f283b04a25eac52a2a8`

这些是新运行的正常对照，不是旧失败缺票身份的证明。正常对照也出现过正文片
先于有效清单及旧高度拒绝，之后由原重试恢复；不能仅据这些错误字样宣布根因。

最后只把观察配额分成8×64并复用发送前已有对象哈希，未再跑第4次长进程用例。
最终观察代码的关闭路径3项测试、开启后实际512条/阶段懒求值边界1项测试通过；
Linux lib及原进程测试目标严格Clippy、格式/diff检查和原CI路由19项通过。
这不是新源码完整CI通过，也不声称已定位或修好停滞。

### 保全与当前边界

[原失败快照、三轮公开进程记录及本地验证日志](continuous-handoff-evidence-20261105.zip)，
SHA256 `04ac7bcbfe9f3291a663a0a2b24bc492243301c4ee3271523ae5e541f85ed2f3`。
140份白名单日志/报告及复现脚本；不包含数据库、配置、用途钥或受保护材料。
本地新库、用途材料和旧下载现场仍留原处，没有删除、清库或转换。

根因定位未完成，cf1469b3失败继续阻断候选签收。后续需要在发生故障的环境取得
这份缺失的逐对象现场；不能事后从已退出进程补回，也不把三轮正常运行当作修复。
用户随后明确批准“现在集中提交并验证一次”：本轮差异及公开证据在main集中提交、
推送一次，由原自动CI采集；不另行手工触发、取消、跳过或反复刷绿。该批准不等于
根因已定位或新CI已通过，结果须核对同一冻结SHA。没有新分支、真实SCM安装或启动，
B/C/D、正式库、经济及共识规则不变，无新TPS结论。

## 2026-11-06：5ce45cbc 的离线历史追赶失败，定向记录补齐

本节依据已下载保存的 CI `42066085246` 原现场，不是重新运行云端检查。
该次失败为 `finalized-before-kill deadline`：在线节点0/1/2已经确认高度3、4、5，
原库重启后0/2位于高度5；随后加入的节点3只记录恢复到高度2，未记录确认高度3。
历史高度3提案者节点1在这个阶段按用例设计离线。它与上述旧高度4现场分别保留，
不能合并为同一根因，也不能把测试节点编号当作物理A/B/C/D。

旧记录中的握手到期/本地offer准入错误指向离线节点1的传输身份
`novovm-mldsa65:a26d45bb5023a3cfc25de7916877956c95bb05a8c94115e58439dc84bbb14cdc`。
这不证明健康节点0/2与节点3之间认证失败。旧开关只传入连续出块阶段，
未传入 `continuous-offline-history`，因而旧现场不足以判断请求未送达、
归档响应未交付、正文被拒绝或后续确认未推进中的哪一项先发生。

### 本地变更：只扩展已有有界诊断

- 将已有显式诊断开关传入离线历史阶段；默认仍关闭。
- 原 HistorySync 记录请求队列准入、通过检查的请求、响应处理时的本地已确认高度
  和已有响应高度；队列准入不等于实际发送或对端接收，响应处理不等于正文交付。
- 复用原正文队列、生命周期接收、正文完成/拒绝及签票记录。新增3种阶段各最多64条，
  全部11种最多704条/进程；阶段额度用尽后的无日志不证明无事件。
- 原180秒离线追赶期限到达时，复用已有有界公共RPC观察，仅采集活动节点链状态；
  无需虚构交易哈希。保留原失败结果，不延长判断期限或重试到成功。
- 不改变恢复策略、认证、票数、生产超时、补送节奏、状态或经济政策。

### 两轮本地自然复现：均通过，未定位或修复原失败

WSL Ubuntu、独立网络namespace、loopback、全用例CPU亲和性0/1；数据目录位于
`/mnt/d/WorksArea/SUPERVM`。这不是GitHub runner的相同硬件或文件系统。
实际AOEM SO仍为 `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`。
两次测试二进制SHA256为 `28375108a1b20a523269d4831409cf7a3f4f170a6900e5c626042f51dbac5d8d`，
节点为 `02f11edbc0adcc425b71e66bc178181cb1b6e5a12e75c52e646a861882d82376`。

| 轮次/父PID | 原进程用例 | 全用例耗时 | 节点3正文完成高度3/4/5，距本进程首个有界记录 |
| --- | --- | --- | --- |
| 1 / 366 | 1通过、0失败 | 234.03秒 | 2.799 / 17.015 / 33.223秒 |
| 2 / 361 | 1通过、0失败 | 220.43秒 | 3.002 / 11.733 / 19.620秒 |

两轮均执行原连续出块、强杀、原库重启、历史追赶和后续重启核对。
这些时间是诊断阶段事件时间，不是TPS、纯网络耗时或最终确认延迟。
本地日志分别为 `target/offline-history-run1b-20261106.log` 和
`target/offline-history-run2-20261106.log`；首个未执行的启动脚本因缺jq失败记录在
`target/offline-history-run1-20261106.log`，不计为产品测试失败。
逐节点记录及数据库留在 `artifacts/audit/candidate-node-processes/` 下对应父PID目录，
未删除或覆盖。两轮之后仅将原超时RPC采样扩展到离线阶段；这段失败采样分支
没有被两轮成功运行触发，不能声称已在真实超时现场验证。

最终差异的 Linux lib/原进程测试目标严格Clippy与格式检查已通过。
关闭诊断的3项单元测试通过；开启诊断的阶段上限/懒求值测试1项通过，
实际输出704条，日志为 `target/offline-history-trace-disabled-20261106.log`、
`target/offline-history-trace-enabled-20261106.log`；严格Clippy记录在
`target/offline-history-clippy-20261106.log`。
上述本地验证阶段未推送、未触发新CI，不以本地正常样本替失败SHA放行。
当前仍是诊断推进，原交付阻断和旧高度4未决风险均未销账。下一次故障需要这份
缺失的逐对象现场，而不是无依据的恢复补丁或无限重跑；真实SCM安装/启动未执行，
B/C/D、正式库及已纠正账户材料不动。

用户随后要求继续下一步：本次仅将这四份恢复诊断源码及本记录集中提交到main，
推送一次并由原自动CI验证，不混入尚未提交的经济规则/分配草稿。新运行的最终结论
必须绑定实际提交SHA；不手动重跑、取消或跳过检查。提交本记录不是故障修复声明。

### 78341bae 的本次 CI：更早的高度1前置失败，未执行历史追赶

2026-11-06只读核对 [42099283537](https://github.com/novovm/supervm/actions/runs/42099283537)：
源码 `78341baea775fff0ca3ef04c957ad46c5cc5bbdf`，attempt=1，completed/failure。
Windows/Linux实际ML-DSA运行库及其原所有者作业通过；Rust历史进程用例在197.62秒后
报 `finalized-before-kill deadline`。汇总作业失败为Rust失败的传递。

下载的原日志表明，这次只运行到 `three-of-four`：节点0/1/2有高度1启动记录，
没有高度1持久确认记录；尚无restart、continuous或offline-history阶段。
两节点前置阶段正常退出，其末尾0/1都处于CollectingVotes，未形成法定确认，
符合该阶段不能以两票确认的要求。三个在线节点的启动时WaitingProposal不能当成
180秒期限到达时的状态，因为原失败现场没有末尾状态快照。

本次PeerIsolated日志均指向未参与此阶段的节点3身份
`novovm-mldsa65:ae30fe061bd815cede8507a7b6f021d722df402a5e1d76e037fefaf09e929ffa`，
每个在线节点5条；它不能证明0/1/2之间认证失败。没有把这次高度1与此前高度4、
离线追赶高度2停滞判作同一根因。此次未取得新增历史追赶证据，不能将未执行计为通过。

原失败日志保存在 `target/offline-history-ci-78341bae-failed.log`；GitHub原公开工件
`fresh-ingress-history-recovery`（id13454410251，124974字节）保存于
`target/offline-history-ci-78341bae/`。没有重跑或再次推送。

本地仅修正测试观察覆盖范围：原显式HANDOFF开关转发到同一用例的前置/重启阶段，
仍使用每阶段64条上限；报错增加phase和height；所有强杀前期限失败复用原公共
RPC观察。未配置RPC的早期阶段会明确记录无RPC地址，而不是擅自开启交易入口。
生产代码、180秒判断期限、票数、原用例顺序和断言不变。

使用原 `fresh_genesis_main_nodes_confirm_three_of_four_and_recover` 本地定向验证，
不为一次计量调整再运行云端完整CI。结果1通过/0失败，用例97.79秒；原高度1确认、
原库恢复及后继恢复按用例完成。父PID697的节点0/1/2均有高度1持久确认，
本地没有复现云端失败。观察到有效签票/接收说明开关已覆盖原前置阶段，但不能
以新正向样本补造云端缺失事件或宣称修复。
测试程序SHA256 `37b31d264f7c75461b24cb7a72b5a08505d90416bd2aa4eb50b4f15c877f7f1e`，
AOEM仍为上列SO；环境仍为WSL隔离网络、CPU0/1。原始日志为
`target/quorum-frontier-run1-20261106.log`，驱动为
`target/diagnose-quorum-frontier-20261106.sh`。一次命令引号错误未启动测试，保留在
`target/quorum-frontier-run-20261106.log`，不计产品失败。
原进程测试目标严格Clippy、格式及diff检查通过；Clippy日志为
`target/quorum-frontier-clippy-20261106.log`。新观察差异未提交/推送，未再次触发CI。
本轮候选仍未获部署资格，下一步定位高度1的首次提案/收票与重连推进，不先改
超时、票数或恢复策略；B/C/D及账户材料不变。

### 后续原 Relay/mesh 重连核对（本地，不触发 CI）

继续核对原 `product_mainline_overlay` 与 `product_relay`：Relay 注册时在同一状态锁内
先排入离线队列，再发布新会话；存在未排空离线队列时，新握手继续排队，客户端也先
消费已缓冲事件。不能只凭 Active 状态能接收 Offer，就断言旧 Offer 在这次失败中
越过了新 Response。尚未证明失败现场出现会话错配，也没有据此修改认证状态机。

在同一 WSL 隔离网络、CPU0/1 条件下运行原有定向测试：

- `three_node_mesh_survives_delayed_restart`：1通过，10.37秒；包含原8.5秒延迟、
  三节点认证、单节点重启后的重新认证及实际消息交付。
- `product_mainline_mesh`：实际命中2项，全部通过，1.07秒；覆盖旧握手响应代次过滤
  及坏MAC只隔离可归责peer。没有把过滤测试扩称旧Offer乱序反例。

测试程序SHA256为 `ba4b426e86a224d03d2c74e9400dd072c15844a3466fed699e70d646f32ad148`；
日志为 `target/quorum-mesh-build-20261106.log`、
`target/quorum-mesh-restart-20261106.log`、`target/quorum-mesh-boundaries-20261106.log`。
这些是原 mesh 组件用例，使用其原组件身份夹具，不是新增四机真实AOEM认证、
完整共识恢复或SCM部署验收。三项通过不覆盖失败CI，也不构成根因排除证明。

当前仍缺失败时节点轮询进度与同候选实际收票的末尾证据；沿已补齐的前置阶段有界
观测继续定向定位。生产实现、超时、法定票数未改；没有推送、重跑云端CI、
安装服务、生成创世或操作B/C/D。

### 早期无RPC阶段的轮询证据补齐（本地未提交）

原早期进程没有交易池，不能为查状态而开启交易RPC。本次沿原
`run_fresh_genesis_confirmation_v1` 和 `native_fresh_timing::handoff_trace` 增加
`main.tick_enter`、`main.tick_status`、`main.overlay_ready` 三个固定阶段。
沿原显式开关，默认关闭；前4轮及其后每5秒采样轮询前后，三个阶段分别最多64条。
状态复用原 `lifecycle.status_json()`，不读私钥、不开新监听、不记录数据报正文。
认证就绪只记录原worker事件，不把它当作实际投票或最终确认。
轮询前后记录可判断采样窗口是否仍有推进；不承诺单靠低频样本定位任意中间函数。

原 `record_deadline` 同时保留每类最后一条公开轮询/收票/接收记录，即使无RPC、
RPC超时或日志末尾截断也能留下已有证据，仍保留原RPC错误，不伪造完整终态。
180秒判断期限、原用例顺序、投票门槛、握手及恢复实现未改。

本地结果：无RPC/截断日志取证反例1通过；有界开关开启1通过（新增三阶段各64条），
关闭3通过；原真实AOEM三票确认/原库重启/后继恢复进程用例1通过，94.76秒。
节点0/1/2的高度1样本均有同候选3票，随后完成原用例；本机未复现CI失败。
相关lib、node二进制、进程测试目标严格Clippy与全workspace格式检查通过。

源码身份为 `78341bae` 加本地诊断差异；实际Linux节点SHA256
`e317aac0acfef1492ce750d7190cb0427630a690079c9c995bb556eba0284445`，
进程测试程序 `ce4a812b8ea5f5436edcfaed335195354c317416514b85f5f43dc75d91574c1c`，
库测试程序 `78bc31fd003e192966b378d1a836c35788bef552be2ce3cf0bfe94999a38c2f9`；
AOEM仍是与失败CI声明一致的 `d73e4a2f…`，没有替换库。
日志为 `target/quorum-progress-{build,deadline-unit,cap-enabled,cap-disabled,process,clippy,fmt}-20261106.log`。
上述定向进程现场父PID703保留在原candidate-node-processes目录。
这次修的是失败取证缺口，不是停滞根因；未推送、未运行云端CI、未取得部署资格。

随后使用同一程序、AOEM和原驱动，在本地隔离网络/CPU0、1只运行一次原CI失败用例
`fresh_genesis_main_nodes_continue_three_heights_without_restart`：1通过，219.42秒。
原断言覆盖同进程连续3→5、耐久入口、离线节点多高度追赶、强杀及原库恢复；
`seal-relay-360-1793915846071148068/continuous-acceptance.json` 中对应字段均为true。
节点现场为父PID360，日志 `target/quorum-progress-history-20261106.log`。
低频状态与认证事件已在实际前置、后继和追赶阶段输出；没有启用早期交易RPC。
这是本地正向样本，不是云端原失败的复现或修复，不再追加同条件盲目重跑。
下一次集中云端验证需使用本轮取证差异，在原失败环境区分循环阻塞、缺少有效票与
父状态/正文等待；不得因本地通过关闭该门禁、提前安装SCM或宣布历史停滞结案。

### 2026-11-06 审计后的定向修正（本地工作区，未推送）

用户要求实施本轮审计发现；在 `main@78341bae` 与已有未提交差异上继续，没有建分支、
触发云端CI、改部署、换库或操作B/C/D。历史高度1、离线追赶高度2及旧高度4失败
仍分别保留；以下短反例不能代替那些原失败的完整因果证据。

本次修改范围：

- 原CI历史恢复步骤从长Rust作业移到独立 `native_recovery` 作业。完整候选仍要求
  两者通过；增加显式 `profile=recovery` 仅供定向诊断，不冒充完整候选。保留原库
  选择器、原恢复命令/时限/网络隔离，检查测试非零命中，并加入进程回收短反例。
  新作业不自动取消运行中的检查；原有其他作业路由不变。
- 原 `finish_cluster` 在全部持久确认并发出kill后，为进程回收单独起计时。业务
  180秒、逐高度120秒、生产换轮300秒均未修改；不以回收期限延长业务确认期限。
- 原 `CandidateBodyInboxV1` 按提案及认证来源独立组装，不混合分片。总条目仍为4，
  每来源64次/秒及固定30秒TTL不变；坏来源只关闭自己的组装，完成后释放同提案
  的其他缓冲并保留原到期的完成标记。清单仍独立验证，完整正文仍核对原承诺。
- 原签票服务只合并同peer当前队列内的同一逻辑消息，保留原字节和绑定一致性；
  不增加验签缓存、队列容量或8次/秒准入预算，不改变票数或签名规则。

修改生产实现前，已加入并运行旧行为反例：

| 反例 | 原实现实测 |
| --- | --- |
| 实际认证重投挤满四个槽位，后续不同证据入队 | 1失败，1.42秒；`retries must not occupy all four slots ahead of distinct evidence` |
| 来源A仅提供清单，来源B完整提供同一签名提案正文 | 1失败，256.05秒；`body fragment source differs from manifest` |

正文反例沿原真实AOEM发布/后继序列取得proof和正文，真实WSS传输捕获认证事件后，
仅控制交付顺序，未复制组装或业务校验逻辑。它属于原历史执行比较路径，不是新的
ML-DSA资产性能或实机四节点验收。原日志为 `target/stall-repair-old-queue-20261106.log`
与 `target/stall-repair-old-body-20261106.log`。外层命令最后的退出码合并因shell变量
传递失败，没有用于判定结果；上述两个测试各自日志明确记录断言与FAILED。

修复后的本地结果：

| 验证范围 | 结果 |
| --- | --- |
| `native_seal_service_` 原配置/队列/签票/换轮/迟到证据/重启回归，含新增队列反例 | 40通过，53.15秒 |
| 原 `native_candidate_node_cli` 非ignored范围，含新增真实子进程回收反例 | 18通过，16 ignored，17.34秒；ignored不计通过 |
| `candidate_workspace_execution_fresh_genesis_real_aoem_publication_and_retry`，含多来源正文、坏来源隔离、TTL及原全局边界 | 1通过，275.08秒；旧行为在中途失败，不能比较两次完整耗时声称提速 |
| CI范围选择/聚合/原候选库选择测试 | 21通过；full/recovery选中项失败、取消、跳过或缺失均不当作成功 |
| actionlint 1.7.12、全workspace格式、相关lib/node/CLI严格Clippy | 通过；actionlint关闭外部shellcheck/pyflakes，非云端完整CI |

当前代码身份是 `78341bae` 加本地已列差异，不是新的已提交或已部署SHA。
Linux节点SHA256 `d3eec253a2bd04fa93bd5352fcda77c9d6f630e6cc377a3abc3485ce18a058a2`；
lib测试程序 `b0540e0c6259499de851a1b3e501f6f7b0b3d8cc4c5851aad8e8ed784b8d6873`；
CLI测试程序 `fc98ea18910d60e230d5b648aff036f52d9035bb8735a0239a3ba21da9974ba1`。
AOEM继续为 `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`，
manifest `e1443e482a3463d8102e06a2264a97c3057eb9d9a7ac59f8d2e571e678e1f4db`。
没有换算法、重签旧域、换正式库或声明Windows部署资格。

运行驱动和哈希清单为 `target/stall-repair-run-20261106.sh`、
`target/stall-repair-fixed-identities-20261106.txt`；输出为
`target/stall-repair-fixed-{build,service,cli,body,process}-20261106.log`，
严格Clippy输出为 `target/stall-repair-clippy-20261106.log`。
原多进程强杀/连续历史恢复结果单独记录，不能用上述单进程或队列反例替代。

同一固定本地程序的原多进程用例
`fresh_genesis_main_nodes_continue_three_heights_without_restart`：**1通过，220.28秒**。
运行环境仍为WSL独立网络命名空间、CPU0/1；这是代码验证，不是吞吐测量。
父PID163140的原库/日志与
`artifacts/audit/candidate-node-processes/seal-relay-163140-1793918333915421213/continuous-acceptance.json`
保留。报告 `accepted=true`，同进程高度3→5、完整最终性历史重启保持、确认后强杀、
耐久入口、非提案者gossip、离线节点多高度追赶及追赶后重启验证均为true；操作配置
未变。物理LAN、断电及公网RPC未执行，不登记为通过。

本轮结论：两个具体推进缺陷已有旧失败/新通过反例，测试回收期限已正确拆分，
CI恢复门禁已本地拆为可单独运行且完整候选仍必需的作业。此前云端高度1/2/4的
确切停滞根因**未因此结案**，不能把这一次成功恢复样本当作全链稳定性或TPS改善。
未提交/推送、未触发或重跑云端CI、未部署SCM、未替换正式库，B/C/D不变。

### 2026-11-06 集中冻结前的三项边界核对

用户后继要求仅核对已有边界证据、缺项补短测，然后集中提交推送一个完整工作包。
上述“未提交/推送”是当时本地验证状态；本节所在修正提交进入新的自动完整CI，
CI结果必须绑定该提交，不能沿用 `54bf0c16` 或旧实机制品资格。

| 边界 | 对应原实现/测试证据 |
| --- | --- |
| 正文内容、提案、来源及上下文仍验证 | `exercise_received_successor_body`：错误高度/未绑定来源、提案hash及正文篡改拒绝；正确签名但输出根错误仍拒绝执行；乱序/相同重复可完成、冲突片关闭。它由已通过的真实AOEM发布序列执行。 |
| 每来源与全部组装有界、按原TTL清理 | `exercise_body_network`：64次/秒、第65次拒绝、下一秒恢复，总4条目、第5条拒绝、30秒清理。`exercise_body_multisource_recovery`：两来源独立、坏来源隔离、不重复交付，各自30/31秒到期且重复清单不延期。所有来源仍共享同一个4条目map，不是每来源4条。 |
| 不合并不同投票、轮次和消息 | `native_seal_service_v3_retains_distinct_decision_votes_in_first_poll` 保留两来源有效票并验证3票证书；原四项split/timeout/restart/late-QC回归保留跨轮及迟到证据；原two-of-four回归保留票数和预算。去重比较完整逻辑消息及绑定，不改变原计票。 |
| 离队后必要重试不被永久屏蔽 | 在 `native_seal_service_retransmissions_preserve_room_for_distinct_evidence` 末尾补直接断言：第一次poll清空后，同一消息再次入队，重复仍只占1项，下一poll累计处理5项且不能形成不足票数的确认。短测1通过，1.41秒；日志 `target/stall-freeze-pending-retry-20261106.log`。没有生产实现变化或新增缓存。 |
| 完整候选不漏恢复、未执行/失败不算通过 | `test_recovery_split_keeps_gate_commands_and_fail_closed_aggregation`、`test_summary_rejects_skipped_cancelled_failed_selected_jobs`、`test_manual_recovery_summary_cannot_claim_full_qualification`；原范围/选择器共21项复核通过。 |
| 回收超限/异常不由业务成功掩盖 | `process_exit_wait_uses_reaping_budget_after_completed_business` 使用真实子进程；原 `try_wait` 错误、超限及退出状态断言仍会失败。只有全部确认且主动终止后才起独立回收时钟，业务期限未变。 |

正文条目和逻辑载荷有界不等于RSS峰值测量；历史正文owner的原容量与主产品8MiB
候选预算不是同一项，本轮均未调整。原40项服务回归、18项CLI、真实AOEM序列及
220.28秒多进程恢复证据继续保留，本次只增加上述离队重试断言并定向复核。
经济分配/AGENTS等另有未提交工作，未混入本次冻结。历史高度停滞仍未归因；
SCM安装、B/C/D、正式库、性能及四机重新认证均不由这次推送授权或登记通过。
