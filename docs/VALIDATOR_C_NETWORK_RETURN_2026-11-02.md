# validator-C：用户授权公开的完整网络资料补交

## 23:24—23:25 校时重试通过（覆盖下方等待确认/未同步的当前状态）

C已读取main `2bed3974`，包含A与D各自成功对时回执。本次不重新清点机器，仅回读当前网卡/IP与系统时间服务。当前Realtek PCIe GbE ifIndex24仍Up/**1 Gbps**，IPv4 **192.168.71.125/24**（DHCP），网关192.168.71.1；时区China Standard Time（UTC+08）。

用户本人完成标准Windows管理员确认后，沿原W32Time执行`w32tm /config /update`（只重新加载原值）、`w32tm /resync /rediscover`，两命令exit0；未更换NTP源、改启动类型、重注册服务、手工设钟或改网络规则。

- **最后成功同步：2026-11-02 23:24:29 +08:00。**
- W32Time **Running/Manual**；实际源 **time.windows.com,0x9**，参考IP52.231.114.183。
- **Leap0、Stratum5、最后同步错误0**；23:24:29与23:25:15两次独立查询一致（间隔约46秒）。
- 报告源相位值`+0.0140598s`，根分散`7.7840859s`原样保留。这不是四机互相偏差、稳态精度或微秒性能证明，不以单次源相位值宣布全组时间门槛通过。
- 临时W32Time取时日志启用后已关闭；私有日志留在ignored target目录，不上传完整日志或用户标识。原时间源与启动类型保持不变。

保留失败过程：23:08初次收到有效数据并校正后服务停止；23:11重新启动后未锁源；23:14—23:15三次resync文本仍无可用时间数据，虽返回exit0也未算成功。之后重新加载原配置、重新发现时取得合格状态；这说明本次恢复有效，**尚不认定历史停止/取样失败的唯一根因已查明**。旧20:45等待UAC记录与原JSON字节/哈希不改写。

[成功结果与前序失败证据](evidence/validator-C-clock-sync-confirmed-2026-11-02.json)，Git/LF字节SHA256 `47eb43cb1bc5d9f5f1138fe2ab22be7f858fb7f49ea7771ba6dfc1d7c3afe68c`。状态：`C_OS_TIME_SYNC_CONFIRMED / GROUP_PRECISION_NOT_ACCEPTED`。A请接收本次提交及JSON哈希，更新共同配置；A、D的执行归属仍为各自回执，C没有远程代执行。没有启链、改防火墙/路由、操作密钥/创世/数据库或手机身份历史。


## 20:45 本机网卡/IP与校时执行增量（不重复整机清点）

交接：`C-ACD-OS-CLOCK-SYNC-20261102`。C已同步并阅读A的`3dfda6ce21cb6f9eb2fb2142309acaa482f7882e`，包含`831d13f3`最新校时授权、A实际同步结果及B网络增量。本机仍为validator-C，不是mobile-B。

- 当前接口：Realtek PCIe GbE Family Controller，ifIndex24，Up/**1 Gbps**。
- 当前IP：**`192.168.71.125/24`**，DHCP，网关`192.168.71.1`；本次未改变地址或路由。
- 本机时区：China Standard Time（UTC+08）。20:45回读W32Time **Running/Manual**，Leap3、Stratum0、Local CMOS Clock、上次成功同步未指定；verbose最后同步错误1（无可用时间数据）。仍是**未同步**，不把Running或query exit0当成功。
- 原配置为NTP、`time.windows.com,0x9`，NtpClient已启用。普通会话`w32tm /resync /rediscover`返回**0x80070005拒绝访问**；`/query /source`也被拒绝，但status允许读取。
- 已按用户授权通过标准Windows RunAs/UAC发起窄脚本：仅当W32Time未运行时启动，沿用原时间源resync，并读取结果；不重设时间源/启动类型/时区，不手工设钟。20:45尚无管理员脚本结果文件，记为**等待系统管理员确认或进程结果**，不宣称校时成功。
- 同一配置源的只读stripchart于20:44:19/21/24返回三次偏差 **+2.0888456 / +2.1003200 / +2.0914182秒**（52.231.114.183:123）。这证明该采样路径收到时间响应；不是W32Time已同步、不是各机相对偏差，也不能排除服务NTP通道与采样路径差异。

[本次有限JSON](evidence/validator-C-clock-sync-2026-11-02.json)，Git/LF字节SHA256 `13c5eab8c08005767f8aef7a4f401e5064f3af6d313ef410f6494cf2b39f8158`。未重复CPU/内存/磁盘或全量端口清点。原历史JSON及哈希保持不变；没有节点启动、密钥/创世/数据库/手机操作或防火墙/路由修改。

A已在最新上游记录20:38:50同步成功（Leap0/Stratum5）；C接收该结果，不能代签D。D按统一交接顶部同项本机指令并行执行即可，不需等待C或重交整机清单。C的管理员执行结果到达后，再补真实成功/失败及最后同步；当前Git回传不是A/D已经签收。


## 20:21 换线后本机只读增量（C-WIRED-REFRESH-20261102）

已同步并阅读A共同草案`fdd897c1`。本机实测当前有线Realtek PCIe GbE为Up/1 Gbps，ifIndex24；LAN已由旧候选`192.168.71.102`变为**`192.168.71.125/24`**，DHCP分配，网关`192.168.71.1`。本地对1.1.1.1的选路查询走该有线接口，未发送跨机探测；WLAN已断开。该地址是当前LAN候选，不是公网或永久静态地址。

当前W32Time为Running/Manual，查询exit0，但Leap3、Stratum0、source=Local CMOS Clock，上次成功同步未指定；仍未证明时间同步或实际偏差。状态相较旧快照变化原因未知，本轮只读，没有启动或修改时间服务。UDP41050当前无监听，仍未批准放行、预留或启链。没有改路由/防火墙/服务，没有操作手机、密钥、创世或数据库。

[最小增量JSON](evidence/validator-C-wired-refresh-2026-11-02.json)，Git/LF字节SHA256 `c5ad02efa6ffb24523b706569b13dad2e17e379fde482955aece1837b16937a4`。以下06:57原JSON、IP、网卡和时钟记录保留为历史，原字节和哈希不改写。A请使用新LAN地址继续共同配置核对；原旧地址ICMP超时不能充作当前C可达性结论。Git发布不是A已接收，也不代表认证UDP已通过。

交接编号：`C-NETWORK-20261102`。收件方：validator-A。日期：2026-11-02 Asia/Shanghai。

## 授权与状态

用户本轮明确回复“允许上传GITHUB”，允许把本次地址、主机名、端口和路由等网络元数据提交给A，覆盖统一模板对这些网络字段仅私下交付的限制。私钥、口令、令牌、完整运行配置、数据库和聊天数据仍不上传。此授权仅用于补交网络资料，不自动授权开端口、改路由、SSH连接A–D、部署或启链。

状态：`NETWORK_METADATA_PUBLISHED / PEER_CONNECTIVITY_NOT_TESTED / WAITING_FOR_A_TOPOLOGY`。发布不是A已读或四机已联网。

## 完整本机证据

[完整网络JSON](evidence/validator-C-network-2026-11-02.json)是本轮实测，未用旧报告覆盖当前状态。采样起点 `2026-11-01T22:57:57.7769726Z`，北京时间 `2026-11-02T06:57:57.7769726+08:00`；补充查询随后完成。W32Time未同步，时间是本机记录，不作跨机时钟偏差证明。

- 采样源码HEAD：`8443da93220e5462b7a2602b8da60280461339d6`；不是A批准的全组验收制品SHA。
- JSON SHA256：`07c54d31921ad8e91e2226c72fd4ae01ee6cb4947e3071e63698ca23fc23cc01`。
- 包含C实际hostname、OS、6个网卡、全部当前IPv4/IPv6地址、接口metric、57条活动路由、网关/DNS、52条TCP监听、70条UDP端点、PID/进程名、NAT/portproxy情况、时钟状态以及13条相关现行防火墙规则及其端口/来源/程序过滤器。
- 本机进程和端口为瞬时值，未终止任何工作负载。未上传无关软件完整防火墙清单或命令行参数。
- 原PowerShell批量端口/地址过滤器枚举返回拒绝访问；按现有NOVOVM/SUPERVM/Rendezvous规则逐项读取成功，均在JSON中。失败也保留，不能把未检查的规则覆盖记为PASS。

## A可直接使用的地址与端口清单

| 对象 | 地址/端口/路由 | 资格与用途 |
| --- | --- | --- |
| C物理LAN | `192.168.71.102/24`，WLAN，ifIndex20；AX211 Wi-Fi协商144.4Mbps | 本机实测，不是实际吞吐或外机可达证明；有线Realtek未连接 |
| C默认出口 | `0.0.0.0/0 -> 192.168.71.1`，ifIndex20，route metric0；接口metric另见JSON | 到B公网IP的本地Find-NetRoute选择同一网关和源地址；这是路由选择，不发包、不证明远端可达 |
| C原生UDP候选 | bind `0.0.0.0:41050`；LAN候选 `192.168.71.102:41050` | 采样时无UDP监听；未预留/批准，未测A/B/D方向；不把bind通配地址作为advertised端点 |
| C既有服务 | Rendezvous TCP443；ADB TCP5037；其余监听详见JSON | 原服务保留，不用443已有放行推断UDP41050已经放行 |
| C防火墙 | WLAN当前Public，三类配置均启用；`NOVOVM-Relay-WSS-TCP-443-v1`为TCP443允许，相关程序规则见JSON | 没有验证当前拟用node程序/41050在Public下的完整有效放行，不关闭防火墙 |
| C虚拟网段 | WSL `172.24.208.1/20`；VirtualBox `192.168.56.1/24`和`192.168.56.2/24`；完整IPv6见JSON | 不将WSL、Host-Only、loopback或IPv6 link-local自动公告为A–D跨机端点 |
| C公网/NAT | WAN地址、路由器UDP端口映射未知；Windows本地NAT/静态映射为空 | 本机路由表与Internet状态不证明路由器无NAT；未调用外部IP发现、UPnP或改网关 |
| B管理入口 | `47.103.59.206:22/TCP`，`ecs-user`；hostname `iZuf69ojo1hbc14c3cvgyqZ` | C在2026-10-25已核验，非本次重连；主机指纹与严格校验见下文 |
| B历史内网/UDP | `172.16.0.48`；UDP41050当时仅候选 | 最新监听、云安全组/精确来源及UDP可达性须A/B核对，不能靠SSH成功替代 |
| A与D | C没有两机本轮完整地址/端口回执 | 由A汇入两机新回传；不把历史LAN、TUN、overlay或示例IP填成当前可达地址 |

## B既有接入与保护边界

B主机Ed25519指纹：`SHA256:AKvetVMytykp2K/gMFFkANmTDk82R1iRNiXG+N0lXQE`。来源是[2026-10-25实际连接回执](VALIDATOR_B_ALIYUN_ACCESS_HANDOFF_2026-10-25.md)，历史SSH独立配置和主机公钥在C用户目录 `.ssh/validator-B-handoff/`。私钥不在本提交；用户如需手动转交，沿原回执位置和安全方式办理，不能使用GitHub认证钥替代云端认证。

B已有22/80/443、TURN、邮箱、Relay和KINGCLUB服务端口按原回执保留。A已在[11月2日B安装回执](VALIDATOR_A_B_DEPLOYMENT_RECEIPT_2026-11-02.md)确认隔离程序安装、未启链；本机不重新部署，不将10月25日旧“未安装程序”状态作为当前判断。

## 给validator-A的下一步与回执字段

1. 接收本提交与JSON SHA256，合并A/D本轮完整地址和B当前网络/安全组证据，固定LAN/公网/虚拟化拓扑。
2. 按各机真实占用明确bind、advertised和UDP端口；逐方向列出来源、目的、主机防火墙/路由器NAT/云安全组需要的差额。当前41050是候选，不是批准端口或已放行保证。
3. 按用户获准下一阶段执行原认证UDP双端往返，保留发/收两端证据；本次没有探测任何远端，所有必要方向均NOT TESTED。TCP/SSH成功、单机路由表和WSS/Relay不能冒充四机UDP直连。
4. 确认共同验收SHA、平台制品、链域、用途公钥和初始化配置，再沿原lifecycle进入获准启动/查询/恢复；本回传没有提供或伪造共同创世/密钥。

A请回传：`received_commit / evidence_sha256 / A_B_C_D_endpoint_sources / chosen_ports / required_rule_changes / first_blocker / next_authorized_phase`。未收到回执前，仅记录已发布。C继续保护NOVOCHAT/ADB、两台手机原身份历史和v108；不把网络资料补交扩展为节点部署或收费授权。
