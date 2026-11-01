# validator-C：用户授权公开的完整网络资料补交

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
