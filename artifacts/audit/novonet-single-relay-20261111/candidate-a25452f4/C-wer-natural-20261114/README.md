# C：一次 WER 自然故障运行已崩溃，取得新 Full Dump

## 终态：2026-11-14 17:13 北京时间

同一 PID34452 约运行 66 分钟后自然异常退出，owner 记录 UTC `09:13:47.4432268Z`，
实际退出码 **-1073740791 / 0xC0000409**，任务结果 **3221226505**。没有资源保护停止，
没有人为触发异常，没有重启。`0xC0000409` 不能仅按名称认定栈溢出、栈 cookie 或具体根因；
还需要读取异常参数和调用栈。

WER 生成 `novovm-node.exe.34452.dmp`，**233,403,217 字节**，flags **0x221826** 含 FullMemory，
两次读取 SHA256 稳定：`7d910fa6713e399c6bb394f2201fec622771dbc8e0665894ce9b8c115c9231aa`。
原始文件只在 LocalService 私有目录，未上传。**目标堆/代码/线程逐项覆盖仍待离线核验**，
不能只凭 FullMemory 标记宣称全部所需证据齐全。
已发起仅复制这份原件和对应日志的管理员只读入口；本次提交时复制仍等待 Windows UAC，
未把“发起”写成“复制或分析完成”。

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
