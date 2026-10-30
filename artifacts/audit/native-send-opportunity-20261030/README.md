# 实际 tick、发送机会与 65K 吞吐复验

基线 `ca60694f2e65637e366b9df283993fab81d1bc5e`；开发分支
`dev/mldsa65-identity-closure`。本轮修复了真实新投递的空闲提示缺失，并校正 Windows
原 1 ms poll watchdog 的计时精度。**CPU 的两组同字节对照改善；GPU 未改善；
持续负载最高 3,217 TPS，12K 恢复任务仍未完成。**

不改 main，不部署、不清库，不替换正式 AOEM 库。不改签名、账户、费用、nonce、
授权、法定门槛、协议期限、容量或发送配额。调用链仍是原
`native_execution_tick → RpcLifecycle → NativeChannel → NativeObjectLink → NativeObjectSender`。

## 实际修改及证据

1. 原 `native_execution_tick` 现在复用既有默认关闭的阶段计时，记录实际
   status/encode/write/output、park/yield；不再以另一入口的 Idle=0 证明本入口无休眠。
2. 原 lifecycle 在网络 poll 后取得新的发送句柄，却未刷新进度 epoch。
   成熟 IdleBackoff 会把刚成为可运行的发送任务当作空闲。现在仅原 owner 的
   `admitted=false→true` 刷新一次；重复入队不刷新。真实 UDP/AOEM 反例在旧行为失败、
   新行为通过：[before](admission-before.log)、[after](admission-after.log)。
3. 原 Windows 平台适配 crate 增加作用域化 `timeBeginPeriod(1)`／`timeEndPeriod(1)`；
   在启动原 resident owner 之前取得，退出配对释放。原 park/unpark、IdleBackoff、
   1 ms watchdog 和协议期限不变；不引入忙等或后台线程。嵌套 owner/unpark 测试通过，
   不保证实时调度或 Linux 收益。计时精度请求的功耗/调度代价仍需部署长跑评估。
   新环境要求：Windows若拒绝1ms精度申请，配置节点启动会明确失败，不静默退回粗精度。
4. 原有界投递诊断增加 actual sender poll/等待分类、全局访问与配额计数。
   接收进度改取真实重组 Entry 的片数，TTL 新代不能与旧代拼成完成；区分首次接纳、
   活动重复和完成后历史命中。累计接收计数不随首次 handoff 快照或对象淘汰消失。
   原接受/拒绝/预算/TTL/完成条件没有变化；诊断关闭不增加逐片 tag、锁或分配。

## 计时问题确实存在，但不是全部瓶颈

诊断开启的 16,384 笔 CPU/GPU/CPU 两轮输入不同，以下仅为机制观察，不是性能 A/B：

| 条件 | CPU 每次实际 Idle 平均 | GPU 平均 | CPU 重复平均 |
| --- | ---: | ---: | ---: |
| 只加诊断、未修正计时 | 10.807–11.929 ms | 1.148–1.301 ms | 10.842–11.802 ms |
| 最终候选 | 1.183–1.268 ms | 1.175–1.290 ms | 1.165–1.322 ms |

代码请求都是 1 ms。不能把旧 CPU/GPU 差异全部归因验签，也没有证明 Vulkan 是计时差异的原因。
候选每节点 status+encode+write 仍累计 1.124–1.735 秒，占 I/O period 观察跨度约13–18%。
因此正式性能轮关闭全部诊断，诊断成绩不能替代下表。

两轮公开来源：[仅加诊断](diagnostic-before-16384.json)、[候选诊断](diagnostic-after-16384.json)。
同名raw.zip保存原始三轮报告，内含四节点最终本地快照和有界事件；精简JSON摘录phase与收发计数。
摘录是进程生命周期累计值（含启动和负载后查询），不是本文按事件窗口对齐的增量值；
嵌套/并发阶段不能直接相加成临界路径。两轮输入不同，不能作为严格before/after吞吐对照。

## 同字节、同初始状态、同制品的 before/after

保持 CPU 业务执行 `cpu_callbacks`，仅比较原版本与本轮版本；两者签名都为 ML-DSA-65。
同一候选 AOEM、1,024行、8 MiB展开预算、四节点全部最终回执终点。
所有轮次余额、费用、账户 nonce、授权预算、回执与冷恢复通过；慢样本保留。

| 负载/顺序 | TPS | P95秒 | P99秒 |
| --- | ---: | ---: | ---: |
| 16K before CPU | 2,315.66 | 4.644 | 4.692 |
| 16K after CPU | 3,162.18 | 3.263 | 3.294 |
| 16K before GPU-required | 3,044.15 | 3.218 | 3.287 |
| 16K after GPU-required | 2,979.15 | 3.450 | 3.474 |
| 16K before CPU重复 | 2,083.26 | 5.190 | 5.256 |
| 65K before CPU | 2,421.94 | 4.115 | 4.721 |
| 65K after CPU | 3,084.23 | 3.159 | 3.699 |
| 65K before GPU-required | 3,057.86 | 3.216 | 3.453 |
| 65K after GPU-required | 3,023.90 | 3.242 | 3.537 |
| 65K before CPU重复 | 2,588.28 | 4.040 | 4.266 |

CPU相对各组首次before提高36.56%／27.35%；GPU下降2.14%／1.11%，不登记GPU提速。
原始完整报告及其SHA：[16K](paired-16384.json)、[65K](paired-65536.json)，同名raw.zip保留原报告。
本轮一次集成了新投递提示和计时精度；不是二者各自收益的独立归因实验。

## 65,536 笔持续在途

8,192笔持续在途，后续生成签名/准备计时，动态合法有效期；相同批准创世和逻辑负载，
**不是同签名字节**。首次提交至四节点全部最终回执的原计时边界不变。

| 顺序 | TPS | P50秒 | P95秒 | P99秒 |
| --- | ---: | ---: | ---: | ---: |
| CPU | 2,904.69 | 2.562 | 3.542 | 3.929 |
| GPU-required | 2,905.61 | 2.695 | 3.264 | 3.391 |
| CPU重复 | 3,217.36 | 2.414 | 3.093 | 3.277 |

[持续供给原始报告](continuous-65536.json)。每轮65,536笔、账务与冷恢复全部通过。
P分位是客户端最慢一方观察回执的延迟，不是节点内部 durable timestamp。
本轮没有重现旧1,372 TPS样本，**不据此宣布全部慢轮次已修复**。

## 初发一秒与大量补片：已回答和未回答

候选诊断入口node0每秒实际有2,924–4,006次poll_io，不是整个主循环每秒只运行几百次。
入口每poll实际数据写15.74–22.21包；仅有active send的poll为26.96–35.99包。
入口41.52–55.92%的active poll耗尽64次数据写配额；四节点汇总只有约4%，会稀释入口压力。
全部匹配输入对象 pacing/backoff/send_blocked/UDP WouldBlock 均0。

原节点每对象一次最多32片，初发与补片共享，整个link一次最多64；每轮只访问每个job一次。
初发仍可推进时，sender没有无理由等待ACK。单对象的服务机会仍与其他对象及收发工作竞争。
入口最慢初发样本跨度1.193–1.319秒，192–212次对象visit，约每5.65–6.91ms取得一次机会；
同区间诊断快照占16.4–28.0%。因此不能把这一秒都叫sleep、签验或socket阻塞。

入口观察窗口非嵌套近似覆盖：原NativeIo约60–61%，其他lifecycle约13–14%，快照约17–18%，
Idle约6–7%。CPU业务执行owner累计约66–70ms，不能与主线程嵌套阶段重复相加。
NativeIo包含收包/认证/解码/反馈/实际发送/交接/诊断，**不是CPU利用率或纯syscall计时**；
其内部计算、搬运、系统调用成本尚未分离，也未证明调大64预算可以改善它。

剩余归因应落在原有函数，不新增网络owner：

| 原边界 | 已有证据 | 尚缺的耗时分解 |
| --- | --- | --- |
| `transport.rs` socket读取／`native_ingress.rs` FIFO交付 | 收包、入出队、深度 | 系统读取与队列处理各自耗时 |
| `endpoint.rs → datagram.rs → product_overlay.rs` 认证解封装 | 会话、重放、认证拒绝计数 | 外层编码、AEAD、内层校验分别耗时；不是逐片ML-DSA |
| `object_link.rs` ACK与Data/Repair处理 | 首次接纳、重复、缺口和收齐 | 原sender/reassembler处理耗时；现有后置时间戳不是socket首收 |
| `object_link.rs::poll_feedback` | 实际反馈写出 | 快照、ACK编码、认证封装、系统发送 |
| `object_sender.rs → datagram.rs` 初发／补片 | 配额、退出分类、实际写片 | 构片、封装与send_to；多轮跨度不是连续计算时间 |

`service.rs::poll`中的`NativeIo`计时结束后才进入`NativeAssembly`；整对象增量hash、
完成交接及业务解码主要属于后者，不能再混入上述60–61%。这五项也没有覆盖所有会话维护、
完成回收及查询回复准入，不做未经测量的时间加总或函数根因认定。

| 候选诊断轮 | 首次Repair填洞 | 活动重复Repair | 完成后Repair历史命中 |
| --- | ---: | ---: | ---: |
| CPU | 50,988 | 1,028 | 7 |
| GPU-required | 59,725 | 1,074 | 18 |
| CPU重复 | 56,627 | 936 | 40 |

对应匹配发送Repair写入62,531／74,680／69,618，47／48／48对象均一发送attempt、一组装代。
主要补片确实首次填入缺口，不是大规模重复已接纳片。完成后统计现在覆盖，但发送成功仍不等于
对端收到；未解释的差额不能当物理丢包。上述输入诊断不覆盖全部正文/控制/查询流量的逐对象归因。

已有预取／socket缓冲负向实验没有重新启用：
[历史记录](../native-proposer-input-20261030/README.md) 已试过仅空队列预取和64预取，仍发生30秒Expired。
不能把同一已撤回机制重新包装成新根因。

## 制品、验证与剩余门禁

候选位于本机 `target/native-send-opportunity-20261030/candidate1`：

- node SHA256 `f136c0c5b4bc3bc1ea78cc9086a84b14b510e370eeb9dbb7b1a36713dc24e74d`
- client `9f1c7d44559e80dfbda287994fbb55e9121aa5c1db4234c904babf17f30d870a`
- 性能轮 test runner `daf7288eeed6c6c4e6e4e4e8523525cf1d7523366bd1f1d485faeb0ad3ac315c`
- AOEM `8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`

网络完整回归904通过/11忽略；原生流水线521通过/124忽略，均单测试线程运行，不据此关闭
历史默认并发/WSS问题。新投递真实反例单独执行通过，不是把ignore当通过。
最终原生回归重复仍521通过；恢复命名空间真实UDP两项、新投递提示真实UDP/AOEM一项单独通过。
Windows作用域计时器测试通过；`cargo fmt --all -- --check` 与
`cargo clippy -p novovm-network -p novovm-udp-batch -p novovm-node --release --lib --tests --locked -- -D warnings`
通过。Cargo仍提示已有四个bin目标共用同一源文件，不把这条配置提示写成已经修复。

最终独立执行的GPU-required故障恢复通过，122.47秒：晚启动验证者、追赶中强杀后原库恢复、
到账再次消费、冷恢复与账务一致。[完整证据](gpu-recovery-final-isolated.json)、
[执行日志](gpu-recovery-final-isolated.log)。第一次141.93秒的恢复运行后段曾与A/A/B测试重叠，
保留为 [first](gpu-recovery-first.json)，不把它称为独立运行或用于性能比较。

性能制品冻结后仅改`cfg(test)`中的诊断断言和下述A/A/B夹具，未再修改生产执行代码。
最终测试runner SHA256为 `ddbaa81ec4815f914a3aff78b61f322ddcc1a3f8b1bb5adf823cd0e4f9fe7052`，
与性能轮runner区分记录；最终回归/恢复仍使用同一`f136c0c5…`节点和上述AOEM库。
诊断数据、测试日志和所有原数据库/journal在本机保留；公开证据不包含私钥。

剩余：12K恢复未完成；NativeIo内部成本和未到片原因未全部解释；旧极慢轮次没有逐对象
完整历史，不能声称彻底根治；Windows单机四进程不代表Linux、多机或发布资格。
本轮没有触发完整云端CI，最终身份迁移批次冻结后按既定门禁执行。

## 验收时发现的 A/A/B 夹具补送遗漏

先保留失败，不用开启诊断后的通过覆盖它。候选无诊断失败后做了交叉验证：
新node+旧runner通过，旧node+新runner无诊断仍失败；状态输出可以改变时序并掩盖窗口。
这将核查范围转到故障注入端，而不是盲目撤回计时修正或放宽投票规则。

有限投递记录在 [原回复失败](aab-old-reply.raw.zip) 中确认 peer1（实际node2）：

- regular正文 `a4e9e8b4…a5b908fdd` 在6,205,885μs取得传输Delivered；
- 提案在6,205,925μs取得Delivered；
- 同一regular正文再次admitted于6,207,465μs、Delivered于6,210,572μs；
- 仍没有指定prevote。

原节点拒收未被已验证提案需求引用的正文；transport的completed去重不能因此撤销。
FaultPeer收到RequestBody后仍调用regular prepare，而生产早已使用
`prepare_recovery_reply`。夹具现在复用同一恢复接口，正文、候选值、签名不变。
并强制先等regular正文传输完成再注入提案，使此前概率窗口成为验收条件。
原round0指定A/A/B真实票、投票期限、单票不能推进、法定门槛与原journal恢复断言保留。
新增独立最多4秒的正文投递等待，不声称测试总等待上限未改变；生产期限未放宽。

强化夹具关闭诊断后连续两次独立通过，17.39秒／17.40秒：
[第一次](aab-fixed-isolated.raw.zip)、[重复](aab-fixed-repeat.raw.zip)。原报告SHA256分别为：

- `eae35f68239cfc67a5b448fc2ec41f16f6e43030b2447fec615eaa3503fb372a`
- `9080f2b919d25bae0574e253e6f989ab628f39a2c2b301994c6db803c31f1fcd`

第一次中node0/2/3先完成普通正文投递，再分别于启动后118/118/109ms观察到真实A/A/B票。
node0与node2实际发出RequestBody，回复使用与regular不同的recovery fragment；node0从收到
请求到回复DONE约4.6ms。单票保持6秒未推进，随后三份真实nil precommit自然换轮，首高度
在round1确认；三诚实节点使用`existing`重启原库，后继交易及两高度的余额、费用、nonce、
回执和状态根一致。不是修改法定门槛，也不是制造QC。此项是夹具正确性修复，不计入TPS收益。
