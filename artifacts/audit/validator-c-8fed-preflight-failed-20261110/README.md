# C 8fed 一次更新预检失败，尚未切换

按 `219f9ace` 已确认的 B/A 健康交接执行。首次 UAC 被取消，未启动脚本；
用户要求再发起后，管理员 PowerShell 7 执行了一次固定测试及普通更新入口。
北京时间 2026-11-10 **18:00:52–18:00:56**。

- 固定工具测试退出码 **0**，实际包及全部定向测试通过。
- 更新器退出码 **1**：`Original process identity not confirmed`。
- 原更新器 SHA256：`1abe055135ac1469bac56b91d813844befc14173e8461edbf61e003e2f641a98`。
- 原 error.json：old_node_stopped=false、existing_start_attempted=false；没有停止旧节点或启动新版。
- 后续普通权限只读 CIM 查询未返回 novovm-node.exe，UDP 41052/42052 未返回监听。
  这不是完整的管理员内核退出证明；没有取得原任务退出码或崩溃事件，退出原因待核对。

本机保持失败现场，没有加 ResumeStopped/RecoverCrashed/RecoverMissingProgress 重试，
没有初始化、清库、换钥、校时或操作 D。C 不能回报 C_8FED_EXISTING_READY_FOR_A_QUERY。
请 A 核对此变化后明确只读退出取证与后续恢复入口，D 的串行切换条件仍未满足。

## 原件

[error.json](error.json)、[execution.json](execution.json)、[tests-output.txt](tests-output.txt)、
[update-output.txt](update-output.txt)、[只读后查](post-failure-readonly.json)、
[首次UAC取消](launch-cancelled.txt)、[SHA256SUMS.txt](SHA256SUMS.txt)。
原始输出保留 CRLF，未上传配置、数据库或受保护材料。没有手工触发 Full。
