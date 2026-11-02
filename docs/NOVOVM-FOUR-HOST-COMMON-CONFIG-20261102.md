# NOVOVM 四机共同配置草案

编号：`NOVOVM-FOUR-HOST-COMMON-CONFIG-20261102`。汇总负责人：validator-A。
状态：**草案已汇总，所列只读核对已执行；非可运行配置，未部署或启链。**
证据/源码核对基线：main `2623eb815505b20a75c08da40301a6896f8dd2ea`，
不是最终运行源码或制品冻结。这里不生成密钥、创世、资金或伪造缺失参数。
本文件是本轮唯一共同草案；各机回执保留为证据，不再各自设计另一份共同规则。

## 1. 回执接收与唯一角色

| 节点 | 当前物理角色/系统 | CPU与内存快照 | 接收依据 |
| --- | --- | --- | --- |
| A | 当前共享实现/集成主机；Windows 11家庭版26200，x86_64 | Ultra 9 275HX，24核/24逻辑CPU；可见63.43 GiB，可用43.06 GiB | 11月2日本机清点；本轮18:54北京时间只读刷新 |
| B | 阿里云Ubuntu 24.04.4，x86_64，云VM | 2 vCPU；3499 MiB总内存，约1.3 GiB可用为安装时快照；仍有原服务负载 | [既有访问清点](VALIDATOR_B_ALIYUN_ACCESS_HANDOFF_2026-10-25.md)及[本轮安装](../artifacts/audit/validator-b-program-install-20261102/README.md) |
| C | Windows LENOVO手机调试笔记本，x86_64 | i9-14900HX，24核/32逻辑CPU；31.73 GiB，可用3.94 GiB为早前清点快照 | [基础回执](VALIDATOR_C_READONLY_RETURN_2026-11-02.md)、[网络补交](VALIDATOR_C_NETWORK_RETURN_2026-11-02.md) |
| D | Windows 11 Pro ASUS `SUPERVM-L2-01`，x86_64 | Ryzen 9 9950X，16核/32逻辑CPU；约64 GB，可用约14.8 GiB为08:54快照 | [当前D回执](VALIDATOR_D_NETWORK_RETURN_2026-11-02.md) |

A已接收C的`397dc5f6389970d54944e83b4f18cbb429a7ed94`和D的
`bfa239e7bb0098283acb390ad6ad590e3c00fd9d`，不再要求重复提交基础清点。
C网络JSON的Git字节SHA256为`07c54d31921ad8e91e2226c72fd4ae01ee6cb4947e3071e63698ca23fc23cc01`。
D网络JSON的Git/LF字节SHA256为`12f039e4a037d6e9d06eccdba64aa1c45cb8f6e87af6ad24227e9994860ee579`；
回执声明的`06d19284e52ca7154012a1e0ab0c1dd872f66c429caf4c0549e5cccd9e44114a`
已核对为同内容Windows/CRLF版本，不将两种字节形式误记为同一哈希。

本轮D仅指上述Windows机器；历史DGX Spark、Ryzen Linux及其包/密钥不作为本轮D。
不自动给旧机器分配新角色，不复制签票身份。mobile-A/mobile-B是手机标签，不是验证机A/B。

## 2. 链路与时钟：最新用户确认优先，保留历史采样

2026-11-02用户最新确认：**C和D均已恢复1000 Mbps（1 Gbps）**。
该确认取代下方旧回执的144.4/100 Mbps作为当前链路状态；原JSON和哈希不改写。
C已在20:21由本机回读有线接口和变化的LAN地址，见[C最小增量](VALIDATOR_C_NETWORK_RETURN_2026-11-02.md)；D当前接口仍待回读。接线前如再切换网卡，只需复核接口/IP变化，无需重交基础回执。链路恢复不自动证明时钟同步、UDP互通或实际吞吐。

| 节点 | 当前已知物理链路 | LAN/公网候选 | 时钟证据及待处理项 |
| --- | --- | --- | --- |
| A | BE200 Wi-Fi；20:04再次读取为866.7 Mbps，旧780 Mbps是历史快照；I226-V有线未连接 | LAN `192.168.71.74/24`，网关`192.168.71.1`；公网映射未知 | 20:04 W32Time为Stopped/Manual，status返回0x80070426；18:54 Running但未同步为历史，不推断停止原因或实际偏差 |
| B | 用户随后提供同一实例控制台截图：公网带宽10 Mbps（峰值）、按使用流量计费；内网规格另见本节核对 | 公网`47.103.59.206`；SSH再次核对内网`172.16.0.48` | 20:04/20:06 NTP synced=yes；chrony stratum3、Leap Normal，系统对源偏差约+22.426微秒，不是四机对时证明 |
| C | 本机20:21实读Realtek PCIe GbE有线Up/1 Gbps，ifIndex24；WLAN断开，旧AX211144.4 Mbps保留为历史 | 当前LAN `192.168.71.125/24`（DHCP），网关`192.168.71.1`；旧102不再是当前候选，公网/NAT映射未知 | 20:21 W32Time Running/Manual、Leap3/Stratum0、Local CMOS Clock，未同步、偏差未知；本轮未改服务 |
| D | 用户最新确认1000 Mbps；08:54的Marvell AQtion100 Mbps仅作历史 | 旧LAN候选`192.168.71.59/24`，实施前复核是否变化；公网/NAT映射未知 | 回执W32Time运行但Free-running、Leap=3/Stratum=0，尚无后续同步证明 |

这些值不是同时采样、不是独占可用带宽，也不是应用吞吐。Wi-Fi可能受共享空口、
信号、重试和双向竞争影响，不能直接按协商数字比较实际吞吐。
C/D原144.4/100 Mbps的低速状态不再列为当前待修阻断，也不把千兆确认写成已实测千兆吞吐。

拟分开记录两种实验条件，均保留B的公网云链路：

- 当前混合拓扑：A仍为Wi-Fi快照，C有线千兆及当前IP已实读，D千兆已由用户确认、当前接口待回读；
  如实标注VPN/NAT及各自负载，不把混合条件称全有线容量上限。
- 受控容量拓扑：条件允许时A接入有线；C/D在已有千兆条件下核对实际接口与稳定性，
  不再要求重复升速。B当前公网峰值10 Mbps，不能把该限制下的成绩当无限带宽容量；
  是否调高峰值及相应费用另行确认，不自行更改计费或采购配置。

不强制所有普通用户使用有线，也不靠强制网卡速率、关闭防火墙/VPN或扩大超时换成绩。
A/C/D时钟需沿原系统时间服务处理，B同时核对；记录源、最后同步和可测偏差，
不自创新的协议时差阈值。交易耗时用驱动单调时钟，跨机墙钟仅辅助对齐，不直接相减当微秒性能。

### 20:04—20:12实际核对（本轮未修配置）

- A本机读取接口/IP/时间服务；经原严格host-key SSH核对B，无新密钥或管理通道。
  B原邮箱、Relay、Nginx、Docker仍active且PID分别为944911/958/3967403/1414，未重启。
- B `chronyc tracking`最后参考时间为UTC11:59:52，Leap Normal、Stratum3；
  `ethtool eth0`只返回Link detected=yes，Speed/duplex unknown，不能据虚拟网卡推断公网千兆。
- B白名单元数据查询只读取实例规格和网络字段：型号`ecs.u1-c1m2.large`，
  `instance/max-netbw-egress=1024000`，`public-ipv4`查询404。没有读取RAM凭证、user-data或口令。
  [阿里云字段定义](https://help.aliyun.com/en/ecs/user-guide/view-instance-metadata/)说明这个值是
  **实例规格最大内网出带宽，单位Kbit/s**，不是公网购买值；404也不推翻已核验的公网SSH地址。
- 服务器无aliyun CLI；浏览器访问工具返回native pipe拒绝访问/browser access host unavailable，
  没有读取到控制台页面。用户随后发来的已登录同一实例截图直接显示**公网10 Mbps（峰值）、按流量计费**，
  此项现已确认，不再请求用户重复提供，也不声称A已实际操作控制台。
  [阿里云带宽说明](https://www.alibabacloud.com/help/en/ecs/user-guide/network-bandwidth/)区分公网购买的出带宽与内网带宽。
  入方向限制/实际吞吐仍不能由这两项值直接推定。
- A对D回执地址`192.168.71.59`发送2次ICMP，2次回应，3—5 ms；
  对C旧地址`192.168.71.102`发送2次均超时，随后邻居状态Unreachable。
  这不证明C掉线或防火墙一定拦截，尤其C可能在切换网卡后换了IP；D结果也仅证明该地址ICMP往返。
- 这些结果只约束接下来的真实四机实验；**B公网10 Mbps不能解释此前单机四进程的2.6K成绩**。
  无带宽升级、时间同步、规则修改、监听器/节点启动或吞吐压测。

## 3. 路由、端口与规则草案

拟用原NovoRUDP UDP41050，**仅为候选，未批准/预留/放行**。各机最终`peers`可因所在网络而用不同地址；
不能把B的视角也填成三台家庭LAN地址。

| 节点 | 拟bind | LAN视角候选advertised | 从公网/B可达端点 |
| --- | --- | --- | --- |
| A | `0.0.0.0:41050` | `192.168.71.74:41050` | 未确定：需要实际公网/NAT或原承载路径证明 |
| B | `0.0.0.0:41050` | 不把云内网直接给家庭LAN使用 | `47.103.59.206:41050`，云安全组/主机策略未验 |
| C | `0.0.0.0:41050` | `192.168.71.125:41050`（20:21本机回读LAN；端口仍仅候选） | 未确定：需要实际公网/NAT或原承载路径证明 |
| D | `0.0.0.0:41050` | `192.168.71.59:41050` | 未确定：需要实际公网/NAT或原承载路径证明 |

A本轮本地路由查询：到C/D走WLAN、同网段on-link；到B公网走Meta，源`198.18.0.1`。
D回执到B也选Meta。路由选择不是实际发包或可达证明；TUN虚拟地址不能公告为公网端点。
A本轮41050未见UDP监听；B本轮`ss -lun`也未见41050监听，41000/43171/43173仍被原服务使用。
B `ufw status verbose`显示active、默认deny incoming，当前显式规则未列41050；
`iptables -S INPUT`为DROP并跳转UFW链。主机端尚无该候选端口的新入站放行，云安全组未核验。
C20:21本机回读41050仍无监听，D仍仅有旧监听快照；接线前复核，不再请求整份基础回执。不挪用B原服务端口。

配置实施前所需的最小差额：

- A/C/D：按最终node程序路径、实际网络profile、选定UDP端口和明确peer来源核对主机规则；
  C的TCP443放行不等于UDP41050，D现有宽泛规则不等于完整有效策略已验。本轮不删除旧规则。
- B：核对对应UDP端口的主机规则与阿里云安全组、实际NAT出口来源；不擅自放开全网。
- 家庭网关：确认公网可达条件与必要UDP映射；若是CGNAT，明确记录直接入站不可用，
  评估原有路径适配而不是假填WAN地址、另建UDP或静默走WSS/Relay。
- 获准接线后，逐一记录A↔B、A↔C、A↔D、B↔C、B↔D、C↔D的认证UDP双向结果，当前全部未验。

现有A SSH配置只有B，没有可供本轮使用的C/D远程管理入口。
因此A不能从ping推断两机当前网卡、时间服务或UDP监听。C/D的Codex只需在本机执行下列
只读增量并回传节点标签、时间和结果，不重做基础清点、不启用远程管理、不发私钥：

```powershell
Get-NetAdapter | Where-Object Status -eq Up | Select-Object Name,InterfaceDescription,ifIndex,LinkSpeed
Get-NetIPConfiguration | Where-Object IPv4DefaultGateway | Select-Object InterfaceAlias,@{n='IPv4';e={$_.IPv4Address.IPAddress}},@{n='Gateway';e={$_.IPv4DefaultGateway.NextHop}}
Get-Service W32Time | Select-Object Name,Status,StartType
w32tm /query /status
Get-NetUDPEndpoint | Where-Object LocalPort -eq 41050 | Select-Object LocalAddress,LocalPort,OwningProcess
```

原认证UDP探测需要两端原owner和合法身份。当前41050无监听、尚无共同用途身份，不能把
裸发包/超时当认证结果，不另写UDP echo、不生成旧算法探测钥来绕开原入口。

## 4. 制品与原入口

唯一运行链为`novovm-node(native_execution_tick) → novovm-exec → aoem-bindings → AOEM`，
网络仍是原NativeChannel/NativeLifecycle/NovoRUDP/APFL；不新增HTTP交易入口或另一套执行器。

| 平台/节点 | 已有制品身份 | 本轮可作的结论 |
| --- | --- | --- |
| B Linux | 源码`14333419ee495e5d216761d31827271a252d9df5`；原11程序；包SHA256 `7d4f4dc184300eb3e19081db8e5199bf32bc1d729fb6a81be0071bdbc339229a` | 已隔离安装，未启链；[完整清单及运行检查](../artifacts/audit/validator-b-program-install-20261102/release-manifest.json) |
| B AOEM核心 | SHA256 `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` | 实际统一语义CPU ML-DSA验签通过；不代表全链/所有sidecars已验 |
| A Windows node | 当前文件`target/runtime-rebuild/release/novovm-node.exe`，SHA256 `24de0d1e0b1f508741b4fdabd9d089609c9924bdb2cfdbce2e766cefeb6f2078` | 仅现存文件身份，尚未定为本轮共同制品 |
| A Windows client | 同目录`novovm-native-client.exe`，SHA256 `62e4fa50819e4281995a0b2ef755ad141e7f0e728132cbdb1d7d59bcc5f1f293` | 同上；不凭文件存在推断源SHA/运行资格 |
| A/C/D随包Windows AOEM | SHA256 `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`；A本轮复核、C/D各自回执 | 匹配随包manifest，不自动取得B新SO或其他Windows候选的资格 |
| C/D node/client | 回执未发现当前目标目录内的程序 | 待A固定同一源码后准备Windows制品，不要求各机重复实现 |

从B包源码到本次读取基线，`crates/ scripts/ Cargo.toml Cargo.lock`未见差异，
这不把文档提交重新标成B实际构建SHA，也不替代最终冻结与CI。
同一平台可共用经过核验的程序包；Windows DLL、Linux SO和不同平台exe不要求同SHA，
要求同一共享合同及各自明确来源/能力证据。D现在需要Windows x86_64包，不是ARM64或旧Linux包。

## 5. 共同规则与尚缺的实际输入

| 项目 | 固定原则/原owner | 当前值 |
| --- | --- | --- |
| 验证者集合 | 本轮固定A/B/C/D，沿原四节点等权3/4，不因D慢或B资源少改成2/3 | 标签已明确；公钥及正式集合承诺待汇总 |
| 验证者用途公钥 | 原`novovmctl native-identity`；ML-DSA-65 validator与独立transport用途分离 | 本轮未收到合格共同域公钥清单，待获准生成/核验，不用历史服务钥代填 |
| Chain ID/epoch/创世时间 | 新的共同`chain_id:u64`、`validator_epoch`、`timestamp_unix_ms`，不复用清空日志旧域 | PENDING_APPROVAL，不取示例71或当前采样时间充数 |
| 协议/初始状态/创世承诺 | 原`ResidentConfig`/`GenesisConfig`、`protocol_commitment()`及原初始化编译器 | 等原始清单齐后计算，不手填零根/假承诺 |
| NOV与账户 | 1亿NOV、9位小数、以后不增发/销毁；普通账户自身控制钥，UCA只关联身份；现行NUAS/APFL v3 | 获批分配、具体账户、控制/恢复配置、费用与初始化批准材料未齐 |
| 订阅经济 | 最新月订阅/池日结与整体80/20/0原则见[现行规格](NOVOVM-ECONOMICS/NOV-COMMUNICATION-SUBSCRIPTION-DAILY-SETTLEMENT-2026-11-02.md) | 不是创世分配、完整经济参数或收费激活批准，不把旧按量/80-10-10作为默认 |
| 容量/执行 | 最多1024行、8 MiB展开预算，保留APFL及独立签名，低负载不等满；原费用/nonce/持久发布 | 沿原实现；不为本次实验改协议预算或生产超时 |
| 资源与后端 | 同一业务`execution_backend`；验签`signature_backend`独立；各机workers/内存按原owner预算 | 本轮先拟全CPU基线，具体本机预算待资格检查；B不能照搬A的24核资源设置 |

公钥生成需在用户批准阶段由各机原custody完成，只回传公钥/用途/指纹与批准材料，
私钥和credential留本机。调用顺序复用原`native-genesis prepare`与
`native-genesis verify-approvals`，见[原身份/初始化入口](NOVOVM_PURPOSE_IDENTITY_PREPARATION.md)。
普通资产账户和身份权限不因同一UCA合并，收款无需对方在线签名，创世批准不等于准许任意开户扣款。

本草案不输出能误运行的`genesis:null`、空peer或fixture资金JSON。输入齐后才装配原
`native-node.json`、节点env、client配置和`unified-node.json`；共同创世/协议承诺相同，
每机的路径、端点、钥匙、DB/journal及资源设置分别绑定，不复制整份运行库/签票状态。

## 6. 实施、回滚与恢复顺序（本轮未执行）

1. A完成此草案与缺项表；用户确认拟用拓扑和下一阶段操作。当前不改网卡、时钟、规则或服务。
2. 用户已要求立即核对：A/B当前状态与所列ICMP已读测，C/D本机只读增量待回传；
   时钟/精确规则等实际变更另获批准后执行，再沿原认证路径验证所有必要方向。
   C/D已由用户确认千兆，不再按旧144.4/100 Mbps安排重复升速。
3. A冻结共同源码，核验Windows/Linux程序、AOEM及实际共享合同；各机只消费固定包。
   Windows统一SCM/task安装目前仍被原`novovmctl lifecycle`拒绝，不能用`sc create`假装已交付；
   需沿原平台适配收口。B既有邮箱/Relay/Web/Docker不迁移，不混用其服务单元。
4. 经批准才准备独立用途身份、完成资金/权限清单批准并计算唯一初始状态/创世，四机核对相同承诺。
5. 激活另行批准；拟B→A→C→D逐机按原owner启动，等四机均就绪才供给交易。
   这不改变法定门槛、不赋予B特殊共识权；常规重启保持原库`existing`，不自动`create`或重创世。
6. 先验真实转账、到账再消费、费用/余额/nonce/权限/回执，再做独立重启、落后追赶和追赶中强杀恢复。
   保留原库/journal/用途身份，不复制别机DB；2/4不得最终确认、3/4故障推进与四方收敛分别记录。
7. 未激活时只保留/撤销本次程序选择，不改账本；激活后先停止问题候选的原owner、保全DB与签票日志，
   首选相同合格版本开原库恢复。未证明跨版本兼容就暂停处理，不拿旧二进制强开新库、清库或倒退签票。

原实现位置：[配置](../crates/novovm-node/src/native_pipeline/service/config.rs)、
[初始化](../crates/novovm-node/src/native_pipeline/service/config/uca_genesis.rs)、
[原lifecycle](../crates/novovmctl/src/commands/lifecycle/unified.rs)、
[原Linux打包器](../scripts/novovm-package-product-linux.ps1)。不另建部署框架。

## 7. 真实四机性能验收出口

沿原负载器/SDK适配真实外部四节点的最终回执和恢复汇总；现有本地`ProductNodes`
及transport-delivery成功不等于外部四机最终确认，这项适配尚未交付，不能靠替换四个IP宣称可跑。
固定65536笔、1024付款账户×64笔、明确收款分布、1024行上限和8192持续在途；
同字节与持续供给分别计时，签名/准备时间分别报告，保持四节点全部最终回执终点及完整经济核对。
先全CPU，再对已经取得资格的GPU节点作明确覆盖范围对照；B没有GPU资格时仍显式CPU，
不把局部GPU组叫“四机全GPU”，不同时切换业务执行后端。保留负向/慢样本、TPS/P95/P99、
实际线上字节/包/补片、每节点CPU及链路条件。历史Wi-Fi/100M样本、当前C/D千兆的混合条件
及后续受控有线条件分别报告，不能改写旧样本的网络条件。

**当前结论：基础回执已齐，统一草案已形成；还缺网络实施/可达性、共同制品资格、
用途公钥与获批创世输入、Windows服务适配及外部四机负载闭环。不是新TPS、启链或正式发布签收。**

## C换线最小增量接收入口（20:21本机回传）

C本机已实读当前LAN为192.168.71.125/24、有线1 Gbps，替代旧102候选；对应地址/时钟/监听及原历史边界见[C网络回传](VALIDATOR_C_NETWORK_RETURN_2026-11-02.md)。这是C通过Git补交，A尚未重新验证该新地址的跨机可达性，不改变上述端口/共同配置/激活批准状态。
