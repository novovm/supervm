# 原网络成本与旧优化迁接核对

后续代码与新测量：见[2026-10-31原APFL资产正文引用实现及完整对照](../native-apfl-reference-20261031/README.md)。
普通Controller账户规则固定后的最新原NovoRUDP批量系统发送、65K同字节/持续供给及恢复结果，
见[Controller V3投递与吞吐记录](../native-controller-throughput-20261031/README.md)。
本页保持原历史样本，不将后续收益回填为本轮已解决。

基线 `444133b78863a4593ac1850f5a5d9758aadee308`，开发分支
`dev/mldsa65-identity-closure`。原入口仍是
`native_execution_tick → NativeChannel → NativeObjectLink → NativeUdpSession`。
本轮没有修改账户模型、ML-DSA、业务执行后端、共识、超时、发送配额、正文容量或正式 AOEM 库。

**结论：旧优化没有整体丢失；网络逐包成本已经量化，但性能恢复仍未通过。**
本轮减少缓冲区复制的中间候选没有证明整链提速，GPU 负向结果与旧基线慢重复均保留。
当时最终候选复验受 D 盘余量约 1.5 GiB 阻断。用户清理编译产物后，已完成下方
“最终制品补验”：八轮65K、冷恢复及四项独立进程故障验收通过，但仍约3K，未恢复12K。
本轮不删除任何测试库、签票记录或证据，也不以磁盘清理解释此前慢轮根因。

## 旧优化迁接对照

对比固定提交 `9978026e` 与 `444133b7`。下表是源码核对；已有历史测试不自动成为本轮新验收。
旧 12K 的来源是 [原冻结报告](../native-local-overlap-20261020/README.md)：
Ed25519/V3、1,024 付款方 × 64 nonce、65,536 笔、8,192 在途、64 高度，
CPU 业务后端 12,324.09 TPS，四节点全部最终回执终点。旧 GPU 列是整数业务执行，不是 GPU 验签。

| 能力 | 旧实际位置 | 当前调用与状态 | 对应证据/反例 |
| --- | --- | --- | --- |
| 持续在途、1,024 行 | `native_product_load` 原驱动 | 同一负载 owner；当前已实际 64 × 1,024，不再是几十行 | `native_asset_continuous_65536_final_receipts_and_cold_economics`；容量报告 |
| follower 当前高度提前执行 | `service/rpc.rs` 去除 leader-only 门禁，`CandidateBodies` | 当前 `poll` 的本地选择仍可供 follower 准备；有效提案后才注册/投票，未漏接 | `native_lifecycle_follower_preparation_cpu/algebraic` |
| 后继 Lookahead | `consensus/bodies/successor.rs` | `successor_selection → offer_successor_admitted` 使用完整执行根；原 journal ACK 后才提升，资产路径已迁接 | `native_successor_uca_complete_state_waits_for_ack_and_recovers`；`remote_body_waits_for_own_parent_without_blocking_current_body` |
| 同父根批捕获/seed | `pipeline/driver.rs::advance_with_seed → try_read_nodes` | 同一 SeedCache；`native_query/uca.rs::retain_capture_read_seed` 已交回同一 locality slot，不缓存权限结论 | `captured_seed_requires_full_parent_and_domain_and_never_binds_another_head`；压力反例 |
| 全局轮询量子 64 | `native_lifecycle::channel.poll_io(...,64)` | 仍为全局 64，不是每 peer 64，未缩小 | 原 object-link 公平、预算及离线 peer 测试 |
| 单对象批次 32、ACK 等待 10ms | `service.rs` 原发送 policy | 参数保留；初发/补片公平及重试游标另有正确性修复 | `lost_early_repair_retries_while_unrelated_initial_progress_continues` |
| UDP 系统调用批发送 | 旧 native `NativeDatagramIo(UdpSocket)::try_send` | 两版都逐包 `send_to`；32 是轮询批次，不是 OS sendmmsg。`novovm-udp-batch::sendmmsg_batch` 仅 Linux 实现、只被 network-only gate 消费；Windows 该接口返回 Unsupported | 不得把 Linux 辅助 gate 当旧 Windows 12K 产品能力 |
| 接收 drain-first/scratch | `transport/native_ingress → receive_packet_v1` | 原 FIFO、有限预取和 scratch 没有被绕过 | `receive_scratch_keeps_exact_payloads_and_original_drain_bounds` |
| 工作进度/Idle | 旧 IdleBackoff 请求 1ms 休眠 | 发送入途进度在 `444133b7` 补回；原哈希进度在 `c5c18d46` 补回。旧版也漏进度，不是 ML-DSA 特有成本 | `native_delivery_admission_refreshes_idle_hint_once`；`hash_progress_preserves_quantum_and_verifies_before_completion` |
| APFL early/bind | 旧 controller 下存在旧 V3 实现 | 旧 12K 实际跑 NativeLifecycle，未调用该 wire early/bind；不可登记为丢失的旧 12K 优化 | 本地 Lookahead Bind 与 wire early/bind 是不同机制 |
| 正文身份/计量编码 | 旧 `bodies.rs` 存在重复编码 | 当前 `candidate_body_id_and_wire_len` 已复用编码长度 | 已有测量不支持它是秒级等待主因 |

因此不再以“还没接 Lookahead/seed”解释当前 3K；也不通过另建网络、重新实现上述能力继续叠层。
当前 16 付款方的依赖分布与旧 1,024 付款方不同，历史 12K 不是同语义算法 A/B。

## 原网络内部测量

沿原 diagnostics 增加默认关闭的 `NOVOVM_PROFILE_NATIVE_IO=1`。同一 poll 线程用固定数组累计，
每 61 次调用采样一次；没有逐包日志、字符串标签、互斥锁或新增调度器。
`run.ps1 -IoDiagnostics` 关闭逐对象/交易大快照，沿原进度输出累计值，每 250ms 输出。
样本是进程生命周期、包含启动收发的墙钟估计，不是 CPU 利用率；父子嵌套区间不能相加。

16,384 笔 CPU 诊断前样本 [完整记录](io-before.json)，入口 node0：

| 原边界 | 调用次数 | 按采样均值 × 调用次数估算 | 与原 NativeIo 墙钟的关系 |
| --- | ---: | ---: | --- |
| 整段 NativeIo | 25,762 | 4,444.5ms（原全量计时） | 100% |
| socket_send 调用/写出尝试 | 449,059 | 2,075.6ms | 约46.7%，计数不单独证明写出成功 |
| secure seal | 448,700 | 1,144.2ms | 约25.7%，其中 AEAD 约811ms |
| secure open | 242,816 | 636.1ms | 约14.3%，其中 AEAD 约465ms |
| receive_packet | 266,211 | 276.5ms | 包含 socket recv/队列/观测，非纯系统读取 |
| frame 准备 | 437,269 | 208.9ms | 包含分片及校验准备 |

外层 envelope 编解码、ACK 和 receive accounting 明显小于以上前三项。
node0 的 24 次 status/encode/write 合计约8.9ms，未再占此前诊断轮的17–18%。
`sender_poll` 包含 seal/socket，不能再加入总和；不能把这些采样比例直接套成正式负载的固定上限。

## 删除的重复工作与未改动的边界

原 `frame.encode → Aead::encrypt` 先分配完整明文，再复制到新的加密 Vec。
当前编码复用原字段/校验算法，预留16字节 tag，直接在同一缓冲区加密。
原 native 解码已经持有密文 Vec，现在直接消费该 Vec 做原地认证解密，不再复制一份明文工作缓冲区。
不是省略加解密、校验和、nonce 或 replay。借用接口仍在路由/nonce/replay 检查后才复制密文，
不复制两个身份字符串；其他调用方没有被要求换接口。

新反例逐字节对照原分配式 AEAD，覆盖空/短/满载、可变 frame checksum、篡改 tag、
认证失败后合法包仍可接纳、已接纳包重放在申请明文缓冲区前拒绝。
未触碰接收重组预算、发送完成/取消句柄、控制保留槽或新旧密码路线。

[短诊断候选](io-after.json) 为 2,857.70 TPS，对照 2,927.36；输入不是同字节，包数也改变，
不能签吞吐收益。每包 seal 的采样均值略降，不等于整链必然改善。

## 65K 同字节中间候选

[五轮完整证据](paired65k-intermediate.json) 使用同一创世、初始根和签名字节，
业务后端均 `cpu_callbacks`；CPU/GPU 只改变 ML-DSA 验签后端，GPU-required 不回退。
1,024 行/块、8 MiB 展开预算、8,192 在途、四节点全部最终回执，诊断关闭。

| 顺序 | TPS | P95 秒 | P99 秒 |
| --- | ---: | ---: | ---: |
| 旧 CPU | 3,068.71 | 3.328403 | 3.789420 |
| 候选 CPU | 3,054.55 | 3.413090 | 3.890900 |
| 旧 GPU-required | 3,042.25 | 3.492241 | 3.617278 |
| 候选 GPU-required | 1,965.22 | 13.002543 | 14.462862 |
| 旧 CPU 重复 | 1,375.57 | 15.523081 | 23.140700 |

全部原经济/授权/nonce/费用/回执与冷恢复核对通过；没有稳定提速，12K恢复未完成。
未修改的旧 CPU 重复也出现慢轮，不能单凭这一轮把所有负向变化归因于缓冲区修改。
这不是持续签名供给成绩。中间候选运行后修正了借用接口的额外字符串复制，最终库/节点不能冒用此表签收。

## 整份正文传播的具体剩余问题

16K 诊断四节点共1,117,319次 seal。下一提案者的 Lookahead 早发有27份 peer 投递责任、
92,836,395字节；`served_bodies` 合计96，`requested_bodies` 合计19。
这些是入队/请求计数，包含去重接管，**不是96份已发送完整正文或物理丢包率**。

实际结构：客户端输入 → 入口单次 fanout（peer 收到不再全网 gossip）；
后继提案者另发带上下文的完整正文；缺正文显式请求；另有归档 Decision DONE 后的自动完整正文续送。
后三者不能跨用途仅按交易内容合并传输身份。共享归档缓存只减少读/编码，不消除多peer网络份数。

已在原真实归档夹具确认“仅请求Decision也会入队Body”，但**没有直接取消这种续送**：
原 `missing_bodies` 允许同验证者两份offer。ArchiveRead(A)未完成时收到 RequestBody(A)、
RequestBody(B)，单个待回复 hint 可能只剩B；自动续送仍覆盖A的恢复责任。
因此“只记最后一个body_id再关闭自动续送”会留下合法恢复缺口，不能用它换TPS。
这项候选政策检查未留在活动测试中充当已有合同缺陷，运行日志保留；生产续送机制保持原样。

当前可定位的工作量是：大正文的逐包 socket/AEAD 成本，以及跨输入、后继、归档用途的整份传播。
尚未量化96次回复中真正多余的完整线上字节；慢重复轮也未取得逐对象因果证据。
后续需要沿原归档请求 owner 理顺精确需求与投递责任，不能只关自动续送、扩大64配额或增加线程。
这是具体剩余边界，不是宣称一次复制优化已经修完网络。

## 制品与验收边界

- 原节点基线 `f136c0c5b4bc3bc1ea78cc9086a84b14b510e370eeb9dbb7b1a36713dc24e74d`。
- 诊断前节点 `73c5a4e2f7e8f4ef928b12f429b668432a29b2ab40adc4c23a0c071183d31aa0`。
- 中间性能候选 `e16a35936c45902f5b83774723d854ddf1ca0b83183d3edc38986dcfba61cae4`。
- 中间runner `dd1b0fb43efbb85ce2d2c4d50a32154fecf7a7aae26b0a2c80a10f6421355220`。
- 最终本地节点 `ff14f1a3c397694819053f736c56094a4b25c86ce9e2906955abe35631a57b6e`。
- 最终runner `b380aa1af020b62d926892d914b9cb7b5aa138c665130562eb82d495f9890910`。
- AOEM `8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`，候选隔离路径，未替换正式包。

中间检查：网络906通过/11忽略；原生521通过/124忽略，均串行；严格 Clippy通过。
不据此关闭历史默认并发/WSS问题，也不声称 Windows 1ms计时精度要求已完成所有部署兼容验收。
最终借用接口修正后：网络906通过/11忽略、原生521通过/124忽略（均串行）、release构建、
严格 Clippy 通过。最终节点/runner保存在 `target/native-io-cost-20261030/final`。
此处原记录的完整65K、持续供给及四进程故障恢复待验，已由以下同制品补验更新；
上面的中间候选数据仍原样保留，不被新样本覆盖。
main、云端完整CI、正式随包库、B服务、部署与旧库均未改动。

## 2026-10-30 最终制品补验（源码 ba9824c3）

用户释放空间后，启动前可用15.21GiB；未重新编译、未修改运行代码或配置上限。
继续使用上列最终node `ff14f1a3…`、runner `b380aa1a…` 和同一AOEM。
原CLI源文件本片未改，恢复测试复用已验证的客户端 `9f1c7d44559e80dfbda287994fbb55e9121aa5c1db4234c904babf17f30d870a`，
放在隔离node目录旁，不替换随包程序。各套件独立顺序运行，无并行构建/压测。
Windows PowerShell脚本启动曾被执行策略拒绝，测试未开始；随后直接运行原测试程序，
设置与原run.ps1相同参数，并显式清除asset trace；未修改执行策略。

### 同字节65K：最终节点

[五轮完整记录](paired65k-final.json)、[日志](paired65k-final.log)。五轮共同签名输入哈希：
`0xa684e964ff14a8b8814673adcc41349b1855c3613ff1b3cae14e73984f9d78cf`。
同创世、同初始根、同AOEM和CPU业务执行；只改变节点制品或ML-DSA验签后端。

| 顺序 | TPS | P95 秒 | P99 秒 |
| --- | ---: | ---: | ---: |
| 旧 CPU | 2,895.67 | 3.476072 | 3.802047 |
| 最终 CPU | 3,087.06 | 3.295297 | 3.856885 |
| 旧 GPU-required | 3,082.64 | 3.189652 | 3.574481 |
| 最终 GPU-required | 3,014.63 | 3.249603 | 3.667907 |
| 旧 CPU 重复 | 3,148.74 | 3.042412 | 3.247998 |

CPU相对首轮约+6.6%，但旧CPU重复比首轮约+8.7%，且最终CPU的P99略差；
GPU约−2.2%。因此本组不能证明缓冲区复用产生稳定整链收益。
五轮均通过完整费用/余额/账户nonce/授权预算/回执和原库冷恢复核对。

### 持续供给65K：最终节点CPU/GPU/CPU

[三轮完整记录](continuous65k-final.json)、[日志](continuous65k-final.log)。后续签名/准备包含在
不间断计时内，使用合法动态有效期；业务与创世相同，但签名字节可不同，不与同字节组混算。

| 顺序 | TPS | P95 秒 | P99 秒 |
| --- | ---: | ---: | ---: |
| CPU | 2,893.07 | 3.270047 | 3.529137 |
| GPU-required | 3,057.93 | 3.266589 | 3.482363 |
| CPU 重复 | 2,781.34 | 4.034304 | 4.659712 |

八轮全部实际形成64高度、每高度1,024行，8MiB展开预算、8,192在途不变，
终点仍为四节点全部最终回执，诊断关闭。三轮持续供给均完成全部经济和冷恢复检查，
没有transport failure、业务失败或nonce拒绝。P95/P99仍是客户端首发尝试到最慢四方回执观察，
不是节点内部持久确认时间。未重现上一轮极慢样本，不等于已查明或修复其根因。

### 最终制品独立故障验收

| 原测试场景 | 结果 | 日志计时 |
| --- | --- | ---: |
| GPU-required：离线40高度、追赶强杀、到账再消费、冷恢复 | 通过 | 122.34秒 |
| 最低编号验证者晚启动、追赶强杀、原库恢复与少数派拒绝 | 通过 | 152.82秒 |
| 最高编号验证者晚启动、追赶强杀、原库恢复与少数派拒绝 | 通过 | 132.78秒 |
| 真实A/A/B、恢复及重启 | 通过 | 17.31秒 |

证据：[GPU恢复](gpu-recovery-final.json)、[最低编号](recovery-lowest-final.json)、
[最高编号](recovery-highest-final.json)、[A/A/B](aab-final.json)，各原报告无改写压缩保留。
GPU恢复中落后节点启动时间为106.742秒，在已耐久高度1、目标高度40归档尚不存在时强杀；
使用自身原库恢复，保留原配置和签票outbox。没有复制其他节点数据库或给落后节点重新灌交易。

A/A/B原始报告没有node/AOEM哈希字段，不补写原报告。制品绑定依据本次调用记录及运行前后哈希核验：
`NOVOVM_RESIDENT_NODE_BINARY=target/native-io-cost-20261030/final/novovm-node.exe`（解析为绝对路径），
运行`final/native-tests.exe native_only_a_a_b_real_honest_signers_recover_and_restart --nocapture --test-threads=1 --ignored`；
node、runner、AOEM和客户端身份均为本节固定值。其余三个故障报告另自带制品身份。

本次只补验既有最终代码：不重复声明新跑了906/521项或Clippy，上一节这些结果对应同一最终代码；
默认并发/WSS历史问题、Linux/多机、Windows计时精度部署兼容、完整候选CI仍单独待验。
**磁盘阻断已解除且上述补验完成；性能恢复任务未完成。** 当前可确认约3K，未证明稳定提速。
剩余网络成本和恢复投递责任仍以上节定位为准，不能以本轮通过关闭它们，也不直接删除自动Body续送。
最终报告不把12K差距全部归给ML-DSA：旧V3的1,024付款方与本组16付款方仍不是同语义算法对照。
