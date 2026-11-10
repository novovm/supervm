# C 原生对象接收记录：有效期拒绝

按 main `7480930a` 原 `Collect-Receiver.ps1 -HostId C`，管理员 PowerShell 7，
仅执行一次。北京时间 2026-11-10 11:39:35，退出码 **0**。
脚本 SHA256：`33c0b2c780459980205f9d3c19155093ec4765175c1b77690a404870f32158f0`。

## 实际结果

- PID 24376 / LocalService / 1ef31893，chain_id=91129655606029，current_height=1，execution_failures=0，recovery_in_progress=false。
- Object ID：`d10357b8fb9cf2f0b6ed94bd704f0f097a124c3c3b8186cfba3c03542725ece1`。
- 记录中的 Session ID：`de749f6a634be69f7f06ca21e9c6ea04`，与 A 交接一致。
- 实际事件 **probe_rejected_outside_validity**，路径 native_udp，来源 `192.168.71.74:41051`。
- issued_at_ms=1794281524742，expires_at_ms=1794281534742，wall_ms=1794281524298。
  接收记录时间比签发时间早 **444 ms**；请 A 对照原发送记录和时间依据定位，不能仅由此断言时钟根因或放宽有效期。
- receiver_verified_observed=false，完整性通过事件数 0，匹配 transport 记录数 0。
- probe_evicted_events=0，transport_evicted_records=0，transport_omitted_records=0。

脚本成功不等于对象验收通过。A 发送端 DONE 与 C 本次有效期拒绝分别记录，
当前不能签收 A→C 接收端完整性通过或 Business Receipt。
采集较发送后快照约晚七分钟，transport 记录为空的原因本轮未确定，不补造 ACK/DONE 详情。
未重发、重启、更新或修改时钟及网络，D 未操作。

## 公开原件

[receiver-public.json](receiver-public.json)、[退出码与脚本摘要](execution.json)、
[完整输出](output.txt)、[SHA256SUMS.txt](SHA256SUMS.txt)。
原件按字节复制；没有上传配置、私钥、数据库或完整原 progress。旧证据保留。
