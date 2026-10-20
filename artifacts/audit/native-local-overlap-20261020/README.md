# 原生候选提前执行与首批成本（2026-10-20）

基线：SUPERVM `b1a3876f9f89a6deac977ded9868d9632041be37`；候选为同基线加本提交的两个生产文件改动。
AOEM 未修改；正式 SDK 未替换。冻结二进制、库、驱动及测试程序 SHA256 见 [measurements.json](measurements.json)。

## 实际入口与替换关系

原节点 tick → NovoRUDP/APFL → `RpcLifecycle::poll` 本地严格鉴权池选择
→ `NativeLifecycle::offer` → `CandidateBodies` → 原 `CandidatePipeline`
→ node/exec/bindings/AOEM → 原 RoundProgress/journal 发布与查询。

此前只有 leader 可以选择当前高度并开始执行；follower 需等 leader 完成执行、
持久提案、收到提案后请求精确正文，再独立执行。本片复用同一个有界本地正文槽和常驻执行 owner，
让已独立鉴权的 follower 输入提前准备。每个高度/轮次最多一份本地选择，精确父绑定不变；
已提升的后继不重复准备，原下一 leader 的后继准备机会保留。

匹配正文才复用结果；异正文仍按原需求获取与独立执行。准备结果留在正文 owner，
直到合法提案需求出现才进入 follower 共识候选表。只有合法 leader 能提案，签票仍要求完整
执行资格、原锁/QC、耐久 ACK；准备完成不签票、不发布。不增模式、控制器、队列或活动签者。
已验证正文需求优先于尚未提交的本地推测；不会抢占已经运行的任务，不能据此宣称任意满额配置都有硬资源预留。
原两处 leader-only 执行门已替换，没有保留旧活动开关或第二执行路径。CPU/GPU 仍由同一语义契约选择。

## 首批 GPU 的实际成本

正式库的 `IntegerLane::start` 在 ready 前通过 `ResidentTransferResources::new/session.open`
创建八个程序及 GPU pipeline。AOEM 的 `ai_checked_integer_composition.rs`、
`ai_checked_integer_resources.rs` 和 `apfl_checked_integer_program.rs`
在核对的 `31b14d6d..97070d17` 间无差异。正式实现的首次 exchange 延迟创建上传/回读缓冲；
先前撤回候选的首次 resident-composition 创建不能混称正式库行为。

复用现有完整计划测试，独立进程资源创建 **983,284 µs**、首次完整执行 **43,363 µs**，
后续16次 **28,993–32,053 µs**。初始化包含加载/会话/程序资源建立，首次执行包含门面与内部工作，
并非纯 shader 时间。此片没有预热正式负载，也没有改变 GPU 程序、整数宽度或 AOEM 调度。
这是一个当前库样本，不能补回历史9.913691秒异常的缺失时间线；历史原因仍未完全关闭。
原始输出见 [初始化与执行](resident_complete_plan_contention_cost.log)。

## 同条件真实交易

Windows、外接电源 BatteryStatus=2、同机四个原节点进程和四库、单原生 SDK/APFL入口；
每轮65,536笔真实签名转账、1,024付款方×64nonce、共享收款方、金额1、
每批1,024笔、在途8,192、查询批128、正式关闭阶段诊断。使用同一冻结驱动与 AOEM库；
没有其他构建或负载竞争。

计时从首次 native SDK 提交尝试直到四节点全部最终回执返回，包含首批与首高度等待。
签名生成、节点启动/资源建立、APFL准备、事后经济核对及冷重启在原计时窗外；
资源建立另在上节实测，不能把正式 TPS 称为包含启动的吞吐。P95/P99包含提交与查询观察调度。
以下聚合 TPS=两轮总成功数/两轮总时间；聚合分位数对两轮131,072个原始延迟样本重新排序，
不是平均两个分位数。

| 后端 | 成功 TPS 旧→新 | P95 秒 旧→新 | P99 秒 旧→新 |
| --- | --- | --- | --- |
| CPU | 7,847.13 → 12,324.09（+57.05%） | 1.063182 → 0.750089 | 1.170024 → 0.766937 |
| GPU 整数完整计划 | 3,962.27 → 6,546.13（+65.21%） | 2.160184 → 1.275597 | 2.193971 → 1.358836 |

| 轮次（实际执行顺序） | 成功 TPS | P95 秒 | P99 秒 | 首高度四节点观察 秒 |
| --- | --- | --- | --- | --- |
| baseline/cpu/1 | 7772.18 | 1.072374 | 1.191564 | 0.263040 |
| candidate/cpu/1 | 12479.38 | 0.668034 | 0.749355 | 0.188176 |
| baseline/integer/1 | 3969.47 | 2.164646 | 2.190838 | 0.324676 |
| candidate/integer/1 | 6541.89 | 1.275175 | 1.355321 | 0.244446 |
| candidate/integer/2 | 6550.39 | 1.277994 | 1.371825 | 0.285371 |
| baseline/integer/2 | 3955.10 | 2.150665 | 2.195899 | 0.344401 |
| candidate/cpu/2 | 12172.63 | 0.758637 | 0.847916 | 0.192726 |
| baseline/cpu/2 | 7923.55 | 1.053602 | 1.163785 | 0.245193 |

每轮65,536成功、0业务失败、回执费用2,949,120；八轮的余额/回执费用守恒、nonce序列和冷恢复全部通过。
所有32个结束状态均为高度64、state_version=65,536、APFL执行65,536行、拒绝输入0；
整数四轮的每节点 integer_rows=65,536，CPU为0。所有轮次最终状态根相同。
完整费用状态审计和业务证明仍分别为 false；本片不签收百万级、GPU超过CPU、完整证明或真实多机性能。

## 验证与复现

- [原生生命周期日志](native_lifecycle_.log)：6项通过（CPU/代数 follower 准备、异正文反例、三高度、两项截止顺序）。
  无 leader 输入时三个 follower 已真实执行，仍无出站/链头；合法 leader 原正文到来后，
  两个匹配 follower 执行一次且不请求正文，异正文 follower 请求精确正文并执行第二次，最终同链头。
  该功能阶段固定协议 Instant 以隔离超时，仍有墙钟 deadline；正式负载没有固定或重置协议时间。
- [A/A/B日志](native_progress_aab_split_recovers_without_external_qc.log)：既有同进程四库夹具1项通过，
  不冒充四进程故障验收。
- [普通共识日志](ordinary-consensus.log)：45项通过、28项忽略。
- release node、release tests 构建通过；四个修改源文件 rustfmt check、范围内 diff check 通过。
- 原始 report.json、observations.json、节点日志和数据库留在 measurements.json 各 path，
  未删除证据、签票日志、runtime数据或49份受保护草稿。

本地复现脚本位于 `target/native-local-overlap-20261019/{check,run,summarize}.ps1`，
正式负载复用 `original_native_sdk_65536_final_receipts_and_cold_economics`。
测试和提交使用同一个生产候选二进制；源码改动只有本提交，无 AOEM/DLL/SDK替换。
本组改善来自同路径提前执行接线，不能外推为任意冲突分布或主网容量。
