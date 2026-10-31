# CPU重复轮稳定性：固定制品与五轮观测准备

基线 `203d3b38`。本轮只扩展原测试器和外部观测，**没有修改生产节点、
AOEM或AEAD原型，也没有取得新的65K性能成绩**。任务A尚未完成归因，任务B冻结。

## 已落实的调用与计量边界

- 原 `native_product_load/uca.rs -> run_native_uca_load_mode -> drive` 增加
  `native_asset_same_corpus_five_round_stability`，顺序为CPU/GPU/CPU/GPU/CPU。
  复用原 `PairedCorpus` 的实际密钥、批准材料和已生成签名字节；每轮独立四进程、
  四库与journal，原四方回执、费用、账户状态和冷恢复核验不变。
  原三轮入口继续复用同一实现，不另建负载发生器。
- 原 `uca/rolling.rs` 增加相同顺序的持续供给入口
  `native_asset_continuous_65536_five_round_stability`。保留8192在途上限、
  回执锚定有效期及计时内后续签名；**这组签名字节可能不同**，不能与严格同字节组混称。
- 原CPU首尾读数增加UTC开始/结束标记，只供外部采样对齐。TPS时钟、成功分母和
  GetProcessTimes差值均未替换。静态组签名准备在计时外；持续组保留原计时规则。
- 薄适配已有 `scripts/novovm-native-pipeline-memory-profiler.ps1 -Action series`，
  外部单进程每秒采样指定测试driver与冻结节点路径，内存缓冲、结束落盘。
  记录各进程和线程CPU、线程瞬时状态、working/private bytes、OS peak working set、
  句柄数、采样耗时及观察器自身CPU。输出不覆盖旧证据；退出/读取错误明确记录。

生产节点仍是v4：`6ed28e989a0d11a78a4943e57e437de112bd7261b1ea57b7c87c282b53200a74`。
AOEM仍是 `8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`。
新测试器：`target/native-controller-throughput-20261031/stability-observer/native-tests.exe`，
SHA256 `647a058adb28dbc1b6a134bbaace33f8a942ea6c2fb5b38abb4e69b335025ecb`。
平台/线程环境继续沿用 [candidate-v1.json](candidate-v1.json) 的显式条件，业务仍CPU callbacks。

## 旧慢轮次的补充核算，不是新实验

来源仍为 [203d3b38原始CPU记录](AEAD-CPU-RESULTS.json)。CPU1与CPU2节点顺序相同：

| 进程 | CPU1 µs/成功笔 | CPU2 µs/成功笔 | 增幅 |
| --- | ---: | ---: | ---: |
| node0 | 630.62 | 653.51 | 3.63% |
| node1 | 535.01 | 578.40 | 8.11% |
| node2 | 527.14 | 591.75 | 12.26% |
| node3 | 543.83 | 553.37 | 1.75% |
| driver | 513.55 | 581.98 | 13.32% |

四节点user累计116.109→122.828秒，kernel累计30.469→32.953秒；driver也增加。
说明额外CPU不只集中在一个节点，但**不能据此判断调度、磁盘、分配或任何一个等待就是根因**。
旧轮缺少线程时序和本轮所需阶段对应，现有数据无法追溯补出这些观测。

## 已执行的短负载验证

最终脚本配合冻结v4节点跑通五轮，每轮1024笔、1024个实际付款账户、共享独立收款账户。
五轮输入摘要完全相同：
`f5ca8ba3e750471b6e7fadd0ef3e2b206a1817354baa73fcbda21b129f3ee66f`。
全部成功、完整费用审计与冷恢复通过；原节点与AOEM哈希一致。

- [五轮公开报告](stability-five-short-final.json) / [原始报告](stability-five-short-final.raw.zip)
- [测试输出](stability-short-final.stdout.log) / [运行日志](stability-short-final.stderr.log)
- [最终外部采样](20261101-000521-585-24124-stability-five-short-final-process-series.json)
- [构建](stability-harness-build.log)、[原生回归](stability-native-regression.log)、
  [严格Clippy](stability-clippy.log)：563通过/147忽略；格式与PowerShell语法检查通过。

最终外部采样31次，平均采样22.05ms、最大39.21ms；观察器采样期间CPU1.84375秒，
落盘前OS peak working set约134.3MiB。它不是零开销；计数不含最终JSON序列化和写出。
进程读取无UNAVAILABLE，但按各轮CPU边界与节点PID过滤后，五轮仅4/4/4/2/0条
完全落入drive区间的节点样本。**最后一轮没有有效内窗样本，不能用短测签收CPU稳定性或内存峰值。**
首次脚本短跑和睡眠进程smoke也保留本地，最终结论只引用最终源码对应的上述记录。

## 正式五轮仍未执行：空间门禁

开工D盘约4.76GiB，构建和两组短测后约4.25GiB；不足以保留五轮65K的独立数据库。
没有删除旧库、journal、源码或审计证据，没有擅自换到另一磁盘以改变存储条件。
已请求释放至少25GiB或指定另一个数据盘。**空间不足不能写成测试通过，也不推断14%已经消失。**

空间满足后，仍用上列冻结节点/AOEM/测试器和原环境，先在1024付款账户、
65536笔、1024行/8MiB、8192在途、所有PROFILE关闭条件下跑严格同字节五轮。
外部series的 `TargetPid` 指测试driver，`NodeBinary` 指冻结v4节点；结果必须按
每轮 `process_cpu_accounting.nodes[].pid`、进程启动时间及 `external_sample_alignment`
关联，不能把同路径的无关节点或启动/冷恢复样本算入负载区间。
观察器如先到时间上限或异常，报告明确PARTIAL，不补造未覆盖样本。

随后持续供给五轮单列；观察器开/关及阶段诊断轮也分列，不把带观测的TPS当无扰动峰值。
若需持久化分解，沿同一冻结节点做独立短诊断重放，使用已有PROFILE事件。
`io_persist`是PersistProgress.step墙钟，**不是物理磁盘延迟或完整排队时间**。
线程状态不是等待时长，内存占用不是分配/释放计数；本轮未开启全局WPR Heap，
这些尚未测得的项目继续标缺口，不能用working set曲线替代。

本检查点交付的是可执行的稳定性回放和观测接线，不是吞吐优化。
AEAD仍只在隔离测试；未推进main、正式库、部署、加密语义、补片、候选或账户规则。
