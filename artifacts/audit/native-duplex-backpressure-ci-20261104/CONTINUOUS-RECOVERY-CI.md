# 旧完整CI的高度4超时：保留失败，补足失败现场

## 2026-11-06 后继：独立原生恢复作业漏接AOEM manifest环境

`3055eefb4785c461d001dcc54e0d760b5371f2a2`的完整CI
[42140191890](https://github.com/novovm/supervm/actions/runs/42140191890)中，
恢复作业[127638522031](https://github.com/novovm/supervm/actions/runs/42140191890/job/127638522031)
六项全部失败于`native_product_rpc.rs`的真实用途身份准备，合计0.01秒；
尚未启动原节点或进入正文、签票、追赶。该次失败保留，不是新的高度停滞证据。

根因是原`ci-native-runtime-candidate.ps1 -GitHubEnvironment`只输出了
`NOVOVM_AOEM_TEST_MANIFEST`和`NOVOVM_AOEM_MANIFEST`，遗漏底层bindings读取的
`AOEM_DLL_MANIFEST`。身份夹具直接调用`AoemExecFacade::open`，在配置节点有机会
应用NOVOVM别名之前，bindings已经要求manifest；于是按SO目录推导了不存在的
`linux/manifest/aoem-manifest.json`。`OnceLock`保留首次准备错误，使六项报告同一错误。
上一轮本地脚本手动设置了底层变量，掩盖了CI跨step环境导出的缺口。

本次只在原候选选择脚本补齐同一个已核验manifest的canonical环境变量，恢复作业
在编译前检查该文件存在且与候选manifest一致，两组真实进程测试入口也核对一致性。
不复制manifest到猜测目录，不改manifest内容或哈希，不关闭required校验；
不修改Rust生产代码、测试断言、票数、超时、网络或库。

本地先清除所有库/manifest别名，只消费旧脚本真实GITHUB_ENV输出，使用上轮同一
Linux测试程序及实际库，复现6失败/0通过、退出101及同一缺失路径（0.08秒）。
修改后的双平台环境回归也先在旧脚本上失败（缺`AOEM_DLL_MANIFEST`），修正后
原23项scope/配置测试及新增的哈希不符禁止导出反例共24项全部通过，包含错误继承
manifest被候选输出覆盖的检查；Windows/Linux各自的哈希不符均在导出前拒绝。

| 定向验证 | 本轮结果 |
| --- | --- |
| 原选择脚本真实导出＋原生进程组 | 已复现原错误，不计为通过 |
| 双平台选择、篡改/身份/能力拒绝与严格CI汇总 | 24项通过；未执行的检查仍不能汇总为通过 |
| 修正后真实原生进程组 | 6项通过、318.95秒，两个身份排序方向均完成原库追赶和少票拒绝 |
| 修正后ML-DSA/UCA恢复组 | 1项通过、147.39秒；40高度含1,024行大正文，追赶中断后从原库恢复、再花及四库冷恢复通过 |

真实回归没有手动补变量：在独立干净进程中重放原选择脚本的输出。选择脚本使用
本机已有Windows PowerShell运行，平台选Linux；仅将绝对文件路径映射为WSL路径，
由原Linux测试程序实际加载仓库中的同一候选SO。WSL没有安装pwsh，不另行安装
工具或伪造Linux选择器通过。库/manifest及node身份与前一节固定值相同。
原CI完整日志、本地原失败及新结果保存在`target/native-runtime-manifest-ci-20261106/`。
两组真实测试流水线退出0，复验后node/client哈希不变。Python语法、YAML解析及差异
检查通过；本次未改Rust，不重新编译整套产品或将旧Clippy结果改记为新执行。

[原失败与修正后本地证据](native-runtime-manifest-ci-local-evidence.zip)含10个明确列出的
选择器输出、原失败/新通过日志、回归日志及本地执行命令，5,150字节，SHA256：
`7908e91d20434d4d476b58cb34a79e9e951514e8bd371e14641f6f39679a7457`。
不包含完整云端下载日志、用途钥、节点/钱包配置、数据库、journal或钱包材料。

本轮先本地提交，不立即推送：截至北京时间09:48核对，原运行的Rust作业仍在执行，
main推送会按现有concurrency取消它并重开全量CI。保留旧结果，让已有运行完成，
后续在完整工作包交付时集中验证；未手工重跑、取消、跳过或修改原CI结果。
本地修正通过不等于新提交已取得云端候选资格。本次仍未部署、不操作B/C/D、
不替换正式库、不产生TPS结论，其他未提交经济文档不混入此修复。

## 2026-11-06 最新范围纠正：旧 WSS 主链退役

本文件下方是历史失败及当时修正记录，不再作为运行旧主链的操作指令。
`fresh_genesis_main_nodes_continue_three_heights_without_restart` 实际使用
`RelayTransportV1::Wss443`、本机 Relay、`NOVOVM_JULY_HISTORICAL_FIXTURE=1`
及 `NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED=true`，不是实机 A/B/C/D 的
ML-DSA-65 NovoRUDP 配置。`49a7e65f` 的失败不能直接定性为原生主网丢包或
安装阻断；旧失败仍为失败，根因未归因，不因退役改为通过。

用户现已明确取消该主链承载。本轮从原节点去掉 WSS 主链启动闭包，移除专属
service/lifecycle、夹具、主链拓扑模板及操作指引，不保留待开兼容模式。
旧环境变量即使配合 JULY 开关，也在开库/开钥/监听前拒绝；应用 Relay/邮箱/
Web 不受此操作影响，B/C/D 没有服务变更。实际主链继续沿原 NativeChannel/
NativeLifecycle/NovoRUDP/APFL；原共识门槛、签名、AEAD、nonce及账务不改。

| 原正确性责任 | 当前原生所有者及验证范围 |
| --- | --- |
| 正文来源、完整性、父状态和预算 | `novovm-network::novorudp::fragments` 的按认证 peer＋对象 hash 重组；原 `consensus/transport`、`native_lifecycle_integration`；不沿旧 WSS manifest inbox 再建一套 |
| 重复输入/投票、不同签票人和轮次 | 原 NativeLifecycle 有界 pending/发送交付及 RoundProgress/VoteCollector；不迁入旧 WSS FIFO |
| 当前制品真实确认与账务 | `native_only_genesis_transactions_queries_and_cold_recovery`、`native_only_minority_and_pending_signature_conflict_recover` |
| 连续高度、离线原库追赶、2/4拒绝、原库恢复 | 两项 `native_only_recovery_late_*_validator_original_database_and_minority`；原生 A/A/B 恢复测试 |
| ML-DSA/UCA 大正文、费用、再花、追赶中断及四库恢复 | `native_uca_process_late_validator_interrupted_catchup_respend_and_cold_recovery` |

独立 recovery job 保留并改由上述既有原生进程测试承担，从原 rust job 移动而非
重复跑；full 仍要求它成功。旧 WSS 专属测试不再列为主链必过，纯验证、离线候选、
配置 pin、子进程回收及其他应用测试继续保留。尚缺的实际原生故障点：

- proposal/prevote 已持久化而未最终确认时的精确强杀、不重新注入钱包恢复。
  旧 `controller_crash` 在本轮删除之前已未挂接编译；不是本轮新丢覆盖。
- 真实 EROFS/ENOSPC、启动存储失败保持文件不变、QC持久化后发布失败恢复。
  旧 WSS 专项是可选 ignored 测试，现有原生冷恢复不是等价验收。

### 删除清单与共用能力误删复核

对照清理前 `49a7e65f488c137ba310ba5878631fa96dca81a3`，
[删除清单](RETIRED-WSS-MAINCHAIN-FILES.txt)列出82个源码、专属夹具/工具/文档文件。
其中包括此前已无mod/include挂接的旧controller/channel源码；不是82项现用能力。
另在原文件中移除旧启动分支、打包模板和CI调用。历史内容可由Git恢复；没有删除
数据库、签票日志、用途钥、应用数据或既有审计证据。经济文档的既有未提交修改
不混入本工作包。原生运行与纯离线校验分开，未把离线参考当作主网兼容运行模式。

用户补充外部审计后，由两路只读复核按真实调用核对，而不是按文件名含有
`overlay`/`relay`就删除；结论为未发现所列共用能力误删阻断，不等于安全全覆盖。

| 共用能力 | 本地差异核对结果 |
| --- | --- |
| 原传输身份与用途钥 | `service.rs`的`purpose_keys`传输/验证者分离加载、`crypto::transport_identity`、`network::product_overlay`及`transport_binding`、整个relay custody实现与清理前Git blob一致；删除的是node的`product_mainline_overlay`，不是共享认证模块 |
| 原生正文/证据/共识 | 原`NativeChannel -> message_evidence::verify -> RoundProgress`、`bodies`的父状态/业务pin/正文身份/预算、`chain`归档及证书验证没有生产差异 |
| 原持久化与执行 | `journal`的签票绑定/恢复/条件写、原AOEM bindings/exec及consensus crate未改；保留同步离线artifact验证与独立持久回读断言 |
| 应用Relay | `product_relay_client`只移除旧主链独占的`has_buffered_events`观察方法；`recv_event`缓冲消费、来源/目标/session/sequence/bytes检查、TLS、认证、deadline、队列上限保留 |
| 应用证据与包 | suite65、公钥/peer_id、签名、路径、哈希、Relay/peer/strategy验证保留；只拒绝退役topology scope。应用relay/peer/overlay/NAT/evidence/mailbox门禁及随包程序保留 |

新增队列测试首次运行24通过/1失败，失败是新增测试误要求完成后的重投永远不交付。
原`fragments`明定at-least-once，完成元数据按容量和TTL淘汰；本夹具每peer仅留4项，
先完成多项后允许再交付。没有为测试改生产去重、缓存、超时或认证。
修正测试保留7个不同签票人/轮次/相位/候选控制对象的唯一初始交付；pending重复
只占1槽/一份字节，离队重试必须经原DONE排空，允许有界历史内直接DONE或最多一次
相同fragment及完整消息再交付。业务重复计票仍由原collector反例验证。
初次失败日志保留为`target/native-only-retirement-validation/archive-initial-failed-linux.log`，
不改写该次为通过；修正后同组25项通过。

### 本轮受影响验证与制品边界

下列是本工作包的本地结果，不继承旧WSS运行结果，也不等于冻结提交的完整CI。
原始日志在`target/native-only-retirement-validation/`；脚本与合成装配日志在
`target/wss-mainchain-retirement-ci-script-checks-cff92b95e48b4341ada3e34669655976/`。

| 范围 | 本地结果 |
| --- | --- |
| Windows/Linux原node全部target | 两平台`cargo check`及严格Clippy通过；格式与差异检查通过 |
| 旧主链配置拒绝 | Windows/Linux各1项，覆盖15种配置组合；在开库、开钥和监听前失败，不因JULY开关绕过 |
| 原NovoRUDP分片重组 | 27项通过，含新增认证来源隔离反例 |
| 原NativeLifecycle归档/有界控制队列 | 修正上述新增测试后25项通过，保留首轮失败日志 |
| 原生来源绑定及collector | 12项、21项通过，分别验证实际来源与独立票据责任 |
| 共享seal纯验证、RPC和应用evidence | 144项、18项、5项通过，不包含退役WSS生命周期 |
| 离线候选CLI及原AOEM发布/回读 | 9项、1项通过；此组显式使用原离线Host参考，不能替代原生主链验收 |
| 原生真实四进程确认/恢复 | 6项通过，336.78秒；包括A/A/B、冷恢复、少票/冲突、就绪重试、最高/最低排序验证者原库追赶 |
| ML-DSA/UCA真实进程恢复 | 1项通过，137.07秒；40高度含1,024行大正文，落后节点在高度1追赶中断后重启，恢复后再花及四库冷恢复通过 |
| mainline门禁和CI范围 | 51字段一致性及原bin lockset反例通过；23项scope/严格汇总测试通过 |
| Windows/Linux原打包器 | 各10项二进制合同及7项拒绝反例通过；34项包安全检查通过；仅合成装配，不是实机安装资格 |

一次探索性`mainline_preflight::tests`筛选命中0项，未计作通过证据；有效门禁证据
来自实际bin lockset测试及scope/汇总断言。完整CI先列出并逐项核对六项`native_only_`
和一项`native_uca_process_`的名称，再执行；`full`只接受被选择任务的`success`，
失败、取消、跳过、缺失和未完成不能汇总为成功。应用安全门禁未被删除。

本次真实进程验收强制`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=0`并固定实际AOEM。
实际运行的是test依赖特性统一后构建的二进制，不能误用此前dev build的哈希：

| 制品 | SHA256 |
| --- | --- |
| Linux node | `eee3876d8ddd8f425bd98a220dae86d35e6267dd32607ad008ae77935ce2ad52` |
| Linux native client | `7b78660ded6280107d5a7aa5e3af5d0e52bb8d8f3a58dc7d2511c352efa8d99f` |
| 实际AOEM libaoem_ffi.so | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |
| 对应manifest | `e1443e482a3463d8102e06a2264a97c3057eb9d9a7ac59f8d2e571e678e1f4db` |

[本轮原始本地日志](native-only-retirement-local-evidence.zip)只收录上述明确列出的
29个日志/命令/哈希文件，共24,069字节，SHA256：
`2b0f6fcdf824480ba6e539cb1d04f13d40ba3870ca0d987c3827e46382f6dd43`。
不包含进程配置、数据库、用途钥、签票journal或钱包材料。测试后的实际二进制及库
再次核对，与上表一致；完整本地流水线退出0。

本工作包将在main集中提交推送，远端检查点仍为`49a7e65f`；最终CI结果必须绑定
实际新SHA，当前本地通过不预报完整CI成功。GitHub直连失败后，核实本机代理监听
在`::`:7897（首轮地址筛选漏掉该监听），仅命令级使用已有代理恢复连接；没有改动
全局网络、Git配置或SSH信任。未启链、未安装SCM、未部署、未替换正式库、无新TPS。

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

### 2026-11-06 冻结提交的自动完整CI及新失败现场

上述14个相关文件已集中提交并推送到 `main@49a7e65f488c137ba310ba5878631fa96dca81a3`。
经济/AGENTS/共同配置的其他未提交工作保留，未混入本次提交。
自动运行 [42125305209](https://github.com/novovm/supervm/actions/runs/42125305209)
绑定该SHA，`event=push`、`run_attempt=1`，选择的是完整检查，不是手工recovery范围。
截至本段初记，完整运行尚未结束，但恢复必检已失败，候选不能签收；没有重跑或取消。
本段属于取得结果后的本地回执，不为文档单独再推送触发完整CI。

恢复作业 `127588061705` 在13分39秒结束。`fresh_pool` 10项、`native_fresh_rpc`
18项、真实子进程回收反例1项全部通过；随后原
`fresh_genesis_main_nodes_continue_three_heights_without_restart` 失败，退出101，
测试总时长300.83秒。实际断言是 `native_seal_continuous_process.rs:354` 的
`continuous height 4 deadline`，由原每高度120秒业务窗口触发，尚未进入正常结束
及进程回收；不是15分钟步骤超时，也不是拆分回收时钟产生的误报。

公开现场核对：

| 观察对象 | 本次能确认的事实及不能越过的边界 |
| --- | --- |
| 拓扑及状态 | 在线node0/1/2、离线node3，门槛3。node0仍为高度3 `PublishedCertificateRelay`、`awaiting_successor_body=true`；node1/2为高度4、round0 `CollectingVotes`。CI编号不是实机A/B/C/D。 |
| 候选及投票 | node1/2的完整subject相同，proposal `404536ea…597bbc71`、parent `d574d767…4774f832`、pre-state `68c5182d…50dedd78`；两边已有票身份均为node1和node2，只有2票。node0没有进入该高度4候选。不能把这次对齐自动套回旧失败。 |
| 高度4正文 | node2向node0的manifest `cb08ebc5…f95473f6`、chunk `847d0587…115dee56`有 `queue_accepted=true`；node1收到并完成同一正文。该字段仅证明发送方本地入队，不证明实际发包、对端ACK或收齐。 |
| node0拒绝记录 | 所列3次 `body fragment has no active signed manifest` 对应高度3旧chunk `142c687a…448f8c4`；其余记录主要是投票不满足正文入口合同，不是高度4正文损坏的证明。node0仅记录一次高度3正文完成。 |
| 仍缺的证据 | 首轮及早期重投窗口未见高度4正文进入node0认证后交接点；接收采样随后达到64条上限，不能以之后无记录证明始终未到。当前缺口只能界定为本地入队到认证后应用交接之间；实际发送、ACK及重组证据不足，根因未确定。 |

原网络命名空间、单测试线程、实际AOEM选择器、票数及生产轮超时不变。
独立恢复作业使用新的runner和工作目录、不再等待完整Rust前置步骤；冷编译单列，
增加回收反例和有界观察。这些属于运行上下文差异，本轮没有CPU/内存实测，不能
声称物理资源完全同条件，也不能据此认定CI拆分导致或未影响此次停滞。

原始恢复日志保存在 `target/stall-freeze-ci-42125305209-recovery.log`；
artifact `fresh-ingress-history-recovery`（ID `13461586595`）已下载到
`target/stall-freeze-ci-42125305209-recovery/`。其中87个文件为26份process JSON、
30份stdout、30份stderr及1份测试列表。包含三个deadline快照和拓扑，
**没有原数据库、签票journal、配置、用途钥或成功acceptance报告**；这里只能登记
日志与公开现场保留，不能写成原库已保全或恢复通过。

冻结后的生产源码未再修改。两项消息处理反例的本地修复证据继续成立；
本次高度3→4停滞仍失败、尚未归因，旧停滞也未结案。不部署SCM、不触动B/C/D、
不替换正式库，不产生新的稳定性或TPS结论。
