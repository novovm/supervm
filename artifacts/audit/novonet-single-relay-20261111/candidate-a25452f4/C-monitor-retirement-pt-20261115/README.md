# C：停用高频调用监控，核对 NtContinue 关闭冲突及原生 PT 入口

本轮没有运行节点或人工样例。**首次 RBP 错位仍未定位，无生产修复，双中继保持暂停。**
上一轮的长时间断点方案不合格，不能把它延长到66分钟。已将两个执行入口改为立即拒绝执行，
保留原执行脚本、所有原始记录及转储；本轮只进行离线系统指令核对和本机工具只读核验。

## 已落地的处置

- `scripts/diagnostics/rbp-original-instruction-run.py` 与 `rbp-sustained-isolated-run.py`
  在任何参数解析、目录写入或进程创建之前，以 `RETIRED` 和退出码1终止。
- 两个入口实际调用均返回1；Python语法解析通过。见 [retirement-check.json](retirement-check.json)。
  没有借验证入口重新运行八组调用、四分钟负载或人工违约样例。
- 原来真正执行的副本仍为 `target/c-original-rbp-sustained-20261115-run1/executed-script.py`，
  不覆盖历史脚本或把它的结果重新标为成功。此次改的是诊断入口，不是生产节点。

## NtContinue：确认了关闭流程的冲突，未确认原始根因

上一轮归档脚本第98行将 `ntdll!NtContinue` 设为无条件、全线程断点，动作是输出
`CONTROLLER_ACTION debugger_q continue_gap` 后执行 `q`；没有关联目标线程或活动调用。
第111–113行的关闭流程先调用 `DebugBreakProcess`，再提交 `off.cmd`；而 `bc *`
清除断点只有在 `off.cmd` 获得执行之后才会发生。

同轮真实记录：UTC `2026-11-14T19:52:00.872215Z` 请求关闭，随后日志出现
`continue_gap`、`quit:`，没有 `MONITOR_COUNTS` 或 `MONITOR_OFF`。被调试节点保留句柄取得的
退出码为 **0xC0000354**，CDB为0。这是诊断守卫退出链，不是原始自然异常证据。
原始终态仍在 [上一轮公开结果](../C-sustained-rbp-20261115/terminal-public.json)。

只读分析旧PID34452 Full Dump中的同机系统代码，发现可成立的自冲突路径：

1. `DbgUiIssueRemoteBreakin` 选择 `DbgUiRemoteBreakin`，并调用 `RtlpCreateUserThreadEx`。
2. `LdrInitializeThunk+0x13` 调用 `NtContinue`。
3. `DbgUiRemoteBreakin` 随后调用 `DbgBreakPoint`。

因此，请求调试暂停所创建的线程初始化就可能先触发尚未清除的守卫，让关闭命令失去执行机会。
**这是静态可行路径，不是此次命中线程的证明**：PID29764守卫没有保存TID、调用栈、
CONTEXT或线程创建记录，不能把旧PID34452指令片段拼接成同次动态历史。
也不能因为末组停在 `CHILD_BEFORE`，就认定子函数破坏RBP。
详细依据见 [NTCONTINUE-AUDIT.md](NTCONTINUE-AUDIT.md)、[系统指令片段](system-code-verified.txt)。
本轮没有通过删掉守卫后重跑来掩盖关闭失败。

## 原生低干扰机制：已验证到哪一步

本机实读：i9-14900HX、Windows 11 build26300，CPUID leaf7 EBX=`239c27a9`，
Processor Trace位为1；leaf14也有输出。WPR与ipt.sys版本为10.0.26100.9818。
HypervisorPresent=true仅记录环境，不据此判断PT可用或不可用。
见 [capability-public.json](capability-public.json)。

| 项目 | 实际结果 | 不能推导的结论 |
| --- | --- | --- |
| CPUID / ipt.sys | 硬件标志及文件存在 | 驱动当前能采集、虚拟化配置兼容 |
| WPR PT配置解析 | **退出0**，输出 `Processor Trace Events: SampledProfile` | 已生成PT包或已低干扰 |
| WPR录制状态 | 退出0，`WPR is not recording` | 无动态性能结果 |
| LBR/PMU来源 | LBR stacksize32，最多同时选择9个profile sources | LBR等于PT历史或RBP写入值 |
| 解码工具 | PATH未发现WPA/xperf/VTune/ptxed；常用安装目录也未发现已验收链 | 不能据此断言所有Windows工具均不支持PT |
| 采集→解码→同次异常关联 | **未验收** | 不具备直接进入66分钟根因捕获的条件 |

实际只读命令为 `wpr -profiledetails <WPRP>!C_PT_Parser.Light`，随后 `wpr -status`。
候选 [C-PT-parser-only.wprp](C-PT-parser-only.wprp) SHA256为
`d270007efa293d3949267a30abbc04d1080eac676c2b7ce1774b6073b03b353e`。
配置包含进程/线程、模块装载、线程切换和SampledProfile，用户态PT及Memory模式。
解析接受不代表实际分配/保留窗口已测量；**没有执行 `wpr -start`**。
原输出见 [profiledetails-output.txt](profiledetails-output.txt)、[result.json](result.json)。

微软的 [WPR schema](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wprcontrolprofiles-schema)
确实支持ProcessorTrace；该配置元素没有PID过滤字段，不能将当前SystemCollector配置
声称为已经验收的单进程连续记录。Intel [libipt](https://github.com/intel/libipt) 提供控制流解码库，
其 [采集说明](https://github.com/intel/libipt/blob/master/doc/howto_capture.md) 不能直接充当本机
Windows WPR ETL到指令流的现成操作链。本机PATH中的ptdump来自PyTables，非Intel解码器。

PT主要提供控制流，不直接记录每条指令的RBP及内存读写值；精确代码映射和最终Full Dump
仍不足以自动确定较早某条 `pop rbp` 当时读到了什么。当前WER Full Dump也不意味着包含PT：
[MINIDUMP_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/minidumpapiset/ne-minidumpapiset-minidump_type)
将FullMemory与IptTrace列为不同标志。不得把最终内存状态当成执行历史。

## 下一次取证的具体缺口，不以扩大监控替代

下一次隔离目标运行之前，必须具备可使用的原生采集/解码链，能区分：

- 目标PID及创建时间、故障TID、模块装载地址和原始代码哈希。
- 同次PT实际覆盖的时间/线程区间、上下文切换、解码缺口与完整转储异常上下文。
- PT包缺失/覆盖与ETW的EventsLost、LogBuffersLost；环形日志覆盖旧记录不代表零缺失。
  [ETW说明](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/ns-evntrace-event_trace_properties)
- 原有状态报告节奏、实际吞吐和资源变化；不能只验证配置解析就声称低干扰。

本轮尚未取得上述动态数据，未启动新的66分钟负载。约6.286秒来自上一轮**CDB已附着的隔离基线**，
不是原生产环境已验证的报告间隔；历史约66分钟是一次观测到的故障时间，不是必然重现的周期。
若采集只能还原控制流而不能判定寄存器值的首次变化，也必须明确这一剩余缺口。
不新造逐调用采集器，不通过换一批正常样例宣称方法合格。

原库、身份、旧失败现场保留；未启动原节点/隔离节点、未附加活进程、未修改WER/ACL/驱动，
未安装录制器、不恢复双中继。公开仅为脚本退役、命令输出、系统指令片段及本报告；
完整DMP、内存/私钥及原始私有日志不上传。C独立推进，Git只同步结果，不等待A签收。
