# Windows同步UDP发送与回环归因：单机有效采集已完成

2026-11-02更新：[本次有效采集结论](QUALIFIED-RESULTS.md)与[结构化结果](RESULTS.json)
已离线收口。当前制品入口send_to调用链为全部目标线程CPU样本的23.69%–24.24%，
回环为15.76%–17.21%，两者包含重叠；两个调度窗口Running约98.5%。
五轮同字节65K及原账务/冷恢复通过，但这是ETW归因负载，不是新的性能提升或持续TPS签收。
没有修改发送策略/生产源码，没有重新录制或启动远端；四机仍待用户设置。

以下为本批采集前准备记录；其中“尚未采集/待采集”已由上述结果替代，其他设置边界仍有效。

固定源码检查点`486967c7`，原node `24de0d1e…`、测试器`647a058a…`、
AOEM `8779d014…`和各自匹配PDB见[candidate.json](candidate.json)。
不改任何生产Rust源码、发送策略、批大小、UDP配置、补片、AEAD、nonce、账户或共识。
**本轮尚未采集新的ETW，也没有新的TPS或内核CPU归因结果。**

## 1. 本轮核实的一条重要边界

微软[USO文档](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/udp-segmentation-offload-uso-)
明确把Loopback排除在NIC硬件UDP分段卸载的适用条件之外。
[WSASetUdpSendMessageSize文档](https://learn.microsoft.com/en-us/windows/win32/api/ws2tcpip/nf-ws2tcpip-wsasetudpsendmessagesize)
说明它设置UDP分段消息尺寸；设置成功本身不是某包已被NIC硬件卸载的证明。

因此，当前本机已消费Windows分段提交，**不等于回环已享受物理网卡USO**。
这支持把同机软件/回环路径与真实网卡路径分别测量；不是认定Loopback解释了全部
12K差距，更不是承诺四机会更快。旧ETW的IppLoopbackEnqueue约15%仍仅属于旧制品。

提供的ROS/eCAL链接本轮返回访问拒绝，未把其内容当作证据；博客园链接为网络基础介绍，
不是NOVOVM性能依据。未安装Npcap、替换驱动、关闭虚拟网卡或改变任何offload配置。

本机只读快照：Windows build26200；Intel I226-V有线网口Disconnected，
活动物理接口为Intel Wi-Fi 7 BE200，驱动24.20.2.1，报告链路866.7Mbps。
WLAN的UsoIPv4/IPv6驱动属性值均为1；这不是当前报文实际硬件卸载的运行证据。
报告链路速率不是测得吞吐，不以它推断127.0.0.1回环瓶颈。
私有无MAC/IP/SSID的完整只读快照保留于`target/native-windows-sync-send-20261102/platform-readonly.json`。

## 2. 复用原有有界采集，不新建录制框架

原[capture.ps1](../native-ingress-hot-thread-20261101/capture.ps1)仅增加可选
`-CandidateManifest`，默认历史调用不变。新清单提供六份当前image/PDB哈希，
输出到本批target目录，不再把新node配上已被后续构建替换的旧runtime PDB。
driver/AOEM符号从前次已核验的私有保存件复用，其image仍是同一哈希。

仍用原稀疏CPU栈与无栈CSwitch/ReadyThread profile，五轮同字节65K
CPU/GPU/CPU/GPU/CPU；每轮只录短窗8/8/5/5/8秒。
原node计时/IO/asset/critical诊断关闭，避免把五段计量开销混作CPU栈成本。
原四进程、1024付款账户、1024行/8MiB、8192在途、四方最终回执/账务/冷恢复不变。
这是单机归因负载，不是新的持续性能签收。

[ValidateOnly](validate-only.log)已验证六份制品/PDB哈希、WPRP XSD和脚本解析；
另有[错哈希拒绝反例](rejected-hash.log)，确认在录制/负载前拒绝混用制品。
没有启动负载或WPR。当前进程非管理员，内核采集未执行，不绕过权限也不自行提权。
现有零丢事件/零丢buffer、与真实drive交集、实际PID/TID线程名、符号覆盖及采样周期
质量门禁全部保留；未知或失败窗口不用于性能归因。

CPU栈将重新核对ws2_32/mswsock/afd/tcpip/Loopback以及原发送调用链，
不沿用旧符号百分比。调度窗口只回答Running/Ready/Waiting及接收线程调度关系；
无切换栈，不能把所有等待归给send_to或宣称已经取得逐次API等待调用栈。
旧`analyze-qualified.cjs`绑定历史PID和报告，不能直接拿来解释新capture。
收到新数据后按其真实进程、轮次和交易时窗分析，未解析样本保留未知。
所有系统ETL、全机CSV、PDB及符号缓存只留本机，不提交公开仓库。

管理员PowerShell执行：

```powershell
pwsh -NoProfile -File D:\WorksArea\SUPERVM\artifacts\audit\native-ingress-hot-thread-20261101\capture.ps1 -CandidateManifest D:\WorksArea\SUPERVM\artifacts\audit\native-windows-sync-send-20261102\candidate.json
```

完成后提供终端输出的`Capture directory`，无需上传系统trace。脚本不提升权限、不取消
其他采集、不改生产超时，不删除旧数据；需至少25GiB空闲且没有其他构建/压测并行。

## 3. 四机只准备边界，设置前必须先提示用户

用户2026-11-02确认目标是既有A/B/C/D，并再次明确：**现在仍是单机，进入真实四机
必须先提示设置。** 未连接、部署或启动远端；B邮箱/Relay/Web、C手机开发和D既有服务不动。

最新用户将安排各机Codex同步GitHub后回报，公网和局域网均存在。
统一使用[原多机交接顶部2026-11-02只读提示及模板](../../../docs/NOVOVM_MULTI_HOST_NUAS_HANDOFF.md#2026-11-02-当前交接四机物理网络测试只读准备)，
不另建清点/网络框架。A已用原inventory脚本完成本机只读清点；有线未连接、
W32Time未运行，未改设置。B/C实际系统与拓扑待各机回执，不套用下方旧快照。
各机只从GitHub取固定文档提交，不构建/部署/启动，也不互相探测；收到回执后再给精确设置步骤。

已保存的2026-10-25主机记录（不是本轮实测）为：
A Windows开发机；B Ubuntu、2vCPU/约3.5GiB；C Windows11/i9-14900HX；
D Ubuntu/Ryzen AI MAX+395。来源见
[B交接](../../../docs/VALIDATOR_B_ALIYUN_ACCESS_HANDOFF_2026-10-25.md)及
[多机交接](../../../docs/NOVOVM_MULTI_HOST_NUAS_HANDOFF.md)。实际OS、负载、端口和版本需要届时确认。
不同CPU/OS及B资源较少会混入A/B差异，不能把四机与单机的TPS差额全部归因于Loopback。
此外，当前`carrier.rs`的32包分段提交适配只在Windows私有socket启用；非Windows
`owned_udp_io`仍返回原UdpSocket，默认batch_limit=1。底层crate虽有Linux sendmmsg，
这里没有相应原生owner调用。B/D不能被假设具有与A相同的批发提交路径；本轮只登记
这个跨平台差异，不接线、不改变Linux或Windows发送策略。

现有源码可复用，但**尚无已完成的四机65K ML-DSA全验收器**：

- `service/client/operator.rs`支持远端固定原节点和当前AssetSpendBatchV3，但一次提交
  返回的是transport_delivery_only/finalized=false，不能用作最终交易TPS。
- `native_product_load/uca.rs`的PairedCorpus仍在内存，ProductNodes启动本地四进程、
  loopback地址；完整经济核对/冷恢复读取本地原库，并非换四个IP即能远程验收。
- 原Linux产品packager有当前验证者/传输身份和共同配置接线，应继续复用。
  旧overlay smoke只验收帧/路由；旧consensus stress/soak并非当前普通ML-DSA账户链，
  不能拿来替代四机最终回执验收。

真正进入四机前，先给用户以下清单并等待设置完成：

1. 固定共同源码、协议、AOEM能力和各平台制品哈希；Windows DLL不能代替Linux制品。
2. 每机独立用途密钥/journal与隔离数据目录，共同获准测试初始状态；不复制其他节点数据库。
3. 原NovoRUDP的真实端点、双向UDP可达性及防火墙配置；若走WSS/隧道，另列拓扑，
   不能再称纯物理UDP与Loopback的一因素对照。
4. 沿原SDK/负载观察器补外部节点薄适配和可复放公开签名语料；不导出私钥。
5. 四方最终回执及每机本地原库经济/冷恢复核对，以同一run/corpus/initial-state身份汇总。

先建立全CPU签验的可比组，保持ML-DSA-65不变；GPU-required只有各机能力已验证时
才可使用，不能在缺GPU的主机静默降CPU后仍称GPU组。是否采用后续混合硬件组另列条件。
不得预先启动A–D、激活正式创世、改变B服务或把链路可达当成四方最终性。

## 当前结论

当前image/PDB绑定、原有界采集及离线解析已完成，详见[有效结果](QUALIFIED-RESULTS.md)。
发送栈中的内核/回环工作已有当前制品证据；不代表完整12K差距归因或性能改善。
四机仍是**待设置、待接线、未执行**，开始前必须先提示用户。
没有新持续TPS签收，4K及12K均未签收。main、正式库、部署和现有数据库/journal保持不动。
