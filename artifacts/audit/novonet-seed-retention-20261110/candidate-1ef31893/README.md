# 1ef31893 固定候选与串行实网交付

## 2026-11-10 当前结果：B→A已更新，公网发现与原生对象交付已实测

用户本轮已批准B→A→C→D串行existing更新。Full运行
[42721507092](https://github.com/novovm/supervm/actions/runs/42721507092)已success，
headSHA为`5c6a6b48db817f7ef2f9dd6c759698a7083423c6`；与固定程序`1ef31893`
完整diff只有本候选的README/清单/证据ZIP与共享交接，没有程序、依赖、构建或CI变化。
这是旧分流规则下实际执行完的Full，不是`0ddf0ebf`轻量结果；未另开或重跑CI。
下方“待批准/尚未签收”为此前历史快照，由本节更新，不改写旧cancelled运行。

- B：PID2858350→2944965，原novovm UID996；80项制品、包内AOEM资格、
  原库existing、原身份/密钥、状态根、41050/42050监听通过，独立认证查询62→64ms。
  邮箱、Relay、nginx、Docker状态及PID未变。
- A：PID37496→17724，原LocalService；77项制品、实际加载AOEM、原库existing、
  原身份/密钥及状态根通过，独立认证查询42→29ms。同范围规则绑定新程序，
  防火墙三类配置不变。没有执行旧initialize任务，没有回滚、清库或换钥。
- 两端原状态根均为`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`。

发现的真实阶段链：A保留序号4166缓存；attempt1使用其私网坐标，8001ms时
carrier connect deadline。attempt2实际选择`configured_seed`，经
`https://euc1-1.relay.n0.iroh.link/`完成QUIC、流、bound认证、发布及NODE_GET，
1481ms完成。attempt3用新声明直连`47.103.59.206:42050`，198ms完成同一发现流程。
B同窗记录入站TLS/Stream Ready。双方attempt是各进程本地编号，不冒称线上的共同ID。
缓存后续序号4910，未清除或回退到seed12；原首次失败仍保留。

独立的原生NovoRUDP验证：A原owner仅发出一条固定非业务对象，实际到
`47.103.59.206:41050`，B观察对端`114.92.30.222:60735`。
Object ID `65a9412ab1973d2af85cda5401d1a06bbb66d97119c44a69b03eba21d4027372`，
Session ID `6f17d7154ef1199da11b37a5b70dbff4`。双端接收完整性、对象ID/session/nonce/
双方身份一致，发送端终态delivered及`probe_object_done`、接收端verified均成立。
单分片Transport ACK与Object DONE分别记录；A本机admitted→DONE为11ms，
不作跨机器单向延迟、TPS或业务Receipt声明。

原始结果：[update-result.json](update-result.json)；
[更新与双端原始证据](original-update-evidence.zip)，SHA256
`c2f964bf187fb8cf72f856132458e104bf5e9c157f665282699398f59e71b911`，
归档后逐成员重新读取核验。含固定更新器、CI差异、旧/新PID、实际库摘要和观察，
不含私钥、数据库或凭据。更新器本地守卫通过；不重复运行既有包内产品回归。

当前C/D未更新；下一项按[原工具交接](CD-UPDATE.md)先C后D。
四机自动发现、单/双中继、并发DONE、中继恢复和资产三票确认仍待实测。
本轮不将Iroh relay称为原生节点单/双中继，不把这条A→B成功扩大为四机通过。
节点沿原24小时有界运行配置，未改生产超时或签收永久服务运行。

## 历史：本地固定制品阶段

程序源码固定为 `1ef3189307382ef579775c7ce2b55a08d829d04a`。本目录所在后继
文档提交不是程序版本。原第33节的源码修复及失败证据不改写，本轮没有新增 Rust 修改。

原自动 [CI 42717131364](https://github.com/novovm/supervm/actions/runs/42717131364)
绑定上述源码，push触发、attempt=1。2026-11-10 07:30:25北京时间读取仍为
in_progress，conclusion为空；因此只交付本地固定制品，**完整候选门禁尚未签收**。
本轮没有手动触发、取消或重跑CI。快照保留在原始证据ZIP。

后继CI分流核对：该运行已于北京时间07:35:01结束，Rust作业为cancelled，整轮
结论cancelled，不能签收完整候选。原快照不改写；文档提交5c6a6b48的旧规则运行
42721507092当时仍在进行。新版同步/候选分流见 `docs/NOVOVM_CI_CHECK_TIERS.md`。

## 制品身份与原始证据

原打包器 `scripts/novovm-package-product-linux.ps1` 为两平台生成实际程序，保留
原角色合同、bundled SQLite、AOEM 和强制 manifest 校验；没有用旧 node/client
替代构建。包内 AOEM 与原68程序使用的合格库相同，不以库未变代替新程序验收。

本机归档目录：
`C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-1ef31893-20261110`。

- Windows：`novonet-observation-1ef31893-windows-x86_64.zip`
- Linux：`novonet-observation-1ef31893-linux-x86_64.tar.gz`

完整字节数、SHA256、release manifest、node/client/ctl、AOEM/其manifest摘要见
[固定清单](candidate-manifest.json)。Windows清单77项、Linux80项；各加
根 `CHECKSUMS.sha256`、`release-manifest.json` 和原AOEM嵌套校验表后，
归档分别80/83个文件。
逐项路径、大小、摘要核验；Windows ZIP逐成员解压后重算SHA256，Linux tar
与测试过的实际目录逐项比较，没有未登记文件或替换后的散装程序。

[原始包内证据](original-package-evidence.zip)包含构建、验证、测试、原CI快照及
复现包装脚本；归档后逐文件独立解压读取核验。没有运行节点私钥、凭据、DB或journal。
源码阶段的旧版失败及中间失败继续保留在父目录的 `original-evidence.zip`，不覆盖。

本轮独立归档检查首次退出1：原打包器按basename排除全部 `CHECKSUMS.sha256`，
因此两平台的AOEM嵌套校验表没有列入release manifest，而初版独立检查只预期根表。
已核对这两份继承表与原输入完全一致，Windows26行/Linux25行指向的实际文件哈希
全部相符。仅修正独立复核脚本，显式固定两份表的完整SHA并逐行验证，绝不放行任意
额外文件。清单单列 `inherited_aoem_checksum`；所有归档成员仍逐项核验。
原失败及修正依据保留在 `record-initial-members-failure.log`，这是终端失败的转录。
没有修改包、release manifest或程序，也没有为此重跑已通过的程序测试。

## 本地资格结果及边界

| 检查 | Windows | Linux | 实际执行对象 |
| --- | --- | --- | --- |
| 包内身份/NAT、同端口重连、错误库拒绝 | 7/7 | 7/7 | 固定包内ctl/NAT可执行程序及AOEM |
| 发现、旧seed备用、单/双中继、并发DONE、重新认证及错误链域拒绝 | 15/15 | 15/15 | 同一源码原network owner测试二进制，实际加载包内AOEM |
| 原生确认、少票/冲突、落后验证者原库恢复 | 6/6 | 6/6 | 固定包内node真实子进程、隔离测试状态 |
| UCA中断追赶、再花及冷恢复 | 1/1 | 1/1 | 固定包内node及相邻native-client |
| 原启动三项pin、冷重开与旧资产合同拒绝 | 1/1 | 1/1 | 原启动owner、包内AOEM、隔离测试状态 |

全部计数均检查实际执行，未将筛选零项记为通过。原失败/取消输出不删改；受控反例
产生的错误输出不自动等于该用例失败。没有改生产超时、连接/带宽/转发额度或票数。
Windows运行于本机，Linux运行于本机WSL Ubuntu-24.04；不是B主机或公网运行结果。
构建前后固定HEAD且工作树清洁；测试后再次验证实际制品及归档。

特别注意：15项发现/Carrier/DONE为原owner本地真实IO组件回归，**不是发布包node
进程的四机发现验收**。另外6+1项确实使用包内node进程，但不能借其恢复结果覆盖
Carrier公网建连。两平台严格Clippy及293/290源码定向组沿第33节同一源码记录引用，
本轮没有重复计数，也没有称其为并行全网络测试通过。

## 待批准的 B→A existing 更新与回滚

本版本尚未获准部署；未上传至B/C/D、停止任何运行节点、改防火墙或清理缓存。
下面以第32节68fa实际更新回执制定，**不是本轮现场运行状态核验**。获准后先读现状。

| 固定对象 | B | A |
| --- | --- | --- |
| 保留的旧程序 | 68fa7588 Linux | 68fa7588 Windows |
| 原运行账户 | novovm（实施时核对UID/GID） | LocalService / S-1-5-19 |
| 原DB | /var/lib/novovm/selfrun/91129655606029/B/chain-state | C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state |
| 原端口 | 41050 / 42050 | 41051 / 42051 |
| 新包 | novonet-observation-1ef31893-linux-x86_64 | novonet-observation-1ef31893-windows-x86_64 |

1. 本版本必要门禁成功且取得部署批准后，核对本清单和归档全成员；现场采集旧程序
   哈希、真实进程归属、账户、原库、用途公钥、状态根及当前日志。已退出与仍运行
   分别处理，不沿用旧PID。保留68程序、原配置、原日志及原discovery-cache。
2. 复用现有版本更新器，绑定本次固定输入；不能直接运行写死旧SHA的更新脚本。
   B独立程序目录暂存，原账户、受保护凭据、域与库路径不变。仅精确停止目标实例，
   确认进程/端口/库不再被它占用后，固定新程序 `START=existing` 启动。非正常退出
   如实按中断恢复记录，不假称正常排空，不删LOCK强开。
3. B核验实际程序/已加载AOEM、原库打开、原身份与状态根、fresh进度、监听与locator、
   独立客户端认证Status；记录其他服务状态，不以Running替代健康或对象交付。
   B成功后才更新A；任一安全检查失败停止后续主机动作。
4. A沿原LocalService/原库existing启动，**不重新运行含initialize的082旧任务**。
   若新程序路径需要防火墙绑定，使用原同范围规则，不扩大地址/端口，不关闭防火墙。
   原delivery观察开关按现有配置显式启用；新64事件Carrier观察仍有界非阻塞。
5. 不清序号4166缓存以制造成功。保留最新有效签名声明、固定身份/域与撤销检查。
   被动记录最新候选失败及原seed轮换、实际连接路径、bound认证、NODE_GET结果；
   同连接双端按实际端点/时窗关联，本地attempt数字不能冒充跨机连接ID。
6. 在批准的非业务探针范围内，记录唯一Object ID/Session ID、发送/接收、Transport
   ACK与最终Object DONE。候选入池、Carrier建立、bound认证、NODE_GET、对象DONE
   五项分开签收；未进入某阶段就注明未执行/未知，不称公网故障已关闭。

回滚：保全失败日志，先准确结束新实例，再用保留的68程序和旧配置 `existing`
打开**同一原库**。不恢复旧账本快照、不重建创世、不混用新旧journal，不更换任何钥。
回滚日志使用独立路径，先检查可用性，避免停节点后才发现日志重名。源码未改变
DB/cache格式；若真实打开结果显示不兼容，保全现场，不强用旧程序继续打开。
保留较新cache，旧68仍可能重现候选丢失故障；回滚不是网络修复或故障销账。

副作用范围：批准后B/A各有一次串行停机和原库重开；首次重新认证及发现可能出现
既有失败计数；新公开元数据观察有原64事件上限及丢弃计数。原生NovoRUDP优先、
Iroh备用、AOEM/账本/共识/资产规则和资源限制不变。C/D、NOVOCHAT不更新。
本轮没有新的公网、四机中继、多跳、资产最终性或TPS结论。
