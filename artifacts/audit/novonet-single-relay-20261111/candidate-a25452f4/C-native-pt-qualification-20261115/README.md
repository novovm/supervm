# C：原生 WPR/PT 实采、转储关联与解码资格验证

本轮已经实际启动并停止原生 WPR，取得真实 PT 数据及同次测试异常 Full Dump。
**原始 NOVOVM 首次 RBP 错位仍未定位。故障窗口覆盖尚不合格，未启动 a254/AOEM966 长跑。**
原节点、原库和身份未动；双中继保持暂停，没有生产修复或重新启用旧 CDB 高频监控。

## 实际运行和停止

首次 UAC 被取消，未执行；用户要求再次发起后，第二次 UAC 成功。
只执行一个15秒基线进程和一个约15秒 PT 资格进程，均为本轮新建独立程序。
程序以固定分支循环验证 PT 控制流与异常转储关联，不是原始 NOVOVM 复现，也没有人工修改 RBP。
测量结束后，资格进程显式触发自定义 `0xE0424354`，由专用线程写出 Full Dump，再以该码退出。
这是声明的资格异常，不能作为自然访问违规或原始 c0000409/c0000005 证据。

时间（UTC）：`2026-11-14T20:22:20.809Z` 至 `20:23:10.035Z`；北京时间为11月15日04:22至04:23。
独立会话为 `C_PT_20261114T202220`。start/status/stop都指定该实例；没有全局cancel、
CDB附加、DebugBreakProcess、NtContinue守卫、驱动安装或WER/ACL修改。

| 执行项 | 实际退出码 | 结果 |
| --- | ---: | --- |
| 基线进程 PID9908 | 0 | 15批次完成 |
| WPR start | 0 | 真实开启 |
| WPR active/counters | 0 / 0 | 原生PT配置处于活动状态 |
| 采集目标 PID28712 | -532528300 / 0xE0424354 | 预先声明的资格异常后退出 |
| WPR stop | 0 | 保存20,709,376字节ETL |
| WPR after | 0 | `WPR is not recording` |

所有目标与WPR退出码来自父进程保留的Process对象。PowerShell提权助手自身OS退出码未另存，
不以其消失代替退出码0。原始控制结果见 [controller-result.json](controller-result.json)。
其中`actualPtDecoded=false/qualificationPassed=false`是控制器结束时的原值，后续离线分析独立记录，
没有覆盖该原件。采集结束后无本轮资格进程遗留；没有启动NOVOVM。

## 同次异常上下文：转储侧核验通过

新 DMP 大小25,508,367字节，SHA256
`11bee06c8a4c08ae57c819148a89a06660e9c1ba56eaa07e6a53c157758582fd`。
flags=`0x221822`包含FullMemory、ThreadInfo和FullMemoryInfo，157段Memory64、6线程上下文。
PID28712、异常TID42560与测试输出一致；ExceptionStream记录如下：

- 异常码 `0xE0424354`，故障QPC `2982258850210`，频率10,000,000Hz。
- RIP `0x7ffce03b569a`，RSP `0x905a4ffd80`，RBP `0`，与异常过滤器保存的原CONTEXT逐项一致。
- RIP属于KERNELBASE，离线CDB调用链为 `RaiseException → PtQualification!Work`。
- RIP代码页、RSP内存页在同份Full Dump中；模块映射包括资格EXE基址 `0x7ff616bb0000`。

RBP=0不构成违约。线程列表是写转储时刻的上下文，未拿它代替ExceptionStream原异常上下文。
离线CDB首轮`r rip rsp rbp`语法错误，原输出保留；随后用三个独立`r`命令确认同值，未重跑目标。
分析代码与结构结果见 [analyze_qualification.py](analyze_qualification.py)、
[qualification-analysis-public.json](qualification-analysis-public.json)。完整DMP仅在本机。

## 短时干扰粗测

| 指标 | PT资格进程相对基线 |
| --- | ---: |
| 总工作吞吐 | -3.8878% |
| 批次中位吞吐 | -3.8471% |
| 线程CPU时间/iteration | +3.8250% |
| cycles/iteration | +3.2572% |

两个进程各完成约15秒测量，没有出现旧方案120秒无报告的停顿。但这是**一对顺序执行的资格微负载**，
不是原NOVONET状态查询；只测线程CPU，没有系统总CPU开销。温度、调度、执行顺序均可能影响差值。
采集进程还多了一个等待中的转储线程，因此不能将全部差值严格归因于PT这一单变量。
15批次不能代表可靠P95/P99，PT覆盖缺口也使其不能作为完整捕获成本的证明。
**没有据此宣布原节点低干扰资格通过。**

## 实际 PT 数据和覆盖缺口

官方WPT的xperf已实际执行tracestats、process、dumper，全部退出0；系统tracerpt导出真实事件。
ETL包含1102条IPT事件候选，头部按`<QIIQII>`解析后，32字节头加TraceSize与payload长度全部吻合。
头部候选来自[原始Windows IPT研究记录](https://gary-nebbett.blogspot.com/2022/07/intel-processor-trace-ipt-under-windows.html)，
本轮另外以实际长度、PID/TID、QPC及官方包解码交叉核验，不仅依赖该布局假设。
目标PID28712/TID42560保留7条，均在CPU1。根据TracePosition重排环形片段后，
Intel官方libipt的ptdump逐片解析，全部退出0、没有报告包解码错误，包含真实PSB/TNT/TIP等包。
这不是仅靠事件名称判断PT存在，也不是将普通ETL事件统计冒充指令轨迹。

关键时间：目标工作开始QPC=`2982108845092`，已保存目标PT事件QPC范围
`2982181363516..2982181423496`，即工作开始后约7.2518424至7.2578404秒。
最后一条目标PT采样距测试异常仍有 **7.7426714秒**。
事件时间是片段采样时点，不是片段内每条指令的独立时间戳。

WPR和ETL报告的EventsLost/LogBuffersLost为0，**不能解释为完整历史**。
当前是系统级Memory模式、请求64×64KiB ETW缓冲以及每片4KiB PT缓存；循环覆盖和采样间隔
必须另行检查，不能因计数0就排除缺口。当前证据不能将缺失唯一归因为环形覆盖、CPU迁移或其他原因。
目标进程/模块启动元数据不完整，xperf的Unknown行也必须与原EventData.ThreadId核对，不能直接当零样本。

原始SampledProfile另外保留6条目标TID记录，QPC/IP与xperf同条记录一致；xperf显示Unknown/-1，
而保留的目标生命周期资料不完整，不能将前者写成目标没有采样，也不唯一归因其解析结果。
CSwitch保留区间到工作开始后8.0921075秒，
也没有覆盖异常前最后6.9084043秒；详见[COVERAGE.md](COVERAGE.md)和[coverage-public.json](coverage-public.json)。
## 已保留片段的指令解码：通过，不代表故障窗口完整

官方Intel `libipt/ptxed`加XED，使用同次Full Dump中40个可执行区段，已对7个目标片段实际解码。
7次进程退出码均为0，检查解码器诊断后也没有剩余错误。实际路径包含
`ExecuteChunk → BranchEven/BranchOdd → return`，并保留真实条件分支、调用及返回指令片段。
目标EXE的402,480字节`.text`与同次转储代码逐字节一致，不是用另一构建替代。
见[decoder-final-public.json](decoder-final-public.json)，内含工具固定提交、SHA256和14行实际指令。

首次只映射资格EXE时，4段虽退出0却报告系统代码地址未映射；该失败保留。
补入**同次**Full Dump捕获的系统模块代码后才解除错误，没有混入其他运行现场。
XED的编码警告、构建输出解码和ptxed包含目录错误也留存，最终仅修正用户态构建环境/路径，
没有修改Intel源码或生产节点。微软工具仅独立提取，未安装系统组件。

868,052条是保留片段的指令输出条数，不是跨片段去重后的全程指令数量。
这些片段内没有OVF/TSC/CYC包，也不能据此声称无覆盖或提供每条指令的精确时间。
ETL头QPC、原始payload PID/TID、同次进程身份和Full Dump代码可以关联**已保留的片段**；
ETL自身未保留完整的进程/线程/模块生命周期旁带，不冒充独立完备的历史。
PT不包含逐指令RBP数值，最后7.7426714秒没有目标PT的区间仍包含工作结束与RaiseException路径。
因此未能证明异常前最后正常指令，更没有NOVOVM首次RBP错位证据。

| 资格项 | 本轮结论 |
| --- | --- |
| 原生WPR实际采集、独立实例安全停止 | 通过 |
| 已保留目标PT包及指令解码 | 通过，仅7个片段 |
| 同次PID/TID、模块代码、异常上下文关联 | 已保留片段可核对；ETL生命周期旁带不完整 |
| 目标故障前窗口覆盖 | **未通过：缺少最后7.7426714秒** |
| 原NOVONET低干扰和66分钟条件 | 未执行，不能用资格微负载替代 |
| 首次RBP错位与生产修复 | 未取得 / 未进行 |

`tools-public.json`中的`etlProcessed=false`是工具准备时保留的原值，不是离线解码终态。

## 保护和后续门槛

私有原件在 `target/c-pt-live-qualification-20261115/run-01`。系统级ETL包含其他进程信息，
完整ETL、XML/CSV导出、DMP、原始内存、全部机器轨迹均不上传；公开只保留目标白名单统计及诊断源码。
原a254、AOEM966、原库、身份及历史失败现场未被修改或启动，未恢复双中继。

本轮没有通过故障前覆盖门槛，所以不自动开始66分钟负载，不根据资格程序提交生产修复。
下一次方案必须先解决可观测区间与进程/模块旁带资料的保留问题，并重新检查实际缺口，
不能只扩大运行时间或将PT包解码成功当作首次RBP写入证据。C独立负责，Git同步不等待A签收。

工具依据：[WPR命名实例](https://devblogs.microsoft.com/performance-diagnostics/controlling-the-event-session-name-with-the-instance-name/)、
[WPR控制命令](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wpr-command-line-options)、
[Memory模式](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/logging-mode)、
[Intel libipt](https://github.com/intel/libipt)、
[MiniDump异常上下文](https://learn.microsoft.com/en-us/windows/win32/api/minidumpapiset/nf-minidumpapiset-minidumpwritedump)。
