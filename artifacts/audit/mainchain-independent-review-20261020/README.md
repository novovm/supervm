# NOVOVM 主链独立复核：迁接遗漏、发布资格与下一步

日期：2026-10-20（北京时间）。固定源码：`9bcf9c582c1592d1916e9368c7a99cffa53acc45`。

审阅期间已fast-forward同步另一设备的`c480ddad`；相对固定源码只增加两份网络/台账
文档，没有修改本次所审程序。保留对方更新，不将手机验收扩大为validator验收。

## 结论与范围

**当前原节点具有真实签名交易、AOEM 执行、耐久 BFT 发布、查询与同高度冷恢复，
但还不能签收可生产主网。首要问题不是再造执行器或共识，而是长期验证者生命周期
与历史追赶没有完整迁入当前唯一主线。** 高吞吐、UCA、隐私、抗量子和完整业务证明
仍按既定产品目标分别交付，不能用正常转账成功代替。

本轮完整阅读了用户粘贴的另一台设备审计与 ChatGPT 评审，并独立追查当前入口、
发送队列、握手、会话替换、归档、签票/发布、鉴权、原生查询、部署脚本和制品身份。
另一台机器 `D:/WEB3_AI/SUPERVM/artifacts/audit/multi-device-architecture-review-20261020/README.md`
原件不在本机；不声称已经取得或读完它。本报告以粘贴内容和本机 Git/源码为证。

`fc7e6615` 是当前 HEAD 的祖先；`fc7e6615..25bd5493` 为687提交，与另一设备记录一致。
`25bd5493..9bcf9c58` 只改两份文档，重点程序实现未变化。本轮没有逐个完整审阅687个
提交，也不是全仓逐行安全证明；历史因果仅对下列具体路径核查。

- 三个有界并发审阅只读核对活性/重连、归档/共识、鉴权/打包；主控独立交叉验证。
- 本机独立执行 `cargo test --release --locked -p novovm-consensus --lib round_bft -- --test-threads=2`：
  **86 passed / 0 failed / 0 ignored，0.49秒**。构建13.59秒。
- 本轮未运行离线饱和、晚启动、单节点重启、追赶中断实验；下面前三项是控制流确认，
  **不是新故障复现 PASS/FAIL**。未重跑 TPS、未部署/扫描其他机器、未改程序或运行数据。

## 一、需要先关闭的恢复链缺口

### P1-1：一个不确认的验证者可最终阻断健康节点广播

实际入口：`bin/novovm-node.rs:35387,44088` → `RpcLifecycle/ResidentNode::poll`
（`native_pipeline/service.rs:160–171`）→ `consensus/native_lifecycle.rs`。

- `native_lifecycle.rs:280–300,422–429`：所有目的端 `fits()` 都满足才入队并消费
  当前耐久 outbox。已有 peer/lane 轮转只能服务已入队对象，不能绕过此准入屏障。
- 生产计额 `service.rs:410–412` 为每peer32条/4MiB；单消息上限512KiB。
- `native_lifecycle.rs:235–239`：Expired 只清 admitted 以便重试，不移出 delivery；
  `:321–324` 高度变化只清 requested-body 集，不退役旧发送义务。
- `progress.rs:422–423,442–443`：outbox未被取走时，安全推进/计时器及后续journal
  ACK处理等待。因此一个长期不返回传输完成的配置peer就足以触发有限队列屏障，
  无需伪造签名；30秒TTL重试不释放名额。

这不是BFT安全状态机票数设计错误，而是传输调度给共识增加了“等所有目的端”的
不必要前置条件。属于正常单故障运行能力缺口，不应降为额外保险。

复用：保留的旧 `consensus/controller.rs`、`runtime/novovm-host/src/consensus/controller.rs`
有逐目的端 flush/retry、随高度/轮次退役实时消息及归档替代义务；后者
`:1536–1593` 和 `RetrySchedule:265–302` 可供对照，原调度反例在
`runtime/.../controller/regressions.rs:134`。优先迁入当前 NativeLifecycle，不能
重新激活第二控制器或强制WSS。**不能简单把 all 改 any、增大队列或丢掉未送工作。**

### P1-2：长期验证者缺少完整的认证重连

- `consensus/native.rs:178–180`：单次握手 lifetime30秒、retry50ms。
- `network/src/novorudp/endpoint.rs:247–281,688–690`：bind时建立尝试；固定validator
  的 streams=None；过期尝试被跳过，没有新的attempt owner推进。
- `novorudp/handshake.rs:172`：过期attempt仍拒收；既有transcript重试不等于接受新会话。
- `transport/native_ingress.rs:110–124` 确实启用了普通客户端reconnect；但
  `endpoint/reconnect.rs:77–79` 要求已建立的open client及streams Some，固定validator不符合。
- 即便只放开endpoint，`native_lifecycle.rs:244–245` 对SessionReplaced仍报错并冻结；
  原tick随后drain并退出。必须贯通两端，不能只加一个开关。

**反证/限定：健康会话不是每30秒断开。** `endpoint.rs:649–665` 已建会话接收不检查
握手TTL。问题是晚启动/失联后新attempt及认证代际交接，不应取消安全期限或重置旧凭据。

复用：`endpoint/reconnect.rs:127–165` 已实现fresh-secret确认后替换，不是仅凭signed offer；
`object_link.rs:516–532` 已有旧validity、重组/ACK/sender撤销及保留原对象句柄的返回。
这些是可复用的通用机制，但validator须保留原固定身份/流策略、共识状态和签票日志。
旧duplex worker的单peer恢复原则/反例可以迁用，不把原生载体改回WSS。

### P1-3：网络历史追赶编排未迁入，不是没有归档或AOEM恢复

- `consensus/mod.rs:5–8` 明确旧controller/channel不再编译。
- `native_lifecycle.rs:404–405` 将历史请求落入拒绝分支，没有活动RequestDecision发送者。
- 所需二进制 `RequestDecision { context }` 已在 `transport.rs:51–56,206–216,338–353`；
  `message_evidence.rs:167–170` 已有请求域验证。不需要另造网络协议。
- `chain.rs:384–409,435–538` 的ArchiveRead已存在，负责本地可信前缀下的记录、决定/QC、
  候选与父绑定核验；当前主要用于本机查询投影。它不是远端同步器。
- `progress.rs:373–383` 保留完整当前context精确校验；不能靠放宽此条件、改本地高度
  或相信对方声称的最新链头来“补齐”追赶。

保留的旧 `consensus/controller.rs` 有完整可复用链：

1. `1743–1756`：有界周期请求本机当前精确parent对应的下一决定，不需要先信远端链头。
2. `1783–1857`：同域、请求高度与本地已决定范围核验，逐peer有界归档读取/缓存。
3. `1861–1897`：读取完成后重检peer请求、parent与本地状态。
4. `1597–1610`：先发小型决定，正文按精确body_id再取，不反复盲发整块。
5. 请求方复用当前CandidateBodies/Pipeline独立鉴权执行，再走同一journal发布。

当前短时落后可能靠仍保留的实时消息补齐，但不是可靠多高度离线追赶。
复制对方数据库、只设高度或替换状态根均不算完成。

## 二、迁接遗漏的可定位因果

以下为Git作者时间，均+08:00；不是推测代码最早发明时间。

| 提交 | 时间 | 本次相关事实 |
| --- | --- | --- |
| `78183da8` | 10-02 09:55 | autonomous controller加入归档追赶及有界发送机制 |
| `d431a74b` | 10-04 03:07 | 这些controller能力迁回原node |
| `a0755a0d` | 10-15 21:26 | 有界UDP单次握手/repair适配；单次期限本身不是缺陷 |
| `3a9dfcce` / `b20f8ac8` | 10-16 00:49 / 01:51 | 固定endpoint尝试及普通客户端重连；不是长期validator恢复闭环 |
| **`f5c2fcd6`** | **10-16 04:48** | 原tick/native生命周期替换时，加入all-peer屏障和SessionReplaced报错，旧controller退出编译，archive未迁 |
| `9978026e` | 10-20 12:01 | follower提前独立执行优化；不是上述恢复缺口的引入点 |

`docs/NOVOVM_JULY_MAIN_INTEGRATION_MAP.md:155–163` 当时已经登记原生离线满队列、
归档和实际进程故障回归缺口。**不是全新的需求，而是已知缺口没有被当作整合退出门。**
七月结构传输资产不应因此废弃；十月已做好的恢复机制也不应因路线纠偏而遗失。

## 三、另外五项参考发现的复核

### UCA：严格直接签名已接，完整账户授权未接

`ingress/authentication.rs:112–170` 固定Ed25519公钥+签名96字节，strict verify后
直接绑定20/32字节付款地址，nonce身份绑定链域与公钥；不消费canonical UCA设备、
用途、撤销/轮换/恢复关系。这不是“无验签”，也不能直接据此声称任意人能盗花。

已有V4规范wire（`protocol/src/uca_transaction.rs:88,107`）、双签管理委托
（`adapter-novovm/src/uca_delegation.rs:115–204`）、UCA效果/父绑定
（`uca_state.rs:197–219`）和持久记录。**管理委托在uca_delegation.rs:160明确拒绝
充当花费权限，不能直接拿来授权Transfer。** `tx_ingress.rs:30017`附近还有V4未接
canonical state时fail-closed的明确反例。应接回已有体系，不新造账户系统。

### 完整业务证明/PQ：未成为当前交易与共识资格

参考引用的 `service/rpc/native_query.rs:17–35` 是历史JSON展示，不是当前原生wire。
真正原生Query::Receipt/Receipts在`:78–89`返回类型化Answer，不经该JSON转换。
更直接的证据是 `protocol/src/native_query.rs:146–154`：Finalized明确为耐久BFT发布，
不作ZK声明；当前决定到journal ACK/本地发布没有完整NOV业务证明校验门。

这不否定BFT最终性，也不否定AOEM已有统一ZK能力；只能说“完整业务证明尚未接入当前
最终确认”。`aoem-bindings/examples/proof_engine_host_smoke.rs:223–279` 有统一
resident proof workload；已有完整NOV关系/反例也应复用，不能再提升为后端直连旁路。

Transfer V3与 `round_bft/wire.rs:263–282,362–387` 当前均用Ed25519。
AOEM bindings已有ML-DSA符号/批验封装，不等于该入口或票据已抗量子。
完整业务证明、PQ交易/封印、隐私资产仍须原产品路径分别验收。

### 自动联网：当前validator静态，不等于NOVONET没有能力

`service/config.rs:35,56–59,167–169` 要求所有远端validator的公钥+SocketAddr；
`service.rs:349–359` 原样交给NativeChannel。当前owner未消费自动发现/NAT/relay/多跳策略。
库中 `adaptive_overlay.rs`、`overlay_runtime.rs:201–251,303–442` 等能力仍在。
旧product_mainline_overlay owner在当前配置下被显式禁止并行启动，不能直接开旧开关。
固定可达地址拓扑可先验恢复，不要求本轮重写自动路由；公网NAT可达性仍需实际核对。

### Linux包：输出路径与启动模板均不匹配现行节点

- `.cargo/config.toml:6` 指定target/runtime-rebuild；打包脚本`:45`未固定target-dir，
  `:60–62`却从target/<Target>/release复制，可能找不到新文件或取旧同名程序。
- `scripts/novovm-package-product-linux.ps1:219–244` 缺当前CONFIG与明确START，
  还启用当前owner禁止并存的旧overlay。问题不只是mode字符串，不能只改名字。
- 应复用Cargo本次compiler-artifact/实际target目录及当前ResidentConfig，绑定构建、
  复制、安装和实际加载身份。独立一次性create与正常existing重启分开。

### 性能制品：同库A/B有效，不是随包库的资格

本轮对磁盘文件重算SHA256，并与八轮测量逐项核对：

| 身份 | SHA256 |
| --- | --- |
| overlap八轮共同AOEM | `95b7133d5f04a10b2d782d06d57f208f8ef387e3131668418f7096f58d399307` |
| Windows随包core | `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947` |

八轮确是65536成功/0业务失败、共同库的旧新node对照，不能因此抹掉执行重叠收益。
但CPU12324/GPU6546的有限样本不能贴给另一份发布库或当作多机容量。跨平台DLL/SO
也不要求相同hash，而需各自固定、契约兼容，并对实际部署包复验完整交易/恢复及性能。
本轮没有重跑上述性能，未更新SDK。

## 四、本轮额外发现与解释修正

### P1：旧故障测试退出编译，没有等价产品门禁

`consensus/tests.rs:8–13` 明确controller_integration、resident_rpc_process的aab/offline
等旧套件不编译；progress_split是同进程driver，不是原生真实进程A/A/B。
`native_product_load.rs:879–922` 是四节点同高度确认后一起停、一起existing启动。
当前laggard fixture仍持续poll_io、每高度等追齐、仅3高度；minority晚启动窗口在15秒内。
这些不能覆盖32条/4MiB饱和、30秒attempt后启动、单节点失联后多高度追赶。

旧 `controller_integration.rs:703–726`、`resident_rpc_process/offline.rs:338–351`
有真正晚到节点归档执行追赶反例，应迁接到当前唯一原节点入口。
**CI通过只证明实际执行的门禁；未编译的测试不算覆盖，也不能继续引用旧成绩为新路径背书。**

### P1操作安全：打包 -Force 可越界删除

`novovm-package-product-linux.ps1:27–32` 将PackageDir拼接后直接递归删除，未规范化
并限定专用包输出子目录。只读路径解析已确认：`.`指向SUPERVM根、`../AOEM`指向兄弟仓。
本轮未运行打包/删除。需先限制绝对目标、拒绝根/仓/兄弟路径，并先完成输入/构建核验；
不能用“调用者不会传错”替代保护。该风险是本地操作破坏，不是已证明的远程链上攻击。

### P2：SDK消费说明落后，不应拿旧字段否定完整代数执行

`aoem/aoem-sdk-manifest.json:37` 仍写optional integer prediction / CPU fees/ordered repair。
当前 `business/nov_transfer_batch/full_effects.rs:401,762` 已有完整条件效应计划编译与委托，
不能用旧manifest断言现在仍只有预测。反过来，统一入口/完整计划也不替代业务ZK证明。
包说明、当前消费者范围、能力契约和真实测量应在最终制品核验时一起对齐。

## 五、已经正确的资产应保留

没有在本次范围内发现“收到peer QC便直接改链头”的旁路：
owner-bound耐久候选、精确父状态、本地执行statement与完整证书仍受验证。
`round_bft/journal/mod.rs:508–537,683–780` 将签票状态/outbox/归档与ChainHead/ChainBlock
放入同一条件变更集合，只有Applied/AlreadyPresent ACK后才暴露消息/新链头，错误冻结。
这些安全边界不能为修活性而删掉。CPU/GPU后端共用同一候选/发布权威；本轮未发现
仅选择后端就创建第二个活动链头。

原node/exec/bindings/AOEM、结构批、严格鉴权、nonce/费用/失败、状态树、耐久签票、
完整整数条件图、执行重叠都不是无用成果。EVM/nETH/治理/隐私等原资产仍须按各自真实
入口接回并验收；本轮不以“文件保留”重新签收这些完整产品能力，也不授权删除它们。

## 六、主控应承担的具体不足

1. 迁接验收偏重正常快路径和吞吐，未把“单节点离线不拖停、独立恢复追平”设为成套
   集成退出门。已写在源码/替换表里的限制，应是P1，而非可无限顺延的保险。
2. 复用了执行/journal，却没有成套接发送退役、validator重连、归档及原故障反例。
   “原模块恢复”或“使用同一语义入口”都不能说明这些生命周期已经恢复。
3. 对测试资格跟踪不足。四库一起冷重启、同进程分票、旧路径的离线PASS不能自动
   继承到新入口；当前实际进程故障门禁缺失应在每轮汇报前列出。
4. 性能证据虽然记录了hash并注明未换SDK，却未及时把发布库实测资格闭合。
   有效优化结论与可交付包资格是两件事；不能让用户靠另一设备审计才发现差异。
5. 设备清点/构建可以继续，但此前部署指引应更早突出上述功能缺口，而不只说缺跨机工具。
   公网mailbox/手机成功不替完整validator；尚未对生产服务进行覆盖或主链上线。
6. 连续文档提交取消CI会留下未结束的提交级门禁。cancelled不等失败也不等通过，
   后续应按完整交付批次集中记录，不能用“仍运行”长期替代最终结论。

## 七、建议的有限整合顺序与验收

下一步是修复建议，不是本轮已经实施或通过：

1. **逐目的端调度/有限退役 → validator认证重连 → 历史决定/精确正文追赶**作为一条
   完整恢复链接回当前NativeLifecycle，保留原共识、唯一journal与AOEM状态owner。
   不仅改all/any、加队列、缩超时或清锁；不得降低3/4，不复制数据库。
2. Linux本次产物/模板/删除边界与制品身份可并行修最小适配，在云主链实际安装前关闭。
3. UCA/PQ/业务ZK/自动路由按已有能力和真实资格另阶段接合，不趁本片重写协议。
   百万TPS目标不变；保留CPU和完整代数后端回归，但先恢复正常故障下可持续推进。

| 必须在当前原节点路径验证 | 不能用什么代替 |
| --- | --- |
| 1个peer长期离线，跨过生产32条/4MiB旧屏障，健康3个持续推进、资源有界 | 只跑首块或增大队列 |
| 第四节点超过初始30秒attempt才启动，其余不重启，两个身份排序均能认证 | 15秒内晚启动或取消安全期限 |
| 同身份/原库独立重启，新会话fresh-secret确认，旧代际帧/ACK/正文拒绝 | 清签票日志或新建签者 |
| 离线产生多高度后只靠归档与原AOEM独立执行追齐 | 钱包重新喂全部数据或复制DB |
| 追赶中再次崩溃后继续，不重复收费/nonce，回执/根一致 | 四节点同高度一起冷重启 |
| 两个不可用仍不最终确认；分票恢复不冲突决定 | 在线人数动态降低法定人数 |
| 本次Linux包真实安装/启动及实际库资格 | 有一个同名旧bin或tar文件 |

先用受控单机多进程定向复现/修复，再用真实多设备同创世复验。各设备的同步、
清点、release构建继续；当前版本不作正式多机长跑、发布包TPS或生产签收。
LAN与含阿里云的公网拓扑分别报告，不把任一有限样本称为硬件极限。

## 证据状态

源码/运行库身份及本轮实际测试摘要见 [evidence.json](evidence.json)。
参考CI `39382347662` 已cancelled；本轮观察的源码CI为 `39382770104`，
已于07:27:18 UTC结束为cancelled，Rust作业的制品上传另报failure，不能算全绿。
同步后`c480ddad`的CI `39386580513`在07:32:19 UTC仍in_progress，
这里只记录时点，不预告通过。CI真实结果需按其提交单独复核。
本审计文件/说明的提交不是修复程序；上线资格仍由上述真实验收决定。
