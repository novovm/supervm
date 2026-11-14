# C：一次 WER 自然故障运行已崩溃，取得新 Full Dump

## 更新：受保护复制与同次故障离线核验完成

前一次复制提权返回取消，未执行。用户要求重新弹窗后，新入口于 UTC
`2026-11-14T12:25:53.2182440Z` 至 `12:25:54.4197381Z` 执行成功，退出码 **0**。
原件与副本 SHA256 一致，源文件前后稳定；源 ACL 未修改。新私有目录仅限当前用户、
SYSTEM 和 Administrators。DMP、完整调试器日志、原进度和事件 XML 均只留本机。

**这次得到新的直接故障证据，但未找到首次破坏指令，未修复。**

- CDB `!analyze -v`、`.exr -1`、`.ecxr` 确认异常 `0xC0000409`，参数 0 为 **2**，
  即 `FAST_FAIL_STACK_COOKIE_CHECK_FAILURE`；故障线程 **30596 / 0x7784**。
- 异常上下文位于 `ntdll!RtlpReAllocateHeapInternal+0x324`，RIP `0x7ffce31aa114`，
  是安全检查调用之后的返回位置，不是首次破坏发生的位置。
- 原始 Memory64 包含 **407** 段；全部 **20** 个列出线程都有上下文。
  本次核验的故障栈/上游帧、ntdll 序言/检查代码页、节点分配调用者代码页，及旧/返回
  分配候选地址各 728 字节均在文件中。仅证明这些指定范围可读，不泛称所有对象历史齐全。
- node/AOEM 的 RSDS GUID+age 与原 a254/AOEM966 一致；node 精确 PDB 缺少自身编译
  单元源码调试流的既有限制仍在。最近符号名及大偏移不当作精确源码归属。

### 同一次故障中：正确位置的 cookie 仍匹配，RBP 使用位置偏移 0x2A0

原转储代码字节直接确认函数序言为 `lea rbp,[rsp-0x50]`、`sub rsp,0x150`，
因此序言完成后应有 **RBP=RSP+0x100**，保护槽在 **RSP+0x130**。
返回检查为 `mov rcx,[rbp+0x30]; xor rcx,rsp; call _security_check_cookie`。

| 同一异常上下文/内存比较 | 结果 |
|---|---|
| RSP | `0x7739fede80` |
| 实际 RBP | `0x7739fee220` |
| 按序言及当前 RSP 推导的 RBP | `0x7739fedf80` |
| 实际 RBP 与预期之差 | `+0x2A0` |
| `[RSP+0x130] XOR RSP` 与该模块 cookie | 相等 |
| `[实际 RBP+0x30] XOR RSP` 与 cookie | 不相等 |
| 后一个计算值与异常上下文 RCX | 相等 |

这是从同一完整转储独立解析获得的比较，未拼接旧 DMP；公开结果不输出 cookie 值或任意堆内容。
函数 unwind 链记录了 `0x150` 栈分配及七个非易失寄存器 push，与序言相符。
**不能把本次简单解释成正确 cookie 槽已被覆盖；存在明确的帧基址关系异常。**
但最终上下文没有历史执行流，尚不能证明 RBP 在哪条指令改变、哪个被调用函数违反约定，
也不能据 ntdll 检出失败就归因 Windows、AOEM、Rust 或硬件。

### 调用链范围和下一处调查位置

调试器展开路径包含 `observe_tick` → `RpcLifecycle::status` → 中间收集/插入路径 →
`RawVecInner::finish_grow` → `RtlReAllocateHeap` → `RtlpReAllocateHeapInternal` 的返回检查。
中间若干项只有最近符号及大偏移，不能据此认定具体业务函数或对象。
这次实际被检测的是分配路径的帧异常，**不是已捕获 Formatter→writer 的首次参数错位**，
也不能宣称与三个历史 PID 同根。

下一项应利用本次完整栈和代码继续核对该堆扩容调用内的 RBP 保存/恢复、被调用者及返回位置，
找出有证据的破坏来源；不再重复普通格式化样例。当前不具备最小生产修复的证据。
没有启动/重启节点，没有改原库、身份、程序或诊断配置；双中继保持暂停。
Git 用于共享结果，不等待 A 审批；推送不等于 A 已签收。

复核材料：`collection-public.json`、`memory-coverage-public.json`、`analysis-public.json`。
`Inspect-NaturalDump.py <本地原DMP> <输出JSON>` 只读核验固定 SHA256；不输出原始内存。
完整 CDB 命令及日志在本机受保护目录，所有离线 CDB 执行退出码均为 0。
第一次 cookie 查询误用了双下划线符号名，CDB 报无法解析；后续按实际单下划线符号
重新读取，并由直接 Memory64 解析独立核验。该查询错误保留在原日志，不以进程退出 0
掩盖；公开比较采用后续成功读取结果。

## 终态：2026-11-14 17:13 北京时间

同一 PID34452 约运行 66 分钟后自然异常退出，owner 记录 UTC `09:13:47.4432268Z`，
实际退出码 **-1073740791 / 0xC0000409**，任务结果 **3221226505**。没有资源保护停止，
没有人为触发异常，没有重启。`0xC0000409` 不能仅按名称认定栈溢出、栈 cookie 或具体根因；
还需要读取异常参数和调用栈。

WER 生成 `novovm-node.exe.34452.dmp`，**233,403,217 字节**，flags **0x221826** 含 FullMemory，
两次读取 SHA256 稳定：`7d910fa6713e399c6bb394f2201fec622771dbc8e0665894ce9b8c115c9231aa`。
原始文件只在 LocalService 私有目录，未上传。**目标堆/代码/线程逐项覆盖仍待离线核验**，
不能只凭 FullMemory 标记宣称全部所需证据齐全。
最初终态提交时复制仍等待 Windows UAC；后续复制和分析结果以上方更新为准。

终态见 `node-exit.json`、`task-result.json`、`terminal-liveness.json`、`result-public.json`、
`dump-inventory-public.json`。根因未解决，C 已退出，双中继仍暂停。
下文保留最初的运行计划和启动记录，不作为当前在线声明。

用户在 `75d8d5c3` 后明确批准受控启动一次 C 原库 existing，等待自然异常。
本轮不是双中继放行：**A/B/D 不应据此发送联调对象，不启动中继观察器。**

北京时间 **2026-11-14 16:07:24**，C 的原 a254 节点 PID **34452** 在 LocalService 下启动。
原配置、原库路径、原三个身份材料哈希与此前记录一致，未初始化或换钥。
实际加载 AOEM966 已核验。原数据库 27 个文件的启动前只读清单只保存在受保护私有目录；
节点恢复及后续正常运行可以按原执行规则写原库，不宣称运行期间数据库字节不变。

| 项目 | 固定值 |
|---|---|
| EXE SHA256 | 1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d |
| 实际 AOEM SHA256 | 5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52 |
| 配置 SHA256 | 5d6a0660b9c79b3c4ef1c92357456258fc5a8a4b653f755b4d07eaf960caf9ef |
| 原 owner SHA256 | 33c2330d8422f875a4b23304c0444ab7fbbc9fac088d6e1d236fd6a35611026c |
| 本次 owner SHA256 | eeeb5efce2e50169407602f810d456d250a273825a833fb3b8c86d0c392aaff0 |
| 运行时限 | 21,600 秒，正常上限北京时间 22:07:24 |
| 重启次数 | 0；此次只有一次节点启动 |

复用原 ExistingOwner，改动仅为本次独立工作/日志路径及原 24 小时运行上限改为 6 小时。
保持原 `RUST_BACKTRACE=full`，不插入 TTD/PageHeap/逐调用钩子，不改生产网络超时。
本机脚本每 15 秒读已有进度/进程状态，不调用网络状态接口、不发送测试对象。
原网络能力按原配置运行；“双中继暂停”指本轮不开展测试，不声称改写了协议的转发能力。

## 启动前条件与空间预算

- 无已有节点、C 监听或旧 Watch-Relay 观察器；没有待执行 probe 命令文件。
- WER 专属 DumpType=2、DumpCount=2、目录与上轮资格一致，目录当时无转储，ACL 继承正常。
- AeDebug 未启用 Auto，target IFEO 不存在；未更改这些设置。
- 第一份入口在 **32 GiB 空间门槛**停止，`node_start_requested=false`，没有启动节点或修改原库。
  这不是节点启动失败，旧错误保留于 `preflight-space-stop.json`。
- C 盘实际约 19.4 GiB 可用。第二份入口改为两份各 8 GiB 转储预算加 2 GiB 余量（18 GiB），
  实际启动前 20,866,244,608 字节可用。预算不是进程全部可访问映射大小的数学上界。
- 每轮读取实际 PrivateMemorySize64 和磁盘空间。若 private 超 6 GiB，或空间不足以支撑
  两份“private+1 GiB”估算加 2 GiB 余量，精确核对 PID/路径/创建时间后进行资源保护停止。
  这种停止会单列为人工资源保护，**绝不算自然故障或根因复现**。没有自动重试。
- 节点应在 6 小时按原 RUN_MS 正常退出；任务有额外 WER 完成余量，上限 7 小时。
  任务限时终止也不能算自然故障。

## 当前证据范围

首次快照：chain_id=91129655606029，height=1，head=null，recovery_in_progress=false，
execution_failures=0，stderr=0，新 DMP=0。该快照不是当前永远在线或修复证明。

本机连续证据：`D:\WEB3_AI\SUPERVM\target\c-wer-natural-20261114T080704Z`。
受保护日志/owner/原配置副本：LocalService Profile 下 `NOVOVM-WER-Natural-C-20261114T080720Z`。
WER 原件仍只保存至上轮 `NOVOVM-Full-C-20261114T074716Z` 私有目录。
公开 Git 仅发布经过审核的元数据，原始 DMP、密钥和数据库不发布。

出现自然异常后先核对退出码、同一 PID 的原始 DMP/完整性和精确模块，再离线分析；
不人为制造异常、不立即重启。观察结束未复现则如实记录未复现，不宣称已修复。
首次破坏指令未找到，根因未解决。C 独立推进，Git 不作为等待 A 批准的门槛。
