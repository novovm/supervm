# C：PT 保留缺口的实际对照与剩余边界

本轮已实际完成一次 File 模式对照，复用上一轮原资格 EXE，没有重新编译目标。
**7.742秒的长尾保留缺口已缩小到351.3微秒，线程和模块旁带已补齐；真正异常路径仍未捕获。**
原 NOVOVM 首次 RBP 错位未定位，没有生产修复，也未启动 a254＋AOEM966 持续负载。

## 先核验旧证据

上一轮 Memory ETL 的两条 `LastDroppedTimes` 相同；32个CPU槽中29个非零，
29个值全部等于对应CPU首条保留PT的内部QPC，其中15个保留起点晚于测试异常。
这直接证明 `Lost Events/Buffers=0` 不能代表完整保留。
重筛全部1102条PT/32个CPU，目标PID/TID仍只有7条、全部CPU1；不是xperf的Unknown/-1
把目标错关联到另一CPU造成的漏查。旧文件没有足够迁移历史，不能进一步证明每条遗漏
只由某一种覆盖算法造成。见 [OLD-RETENTION.md](OLD-RETENTION.md)。

## 实际 File 对照

执行时间：UTC 2026-11-14 20:51:11.355至20:52:02.136，
即北京时间2026-11-15 04:51:11至04:52:02。
实例 `C_PT_FILE_20261114T205111`，目标PID39352、工作/异常TID45712。

沿用4KiB PT、64×64KiB ETW缓冲和SampledProfile触发，只把外层保留改为File模式。
另有一项明确的对照修正：本轮基线和采集组均使用相同 `--fault` 模式，
两者都有等待中的转储线程；不再沿用上一轮normal/fault两组不同的线程数。
每组工作15秒，随后显式触发资格异常0xE0424354，**不是NOVOVM自然崩溃**。

配置请求512MiB Sequential上限，另以250ms轮询检查空间；保留至少8GiB空闲，
保存/合并前再检查空间。WPR接受配置，实际ETL178,651,136字节未触界；
不把未触界说成已实测上限停止。补充了自有命令清理和自有WPR取消兜底，本轮未触发。
原始执行结果保留，不因后续离线分析而改写控制器当时的decoded/qualification字段。

| 操作 | 实际退出码/结果 |
| --- | --- |
| 基线、采集目标 | 均为 -532528300 / 0xE0424354 |
| WPR start、active、counters、stop | 均0 |
| 本实例及全局after | 均0，WPR is not recording |
| xperf stats/process/dumper、tracerpt | 均0 |
| Full Dump | 25,516,559字节，FullMemory/ThreadInfo/FullMemoryInfo |

提权助手自身OS退出码未单独保存；上表目标/WPR退出码来自保留的Process对象。
异常ExceptionStream的RIP/RSP/RBP与同次原CONTEXT逐项一致，6线程上下文、157内存段；
RBP=0在此优化资格程序中不是寄存器违约。完整内存和系统级ETL只留本地。

## 保留问题的实测改善

| 检查项 | 旧Memory | 本次File |
| --- | ---: | ---: |
| 目标PT记录 | 7 | 14,931 |
| 最后PT至异常处理器取时 | 7.7426714秒 | 0.0003513秒 |
| 目标进程生命周期事件 | 0 | 3 |
| 工作线程创建/退出 | 0 | 2 |
| 目标模块加载/卸载 | 0 | 34 |
| 目标CSwitch | 7 | 10,716 |
| LastDroppedTimes | 2条，29个非零槽 | 未出现 |

目标原始SampledProfile的14,931条QPC/IP与xperf全部匹配，没有时间换算错配。
EXE、KernelBase模块范围与同次DMP对应；同线程在CPU8切入，异常处理器取时后8.1微秒
切出，随后直到线程退出的旁带仍在。因此本次尾部不能再简单归结为整个时间段未保留。
详见 [FILE01.md](FILE01.md) 和 [file01-coverage-compact-public.json](file01-coverage-compact-public.json)。

## 为什么仍未通过故障窗口门槛

末6个目标片段经原Intel libipt/XED实际包解码和指令解码，退出码均0且无诊断错误。
但688,203条指令输出中没有RaiseException、Work调用RaiseException的指令、
CaptureQualificationException或相关Wait调用；最后解码指令仍是ExecuteChunk内的
`0x7ff616bb13c3: call BranchOdd`。见 [fault-path-public.json](fault-path-public.json)。

最后PT距工作结束327.8微秒、距异常处理器取时351.3微秒。后一个QPC是在处理器复制
异常CONTEXT后读取，不是RaiseException那条指令的精确时间，不能把两者混用。
File模式保住了已导出的事件，却不会让4KiB的采样片段变成连续指令历史。

还有可复核的容量上界：[BRANCH-CAPACITY.md](BRANCH-CAPACITY.md)。同次工作完成
3,590,651,904次循环，原机器码每次至少一个条件分支；即使把全系统PT数据都算给目标，
其全部位数也最多满足这个最低条件分支结果需求的27.0449%。这不是精确指令覆盖率，
但足以否定“已保留完整15秒执行历史”。它不排除某个短故障尾窗可被完整捕获。

## 实际干扰与第二步状态

本次总吞吐相对基线 **-3.4543%**，批次中位吞吐-3.0310%，
每次迭代线程CPU时间+3.3595%，cycles+3.1438%。只有一对顺序执行的15秒资格负载，
未测全系统CPU，也不是可靠P95/P99或原NOVONET长期低干扰验收。

实际故障后切出有CSwitch旁带，但当前PT触发仅SampledProfile，故准备了单独的
File+CSwitch对照，沿用同一EXE、同样fault/fault基线、缓冲和上限；不是新增采集器。
WPR解析确认两个原生触发，**第二次UAC返回用户取消，run-filecs01未创建、未执行**。
`controller-file-cswitch/launch-01.json`保留取消结果。准备成功不等于异常尾部已采到。

## 保护与同步

本轮没有原节点启动、原库/身份读取或改写、TTD、PageHeap、高频CDB回调、猜测性修复或
双中继恢复。WPR已停止，无资格进程遗留。原失败现场不覆盖。
C独立调查，Git用于同步，不以A签收为前置。

公开目录只含目标白名单数值、原公开命令输出、哈希和诊断源码；ETL、XML/CSV、DMP、
原始PT字节及完整解码输出不上传。源码副本记录本机执行位置，不能脱离原私有输入
目录直接假称可重放原故障。后续必须实际捕获并核验短故障尾窗，再判断是否足以进入
原程序隔离运行；不能用线程/模块旁带齐全替代首次RBP写入证据。

依据：[WPR保留模式](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/logging-mode)、
[Sequential文件上限](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/maximumfilesize)、
[WPR schema](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wprcontrolprofiles-schema)。
采样与指令完整性边界另见 [PT-SEMANTICS.md](PT-SEMANTICS.md)。
