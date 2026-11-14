# C Phase 5：原状态查询与并发上下文（2026-11-14）

**C 独立完成本轮分析和隔离验证。首次参数错位仍未定位；无生产修复，C NOT READY，双中继继续暂停。**

本轮从 `5c8ea29a` 继续，实际 Git fetch 后 main 仍为该提交。A 最近明确签收是
`0470f343` 对 `e1e3a999` 的回执；未见对后续分析的新增签收。A 仅接收 Git 协调结果，
**不是 C 排查、构建或修复的审批者，不等待 A 才继续本机工作**。

## 本轮结果与证据范围

| 项目 | 实际结果 | 不代表什么 |
|---|---|---|
| 原调用路径 | 已区分周期 JSON 进度、历史 JSON-RPC、原生二进制查询和观察器读文件 | 四者不能笼统视为同一次 status 调用 |
| 原 a254 库隔离接入 | 真实 `ResidentNode`、`RpcLifecycle::poll/status`、AOEM966、后台 compute/I/O owners | 没有启动部署节点或连接真实验证机 |
| 并发上下文 | 独立记录线程在 status 窗口内完成 3 次原 recorder/work API 更新 | 不是原故障数小时网络/业务负载的重放 |
| 完整边界 | 448 组，434 尾调用、14 普通零填充调用；11,176 项离线比较无失败 | 不是新增 448 次节点压力测试 |
| 采集缺口 | 33 组不完整；另一个 String writer 未设入口断点，部分观测重复或缺失 | 不把总计 481 个采集序号当作 481 次完整验证 |
| 原始结果 | 目标及 CDB 退出均 0，序列化回读相同，原 shutdown 成功 | **未复现不等于修复，未定位首次错误指令** |

[实际结果](capture-results-public.json)、[结构化边界](capture-records-public.json)、
[筛选的调试输出](capture1-public.log)、[未通过完整覆盖判定的记录](coverage-gaps-public.json)、
[实际时间及退出码](capture1-exit.json)、[线程交错](thread-timeline-public.txt)。
完整原日志留本机，[哈希](private-log-hashes.json)可核对。公开输出只来自新隔离测试进程，
仅保留边界、两段实际调用栈及明确测试标记；工作区路径脱敏。没有上传转储、PDB、密钥或数据库。

## 1. 原调用链：先区分真正进入故障路径的入口

以下行号固定于 `a25452f4c49be37cd984e650f2079642e6c273b3`。

### 周期进度输出

`crates/novovm-node/src/bin/novovm-node.rs:42602` 附近的原 owner 循环：

1. 原网络驱动推进，`product.poll(&mut self)` 更新 controller/pending 等状态。
2. 到进度周期后，42635 调用 `observe_tick(Status, || product.status())`。
3. `service/rpc.rs:1326` 的 `RpcLifecycle::status(&self)` 在当前 owner 栈上创建 owned JSON，
   读取 controller、普通集合及后台计数，1374 调用 `profile::snapshot()`。
4. `profile.rs:535` 的 snapshot 读取原记录容器，`hash` 将按值取得的 32 字节数组格式化为 String。
   原链是 `hash → format_inner → core::fmt::write → byte LowerHex → pad_integral → writer`。
5. status 返回 owned `Value`。42645 才做 `serde_json::to_vec_pretty`，42646 后才写进度文件；
   verbose 输出在 42666 附近。不能把最后的文件读取当作 String 写入调用者。

`observe_tick`（rpc.rs:50）同步执行闭包后才记录本次耗时；不是异步任务分发。
本轮捕获的两段实际栈分别证明 `rpc::hex → status` 和 `profile::hash → snapshot → status`。
这补齐了原生库调用上下文，但新测试入口不是部署节点的完整外层循环。

### 其他三种入口

- 历史 JSON-RPC：`rpc.rs:1409` 的 `handle`，1431 `nov_chainStatus → self.status()`，会走上述 JSON 快照。
- 当前认证原生查询：`rpc/native_query.rs:41,74–75` 的 `Query::Status → native_status()`，
  返回二进制 `Status`；**不调用该 JSON status/profile snapshot**。A 的认证查询成功不是这条格式化路径通过的证据。
- `Watch-Relay.ps1:18,24–25,47` 及 `Read-RelayProgress.ps1:14–24` 只打开原进度文件读 JSON，
  不发送状态 RPC、不注入节点。观察器开始后节点崩溃是相关时间线，现有代码不能证明观察器直接触发内存破坏。
  文件读写交错能产生不完整 JSON，原 reader 有有界重读；这和节点进程内错误地址须分开。

## 2. 线程、所有权、锁与销毁

| 对象或执行者 | 创建/拥有 | 共享、读取及释放 |
|---|---|---|
| 原 tick owner | bin:43261 或 native_service.rs:242 的 `novovm-native-tick`，16 MiB 栈 | `poll(&mut self)` 与 `status(&self)` 在这个调用路径上顺序执行；不是两个线程同时可变借用 RpcLifecycle |
| RpcLifecycle | rpc.rs:393,439；按值持有 ResidentNode、pending、projection 等 | status 同步借用，返回 owned Value；未发现局部 Formatter/String 被送到异步队列 |
| 后台 coordinator | pipeline/mod.rs:614–639；有界 channel、Arc 配置/身份、Mutex usage、原子签名计数 | `signature_counts` 读取原子计数；不持有 status 的局部 String |
| compute owner | pipeline/compute.rs:316 起；本线程持有 ComputeSession | 原消息/ticket 路径；本轮没有提交真实计算或交易任务 |
| I/O owner | persistence/io.rs:327 起；本线程打开 CandidateStore | 有界队列和结果 ticket；本轮仅访问新测试数据库 |
| CRITICAL / DELIVERY | profile.rs:105,109；owned CriticalRecord 内含按值数组 | snapshot 的 iter/map/collect 在 mutex guard 生命周期内完成；DELIVERY 使用 try_lock，不把悬空借用带到解锁之后 |
| CONTROL / ASSET / INTEGER | profile.rs:115,117,403 | CONTROL 同样持锁 collect；ASSET 同步序列化；INTEGER 持锁 clone；每个快照字段不是全局原子一致视图 |
| byte String / Formatter | hash 同步 `format!` 创建局部 String，再 collect 为 owned String | Formatter 仅在格式化调用期间借用；直接路径未见 FFI 保存 String 内部地址、raw parts 或跨线程回调 |
| 正常退出 | service.rs:247；pipeline/mod.rs:868–895 | drop controller，断开 sender，唤醒并 join coordinator，再 drain I/O；Drop 本身只断开/唤醒，不等同完成 shutdown |

后台 `work_interval` 可同时追加 CONTROL，delivery recorder 可同时追加 DELIVERY；原锁负责容器访问。
Relaxed 计数可能在不同时间读取，不等于发生数据竞争。以上只证明这段源码的所有权结构，
**不证明整个进程没有更早的 unsafe/FFI/栈破坏**。`catch_unwind` 也不能据此保证访问违规可恢复。
该路径是同步 std 线程，没有可凭空补写的 Tokio task ID；本轮记录 OS TID、Rust ThreadId、查询序号和测试任务序号。

## 3. 本次隔离环境实际做了什么

没有重复 Phase4 的 64 次普通 snapshot 样例。独立导出原 a254，原状态/服务/持久实现逐文件
与 Git 比较，[源码核验](source-verification.json)包含 875/a254 对照。所核对状态/快照/tick
源码相同，不宣称两个完整构建、全部库或两台机器环境相同。

[诊断主程序](diagnostic-main.rs)在独立非管理员进程中：

- 使用不存在的新目录、新测试 purpose keys 和新测试数据库；显式拒绝复用已有测试根目录。
- 原始初始化器创建明确的隔离测试状态，四个测试身份但只有一个 owner；三个 peer 端口由
  回环 socket 占用，不运行另外三个验证者，不建立真实四机共识，也不提交交易。
- 使用原 AOEM966 DLL，SHA256 `5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`，
  复制到私有诊断目录后显式加载；没有替换部署 DLL。另一本地 build 输出哈希不匹配，未采用。
- 原 `ResidentNode::start`、`RpcLifecycle::new/poll/status` 实际执行。唯一 bridge 是调用原
  recorder 的测试输入（[源码](diagnostic-module.rs)），没有替换 status 或锁实现。
- 独立线程通过原 work/recorder API 做 3 次有界更新；status 前 completed=0、后=3。
  owner OS TID=22392，producer TID=6816；七条 producer 断点观测中有一个重复入口，
  实际完成计数仍为 3，不能声称执行了第四次任务。
- 实际 JSON 序列化 13,453 字节，写新测试文件并回读相等；原 shutdown drain 完成。

运行 UTC `00:45:43.666016–00:45:54.359425`，目标 PID 12424（CDB 记录 `0x3088`），
最大允许 180 秒，实际未触发上限。目标和调试器退出 0；测试进程随后已退出。
新测试库/密钥只保留本机，不是生产创世、资产分配、原库迁移或正式收益。

[构建和 EXE/PDB 身份](diagnostic-build-public.json)：Rust 1.94.0，release 优化，node 包 debug=2；
新 PDB GUID `D48DE755-86BD-4779-8F1A-A880E15252EB` age1，仅配新 EXE。
它不是原 a254 PDB，不能用于原 PID37796 DMP。首轮私有诊断构建退出101：诊断导出属性与
forbid lint 冲突、测试索引类型不匹配；只修诊断代码后构建退出0，两轮日志均保留。

## 4. 参数记录及明确缺口

与原 a254 比较，[六组函数归一化机器码和 unwind](c-phase5-status-function-map.json)相符；
byte_hex 有两个同形函数，不任意选成唯一来源。只屏蔽地址重定位，不等同整个程序一致。

448 组完整 profile 格式化记录均满足同对象 receiver、同文本/长度、prefix 前后非易失 GPR、
尾调用或普通调用各自的栈变化，以及 writer 的 buffer+len 写入计算。448 组来自一次真实
status 内的事件格式化；当前 7 条 delivery 记录各含两个 32 字节值，总数与捕获范围相符。

其余 33 个采集序号不能算完整通过：

- 入口 `rpc::hex` 使用的 writer 位于新 EXE RVA `0x13ee20`，不是原映射 writer `0x588d80`。
  [原 PDB public symbol](alternate-writer-symbol.txt)证实也是 `String::write_str`，
  [反汇编](alternate-writer-disassembly.txt)显示不同优化函数体。采集器只在后者设了 writer 断点。
- 部分入口/调用观测重复或缺失；原始记录完整保留，不删除后拼成“全通过”，也不能
  仅凭重复断点文本认定程序执行了错误调用。未另跑一轮来掩盖这次覆盖缺口。
- 所有已观测 receiver 均未发现不一致，但未捕获的边界不能推定正确。

本轮进入 status 的实际并发窗口已记录；**delivery_contended=0**，未覆盖锁竞争失败分支，
也未证明 producer 与 hash 的某一指令真正同时执行。调试器会暂停线程、改变时序。
完整记录没有发现第一个错误调用边界，不能把正常样本当作原失败历史的替代。

## 5. 原故障交叉对照与下一项最小工作

- PID3228/37796 的原 DMP 保留故障时的 receiver/栈异常，但没有调用入口的执行历史；
  本次补出原 JSON 状态路径的实际调用栈，不补造原时刻的 task、创建/释放或寄存器事件。
- PID31296 的异常寄存器恢复尚未与此路径建立共同来源；不因同机就宣称同根。
- 原 875 与 a254 的选择源码一致；原 37796 AOEM966 与本次 DLL 哈希一致。新 EXE/链接布局、
  当前用户权限、短时新库、回环空负载与原 LocalService 数小时运行仍不同。
- 未覆盖原真实网络/中继状态、accepted compute/I/O backlog、取消/关闭与长寿命堆布局、
  status 的全部分支/全部 writer 体、实际失败瞬间的寄存器演变；不能排除其影响。

**下一项最小工作由 C 独立推进，不以 A 回执为前置**：先用原日志和源码确定原失败前
正在推进的具体网络/任务阶段，再只为该阶段加入隔离的真实任务/取消或积压上下文；
补齐必要实际 writer 体的边界记录，避免再循环证明当前已通过的空负载 status。
若仍没有对应原负载证据，则明确缺口，不能用随机增加并发次数代替原条件。
首次损坏未找到，当前没有可提交的最小生产修复或证明修复的回归方案。

本轮未修改生产代码、未访问原库/身份、未重新启动 C 部署节点、未运行 TTD，
未恢复观察器/双中继。Git 仅同步分析和隔离诊断资料，不授权 A/D 重跑或更新节点。
