# 1ef四机换包收口与A→C/D真实原生对象

## C 时钟证据已回传（2026-11-10 12:00:26 北京时间）

C 按 `01b1bbb3` 一次只读查询：W32Time 为 Stopped / Manual；三条 w32tm 查询
均返回 -2147023834（0x80070426，服务尚未启动），未取得时间源或配置值。
[原始输出、逐条退出码及UTC时间](../validator-c-clock-readonly-20261110/README.md)。
未校时、启动服务、重发或重启；当前状态不证明事件发生时状态或444ms根因。D 未操作。

## A 双端复核结论（覆盖下方待回传状态）

A 已读取 main@e966fee7 中 C/D 原始回传，核验回传摘要，并与原 A 发送快照逐项匹配
Object ID、Session ID、nonce、sender、recipient；两个方向五项均一致。

| 方向 | A 发送端 | 接收端原节点 | 本轮结论 |
| --- | --- | --- | --- |
| A→C | native_udp / Object DONE | probe_rejected_outside_validity | 对象已到达，但诊断验收失败 |
| A→D | native_udp / Object DONE | probe_received_integrity_verified | 双端对象事件匹配，接收侧短时传输明细缺失 |

C issued_at_ms=1794281524742、expires_at_ms=1794281534742，接收事件
wall_ms=1794281524298，比签发时间早444ms。A admitted_ms=1794281525118、
DONE_ms=1794281525131。源码 `consensus/transport/probe.rs::validate_time` 要求
`issued_at_ms <= wall_ms < expires_at_ms`；`native_lifecycle/probe.rs` 在接收端
执行该检查，传输 custody DONE 不等于诊断验收或 Business Receipt。
这些跨机时间戳说明本次时间依据不一致，不足以确定哪台时钟错误、当前偏差或历史校时原因；
不得把444ms直接登记为已经测出的NTP偏差，也不通过倒填签发时间或放宽有效期消除反例。

D 接收 wall_ms=1794281547245；issued_at_ms=1794281547111，A DONE_ms=1794281547189。
双方路径为 native_udp；不以不同主机的壁钟相减声称单向网络延迟。
[D原始报告](../validator-d-1ef-lan-receiver-20261110/README.md)。

两份接收报告的 transport 均为空。原 `novorudp/diagnostics.rs::State::expire` 会清理
first_seen 达120秒的传输记录，而本次发送后约7分钟才人工回读，已超过保留窗口。
旧采集器没有导出 expired_records，evicted_records=0 不证明没有到期清理。
此次采集时序不适合短时证据；不能用缺失记录否认D已有收件事件，也不能补造接收ACK/分片时间线。
保留原脚本和原回传，不覆写其摘要，不将两条均登记为完整交付验收通过。

### 当前动作：仅 C 只读核对时钟，D 无需操作

C Codex 在本机记录开始/结束UTC，并一次执行以下命令，逐条保留退出码及原始输出：

```powershell
[DateTimeOffset]::UtcNow.ToString('o')
w32tm /query /status /verbose
w32tm /query /source
w32tm /query /configuration
Get-Service W32Time | Select-Object Name,Status,StartType
[DateTimeOffset]::UtcNow.ToString('o')
```

这是查询，不执行 `/resync`、改时间服务配置或重启；失败照实返回，不循环重试。
A 同样已读时钟：当时 source=time.windows.com,0x9，stratum=5，最后成功同步显示
2026-11-10 08:58:25；这个状态本身不证明与C同步。C回传后核对两端依据，再决定是否需要校时。
下一次对象验证先让接收端有界只读观察器就绪，在120秒保留期内自动保存匹配证据，
然后A通过原owner发新的唯一对象；不重放旧对象，不扩大生产TTL或日志保留上限。

本轮未重发、未更新/重启节点、未改时钟/密钥/数据库/网络配置；不额外触发Full CI。

## C 已回传接收记录（2026-11-10 11:39:35 北京时间）

C 按 `7480930a` 只读采集一次，退出码 0。
同一 Object ID / Session ID 有 `probe_rejected_outside_validity`，native_udp，
来源 192.168.71.74:41051；wall_ms 比 issued_at_ms 早 444ms。
完整性通过事件数 0、匹配 transport 记录数 0，不能把 A 发送端 DONE 升格为接收验收通过。
[原始公开报告、执行结果和脚本摘要](../validator-c-1ef-lan-receiver-20261110/README.md)。
未重发、重启、更新或改时钟；请 A 对照发送端及时间依据定位，D 回传由 D 独立负责。

## 版本与查询结论（已完成）

A已核验D `6c49668f`的ZIP摘要、字节数和全部17成员：D以1ef原库existing运行，
PID2068 / LocalService，77项制品、实际AOEM、原用途材料及本地progress比较通过。
原stdout被运行进程锁定导致的只读观察失败保留，不改写成节点或更新失败。

A随后仅一次原SDK查询D `192.168.71.59:41053`成功，30ms；固定D身份、域、
genesis/protocol及状态根与D历史认证锚点匹配，height0/current_height1/state_version0，
recovering=false、projection_failed=false。根为
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`。
复用已合格3fa操作员和既有41190规则，未改任何节点/网络配置。
这项加上各机已回传的实机更新记录，完成A/B/C/D本轮1ef版本切换及逐机健康查询。
不是同时在线长期稳定性、四机共识或交易验收。

## 两次原owner对象发送（历史发送阶段，回传结论见顶部）

| 方向 | Object ID | Session ID | 实际发送路径 | 发送端结果 |
| --- | --- | --- | --- | --- |
| A→C | d10357b8fb9cf2f0b6ed94bd704f0f097a124c3c3b8186cfba3c03542725ece1 | de749f6a634be69f7f06ca21e9c6ea04 | native_udp / 192.168.71.125:41052 | Object DONE |
| A→D | b379c39bf958f5f05eef55b136b22a123f05f8d2ed2b5c0583f26c0d3628c2c7 | 147302e8176d0de12c49f4f0d9f9de3e | native_udp / 192.168.71.59:41053 | Object DONE |

A端分别在北京时间11:32:17、11:32:40收集发送后快照。每方向只调用一次原
`novovmctl native-network-probe prepare`，原运行节点读取命令并交付，未新开socket、
启动服务、发资产交易、改超时/资源限制或重启。旧命令经核验已DONE且过期，原字节
移入新的证据目录留存，再创建新命令；不覆盖未完成指令，不清旧计数。
双方新对象各有唯一nonce/Object ID；发送后execution_failures仍为0。

发送端DONE及UDP路径已观察到，**接收端完整性及同对象/session/身份/nonce/transport
tag仍待双端匹配**，不能把Status或ACK替代。A→B先前通过不重发。
单/双中继、并发、中继恢复、备用切回及资产业务三票确认仍未签收。

## 原只读回传指令（已执行，不要重复运行）

不再更新、不重发对象、不重启。同步包含本节的main，用管理员PowerShell7在本目录：

```powershell
# C填C，D填D；EvidenceRoot必须为本机一个新的不存在目录。
./Collect-Receiver.ps1 -HostId C -EvidenceRoot '<新目录>'
```

回传`receiver-public.json`、退出码及脚本SHA256。D把HostId改D。
该脚本只读指定原progress（最多4MiB）、固定进程/账户/程序哈希，筛选本次固定对象
和传输tag；不读取钥、DB或锁定的stdout，不发任何指令。无匹配也原样回传，不重跑
来掩盖遗漏或回收，不把脚本退出0等同对象验收通过。原始progress本机保留。
A拿到两端记录后复用原匹配规则核对ACK、接管DONE和实际路径，再推进中继。
脚本本地语法通过，接收端尚未执行；当前没有额外Full CI。

## 证据

`sender-and-D-query.zip`包含D认证原始stdout/stderr、执行记录与脚本，A两次原始
before/after、命令输出、二进制诊断命令、结果及发送脚本；无配置全文、私钥或DB。
摘要见`SHA256SUMS.txt`。D查询投影的`pre_shutdown_snapshot_available=true`只对应
D有旧progress，不证明有正常退出快照；`pre_post_continuity_certified=false`保留。
C丢失旧投影、全零原因未知的限制继续保留，不因为本次DONE观察而销账。
