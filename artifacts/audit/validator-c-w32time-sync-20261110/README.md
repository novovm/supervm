# C 原配置一次时间同步结果

用户明确批准启动 W32Time，核对现有配置后执行一次正常同步。
第一轮 UAC 被取消，未执行脚本；用户要求重新发起后，本轮管理员 PowerShell 7 完成。

UTC 开始 `2026-11-10T04:31:11.0849501+00:00`，结束
`2026-11-10T04:31:12.7696341+00:00`。单调计时1682ms。

- 同步前：服务 Stopped / Manual，三项 w32tm 查询均为 0x80070426。
- 启动前登记的原配置：Type=NTP，NtpServer=time.windows.com,0x9；启动后有效配置再次核对一致。
- Start-Service 成功，仅执行一次 `w32tm /resync`，退出码 **0**。
- 同步后：服务 Running / Manual；Leap=0（无警告），stratum=5，source=time.windows.com,0x9，last sync error=0。
- 最后成功同步：北京时间 **2026-11-10 12:31:12**。
- 状态原值：phase offset=0.8947738s，root delay=0.1191019s，root dispersion=8.6647694s。
  这些是本次 W32Time 报告值，不能当作 A/C 实测偏差、已消除的误差或旧444ms的根因证明。
- 原参数及 Start=3 前后一致，有效配置输出 SHA256 前后一致。

未更换时间源、手工设时、修改启动类型、重启 NOVOVM、改程序/数据库/密钥或重发对象；D 未操作。
本轮只证明一次原配置同步命令及紧随其后的状态通过，不证明长期持续同步或 A→C 对象复验通过。

## 原始证据

[execution.json](execution.json)包含每条命令UTC和退出码；原 stdout/stderr 保留字节，
w32tm 中文输出可用 CP936 查看。before/after 的 service、parameters、start JSON 保留原值。
[Run.ps1](Run.ps1)为本次执行入口，[SHA256SUMS.txt](SHA256SUMS.txt)覆盖执行产物。
[launch-cancelled.txt](launch-cancelled.txt)记录首次 UAC 取消，未重试已经执行的同步。
不手工触发完整 CI。
