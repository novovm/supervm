# 原owner UDP恢复观察接线

2026-11-09最新：A/C已以5181da73及原库existing运行；C-run3和120秒观察签收。
A对更新后的C认证Status及状态根核验通过；**新A→C固定对象的原生UDP双端接收及
发送ACK/Object DONE已与C回执267eae81匹配完成**，不再要求C读取、重发或更新。
旧崩溃与历史恢复停顿根因不因此结案。实际证据和验收边界见本文末节。

基于bec85de4审计，在原network crate及原观察器内实现。完整源码diff随本提交保留，
调用链和新字段合同见共享交接第22节。不新增网络或身份，不更改发送/恢复/权限决策。

2026-11-09后继：已完成原owner逐跳观察接线、两平台各17项定向回归及固定883c6183
候选的独立离线核验，未更新现有节点。下一阶段真实中继仍待受控更新和明确路径实验；
不是重跑已签收A→C对象。见[本轮源码/制品与范围](relay-20261109/README.md)。

## 本地验证

- Windows网络库串行：1292 passed、0 failed、13 ignored，1305项被列入。
  这是接线阶段的全库结果；最后仅对枚举标签精确命名及测试断言/feature清理后，
  对最终源码重新运行下列5项受影响检查，不把它称为又跑了一次全库。
- Windows/Linux最终定向各5项：
  - udp_recovery_evidence_is_bounded_saturating_and_independent_of_objects
  - udp_recovery_observation_never_waits_for_an_exporter
  - recovery_observer_distinguishes_original_send_deferrals_errors_and_deadlines
  - udp_recovery_observer_tracks_timed_retry_without_replacing_healthy_carrier
  - configured_carrier_rejects_wrong_identity_streams_and_returns_to_original_udp
- Windows默认feature及Iroh lib/tests严格Clippy；Linux Iroh lib/tests严格Clippy。
- Windows/Linux novovm-node --features iroh-transport --lib检查。
- 格式检查及git diff --check。均只在本机/本机WSL执行，不是云端或物理四机验收。

完整本地日志目录：`target/udp-recovery-observation-validation-20261108`。
本次不手动发云端CI；main自动工作流照常，不使用skip、不关闭或取消既有检查。

## 开发过程失败，未隐去

1. 最初以recovery_宽筛选默认并行运行：21 passed、2 failed。
   两项bootstrap_recovery旧用例报`carrier endpoint limit: no permits available`，
   其中一项把它与预期的耗尽原因比较不符；本次新观察器3项在该次通过。
   后续以原1305项串行执行，包含这两项并通过。未提高production端点额度或删测试；
   该对照不将所有历史网络故障都归因于测试并发。
2. 中间严格Clippy指出新增测试忽略5个must_use的NativeLinkPoll：已改为逐次断言
   没有会话换代及完成对象，而不是allow或静默丢掉报告。
3. 默认feature严格Clippy发现仅载体feature使用的5个枚举分支未构造：按原
   iroh/tls feature条件编译，保留公开计数索引固定，不用allow关闭检查。

## 交付范围

源码和本地接线检查已交付。下节更新2026-11-09的固定制品状态；没有替换运行库、
启动/停止服务或修改A/B/C/D。实机原生恢复根因仍未定位，更没有新的DONE或TPS结论。
新观察需获准existing更新后生效，不能假称可热加载到当前ddd89e8e。

## 2026-11-09 固定候选完成，未更新节点

用户要求不再零碎推进。本轮沿原打包器一次完成Windows/Linux候选构建、包内实际库
资格核验、压缩包内容核验及原库更新/回退方案。程序源码固定为
`5181da733d59d998afa898bb9964db5deae9e72c`；后继交接提交不是新的程序源码身份。
这是诊断能力制品化，不是凭猜测修改恢复策略，更不宣称实机故障已经修复。

| 项目 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| 归档 | novonet-probe-5181da73-windows-x86_64.zip | novonet-probe-5181da73-linux-x86_64.tar.gz |
| 字节 | 150464532 | 359111230 |
| 归档SHA256 | 6f28ddee10711ab9a4f72b2820f28c93475f075ddceee805ef2f10412510e811 | 67ea573bdcb7ac8b90f4617ad6adeaf583da1515be399a481eeb08c609cd9f25 |
| release-manifest SHA256 | f381ccd4eed0a9176e3b91aded8f699f19bb64bd983307c44f1dbf8086162ab3 | 5fb039bb26c266afe53c68b1c31bbb961e155d683d3a71ecaaa937ae71374dfd |
| 实际node SHA256 | ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3 | 7306580b501ccbee539a12c2a36436d1d8a49b2f4f071919b5738a93dc534790 |
| 实际AOEM SHA256 | 1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50 | d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33 |
| 清单制品数 | 77 | 80 |

A本地归档目录：`D:\WorksArea\SUPERVM\target\udp-recovery-candidate-20261109`。
解包候选位于`artifacts/product-overlay/novonet-probe-5181da73-{windows,linux}-x86_64`。
压缩包不提交Git；公开哈希及实际核验原报告保存在
[candidate-20261109](candidate-20261109/candidate-manifest.json)，不含用途钥或数据库。

实际完成的离线检查：

- 原打包器要求构建前后工作区干净、源码SHA相同；未用脏工作区冒充固定源码。
- 两平台全部清单制品长度/哈希、CHECKSUMS及路径边界核验。
- 通过包内实际ctl装载包内AOEM，核对路径及SHA；scalar、统一语义、混合批验签KAT通过。
- 实际node `--service-contract`确认原native_execution_tick、existing及平台SCM支持边界。
- 实际ctl完成377字节二进制探针prepare/cancel；没有发包、启节点、解封或创建数据库。
- Windows ZIP逐文件流解压核对长度/哈希及成员数；Linux tar与原包逐成员比较通过。

Windows日志`build-windows.log`/`verify-windows.log`、Linux对应日志及构建/核验脚本
保留在上述target目录。Linux最初启动器因pwsh不在PATH未启动构建，随后明确调用
已有可信pwsh，完成构建；不把启动器缺PATH记成产品逻辑失败。两平台结果是本机及
本机WSL结果，不是云端CI、物理多机或新增稳定性验收。

只读核对既有自动CI：[42492089086](https://github.com/novovm/supervm/actions/runs/42492089086)
确实绑定5181da73完整SHA，push触发、run_attempt=1、completed/success；
[原始状态回执](candidate-20261109/ci-run.json)保留。没有手动重跑、额外dispatch、
取消或skip。该结果不替代尚未执行的A/C新包实机恢复和对象DONE验收。

后继一次性A/C更新、观察、触发和回退范围见
[原库更新与定位方案](UPDATE-PLAN-5181da73.md)。2026-11-09用户已明确批准此次
5181da73 A/C更新，不再重复申请；此前ddd的授权不冒充本次授权。
新增固定部署脚本复用原进程屏障、LocalService owner及原库existing，独立新目录，
回退为当前ddd程序而非082或账本回退。部署门禁7项、实际固定包门禁及原进程屏障5项
本地通过。首次部署门禁发现路径祖先的.NET DirectoryInfo无PSIsContainer扩展属性，
已在任何提权执行前改用真实DirectoryInfo类型判断；原失败没有归到网络或节点。
A更新前原认证SDK Status成功，height0、原根f6186d40…f56333，耗时20ms。
本次只请求A一次UAC，Start-Process最终返回“操作已被用户取消”；助手未启动，
没有A-run1实施目录，未执行节点更新。没有再次弹窗。可由用户在A管理员终端直接运行
`target/novonet-5181-update-20261109/Start-A.ps1`；这不是重新申请授权。
[C接收与更新指令](C-UPDATE-5181da73.md)允许先核验新ZIP；A健康后C才切换，
B/D及NOVOCHAT不动，不重做已完成对象验收。

## 2026-11-09 固定更新脚本的规则关联比较修正

用户在管理员终端执行A更新后报`Copied firewall filter scope differs`。
实际`A-run1/error.json`记录`old_node_stopped=false`；旧node PID24444仍在运行。
新release、work和新程序窄规则已准备，尚未进入停止旧节点/打开新节点步骤。
这是更新脚本门禁误判，不是节点恢复场景失败，也不据此重装、换包、改规则范围或回滚库。

只读比较原规则与已复制新规则的六类真实CIM过滤器，差异均仅为
`CreationClassName=MSFT|FW|FirewallRule|<关联规则名>`。新规则自然需要新的关联名，
旧比较把它与原规则逐字节相等，故错误拒绝。所有实际过滤范围字段一致，原始只读
[对照记录](firewall-fix-20261109.json)保留，未删除第一次失败及准备结果。

修正后先严格核对CIM类型及两边各自绑定的预期规则，再排除此关联标识比较权限字段；
不忽略端口、地址、协议、接口、用户或服务等任何实际范围。新回归13项包含同范围
更名通过、10种权限/语义变化拒绝、错误关联拒绝、错误CIM类型拒绝；六类真实已准备
过滤器也通过**同一个产品部署门禁**。原7项部署门禁、实际固定包及5项退出屏障通过。
所有检查本地完成，未手动发云端CI，未改变任何运行节点或现有规则。

本地`Start-A.ps1`现在指向新的`A-run2`证据目录并显式`ResumePrepared`：
会重新核验现有准备与当前原节点、原库和用途文件，复用已有规则，不再复制一条或
覆盖旧报告。用户可在原管理员终端执行同一条Start-A命令继续；本轮未重复弹UAC。
脚本修正不改变5181da73程序或ZIP，**不需要重新构建/复制候选包**；C需同步最新
更新脚本后再执行。A尚未完成更新，C仍等待A健康确认。

## 2026-11-09 A已完成、C先前崩溃回执已签收

A随后手动完成A-run2，固定node ff68c593…087f3、AOEM 1b764a36…ce50，
LocalService PID13964、原库existing、用途文件不变、41051/42051监听，22项恢复
观察字段可用。04:30与04:56两次原认证SDK Status分别27/29ms，height0、原根不变。
实际原始结果及查询保存于A本地`target/novonet-5181-update-20261109`；
[公开摘要](a-update-confirmed-20261109.json)不含配置或密钥。这是原库/程序健康检查，
不是UDP切回、四机最终性、Business Receipt或新TPS结论。

只读接收并fast-forward至C回执5cb5736e：新ZIP及77项已核验，但C-run1没有启动
新版。后继原Windows Application事件确认旧ddd PID23024在北京时间00:51:52崩溃，
早于本次更新，原任务结果0xC0000005；PID8672的路径为空、零线程/句柄并不证明
其已经退出。完整原回执见[原C报告](../validator-c-5181-update-preflight-20261109/README.md)。

现有固定更新脚本此前只接受活跃健康旧节点，不能处理该已崩溃owner。因此在同一
脚本增加显式RecoverCrashed：原任务/崩溃路径/时间/状态匹配，所有CIM残留PID经只读
Windows内核查询证明已退出，原端口无人监听，才由原owner existing恢复原库。
共享退出屏障继续等待两次“无执行中node”，终止对象仍记录，不将CIM行等同活跃线程。
内核查询只请求QUERY_LIMITED_INFORMATION及SYNCHRONIZE，无Terminate、调试或
读写内存权；访问失败或不确定即拒绝。启动前再次核对，不删除LOCK或强开原库。
不能借此忽略活跃进程身份、降低数据库恢复义务或启动第二个网络。

本地验证：原退出屏障5项仍过；新增进程状态8项（含真实隔离Windows进程运行/
退出及同句柄创建时间），PowerShell7与5.1各通过；12项崩溃owner正反例、实际C事件
通过同一证据门禁；原7项部署、13项规则及6类真实过滤器、实际固定包门禁通过。
核验日志`target/novonet-5181-update-20261109/crashed-owner-tests-final.log`。
这是A本机验证，**不是C残留PID已获内核退出资格**。本轮未停止任何现用节点；
C需按[后继一次性指令](C-UPDATE-5181da73.md#2026-11-09-c旧节点已在更新前崩溃使用异常恢复分支)
执行一次C-run2，原授权已覆盖，不再询问换包或重复清点。

进程终止证明的API边界核对依据微软
[Process termination](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process)
及[WaitForSingleObject](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)：
已终止PROCESS对象会signaled；不能只把STILL_ACTIVE数值、线程快照或过期CIM信息
当成退出证明。0xc0000005具体根因仍未定位，不把部署适配当崩溃修复。

## 2026-11-09 C-run2空XML字段失败已定位，仍未启动新版

已签收远端`49dffb0b`，[C-run2原报告](../validator-c-5181-crashed-recovery-run2-20261109/README.md)
及3237字节原字节ZIP保留。失败UTC21:02:01，old_node_stopped=false、
existing_start_attempted=false；没有新work/任务或120秒观察，不以此前已执行的
内核查询代码推定C已取得落盘的退出证明。ZIP实际SHA256与C的SHA256SUMS一致；
README在A被Git检出为CRLF，归一LF后也与C公布的README哈希一致，不把检出换行
差异当作原归档损坏。

直接原因：恢复脚本逐字段读取`$field.'#text'`，遇到真实Application事件中两个
空Package字段时，StrictMode Latest抛属性不存在。C对同一原XML的InnerText
只读对照通过。A采用该修正，提取同一个ConvertFrom-NovonetCrashEvent供原
事件读取流水线及定向测试复用。使用DOM节点和InnerText而非PowerShell空元素的
字符串适配，支持Windows事件命名空间；无关provider缺应用字段不冒充目标事件，
匹配事件仍要求原必要字段、程序/异常、任务及时间线，未关闭StrictMode或吞错。

本地用C已公开的完整原字段重建XML（不是谎称取得其完整原XML文件），保留两项
空字段，证明旧读取在严格模式失败、新读取正常得到PID23024/时间/Report ID。
8项XML回归还实际执行原Get-WinEvent→转换→排序表达式（仅事件源隔离），并覆盖
Windows命名空间、无关路径/异常/provider和必要PID为空。原7项部署、实际包、
13项规则、6类真实过滤器、12项崩溃owner、实际C事件、5+8项进程门禁通过。
[本地最终日志](crash-event-xml-local-tests.log)保留。开发中新夹具对空EventData使用
PS字符串AppendChild及对不存在的report_id取值两处测试错误也已修正，早期日志仍在
本机target；没有把它们称为运行节点失败，也未通过取消严格检查取得通过。

本次仅修部署脚本，不改5181da73程序或包，不需重传ZIP。C按原文末节新C-run3
入口执行一次，C-run1/C-run2及旧崩溃记录不覆盖；B/D和运行A不变，不额外手工发CI。
C现场内核退出、新节点启动、120秒观察及原UDP恢复仍未签收。

## 2026-11-09 C原库恢复签收，新A→C对象原生UDP发送及DONE

已fast-forward并逐项读取C回执`5e82ce20`，C原字节公开ZIP SHA256核对一致：
LocalService PID34844、固定node/AOEM、原chain-state existing、三份用途材料不变；
内核same-handle wait=0证明PID8672已退出，未杀残留PID。新41052/42052监听正常。
25次采样120.550797秒，read_errors=0、最大观测陈旧1.704092秒，ticks/round推进，
执行失败0。bound=0、udp_recovery为空；该事实不等于UDP故障或恢复成功。
完整C证据见[原C-run3报告](../validator-c-5181-existing-run3-20261109/README.md)。

A于北京时间07:08通过原认证SDK查询C，104ms，height0，状态根与原根一致。
随后按已批准“必要的一条新固定对象”范围，原管理员入口向A现有377字节命令槽
发布一次命令，实际发送仍是原NativeLifecycle/NativeChannel/NativeObjectLink。
未为对象检查创建独立发送socket/服务/身份，未停节点、改防火墙、重创世或改变原额度/超时。

| 字段 | 实际A发送端证据 |
| --- | --- |
| 固定程序 | 5181da733d59d998afa898bb9964db5deae9e72c，A PID13964 |
| Object ID | 6cdfebd7bf6555c6a60962d3f11a51863d1fe9246dd41fed50f2e5d363de4524 |
| Session ID | 624b3e7ba0878c772d60c1ade8ef9212 |
| stream | NVBFT001 |
| transfer tag | 9260805feef25d32b491140a426cddedc48fdf47bbb52d1e75449f6278a153a5 |
| 路径 | native_udp，192.168.71.125:41052 |
| 原owner事件 | probe_owned、probe_admitted、probe_object_done，均同Object/Session及路径 |
| ACK/传输明细 | acknowledged_best=1、1次发送、send_terminal_state=delivered、transport_done有原时刻 |

tag由原公开算法对Session、stream、Object计算，与唯一原发送明细一致，不把另两条
其他对象的收发明细混入。原命令prepare时刻1794179420722、owned/admitted
1794179420905、DONE1794179420909；4ms只是本次原owner到DONE差值，187ms是
命令prepare到DONE差值，不是UI延迟、持续TPS或分位数成绩。

A继续完成原120秒窗口：07:10:20.733263至07:12:20.764587，60个采样，58个完整；
第34/35条解析后的progress缺少novonet属性，被明确记录为read_ok=false和原错误，
未删除、补零或重跑。其余/最终原执行失败0；全局handshake拒绝2586、no_session19
前后不变，connection_failures2前后不变，未清计数。两条投影缺项的原因未定位，
不把它们当作C收包失败，也不假报观察全成功。

这次对象直接使用已有原生UDP会话，bound=0、udp_recovery为空，**不是Iroh切回
UDP恢复分支的实测**，更不证明旧停顿已找到根因。对象后原认证Status A/C分别
28/59ms，均height0和同一原状态根，未涉及交易、扣款、账务发布或最终性。

[发送端摘要](a-c-native-20261109/sender-summary.json)及
[原公开结果ZIP](a-c-native-20261109/original-a-public-results.zip)保留全部60条采样、
原命令/事件/结果和对象后两端SDK Status；ZIP40567字节，SHA256
`9ff0d1fd0ae9b95ba11b0066db1725dc7091633e3cd46595a4314edd366e9bdc`。
原本地执行脚本/结果在A `target/novonet-5181-update-20261109/A-C-object-5181`；
本次不手动启动CI，不用组件或localhost代替真实A/C。

截至94240da4的发送阶段，可登记C原库恢复成功及A发送端原生UDP ACK/DONE，当时仍待C只读导出
同Object/Session的probe_received_integrity_verified及实际路径，原投影若已淘汰
逐对象明细需注明，不重发。见[C当前只读指令](C-UPDATE-5181da73.md#c-run3已签收只读回传既有ac对象接收记录不再更新或重发)。
原0xc0000005、历史停顿、Iroh→UDP实机恢复、多跳、四机业务与TPS均未由本样本签收。

## 2026-11-09 指定A→C原生UDP对象双端匹配完成

已签收`267eae81`，[C原接收报告](../validator-c-5181-native-object-receive-20261109/README.md)
及4423字节原字节ZIP完整读取。ZIP SHA256为
`8f3442771d827199c9fa2f55fbf540bc9f07e1048aa2f1f9e4a7e52d6527eddc`，与C清单一致。
C原归档的receive-public.json及A工作区检出文件同为CRLF，原字节SHA256为
`a0f721b830268de0a381e41cc8fdc9994f4a9b48d4b9f7d28fa9436abab438fe`；
归一LF后为C清单的e38dca6a…716b5，两份JSON语义逐字段相同。
不混淆原ZIP完整性、文本检出换行与内容差异，也没有覆盖C原文件。

离线匹配直接读取A/C两份原ZIP，不仅采用双方README的结论：

| 检查 | 实际匹配结果 |
| --- | --- |
| Object | 6cdfebd7bf6555c6a60962d3f11a51863d1fe9246dd41fed50f2e5d363de4524 |
| Session | 624b3e7ba0878c772d60c1ade8ef9212 |
| nonce | 8acb3ecfbd46381387cb835977e135469a180689227f2d3bf8b9b943cc1c4e82 |
| 身份与域内对象绑定 | A原命令/owned/admitted/DONE与C原事件的sender、recipient、nonce、有效期一致；C authenticated peer为预期A |
| 发送路径 | native_udp，目标192.168.71.125:41052 |
| 接收路径 | native_udp，来源192.168.71.74:41051 |
| C实际接收事件 | 恰一条保留的probe_received_integrity_verified，PID34844/LocalService/固定node及41052监听匹配 |
| A ACK与DONE | 原tag对应记录acknowledged_best=1、delivered、transport_done及同一会话Object DONE |

C的matching_probe_events和exact_receive_events是**同一条事件的两种投影**，没有
计为两份独立收件。C当前逐对象tag明细和stage明细均已缺失：transport全局
expired_records=30、stage全局evicted=30221，不能把这些累计值全部归因本对象，
也不补造C逐片计数/ACK/DONE。C探针64项事件历史evicted_events=0，目标真实
完整性/接收者核验事件仍在，并足以与A的独立发送ACK/DONE匹配本条交付合同。
transfer tag按原算法由Session/stream/Object计算，与A实际发送明细匹配；C公开
tag字段仅是其筛选输入，不伪称C仍有该tag传输明细。

C事件wall_ms=1794179420829、A owned/admitted=1794179420905、DONE=1794179420909。
这些是各机本地时钟，未作跨机相减的单向延迟、时钟误差或端到端性能结论。
原绝对时刻全部保留，不为制造正延迟篡改C时间。

[双端原始字段匹配报告](a-c-native-20261109/two-ended-match.json)已生成，并再次
从同两份固定归档生成另一份本地结果，字节一致；这是离线核对，不是重新发对象。
匹配脚本/核对副本保留A本地`target/novonet-5181-update-20261109/Match-AC-5181.ps1`。

**本项结案：指定单条非业务对象，经原A/C owner的实际原生UDP传输，C完整性及
接收者身份核验、A Transport ACK/Object DONE双端记录匹配。** A/C部署和本条
验收不再要求C重装、重启、换钥、重复采集或重发。本次离线比对未操作B/D、NOVOCHAT，
未更换任何库或用途材料。
后继工作回到既定单中继/双中继和恢复场景，但不自动授权阻断直连、重启中继或更新B/D。
本项不是资产执行、Business Receipt、共识最终性、Iroh→UDP切回、多跳或TPS验收，
0xc0000005和历史停顿根因仍独立保留。没有为本次文档/离线核对手动发云端CI。
