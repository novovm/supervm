# C_EXISTING_RECOVERY_READY_FOR_A_QUERY

按 main `9a3160fa` 的固定恢复工具，仅执行一次 `-HostId C -ResumeStopped -RecoverMissingProgress`。
管理员 PowerShell 7，2026-11-10 北京时间 **10:48:39–10:49:16**。
工具测试退出码 **0**，恢复更新器退出码 **0**。原 Windows 程序包复用，未重传或重编译。

## 实际恢复结果

- 程序源 `1ef3189307382ef579775c7ce2b55a08d829d04a`，PID **24376**，账户 `S-1-5-19`（LocalService）。
- node SHA256 `d419df614d6b804136eca82fd7427d84155241c82b7f74c3c00bf2fdb3ea6087`。
- 实际加载 AOEM SHA256 `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
- 原库 `C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state`，`START=existing`。
- `custody_unchanged=true`、`initialize_executed=false`、`keys_generated=false`。
- 首份有效进度：chain_id=91129655606029，current_height=1，head=null，round=289，ticks=1，execution_failures=0，recovery_in_progress=false。
- 实际 UDP 监听 `192.168.71.125:41052`、`192.168.71.125:42052`，对应 PID 24376。
- 新程序防火墙规则按原范围精确复制，旧规则保留，profiles 不变。
- 退出后只读复核同一 PID 仍在、两端口仍由其监听。保持运行供 A 查询，未发测试对象或资产交易，D 未操作。

## 必须保留的限制

`existing_recovery_opened=true` 仅证明新版按原库恢复入口成功运行。
`previous_state_available=false`、`progress_state_unchanged=null`、`update_completed=false`。
旧 progress 全零、stdout 空、exit 缺失的原件保留，未伪造升级前状态，未认定故障根因已解决。
本轮不是120秒观察或长期稳定性签收。请 A 使用原认证 SDK 查询 C 的身份、链域、
高度和状态根，结合历史锚点分别核对；历史锚点不等于遗失的停机前快照。

## 公开原件

[result.json](result.json)、[after.json](after.json)、[observation-loss.json](observation-loss.json)、
[old-exit.json](old-exit.json)、[firewall.json](firewall.json)、
[停止守卫](stopped-owner-preflight.json)、[执行退出码](execution-result.json)、
[测试输出](tests-output.txt)、[恢复输出](update-output.txt)、[SHA256](SHA256SUMS.txt)。

原件按原字节复制。未上传 before/config、数据库、journal、私钥或受保护用途材料。
没有手工触发完整 CI。
