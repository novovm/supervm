# 原生投递链修复与整链吞吐复验

本轮基于 `36049108aff87d28dffe6836d80f37042b0033e6`，沿
`dev/mldsa65-identity-closure` 集中修正原发送游标、初发与补片的公平推进及原接收统计。
**已复现的投递缺陷得到修复，65K 整链没有稳定提速，历史约12K的性能恢复任务仍未完成。**
最终候选只保留在开发分支，不升级 main、正式 AOEM 随包库或 D 的部署包。

## 修改了哪些实际等待

调用链保持原 `native_execution_tick → NativeChannel → NativeObjectLink → NativeObjectSender`
及原 UDP、认证、重组、准入、执行与 journal。没有新增线程、队列、账户、运行时或缓存系统。

| 原缺陷 | 本轮修复 | 旧行为反例 |
| --- | --- | --- |
| ACK 缺片集合变化时重建游标，已经成功补出的片又从头发送 | 同一补片轮次保存实际成功写入的进度；仍由原重试期限重新打开未确认缺口 | 已补0–7，ACK仅确认0，旧游标回到1而非8 |
| 只裁剪末窗口的未确认尾部，中间窗口会把尚在初发途中的尾部提前补发 | 使用原全对象缺片计数和窗口范围识别未确认尾部；保留已证明的内部缺口 | 128片中仅收到0–31时，旧实现立即补发32起的片 |
| 已收到真实缺片反馈，仍必须等整份大正文初发结束才补片 | 两个原有游标在原发送配额内交替；只在实际写入后交替，WouldBlock不推进 | 原 UDP／重组反例丢1、3、5片，初发未结束时旧实现补片数为0 |
| 早补片再次丢失，其原等待被无关初发及ACK进度不断推后 | 保留该补片轮次最后一次实际写出的原 `ack_wait`；有证据的缺口可以在初发完成前重试 | 持续初发与无关ACK到来时，旧实现不重试丢失的早补片 |
| 固定16条接收样本已满，仍逐包构造随后丢弃的字符串，并重算不变的百分位数 | 原采样容器满后不构造值；只有数值样本增长时刷新百分位数 | 新反例验证满样本后factory不调用，但计数、错误分类、最大间隔继续更新 |

证据：[旧游标／尾片失败](old-counterexamples.log)、[初发优先旧失败](initial-priority-old-fail.log)、
[早补片旧失败](early-retry-old-fail.log)、[最终29项发送测试](sender-candidate3-final.log)。
部分写入、pending句柄归还、generation、副本数变化、backoff、纯尾片及新旧流量公平性均在反例中覆盖。
重试时间参数、TTL、资源配额、共识门槛和协议容量未改变；不把未证实的尾片当成早期丢失。

三份内部候选依次加入上述修复，并非三条生产路径。最终候选包含全部关联修复；
没有把某次正向样本或反例通过当成吞吐验收。已有 socket／prefill 负向实验未重新启用。

## 时间具体省在哪里

沿原默认关闭的有界诊断补齐实际 ACK 写出／收到、缺口采样桶、补片首写与首次重试。
仍只跟踪 NVINPUT1，原256对象／120秒、8192条事件环不变；每对象每方向ACK窗口样本最多256条。
发送计数只在 carrier 实际写入成功后增长。接收按同 peer、session、scope、fragment 对齐。

16384笔诊断的三份内部候选，负载类型相同但输入不完全相同，以下是机制观察，**不是公平 TPS A/B**：

| 指标 | 候选1 | 候选2 | 最终候选3 |
| --- | ---: | ---: | ---: |
| 首ACK到首次补片 P50 | 284.024 ms | 5.932 ms | 见逐对象记录 |
| 首ACK到首次补片 P95 | 754.016 ms | 41.290 ms | 见逐对象记录 |
| 首个缺口采样桶停留 P95 | — | 437.970 ms | 57.920 ms |
| 初发结束后才首次推进该桶 | — | 14/47对象 | 0/48对象 |
| 初发实际写入跨度 P50 | — | 275.303 ms | **432.250 ms** |
| 初发实际写入跨度 P95 | — | 604.108 ms | **1044.759 ms** |
| 初发结束到 DONE P95 | — | 1087.181 ms | 580.601 ms |
| 接收首片到收齐 P95 | — | 1590.905 ms | 1401.396 ms |
| Repair 写入数／Initial 写入数 | 47.93% | 53.09% | 49.93% |

首缺口按64片采样桶观察，不是逐片闭合时间；Repair比例也不是物理丢包率。
缺口等待和初发之后的等待缩短，但初发本身变长。不能把省下的局部时间直接相加成整链收益。
最终48个保留发送对象均仅观察到1次send attempt并完成，未出现记录遗漏或淘汰；
47个对象观察到实际补片重试。接收重组代次未独立观测，不能据此断言接收过程没有重置。

最终最慢对象 node0→node3、scope30、fragment `99453ba0…e2706a4`：

| 边界 | 时间 |
| --- | ---: |
| 发送准入到首次初发 | 13.651 ms |
| 首次初发到首ACK | 0.462 ms |
| 首次初发到首次补片 | 75.759 ms，index96 |
| 首次初发到首次补片重试 | 100.648 ms，index112 |
| 首次初发到初发全部写完 | 1024.903 ms |
| 首次初发到 DONE | 1730.954 ms |
| 接收首片到收齐 | 1719.817 ms |
| 最后片到哈希验证完成 | 0.283 ms |
| 验证完成到交接 | 0.004 ms |
| 交接后解码 | 2.691 ms |

该对象有4477次Initial、4148次Repair、53轮实际补片重试。768字节净chunk承载约3.44MB APFL正文，
不是5.58MB展开正文。反馈持续存在；剩余秒级等待仍位于分片完成之前，不是末尾哈希或解码。
旧约2.35秒对象与本对象并非相同输入，不能以二者差值宣布净提速。
详见[候选1](analysis-candidate1.json)、[候选2](analysis-candidate2.json)、[候选3](analysis-candidate3.json)
以及对应 `trace-*.raw.zip` 中未经改写的完整公开报告。

## 最终65K同字节对照

Windows同机四个真实原节点进程、独立数据库。最大1024行／块、8MiB展开预算、8192笔持续在途，
65536笔全部合法成功，四节点所有最终回执返回为终点。业务后端始终 `cpu_callbacks`；
只改变ML-DSA验签CPU／GPU-required，不更换算法、经济规则、法定票数或业务后端。
GPU-required无CPU回退。正式性能轮次关闭阶段、投递和资产trace诊断。

同一份预生成签名字节、同创世、同初始根，由同一最终测试程序依次运行旧／新节点：

| 轮次 | 成功TPS | P50秒 | P95秒 | P99秒 |
| --- | ---: | ---: | ---: | ---: |
| 旧CPU | 2505.80 | 3.162 | 4.267 | 4.381 |
| 最终候选CPU | 2495.62 | 2.961 | 4.558 | **5.181** |
| 旧GPU-required | 2680.27 | 2.776 | 4.820 | 5.152 |
| 最终候选GPU-required | 2661.86 | 2.873 | 4.605 | **5.322** |
| 旧CPU重复 | 2234.26 | 3.658 | 4.934 | 5.243 |

CPU约−0.41%，GPU约−0.69%；CPU P99约上升18.27%。旧程序重复也波动，不能签稳定改善，
也未定量证明每项修复分别造成多少回退。全部64个应用批次完成，费用、预算、nonce、余额、回执和冷恢复通过。
签名输入SHA-256为 `281dc0da1976ba217d030a7164eae3a9738fdd0b8fe70f02ee0c2493cf951124`。
原始成绩和哈希见[同字节对照](same-corpus-candidate3-65536.json)。

## 持续供给与负向样本

后续签名生成、结构准备、排队及背压进入计时；随回执推进合法有效期，不能与预生成输入口径混用。
P95/P99均为首次提交到四方最慢回执查询观察，不是节点内部耐久发布延迟。

| 最终候选持续供给 | 成功TPS | P95秒 | P99秒 |
| --- | ---: | ---: | ---: |
| CPU | 2023.45 | 5.098 | 5.186 |
| GPU-required | **1372.33** | **17.047** | **18.786** |
| CPU重复 | 2338.55 | 5.576 | 6.085 |

为保留波动边界，随后不改代码、不改资源参数再测：旧GPU 2875.82 TPS、P95/P99 3.583/4.171秒；
最终候选GPU 3016.61 TPS、3.261/3.399秒。后两轮持续生成的签名输入hash不同，
属于同业务条件重复，不是严格同字节因果对照。不能用3017覆盖1372或声称GPU已稳定领先。
旧GPU补测期间发生一次约4秒只读观测文件解析，故该轮也不作为严谨收益结论。

见[三轮持续供给](continuous-candidate3-65536.json)、[旧GPU补测](continuous-baseline-gpu-repeat-65536.json)、
[候选GPU补测](continuous-candidate3-gpu-repeat-65536.json)。全部成功并通过冷恢复与完整费用核对。

内部候选的负向结果一并保留：候选1持续CPU/GPU/CPU为2385.52/2741.54/2545.37 TPS；
候选2同字节旧CPU/新CPU/旧GPU/新GPU/旧CPU重复为2616.85/2638.04/2893.88/2811.60/2610.93 TPS。
详见本目录 `continuous-candidate1-*` 和 `same-corpus-candidate2-*`，不跨轮挑最好数字。

## 剩余限制与已经排除的解释

1. **实现缺陷已复现**：ACK更新倒退、未确认尾部早补、初发压住缺口、无关进度推后早补片重试。
   测试证明这些控制流修正，不等于它们解释了全部12K差距。
2. **剩余成本定位**：4477片初发仍有约1秒跨度，之后仍需数百毫秒补齐；大量Repair与Initial共享原发送配额。
   原endpoint每轮全局Data/Repair写配额64，ACK另有反馈机会；不能误读成每peer64，
   也不能由此直接认定扩配额就能恢复TPS。
3. **接收链未发现应用FIFO主动丢弃**：队列满时停止继续读，成功读取包进入原FIFO。
   但这不排除OS/socket丢包、重组新对象背压或会话退休过滤；现有拒绝计数为累计分类，非完整丢包统计。
4. **不是逐片ML-DSA**：建立会话后逐包执行ChaCha20-Poly1305及片校验，没有逐片签名、公钥解析或HKDF。
   源码确认发送片checksum在创建及编码各算一次，多层payload复制与分配仍存在，
   但未取得CPU采样，不能量化这些成本或当成主要根因后直接改安全编码合同。
5. **主循环归因缺口仍在**：实际压测走 `native_execution_tick`，不是带Idle/Yield计时的另一处 `rpc::run`。
   IO间隔外的时间尚未分开JSON观察写入、park/yield、空轮询和OS抢占，零Idle计数不能证明没有睡眠。
   目前未证明 `ack_progress` 未进入hint单独造成错误休眠，因此没有加入猜测性调度补丁。
6. **系统采样不可用**：xperf初始化返回0x3ec；按已安装WPR帮助改用CPU profile后被Windows策略拒绝，
   错误0xc5585011。没有启动采样负载、修改策略或提升权限；WPR最终未录制。
   见[采样阻断](cpu-sampling-blocked.log)。这不证明CPU慢，更不证明硬件已达上限。

当前能重现“输入分片收齐慢”，但1372 TPS慢轮次的首个异常事件、物理丢失与CPU/OS调度占比仍未查明。
不是完整根因结案，不再围绕GPU算子、UCA寻址或继续放大区块作无依据修改。

## 最终候选验证

| 检查 | 本机结果 |
| --- | --- |
| sender定向 | 29通过，包含旧失败反例及部分写入／重试边界 |
| network相关完整回归 | 903通过、0失败、11忽略 |
| 原生流水线回归 | 521通过、0失败、123忽略 |
| GPU-required晚启动／追赶中强杀／原库恢复／到账再消费 | 通过；中断高度1、目标40，最后四节点高度41、状态版本1064一致 |
| 真实诚实签票A/A/B与重启 | 通过；不修改法定门槛，不外造QC |
| node/network release tests严格Clippy | 通过，`--locked -- -D warnings` |
| 全仓format；源码、文档和采集脚本diff空白检查 | 通过；原始测试日志的末尾空行原样保留 |

对应 `sender-candidate3-final.log`、`network-candidate3-final.log`、`node-native-candidate3.log`、
[GPU恢复公开报告](process-recovery-candidate3-gpu.json)、[A/A/B原始报告](process-aab-candidate3.raw.zip)、
`clippy-candidate3.log`。网络本轮WSS背压测试通过，不据此关闭此前偶发失败。
旧编译错误（本轮缺少enum import）已修复，原失败日志保留在 `build-tests.log`，不冒充历史债。
上述忽略项并未全运行；整仓发布、Linux、多机和完整CI均未签收。

## 固定制品与复现

| 制品 | SHA-256 |
| --- | --- |
| 最终node | `18d5f1376d1b7459e4e674402fcb2609cf45a4db0c81b4b201144d3ceaccb157` |
| 最终client | `5f379a994e0f65b94cecccded432f2d0da617c6928db033aaad9463c24d9ed9d` |
| 最终tests | `7e38056eaca8bbec32308dd5ac896bb617aaf03609dce87bdeb42d2fdd0072de` |
| 原36049108 node | `f11a829af5b6b845c77f25b3c771269c7a89b10cd61ee1043551078ce176d3f5` |
| 本轮未改AOEM DLL | `8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671` |
| 完整最终运行源码patch | `dd3aab8c7437b9c49aebd049c8d19993158590fdd8cd544a37fefaf5b38c92ca` |
| patch原样压缩包 | `10aecbe609c878240052b5728293e4199e3862efe21810f02b025e0227bcc3f3` |

源码身份为父提交加 [candidate3-source.patch.zip](candidate3-source.patch.zip) 内的原始patch，
包含新增反例文件；表中为解压后patch的SHA，压缩保存避免checkout改写行尾。
构建后没有再修改运行源码。最终EXE与PDB在 `target/native-delivery-closure-20261030/candidate3/`，
AOEM候选在相邻AOEM仓库 `artifacts/mldsa-native-paired-20261027/`，未覆盖SUPERVM随包库。
原测试入口及环境使用 [已有runner](../native-throughput-recovery-20261029/run.ps1)：

```powershell
$env:NOVOVM_PROFILE_ASSET_TRACE = '0'
& artifacts/audit/native-throughput-recovery-20261029/run.ps1 `
  -Filter native_asset_continuous_65536_signature_cpu_gpu_cpu `
  -Log <new-log-path> `
  -Node target/native-delivery-closure-20261030/candidate3/novovm-node.exe `
  -Tests target/native-delivery-closure-20261030/candidate3/native-tests.exe `
  -SignatureBackend cpu -Transactions 65536 -FrameRows 1024
```

严格同字节对照改用filter `native_uca_same_corpus_bulk_authority_before_after`，并将
`NOVOVM_NATIVE_UCA_BASELINE_NODE_BINARY` 指向原36049108冻结node。
完整恢复filter为 `native_uca_process_late_validator_interrupted_catchup_respend_and_cold_recovery`，
显式 `-SignatureBackend gpu_required`。每轮使用原脚本新建专属运行目录，旧库及签票journal不清理。

`collect.ps1` 仅汇集公开report并原样压缩，记录源报告／压缩包SHA；不收集私钥、配置或数据库。
本轮用户释放磁盘后继续验收，代理未删除任何文件。仍保留增量编译；磁盘清理不作为TPS归因。
此检查点用于保存已验证修复、全部负向样本和未解决问题，**不是性能完成或部署授权**。
