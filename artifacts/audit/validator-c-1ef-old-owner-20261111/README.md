# C 原 1ef owner 一次只读退出取证

按 main `30a4684f` 原 `Inspect-C-OldOwner.ps1`，管理员 PowerShell 7 执行一次。
UTC 2026-11-11T02:17:52.5801764 至 02:17:55.9686664，退出码 **0**。
脚本 SHA256：`0df067c53388d4fe3383a40e0ea8e2807e0db4665dde9a4a71434889940d1ef3`。

## 实际结果

- 原 1ef 任务 Ready，SID=S-1-5-19，last result=3221225477 / C0000005；无自动重启或触发器。
- active_nodes、exited_nodes、listeners 均为空，采集 errors 及 event_query_errors 均为空。
- 原 progress 是有效 JSON（828312字节），最后写入 UTC 2026-11-10T04:25:20.9176535Z；
  chain_id=91129655606029，current_height=1，head=null，execution_failures=0，ticks=7770937，recovery_in_progress=false。
- exit.json 有效：exit_code=-1073741819（C0000005），start=existing，时间 UTC 2026-11-10T04:25:30.8015822Z。
- Application 1000 匹配原 1ef 程序路径，记录号545473，UTC 2026-11-10T04:25:26.1118322Z
  （北京时间11月10日12:25:26），异常c0000005，PID 0x5f38 = **24376**，与此前1ef启动PID一致。
  Report ID `2d1b4b88-a3f0-4799-842f-175d9acb20cd`。
- node.stdout 133字节，固定读取器判为 invalid_json_or_encoding，未导出原内容；stderr为空。

任务、exit及Application事件支持旧1ef进程发生异常退出，具体访问冲突根因未定位。
这是8fed更新尝试前的旧进程崩溃，不是8fed启动崩溃；不把最后progress视作完整退出快照。
后续由A核对并指定原恢复入口；本轮未调用更新器、启动进程、打开DB、读取钥或改变时钟。
D 未操作，旧失败及原记录保留。

## 原始公开文件

[C-old-owner-public.json](C-old-owner-public.json)、[执行结果和脚本摘要](execution.json)、
[输出](output.txt)、[SHA256SUMS.txt](SHA256SUMS.txt)。原字节/CRLF保留，无配置或私钥。
