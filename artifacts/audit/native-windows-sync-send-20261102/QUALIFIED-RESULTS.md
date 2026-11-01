# 当前Windows同步发送与回环：有效采集离线结论

**本次证明：原入口线程在同步发送调用链中承担了可观的内核工作，且大部分发送样本经过本机回环交付；不是主要卡在Winsock用户态入口或调度休眠。**
这不是性能修改或12K差距的完整解释，也不是物理四机网络结果。

原始目录：`target/native-windows-sync-send-20261102/20261102-030653-15912`。
用户提供采集后，本次仅离线解析；没有重新录制、启动节点/压测、修改生产Rust或网络配置。
结构化结果见[RESULTS.json](RESULTS.json)，五轮原报告见
[qualified-workload.json](qualified-workload.json)及其哈希绑定的原始压缩件。

## 1. 资格与原负载

冻结源码`486967c7`，node `24de0d1e…`、driver `647a058a…`、AOEM `8779d014…`，
三份匹配PDB及SHA-256见[candidate.json](candidate.json)。本次重新计算六份文件哈希，全部一致。
生产策略不变；仍是原`native_execution_tick → NativeLifecycle → NovoRUDP/APFL`。

两次预检加五次正式ETL头均为零丢事件、零丢buffer。五个正式窗口全部位于相应轮次
GetProcessTimes的drive前后UTC标记之间；这些标记包含边界统计/尾部核对，不是精确的耐久确认时间戳。
CPU采样周期均为1000µs、目标事件权重均为1。用该轮原node0 PID和T-DCStart/T-DCEnd中的
真实`novovm-native-tick`定位，不复用上次PID，也不把它叫作Windows初始主线程。

五轮同一份预生成ML-DSA-65签名字节、同创世和初始根；1,024个真实付款账户、共享收款方，
业务执行始终CPU callbacks；1,024行/8MiB、8,192在途、单机四进程、四方最终回执终点。
原节点额外阶段/IO/asset诊断关闭，但有ETW，所以以下仅为**静态同字节归因负载**，
不能代替持续供给或关闭采样的性能验收。

| 轮次 | 验签 | profile / ETL秒数 | 入口PID/TID | 成功TPS | P95 / P99秒 |
| --- | --- | --- | --- | ---: | ---: |
| 0 | CPU | CPU栈 / 8.488 | 35580 / 31996 | 2593.17 | 3.951 / 4.121 |
| 1 | GPU-required | CPU栈 / 8.875 | 21056 / 24184 | 2660.01 | 3.722 / 4.423 |
| 2 | CPU | 调度 / 5.516 | 31004 / 19464 | 2638.02 | 3.595 / 4.235 |
| 3 | GPU-required | 调度 / 5.491 | 24928 / 3800 | 2765.67 | 3.704 / 4.400 |
| 4 | CPU | CPU栈 / 8.490 | 33808 / 33880 | 2698.53 | 3.708 / 4.112 |

五轮各65,536笔，共327,680笔成功；每轮64高度×1,024笔，四节点最终一致。
费用、账户控制权、余额、nonce及四库冷恢复均通过。没有新做强杀故障轮，不能把冷恢复扩大为新增故障验收。
同机样本不能签稳定4K、恢复12K，亦不能由这些不同profile轮次计算GPU因果收益。

## 2. 当前制品CPU栈：发送链约24%，回环约16%–17%

所有下列百分比的分母是**该原PID/TID的全部原始SampledProfile事件**，
不是整机CPU、不是整笔交易墙钟，也没有把缺栈样本从分母中删除。

| CPU栈窗口 | 原始样本 | 已表示栈 / 缺栈 | 原网络poll_io（含子调用） | send_to调用链（含子调用） | IppLoopbackEnqueue（含子调用） |
| --- | ---: | ---: | ---: | ---: | ---: |
| CPU / r0 | 8348 | 8164 / 184 | 62.40% | 2021 / 24.21% | 1418 / 16.99% |
| GPU / r1 | 8319 | 8139 / 180 | 62.10% | 1971 / 23.69% | 1311 / 15.76% |
| CPU / r4 | 8354 | 8163 / 191 | 62.64% | 2025 / 24.24% | 1438 / 17.21% |

栈表示覆盖97.71%–97.84%；已表示栈内另有3/2/6个未知叶样本，保留未知。
**这些是包含关系，不能相加。** 本次没有沿用旧ETW的15%回环比例。

对同一PID/TID另导出包含`UdpSocket::send_to`和`IppLoopbackEnqueue`的过滤栈，
其总数分别与全栈对应包含计数一致。用发送过滤栈计算真正的交集：

| 窗口 | 发送样本 | 其中回环交集 | 交集占发送样本 | 未带send帧的回环样本 | 发送栈已知内核叶样本 |
| --- | ---: | ---: | ---: | ---: | ---: |
| r0 | 2021 | 1417 | 70.11% | 1 | 1994 / 98.66% |
| r1 | 1971 | 1309 | 66.41% | 2 | 1937 / 98.27% |
| r4 | 2025 | 1437 | 70.96% | 1 | 1992 / 98.37% |

这里最后一列按**互斥叶样本所属模块**计数：ntkrnlmp、tcpip、netio、afd、NDIS、
winnat等；余量保留Winsock用户态/其他/未知叶，完整分区在JSON。
这说明采样命中时，发送调用栈主要在内核执行，而非Rust拼接或Winsock用户态准备。
不是一次调用的98%墙钟，也没有单独扣除中断/DPC；不能叫作纯send指令周期。

回环过滤栈还直接显示接收侧内核工作：

| 包含函数样本（不可相加） | r0 | r1 | r4 |
| --- | ---: | ---: | ---: |
| UdpDeliverDatagrams | 1007 | 916 | 1049 |
| InetInspectReceiveDatagram | 795 | 740 | 820 |
| WfpProcessInTransportStackIndication | 743 | 689 | 756 |
| AfdTLDgramReceiveEventHandler | 168 | 140 | 170 |

因此“send_to里耗时”不只是用户态→内核的一次切换；本机交付和接收检查路径确实出现在
发送线程上下文中。但符号出现不证明WFP/WinNAT/NDIS异常或可删除；没有关闭防火墙、驱动、
认证或校验。不能把稀疏的`UdpSegmentMessage`符号计数当成全部分段成本，
也没有分段/单发、不同批大小的一因素实验。

## 3. 调度窗口：不是主要等接收线程唤醒或OS排程

两个独立调度轮均选择trace内300,000–5,200,000µs，共4.9秒；位于首个可靠CSwitch之后。
使用原CSwitch/ReadyThread还原，独立xperf cpu util交叉核对。

| 状态 | CPU r2 | GPU r3 |
| --- | ---: | ---: |
| Running | 4,825,833µs / 98.4864% | 4,826,751µs / 98.5051% |
| Ready | 70,651µs / 1.4419% | 64,813µs / 1.3227% |
| Waiting | 3,481µs / 0.0710% | 7,083µs / 0.1446% |
| Unknown | 35µs / 0.0007% | 1,353µs / 0.0276% |

xperf独立Running为4,825,836/4,827,829µs，与还原差3/1,078µs，不超过保留Unknown量。
最长已表示等待为2,562/2,466µs；Waiting均标为WrAlertByThreadId，**不是等待调用栈**。
原始缺配对窗口留Unknown，早于switch-out的Ready事件不当作在CPU上等待。

Running是线程驻留，可能含轮询/自旋及DPC/ISR，不能全部叫作有效业务计算。
CPU栈与调度来自不同轮的短窗，不能拼成逐次send_to等待时间。
当前数据没有接收端唤醒栈/跨线程因果链，不能断言某个接收线程拖住发送。
入口进程中的candidate-io、compute-owner、candidate-pipeline仍有运行；
没有因入口忙就认定AOEM没有多核工作。

## 4. 已修正一个会误导优化的符号归属

优化后的公开Rust PDB缺少部分私有/inline信息。部分发送调用者被最近的公开符号显示为
`novovm_udp_batch::poll_timer::NativePollTimer::new`，**不能登记为计时器成本**。

匹配image/PDB的unwind、反汇编与原源码表明：

- 真正timer函数RVA `0x17a38a0–0x17a3971`，cleanup `0x17a3980–0x17a39b7`；
  其中调用timeBeginPeriod(1)，没有发送数据。
- 独立的原批发送机器函数在`0x17a39c0–0x17a3e9f`，
  `0x17a3d0a`调用std send_to，返回RVA `0x17a3d0f`。
- r0原始Stack地址以该进程真实Image rundown基址`0x00007ff63b170000`还原。
  误标的全部1,001个样本时间均命中返回RVA `0x17a3d0f`，且与原PID35580/TID31996
  的SampledProfile时间相交；实际timer本体/cleanup为零命中。

这证明原批发路径实际运行，不能再次以“批发没接上”或“timer占用一半发送CPU”为方向。
其他两轮902/998个误标桶仅有同image静态映射，本次没有各自做完整原始地址闭合，区别保留。
广义`rpc::error`公开标签同样不能当作HTTP业务成本。

独立raw dumper退出0，但保留24条InvalidEvent解码警告；其中目标SampledProfile为8,345条，
与profile专门导出的8,348条差3。原始地址分析只声明上述1,001个时间交集闭合，
不称整个raw导出无缺口；ETL头零lost与导出器解码警告是两个不同检查。

## 5. 本轮决定与边界

不再重构Frame或Windows连续缓冲区，不改scatter/gather、批大小、发送配额、补片、
线程、AEAD、nonce、账户、AOEM/GPU或共识。这里没有实现新的提速，不能宣称恢复4K/12K。

本次把混合成本进一步落到**原同步发送内部、尤其同机内核回环交付**，但send调用只占
入口CPU样本约24%，不是整个系统吞吐的24%，其余网络/状态工作也没有消失。
没有完成历史12K至当前差距的全因果分解，也不能推算移走回环就能提高多少TPS。

下一项优先是按既有A/B/C/D做隔离物理网络对照的准备，**先提示用户完成设置**。
目前仍是单机，没有连接/部署/启动远端；原[README四机清单](README.md#3-四机只准备边界设置前必须先提示用户)
继续有效。不同OS、CPU和Linux当前单包owner路径也必须明列，不能将差异全部归于回环。
main、正式随包库、部署、既有数据库及签票日志保持不动。

## 6. 可复现离线分析与检查

复用原栈解析器，仅为其增加可导入入口；历史固定capture CLI保留，不执行它覆盖旧报告。
新增两个脚本只读现有xperf导出并生成归约JSON，绝不启动节点、采集或网络探测：

```powershell
$capture = 'D:\WorksArea\SUPERVM\target\native-windows-sync-send-20261102\20261102-030653-15912'
node artifacts/audit/native-windows-sync-send-20261102/analyze-schedule.cjs $capture
node artifacts/audit/native-windows-sync-send-20261102/analyze-sync-send.cjs $capture artifacts/audit/native-windows-sync-send-20261102/RESULTS.json
```

原xperf导出包括：CPU profile事件、目标PID/TID butterfly栈、send/loopback过滤栈、线程名rundown、
频率、schedule dumper/cpu util，以及r0原始Stack。只选当前目标进程做公开归约；
完整ETL、全系统CSV、PDB、符号缓存及平台隐私快照全部留在本机target。
公开RESULTS保留来源文件哈希、分母、缺失量及符号归属限制，原产品报告压缩件不含系统trace或私钥。

通过范围：两脚本语法/离线重算、7个零损header、6份image/PDB哈希、5个drive覆盖窗、
3组原始样本/栈/互斥叶/过滤交集、2组调度独立核对及历史栈解析器回归。
本次没有生产源码修改，不重跑无关Rust全仓/云端CI，也不把既往回归记作本次新通过。
