# Native Ingress Hot Thread Attribution：首次录制因丢事件拒收

当前冻结节点、AOEM、账户/共识、AEAD、补片、候选、线程数量和生产调度不变。
这是诊断工具的修正，不是NOVOVM程序提速或回退。完整首次复核见
[FIRST-TRACE-REVIEW.md](FIRST-TRACE-REVIEW.md)。

## 当前结论

用户已完成管理员采集，原权限门禁已解除。原脚本使用默认CPU详细profile，
产生28,413,263,872字节ETL，并丢失269,327,509事件。load_exit=0、stop_exit=0
只证明负载/保存流程成功，**不证明采样完整**。本次函数占比与Ready/Wait时长拒收。

PDB可用，残留样本确实能解析原Rust函数。三组PE/PDB GUID、Age及哈希见
[symbol-validation.json](symbol-validation.json)，但符号可解析不能补回丢失事件。
原ETL、PDB和含其他进程信息的分析文件仍仅留本机；没有删除或公开上传。

## 已替换过重的采集方式

原默认CPU profile同时含系统切换/Ready栈及Networking-Correlation等event providers。
新[ingress-minimal.wprp](ingress-minimal.wprp)拆为：

| Profile | 采集 | 不采集 |
| --- | --- | --- |
| NativeCpuStacks | 周期CPU样本栈、进程/线程/模块身份 | CSwitch/ReadyThread栈、Networking-Correlation等event providers |
| NativeSchedule | CSwitch/ReadyThread事件、进程/线程/模块身份 | 所有栈；因此不能提供具体等待调用点或某把锁的归因 |

两者各使用128×1024KiB缓冲（约128MiB），不是无限内存，也不保证不丢事件。
系统provider仍是全机录制，**没有伪称在录制端按NOVOVM PID过滤**；只在离线分析筛目标。
不改全局采样周期、系统策略、亲和性、worker或节点参数。

## 只需重新运行同一个管理员命令

在管理员PowerShell执行：

```powershell
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -File 'D:\WorksArea\SUPERVM\artifacts\audit\native-ingress-hot-thread-20261101\capture.ps1'
```

[capture.ps1](capture.ps1)仍校验原三份冻结制品和三份匹配PDB，不重建/替换它们。
先分别短启/停两种profile并检查header丢事件；失败不启动负载。再启动原有
`native_asset_same_corpus_five_round_stability`，同字节CPU/GPU/CPU/GPU/CPU五轮65K，
不另造负载器；1024账户、1024行/8MiB、8192在途、四方回执及冷恢复要求均保留。

只在各轮四个create stdout出现后约8秒请求下列短窗口，不从keygen一直录到冷恢复：

| 轮 | 验签 | 录制 | 请求窗口 |
| --- | --- | --- | ---: |
| 0 | CPU | 周期CPU栈 | 8秒 |
| 1 | GPU-required | 周期CPU栈 | 8秒 |
| 2 | CPU | 无栈调度事件 | 5秒 |
| 3 | GPU-required | 无栈调度事件 | 5秒 |
| 4 | CPU | 周期CPU栈重复 | 8秒 |

这是采集时机的提示，**不是可信就绪或交易边界**。实际ETL仍须与原drive UTC相交：
CPU窗口至少4秒、调度窗口至少3秒有效重叠，否则拒收；不能把短窗口外推为全负载占比。
原600秒单轮/原准备截止不变；观察器等待超过自身界限只停止继续采集，不取消原负载。

每个窗口保存后读取header；丢事件非零、读取失败或计数缺失，立即停止后续录制，
保留日志并等待原负载完成，不清库、不删journal、不自动加缓存重试。
`-tle`仅用于读取header计数，**不用于让损坏记录通过门禁**。
零丢事件也只记为“需要目标窗口/频率/符号复验”，不直接记完整诊断PASS。
同一header读取实现已用本次真实损坏ETL和不存在的文件验证：分别返回
REJECTED_EVENT_LOSS和UNKNOWN，均不通过采集门禁；见[质量门测试](quality-gate-tests.json)。

输出仍为 `target/native-ingress-hot-thread-20261101/<时间>-<PID>/`，包含
capture.json、每窗ETL/日志/header和符号副本；原28GB失败记录不动。
任何stop失败都须按capture.json.windows里的session检查/补停，不全局cancel其他会话：

```powershell
wpr -status -instancename <session>
wpr -stop <etl路径> -skipPdbGen -instancename <session>
```

-ValidateOnly只验证六份文件哈希和WPRP XSD，不启动录制或负载。
当前新配置已通过PowerShell语法、安装SDK的XSD、WPR profiledetails及ValidateOnly；
**新的实际管理员采集流程尚未执行验收**。

## 分析门槛

1. 用原report里实际single_submission_node、各轮PID、进程身份及drive UTC选窗，不沿用旧PID/TID。
2. 确认事件完整、有效重叠、采样频率和符号覆盖，再报告忙线程的独占/包含样本占比；
   两者不能相加，未解析或内联的不伪归类。
3. 调度窗重建Running/Ready/Waiting，边缘不完整区间单列；没有等待栈，不直接判为锁竞争。
4. CPU/GPU比较说明实际覆盖的时段和工作，短窗份额不是整轮CPU/笔；原全进程CPU计量另列。
5. 原始system ETL/PDB不公开上传，仅保存目标节点/driver的筛选结果。

依据：[SystemProvider](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/systemprovider)、
[CaptureStateOnStart](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/capturestateonstart-in-systemprovider)、
[CPU Analysis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/cpu-analysis)。
