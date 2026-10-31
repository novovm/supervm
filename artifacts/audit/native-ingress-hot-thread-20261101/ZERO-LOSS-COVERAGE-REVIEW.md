# 第二次ETW复核：零丢失，但五窗均错过交易

本次复核用户提供的 `target/native-ingress-hot-thread-20261101/20261101-054438-36312`。
结论是 **REJECTED_INSUFFICIENT_LOAD_OVERLAP**，不是函数级归因完成。
五份ETL均零丢事件、零丢buffer，但全部在对应原始交易drive结束后才开始。

## 实际边界

下表毫秒差取ETL header实际UTC与原report `after_read_started_unix_ms`；
原report CPU计量包络已经宽于交易主计时，仍无交集。

| 轮次 | 后端 | 录制 | ETL晚于drive结束 | 有效交集 |
| --- | --- | --- | ---: | ---: |
| 0 | CPU | CPU栈 | 1,482 ms | 0 s |
| 1 | GPU-required | CPU栈 | 1,028 ms | 0 s |
| 2 | CPU | 调度 | 1,343 ms | 0 s |
| 3 | GPU-required | 调度 | 1,308 ms | 0 s |
| 4 | CPU | CPU栈 | 1,181 ms | 0 s |

详细ETL哈希、字节数、PID、输入身份及边界见
[ZERO-LOSS-COVERAGE-RESULTS.json](ZERO-LOSS-COVERAGE-RESULTS.json)。
五轮入口均为node0；不能沿用上一捕获的node1或旧PID/TID。
即使后续冷恢复进程也叫novovm-node，也不能算作原交易处理线程。

五轮65536笔同签名字节、四方回执、费用及冷恢复通过，原冻结node、tester和AOEM未变。
原工作负载摘录见[zero-loss-post-load-workload.json](zero-loss-post-load-workload.json)，
这只是诊断伴随负载，不替换既有容量验收结果。

## 原因及修正

旧采集器等四份stdout的 `Length > 0`，然后按 `LastWriteTimeUtc + 8秒`请求录制。
原 `ProductNodes::start` 在spawn前就创建这些文件；文件内容可见性/修改时间不是
交易就绪边界。本次五个实际窗口证明这个触发条件失效，责任在采集器，不在节点。

修正仅在原capture.ps1：按文件存在和CreationTimeUtc触发；不改测试器或节点。
负载结束后读取ETL实际UTC，自动检查与本轮drive边界的交集；CPU栈至少4秒、
调度至少3秒，缺失或不足都报失败。使用同一实现回放本次五个零丢失坏窗口，
全部被拒绝，见[反例结果](zero-loss-outside-load-tests.json)。

另用原冻结负载真实运行CPU/GPU/CPU三轮各65536笔，**没有录ETW**。
同一触发函数取得的观察窗分别8.017、8.004、8.004秒，均完整落在drive中；
交易、经济核对和冷恢复均通过。见[触发证据](trigger-live-validation.json)及
[工作负载摘录](trigger-validation-workload.json)。这证明新触发在本次负载有效，
不是v3实际ETW通过，也不外推成所有运行环境保证。

该验证中早先枚举持有的FileInfo长度为0，而后续Get-Item和文件句柄读得133字节。
这只是缓存对象与后续读取的差异，**未证明具体NTFS或系统元数据机制**。

## 仍待完成

当前代理进程实测仍被WPR权限拒绝（0xc5585011），没有自动提权或改系统策略。
需要再次在管理员PowerShell运行README中的同一命令；v3自动保存与校验实际覆盖。
通过后才对原入口的 `novovm-native-tick` 线程做函数独占/包含样本分析和调度分析。
调度profile无等待栈，不能据此认定某把锁；SHA等叶函数也不能替代调用链归因。

所有原始system ETL、PDB及含其他进程信息的导出继续仅留本机target；
未清库、删journal、改main、替换正式库、部署或运行完整CI。
