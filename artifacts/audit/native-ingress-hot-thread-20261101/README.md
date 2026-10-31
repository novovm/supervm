# Native Ingress Hot Thread Attribution：符号已齐，采样权限受阻

当前为诊断准备，不是函数成本结论。基线 `b4ea57df`，生产节点、AOEM、
AEAD接线、补片、候选、线程数量/调度、共识、main与正式部署均未改变。

## 实际阻断

本机WPR/WPA/xperf已安装。CPU profile实际包含SampledProfile、CSwitch、ReadyThread
及相应栈；尚无已有录制，使用本轮独立instance尝试开启后返回：

```text
Failed to enable the policy to profile system performance.
Error code: 0xc5585011
```

[原始失败日志](../native-controller-throughput-20261031/ingress-etw-start.log)。
随后核验当前进程AdministratorToken=False，指定instance状态仍为“not recording”。
没有启动压测、没有取消其他会话、没有修改注册表/策略或尝试自动提权。
**尚未取得CPU函数样本、Ready/Wait时长；不能填造网络/AEAD/解码等占比。**

## 匹配符号已找到，无需重建或更换运行制品

| 制品 | PDB位置 | CodeView GUID / Age |
| --- | --- | --- |
| 冻结节点6ed28e98… | `target/runtime-rebuild/release/novovm_node.pdb` | `5EA003E9-B833-45C8-B129-6570542B4CFE` / 1 |
| AOEM 8779d014… | `D:/WorksArea/AOEM/cargo-target-ci-local/release/aoem_ffi.pdb` | `DB48DF11-8E23-454A-88F8-0DD8577E6457` / 1 |
| 测试器647a058a… | `target/runtime-rebuild/release/deps/novovm_node-9c742166f7d138ac.pdb` | `0912C349-16A5-4477-A364-5ED8C9BE5888` / 4 |

已核验GUID/Age与各PE匹配，PDB中有真实Rust函数符号，包括原NativeChannel::poll_io、
原tick、E2eSecureChannelV1及UcaBatchStateRead等。优化内联函数可能没有独立采样地址，
不能因为平面表没有某个函数，就宣称它不消耗CPU。完整哈希固定在capture.ps1中。

## 人工启动唯一的受限采集

请在**管理员PowerShell**中执行（使用本机已安装的PowerShell 7，不读取用户profile）：

```powershell
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -File 'D:\WorksArea\SUPERVM\artifacts\audit\native-ingress-hot-thread-20261101\capture.ps1'
```

脚本仅验证三份冻结制品及匹配PDB、复制PDB到私有诊断目录，读取原候选环境，
启动独立命名的CPU录制，然后运行**原测试器**的同字节CPU/GPU/CPU三轮65K。
原业务后端、1024账户、1024行/8MiB、8192在途、四方回执及冷恢复核验不变，PROFILE关闭。
不是另建负载器，不改生产源码/签名，不加线程，也不把诊断TPS当新峰值。

录制结束由finally尝试停止**该instance**并保留ETL、日志、manifest与匹配符号，位于
`target/native-ingress-hot-thread-20261101/<时间>-<PID>/`。异常也保留目录；不清库，
不自动安装工具、不改系统性能策略、不用RunAs自动提权。若WPR启动失败，不运行负载。
该测试沿原600秒单轮截止/原准备限制；请等待进程完成，不在采集中运行其他重负载。

如stop失败，录制可能仍在。按输出目录 `capture.json` 的session和etl字段，
在管理员终端用 `wpr -status -instancename <session>` 检查，必要时执行
`wpr -stop <etl路径> -skipPdbGen -instancename <session>`。不全局cancel其他会话。

`-ValidateOnly`只校验现有文件哈希，不生成录制或负载，可在普通PowerShell 7中使用。
当前仅语法检查和ValidateOnly已通过；管理员实际采集/保存路径尚未执行验证。
三组PE/PDB GUID与Age的小型验证记录见[symbol-validation.json](symbol-validation.json)。
Windows system ETL可能含其他进程/文件元数据，**不公开上传原ETL或PDB**。
采集完成后告知输出目录，即可在普通权限下继续离线分析。

## 分析门槛

1. 用原每轮CPU边界UTC和ETL trace header对齐，仅取drive窗口、对应节点/driver进程身份；
   ETW自己的TID和运行时段确定忙线程，不混入启动、资格检查或冷恢复。
2. 先核查事件丢失、采样频率及符号覆盖，再报告独占/包含栈样本占比；两类不能相加。
3. 依CSwitch/ReadyThread计算Running/Ready/Waiting，区分执行与被唤醒后等调度；
   栈缺失时不把等待直接叫作锁竞争，也不以调用次数替代实际CPU成本。
4. 分别比较CPU/GPU轮的忙线程及全进程，说明节省发生在哪些函数、哪些关键工作未改变。
   原已有采样显示的0.97核可能含活跃轮询，不预定其为有用计算饱和。

方法依据：[Microsoft CPU Analysis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/cpu-analysis)
与[WPA Exporter](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/exporter)。
目前权限门禁未过，以上分析尚未执行，不登记性能或诊断闭环通过。
