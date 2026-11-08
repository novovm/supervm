# A/C原库更新与原生UDP恢复定位：一次工作包

状态：候选已构建核验；**2026-11-09用户已明确批准A/C更新及必要同范围规则**。
实施状态另据实际进程和结果记录，不以授权替代完成。本计划不重新申请已完成的
四机认证、不另建测试网络、不要求B/D操作；目标是取得当前A/C恢复停顿的实际状态，
随后只修首个有证据的断点。最终完成标准不是“又加了一份日志”。

## 固定输入

- 源码：`5181da733d59d998afa898bb9964db5deae9e72c`。
- 使用[已封存清单](candidate-20261109/candidate-manifest.json)的Windows ZIP；A/C同程序。
- 归档在A：`D:\WorksArea\SUPERVM\target\udp-recovery-candidate-20261109\novonet-probe-5181da73-windows-x86_64.zip`。
- C接收可继续用既有`E:\区块链四机通讯`，不是要求再次收集设备信息。
- 原链域91129655606029、原各机chain-state/journal、原LocalService和原用途材料保持。
- 回退程序是当前ddd89e8e；回退指旧程序以existing打开同一原库，不是恢复旧账本快照、
  重创世、删LOCK或恢复退役WSS。082旧初始化任务禁止运行。

## 获准后连续完成的操作

1. A先更新，健康后C更新；不同时停两端。各端停止前按实际PID、路径、LocalService
   SID、exe哈希、数据库和当前状态根核实归属。无法确认就保全并停止，不猜PID。
2. 新独立release及工作/logs目录，不覆盖ddd程序、旧日志、旧任务、配置、私钥或原库。
   从当前运行配置继承；只接合新release路径、新progress和空的受限probe命令路径。
   不复制旧命令到新入口，避免新进程重新消费旧测试命令。原locator/cache及用途路径
   沿用，不重新生成密钥、创世或余额，不复制运行中数据库冒充一致性备份。
3. 使用既有`scripts/windows/NovonetExistingProcess.ps1`精确进程句柄退出屏障。
   若需精确终止，只结束已核对的目标节点；按中断恢复记录，不能称正常排空。
   不并发打开同一库。原任务若仍有活动owner，先准确处理其目标动作，禁止重跑initialize。
4. 原LocalService owner直接`START=existing`运行新节点。节点运行期限、网络策略、连接/
   转发/字节预算、握手及生产超时均照旧，不为采集增加额度或强制走UDP。
5. 防火墙保持启用，不重复修改Allow。仅当原规则绑定旧程序路径而新路径确需准入时，
   须在本次授权范围内复制**完全相同地址/端口/方向/配置范围**的窄Allow到新程序路径；
   旧规则保留回退。不关闭防火墙、不改VPN/TUN、网关、端口映射或B安全组。
6. 每端核实实际node/AOEM模块哈希、原身份、原库恢复、原状态根及监听。原探针配置
   开启同一delivery_event观察器；没有新守护程序、socket或接收队列。B/D不更新。
7. 两端连续采集原progress的一次有界窗口（建议120秒，每5秒读取），保留起止与差值、
   原peer索引对应关系、运行进程和文件更新时间。失效/停更/竞争遗漏单独记录，不补零。
   默认先不发对象；原恢复调度本身应能产生观测。私有原日志留本机，只回传公开报告。
8. 如需确认新UDP会话，沿原已授权受限probe命令发**一条新Object ID**的固定非业务
   对象。必须记录实际路径；carrier成功不能计UDP成功。原ACK、Object DONE、业务回执
   和最终性继续分开。不重放旧命令、不伪造AEAD确认或新增心跳协议。

## 按首个断点处理，不再盲目抓包或改网络

恢复角色来自**传输**公钥派生peer_id，不是validator配置public_key。当前原身份派生
C `novovm-mldsa65:516fc66162a3caa1d0153da9c42b5bbae9eae1e8bdcfb097b70aaa4ee97dd9f2`
小于A `novovm-mldsa65:6144e3de68011031592a7f66422b077989e643b7281b8904384b6cf1c321d5b3`，
故C主动、A被动；A不主动发INIT本身不是故障。实机更新前仍要核对身份未变。

| 实际增量 | 首个应核对的边界 |
| --- | --- |
| C无request且progress仍推进 | 原Driver恢复调度、bound路径及next_udp_probe |
| C只有passive_role或pending_reconnect | 实际传输身份/角色、原pending生命周期；不改选举 |
| C有attempt_started但无成功发送 | retry_wait/deadline、预算延后、WouldBlock、send_failed及OS码 |
| C有datagram_sent，A无对应输入观察 | 仅证明UDP IO接受提交；继续区分内核/实际线上/原接收入口，不凭计数判丢包 |
| A有INIT/challenge/part或握手拒绝 | 记录仍未认证；沿原续传、签名、域、时效及AEAD核对 |
| awaiting_aead_confirmation持续 | 原pending等待实际AEAD；不以改状态或假反馈凑确认 |
| 两端udp_route_activated | 仅本地安装会话；再匹配实际UDP对象完整接管和发送端DONE |

计数为本地观察次数，不是去重后的包数或对端收件证明。原生路径切回需重新认证，
保持原稳定性策略及健康Iroh托底，不逐消息重选。证据不足继续记未定位，不先改策略。

## 失败与回退

原库打不开、身份/核心库不匹配、状态根异常或无法准确识别进程时停止后续端更新。
保全新旧日志；准确结束新实例，经原退出屏障后，用保留ddd89e8e及其旧配置
`existing`重开**同一原库**。不清库、不删除LOCK、不改签票日志、不更换钥、不重创世。
若无法证明库格式仍兼容，先保全并报告，不能盲目强开。B邮箱/Relay/Web/Docker不动。

本次版本替换以及必要同范围新路径规则已获一次明确批准，不重复申请。
沿[原进程屏障复用的固定更新脚本](../../../scripts/windows/Update-Novonet5181Existing.ps1)
实施；A先通过健康检查再通知C切换。C接收与执行见
[C一次性更新指令](C-UPDATE-5181da73.md)，不再人工协调一串短抓包窗口。

2026-11-09本次A的UAC请求被系统报告“操作已被用户取消”，助手未启动，不计更新。
不反复发起UAC；用户可在A管理员PowerShell运行：

```powershell
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -ExecutionPolicy RemoteSigned -File 'D:\WorksArea\SUPERVM\target\novonet-5181-update-20261109\Start-A.ps1'
```

该本地短入口仅调用本节固定更新脚本，不自提权、不调用初始化；结果保存在
`D:\WorksArea\SUPERVM\target\novonet-5181-update-20261109\A-run1`。
如果该目录已存在，先检查原结果，不自动覆盖或重复启动。
