# C：持续 NOVONET 隔离负载已运行，采集干扰门槛失败，首次错位未捕获

本轮只执行一次原 **a254＋AOEM966**、全新隔离库运行，没有重做八组启动调用或人工违约样例。
补齐 NOVONET 状态路径和目标进程独立退出句柄，但 **未复现原始 RBP 错位，未修复节点**。
持续断点开启后两分钟没有新的状态报告，停止长期采集；关闭监控时诊断守卫又主动退出，
原计划的恢复对照及75分钟窗口没有完成。不能将此次退出写成原始自然崩溃。

## 真实执行与原故障的关系

- 北京时间 **2026-11-15 03:48:00 至 03:52:01**；实际总墙钟 **241.625秒**。
- EXE SHA256：`1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d`。
- AOEM966 SHA256：`5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`。
- 四个原 EXE、当前用户独立测试 validator/transport 钥、四个新数据库。
  原程序在新目录自行生成独立 carrier 用途材料和签名 locator，没有复制原身份。
- 启用实际 `novonet_config_file` 和 `probe_control_file` 路径，但不创建任何 probe 命令。
  固定 loopback 对端，discovery=false、relay_forwarding=false、relays=[]，没有资产交易、
  人工业务对象或双中继发送；没有生产配置、超时和二进制修改。
- 原 a254 的周期路径包括 `network.observation()`、`product.status()`、序列化及文件写出。
  明确指定报告周期5000ms；文件更新间隔包含查询、调度和写盘，不是单次查询延迟。

上轮 run2 的 `novonet_config_file=None`，进度没有 `novonet` 和 `native_network_probe`；
不能仅把那轮延长后称作原路径复现。本轮两个状态分支实际出现，并有真实协议收发和记录周转：

| 计数 | 原 PID34452 最后归档 | 本轮目标最后报告（监控前） |
| --- | ---: | ---: |
| 运行范围 | 约66.22分钟 | 119.39秒时的最后报告 |
| ticks | 2847944 | 98511 |
| 保留传输对象记录 | 76 | 256 |
| expired / evicted | 2643 / 0 | 0 / 84 |
| send_polls.active_calls | 423210 | 17348 |
| reported_data_writes | 17192 | 1462 |
| accepted_data / history_data | 1708 / 15497 | 280 / 1100 |

另外三个隔离对端持续报告至约236秒，出现 expired/evicted 增长，证明存在持续周转。
这些是实际数值，**不是与历史负载等价的证明**：历史逐包顺序、三台远端的时序、LocalService
运行账户及旧堆布局没有复现。原 PID34452 没有执行用户 probe 或资产交易，本轮也未制造此类负载。

## 干扰与有界停止

同一目标进程只启动一次，计划 OFF 120秒 → ON 120秒 → 超限时 OFF 60秒，未超限才继续至75分钟。
门槛预先固定为报告间隔退化超过25%、ticks速率下降超过25%，或不足10份有效报告；
另有硬件命中、日志512MiB、磁盘和外部4560秒上限。25%是停止阈值，不是低干扰资格证明。

| 阶段 | 实际证据 |
| --- | --- |
| 未设调用断点、CDB已附着 | 20份目标报告，中位更新间隔 **6.286秒**，约831.42 ticks/s |
| 监控开启 | 实际 `MONITOR_ON`，连续约120秒 **零份新目标报告**；期间仍有寄存器事件 |
| 关闭监控 | 240.594秒发出请求，未取得 `MONITOR_OFF`，不能声明完成恢复对照 |

没有 ON 的报告样本，不能编造其 P50/P95/P99、tick吞吐或“增加25%”的实测百分比。
截至关闭请求，距最后一次报告已超过121秒；该间隔被停止事件截断，不能当作已完成的查询耗时。
同期其他节点持续写报告。**本配置没有通过持续负载采集门槛，不应继续加时等待原故障。**
原始66分钟条件没有覆盖，75分钟只是上限，不能写成已运行75分钟。

## RBP关联及未覆盖区间

`ntdll+0x5a108` 的 `RBP=RSP+0x100` 检查独立保留于全部线程，不再随八组结束清除。
原节点 `+0x84eed4` 调用点仍用于关联选定活动组的 LEA前/后、内部子调用前/后和检查前记录。
检查到错位会冻结同次完整转储，并继续原异常路径；没有使用人工错误指令。

离线核对2153组完整记录，未发现已观测边界错位；第2154组只有入口、LEA前/后、子调用前，
因为诊断退出而不完整，**不能当作子调用破坏RBP**。这些数量仅用于核对记录完整性，不作为修复成果。
全线程检查的最终总命中计数没有输出；选定活动组以外仍没有入口历史，内层子函数也不是逐指令记录。
正常前后值不能排除中间合法临时使用，不能证明已覆盖首次破坏指令。

无实际 `RBP_MISMATCH`、AV/fast-fail回调、调试命令语法失败；未生成本轮异常转储。
原始寄存器日志留本机，公开首/末完整组及未完成组见 [event-excerpt-public.txt](event-excerpt-public.txt)，
完整离线计数和判定见 [analysis-public.json](analysis-public.json)。

## 目标真实退出码与诊断自身失败

初始断点暂停时取得目标PID、路径、创建时间和 Windows 进程句柄，核对后才放行。
结束时使用保留句柄确认signaled并调用 GetExitCodeProcess，不再用 CDB 退出码代替节点。
接口依据：[GetExitCodeProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess)。

| 进程 | PID | 实际码 | 本次分类 |
| --- | ---: | --- | --- |
| 被调试隔离节点0 | 29764 | **0xC0000354** / -1073740972 | CDB守卫执行q后的退出，非已证实自然崩溃 |
| CDB | 41168 | 0 | 调试器退出 |
| 隔离节点1 | 4676 | 1 | 外层明确taskkill收尾 |
| 隔离节点2 | 20336 | 1 | 外层明确taskkill收尾 |
| 隔离节点3 | 39204 | 1 | 外层明确taskkill收尾 |

关闭监控请求后，原日志出现 **`CONTROLLER_ACTION debugger_q continue_gap`**，随后 `quit:`。
因此外层未见 `MONITOR_OFF`，保存 `runner_error: phase marker not observed: MONITOR_OFF`。
这是本轮诊断控制与保守 NtContinue 守卫发生冲突的失败现场；没有保存该入口的完整堆栈/恢复上下文，
不能反过来断定 NtContinue 是原始 RBP 根因。CDB的q会结束附着目标，行为依据见
[微软q说明](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/q--qq--quit-)。
原始 [terminal-public.json](terminal-public.json)、[actions-public.jsonl](actions-public.jsonl) 均保留，不改写为成功。
Python入口最终返回0只代表成功写出终态JSON，不代表取证验收成功。

结束后没有遗留节点或CDB；未重试。本轮目标OS退出值、控制动作和异常记录相互独立，
不把有日志的主动终止或未知退出统称“自然崩溃”。

## 保护及交接

私有目录：`target/c-original-rbp-sustained-20261115-run1`；旧run1/run2、PID34452完整转储及其余失败现场保留。
公开Git只有执行脚本、命令、白名单数值、退出证据和寄存器摘录。配置、封装钥、新库、完整日志及DMP不上传。
源代码语法核对及离线结果核验完成；未修改生产代码、未打开原库/原身份、未重启原节点、未恢复双中继。
首次错位仍未捕获，无猜测性修复。C独立执行，Git用于共享，不等待A签收。
