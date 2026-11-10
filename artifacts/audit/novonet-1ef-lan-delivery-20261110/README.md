# 1ef四机换包收口与A→C/D真实原生对象

## 当前结论

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

## 两次原owner对象发送（接收端取证待回传）

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

## C/D现在各做一次只读回传

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
