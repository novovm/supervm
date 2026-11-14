# C Phase 6：原负载、线程现场与满载记录遍历（2026-11-14）

**首次参数错位仍未定位；没有生产修复，C NOT READY，双中继继续暂停。**

C 独立完成原日志采集、原转储分析、源码核对和新的隔离验证，不等待 A 代查。
本轮开始 fetch 后 main 为 `48242b6a`；A 最近明确签收仍为 `0470f343` 对
`e1e3a999` 的回执。Git 通知用于共享进度，不是本机分析的审批前提。

本轮新隔离状态查询实际完成：2,048 条输入逐字段核对（12,288 项）；131,072 次 hash 字节格式化入口计数。
选定窗口记录 2,304 条边界、384 组完整调用（344 尾调用、40 普通零填充调用），
6,149 项边界/运行检查无失败、无不完整组。目标和 CDB 退出均 0，约 164.426 秒，未触及 300 秒上限。
**未复现原故障，不把计数范围等同完整边界覆盖。**

## 1. 补齐了哪些原始条件

本轮只读复制 PID37796 原运行目录的 progress、stdout、stderr、owner 和 exit。
源文件复制前后 SHA256 一致，采集退出 0；没有打开数据库或用途钥。
[采集原结果](collection-public.json)、[最后完整进度的筛选计数](original-load-public.json)。
原始进度、转储、PDB 和未经筛选的日志留在本机，不上传 Git。

| 原证据 | 实际结果 | 解释边界 |
|---|---|---|
| 最后完整 progress | UTC 11-13 02:15:47.4746352；1,511,456 字节 | 比 WER 异常时间早约 5.327 秒，不是崩溃瞬间快照 |
| WER 异常 / dump | 02:15:52.801678 / 02:15:56 UTC | 文件写入、异常和 dump 时间分别保留 |
| 原运行入口 | original_native_execution_tick；7,007,103 ticks；height 1 / round 294 | 周期状态输出，不能把轮次当已执行区块 |
| 业务执行 | pending、executed_batches、durable_decisions、execution_failures 均 0 | 未发现该快照中的交易执行积压；不等于整个进程没有负载 |
| 传输记录 | 2,048 条满载，已淘汰 213,510 条；保留 seq 213510..215557 | 长期周转的记录容器，前轮小样例没有覆盖 |
| 保留事件 | realtime_owned 693、admitted 695、delivered 645、received 15 | 约 54 秒窗口；是原事件名称，不是 Business Receipt 或最终性 |
| 原传输计数 | active polls 2,800,415；reported data writes 74,103；accepted data 7,596 | 存在原生传输活动，不能称为纯空闲节点 |
| 对象观察 | 当前 245；evicted 1,907；expired 9,739 | 有对象周转，未保存全部旧对象内容 |
| 中继诊断 | probe commands 0；relay events 0；active false；verified locators 3 | 该快照没有本次双中继对象负载，不从“有观察器”推导已发对象 |
| 其他 profile | stage、critical、control、asset 关闭；native_delivery 开启 | 新验证按此配置，未继续使用前轮全部 profile 开启的条件 |

原 stdout 只有 133 字节启动行，stderr 为 0 字节；没有可据此重建每次调度的日志。
`native_rejected_messages=558`、发现/握手和历史包计数均在 JSON 中按原字段保存，
不能将所有拒绝合并为同一种故障原因。

## 2. 原转储的线程和容器现场

使用原 a254 EXE、精确 GUID `E16D0B11-47CA-4EBF-B482-C91380CC57F8` / age 1 PDB，
离线打开未注入 TTD 的 PID37796 MiniDump；未用本轮新 PDB 替换原符号。
[结构化结果](original-context-public.json)及[私有文件哈希](private-evidence-hashes.json)。

- 共 20 个线程；异常线程为 `novovm-native-tick`，TID `0x7c84`。
- pipeline `0x76c8`、I/O `0x6dd4`、compute owner `0x9e00` 在保存现场中等待 channel。
- 四个 `aoem-semantic-graph-v2-0` 线程在 WaitOnAddress；Tokio blocking pool 在等待，
  socket helper 在完成端口等待。其他 AOEM 帧只有最近导出名，不能把大偏移标签当实际函数。
- **其他线程的普通栈是在 WER 处理期间保存的**，并非故障前每个调用边界的同时记录；
  不能据此排除更早的并发写入，也不能断言 AOEM 一直空闲。
- `!runaway` 给出零 CPU 和不合理的 6413 天 elapsed，未作为线程负载证据。

`.ecxr` 后的异常栈继续指向：

`original tick → observe_tick(Status) → RpcLifecycle::status → snapshot → record fold`
`→ String::from_iter → format_inner → fmt::write → byte hex → writer → memcpy`。

原 snapshot 机器码范围 RVA `0x75c4a0..0x76330f`，返回位置 `0x7614af`。
最近符号仍可能显示 `elapsed+0x50df`；不据此将故障归到计时函数。

### 新增的环形容器证据

机器码 stride 为 `0xb8`（184 字节）。原栈槽 `0xe7e2eead0` 保存两个迭代范围：

| 范围 | 开始 | 结束 | 条数 |
|---|---|---|---|
| 第一段 | `0x2044cc02d50` | `0x2044cc3f070` | 1,340 |
| 第二段 | `0x2044cbe3070` | `0x2044cc02d50` | 708 |

展开帧中的 record 地址 `0x2044cc156e0` 位于第一段第 414 项，地址差可整除 184；
与相邻帧恢复出的迭代计数相符。这些是 **unwind 恢复值及最终内存**，不是历史入口寄存器录制。
对应 record 的堆页不在 MiniDump 中，读取为 `????????`，无法取得其确切 seq/event。

栈上复制的 32 字节值匹配最后进度中的一个 peer 值：681 条匹配，fragment 匹配 0。
因此不能唯一识别原失败记录。复制数组的第 19 字节与字节格式化路径一致，但没有证明首次破坏。
为避免公开节点关联，未发布该 peer 值及原始记录表。

最后 progress 按原 push/pop 语义推得分段 1,530＋518，实际故障帧为 1,340＋708。
若只前进 190 条，故障项可能对应旧窗口第 604 项；**环绕次数及缺失内容无法确认，
此项只是后续采样位置的条件假设，不是已定位的原故障事件**。

## 3. 调用与并发关系的源码核对

源码固定为 `a25452f4c49be37cd984e650f2079642e6c273b3`，
[8 项文件核对](source-verification.json)包含 profile、原服务/RPC、native lifecycle、compute 和 I/O。

- `profile.rs:85` 的 `push_latest` 满载时 pop_front 再 push_back，不调用 make_contiguous。
- `profile.rs:180` 的 delivery recorder 在锁内复制值并记录；snapshot 的 iterator/map/collect
  在同一个 DELIVERY guard 范围内完成，不把 record 借用带到释放 guard 之后。
- `consensus/native_lifecycle.rs:503/590/694/801` 的 delivered/admitted/realtime_owned/received
  接在原 lifecycle 路径；不能把这些事件名称当作四个异步写入线程。
- 原 owner 的 poll 与 status 顺序执行；状态 JSON 生成后才编码并写文件。
  外部 Watch-Relay 读进度文件，不拥有局部 String 或 Formatter，也不发起该状态 RPC。
- 后台线程仍可运行，但本轮未发现把这个局部 String 指针跨 FFI/线程保存的源码。
  这不排除其他模块更早破坏进程内存。

## 4. 新隔离验证的范围

不是重复 Phase4 的 64 次快照或 Phase5 的小记录样例。
本轮新诊断 EXE 使用原 a254 服务实现、AOEM966、原 16 MiB tick 栈及真实 status 入口，
仅使用不存在的新测试目录、新测试 purpose keys、回环 peer 和新测试库。
没有运行部署节点、连接其他验证机或执行实际资产交易。

通过原 recorder 写入原最后保留的 2,048 条记录，逐字段核对 seq/event/height/purposes/peer/fragment。
为得到相同环形位置，之前已丢失的 213,510 条内容使用明确的测试填充项，采集前全部被淘汰。
**未重现旧内容、旧堆分配历史、原时间间隔或故障瞬间的 1,340＋708 窗口。**
没有额外添加虚构的重计算任务或并发生产者，以免冒充原故障负载。

新 EXE/PDB 配对和 32 个 bin/lib 编译单元调试流已核验：
[构建记录](diagnostic-build-public.json)、[编译参数](build-command.json)、
[关键函数机器码/unwind 对照](c-phase6-status-function-map.json)。
规范化机器码排除重定位位移后相同，不代表整个程序、布局、时序或原编译配置完全相同。

边界采集覆盖两组选择窗口：旧窗口第 603..605 项以及 1529..1531 项（零起始）。
前者是上述条件对应位置，后者跨原最后进度的物理分段。其余格式化入口只计数，
**不能把整个状态成功输出当成每个调用边界都已记录**。
[实际退出与比较](capture-results-public.json)、[边界记录](capture-records-public.json)、
[覆盖缺口](coverage-gaps-public.json)、[筛选调试输出](capture2-public.log)。

第一次采集脚本使用错误的显式断点编号写法，CDB 报 Range error，目标虽然正常输出，
但没有取得边界记录；该次只记采集失败。修正入口后使用新的 capture2 目录，旧结果保留。
本轮离线分析也保留了 `.frame /r` 后错误使用表达式寄存器的读数；随后改用输出中的
明确地址复核，结论只采用复核结果。未覆盖任何失败证据。

## 5. 尚缺什么，下一步如何收敛

尚未捕获首次参数变化，也没有证实 ABI、String、IndexMap、AOEM 或硬件中任何一项为根因。
本轮不能推出“满载是原因”，也不能推出“所有并发条件已经排除”。

仍缺：

1. 最后完整 progress 到异常之间的真实记录内容；MiniDump 缺 record 所在堆页。
2. 原进程持续运行期间的分配/释放历史，以及实际网络 poll 与诊断对象周转的先后关系。
3. 同一次失败执行中，Formatter 调用前到 writer 入口的完整参数证据；最终栈不能代替历史执行。

下一项有意义的隔离工作是把原 lifecycle 的入队、送达、过期/淘汰与周期 status 放进
同一个原 owner 循环，按已核对的控制流生成有界负载；不能再仅增加本轮 recorder 快照次数。
若缺少原包/调度内容，应明确这是控制流验证而非失败过程重放。只有实际捕获参数变化，
才能提出针对该指令/所有者的修复及独立回归，不能修补受害 writer 来掩盖错误。

本轮仅提交诊断源码、筛选证据和交接；未改生产代码、部署程序、原库或身份。
不要求 A/D 重跑、更新或重新预约；A 只需读取 Git 结果，C 继续独立调查。
