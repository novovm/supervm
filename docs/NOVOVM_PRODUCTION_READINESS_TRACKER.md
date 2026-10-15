# NOVOVM 生产部署目标与验收台账

本文件记录可核验的交付边界，不是发布授权或完成比例。
当前结论以最上方验收记录为准；后续章节保留历轮结果及当时的待完成项。

## 2026-10-16 七月恢复现场：真实原节点单RPC与原生UDP功能闭环

恢复线35baaa00→2856c185f710a52438894acd1610823bc9dd2e16，仍以4f45526d为祖先。
本节所在main提交仅保存结果；**根main尚未切换，恢复源码尚未推入远端活动产品**。
原49项草稿、AOEM仓/库和另一通讯会话的工作未改动。不是生产/TPS签收。

原`crates/novovm-node/src/bin/novovm-node.rs`现有启动函数新增显式
`NOVOVM_NODE_MODE=native-resident-udp-v1`装配，非另造二进制或节点。复用a67978b1
的genesis配置、异步RPC签名准入、nonce预留、有界内存池、回执/余额查询、冷恢复投影、
遗漏接收者补传及HTTP延迟应答；移植时去掉Relay配置/控制器依赖，装配已有
NativeLifecycle/CandidateBodies/RoundProgress/CandidatePipeline。消息和业务域未重标。
完整CPU与整数代数执行继续经node→exec→bindings→AOEM，不修改执行器/经济规则。

配置显式指定UDP监听与验证者地址/公钥、AOEM库、签者文件、创世及create/existing；
路径相对配置文件解析，无默认私钥或数据库重建。这里的验证者路由不是NOVONET
全局准入名单。RPC仅loopback；用户交易仍用原nov_sendRawTransaction等方法。
`received`明确非耐久、未执行、未finalized；不得与legacy执行许可同时启动。

原Transactions/APFL输入通过原生lane交付，接收方仍独立异步验签。每peer至多两项
出站输入任务并预留控制/正文容量，满peer由既有RPC遗漏提示补传；入站有界移交，
三lane分别保留待处理对象，输入不消费不阻塞控制完成。传输DONE不代表验签或落账。
查询新增实际完成的APFL行数/整数行数/完整计划数，诊断不进入业务或共识承诺。

Windows本机debug原二进制、四独立进程/四库、同一AOEM库验证：
- 仅向初始非提案节点提交；其余三个HTTP交易提交数为0（夹具调用记录，不是运行计数器）。
  六笔真实签名含一笔余额不足，费用扣减、失败后nonce推进、完整回执和余额四节点一致；
  四进程终止后用原库冷启动，恢复同链头/回执/余额，重放不重复执行，再完成两笔后继。
- 另一个真实进程用例先只启动2/4；两节点执行后仍无发布链头。pending原交易相同
  canonical hash的坏签名和同nonce不同值被拒绝；启动其余两个真实节点后继续确认。
  没有测试代签诚实票/预造QC，也没有清锁/清日志。此用例不是A/A/B组合故障验收。
- CPU两个进程用例最终4.89s通过；完整整数路径同一单入口/恢复用例9.50s通过，
  每节点实际整数行数≥6、计划数≥3、APFL执行行数≥6。不是GPU性能或物理设备新验收。
- 本轮完整共识47项通过76.86s，含原A/A/B/未决签票恢复；随后只补诊断观察字段，
  最终构建复跑上述进程用例。普通native_pipeline在相邻检查点337通过/38忽略，17.63s。
  最终真实库genesis创建/重开/拒绝重建用例0.13s通过；binary构建、diff检查通过。
- Clippy严格模式仍失败：含依赖时既有adapter 1项/network 13项；no-deps node 11项
  （旧tx_ingress、未接回后台资产和诊断枚举）。本片新增文件未出现lib lint报错，未关闭lint。
  初次测试编译缺移植的签名夹具、增加status字段触发宏递归上限，均已修正后复验。

最终EXE SHA256 `ccdecf882a75920e232422368a7f3749b268c01ba9c144c8ed069c4b52aa4bb0`；
AOEM DLL `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`。
本地原始报告位于`docs/temp/july-native-rebuild/target/native-product-rpc/`：
CPU `22368-1792080299094861600/report.json`、整数`21324-1792080327003571300/report.json`、
少数节点`minority-22368-1792080297491855400/report.json`，日志和数据库保留，未公开上传私有数据。

剩余明确缺口：新装配尚未激活后继提前执行/精确后态复用（RPC明确报告false）、
历史归档RequestDecision追赶及长期离线队列回收；永久离线peer仍可能填满控制保留队列。
不得因此退回Relay，亦不得把当前按高度装配当作性能最终结构。原有效资产仍待按完整依赖
接回。本轮没有65,536笔正式同条件TPS、Linux/多设备/完整业务证明或生产默认签收。
接回main必须保留无关NOVOCHAT通信成果及草稿，不用整个旧树覆盖它们；恢复工程主体
与高性能/发布验收分别记录，不再以所有后续目标都完成作为解释“已经切换”的替代品。

## 2026-10-15 七月恢复现场：节点自动精确正文请求与执行交接

恢复现场d412e38d→35baaa00，祖先仍为七月4f45526d；根main尚未切换，
本节推送不等于恢复源码已进入远端活动产品。迁入原node的CandidateBodies/
NativeLifecycle，复用58e3aade的keep_body/submit_executions/poll_executions
边界和父状态检查，连接已有NativeChannel、RoundProgress及CandidatePipeline；
没有新增执行器、共识安全状态机或Relay依赖，没有修改AOEM内核/动态库。

现在测试每高度只向一个本地提案节点提供APFL正文，其余节点根据已验证的提案/
决定提示自动请求精确body_id，经原生NovoRUDP收取原签名正文、独立异步鉴权与
执行、候选耐久后投票和发布。夹具不再代为向远端提交执行、登记结果或转发协议。
同一owner/父状态/业务域/原序字节校验保留；未请求正文拒绝，额度不足保留调用方
输入，失败写入或未分类owner错误锁存，不把传输DONE作为业务确认。
冷恢复后保留原始候选正文供请求方取得，不从余额/回执重新构造交易。

发送端按peer保留原准备字节至真实交付；过期重试相同对象，不产生新交易或票。
每peer每lane独立推进，BODY lane背压不阻止后续control lane尝试。
仍有限制：永久离线peer填满保留队列后可能反压后续广播，必须接回基于已决定
归档的回收/追平；当前缓存只保留当前与紧邻前高度，不代替历史归档服务。
新编排未激活Transactions/EarlyBody/RequestDecision等入口，不能宣称完整RPC/
后继重叠/长期离线恢复已经接完；原已迁入相关能力仍保留供下一步装配。

Windows真实AOEM全44项共识回归通过72.36s，含原A/A/B和未决签票恢复。
新增CPU/完整整数代数用例各经四独立UDP socket/四库、三高度六笔原交易完成，
第1高度故意延迟一个节点的应用poll，其他三个先发布，再自动补正文使其追平。
另有一个坏签名候选被拒绝一次；六笔中一个业务失败，冷恢复收款余额350、
付款nonce各3，h2两回执一成功一失败，归档QC有效。非四独立进程/RPC/TPS。
首次两用例因测试错误使用大端预期读取小端余额而失败，修正断言后两项18.16s
通过，最终全44项再过；没有修改金额规则或放宽确认条件。
普通native_pipeline回归302通过/36显式忽略，17.50s；原binary check通过。
严格Clippy仍有既有14项lib/18项lib-test问题；新增布尔表达式lint已修，未禁lint。
AOEM DLL SHA256仍为31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947。
main已快进保留b4abb6f5的通讯部署记录，49份既有草稿未动。下一缺口是原执行文件/
RPC装配及归档交接，之后真实四进程同条件负载；本轮无新增TPS或生产签收。

## 2026-10-15 热态视图部署与原Android双向消息

c973e34d节点release（tls-transport）构建3m07s，EXE SHA256
`b296bae4504823e36271f69c23b148e2d9c4174adb640c0c87fc3f190dab260a`。
通过原Windows用户会话任务安装至原位置，原DPAPI/自主策略/身份文件摘要
保持，原信箱数据库保留、serving状态确认；TLS监听仍未启用。NOVOCHAT
f219eaa APK e4ebb4969cdf同步原位安装两台原Android，各5项保护文件不变，
无ADB业务转发、无VPN/防火墙改动。首个GUI检查退出码读取失败在停止旧节点前
退出；隐藏Start-Process等待exit0后重试检查及正式更新成功。

四条普通输入框双向新消息均在两端可见并有持久设备回执，首Put至首回执
3974/2757/2293/2335ms。后两条136个只读时钟锚点得到Put至对端持久入库
区间1020–1125ms、1366–1471ms；不是UI绘制时间或P95。两端task1持续，
本批次没有新的断网/重启验收。L3/L4投递已并行：Wi-Fi→L3一次请求28ms，
本地权限/取流1ms；蜂窝→L3一次请求2288ms。蜂窝L3与两端L4均仍走Iroh
Relay，观测RTT约0.8–1.5秒，不能将其解释为逐消息重新认证或全部归为物理
网络延迟。更多分段见NOVOCHAT docs/implementation/AUTOMATIC_RECOVERY.md。

本片前的联合TLS Application用例在热视图修复后149.00s通过，原QUIC对照
150.03s通过（不同输入修订，不是公平载体测速）；前面三次40s突发失败保留。
本机TLS/跨载体成功不代表已交付公网TCP路径、低于1秒或摆脱第三方中继。
正式自主传输/低延迟目标继续未签收；不涉及金融/AOEM/七月重建资料。

## 2026-10-15 七月恢复现场：自主轮次推进、未决重启和迟到提案交接

恢复现场49de29d2→d412e38d，仍以4f45526d为祖先；根main没有切换。
本轮迁入原node内部的RoundProgress和冷恢复编排，来源为固定58e3aade的
drive_consensus/poll_journal/controller_recovery。规则、锁、签名、计时资格、
QC和原子发布仍由原novovm-consensus::round_bft实现；没有复制Relay controller，
没有创建另一个签者/链头/执行器。网络和AOEM任务分别推进，不等待传输ACK再做执行。

候选登记校验同一常驻owner、精确父状态和完整执行结果；有界额度不足不消耗调用方
候选。耐久ACK后才释放单槽outbox；消费者未接走时不会继续覆盖签名。未决恢复从
原journal依赖读取精确候选、核对document/value/policy、经原pipeline重新鉴权执行，
再释放原签名原字节；存储记录不是执行许可。body准备/哈希与小型控制poll分开。
轮次ACK前先保留已形成的旧轮决定QC，换高度/轮次时回收不再需要的有界候选。

新增实际原生UDP用例不再由夹具调用诚实journal.propose/observe_prevotes/
observe_decision或拼QC。CPU与完整AOEM整数代数路径均自主完成三高度、六笔交易、
独立四库发布和冷恢复；其中一笔业务失败，恢复后收款余额350、两个付款方nonce均3。
首提案耐久但尚未发送时关闭并重开同一AOEM库，原候选重验后重放完全相同提案；
候选未就绪时即使计时越过一小时也不能签票/发布。迟到候选接上真实决定，不制造额外票。

故障peer只用自己的密钥签A/B冲突提案；三个诚实节点实际形成A/A/B prevote、
三nil precommit，后继round1由三个诚实签者形成QC并完成同头发布/冷恢复。
重复测试曾在60秒内不收敛。强制延迟一个节点轮次poll、先送达round1提案的反例，
在修复前29.99秒因未在round1决定失败；确认原编排丢弃了早于本地换轮ACK的提案。
原生传输的已完成对象不会靠重复同正文再次交给应用，因此不能依赖重发掩盖此丢失。
现仅按既有future-span保留每个已认证提案者一份未来提案，在原journal确认相应轮次
后才晋升，未来提案本身不授予换轮/签票权限。越界提案拒绝且不冻结诚实owner。
未改5秒阶段计时、法定人数或60秒测试预算；强化反例修复后单独12.09秒通过。

最终Windows验证：真实AOEM的共识全42项通过，56.78秒；
普通native_pipeline回归302通过、34显式忽略（其中三项为本轮真实库用例）。
原novovm-node binary check通过；严格node lib/tests Clippy仍被未修改的tx_ingress
及未消费字段告警阻断，无新增文件的lint诊断，未禁lint签收。编译期间曾因另一个
仍在运行的测试占用EXE出现LNK1104；等待原句柄结束后重建，未终止/重启运行测试。

AOEM DLL仍显式加载根工程31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947，
未替换七月SDK历史副本，AOEM仓/财务/手机服务均未改。main快进保留c973e34d/b6f1e4e8等
通讯更新和49份既有草稿。恢复代码本地detached提交；本节推送只发布进度，不把根
main文件树或远端CI宣称为已经启用本代码。

仍缺原可执行节点/RPC生命周期装配、自动精确正文请求/归档追平及网络重传责任交接，
然后真实四进程、单入口65,536笔同条件最终确认TPS；本轮同进程socket用例和耗时
不替代这些验收。没有新增主链TPS、GPU性能或生产发布结论。继续同一七月主体，
不把本测试夹具变成产品控制器，不新增执行器/共识。

## 2026-10-15 存活检查中的候选读取共用原解析视图

输入b6f1e4e8。进一步追踪真实Application：endpoint_permit的live检查会调用
AdaptiveProfile.check→nodes_now→ServiceLeaseBook.bootstrap_candidates；
此前只优化continuous_peer仍会从这一路重新解码和验证公钥合法性。
本次将自主候选读取与check_bootstrap_route也接入原Entry完整记录匹配视图，
新候选仍只是拨号候选，不是对端在线或业务授权证明；租约/握手/撤销不变。

8项自主来源回归通过32.87s，4项真实节点/跨载体/换钥测试通过12.12s，
严格lib Clippy通过5.93s。热态2000次候选/路由读取采样957微秒，仅本地
debug状态读取；不能代替手机端消息耗时。日志artifacts/hot-candidate-*.log。
NOVOCHAT前一片原QUIC完整用例150.03s通过，TLS五条突发40s期限仍失败，
本片之后须重新执行；未部署手机，未将未通过的TLS功能签收。

## 2026-10-15 持续会话复用已验证声明的解析结果

输入493734ae。NOVOCHAT本机双客户端TLS完整用例首跑失败：五条突发消息在
40秒内未全部取得设备回执（实际41060ms）；auth/acquire/RPC均有明显等待，
失败记录保留在NOVOCHAT artifacts/mailbox-tls-application.log。
源码确认持续会话每次IO/看门狗检查都重新解析同一签名记录及验证公钥合法性。
TLS和QUIC检查都在同一ServiceLeaseBook锁内执行，双地址记录重复成本更多。

现于原book的Entry内复用解析结果，只要完整current记录任一字段不同即重建。
不跨owner缓存，不存盘；每次仍查当前条目、撤销、时钟、策略和持久化失败状态。
新拨号仍查原地址及有效期，旧签名/新声明仍经过完整校验和原CAS后才可进入book。
这不是跳过认证，也没有修改金融、共识或七月恢复现场。

4项真实节点测试通过（10.25s），覆盖地址迁移、双载体换钥、伪造更新、移除、
撤销与重开；2000次热态book检查本机debug采样472微秒，仅为本地状态检查，
不是消息延迟。49项来源/租约回归通过（102.33s），明确排除此前已知一秒
deadline计时断言，未将其标为修复；严格network lib Clippy通过（6.92s）。
证据：artifacts/tls-book-reuse-{node,lease,clippy}.log。Application完整用例
及手机部署在NOVOCHAT继续；本节不宣称300ms、跨网TLS可达或产品签收。

## 2026-10-15 TLS 并行通道容量与建连首错分类

输入69a0baa5。TLS监听原四连接默认保持，可由现有受保护owner显式设置1–64
的节点总连接额度；TLS与QUIC共用预算，声明签入实际额度，旧四连接URI保持
原编码。两台手机各三条TLS通道需要容纳六条连接及控制操作，不能直接套用
原QUIC四连接限制。存储、带宽、请求频率与手机L4额度不变。

四项真实节点测试通过11.11秒，增加六个同时TLS建连/选择入口并保留原QUIC
领取进展；额外五流尚未完成业务握手，不能算六个已认证用户吞吐验收。Windows
两项进程测试19.26秒通过，覆盖保护额度8、越界0/65拒绝及原重启密文领取。
TLS握手IO现在通过既有io_result分类；真实断开归载体IO，证书/协议错误仍
拒绝，owner自身返回IO错误也不包装为可重试载体故障。14项TLS回归30.99秒
及新增首错区分用例0.03秒通过；严格network lib Clippy5.99秒通过。
初次u16/u32容量比较编译错误已修正，原日志保留。证据在忽略artifacts/
tls-capacity-*.log、tls-handshake-classification.log、tls-lanes-final-clippy.log。

本次未部署或改写正式节点配置；手机Application的候选竞争及独立通道接线在
NOVOCHAT推进，双手机实测仍待完成。此前一秒计时回归失败不因本次通过消失。

## 2026-10-15 七月恢复现场：原生通道承载耐久共识与完整代数候选

恢复现场643ecf57→49de29d2，祖先仍4f45526d。根main尚未切换；本节不是
四个独立节点进程/RPC/性能签收。恢复代码已作本地detached提交，工作树干净。

原APFL socket中的重组/反馈机制提取到唯一NativeObjectReceiver/NativeObjectLink，
APFL适配不再保留生产副本。仍使用原握手、加密帧、缺片窗口、pacing和完成历史；
控制/候选正文/交易输入有独立有限额度，每peer发送额度不被离线peer借尽。
等待ACK不阻塞轮询；背压保留调用方句柄，TTL到期明确返回原对象，不伪造交付。
完成反馈仍只表示校验正文并交给有界消费者，不表示鉴权、业务成功或落账。

原node的NativeChannel消费既有round-bft二进制codec，消息域/签名/valid-round/QC
检查直接从固定58e3aade的channel提取，调用原consensus内核，不复制controller、
Relay worker或安全状态机。发送准备与组装校验同I/O轮询分开；跨owner句柄、错域、
错lane和坏签名拒绝，错误应用消息不关闭其它健康peer。此处是固定验证者开发接线，
不把验证者名单扩成NOVONET全网准入政策。

四socket测试暴露合法握手响应重传被报“no pending native offer”；新增反例先失败，
随后仅允许同一存活尝试已验证的完全相同响应，不重建会话/重置序号，伪造及过期仍拒绝。
早期提取遗留self引用、测试签名helper参数错误也已修正，不计初次失败为通过。

Windows本机最终验证：
- network novorudp：69通过；node native_pipeline：302通过、31显式忽略。
- 显式真实AOEM运行consensus全部39项通过，23.91秒；含CPU和完整整数代数两种
  四socket/四独立库/连续三高度用例，以及原签票确认丢失、锁恢复、归档损坏反例。
  APFL正文、真实耐久提案/prevote/precommit/决定经原生UDP；接收者独立鉴权执行。
  整数用例断言业务回调数0、重算数0、每候选一次完整计划；并核余额、失败消耗nonce、
  不同合法QC子集、发布前ACK边界、冷恢复、部分回滚及四类损坏拒绝。
  测试编排仍同进程；不是自主四进程活性、A/A/B或端到端TPS。
- 原node binary check通过，7项现有未消费/诊断告警。network严格Clippy仍为原
  eth_rlpx/overlay/transport的13项失败，未禁lint或修改无关业务取得绿灯。
- AOEM DLL SHA256仍31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947；
  AOEM仓/SDK、main的49份草稿、运行数据和手机信箱均未改。

剩余交付仍是将此接合放入原node自主生命周期：有界组装/收票、精确正文请求、
未决恢复、后继推进及真实RPC，然后四进程故障和65,536笔同条件最终确认对照。
不能把本测试夹具变成产品控制器或把其耗时换算TPS；不重造已保留的共识规则。
主目录基线切换与上述能力验收分别报告，不再将恢复目录存在称为根工程已切回。

## 2026-10-15 Windows 原 owner 接通受保护 TLS 配置及真实重启寄存

输入fdbc5519。原novovm-rendezvous-service增加tls-transport构建选项及
--set-self-owned-tls CONFIG LISTENER_JSON|off，复用原Owner文件锁、DPAPI
封装和写穿替换。仅地址设置进入policy；固定域分离HKDF从原保护载体材料
派生TLS用途密钥，原签名/存储/Iroh身份及历史不变。不另建许可证、密钥服务
或信箱。未编译载体、非法地址、运行中变更和relay-only冲突均拒绝，不暗改
隐私/路由。原run入口接同一可选监听及签名记录；启动绑定失败更新failed_closed，
第二写入者抢锁失败不覆盖运行状态。此轮为隔离真实进程验证，正式节点未重启。

两个新增进程用例通过18.00秒：实际EXE、当前Windows用户DPAPI、TCP/TLS及
SQLite；寄存后取得签名StoreReceipt，强制结束进程再重启，身份与用途公钥
不变，领取/Consume保留原对象、密文和TTL。停用监听不删密文库或身份；非法
设置不改保护文件，端口占用明确失败。原carrier-change进程回归11.00秒、
fresh-node回归8.68秒通过。入口及进程测试严格Clippy通过15.14秒，iroh-only
原入口检查通过1.24秒；旧Cargo重复bin源提示仍在。新增测试初次引用模块
错误已修正，初次失败保留，未通过扩大期限解决。旧一秒计时失败未重新签收。

证据：忽略artifacts/tls-windows-{process,process-verified,process-final,
clippy,carrier-regression,fresh-regression,iroh-only-check}.log。只有测试
临时目录被使用，原节点、两台手机、VPN及防火墙不变；无新手机延迟数据。
旧手机不识别双地址记录，部署前先接兼容解析和原Application通道选择。须
核对QUIC多流到TLS独立通道的总连接容量，不能把多个长通道塞入原限额后再
称并行可用。公网可达性、CGNAT、自动选路及手机性能仍待实测，不因TCP本机
通过宣称摆脱Iroh中继或300ms达标。入口用法见NOVOVM_RENDEZVOUS_ENTRY。

## 2026-10-15 原自主节点接通签名 TLS 地址与共同信箱监听

输入0ccf3dbd；提交前保留远端仅文档c8edc570。product_service_tls定义原签名
节点记录内的可选TLS地址/原始公钥；新连接要求新鲜且完全匹配的地址，健康
连接不因普通地址期限结束而断开，删除、换密钥及撤销仍拒绝。原加密历史、
序号和回滚规则复用，正常声明更新保留TLS，明确停用时移除。旧纯Iroh记录
继续有效；含新枚举的双地址记录需要升级客户端后才能发布，不能静默降级。

product_rendezvous_owned::run_self_owned_node_with_tls_v1在同一Node owner中
轮询原QUIC入口及借用的TcpListener；共用MailboxService、维护循环及总连接
额度，不新增调度器或信箱。TLS沿既有NVMBOX06、原始公钥TLS1.3、exporter绑定
及bound session处理请求。平台传入受保护的用途密钥；未把密钥写入地址记录。
已有可用TLS监听不再等待独立Iroh Relay注册；签名QUIC地址仍只是候选而非在线
证明。发现控制链仍需要Iroh，此轮不是纯TLS自主引导。

mailbox_tls_node四项真实本机测试通过（14.07秒）：签名/过期/撤销/回滚，原
节点TLS寄存→QUIC领取与确认，同一托管种子重启身份稳定，加密历史恢复后
保留/移除地址，以及可选Relay不可达时TCP信箱仍工作。原mailbox_multiplex
四项通过（122.13秒）；TLS14项通过（30.99秒）；directory六项通过。
联合feature严格lib Clippy通过6.08秒；原iroh-only节点binary检查通过8.95秒。

lease回归49通过1失败（114.67秒）：bootstrap_recovery_original_deadline_and_
permission_prevent_dials的一秒耗时断言失败，原样单独复测1.19秒仍失败；
没有放宽期限或将失败算通过，具体清理耗时仍待定位。本轮夹具初期类型、历史
附加顺序和非法Relay端口失败修正后再测，产品验证保持。证据在忽略artifacts/
tls-node-*.log、tls-route-*.log；不是全仓全绿、独立多节点或手机性能验收。

Windows受保护策略的TLS配置与Android Application选路尚未接通，本轮未安装
手机、重启正式节点或调整VPN。TCP监听不能自动突破CGNAT；当前约两秒旧链路
样本未改善。后续接原平台owner配置及客户端有界通道，必须如实验证实际可达性。

## 2026-10-15 七月恢复现场：原生 socket 与同 owner 耐久签票/归档适配

恢复现场fe89340b→本地643ecf57，祖先仍七月4f45526d；不是根main已切换。
NativeEndpoint持有原UDP socket/握手/加密会话，按有限quantum接收并公平推进握手；
NativeApflSocket复用原重组、缺片窗口与完成历史，仅组装owner取走正文后发DONE。
未腾出组装输出槽时不发完成；鉴权背压返回同一APFL结构，不能把传输DONE当入池或落账。
完整peer列表/顺序验证，陌生或超长数据报拒绝；UDP异步ICMP错误计数，不因缺席peer
关闭健康会话。此处仍为固定peer开发接线，不宣称完成公网发现、自动重连或全网服务。

从固定main@58e3aade直接复用consensus/statement、chain、journal适配及相应反例，
规则仍调用原novovm-consensus::round_bft，不复制安全内核。共识二进制消息/APFL正文
codec一并复用，仅将fragment依赖指回原novorudp；未迁入Relay worker或controller。
保留原签名字节、域标签、QC含义及原子metadata契约，不将旧日志重标。候选、签票日志、
决定outbox、归档及唯一链头使用同一CandidatePipeline的AOEM条件持久化owner。
这一接合已经可实际验证签票/发布恢复，不代表原可执行节点已经启用它。

Windows本机：network novorudp 67项通过；node native_pipeline 300通过、29显式忽略。
另带真实AOEM库运行consensus全部35项（包括5项原忽略的真库集成）全部通过：四独立库
先持久化再释放签名、三个连续高度、不同QC子集、丢失写确认、迟到QC/锁恢复、陈旧CAS、
归档损坏与部分回滚拒绝。共识消息由同进程夹具传递，不冒充四节点网络/进程崩溃验收。
真实完整代数候选改用NativeApflSocket接收，经济核对/冷恢复专项通过（最终7.06秒为
测试总时间，非TPS）；组装背压、丢失DONE及重复探测无二次交付另有回归。
原node binary检查通过，8项未消费/诊断告警；严格network Clippy仍有原13项失败。
AOEM DLL SHA256仍31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947。

仍缺：同一原节点进程中的原生双向共识/正文调度、连续生命周期及RPC接合，随后真实
四进程故障和65,536笔最终确认对照；没有新增TPS或生产签收。恢复源码仅本地提交，
本次main只同步可核验进度；root49份草稿、手机信箱、AOEM仓/SDK未改。已ff同步远端
0ccf3dbd通讯变更，未用此次主链测试替它作手机部署签收。目标保持active。

## 2026-10-15 同一 NVMBOX06 入口按实际载体分派

输入8c06b87a；提交前仅台账快进至58e3aade，保留另一现场七月恢复记录。
原节点serve分支改用serve_mailbox_prefetch_with_policy_v6：Iroh仍进入原有界
多流宿主；TLS进入原单流持续会话。request_mailbox_continuous_v6复用同一个
NVMBOX06选择报文，不新增协议、不降级旧版、不共享不同连接的认证结果。
链路为selector → carrier dispatcher → 原bound session → 原MailboxService。

本机真实TCP/TLS两项通过，39.79秒，包含刻意31秒无请求空闲。其一验证通知
等待不阻塞独立Put；另一项经实际选择入口寄存、错误读取能力拒绝、通知及
Consume，然后关闭TLS，以真实本机QUIC领取同一服务仍保管的另一个对象。
不重新注册信箱，原对象标识、密文、到期时间保持一致；QUIC原多流选择保留。
这证明组件共享存储与协议，不是两个独立运营节点或手机自动换路验收。

另外13项信箱回归通过（24.12秒）：重放、撤销、取消、能力、通知竞态、关闭
贡献与丢ACK恢复等。联合feature严格lib Clippy通过（6.13秒）；现有
novovm-rendezvous-service的iroh-only binary check通过（21.52秒）。Cargo
原多binary源文件提示保留。初次夹具编译借用、期限类型错误和未设置监听身份
导致的失败均保留在本地日志；修正夹具，未修改产品安全检查或超时。
一次错误测试筛选命中0项不计验收，之后使用实际模块名运行上述13项。

证据位于忽略artifacts/mailbox-carrier-{dispatch-verified,security-regression,
dispatch-clippy,node-check}.log。正式Windows监听与Android发现仍使用Iroh，
本次未换包、未重启节点、未新增公网入口或改变VPN。TCP/TLS不会自动解决
CGNAT入站；下一步需将可达载体地址、原可信节点身份与现有owner选路接通。
不以这两项本机测试宣称手机提速、300ms/1秒、完全自主或隐私路径已完成。

## 2026-10-15 七月恢复现场：完成确认丢失后重答

恢复现场a0755a0d→本地fe89340b，仍基于七月4f45526d。复用原重组器，显式启用
有界完成元数据；只有完整正文哈希校验通过并移交owner后才可重答传输DONE。
不保存另一份正文，不读取链账本；元数据受原全局/每peer消息数及TTL约束，重复
探测不续期。同peer新增完成先淘汰自身旧元数据，不清理未完成正文；元数据到期或
淘汰后允许再次交付，业务鉴权/nonce去重仍独立。旧大帧调用默认不启用这项行为。
原node的反馈现在来自此记录，而非测试夹具直接构造成功ACK。夹具实际丢失首个
加密DONE，等发送方尾探测后由接收端重答，确认没有第二次正文交付或残留重组额度。

本机：network NovoRUDP 65通过；node native_pipeline 269通过、24忽略；真实AOEM
完整代数候选/经济核对/冷恢复显式通过（5.02秒）；原node binary检查通过，13项
未消费接口告警保留。没有新增TPS、真实四节点发布或生产签收；库与AOEM仓未变。
本次未再跑严格Clippy，上一节原13项失败仍未关闭。

根目录切换尚未执行。整包差异核对确认：当前network中的bound_session和发现
控制等有效通讯能力仍直接引用duplex安全类型；原node后续模块也有共用依赖。
不能为去掉十月主链耦合而同时删除现用手机信箱/身份资产。此事实不授权继续在
十月controller开发，也不把基线恢复与性能签收等同。继续在七月主体补原节点连续
生命周期、多peer有界owner与匹配的唯一共识发布；受控接回时保留通讯资产。
代码目前仅本地恢复提交；main仍只同步事实记录，49份草稿未动。

## 2026-10-15 持续信箱接通已有 TLS 载体（组件，非手机切换）

输入ce5624b9。tls_transport的原有限V1仍拒绝过期及升级；新增由现有owner
控制的持续scope，TLS/应用认证启动有30秒预算，完成NVMBOX06请求后进入
认证空闲；下一请求首字节启动单一30秒预算，包含剩余帧、处理及回复。
正常空闲不重建连接；撤销、部分帧取消/超时仍关闭真实IO，不复活过期请求。
原每scope连接容量、全局scope容量、TLS1.3原始公钥与exporter绑定不变。

CarrierStream复用这条生命周期，Mailbox增加同一serve_session的V6单流入口，
不另建服务/身份/协议/存储。TLS没有QUIC多流：通知与寄存用独立且分别认证的
有界连接，共享原节点存储及资源预算；不把不同连接的认证结果相互借用。
当前Mailbox模块仍在iroh-transport编译feature下，但TLS测试实际走loopback
TCP/TLS，没有Iroh网络调用。尚未接Windows宿主路由与Android自动选路，
不是可达公网WSS部署、NAT打洞或完全自主路径验收。

真实TCP/TLS测试完成31秒无请求空闲后的Register/Put/WatchFetch/Consume，
存储回执验签、错误读取能力拒绝、原对象/期限不变和正常关闭；另一测试先
建立10秒通知等待，在另一个认证连接Put，并在3秒测试上限内取得存储确认与
通知，证明等待未串行阻塞写入。该上限是本机反阻塞检查，不是手机延迟/P95。
TLS回归另覆盖撤销时无请求也实际关闭IO、首字节后半包30秒超时且不能重启、
有限scope禁止升级、错pin/伪签名/exporter重用/截断/取消等。

首次测试编译因夹具借用临时数组失败，已改为有明确生命周期的绑定。较宽tls_
筛选44通过、1项原Iroh已知peer测试发生scope expired；同一已构建binary单项
复测通过6.41秒，不修改其期限、不把初次失败抹去。联合feature及tls-only的
严格lib Clippy均通过；Cargo原多binary共用源文件提示仍存在。
最终定向回归16通过、0失败、62.28秒，包含上述两项真实TCP信箱用例；命令与
日志见本地tls-continuous-final.log，所有日志在忽略artifacts。

原两台正式APP未替换、未重启。9128-IDLE经原普通输入框发送，两端页面各见
同一消息，最早L4 Put到发送方首设备持久回执2072ms（L3 Put起算2013ms）；
两端仍running/code0，蜂窝
继续外部Iroh中继。这个结果不是本轮TLS组件的手机性能收益；300ms/1秒仍未
签收。下一步在原宿主和客户端接真实可达的合格字节流，保留原消息与路由策略。

## 2026-10-15 七月原生握手与窗口补传轮询接线

目标仍active；七月恢复现场e52a2b20→本地a0755a0d，基底仍4f45526d。
不等待逐刀确认，不切回十月Relay controller。此次原node测试消费真实UDP握手、
加密ACK与有界发送/repair轮询，替代上一节夹具内完成握手和传递缺片快照的捷径。

复用原product_overlay签名/KDF/重放契约及NovoRUDP缺片窗口、repair与pacing选择。
新增固定长度NVHSHK01载体（不改变签名字节）和NVACK001完整对象绑定；正常会话
不随握手凭据过期而重建。丢失响应重传同一握手，不重新派生channel/重置加密计数；
伪造响应不消耗待完成的原发起者。ACK只承诺传输交接，不读取业务回执或授予最终性。

NativeObjectSender保留原对象，非阻塞有界发送；一个最新缺片快照可跨轮询排空，
不每发一批等待新ACK，重复快照不回绕游标。未知/错会话/错完整对象/过期epoch拒绝。
无反馈仅有界尾探测，no-progress开关启用时达到重试预算暂停发送，仍保留原输入；
显式TTL才到期，不伪造Delivered。不是已验收所有历史覆盖抑制/多peer公平策略。
接收反馈复用首个缺片起点的64片窗口（不是强制网格对齐）；前缀推进后仅扫描当前
窗口，不在每次ACK轮询扫描整对象。乱序/重复后的快照与原完整扫描逐步对照一致。

本机Windows：network NovoRUDP 64项通过，product_overlay 9项通过；node
native_pipeline 269项通过、24忽略。真实AOEM完整代数候选/经济结果/冷恢复专项
通过，最终sender版本复跑5.17秒；1024笔签名APFL原生交付/逐行鉴权参照复跑
6.31秒。时间是测试总耗时，不是TPS或批执行延迟。测试主动丢失真实数据报并补传；
真实AOEM库SHA256仍31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947。
原节点binary检查通过（13项未消费接口告警）；严格network Clippy仍因原未改动的
eth_rlpx/overlay/transport共13项失败，不称全绿。新代码的大错误返回已用所有权盒
修正，不禁止lint。初始握手长度常量与网格窗口假设被测试抓出并修正，未放宽断言。

尚未完成：原node二进制连续生命周期、多peer对象额度/调度、完成ACK有界留存与
丢失DONE后去重重答、唯一活动共识发布/RPC。当前完成反馈由测试交接owner保留；
不是完整生产可靠网络，不是四节点主链或新TPS。继续复用原生命周期接合，随后
执行同条件最终确认负载。恢复代码仅本地detached提交；main本次仍仅推送进度证据，
不将未闭环代码替换生产默认。49份legacy/runtime草稿、AOEM仓库与SDK未改。

## 2026-10-15 七月原生安全对象交付接入候选流水线

目标保持active；恢复现场70eb8dab→本地e52a2b20，仍以4f45526d为主体。
复用main@a9478676的原product_overlay握手/加密/防重放、checked原生帧及
既有有界fragments，不迁入duplex worker、Relay controller或新执行器。
帧只保留一份实现并接回后续长度/reserved修复；旧内嵌实现已替换，可从父提交恢复。
原ACK窗口/repair选择函数未改，传输缺片不读取AOEM回执或账本计数。

新增适配：NVRLY002既有Data二进制codec的独立载体子集（没有Relay连接或JSON），
绑定原认证channel和来源端点的1200字节原生UDP；原重组器增加显式NVFRAG02
768字节分片profile，保留NVFRAG01原大帧解释和正文哈希。整对象/每peer额度、
TTL、乱序/重复/冲突校验沿用，不把大型签名批直接塞进单个UDP包。中间descriptor
仍受消息数及声明字节上限约束；这些额度不是精确allocator/native内存计量。

原node新增NativeApflReceiver交接：认证数据报→有界重组→装配owner解析APFL→
既有AuthenticationRequest→同一CandidatePipeline。socket侧不执行交易验签/业务，
装配操作要求下游有空槽，pipeline背压仍保留原请求。结果不授予nonce或投票权。
原签名、链域、父绑定、完整代数执行和RocksDB候选恢复均复用上一片。

Windows本机结果：原node binary locked check通过（13项未接消费者告警保留）；
native_pipeline普通回归269通过、24忽略；network NovoRUDP筛选56通过、安全与
二进制载体8通过。新增1024笔真实签名APFL原生加密UDP交付通过（逐行独立验签
参照，不是1024笔共识/执行TPS）；256KiB对象丢片/乱序测试用原缺片窗口选择补齐。
坏来源/篡改/重放/旧会话、错误stream/object、profile混用、per-peer背压及TTL反例通过。
两笔真实交易的真AOEM完整计划测试现已经过实际UDP交付，坏签名/重复/错上下文、
两种父余额、完整经济结果和冷重启核对通过。最终新增反例后两项重点测试均复跑。
真库SHA256仍为31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947；
未修改AOEM仓库或旧SDK。一次带不完整exact名称的命令匹配0项，未计入通过；
随后正确命令实际运行1项通过。network严格Clippy失败13项，均位于本轮未改的
eth_rlpx/overlay/transport旧源；不取消检查，也不宣称CI全绿。

尚未完成：原node启动/连续生命周期消费、UDP线上握手与ACK收发/重试调度、
唯一活动共识发布和RPC。当前握手密码操作在夹具内完成，丢片快照到repair选择
也在进程内传递；因此不能声称已验收完整可靠网络或四节点主链。没有新增TPS。
下一步继续这些实际接合，不重造ACK、不将收到数据视为落账；后续端到端负载仍
按真实确认条件执行。源码仅本地detached提交，main本次只发布进度证据，49份
草稿保持不动，未将尚未完成的恢复工程替换为生产默认。

## 2026-10-15 七月主体迁入完整代数候选与耐久流水线

目标模式持续active。恢复现场由463220f7推进至本地70eb8dab，基底仍是七月
4f45526d；不是把七月网络塞回十月controller。复用来源为main@7d08f975的
已提交exec/bindings、node业务/状态/候选流水线和consensus/round_bft资产，
迁入前确认这些来源路径无未提交修改；未触碰49份legacy/runtime草稿。

实际迁入：统一整数组合与二进制结果、完整费用/窗口/诊断/失败效应编译、
原CPU参照、批读/批树、父状态捕获、常驻计算与独立I/O、候选耐久存储与恢复。
原共识模块接回整套round_bft规则/journal/pacemaker；尚未启动新签者或发布链头。
七月原节点只适配两处既有commitment调用的类型化参数，不改变其承诺字节。
旧SDK未被覆盖，真库测试显式加载当前main随仓AOEM。

完成结构正文→AuthenticationRequest→同一CandidatePipeline→完整代数计划→
AOEM RocksDB候选的接线。删除上一片临时鉴权专用owner及重复验签循环，
改用已有完整compute owner和整数lane；未新增执行器、controller或活动账本。
删除临时V2 ABI提取副本，使用原bindings完整ABI定义。删除内容均为本次恢复
引入且已有替代的受控源码，可从463220f7恢复；不删除运行数据、证据或草稿。

本机Windows结果：原node binary cargo check通过（13项未接消费者告警仍在）；
node native_pipeline普通测试268通过、24忽略；exec 65通过、25忽略；bindings
30通过、8忽略；round_bft 86通过。忽略测试不计入通过；另显式执行真库二进制
结果、完整费用依赖、窗口诊断、CPU候选恢复、整数lane不阻塞鉴权与排空均通过。
新增july_apfl_complete_plan_persists_and_recovers_on_original_pipeline通过：
真实签名APFL、坏签名/重复拒绝、两种父余额、单次完整计划且零业务重算、
候选记录/根/经济状态一致、关闭重开、旧owner令牌拒绝。是本地候选耐久测试，
不是网络共识最终性、GPU吞吐或业务证明生成验收。

尚未贯通原node进程的原生安全分片接收、活动共识发布/RPC；没有新增TPS。
下一步接原生NovoRUDP有界对象交付至上述唯一流水线，再接匹配发布协议，
不回到临时只验签owner或十月强制Relay入口。恢复代码暂存本地detached提交，
main本次仅同步可核验进度；整合验收后再受控接回main，不能称源码已远端交付。

## 2026-10-15 目标模式恢复：七月工程接入常驻AOEM鉴权owner

目标已设为active，普通开发不再逐步等待用户确认。恢复现场从d436296b推进到
本地463220f7：复用当前bindings的compute/library、V2 ABI及取消/生命周期测试，
经原exec门面调用。node迁入原鉴权graph的逐行结果绑定、坏签名作为拒绝数据，
并按原compute-owner模式适配有界命令与非阻塞ticket。解码/验签不在提交线程；
命令及未消费回复共享额度，背压返回原输入allocation。无新线程池或业务状态库。

校验当前main随仓DLL SHA256为
31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947，显式加载测试，
未替换七月SDK。bindings 10项普通回归及5项真库取消/超时/会话回归通过；node
35项输入回归及2项真库鉴权/owner回归通过，最后的permit交接修正另复跑通过。
原node cargo check通过（保留4项尚无消费者告警）。旧同步验签仅保留cfg(test)
作正确性参照，生产候选接口通过exec/bindings调用真实AOEM。

尚未接原node进程接收循环，未提交AOEM业务计划、状态发布或TPS验收；当前为
CPU验签任务接线，不是完整代数/GPU执行。恢复代码本地保存，main仅同步进度。
继续迁接同一owner和原生接收生命周期，不要求用户再发“下一步”。

## 2026-10-15 七月结构正文交接测试：不是进程接收/执行贯通

恢复现场本地detached提交d436296b（父c5a65ea1），新增node ingress/delivery，
复用已有APFL解码与严格行验签，返回保留共享结构的已验签行；整批成功才返回，
最后一行坏签名不暴露前缀。分别限制wire字节和展开预算，不重建逐笔raw正文。
不写pending、不预留nonce、不ACK、不调用AOEM，不增加网络线程/队列或执行器。

Windows本机原node库ingress测试35通过，包括真实loopback UDP承载原NovoRUDP
帧并交付三笔真实签名结构行、坏末行/错链/越界预算拒绝。这个fixture没有测试
网络身份握手、ACK/补传、丢包、主节点进程、共识或最终性，不能扩大签收。
仅节点库交接能力；生产收包循环尚未调用。函数当前同步验签，后续应复用常驻
异步鉴权，不得直接放进网络pump阻塞收包。AOEM执行接合与TPS仍未完成。
源修改仅在本地恢复worktree提交，未推送为活动产品；main只更新本记录。

## 2026-10-15 七月恢复现场：迁入真实签名结构输入，尚未连接收循环

恢复现场从4f45526d推进至本地detached提交
`c5a65ea179c56ce99636c5848b251af03ee412c2`，没有创建分支或改当前生产入口。
复用main@24610cc7的node `native_pipeline/ingress`中wire、apfl、authentication
及原测试六文件，迁入七月原node库；只增加模块装配和一项结构行直接鉴权反例，
不迁十月controller、网络或执行器。源实现已经在Git中；恢复提交目前仅本机保存，
尚未合入/推送main，不能将本台账推送解释为恢复代码已发布。

Windows执行`cargo test --locked -p novovm-node --lib native_pipeline::ingress`
32通过、0失败、0忽略。新增用例覆盖APFL共享行直接验签、原交易摘要/nonce身份
一致、同签者连续nonce，以及错链域、越界行、修改金额未重签拒绝。
`cargo check --locked -p novovm-node --bin novovm-node`通过，但有6项未使用告警：
这些能力尚未连接收/执行循环，不隐藏告警，不称生产接线完成。

明确接合缺口：七月原protocol交易版本及签名形状不同；opcode114 bulk调用点位于
原network-only gate，不能把该工具的计数当成原节点完整用户交易执行。下一步仍以
七月工程为主体，在原接收边界交付真实签名结构输入并接统一执行，不运行gate代替
节点、不将V3原文直接塞入旧固定profile。当前无新增TPS、AOEM或ACK变更。

## 2026-10-15 七月原生实现恢复起步：原节点构建检查通过

用户决定以七月原生NovoRUDP/APFL实现为主体，将后续有效能力迁入，不再以
十月Relay控制流作为主链实现主体。已从`4f45526d38c48676fbc118ae7cc5af254a90a676`
建立仓内忽略目录`docs/temp/july-native-rebuild`的detached worktree，无新分支。
其`cargo metadata --locked --no-deps`解析19个成员；Windows本机执行
`cargo check --locked -p novovm-node --bin novovm-node`退出0，耗时47.76秒。
这是编译检查，不是链接后运行、交易闭环或历史吞吐复测；尚无本轮TPS。

原SDK保留，AOEM仓库未改。当前main及49项受保护草稿未覆盖；同步保留远端
`90b2be65`通信节点部署改动。临时恢复现场不是第二套正式产品；后续按依赖迁入
真实业务鉴权、经济语义、共识发布恢复及统一代数执行等有效成果，验收后以
新增纠错提交替换活动实现。历史跨机性能及2.4G环境归属见路线，不冒充本轮测量。

## 2026-10-15 原 Windows 节点升级到空闲 V6 运行时

固定输入b8afbf95，release构建通过；原托管节点由da55b71502d4升级为
cab8da1354b6dbbb2c6fa12f0c1c7d3bb578a464920255ad6ffc03dd945c9c65。
沿原用户任务安装入口升级，配置/身份/DPAPI/策略4项哈希保持，停机后备份，
信箱及两份日志在启动前保持，未回滚日志、未初始化新节点。单进程fresh serving
约7.2秒。VPN路径与配置未修改；不是全新无依赖网络或秒级交付声明。

原两台正式Android不换安装、不手动Connect，恢复后9108/9109双向消息及
持久设备回执通过，截图核对各一次。发送侧Put至首个设备回执1797/2061ms；
期间用户另发一条消息，未混入两个标记的统计。首次升级观察保留旧连接code3、
蜂窝恢复期间一次code8/网络代次变化；后续两端running/code0，Wi-Fi direct、
蜂窝仍外部relay。不能把观察尾部健康描述为全过程无错误或五轮恢复通过。

既有mailbox_peer_probe增加可选0–35秒空闲，再使用同一会话寄存；保留原55秒
请求整体限制与能力/存储回执校验。release编译通过。对原L3的独立随机身份
探测在认证前被拒绝，原L3仍为Pinned客户端策略，未为测试放宽策略；不将其
算成空闲验收通过。此后应由原手机已认证会话验证删除业务保活后的实际空闲。
证据：artifacts/mailbox-idle-node-{release-build,probe-build,live-probe}.log，
NOVOCHAT/artifacts/idle-node-upgrade.json、message-events-device/idle-node-*.json。

## 2026-10-15 原生帧实现收敛：删除重复codec，载体尚未切换

输入8dfa2820。核对原novorudp与duplex帧代码：同NOVRUDP0/version1、字段布局和
checksum域，但duplex额外具有长度溢出拒绝和非零reserved拒绝。将已有强化实现及
六项反例迁回`novorudp/frame.rs`，删除根文件旧实现和duplex独立codec文件；
当前worker与E2E overlay直接引用原模块的同一个帧类型。duplex旧路径仅为选定
类型的兼容重导出，不含编解码逻辑，不引入root的旧ledger状态机。

正常帧格式不改；原模块现在同样拒绝非零reserved及无法表示的编码长度，不能称
所有异常输入行为完全不变。旧原件可从Git恢复，未删除历史证据或runtime/legacy草稿。
本机NovoRUDP相关48项通过；duplex回归213通过、10忽略，包含真实WSS双工、
背压和身份用例；忽略项不计通过。本片没有重建ACK、执行器或共识。
原`novovm-node` binary的cargo check通过，network lib严格Clippy通过。

范围限制：这是实际消费者共用原生codec和重复代码清理，不是可靠UDP载体接入。
run_worker仍连接Relay，尚无原生四节点/TPS结果。下一接合点仍为worker/session
中的实际I/O与完成事件，不改变node候选、APFL、AOEM或最终确认合同。
此前gate的APFL decode-only失败未在本片修复，不让旁支工具修复冒充主线交付。

## 2026-10-15 ACK草稿纠偏：仅工具校验，不签收主链迁接

输入b561c392。历史核对后撤回草稿中的network公开`novorudp_ack`入口，
NetworkOnlyAckV0和AckScopeV0仅置于原gate的私有`novorudp_gate/ack.rs`。
不新增生产ACK协议或调度器，不把工具抽取称作主链迁接完成。保留原ACK JSON控制格式与
NovoRUDP二进制帧，不新增业务JSON、不更改DATA/APFL、执行、共识或SDK。
原接收代码忽略recv_from来源；现按固定receiver地址、session、stream=1、
ACK对象/序号、expected_total、帧/正文epoch校验，拒绝重放/迟到、重叠/越界
missing范围、计数不守恒及虚假done。只有全部检查通过才推进epoch；无效数据
不能毒化后继合法ACK。解析前限UDP有效负载65507B，不扩大队列或重试。

此处来源匹配不是密码学认证：同地址伪造风险必须由后续既有E2E会话接合解决。
ACK只表示传输，不授予执行/持久化/最终性。不要把原gate提升为生产节点。
真实loopback UDP覆盖第三方来源拒绝、2/0乱序交付且丢1、复用原build_repair_plan
补1、完整ACK；另测错误session/stream、epoch不一致/重放、范围/计数拒绝。
移动前`cargo test --locked -p novovm-network --lib novorudp`：50通过，0失败/忽略；
`cargo check --locked -p novovm-node --bin supervm-novorudp-network-only-gate`通过。
首轮check有提取后unused serde imports，已删除并复验无该warning；原Cargo重复
bin目标提示仍存在，不属于本片。未跑四原节点/TPS；未改49份受保护草稿。

当前缺口仍是将原UDP可靠对象交付与既有加密会话接入同一node channel，替换
worker/session的强制Relay依赖；本片仅保留原工具调用者的校验，不提供新生产入口。
不能把组件通过说成主链已回到七月原生载体，亦不因此停止完整接合。

移动后本机复验：工具私有`gate_ack::tests`三项通过；完整工具九项中八项通过，
`tests::native_transfer_apfl_payload_mode_decode_only_reconstructs_canonical_native_txs`
失败，business_decode_error_count为1（预期0），单独精确复跑仍失败。第一次精确
筛选漏写`tests::`仅运行0项，不计通过。失败位于未修改的APFL decode-only用例，
本轮尚未做冻结旧构建对照或根因确认，不能宣称整套历史工具全绿，也不删断言。
本轮rustfmt检查与限定路径diff检查通过。未启动TPS、未改变主链默认载体。

## 2026-10-15 原节点完成通知：功能通过，性能回退，未晋级

输入5d4ce085；仅在pipeline终态reply发送后通知原产品线程，并把原RPC的
1ms sleep换为可中断park_timeout。计算/I/O此前已通知协调器；本次不是新增
AOEM语义或GPU能力，也不是完整事件驱动。协议、队列、费用和最终确认不变。

匹配构建旧节点321d2496、新节点6da0a8a9，同SDK31306037、冻结驱动07090a3c，
Windows插电，同机四原节点、单RPC、65,536真实签名、1024批量、共享收款方、
8192在途、24语义worker、诊断关闭；终点仍为四节点最终回执，另核对经济与冷恢复。
预定顺序旧1/新1/新2/旧2，未追加择优样本：

| 样本 | 成功TPS | P95(ms) | P99(ms) |
| --- | ---: | ---: | ---: |
| 旧1 | 9657.54 | 996.600 | 1023.492 |
| 新1 | 6858.77 | 1349.029 | 1491.587 |
| 新2 | 6178.24 | 1595.682 | 1690.855 |
| 旧2 | 10362.69 | 973.585 | 1187.036 |

按总成功数/总时间合并9997.70→6500.74，−34.98%；每轮65536成功、0业务失败、
0永久拒绝/未决，正常负载不代替失败测试。P95/P99包含四节点HTTP观察；不平均
分位数。查询次数旧755712/640000、新893952/915456，是伴随现象而非已证根因。
完整测试耗时旧47.65/48.56s、新120.87/138.91s，包含计时窗外历史扩展/重启，
不能与6.786/6.324/9.555/10.608s负载窗混用。电池预检曾拒绝启动，无电池负载；
四轮结束均Online。未确定调度/等待的细部根因，不宣称硬件或AOEM不支持并发。

候选构建403普通pipeline测试通过(65忽略)；真实完成唤醒1项、单入口失败/重启、
A/A/B、离线法定人数推进/追平/重启各1项通过。最初误用bin测试目标得到0项，
日志保留但不计通过；后改为lib目标实际运行。功能测试耗时不是性能证据。

**处置：候选不晋级。** 六个源码文件已精确撤回，本次只提交试验补丁、证据和结论，
不改AOEM/SDK、不改共识、不增加开关。49份受保护草稿哈希一致。独立远端6e3f80b9
定位更新已快进保留，不在本次冻结二进制性能声明内。默认本地build仍321d2496，
仅对应5d4ce085，不能称为包含最新定位更新的新构建。

后续不再将完成通知当作已证瓶颈修复；回到既有完整代数计划的数据驻留与实际工作
消减，保留CPU门。GPU层间交换、完整证明和百万TPS均未因本片完成，未生产发布。
证据：[固定契约与全部样本](../artifacts/audit/completion-wake-20261015/summary.json)、
[验证与二进制身份](../artifacts/audit/completion-wake-20261015/validation.json)、
[未晋级补丁](../artifacts/audit/completion-wake-20261015/candidate.patch)。

## 2026-10-15 自主节点声明更新当前载体地址

输入5d4ce085。product_rendezvous_owned::Node::serve此前只在启动时读取
scope.address，后续每60秒虽重签声明，仍沿用最初地址。本轮在原轮换分支
从同一长期Endpoint重新取得地址，再按原host签名并持久化；不重绑UDP、
不关闭健康连接、不延长旧声明/消息TTL、不允许替换传输身份。
覆盖L3与L4同一宿主。新地址仍经现有签名、序号、撤销和接收方策略验证。

新增回归证明候选更新可持久重开、后续Offer使用新地址、持续对端仍有效，
传输密钥替换及撤销后重入仍拒绝。测试使用明确的合成地址，不是公网连通证明。
服务租约组49/50通过(114.61秒)；原bootstrap_recovery_original_deadline_and_permission_prevent_dials
的总耗时<1秒断言失败，单独复测4.23秒仍失败；10月14日已有旧测试产物
novovm_network-d8e1789687f933a1也在同断言失败(3.28秒)。未放宽断言，
不能把该组记为全绿；这是待定位的原有端点期限/清理时延问题。
严格库Clippy通过。手机部署和实际直连升级/时延收益待下游验证；刷新地址
不会保证穿透CGNAT，也不证明300ms或自主传输。Windows现运行节点尚未升级。
本地忽略证据位于NOVOCHAT/artifacts/live-locator-{tests,deadline-retest,
deadline-old-binary,clippy}.log。金融、AOEM、协议线格式与VPN均未修改。


## 2026-10-15 信箱当前载体观测：组件通过，非选路优化

输入627bef00。MailboxClientSessionV2::carrier_observation_v1读取既有连接
的当前选中路径及QUIC平滑RTT；路径和RTT来自同一快照，缺失保持None。
检查原流有效性与poison状态，不发探测、不建连接、不改变协议或准入。
用于区分握手时Relay与后续路径迁移，不能将RTT等同寄存或端到端时延。
原真实QUIC测试增加首次/31秒空闲后观察及撤销拒绝：2项通过56.21s，
严格网络库Clippy通过。Android消费及真实路径数据由下游继续验收，
本片不证明300ms、匿名、自主传输或路径择优完成。
证据：target/mailbox-carrier-observation-{test,clippy}.log。

## 2026-10-15 手机自动L4空闲后双向寄存：本批通过，非300ms

NOVOCHAT 84cf380 固定通讯输入98c72b5e，原两台正式安装覆盖/readback
APK da3686f1be76。原Application owner自动连接收件手机L4，分别空闲
96.913/112.375秒后寄存，再空闲95.911/93.882秒继续发送；每端仅一次
L4拨号/认证，两次签名存储确认、两次本机领取/Consume及两次持久设备回执。
9049–9052四条正常输入，原ID/历史/联系人保留；结束仍attempt1、running、
code0，观测无first-cause。ADB业务转发为空，VPN/原L3节点二进制未修改。

L4 Put到发送端存储回执落盘1064/1139/1315/1206ms。两方向均经过外部
Iroh Relay；不是自有直达、匿名或300ms。收件端同进程比较L4与L3副本
入库：L4三次先到(981/1003/513ms)，L3一次先到(100ms)，不能只按L4
标签强制择路。当前为机会性L4副本与L3并行保管，尚非质量择优算法；
本机领取入口及设备回执仍依赖原L3 owner，独立多副本未签收。

下游真实QUIC Application回归首次在并行Android编译时40秒突发门失败；
构建结束后原测试、原预算复验178.47s通过，突发38.141s。保留首轮失败，
不认定资源竞争是唯一根因，不据此签收低延迟/长期稳定。完整源码位置和
证据见NOVOCHAT docs/implementation/AUTOMATIC_RECOVERY.md最新节，
及其artifacts/l4-idle-*、message-events-device/l4-idle-*.json。
下一片在既有长连接和原消息队列内做实际路径质量与延迟归因，不新增协议。

## 2026-10-15 NVMBOX06 空闲与请求期限分离：组件通过，手机待验

输入 f0a73d7b；NOVOCHAT 84c30c4 已接自动收件 L4 副本，但两部手机在
L4 认证后分别空闲约39.6/60.2秒，Put立即拒绝，未取得L4 StoreReceipt。
源码确认：已完成请求留下30秒期限，服务器也将等待下一请求计入30秒。

本片仅修改原Iroh/CarrierStream/Mailbox生命周期。V6完整成功交换后进入
可撤销空闲；服务端收到第一字节才开始原30秒预算，覆盖剩余帧、所有分块、
资源等待、持久工作和回复。客户端每次请求仍有限时。取消、半截帧超时、
身份/能力错误不能复活旧流；V1–V5未取消期限；原序号/AEAD/配额保持。
使用现有QUIC保活，无新心跳RPC、无限重试或并行调度器。

验证：4项真实loopback QUIC测试通过(99.06s)：31秒真实空闲后原会话
完成多帧Put、存储签名验证和领取；撤销拒绝；首字节后原预算超时不能复活；
空闲读取取消不能复活。13项原信箱回归通过(26.54s)，严格Clippy通过。
旧有限入站测试新增拒绝空闲API断言：首次原有限scope超时，未改预算重跑
通过(4.40s)，此时间性失败保留，不能据此签收关闭/长期稳定性。

开发期间首次命令误用上游target启动冷依赖构建，已取消；新测试缺少
Zeroizing导入已修；取消测试原夹具未发送开启QUIC流的前导帧导致失败，
补齐后四项全过。完整日志位于artifacts/mailbox-idle-v6-*.log，不覆盖失败。
本轮不是Android实机结果，不证明300ms、P95、独立传输或匿名。下一步
下游固定此输入，保留原手机数据，验证闲置超过30秒后真实自动L4寄存。

## 2026-10-14 Android L4 真实入站寄存/领取：direct 与 relay 对照

在通讯输入 4fd28feb、NOVOCHAT e36c4fd 的两部原正式安装上，使用新增通用客户端
示例 `crates/novovm-network/examples/mailbox_peer_probe.rs` 调用已有 NVMBOX06；
没有新增宿主、账号、许可、网络协议或运行时。客户端校验从手机原 MethodChannel
读取的新鲜签名记录，节点公钥固定到该记录，Relay 由本机参数明确批准。随机
能力和探测数据只驻内存，数据先 AEAD 加密；节点不持有解密密钥。费用未接线。

两机均成功：Register、Put、验证 StoreReceipt、重复 Put 幂等；关闭发送端
Endpoint 后，使用新的 Endpoint/用途签名者读取，错误能力被拒绝，正确能力返回
原密文/对象/期限，Consume 后为空，最终解密匹配。两个客户端顺序运行在同一
探测进程，不冒称独立物理客户端或独立进程。实际宿主是手机原私有 SQLite，
不是临时宿主；没有 ADB 业务转发。读取记录短暂使用的调试转发在业务前全部移除。

|宿主|实际路径|建连至认证（不含 bind）|Register+Put+存储签名核验|四步领取检查*|
|---|---|---:|---:|---:|
|PKL110，Wi-Fi|direct，笔记本同局域网|103ms|68ms|87ms|
|PCLM50，Wi-Fi关闭/蜂窝|外部 Iroh Relay|5211ms|1929ms|2107ms|

*错误能力 Fetch、正确 Fetch、Consume、空 Fetch；新 reader 认证分别103/7620ms。
仅各一次样本，不是P95、A→B手机聊天时延或300ms验收。笔记本探测进程沿当前
系统网络策略，未增加VPN分流规则；原NOVOVM节点直连规则/Codex VPN保持。
Wi-Fi手机显式结束进程再打开后 L4 自动恢复，原身份/联系人/历史未清理；蜂窝
手机不重启。原聊天任务仍有运行事件，但本轮未新增聊天消息或重测五轮恢复。

结论：Android L4 已有真实外部入站服务证据；仅换为L4并不保证更快，必须区分
直达与外部Relay。尚未签收手机A自动向手机B寄存、联系人内的可信节点交换、
自动质量选路、独立多副本、奖励、匿名或自主传输。现有聊天信箱仍选已配置节点。
下一片应在原Application owner接加密且有序的手机节点元数据及候选选择，不能
改写已有日级不可变信箱声明的固定nonce载荷，也不能用收件手机替代唯一离线副本。

验证：示例构建与严格Clippy通过；首次编译rand 0.8不支持64字节数组random，
改为OsRng.fill_bytes后通过。证据：SUPERVM artifacts/l4-peer-probe-{build,
build-fixed,clippy}.log；NOVOCHAT artifacts/l4-{cell,wifi}-probe.log 和
l4-peer-after-*.log。签名地址记录/调试VM令牌留在忽略目录，不提交。

## 2026-10-14 固定24-worker同源库阶段对照：未复现稳定CPU回退

输入479cca10；AOEM c11238d5已含独立模型/GQA更新，不能把新构建直接和此前
ee6cee6f库当作单变量对照。因此同一c11238d5 FULLMAX构建父版90792e9c与
仅复用da80f306十二个已核Git对象的候选328b43b1；CPU语义调度源码未修改。
同一冻结原节点81da4509，不宣称当前整仓重新构建；SDK仍31306037。

固定24语义workers，插电、四原节点、单RPC、65536真实签名交易、1024笔批、
共享收款方和8192在途；预定父/候选/候选/父四轮，均开启已有阶段计时。
这是诊断，不是关闭计时的正式性能/晋级样本，不与原八轮合并或覆盖其结论。

|配对|父版诊断TPS|候选诊断TPS|变化|父P95/P99秒|候选P95/P99秒|
|---|---:|---:|---:|---|---|
|1|9095.76|8932.62|−1.79%|1.177/1.380|1.235/1.380|
|2（反序）|9098.60|9499.71|+4.41%|1.233/1.271|1.122/1.137|

每节点64批的跨节点均值：CPU执行684.2→691.4ms、689.3→689.4ms；批树
404.8→403.6ms、406.1→408.5ms，未见与此前16%降幅对应的计算阶段差异。
鉴权272.7→290.0ms、288.2→297.2ms；元数据写505.2→625.3ms、495.7→526.9ms
均较慢，但候选第二轮整体较快，不能认定它们解释了历史回退。主持久化反而
1205.3→1121.2ms、1098.3→1085.6ms。各项是嵌套/并发墙钟，不能相加或当CPU时间。
两对idle分别+58.0ms/−241.5ms；回执观察请求858112→863232、845824→776192。
相关变化不足以证明观察器、调度器或Windows是唯一原因，不能据此缩短等待。
四轮每节点64次业务/鉴权、8512次持久操作，整数执行计数0；工作未被跳过。

全部四轮65536成功，业务失败/永久拒绝/未完成均0，完整经济、历史和冷恢复通过。
本轮没有发现足以授权生产调度修复的因果缺陷，CPU历史负向观测仍UNRESOLVED。
不再无差别重复worker或TPS扫描；这组诊断不能撤销此前未晋级，也不能宣称
新库已无回归。后续有明确等待交接/执行成本假设时再做针对性验证，不以
“继续归因”替代完整代数计划的设备驻留交付，不重造执行器或修改共识门槛。
本轮不更新SDK、默认模式、业务/共识；临时驻留候选撤回，源码历史和证据保留。
实际四轮阶段、调用次数和原始报告见
[固定预算阶段对照](../artifacts/audit/cpu-library-phases-20261014/summary.json)。

撤回核验：FULLMAX正式源码重建4m19s，库95b7133d与本轮父版仅PE构建身份字段
不同，解析遮蔽后完全相等；完整费用/后继修复显式启用回归通过3.31s。首次漏加
--ignored的0测试记录保留，不计通过。49草稿逐项哈希未变；仅删除本轮候选DLL
及专用生成SPIR-V，可从历史源码重建，未删除证据、正式SDK或用户运行数据。

## 2026-10-14 CPU回退有限归因：配置边界和旧程序自身波动

基线cd0f0932，AOEM c11238d5只读。没有恢复未晋级驻留代码、没有重建库或修改
默认执行。四个原节点的workers=4只进入ComputeSession的ingress配置；AOEM
SemanticGraphRuntimeV2::new独立读取AOEM_SEMANTIC_GRAPH_V2_WORKERS，否则使用
可用逻辑核（本机24）。CPU分支不创建IntegerLane，旧/新历史报告整数行数均0、
每节点64批/65536分量和鉴权行数一致；并发峰值21–24，不是退回单线程。
这能排除CPU负载实际走了整数GPU分支的解释，不能排除共享库布局等未测影响。
已在ResidentConfig.workers处补准确说明，不增加配置/ABI或新的调度器。

同一冻结节点81da4509与旧库16a303d0，插电、四节点、单RPC、65536签名交易、
1024笔候选、共享收款方、8192在途和四节点最终回执不变。四轮预定24/6/6/24，
正式关闭计时；只改变已有语义worker环境设置，不是同条件实现提速对照。

|语义workers/计算池|第一轮TPS|第二轮TPS|合并TPS|第一轮P95/P99秒|第二轮P95/P99秒|
|---|---:|---:|---:|---|---|
|24|8238.36|10205.43|9117.00|1.277 / 1.288|0.896 / 1.033|
|6|8441.08|8095.86|8264.87|1.180 / 1.206|1.328 / 1.626|

实际峰值分别24/6，证明设置生效；6-worker合并低约9.35%，两组方向不同，不能
签收缩池提速。更重要的是未修改的旧库本身在8238–10205之间变化，覆盖上轮
新库8184附近与旧库9743附近：−16%仍是有效阻断观测，但不能直接认定因果。
这不是证明库完全无回归，更不授权用新样本推翻上一轮未晋级。

随后仅两轮诊断（不混入正式成绩）：24-worker 9640.98、6-worker 8054.16。
每节点64批累计鉴权273.7–290.4ms→543.6–552.7ms，业务CPU677.2–694.4ms→
650.0–658.8ms，归并/批树基本接近。减少线程的少量业务收益伴随更慢鉴权，
不支持将所有阶段统一缩到6。I/O持久化1298–1430ms→1059–1199ms、HTTP
1108–1226ms→1438–1569ms、idle4354–4630ms→5529–5844ms；这些是嵌套/并发
墙钟，不能相加或将idle全部判成无用空转，也不能据此解释未记录阶段的上一轮。

六轮全部65536成功、业务失败0、永久拒绝0、未完成0，经济/历史与冷恢复通过。
不新增生产计时、不调共识超时、不更换SDK、不修改AOEM、不重跑独立故障场景。
此片解决的是测量/配置理解，不声称修好了CPU库差异或交付驻留性能。下一项
限定固定资源下不同库的现有阶段对照，定位实际多出的工作/等待；不再扫worker
参数或根据“GPU比CPU慢”重造执行器。原CPU默认与未晋级决定保持。

完整预定顺序、原始六样本与阶段统计见
[归因结果](../artifacts/audit/cpu-worker-attribution-20261014/summary.json)。

## 2026-10-14 重叠流水线复用设备驻留：整数收益，CPU保留门未过

用户明确授权修改AOEM通用适配。输入SUPERVM c043b8b1 / AOEM ee6cee6f；只复用
da80f306原12个runtime文件，保留现行独立模型上下文。复验理由是原负向驻留
实验使用串行计算owner，而当前节点已分离有界鉴权与整数通道。没有重造代数
计划、算术、共识、节点或API，没有把NOV业务移进内核。

候选仍走原node完整条件/费用计划→exec→bindings→AOEM的v10与AOIR0002。
原逐层CPU绑定、回读/上传被设备值槽与guard取代，层间仍保留必要设备屏障；
一次上传/终端导出读取，不是单kernel。执行候选与签票/发布依旧分离；原
successor允许精确耐久父候选上的私有准备与父共识重叠，不授予未来签票权限。

插电Windows、选择5090、四个原节点/库、单RPC、65536真实签名转账、1024笔
候选、1024付款方/共享收款方、在途8192、四节点全部成功最终回执，正式关闭
诊断。固定八轮交错/反序，不追加更有利样本；同节点二进制81da4509，旧库
16a303d0，新库36d399de，均同FULLMAX release设置。合并按总成功数/总耗时。

|路径/轮|旧TPS|新TPS|旧P95/P99秒|新P95/P99秒|
|---|---:|---:|---|---|
|整数1|4971.85|6752.14|1.853 / 2.094|1.352 / 1.753|
|整数2|5114.03|5979.43|2.124 / 2.364|1.680 / 1.806|
|CPU1|9856.12|8144.64|0.988 / 1.148|1.282 / 1.330|
|CPU2|9632.09|8223.21|1.477 / 1.638|1.378 / 1.510|

整数合并5041.94→6342.34（+25.79%），CPU9742.82→8183.73（−16.00%）。
每轮65536成功、业务失败0、永久拒绝0、未完成0；完整经济/历史回执/冷恢复通过。
查询观察次数变化保留，不能把差额全归为纯GPU计算或把该有限样本称为稳定容量。
七项显式测试通过：完整费用与后继修复、共享/未共享导出、窗口/诊断/容量、
u128、单入口失败与重启、实际A/A/B（14.68秒）、离线隔离/追平/重启（49.87秒）。
后两项总测试时间不是在线确认时间；测试运行产生的正常断连日志完整保留。

**决定：未晋级。** 整数收益不抵消CPU两组下降；CPU算法未改不能排除共享库
回归，也尚无证据指定热量、系统调度或代码布局为原因。按预声明门槛撤回本轮
活动runtime，不替换SDK、不留拒绝版本开关；已有实现仍可从da80f306恢复，
测试及原始结果保留。当前产品层间回读问题仍OPEN，不能把候选成绩写成已部署
收益。下一片应有限定位这次CPU负向观测后再决定驻留实现，不重新设计主链。

49份草稿哈希无变化；未改变CPU默认、业务/共识/确认终点，未执行新完整证明
或百万TPS验收。本片没有主CI代码签收，产品节点是已冻结81da4509，不将其
称为当前整仓重新构建。见[完整测量](../artifacts/audit/resident-forward-20261014/measurements.json)
及同目录contract.json、run-load.ps1、原始八轮报告与功能日志。

撤回核对完成：AOEM活动runtime源码无差异，FULLMAX重建4m17s；恢复库72bb4a16
与本轮旧库仅PE链接/调试时间及GUID不同，解析遮蔽后字节相等。恢复库完整费用/
后继修复回归3.31s通过。只移除试验candidate.dll和专用composition.spv，历史
实现和全部证据可保留复用，未删除运行数据。详见同目录restored-binary.json。


## 2026-10-14 NVMBOX06 有界密文通知与 Android 实测

通讯实现 4af93ea6，合并主线 f4ebf9f3 保留其他机器计算工作且通讯输入树不变。
原 Mailbox 的持续操作/通知流显式协商 V6，WatchFetch 最多附带一个 64KiB 密文，
验证全部能力/游标后通过原 store.fetch_one 读取，通知不删除对象、不冒充送达。
V5 保留且拒绝 V6 操作；重启/丢通知后仍可 Fetch，TTL、逐信箱 Consume 和撤销保持。
11 项共享单测及4项真实QUIC多流/拒绝回归通过，NOVOCHAT 7项客户端回归、严格
Clippy、25项工具检查通过。release 构建3m14s，原Windows托管节点同身份升级，
两部原正式安装覆盖新APK并核对完整哈希，无清数据/改邀请/ADB业务隧道。

普通输入框9033–9036四条双向消息取得持久设备回执，均实际使用通知附带对象。
Put→收件持久Inbox有界投影约1.55–1.65/1.00–1.10/2.37–2.47/1.77–1.88秒；
Put→发送方DeviceReceipt为3027/2449/4479/4064ms。不是UI首绘或P95，仍未达到
300ms/整体即时性。Wi-Fi direct，蜂窝经 euc1-1 外部Iroh Relay；当前路径平滑RTT
重复观测 direct90次均值10.146ms，relay83次均值893.954ms、最大1323.103ms。
节点Put/Fetch/Consume最大48.229/16.102/17.555ms，准入无等待。省去首对象Fetch
不等于无后续空查，更不能把外部Relay算成自主传输或低延迟通过。

原始证据：artifacts/watch-fetch-{unit,multiplex}-tests.log、watch-fetch-release-build.log、
node-watch-fetch-vsix-deployment.json、node-watch-fetch-metrics-final.json；客户端
完整样本/时钟边界见 NOVOCHAT 的 AUTOMATIC_RECOVERY.md 最新记录。下一片在原
Application owner 接 Android L4 服务；通用协议保持，不改金融/AOEM/共识。

## 2026-10-14 完整计划行块实测：不调整生产默认

输入577ab6f6，已快进保留通讯更新。沿原node的完整费用计划、exec统一入口和
同一SDK31306037增加测试专用资源形状入口；生产仍128行块，容量/决策/分桶仍1。
无AOEM改动、无GPU旁路、无CPU算法改变。先前驻留候选仍未晋级。

复用原同值/异质1024笔计划与静态分组统计；插电Windows，选择5090，同一完整
计划与导出，32/64/128/256/512/1024正序与逆序各一轮，每次资源创建和一次预热
在计时外。测完整run_effects，不是kernel时间，也不含签名/共识/持久化。
24个测量及各自预热输出全部一致，测量时没有编译/节点负载并跑。

|行块|同值两轮毫秒|异质两轮毫秒|
|---|---|---|
|32|45.059 / 43.437|134.786 / 126.931|
|64|33.212 / 34.539|87.749 / 91.412|
|128（当前）|30.442 / 27.717|82.218 / 75.454|
|256|33.213 / 34.567|77.429 / 66.798|
|512|45.203 / 45.630|80.630 / 80.653|
|1024|87.711 / 87.946|127.256 / 129.014|

两计划均28层，6689/20572节点。128行块静态dispatch上界90/193、填充后行数
11266/24196；1024行块为50/61和49154/58372。动态守卫可跳过节点，以上不是
真实设备dispatch计数。总负载65536不是一次GPU有效批量；此结果不支持仅扩大
行块解决问题，也不能由两个夹具断言最优参数或整链主要耗时比例。

403常规native_pipeline测试通过（64显式忽略）；新增真实AOEM实验通过10.88秒。
未测新的四进程TPS/P95/P99、故障或冷恢复，上一轮9568.59/5491.57仅是历史成绩，
不重标为本片结果。没有生产性能修复，不签收内部驻留或百万TPS。首次构建误用
默认target/runtime-rebuild后停止本次进程，改用既有target缓存，未删除文件；
最终release测试构建2m28s。已有草稿不纳入提交。

结论：保留实验和输出等价保护，不改生产行块。下一项应从已有da80f306驻留实现
解决逐层回读/上传与逐组完成等待的组合成本，而非重复造DAG、二进制或调整总笔数。
源码适配、完整原始24样本见[实验汇总](../artifacts/audit/resident-tile-cost-20261014/summary.json)
及同目录run.ps1、tile-cost.log；未将私有知识库或运行密钥纳入证据。

## 2026-10-14 实际QUIC路径RTT与通知往返成本

输入577ab6f6 / NOVOCHAT75e767f。在原IrohStream/CarrierStream读取当前选中
路径的平滑RTT，原MailboxService响应阶段累计direct/relay固定计数，仍由既有
status发布。无探测包、新服务、地址/身份字段或请求路径/权限改变。缺失观测
不是零；RTT不是业务处理时间、签名回执、隐私凭证或独立样本P95。

10项原信箱及计数回归通过15.57s；真实QUIC双向大帧/绑定用例6.43s通过，
同时检查选中路径RTT存在且非零。不改原超时、IO失败锁存和认证。release构建
2m34s通过，原任务部署7AC6717417370BE985D0594311147CF8B9EE8611F8755B03E454DB3415F310EF，
PID36820；原keys/policy/journal/mailbox在替换时不变、同公钥新序号恢复。
手机APK和进程不变，均自动进入attempt4；Wi-Fi direct、蜂窝relay。
最终补充仅非Iroh构建的特性边界；TLS-only检查19.01s通过，无虚构RTT值。

首份同窗口快照：direct17次平滑RTT观测平均5.752ms、最大6.158ms；relay17次
平均1105.217ms、最大1402.693ms。节点Put/Fetch/Consume最大42.482/10.012/
39.828ms，响应入队最大58us。证明本批蜂窝载体存在约一秒往返成本，不能再以
节点几十毫秒处理解释主要等待；不据此断言所有地区/所有样本具有相同瓶颈。
现有自动直连配置保留，节点程序TCP/UDP防火墙Public规则为Allow，无本轮修改。

9031/9032双向新消息均持久收件并取得设备回执；同发送端Put起算StoreReceipt
148/2420ms、DeviceReceipt3694/3864ms。没有ADB业务转发，未手动重连、未重建
联系人/消息/TTL。仍非秒级或独立自主传输；原外部Relay参与。

源码还确认现有Watch只返回Notice，客户端随后串行Fetch才能获得密文；在当前
蜂窝RTT下，这一步有真实成本。下一片在同一信箱中显式版本协商“通知+有界密文”，
消除一次领取往返，复用原存储/能力/去重/设备回执；保留旧版拒绝与兼容处理，
不静默更改旧线格式。不将这一待实现优化当成已经改善延迟。

证据artifacts/mailbox-rtt-tests.log、mailbox-rtt-carrier-test.log、mailbox-rtt-release-build.log、
node-rtt-release-deployment.json、node-rtt-metrics-first.json；NOVOCHAT
artifacts/message-events-device/rtt-ready.json、rtt-messages.json及test-enqueues。

## 2026-10-14 信箱节点处理分段与优化部署对照

输入7d8ff5a7，NOVOCHAT c0524ff。用户批准仅节点进程分流直连并已打开两台原
Android。不改VPN/TUN、节点策略、手机安装、联系人、消息、TTL、预算或隐私。
此前托管节点来自target/debug，不能用未优化构建代表最终节点性能。

原MailboxServiceV1增加固定本机原子计数，原Windows owner在既有定位声明发布
回调输出status.json.mailboxTimings。没有逐请求日志、新RPC、计量结算或第二
调度器；不保存内容、身份或能力。execute计时含排队、工作线程和存储提交，
response_enqueue只测本地响应提交；不包含传输到达、解帧或对端收件。Watch
本来就有等待，不能算成处理卡顿；中止可以发生在已落盘之后，不能当成未存。

9项mailbox_rpc回归通过（22.77s），包括真实存储队列等待、错误领取能力仍拒绝，
错误/取消不伪装成功。自主节点真实进程回归1项通过（12.86s），验证同身份恢复、
运行中变更拒绝以及新版status计数。原多bin路径编译告警保留。

21:19部署debug计时版本FD6F82A776FDD7F31DAF1D5BB2BD6FB34C8C5118A6B59928EBEE3BDEEA8E47A8，
PID37876；停机期间keys/policy/journal/mailbox哈希一致，重启签名序号推进、原公钥
和DPAPI保持。两机自行进入attempt2，Wi-Fi direct、蜂窝relay，无ADB业务隧道。
9025/9026双向新消息真实inbox持久提交，Put至StoreReceipt为265/1747ms，
Put至设备持久回执4453/3388ms；不是远端首次显示计时，也不是P95。
节点同窗口Put4次最大50.726ms、Fetch8次最大40.654ms、Consume4次最大73.152ms；
请求准入最大7us，响应入队最大5.033ms，无请求拒绝或中止。蜂窝17次完整RPC
713–2490ms；由此能排除本批存储处理/资源队列的秒级阻塞，尚未精确划分载体
等待、解帧和其他检查。该批同时进行release首次编译，不作为严格性能A/B基准。

网络核心实时读回：节点TCP及UDP均ProcessPath→DIRECT，Codex仍SG代理，TUN=true。
蜂窝仍经原批准外部Iroh Relay；分流不等于自主传输或秒级通过。
证据：artifacts/mailbox-node-timings-tests.log、mailbox-node-timings-owner-test.log、
node-debug-deployment.json、node-debug-metrics-after.json；NOVOCHAT
artifacts/message-events-device/node-debug-ready.json、node-debug-messages.json及
原test-enqueues。

同源码release首次构建8m38s通过。21:28原任务部署优化产物
391E6A681B1428B0D482A9A3B84624950E8571D8BA5D1835A39006064D919C4C，PID10244；
同样验证停机期间四份持久状态字节不变、重启原密钥/公钥及新序号。两机原进程
自行进入attempt3，未手动Connect，Wi-Fi direct/蜂窝relay保持。9027–9030四条
新消息全部收件持久提交且有发送方设备回执；Put→StoreReceipt分别171/2022/
228/2240ms，Put→DeviceReceipt分别8307/5694/3375/4484ms，方向交替Wi-Fi→
蜂窝、蜂窝→Wi-Fi。前两条在节点重启恢复后，后两条为同健康会话继续发送。
不能将前两条排除后声称性能提升，也不能将这些少量样本作P95/生产承诺。

release同窗口累计Put8次/Fetch16次/Consume8次（包括设备回执对象），最大
44.232/49.303/56.504ms；响应入队最大147us，资源准入低于计时微秒精度，
无请求拒绝/中止。最初4次Put最大9.330ms，但后续存储耗时会变，不以最快值
代表全批。手机普通RPC：Wi-Fi 30次11–133ms；蜂窝27次609–3420ms。
优化部署降低部分节点处理成本，但本批整体投递没有已证明的改善；仍需细分
蜂窝中继传输/接收与往返流程，不增加重试预算或放宽验签。保留release部署。

新增证据node-release-deployment.json、node-release-metrics-first/after.json、
mailbox-node-release-build.log；NOVOCHAT node-release-ready/messages/steady.json。
Windows崩溃自动重启、L4实机寄存、独立冗余和整体生产目标仍未签收。

## 2026-10-14 CPU单通道重复计额剥离与冻结参照

输入7d8ff5a7，已保留远端L3安装/载体选择更新；实际输入node仍与上轮77a504eb
相同。限定原compute/lanes，不改AOEM、统一执行门面、完整代数计划/二进制结果、
鉴权、经济规则、CPU计算算法、共识或发布。整数模式的鉴权/执行重叠完整保留。

CPU模式只有一个有界sync_channel，却仍逐命令申请共享Budget并在出队释放。
现在只在两个通道确实共用上限时创建Budget；CPU单通道由原channel直接限制排队。
每条CPU命令省去共享额度申请/释放与Slot的Arc克隆/释放，满队仍退回同一输入，
失败锁存/关闭/退出排空和原流水线端到端许可不变。整数两通道仍合计N个排队额度，
不各放大成N、不取消panic/原生资源锁存。是减少冗余计额，不是另造调度执行器。
新反例检查CPU无共享Slot、满队背压、消费后可重入、故障拒绝与接收端关闭；
原双通道配额、竞争panic及真实AOEM重叠/排空反例继续通过。

正式顺序测前固定十轮：两组CPU与整数新旧交错，加两轮冻结重叠前CPU参照。
插电Windows、四真实原节点/四库、单RPC；65536真实预签名、1024付款方各64nonce/
共享收款方、候选1024、在途8192。整数四进程共享选定5090。诊断关闭，构建不与
负载并跑；仍等四节点最终回执HTTP返回，签名生成/启动/经济冷核对不计入窗口。
新增参照不与基线混算；全部轮次保留，合并为总成功数/总耗时。

|模式|输入版两轮TPS|本轮两轮TPS|合并TPS|
|---|---|---|---|
|CPU快路径|8899.85 / 9189.90|9321.15 / 9829.53|9042.55→9568.59（+5.82%）|
|整数完整计划|5214.88 / 5737.59|5386.88 / 5600.41|5463.76→5491.57（+0.51%）|
|冻结重叠前CPU（单独参照）|9494.82 / 9103.45|不适用|9295.02|

P95/P99毫秒：CPU第一组1205.750/1231.251→1234.554/1263.957，第二组
1140.785/1182.934→1062.858/1140.278；整数第一组1842.008/1964.326→
1670.377/1744.412，第二组1730.617/1770.230→1669.063/1684.907。
冻结CPU参照为1096.934/1272.800及1102.709/1175.506。
本轮CPU合并比冻结参照高2.94%，但逐组相对参照方向不同；输入版合并比参照低
2.72%，同样一负一正。**保留实际开销削减及本组CPU正向观测，不把前轮−3.11%
全部归因于队列，不宣称回归风险完全排除或稳定GPU提速。** 整数代码的预算与
业务执行没有改变，其吞吐变化不能作为CPU计额修改加速GPU的证据。

十轮均65536成功、0业务失败/永久拒绝/未完成，经济、历史和四库冷恢复通过。
baseline-cpu-1、baseline-integer-1、candidate-cpu-2各有一次观察HTTP连接超时，
随后确认完成；原events保留且不从计时扣除。正常负载没有业务失败，失败语义由
单独回归验证。403项非忽略native_pipeline测试通过，63项明确忽略；9项真实AOEM/
节点显式验证通过，含费用/窗口/后继修复、完整u128、A/A/B、离线追平和重启。
全工作区fmt、release lib严格Clippy通过。没有本轮Linux/四物理设备/新业务证明
验收；独立BigInt oracle未重跑。49草稿哈希保持。上轮CI38563008376最终cancelled，
不是通过；输入7d8ff5a7的CI38565266698在本轮核查时仍运行，交付CI另查。

旧/新node SHA256为`77a504eb520b66f66f9e8c21f90ea97ab72f517a41afd0ebfd539679dab2ff62` /
`81da45094cb6eeef5419ebb3af32264ecb3fd9450fda7e3b9e76bf4948e5c98f`。
SDK仍为`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`。
详见[汇总及逐轮数据](../artifacts/audit/resident-cpu-queue-20261014/summary.json)、
[源码/二进制/回归核对](../artifacts/audit/resident-cpu-queue-20261014/validation.json)和
[固定对照契约](../artifacts/audit/resident-cpu-queue-20261014/contract.json)。逐笔观察大文件
本机保留，汇总记录其路径、字节数和哈希；错误事件与完整结果报告随提交保存。

本项有限隔离收口，不再围绕此小包装不断调参。后续应回到统一完整计划的层/程序
分组、有效行、真实依赖和内部调度/数据流，复用已有驻留实现及失败对照。65536是
总负载而非一次GPU批量，现候选1024、常用资源行块128，不能仅增加总笔数声称并行
充分；也不能将已经撤回的历史驻留候选写成现行SDK能力。CPU默认及百万级目标保留。

## 2026-10-14 自有节点自动载体选择与 VPN 保留

输入2f53183f。现有节点原来relay_only=true，虽同Wi-Fi手机可本地访问，仍固定
经外部Iroh Relay。新增原owner本机维护命令--set-self-owned-carrier；只在现有
DPAPI policy内保存automatic/relay-only选择，同目录封装替换；不修改登记配置、
客户端范围、节点/传输身份、撤销、journal、SQLite或手机Privacy。无新网络/许可。

新增真实进程回归通过：运行中修改拒绝、无批准中继/未知模式拒绝且原policy不变；
有效切换时原配置/keys/journal/mailbox字节不变，重启后节点和载体公钥不变并能
真实收发。连同原自主新建和迁移/跨任期/重启，共3项通过171.27秒。首次编译
共享helper导致mirror/publisher未使用告警，已将helper收回唯一owner，最终无该告警。
保留工作区已有多bin路径告警。diff检查通过。

20:34原Windows任务替换同一原生程序并切automatic，PID33556，11.154秒新序号
serving；keys和节点公钥不变，切换瞬间journal/mailbox保持。EXE SHA256
c84e709c888cbbafc24a11d3213fe197aa0c1a789585d1796a59abcdacd3eddf。
签名定位声明含直接IP候选和原批准Relay；实际Wi-Fi手机自动恢复attempt10，
认证path=direct，蜂窝attempt12仍relay。没有ADB forward/reverse，没有改VPN/TUN、
默认路由、网卡、防火墙或手机安装。direct表示实测载体路径，不能据此宣称匿名。

正式聊天9017-AUTO从Wi-Fi发送，原任务记录Put→StoreReceipt259ms、
Put→DeviceReceipt4701ms；蜂窝真实inbox持久提交。单个样本不能推出P95或
整体稳定性/速率提升。蜂窝侧仍外部Relay，整体秒级未达。反向9018工具因
NOVOCHAT非前台在输入前拒绝；未替用户切走正在使用的应用，不计发送或双向通过。
原APK/身份/联系人/数据不变。

证据：artifacts/carrier-switch.json；NOVOCHAT artifacts/message-events-device/
carrier-before.json、carrier-after.json、carrier-messages.json及test-enqueues账本。
Windows异常退出自动重启缺口仍在，未因本片重新签收；下一片分开验证节点VPN分流
可达性与蜂窝Relay延迟，保持Codex可用。旧EXE无法读取新增policy字段，回退只能
用新版命令调整载体偏好，禁止恢复旧密钥/日志快照。

## 2026-10-14 当前用户节点登录启动与异常恢复缺口

输入0f7babce。笔记本19:19重启使原前台节点消失，status.json旧serving不能作为
健康证据。原v1专用服务账号安装器不能直接读取当前用户的v2 DPAPI材料。
扩展原install-novovm-rendezvous.ps1的-UserSession分支：同一用户、原节点根，
LocalAppData受限ACL下的固定二进制/配置，通过Windows任务计划登录启动。
不改原信任、钱包、VPN、TTL或网络；没有第二运行循环/账号/网络许可。

原生入口改Windows无控制台子系统，任务直接监管原生进程；重定向进程测试
仍可读诊断。原生构建通过，既有自有节点真实进程回归2项通过（158.35秒），
覆盖自主新建、旧身份迁移、跨实例、第二写入者拒绝、崩溃重启及缺历史拒绝。
PowerShell解析/diff检查通过；安装实测拒绝错误SHA、旧控制台程序和改策略配置，
原健康节点未受影响。首次Status误将TaskScheduler回显用户名与SID直接比较，
已改为解析相同SID再比较；真实读回通过，不放宽身份或执行路径核对。

系统任务已安装，交付EXE SHA256
dc666d1c1cb221b78edcc1303dad3a663dc1ec2a3ab7bf3228e1dccc122d49ee。
20:15原PID35600结束后，任务PID30352在约6.864秒重新serving；原keys.dpapi、
self-owned-policy.dpapi字节不变，节点公钥不变且签名序号推进。原SQLite/撤销/聊天
数据保留。任务按当前用户登录触发，无执行时长上限；没有重启电脑或模拟登录，
因此真实开机登录启动仍待验，不承诺用户未登录或电脑睡眠时可用。

**异常恢复失败必须保留：** 向Windows配置RestartCount3/Interval1分钟后，
终止30352，92.3秒观察没有新进程，LastTaskResult=4294967295。20:17手动启动
同一任务恢复PID28748，未回滚数据。随后独立四任务对照：定时/手动触发 ×
UnifiedSchedulingEngine开/关，每个实际退出1，仅启动一次，100秒内均未重启。
四测试任务已定向注销。原因尚未定位，不把配置读回当作系统自动恢复成功；
当前Status分别报告真实进程数量、任务状态和configuredRestartCount，不宣称网络健康。
不得用无限重复任务掩盖失效监督。该断网测试不是无缝升级/长期稳定性通过。

恢复后正式两机无需点击连接，attempt9/11继续message_exchange；9015/9016新
消息真实双向领取并取得持久设备回执，Put起算约5.859/5.798秒，仍未达秒级。
APK仍NOVOCHAT79efe12/fd82796b，无卸载/清数据。仍依赖外部Iroh Relay，
relay_only未改变；新程序路径入站防火墙、自主直连及后台长期恢复分别待验。
日志artifacts/rendezvous-user-*.log/json/xml、task-recovery-*/result.json，手机
证据NOVOCHAT/artifacts/message-events-device/node-task-*.json，均为私有原始产物。

## 2026-10-14 原常驻owner鉴权与整数执行重叠：功能成立，稳定性能未签收

输入0f7babce20a5231ac4e5891b19cb491f38f31f2f，保留最新NOV服务文档、DNS补丁
及L3信箱更新。范围仅原node的compute owner与测试；不改AOEM/SDK、共识、GPU
算法、经济规则、v10完整代数计划或二进制结果。原node→exec→bindings→AOEM
路径保持，不增加新执行器、第二权威状态或CPU业务修复回调。

整数模式原已持有CPU ComputeSession及ResidentTransferResources两个原生上下文，
但都在一个线程同步服务，整数调用占用时后续鉴权只能排队。现在同一owner有两个
固定通道：原CPU会话负责准入/认证/父绑定/准备，原整数上下文负责捕获后的完整
候选执行及既有packet生成。上下文均在所属线程创建、使用、销毁；不跨线程移动
非Send句柄、不新增原生上下文、不逐批建线程。一次完整候选委托没有拆回逐波Host
裁决；没有将业务循环搬进新线程冒充新增代数能力。CPU默认仍单通道、算法不变。

两通道共用原N个**排队**额度，而不是各N个；可同时各执行一个命令，这是本轮的
有意重叠。原流水线端到端许可继续保留到结果消费。排队满/关闭原样退回输入；
shared panic latch阻止后续工作并在返回前再次检查，原原生资源错误锁存不变。
普通签名/业务拒绝不等同panic；不得声称所有原生错误均由此共享标志处理。
显式shutdown关闭、排空并join两通道；不重开资源、不取消已接受工作。

新增实际AOEM验证：先持住整数通道，排入完整候选执行；同一owner的真实有效签名
在CPU会话完成、篡改签名被拒绝，此时执行结果仍未返回。释放后核对完整候选；
第二次执行接受后立即shutdown，退出后仍可取回等价记录/状态根/承诺。
这证明通道/生命周期独立推进，不是物理CPU/GPU同时利用率或饱和度证明。
共享上限、消费/断连释放、普通错误/panic、另一通道竞争返回及外层unwind另有反例。

独立启用阶段诊断的样本（正式跑分关闭），每节点64批累计墙钟；嵌套、并发，
不能相加为关键路径，排队下降不能直接换算整链加速：

|范围|原串联owner|双通道owner|
|---|---|---|
|node0准入鉴权排队|2889.76ms|9.36ms|
|完整计划编译/导出投影|1239–1257ms|1243–1274ms|
|整数统一门面|3699–3768ms|3752–3853ms|
|完整整数执行（含编译/门面/树等）|5615–5727ms|5689–5785ms|
|node0 HTTP服务累计|1824ms|2699ms|

鉴权串联解除，但完整执行成本未下降。诊断回执观察请求1190912→1980416；
正式整数两组观察请求也上升。说明提交/观察节奏发生变化，尚不能断言新增HTTP
工作吞掉全部收益，更不能把约3.8秒门面累计全部称GPU kernel时间。

正式条件不变：插电Windows、四原节点/四库共享5090、单RPC、65536真实预签名，
1024付款方各64nonce/共同收款方，候选1024、在途8192；四节点最终回执HTTP
返回作为终点。八轮顺序测前固定：旧整数/新整数/旧CPU/新CPU，再反向第二组。
构建和其他测试不与负载并跑；冷热恢复、经济历史核对不计入计时，仍要求通过。

|模式|旧版两轮TPS|新版两轮TPS|总成功/总耗时合并|
|---|---|---|---|
|整数完整计划|4784.05 / 5037.61|5169.33 / 4888.13|4907.56→5024.80（+2.39%）|
|CPU快路径|8523.42 / 9178.21|8374.44 / 8761.99|8838.70→8563.83（−3.11%）|

P95/P99毫秒：整数第一组2081.364/2283.266→1941.344/2037.067，第二组
1831.110/1877.737→2080.439/2273.782；CPU第一组1119.750/1300.326→
1271.048/1432.924，第二组1166.134/1344.792→1291.185/1575.477。
**整数方向不一致，CPU两组均下降且尾延迟均变差：稳定提速未签收，CPU负向待归因。**
不能因CPU业务算法没改就排除回归：此次CPU也经过新的队列/健康状态包装。
保留可重叠的功能及局部排队证据，不把本轮宣称GPU加速、百万TPS或生产晋级。

8正式+2诊断均65536成功、0业务失败/未完成/永久拒绝，完整经济/历史/四库冷恢复
通过；业务失败另由回归覆盖。402非忽略native_pipeline测试通过，63明确忽略；
9项显式验证通过：新双通道、原常驻CPU、计划共享等价、费用容量影响后继、窗口/
诊断/容量、完整u128，以及真实原节点单入口失败重启、A/A/B、离线追平重启。
A/A/B仍为3个真实诚实进程+受控故障peer，不冒充四物理机器或任意崩溃注入。
release lib严格Clippy、全工作区fmt通过，49草稿逐文件哈希未变；独立BigInt oracle
本轮未重跑，无本轮Linux实跑。初次测试编译误用AdmissionRow.checked已改为result，
保留失败日志。relay只做rustfmt：输入main CI38547158541失败于该文件格式，
未进入主链测试；前片ae06a6b3 CI38546748226为cancelled。最终提交CI须另查。

冻结旧node SHA256 `6c4c723573001e0ed4377d2d8cb15b3d12016bf186b6a6a86dc21ec1ebc3fd82`；
交付node `77a504eb520b66f66f9e8c21f90ea97ab72f517a41afd0ebfd539679dab2ff62`；
SDK `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947` 未变。
证据在[本轮汇总](../artifacts/audit/resident-overlap-20261014/summary.json)、
[验收及源码哈希](../artifacts/audit/resident-overlap-20261014/validation.json)、
[预定对照契约](../artifacts/audit/resident-overlap-20261014/contract.json)。原始10轮与
全部失败/回归日志一并保留。下一片不再重复证明此排队问题，先有限隔离CPU包装
与运行条件的影响，再沿同一完整计划定位实际关键路径；不直接重启已未晋级的
GPU驻留补丁，不增大窗口、不降低确认、不以总交易数代替候选有效批量。

## 2026-10-14 L3 冷启动 DNS 故障修复与请求预算

输入6f60edd1。Android用户反馈约两秒远端显示，随后本机L3升级重启失败。
新程序及原程序均在20秒relay online等待后退出；HTTPS及WebSocket升级成功，
相同Iroh解析器A查询却报other response code，home relay始终为空。抓取的真实
DNS响应header为NOERROR，OPT TTL为0x00000001；simple-dns 0.12.0错误地将
TTL低8位作为扩展RCODE，遂将标志位解成BADVERS。此为可复现首错，非信箱
签名/数据错误，也不以增加等待期限解决。

vendor/simple-dns按RFC6891 6.1.3修正扩展RCODE高8位及VERSION次高8位的
解析/编码；保留真实BADVERS、REFUSED及身份/问句/TLS核验。源为锁定MIT包，
有来源校验和/变更说明。158库测试、8集成测试、6文档测试通过；新增线格式
回归覆盖各标志位、真实扩展错误、版本与编码。首次测试遗漏上游samples，
补齐后通过；上游README错误位序示例亦修正，保留失败记录。

桌面L3复用既有节点总请求桶，明确burst64/refill32每秒，代替整个节点2请求/秒
的移动保守默认；不变更L4默认、连接数、字节/存储配额。已有共享预算回归1项
通过，不将此配置宣称为已定位全部延迟或已通过负载抗耗尽验收。

Windows构建通过；同一受保护节点根、密钥、撤销与SQLite，在17:58重新serving。
原两机PID未变、自动重新认证，原双向消息补收并显示持久送达；随后两条
RECOVERY9003/9004双向新消息亦送达。仍经外部Iroh Relay，非自主传输验收。
这次部署中断和回滚启动失败必须计入失败样本；不是无缝升级或生产稳定性PASS。
本机原始证据：artifacts/dns-*.log、NOVOCHAT/artifacts/message-events-device/。
无资金/共识/AOEM改变；后续手机构建必须显式复用同一DNS补丁（依赖根patch
不会自动传递到另一workspace）。消息即时性、此前E8及L4正式激活仍未签收。

## 2026-10-14 常驻计算排队定位与候选编译紧凑键

输入6f60edd1，保留最新开放信箱更新。本轮不改AOEM、SDK、GPU shader、共识、
经济规则或正式默认后端。实际路径仍为原node→exec→bindings→AOEM v10完整
条件/效应计划及二进制结果；不是重建预测循环，也不恢复独立finish业务裁决。

原编译器BindingKey将完整1024位Literal与Output放在同一枚举中，每个依赖引用
都需容纳宽变体。本轮在每次编译/导出投影内部精确驻留完整常量，以紧凑符号作为
哈希键：Output仍是明确row/column，键不超过12字节；完整常量仍参与精确比较，
不截断、不缓存业务授权、不跨候选复用。下发AOEM的字面量、依赖、守卫、程序、
导出顺序及既有共享节点数不变。未测峰值分配，不能称零拷贝或设备驻留改进。

扩展既有默认关闭的阶段统计，不新建遥测系统：queue_admission从成功入队前的
标记到compute owner出队，不含执行服务；queue_candidate包括其余候选命令，
不是纯GPU队列。integer_compile含完整计划与导出投影构建；integer_projection
为返回后物化，排除stage_results。诊断用实际v10二进制路径，不用旧JSON oracle。

独立诊断每节点64批累计墙钟（嵌套及并发，不能相加当关键路径）：

|范围|对照|紧凑键候选|
|---|---|---|
|完整计划编译/导出投影|1511–1557ms|1221–1242ms|
|整数统一门面|3664–3710ms|3775–3916ms|
|最终结果物化|105–109ms|106–115ms|
|完整整数执行（包含以上及树）|5873–6035ms|5687–5845ms|
|node0准入鉴权排队|2731ms|2645ms|

另列CPU诊断鉴权排队191–204ms/节点，完整CPU执行680–698ms。源码run_owner
逐命令同步调用execute，整数调用期间不能处理后续Admission；此结构及排队样本
支持后续处理该交接，但尚不能将全部排队/端到端差额归给GPU、OS或某一个锁。
ComputeSession仍有同线程/前图drain约束，不能简单跨线程移动或假称已异步化。

正式条件：插电Windows、四原节点/四库/共享一块5090、单RPC；65536真实预签名，
1024付款方各64nonce/共同收款方，候选1024、在途8192。仍等四节点最终回执HTTP
返回；签名生成/启动/经济历史冷核对不计入计时。八轮顺序先固定，正式诊断关闭。
对照是同一6f60edd1加相同可选计时、未加紧凑键；不是拿旧网络二进制作对照。

|正式模式|对照两轮TPS|候选两轮TPS|总成功/总耗时|
|---|---|---|---|
|整数完整计划|5154.10 / 4468.96|4930.84 / 5107.44|4787.14→5017.59（+4.81%）|
|CPU快路径|9415.03 / 8929.81|8844.30 / 10008.04|9166.00→9390.26（+2.45%）|

整数P95/P99毫秒：第一组1850.063/1908.877→2121.586/2237.423；第二组
2073.960/2135.419→1830.602/2133.525。CPU第一组1105.576/1271.309→
1144.088/1403.290；第二组1208.466/1241.202→948.567/985.650。
**两组吞吐/尾延迟方向不一致，不签收稳定提速；CPU算法没有因此变快的证据。**
保留紧凑编译表示及其局部工作削减，不把合并正向扩大为GPU或主链容量突破。

十一轮负载（八正式、三独立诊断）均65536成功、0业务失败/未完成/永久拒绝，
经济、历史回执、四库冷恢复通过；正常负载不冒充失败业务覆盖。当前测试二进制
399项非忽略回归通过、62项明确忽略；另显式跑7项：完整共享/非共享导出等价、
费用拒绝影响后继及CPU对照、窗口/诊断/容量、完整u128、单入口失败重启、A/A/B、
离线追平重启，全部通过。A/A/B仍为三个真实诚实节点+受控故障peer；本机同机，
不等于四物理设备。独立BigInt oracle本轮未重跑。首次测试编译缺queued解构已修，
原失败日志保留；汇总脚本首次假设正式样本含profile_start，改用真实live_status
确认诊断关闭，未更改原测量记录。
release lib严格Clippy与全工作区fmt检查通过；无本轮Linux实跑。

SDK SHA256仍31306037；对照node9e389f09、交付node6c4c7235、当前测试5e246e63、
冻结负载驱动07090a3c。完整哈希、所有轮次及49草稿无漂移见
[本轮汇总](../artifacts/audit/resident-queue-20261014/summary.json)与
[预声明条件](../artifacts/audit/resident-queue-20261014/contract.json)。

结论不是“65536太少”或“GPU硬件弱”：65536是整场负载，实际候选上限1024，
算子资源行块多为128；当前整数完整计划仍显著慢于CPU快路径。内部层间传输、
门面工作和同步owner占用尚未全部消除；四进程共享设备、客户端观察窗口也限制
本测量的解释。下一片处理原常驻owner鉴权/执行完成交接，复用已有统一语义和
生命周期，不另造执行器、不把上下文冒充Send、不取消背压/失败锁存/父状态绑定。
本片没有新完整证明、百万TPS、Linux/公网或生产签收；CI另核。

## 2026-10-14 L4 无客户端预登记的持续信箱

输入 `85e3c10b`，限定既有网络握手/信箱/平台宿主。`ServicePeerPolicyV1`明确
区分 Pinned 与 AnyAuthenticated；后者接受未预登记但能完成签名、载体绑定和
双向 Finished 的密钥。空/超限受限名单仍拒绝，不把未知身份直接当作可信联系人。
`run_self_owned_mailbox_on_endpoint_v1 → Node::serve → NVMBOX04/05`复用原服务，
不新增 wire 格式或调度器；旧入口保持受限，开放宿主拒绝旧信箱/寻址调用。
每条流认证一次，操作继续验证能力/序号/撤销。所有连接共用原节点存储/请求预算，
贡献关闭/资源暂停拒绝新义务，保留密文、领取及原 TTL。不取消用户 Privacy 策略。

Windows 真实 QUIC/SQLite 平台宿主5项通过（1.01s），其中开放模式无需登记写者，
第三把未登记密钥凭正确能力领取，错误能力拒绝；同时原出站通信保持，停止和
重开保留内容。握手7项通过（7.90s），新增矩阵含开放成功、受限未知/空名单、
伪造签名、篡改Finished、弱键及反射身份拒绝。原多流2项（5.99s）与存储7项
（0.68s）通过，定向严格Clippy通过。日志在 `artifacts/l4-platform-owner/open-*.log`。
初次负向夹具被客户端编码器提前拒绝导致服务端等待超时；修正为实际发出畸形
字节后验证服务端拒绝。首版夹具类型推断编译失败亦保留，未增大超时或放宽验签。

这是组件及原生平台接口交付，不是正式Android L4寄存、负载/抗耗尽、独立副本
或自主匿名验收；两机安装和既有L3配置未改变。Android按原Application owner
接Keystore、贡献设置及资源观察；不应再构造固定客户端列表作为公开信箱准入。

## 2026-10-14 复用设备驻留计划：内部成本下降，端到端门未过

输入85e3c10b、AOEM b6cc47f0，保留最新Android锁修复和独立模型上下文。
直接复用历史da80f306的12个运行文件，Git blob逐项相同；未重造DAG、二进制、
业务计划或GPU算术。父版与候选均Rust1.94.0、同FULLMAX/locked/release构建，
两库在同一验证目录。冻结原node4d6f4bbc/驱动07090a3c；这不是重建最新网络产品。

插电Windows、指定5090、四原节点四库、单RPC，65536真实预签名、1024付款方
各64nonce/共享收款方、批1024/在途8192，仍计到四节点最终回执HTTP返回。
签名生成/启动/冷核对不入计时，正式八轮关闭诊断，顺序开跑前固定。

|正式模式|父版两轮TPS|候选两轮TPS|合并TPS（总成功/总耗时）|
|---|---|---|---|
|整数完整计划|5436.55 / 4837.45|5018.30 / 5172.38|5119.53 → 5094.18（−0.50%）|
|CPU默认快路径|8958.61 / 8429.15|9340.72 / 10252.85|8685.82 → 9775.55（+12.55%）|

整数P95/P99（毫秒）：第一组1631.036/1670.346 →1946.261/2259.117；
第二组2150.206/2197.547 →1933.705/2112.302，方向不一致。
CPU第一组1285.469/1343.331 →1157.758/1277.690，第二组1113.554/1161.539
→885.781/1010.195。CPU正向是观测，不称算法提速，也不消除之前负向记录。
整数未满足预声明端到端门，**不晋级、不替换正式SDK**；不能用CPU收益抵消。

正式八轮及另列诊断两轮均65536成功、0业务失败/永久拒绝/未完成/计时内传输错误，
完整费用经济、历史回执和四库冷恢复通过。失败语义由原正反依赖、窗口/诊断/
容量及u128测试覆盖；加导出等价、单入口、A/A/B、离线追平/重启共7项通过。
AOEM资源13项及显式Vulkan5项通过，首次过滤器匹配0测试保留且不计通过。
未重跑独立BigInt oracle，不把历史结果改称本次；A/A/B仍为3诚实进程+故障peer。

补充诊断走真实v10，不走旧JSON行profile；不混入正式成绩。每节点64批累计：
整数门面3671–3723ms →2179–2884ms，完整整数执行5881–5974 →4365–5090ms；
树405–434 →409–439ms，HTTP1825–1991 →2284–2380ms。均嵌套/并发墙钟，
不能相加、不能叫GPU kernel时间，也不能由此认定HTTP就是唯一根因。
内部执行缩短尚未兑现为端到端收益；四进程共享一块GPU，不能外推四独立设备。

活动12文件补丁撤回到当前b6cc47f0，模型上下文不回退；完整实现仍在da80f306，
不留备用开关，候选DLL/生成shader及相关产物按内核规则撤除。SDK仍31306037，
49草稿哈希保持。源码、正式与诊断结果分开保留在
[本轮证据](../artifacts/audit/resident-plan-reuse-20261014/validation.json)。
下一处是定位候选就绪、发布与RPC观察之间抵消局部收益的实际等待，复用已有阶段
和原候选，不重做驻留、不继续盲调CPU，不改变窗口/确认口径追数。
本片没有新正式GPU能力、Linux/多机/完整证明/百万TPS或生产签收；CI另核。

## 2026-10-14 L4 手机共享日志锁适配

输入 `bcc0d26f`。Android平台的标准库File::try_lock在当前工具链返回Unsupported，
其源码sys/fs/unix.rs的目标条件已核对。共享`ServiceLeaseFileV1`的稳定owner文件、
数据文件及压缩替换文件统一使用安全的非阻塞flock适配；其余平台保留std锁。
复用已锁定rustix的fs接口，network仍forbid(unsafe_code)，无新日志格式/手机数据库。
密文、序号CAS、原授权/撤销历史和压缩事务语义不改，锁失败继续拒绝操作。

Windows新增原生跨进程独占/错误密钥/落盘重开回归2项通过（含子进程实际拒绝），
已有6项日志压缩及每个发布边界进程崩溃回归通过，定向严格Clippy通过。
日志在`artifacts/l4-platform-owner/journal-*.log`。Rust 1.94.0 / NDK 28.2.13676358
ARM64 release原生检查程序构建15m31s；SHA256
`7cc734f6af476df3a6c6a5600db5c317e67461a7b0565ac8e7673683957e7038`。
同一程序在PKL110(Android15)、PCLM50(Android12)各2项通过（0.05/0.04s），
均实际启动独立子进程验证持锁时拒绝第二写者，并验证落盘重开/错误密钥/CAS。
使用各机`/data/local/tmp`临时夹具，不涉及主App数据、安装或网络服务部署；
这是真机日志兼容性证据，不是手机L4寄存、E2EE或冗余验收。

## 2026-10-14 CPU有界归因：同机器码负对照仍有约10%差异

输入2972b81e，快进59ab066e保留网络更新；本轮不改运行算法/SDK/计时器。
AOEM94c214e1与已接受aab3f8bb仅测试/文档不同，原FULLMAX features、Rust1.94.0
release/locked重建成功。旧31306037与重建9d0294f1均44,705,792字节，`.text`
完全相同；整文件仅28字节不同，均为解析PE目录确认的链接/debug时间戳和PDB GUID。
只屏蔽这些元数据后全文件SHA256一致，不是凭“CPU源码没改”推断机器码相同。

固定旧/重建/重建/旧顺序，另列旧/重建诊断两轮，不追加挑选样本。实际使用冻结
node4d6f4bbc和驱动07090a3c，不称重测了最新网络构建。插电Windows、四原节点
四库、单RPC、65536笔、1024付款方×64nonce/共享收款方、批1024、在途8192；
终点仍为四节点成功最终回执HTTP返回，签名/启动/冷核对排除。

|正式负对照（诊断关闭）|成功TPS|P95毫秒|P99毫秒|观察请求数|
|---|---:|---:|---:|---:|
|旧1|8613.75|1270.550|1424.111|919552|
|同源码重建1|9533.03|1024.355|1137.666|775168|
|同源码重建2|9658.79|1060.278|1139.010|716800|
|旧2|8847.47|1277.606|1450.068|859136|

总成功/总时间为8729.05→9595.50（+9.93%），**没有算法收益，不能登记为提速**。
加载路径和PE身份不同，未逐项干预系统调度/加载/存储/查询反馈，根因仍未定。
CPU resident owner显式加载/固定worker，不走安装profile自动推荐，也不创建
整数资源；这不等于穷尽系统影响。不能把差异全称随机噪声或放宽保护门到10%。

诊断9105.17/9816.53 TPS不混入正式合并。四节点阶段差值范围（旧/重建ms）：
业务execute_cpu701–709/691–696，树417–423/407–413，persist1109–1190/
1118–1257，HTTP1186–1379/999–1232，idle4506–4925/4264–4681；整数execute/
wire调用均0。均为嵌套/并发墙钟，不能相加或将idle全当可删除停顿。端到端差异
未表现为同比例业务计算或持久化增长，但这不是已定位唯一瓶颈。

六轮各65536成功、0业务失败/永久拒绝/未完成/计时内传输错误，经济、四库冷恢复
和历史回执检查通过。关停relay连接关闭日志保留；不是新增失败语义/AAB验收。
49份草稿哈希保持。SDK仍31306037，AOEM源码不改，既有GPU候选不重新晋级。
前两刀负向观测与撤回决定保留，但GPU补丁导致CPU回退的因果尚未证明。
下一候选直接复用da80f306驻留实现，与同工具链重建父版本、同目录布局成组对照；
负向须有同构建阶段证据，不能拿本控制的较快数字替换旧候选成绩或越过CPU门。

[六轮/PE/阶段证据](../artifacts/audit/cpu-build-control-20261014/measurements.json)、
[边界](../artifacts/audit/cpu-build-control-20261014/validation.json)。旧CI38506419662
最终cancelled，非通过/失败。59ab066e的38522021023失败在network两个文件fmt，
尚未执行主链步骤；本片只修相同格式差异，根workspace fmt复查通过，CI另验。
本片未交付新GPU能力、百万TPS或生产。

## 2026-10-14 Windows 共用 target 的双 Tokio 来源定位

输入 `28b10066`。NOVOCHAT 原owner回归2通过4失败，新定向夹具证明同一
Application runtime内Handle检查和直接Tokio UDP绑定成功，Iroh/netwatch
`UdpSocket::from_std`却报无reactor。失败linker map实际包含Tokio
`846da778cd63c275`与`a343ea2b4b26e36d`及两个独立TLS context；Cargo指纹
仅预期后者。实际载体来自13:36的共享仓构建，而APP期望13:00的依赖图。

已定位vendored Iroh同时生成lib/cdylib，产物为无hash的`libiroh.rlib`，
两仓不同锁定依赖（socket2/syn）在同一target顺序构建时覆盖它。改为本地
Rust依赖所需的rlib，由Cargo按图生成带hash文件；APP自身JNI/FFI库类型不变。
不改Tokio runtime、重试、网络协议或权限，不把清缓存/禁增量当修复。
原始上游清单/哈希保留，补丁范围说明已更新；通用碰撞背景见Cargo #6313，
本事故依据实际链接证据。失败EXE/PDB/map保留于NOVOCHAT忽略artifacts目录。

修复提交 `b2f43844` 后共享真实平台owner4项0.63秒通过，APP owner7项
0.58秒通过。实际新map仅含Tokio `a343ea2b4b26e36d`、netwatch
`0010d46bd510e801`和带hash的Iroh `0036a5c503aed22e`。再次从SUPERVM
编译（4项0.63秒），共享Iroh `48590cd38d223a0e`与APP Iroh文件SHA256均
不变；回到APP移除临时link-map钩子并重新链接，7项1.80秒再次通过，严格
Clippy含lib/tests通过。旧无hash库仍在target，没有删除依赖/清缓存。

初次APP重试曾因 `cargo fmt --all`格式化路径依赖、触发已审核输入门禁而
拒绝；已核对并仅撤回本次额外格式化，再完成上述验证，未绕过输入检查。
原失败与最终日志/链接比较JSON在NOVOCHAT `artifacts/l4-reactor-failure/`
及 `artifacts/l4-platform-*.log`，共享日志在 `artifacts/l4-platform-owner/`。
这是本机双Tokio初始化缺陷的确定修复，不代表历史所有网络异常或另一项
并发组合断言都已归因。Android标准构建另由产品仓记录；已装两机与当前L3
未更换，L4手机接线、性能及独立副本仍待验。

## 2026-10-14 L4 平台宿主接缝：借用持久端点，沿用原信箱

输入 `50794698`。新增 `IrohEndpointOwnerV1::with_owned_listener_v1` 和
`run_self_owned_node_on_endpoint_v1`，与专用 L3 入口共用原 `Node::serve`；
不创建新网络协议、数据库、runtime 或调度器。平台提供稳定用途传输身份和
本地 owner，入站消费者在整个服务期独占；出站操作保留独立认证与期限。
平台宿主最多两条入站连接，共享原四连接额度；专用 L3 仍为四条。

真实 localhost QUIC/SQLite 新回归4项通过（最终0.63秒）：寄存/验存储回执，
关闭资源及贡献后拒绝新义务、允许原对象重复确认/领取，重开保留密文、原TTL
和关闭选择；在同一端点上并行出站，宿主结束后原出站连接继续；临时身份拒绝、
竞争入站消费者拒绝；取消释放所有权、owner撤销不可复活；关闭等待借用结束
后才报告socket关闭。总bind计数未增加，不把传输测试字节当聊天E2EE证明。

旧NVMBOX04连续协议2项148.49秒通过，真实会话134.695秒跨3声明及70次初始
Fetch后仍可投递；签名/版本/有限scope/撤销/取消反例保持。NVMBOX05三客户端
六认证流及七类反例2项5.41秒通过；原持久owner8项5.23秒通过，严格Clippy通过。
命令均指定同一NOVOCHAT target、`CARGO_INCREMENTAL=0`、`-j1`，原始日志在
本机忽略目录 `artifacts/l4-platform-owner/`。不据本次通过宣布上一轮Tokio
初始化或APP并发断言间歇失败已经修复。

尚未交付：Android托管L4用途密钥、贡献UI与执行/电量/网络资源接线，以及
真实手机接受他人密文、独立冗余。临时聊天端点不能静默升级为公开稳定节点；
贡献密钥不授予资产花费权，也不证明现有本地根已绑定链上UCA。原安装手机和
运行L3此次没有替换或清数据，最新手机性能仍以上一批实测为准。

## 2026-10-14 NVMBOX05 正式 Android：单连接双流、离线队列与重开保留

共享输入 `ae8fc007`，产品 `NOVOCHAT@b301f5f`。同一本机自主L3原位升级，
保留原配置、DPAPI身份/策略、journal和SQLite信箱；配置及密钥/策略文件哈希
一致，无新Publisher、邀请或账户。foreground宿主按核对PID/路径定向结束后
替换，不称优雅停机；独占journal哈希读取失败导致升级间隔，旧04手机自动
重试并恢复。节点新exe SHA256 `beead7fbf31e32d8a4439ffebe8a3452c5727767ca90b8b47b178a958e41b3a4`。

正式APK `18c1bcd1129fd4b1b1732a0b11fe94552226c13d31e16ccfe80a0f37bb04d38f`
原位安装PKL110 Wi-Fi/PCLM50蜂窝，实际包哈希一致，无ADB业务转发。原
Application owner只connect一次，操作和通知流各自Finished/AEAD，不降低
验签、撤销、Privacy或旧协议门禁，不提高节点全局4连接/每组4流上限。
初次任务双流认证5.932/8.694秒；五次成功尝试（含恢复/冷开）每次均观测到
1 connection + 1 notification stream，没有第二次notification connect。

80–85共6条新消息真实持久交付。35秒蜂窝中断期间82已节点寄存、83本地排队，
恢复后原ID继续处理，无人工重连；任务2在15.379秒E211后由任务3接替，网络
可用到认证31.952–31.957秒。随后新消息仍送达，两端am kill确认进程结束再
显式打开后保留历史/Delivered，自动认证7.583/6.400秒，不称真实LMK/后台保活。
健康唤醒→StoreReceipt1.694–1.968秒，→DeviceReceipt6.774–10.996秒；含离线
等待的一条59.939秒全部保留，不声称性能提升、秒级P95或本批五轮恢复。

产品有限离线回归310.84秒通过，连续夹具有161.99/158.90秒通过记录，跨3声明
各1绑定/1连接/两流认证；2个边界、JVM42、Python23+2及严格Clippy/正式构建
通过。开放项：Windows初始UDP bind的Tokio reactor间歇异常，在信箱协议前
发生；重建dev依赖/禁增量后通过但未定位根因。一次连续夹具虽全部Delivered，
最终组合并发断言失败；拆分诊断后复跑通过，具体失败分项仍未复现。未删除
断言/扩大产品期限，不以单次通过宣称消除。原失败与新日志完整保留。

本片仅交付手机单连接双流，不是跨联系人节点聚合；仍单L3和外部Iroh Relay。
L4同协议宿主、独立副本、自主主载体、匿名及生产容量未签收。继续原L4主线，
另跟踪恢复长尾及主机回归。细节见产品 `docs/implementation/AUTOMATIC_RECOVERY.md`
Multiplex Mailbox v1节；忽略证据目录`NOVOCHAT/artifacts/multiplex-mailbox/`，
Android `acceptance.json` SHA256 `d9d9cec17fb95de82e9808417e77fefb7ccf1430e681d1640294c56a00d0356b`。

## 2026-10-14 NVMBOX05 单连接多流：组件通过，Android 接线待验

输入 `30d4cf45`。原 Iroh/CarrierStream 增加作用域内有界共享连接，原自主
节点分派选择 NVMBOX05 后，操作/通知各自认证与 AEAD，最多4并发流。
Finished 的 TLS exporter 增加版本域及实际 QUIC stream ID；根/子流都明确
选择05，旧版本拒绝且不降级。共享一个连接准入，不增加全局4连接上限、
收发窗口、存储配额或共享请求预算。正常流结束保留同组其他流，owner退出、
撤销、安全错误及部分IO取消仍关闭整组；原消息/TTL/回执和旧协议不变。

真实 localhost QUIC + SQLite：3客户端/6独立认证流同时存在，通过原
`run_self_owned_node_v1` 完成 Register、Watch在途Put、验StoreReceipt、Fetch、
Consume及结束通知后继续操作。它证明原4连接限制下可容纳三个双流客户端，
不是3台独立物理设备，也不是生产容量或性能测试。7种负向情形覆盖旧选择器、
有限scope、子流身份/pin/版本错误、撤销和取消；所有权单测另验证stream ID
绑定、第五流拒绝、准入回收、普通流结束和owner关闭。

共享信箱重放/通知竞态/能力/撤销/维护回归8项通过；旧NVMBOX04连续fixture
跨3声明约134.6秒及7种负向情形通过（2测试）；严格Clippy通过。首轮容量测试
最后断言误将进程级bind计数当单客户端计数，修为同owner实例/操作次数后复测
通过，初始日志保留。首次错误过滤器匹配0测试不计通过。Clippy发现新结构的
枚举尺寸差异，Iroh变体与TLS一样用Box承载，另修测试不必要借用，无静默抑制。
证据在忽略目录 `artifacts/multiplex-*.log`。

此时在用节点/手机仍为上一片。下一步升级同一节点程序及NOVOCHAT原owner，
保留原受保护身份、联系人、消息和运行选择，验证每任务一次connect、双流
各自认证、在线投递与蜂窝恢复。跨联系人节点连接聚合、L4实机寄存、多副本、
生产容量、P95秒级、完全自主传输和匿名都仍未签收，不用本组件结果替代。

## 2026-10-14 Android 健康信箱通知/发件分离：已交付，容量与秒级仍待

产品 `NOVOCHAT@18e2629`，共享审核输入仍为 `6fbe812c`，与本仓快进到
`2972b81e` 的 Cargo.toml/network/protocol/adapter-api/vendor/iroh 树一致。
本片不改节点二进制、协议、身份或金融/AOEM。原 Application owner 在同一
Endpoint 上建立两个 NVMBOX04 通道，各自 Finished/AEAD 认证同一节点pin；
操作通道 Put/Fetch/Consume，观察通道有界 Watch，既有加密待发队列提交即
唤醒发件。没有新的网络/调度器，没有跳过验签、撤销、Privacy或重置原ID/TTL。

这里是两条 QUIC 连接，不是单连接多流。当前载体 bidi stream 上限1、scope
连接上限4，两部手机各2连接已经用满；不能据此签收多用户/多联系人容量。
首次夹具双connect后才发selector导致lazy open_bi的服务端建立窗口失败，
已改为每条connect后立即发送。任一路失败仍释放并交回原owner恢复，不忽略拒接。

正式APK `6343fc7d` 已原位安装原两部Android，保留原身份/联系人/历史，
冷开不点Connect，任务到两路认证8.196/7.027秒。健康72–77及恢复后78/79共
8条消息得到设备持久回执。健康本地提交唤醒至StoreReceipt为1.274–1.693秒，
至Delivered为5.089–7.838秒；不是点击发送时延，缺失独立唤醒点的一条不造数。
一次18秒蜂窝中断后网络可用至认证约14.1秒，原任务正常取消后任务2自动恢复。
78/79实际在恢复后发送，不作断网期间入队证据。约16分钟末A仍任务1、B任务2，
均running/message_exchange/code0；无ADB业务转发，实际外部Iroh Relay仍参与。

真实连续Application夹具1项162.96秒、跨3声明、bind各1/认证每通道各1，
Watch在途时Put/Store推进；五条突发最终回执28.981秒，非性能通过。旧有限
离线夹具1项310.61秒、安全边界5项、JVM17项、Python23+2项、严格Clippy、
fmt/标准Android构建通过。初次10秒突发性能断言失败与Windows relink锁定
失败保留，后续功能断言/独立有界观察及串行重跑通过；产品超时未扩大。

不签收本候选五轮恢复、秒级P95、生产容量、全天后台或匿名。下一片在原共享
载体中补有界多流/节点级连接共享，再推进L4同协议信箱，不能把每联系人两条
连接复制成产品网络，也不盲目调大上限。原始样本与调用链见产品
`docs/implementation/AUTOMATIC_RECOVERY.md` 的 Healthy Mailbox Wake v1；
本地 `artifacts/healthy-mailbox/acceptance.json` SHA256
`62eccb14e450c63b96dce898ae7b287f0d2f081a7f4096daddcab13dff1144e3`。

## 2026-10-14 计划内设备值驻留：功能验证通过，CPU保护门未过，不晋级

产品输入`f1bbf47d`，AOEM输入`0273655d`；候选源码完整保存在AOEM历史
`da80f30654c7ce60ab6c6f73333cf0cbb5657d18`，不是当前正式库。复用原统一v10、
完整费用计划、checked-i1024解释器和常驻Vulkan命令运行时，新增通用设备绑定
适配：层间守卫、fallback、前序引用和中间值留在设备槽中，一次上传静态计划，
最终才投影/回读。没有NOV内核指令、Host回调搬迁、JSON结果或GPU旁路。
CPU仍做计划校验/布局，GPU层间屏障及最终Host状态树/发布仍保留；增加有界驻留
内存，不宣称内存减少。以上是候选行为，**未进入随仓正式SDK**。

相同node `4d6f4bbc…`、驱动、插电Windows/5090、四原节点四库、单RPC；65536
预签名、1024付款方各64 nonce/共享收款方、批1024、在途8192。终点仍是四节点
最终成功回执HTTP返回，P95/P99含观察。八轮全部65536成功，0业务失败、永久拒绝
和未完成，完整经济/历史/冷恢复通过。候选整数第一轮有2次观察通道错误，原样
保留；不是零传输错误，也没有从计时中扣除。失败经济语义另由反例验证。

|同条件两轮合并|旧SDK TPS|候选 TPS|变化|P95/P99逐组结论|
|---|---:|---:|---:|---|
|整数完整计划|4859.25|5613.95|+15.53%|两组均下降|
|CPU默认快路径|9217.02|8849.45|−3.99%|第一组略好，第二组明显变差|

CPU两组吞吐都降低。原因未定，但不得用整数收益抵消，也不追加挑选有利轮次。
按AOEM单产品规则撤回活动候选、保留完整Git实现与证据，不留下备用开关；正式
SDK仍为`31306037…`。因此当前产品仍有层间回读，不能把本轮候选写成已交付能力。
AOEM最终`94c214e1`已推送，运行源码与`0273655d`一致，保留的新多层/未导出故障
回归在恢复后通过。候选DLL、生成shader和本轮相关编译缓存已清理，可由Git重建；
不是删除业务数据或已接受成果。

最终候选DLL `9d78d398…`经7项原节点完整效应/费用反例/窗口诊断/u128/单入口/
AAB/离线追平重启验证；AOEM资源11项及显式5项Vulkan测试通过，独立12算子
BigInt对照通过。AAB是3诚实原进程加受控故障peer，不是实体多机。首个GLSL
大数组函数版本管线创建失败、改为同解释器词法共享后通过；初次oracle漏feature
匹配0测试不计通过。格式化后DLL哈希改变，已用最终构建重跑全部7项及八轮负载。

前一提交CI `38501072228`失败在测试`clone_on_copy`，不是共识失败；本轮只将
Copy类型的`ctx.clone()`等价改为`ctx`，严格node lib/tests Clippy通过。远端新CI
另验。49份草稿哈希无漂移，运行数据/旧证据不删除。
[八轮原始数据与逐轮尾延迟](../artifacts/audit/resident-plan-arena-20261014/measurements.json)、
[验证边界](../artifacts/audit/resident-plan-arena-20261014/validation.json)。
下一轮先有限定位连续两刀新DLL在CPU路径的负向原因，再决定是否重新接回这份
已有设备驻留实现；不重造代数计划、不扩大为共识/网络改写。百万TPS和证明仍未签收。

## 2026-10-14 NOVOCHAT 冷启动前台恢复已接原 NVMBOX04

产品实现 `NOVOCHAT@34070fd`，共享 network/协议/身份输入仍为 `6fbe812c`，
与本仓当前合并 HEAD 的相关源码树核对相同。本片不改共享网络或金融/AOEM语义。
原 Application owner 使用原加密目录 v6 保存启用/关闭选择，重开重新验证本地
身份 epoch、双方联系人 revision 与自主节点策略；不恢复旧 running/会话授权。
旧进程内确认不自动扩大为跨进程同意。正常关闭必须落盘后确认，Privacy/撤销保持。

正式 APK `227bf794` 原位安装 PKL110 Wi-Fi/PCLM50 蜂窝，无 ADB 业务转发。
5 次进程重开未点 Connect 即认证并补查；任务创建至认证单调耗时
4.758/5.311/4.240/4.452/9.587 秒，不冒充从用户打开到消息送达/P95。
68/69 两条新消息在对方进程退出时由原单 L3 寄存，随后独立领取和持久设备回执
确认，双方重开仍保留原历史/状态。B 主动关闭后重开 25 秒/8 样本保持 off/零
网络任务；force-stop 后 24.791 秒无自启，再明确打开才恢复。

本轮开始原 L3 进程已经退出、PID 被无关进程重用；以可执行文件身份核实后，
用原自有节点配置/密钥/信箱恢复，手机从 E211 自动接回。退出原因未确定，
不能据此签收 Windows 服务常驻。未改旧期限或重建 Publisher/邀请。
旧65不确定输入已归档且未重发；68第一次输入缺字在 Send 前被工具拒绝，
核验未发送后补齐原草稿并只提交一次。原始失败均留在产品报告。

Rust 冷开/安全门禁3项、目录10项、权限9项，JVM17项、Flutter定向26项、
analyze/Clippy/Android构建通过，分组有重叠不计为独立总数。
真实低内存回收、整机重启/后台唤醒、多联系人、L4/独立副本仍待；实际为外部
Iroh Relay，普通工程路径不等于匿名。下一片健康消息即时性，不重做网络体系。
完整调用链、数据边界和证据哈希见产品 `docs/implementation/AUTOMATIC_RECOVERY.md`
最新 Cold-Start 小节与忽略目录 `artifacts/cold-start/acceptance.json`。

## 2026-10-14 同层常驻图合批：完成实验，CPU保护门未通过，候选撤回

产品源码输入`f054342d`，测试期间保留快进至`f9cc1920`的独立经济文档更新；
同一node binary `4d6f4bbc…`、驱动、插电Windows，四原节点四库、单RPC，65536笔
预签名、1024付款方各64连续nonce/共享收款方、批1024、在途8192。确认终点仍是
四节点成功最终回执HTTP返回，P95/P99含观察，签名/启动/冷核对不计入。

复用原统一v10图与二进制结果，候选将同层独立程序块合为有界图提交；共享缓存
资源按物理身份合并，守卫及故障完整检查。没有新shader、NOV内核业务或GPU旁路。
等待数从各组块数之和变为最大块数；跨层CPU绑定、回读和上传字节均没有消失。

八轮全保留，均65536成功、0业务失败/永久拒绝/未完成/传输错误，经济与冷恢复通过。
合并按总成功数/总耗时：整数4479.80→5118.53 TPS（+14.26%）；
CPU9379.04→8929.45（−4.79%）。整数两组P95/P99改善，CPU两组均恶化。
CPU原因未定，不能用整数收益抵消，也不追加挑选有利样本：**本候选不晋级**。

候选AOEM源码留在历史`69bf37da`，四个运行时文件恢复`aab3f8bb`；新增别名、
多块、尾块、跳过非法分支、有效故障锁存/无旧结果回归保留。候选通过13项资源
及5项显式ignored测试、7项原节点完整效应/单入口/AAB/离线追平恢复测试。
AAB为三个诚实原进程加受控故障peer，不是四机部署。初次测试夹具的奇数精确
除法错误已更正、未放宽域检查；旧v5/v7微测因skip名称不匹配也运行，原日志保留
但不用于本轮性能结论。正式SDK从未替换，仍为`31306037…`的已签收库。
49份legacy/runtime草稿哈希保持。候选测试DLL清理，非删除历史/证据/运行数据。

[原始八轮和哈希](../artifacts/audit/resident-layer-batch-20261014/measurements.json)、
[验收与限制](../artifacts/audit/resident-layer-batch-20261014/validation.json)。
前一提交`f054342d`的主CI `38498613734`已cancelled，不是通过或测试失败。
本轮没有新的生产性能升级，不签收完整GPU驻留、业务证明或百万TPS。

## 2026-10-14 AOEM 常驻层间交换：调度/分配减少，尚未消除层间搬运

产品输入 `6fbe812c`，保持原 `node → exec → bindings → AOEM` 完整效应计划，
本轮没有更改NOV业务、共识、CPU默认或负载口径。AOEM从`7fae6f32`安全快进至
`2c59a08f`（保留独立AI更新），本轮通用实现提交`aab3f8bb`已推送。
随仓Windows FULLMAX为44705792字节，SHA256
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`。
Linux与独立插件未更新，不宣称它们完成本项。

实际删除的工作：原composition每有效分组块的上传、执行、回读3次提交/等待，
改成原有序CopyRegion/Dispatch批的1次提交/等待；2个传输缓冲在首次使用创建，
此后随原资源复用，纳入预算及Drop。没有新增shader、C ABI或JSON正常结果。
共享图/直接设备边已核对，但当前仍由AOEM CPU解析层间守卫/引用；层间回读与
上传字节没有消失，驻留内存预算反而增加，不宣称完整GPU驻留或峰值内存下降。

插电Windows、同一节点binary `4d6f4bbc…`、同一冻结驱动、单RPC、四原节点四库，
65536预签名交易、1024付款方各64连续nonce/共享收款方、批1024、在途8192；
计时至四节点成功最终回执HTTP返回，签名/启动/冷恢复不计入，随后完整经济/
历史/冷恢复核对。正式测量关闭诊断；P95/P99包含客户端观察，不是纯共识延迟。

|正式检查点|成功TPS|P95毫秒（逐轮）|P99毫秒（逐轮）|
|---|---:|---|---|
|旧DLL整数两轮合并|3340.86|2671.695 / 2828.668|2908.913 / 2902.615|
|初次新DLL整数两轮合并|4636.16|2109.997 / 2055.658|2254.012 / 2177.318|
|旧DLL CPU两轮合并|9763.07|1078.638 / 1249.210|1115.012 / 1425.003|
|初次新DLL CPU两轮合并|8444.34|1225.143 / 1259.778|1260.455 / 1336.653|
|最终交付DLL CPU单轮|9373.16|1191.947|1249.724|
|最终交付DLL整数单轮|4487.81|2043.875|2193.824|

检查点整数+38.77%，CPU-13.51%，不以整数收益冲抵CPU负向样本。为有限归因，
构建同步后同源码/同features但无本补丁的参考DLL：CPU两轮9050.24/9084.07，
整数一轮3529.38；原新DLL同哈希追加CPU9785.52/9585.22。负向现象没有得到
稳定根因，不能签收CPU稳定改善或宣布不存在回归。参考仅冻结对照，未新增活动
产品/运行开关。恢复补丁重建后DLL哈希变化，因此最终2轮及全部7项正确性测试
独立复测；不把初次新DLL的4636.16成绩标成最终DLL成绩。

15个正式负载样本各65536成功，业务失败/永久拒绝/未完成/传输错误均0；失败
语义由独立正反依赖测试验证，不纳入成功TPS。AOEM原12算子独立BigInt/Vulkan
对照通过（debug）；release kernel libtest被17项已有AI测试编译错误阻断，不
冒充通过。FULLMAX release构建成功；资源测试13通过/4 ignored，其中3个真实
wire条件/二进制/依赖测试显式运行通过，微基准未运行。最终DLL四项计划/费用/
窗口/完整u128回归，加单入口、A/A/B、离线追平重启三项原节点测试全通过。
A/A/B为3诚实原进程加受控故障peer，不是四台实体机。49份受保护草稿哈希不变。
旧FULLMAX包装脚本在校验和/Windows哈希检查后，被历史累计ABI变更=true阻断；
其要求所有变更为false的旧契约未放宽，不计通过。另对实际随仓DLL做FFI探测及
单入口/失败/重启测试，结果随报告保存；本轮不重签收证明/隐私/worker能力。

原始报告、所有负向样本、命令、驱动/DLL哈希见
`../artifacts/audit/resident-layer-exchange-20261014/validation.json`及同目录。
上一提交`d787dc3c`主CI `38488844558`最终为cancelled，非通过/测试失败；本项
本机结果不替代远端CI。后续继续解决守卫/引用绑定与中间值设备驻留，不重做DAG、
二进制通道或完整业务裁决；百万TPS、完整证明、生产发布均未据本项签收。

## 2026-10-14 NVMBOX04 已接双Android持续运行

共享运行时为`6fbe812c`，产品接线在NOVOCHAT `291fc7c`。原Application owner/
IO owner显式选择V4，经过真实Finished后不再每60秒结束任务、每600秒申请运行
许可；每请求仍有边界、本地有限用途证明及实时身份/联系人/Privacy/撤销检查。
旧V2/01–03过期拒绝不变，没有新增账户、官方许可、网络或调度器。

本机原L3身份、DPAPI策略保留，现用rendezvous服务重新构建升级至6fbe812c，
信箱schema2/quick_check通过；旧schema1程序不得回写。正式APK70e525be原位
安装两台手机，各四份保护文件不变，无ADB转发隧道。PKL110 Wi-Fi同一个任务/
一次认证至少1125.372秒，跨原600秒边界仍收发；PCLM50蜂窝只在三次真实断网后
创建后继任务（共4任务、3次对应E1）。57–64及67九条测试消息持久交付，B重开
进程后回执保留。原联系人、密钥、历史未重建。65输入超时保留草稿，66误入搜索
未发送，不计成功；工具已补聊天页门禁，详细失败和样本在产品验收文档。

从Android实际网络可用到恢复认证约10.6/19.3/5.5秒，第三轮为3秒短中断，消息在
恢复网络后提交；前两轮有真实离线寄存。有限样本不称P95或完整生产稳定性。
节点另14次只读采样跨2声明，解析剔除正常变化instance后身份与地址一致；客户端
健康会话不随其声明更新拆除。实际仍使用外部Iroh Relay、单L3，不称自主匿名。

产品真实本机双owner用例145.551秒跨3声明，每端一次bind/Finished，原消息及
130秒后新消息持久确认，Privacy拒绝/实际关闭通过。信箱分组9项、未认证RUNNING
不能跳过启动期限1项、生命周期5项及严格Clippy/Flutter/Android检查通过；不是
新增共享组件测试数。冷开运行选择、多联系人、L4平台服务/独立副本、密钥长期
轮换/负载、匿名及秒级体验仍待，当前仅签收持续会话与本批三轮前台恢复。

## 2026-10-14 NVMBOX04 持续认证信箱：组件通过，手机尚未切换

输入 `35f60734`。本轮仅通用network；保留原mailbox、存储、Finished、节点owner。
调用链：`run_self_owned_node_v1` → `serve_self_owned_node_selected_v4` →
`ServiceLeaseHostV1::check_continuous_peer_v4` → `serve_mailbox_continuous_v4` →
原`MailboxServiceV1::execute`/SQLite。客户端原Endpoint owner的
`with_continuous_service_v4` → 同一CarrierStream → V4显式选择 → 原Finished →
加密Begin/Ready → 原Put/Fetch/Watch/Consume。没有Publisher或短期Offer签发。

本地pin与载体key绑定、撤销/失效owner仍检查；记录过期只失去新鲜定位声明资格，
不能使旧版租约有效，也不能代替本次Finished。节点60/90秒寻址存储仍原样回收，
健康V4信箱流不依赖其instance。旧01–03保留期限、64次会话上限及原签名流程。
V4改用服务owner共享令牌桶（默认64突发、2请求/秒，提供者可配置有界参数），
重连不清零；每请求含等待/帧/持久操作仍30秒，连续序号不归零；失败/取消终止流。
平台停止、live撤销可中断Watch。能力/单消息TTL/StoreReceipt/DeviceReceipt未改。

真实localhost QUIC结果（测试身份/载荷，**不是手机或匿名证据**）：
- `mailbox_continuous` 2项通过，146.67秒：同一scope/同一认证流134.596秒，3个
  正常声明代次，超过原90/120秒与64请求边界；70次初始Fetch后继续寄存/读取并
  核验持久StoreReceipt。客户端原记录已过期，未导入新租约、未重新Finished。
- 随后扩展负向集为7场景，定向1项13.32秒通过：错误客户端/节点pin、旧模式
  Begin、旧selector拒绝V4、有限scope不能升级、Watch中撤销、取消后不能复用。
- 新单元4项0.41秒通过：本地信任/旧声明/恢复撤销、共享速率界限、连续加密重放
  拒绝、owner关闭中断等待并实际释放UDP。信任49项51.41秒、信箱单元8项1.61秒、
  原信箱真实QUIC6项87.04秒通过；这些分组含重叠，不相加成独立总数。
- network库/两项集成目标严格Clippy、relay实际Windows入口cargo check、定向格式
  与diff检查通过。未运行金融测试，不据此评价远端node性能改动。
- 合并另一台机器的`d787dc3c`后，所测network输入树保持不变。随后补齐持续拨号
  的类型边界：单次拨号超时属于`IrohConnectUnavailableV1`，不再冒充不存在的
  owner期限。真实本机UDP黑洞1项15.20秒通过，确认失败、资源归还与owner关闭；
  本次底层先返回连接不可达，未触发外层30秒兜底，不能据此声称兜底定时分支已覆盖。

可复现：显式CARGO_TARGET_DIR=本仓target/runtime-rebuild，`cargo test --locked
-j 1 -p novovm-network --features iroh-transport,tls-transport --test mailbox_continuous
-- --test-threads=1 --nocapture`；旧回归改为`--test mailbox_runtime`；单元采用`--lib`
和`continuous`/`mailbox_rpc::`/`product_service_lease::tests::`对应筛选。

首轮两项失败保留：测试主动停止被当错误、Watch未先取游标因而立即返回，不能
验证等待中撤销。修正夹具为持久游标后重跑通过；不是放宽生产校验。实现过程中
测试helper/lifetime编译问题和clone-on-Copy lint已修正，不计失败构建为通过。

**仍未完成：** NOVOCHAT Application运行设置/跨任务会话迁移、双安卓原位升级及
实际Wi-Fi/蜂窝连续性/延迟；在用节点与手机仍是旧二进制。现有最大4载体连接和
保守请求速率只是有界组件配置，尚未作生产负载/公平性/密钥长期轮换验收。
同一协议L4平台接线、独立多副本、自有载体/匿名仍待；没有新增官方通行许可。

## 2026-10-14 完整计划精确导出：减少重复结果，有限收益，CPU归因未关闭

输入 `4b8affcf`，交付父提交 `35f60734`，独立网络更新已快进保留。生产改动仅在
原 node 的完整效应结果选择；不改AOEM、算子、费用/nonce规则、CPU默认或共识。
同一1024位常量或相同行/列引用只导出一次，再按原槽位映射；不按运行时数值合并
不同引用，不省略计划节点，不将常量绕过AOEM验证，不恢复Host业务判断或JSON。
真实AOEM核对全部16405个逻辑输出：同值1024行夹具导出2582个、异质13834个，
固定数值负载2099840→330496／1770752字节（另有32字节头）。不是所有实际候选
的固定压缩比，也不宣称减少了内部层间回读或峰值内存。

**检查点对照**（node `31f8c9f1…`→`4ddcc3a4…`，AOEM `d81cf004…`不变）：
Windows插电、四原节点/四库、单RPC node0、65536真实预签名、1024付款方×64nonce、
共享收款方、批1024/在途8192；profile关闭，终点仍为四节点成功最终回执HTTP返回。
签名生成、启动和冷恢复不计时；各轮经济/历史回执/冷恢复核对通过。
所有原始样本保留，不平均分位数；P95/P99包含查询观察调度，不是GPU或纯共识延迟。

| 样本 | 成功TPS | P95 ms | P99 ms |
| --- | ---: | ---: | ---: |
| 冻结CPU 1 | 9118.74 | 1097.868 | 1109.252 |
| 冻结CPU 2 | 9320.41 | 1152.190 | 1556.950 |
| 检查点CPU 1 | 9584.29 | 1215.376 | 1365.168 |
| 检查点CPU 2 | 9351.79 | 1222.766 | 1232.537 |
| 冻结整数 1 | 3109.29 | 2993.173 | 3240.631 |
| 冻结整数 2 | 3316.55 | 2851.489 | 2967.770 |
| 冻结整数 3（穿插重跑） | 3182.66 | 2918.013 | 2964.742 |
| 检查点整数 1 | 3329.13 | 2813.067 | 3054.449 |
| 检查点整数 2 | 3378.88 | 2910.896 | 3018.476 |

总成功数/总时间：整数3200.55→3353.82（+4.79%）；CPU9218.47→9466.61（+2.69%）。
尾延迟未一致改善，不能签收稳定提速或将CPU变化归功于未被CPU调用的导出投影。
开头CPU回放同时有轻量源码/格式核查、没有构建，不作为隔离变量因果实验。
更早冻结 `02d86588…` CPU另两次回放9832.92／9281.75，合并9549.39；首轮1次HTTP
传输错误已恢复，未丢交易。既证明同一程序也有波动，也**不能据此抹掉上轮负向结果**。
先前诊断的末次提交6137.268→6639.412ms、末次回执观察6823.690→7681.175ms，
差额跨提交与排空阶段；这些不是节点发布时间。CPU业务/树/持久化嵌套累计与零整数
调用没有给出因果解释。CPU回退归因仍开放，不更改窗口、超时或票数掩盖。

**最终构建复验单列：** 网络增量使最终node变为`4d6f4bbc…`，不沿用上述检查点数字。
同条件各一轮：CPU9682.95 TPS（P95/P99 1146.801/1166.046ms），完整整数3300.64 TPS
（2804.932/2939.745ms）。各65536成功、业务失败/永久拒绝/未完成均0，经济与冷恢复
通过。最终binary再通过整数单入口失败/重启、A/A/B、离线法定人数推进/追平/重启。
A/A/B是三个诚实原节点加一个受控故障peer，不是四台实体机器。功能测试驱动冻结
于检查点，实际启动的是最终binary；哈希及原始报告全部保留。

本片398项流水线回归通过（62项默认忽略），另显式4项真实AOEM回归及exec错误
二进制失败锁存回归；强区分容量拒绝/成功后继、窗口/诊断/溢出、完整u128均保留。
最终build/fmt通过，release库Clippy在检查点通过；49份草稿哈希无变化。
证据与准确哈希见[validation.json](../artifacts/audit/resident-effect-projection-20261014/validation.json)。

**仍未完成：** 完整整数计划明显慢于CPU；下一主要接合是AOEM计划内部层间搬运/
调度成本，不能继续只拿最终结果压缩解释整体差距。CPU负向归因单列，不恢复Host
接力、不重造DAG/二进制接口。未签收GPU连续驻留、完整业务证明、百万TPS、Linux/
多机或生产。前一提交CI为cancelled，本次CI以提交后的实际结论为准。

## 2026-10-14 完整代数计划共享与行块：整数改善，CPU负向未关闭

输入 `a6ededc1`，交付父提交 `f540d5c2`（两笔独立relay/手机更新已保留；未影响本轮
node依赖）。原产品路径、v10完整条件效应、AOIR0002、常驻owner与CPU默认不变。
AOEM DLL `d81cf004…` 未更换；冻结node `02d86588…`，交付node `31f8c9f1…`。
原始报告、源码及完整二进制哈希见
[validation.json](../artifacts/audit/resident-effect-sharing-20261014/validation.json)。

**实际减少的工作：** 编译器候选内共享完全相同的纯表达式，键含程序、守卫、全部
1024位输入/依赖引用及形状；不缓存状态准入，不合并交易和费用次数。1024行同值
编译对照节点22521→6689，异质输入22227→20572，两者都保留16405输出；共享/
未共享图由真实AOEM逐项对照一致。原composition行块通常128，容量/费用前缀/
分桶为1，候选上限仍1024。静态填充行估算60417→11266（同值）、60419→24196
（异质）；dispatch上界60→90、62→193，**不是减少所有调度，也不是设备时间**。

**正式负载不变：** Windows插电，四原节点/四库、单RPC入口，65536真实预签名，
1024付款方×64 nonce、共享收款方、批1024、在途8192、计时关闭。终点为四节点
成功最终回执返回；签名生成/启动/冷恢复不计时。所有八轮65536成功，业务失败、
永久拒绝及未完成均0；经济、历史回执与冷恢复通过。业务失败另由反例检查，不计
成功TPS。P95/P99含HTTP查询观察，不是纯执行延迟，下面列每轮，不平均分位数。

| 模式/轮次 | 旧TPS → 新TPS | P95 ms 旧→新 | P99 ms 旧→新 |
| --- | --- | --- | --- |
| CPU 1 | 9258.25 → 8843.81 | 1256.066 → 1151.704 | 1302.707 → 1485.883 |
| CPU 2 | 9486.22 → 7988.15 | 1112.006 → 1328.704 | 1168.415 → 1360.434 |
| 完整整数 1 | 2568.64 → 3177.93 | 3757.416 → 2802.697 | 3852.025 → 3042.404 |
| 完整整数 2 | 2739.94 → 3367.44 | 3563.104 → 2925.700 | 3614.185 → 2980.032 |

总成功数/总时间：整数2651.53→3269.94（+23.32%），CPU9370.85→8394.23
（−10.42%）。**CPU两组均负向，不签收CPU性能保住或整体稳定提速。** 独立开启
计时对照亦为9605.51→8533.17，不混入正式合并；两边四节点整数执行/接口次数均0，
实际CPU业务约683–704ms与688–708ms/节点，树与持久化未见同幅增时。新版观察请求
771072→945152，HTTP与idle累计更大，但可能是等待的结果而非原因，尚未定因；
嵌套/并发累计不得相加解释关键路径。不擅改查询窗口、计时器或共识来抵消负向。

397项流水线回归通过（62项默认忽略，未冒充执行）；另显式运行4项真实AOEM测试，
包括1024行共享/未共享全部输出、强区分费用容量正反依赖、窗口/诊断/溢出、完整u128。
交付node通过单入口失败/重启、A/A/B（三真实诚实节点+故障peer）、离线法定人数推进/
追平/重启。fmt与release库Clippy通过；49份草稿哈希无漂移。未改AOEM兄弟库。

**下一边界：** 保留这项纯计算共享；完整计划仍明显慢于CPU及旧预测路径，层间
回读/上传仍在。进一步核对同一计划的内部搬运/调度与结果导出成本，复用已有通用
资源能力，不恢复Host裁决，不重做图/二进制/共识。CPU负向继续单列；本片不是连续
GPU驻留、完整业务证明、百万TPS、Linux/多机或生产验收。上一提交CI取消并非通过；
本次提交CI单独检查，不将本机结果冒充远端全绿。

产品目标与当前交接入口：[NOVOVM 产品目标与交付主线](NOVOVM_DELIVERY_ALIGNMENT.md)。本轮按单机执行，历史 A/B 分工不是开工前置。安全和恢复门禁不替代高性能、隐匿资产、抗量子的产品交付。

2026-10-01 核对基线为 `main@9e104c0`。下文的 `b8aad0a` 加未提交工作区等文字是验收发生时的历史状态，相关更新现已进入该提交；不是当前工作区状态。后续以实际 HEAD 和新证据更新，不覆盖旧失败记录。

历史起点：开发分支 `feature/treasury-balance-backed-v2`，基线 `8025fd7`；该分支已获用户授权合入 main 并删除。当前只使用 main，新建分支须用户明确授权。

## 2026-10-14 Shared L3/L4 mailbox contribution admission and retention

Input `f540d5c2`, network/store only. `MailboxStore::open_contributor` uses the
existing SQLite mailbox, with a durable enabled choice and transient resource
readiness initially false on EVERY contributor open. The install default only
applies when creating the choice; an existing opt-out wins. `open` retains the
L3 ready default but likewise cannot override a persisted opt-out.

Schema 1 to 2 migration is transactional and preserves payloads, IDs, original
expiry, capability hashes, replay tombstones, clock guard and notification
positions. Missing/corrupt version-2 policy is an error, not reenrollment.
Upgrade must stop the prior store owner first; old binaries must not write the
new schema. No running node database was migrated in this round.

Local `MailboxServiceV1` owner updates use the SAME fair commit queue as RPC.
After disable acknowledges, new Register/Put/retention-extending Renew returns
the existing Quota response (zero available new capacity). Exact retries of
already committed registration/Put still reconcile lost ACKs; Fetch, Watch,
Consume, Revoke and original-TTL sweep remain available under live execution
authority. No wire message, global permission, monetary ledger or background
execution authorization was added. Resource readiness cannot undo user opt-out.
Retention counters are local status, not verified service rewards or evidence
of an independent durable replica. Owners must keep polling queued RPC and
control futures together; failure/cancellation is not an acknowledged update.

Verification on Windows, `iroh-transport,tls-transport`, serial tests:
- mailbox_store: 7 PASS (0.68s), including old-schema real-content migration,
  failed update, missing policy, restart, limits, per-device retention, exact
  expiry, no consumed-object resurrection and clock rollback rejection.
- mailbox_rpc unit: 6 PASS (1.49s). Forced BOTH orders on the actual SQLite
  commit queue: stop first rejects; Put first retains a verifiable StoreReceipt.
- mailbox_runtime: 6 PASS (85.22s). Actual localhost QUIC peers use the SAME
  authenticated RPC before/after contribution opt-out and service/store reopen;
  a departed sender is not needed for either recipient mailbox to fetch/consume.
  Wrong capabilities remain rejected. Existing signed-window, revocation,
  framing, request-cap and no-downgrade cases also PASS.
- Scoped lib/test Clippy with `-D warnings` PASS. Initial compile rejected a
  u64 SQLite read; now reads signed SQLite integers with checked conversion.

Android contribution platform/owner/UI wiring, mobile inbound reachability,
actual third-device forwarding/storage, independent replicas and repair remain
unaccepted. The currently installed own node/phone packages are unchanged;
this component gate is not phone deployment, privacy or production acceptance.

## 2026-10-14 Lease-bounded authenticated mailbox window

Android follow-through (NOVOCHAT regular APK d87c7e24): existing pending48
recovered without another Send. New49-56 all gained device durable receipts
within ONE pair of original ten-minute engineering grants. Three cellular
cuts (20.133/12.114/3.113s) and a new reverse message after each recovered
without manual reconnect. Healthy signed windows reached about53.9s; one
Put committed34.576s into the same original task. Both clients consumed the
existing E219 boundary statuses after the decoder fix, ending normally at
their original permission deadlines (14/17 tasks, zero native_reply_rejected
or trace_unavailable). App restart retained recent messages and Delivered.
One endpoint bind per process; QUIC opened/reused1/12 and5/11. The first
recovery included another network-generation failure, retained in the report.
Online new-message device receipts3.986-7.311s; restore-to-receipt
19.201/8.929/9.524s, not P95. Single own L3 plus external Iroh Relay remains;
this is NOT production latency, redundancy or anonymity acceptance.
Phone keys/identity and node protected files unchanged. Complete failure and
timing evidence is in NOVOCHAT AUTOMATIC_RECOVERY.md and ignored window logs.

Windows follow-through: the first installed Android sample still received a
30s offer. The real `rendezvous_self_owned_owner` entry had a hardcoded
`with_lease_limit(30_000)` omitted by the component fixture. Removed that
admission-era override: common 120s policy is further bounded by the original
90s signed generation. Old offers/keys/config/history are untouched. The real
Windows process test now asserts fresh offers exceed 30s and never exceed 90s.
All seven owner process tests PASS (175.23s), including migration, two instance
turns, restart, revocation, missing keys, unavailable publisher and sequence
reservation; strict entry/test Clippy PASS. Own node upgraded in place with
four protected hashes unchanged. Phone A retained authenticated sessions for
about 53s and deposited the original message 48 after 30s in one stream.
Phone B exposed an existing Android E217-219 status-decoder omission after the
intentional node restart; this partial batch is NOT a recovery gate PASS and
StoreReceipt is NOT device delivery. Corrected follow-through is separately
recorded above; it does not turn the original failed batch into a PASS.

Input `67defadb`, network-only. `NVMBOX03` explicitly selects the existing
mailbox session with a signed-lease service window. No fallback to an older
selector. The same Finished authentication, message framing, capabilities,
durable cursor/pending snapshot, 64-request cap and continuous AEAD counters
apply. The owned listener retains its 30s unauthenticated admission; only after
Finished and BeginSession can the internal carrier seam admit one immutable
window, at most 120s and bounded by the ORIGINAL signed offer. Each request has
at most 30s IO including notification wait/storage; activity cannot reset the
window. Finite/outbound scopes cannot use that seam. The host still checks lease
expiry per operation and its original local revocation policy continuously.

Real QUIC mailbox_runtime: four existing cases PASS (the own-node case now
retains ONE authenticated session for 64s, across the original 30s cutoff and
the node's 60s rendezvous rotation). Added expiry/revocation/wrong-identity
negative case PASS, with no rejected registration committed. First negative
fixture aborted the whole endpoint before its close was delivered, and hit
the client scope deadline; corrected it to retain the listener like the real
node while closing the rejected stream. This was a test lifetime correction,
not a production retry/authorization change. Finite/outbound rejection QUIC
test and lib/test-target strict Clippy PASS. Android integration is not yet
accepted; a longer authorized service window is not cross-task authentication
reuse, lease renewal, a privacy route, P95 evidence or production certification.

## 2026-10-14 Mailbox maintenance and queued-client progress

Input `f9baf7a8`, network-only. The installed relay-only node process remained
alive but status/source publication stopped at 03:06 local and both Android
clients subsequently failed to connect. Absence of UDP is expected in relay-only
mode and is NOT evidence of endpoint loss. No process-stack capture was taken.

The actual MailboxServiceV1 fair one-permit store queue reproduces an owner
deadlock: a request queues before sweep; a worker releases its permit to that
request; the owner awaits sweep inside its timer branch without polling the
request. This source condition is consistent with the observed total stall,
not a claim of a captured historic thread stack. The existing owned listener now
polls ONE borrowed sweeping future beside incoming/active requests and maintenance.
No detached scheduler, concurrent sweeps, authority extension or protocol change;
cleanup errors remain fatal and owner stop drops the future.

Five mailbox_rpc tests passed (1.18s), including deterministic queue contention,
notification races, cancellation/revocation and session replay. Four mailbox_runtime
real QUIC tests passed (8.85s), including existing owned listener dispatch,
persistent offline fetch/restart/full storage, legacy refusal and session budget.
Scoped network/relay Clippy `-D warnings` passed (23.05s); node binary built.
Reproduce with `cargo test --locked -p novovm-network --features iroh-transport,tls-transport --lib mailbox_rpc -- --test-threads=1`
and the same package/features `--test mailbox_runtime -- --test-threads=1`.
Evidence: artifacts/mailbox-maintenance-{all-tests,runtime-tests,clippy,build}.log.

Same original notebook node/config restarted with the fix at 03:39 local;
keys.dpapi, self-owned-policy.dpapi, identity.json and owner.journal hashes unchanged.
No mailbox reset, new enrollment or changed invitation. Node publication resumed.
NOVOCHAT `4a537fb` installed APK `9114cf8e...` then began a fresh bounded ten-minute
engineering session on Wi-Fi/cellular phones. Original pending message17 recovered
with inbox/device-receipt commits; a new message18 after session13 reached the
recipient. The full ten-minute grant then completed with 26 tasks on each phone
and normal expired/tasksFinished=true; original17 and new18 sender Delivered
states visually verified. One final cellular E8 had grant/task remaining=0.
No outage injected in this batch; not a five-round or long-term gate. Node
publication kept advancing.
Android currently rejects six newly added fixed mailbox trace event names, losing
the remainder of those drained batches; NOVOCHAT is fixing that contract before
full timing/count acceptance. Partial traces must not imply absence of failures.
Local node process metadata and custody comparison: artifacts/mailbox-maintenance-*,
phone traces/screens in NOVOCHAT artifacts/mailbox-maintenance-*.
Still a third-party Iroh relay path; no self-owned transport/anonymity/P95 claim.

## 2026-10-14 Bounded service journal compaction and installed migration

Input `92e56e83`, networking only. ServiceLeaseFileV1 now reads NVLSJ001/002,
compacts at512KiB while keeping the original16MiB read limit. Exact latest
checkpoint plus authenticated predecessor digest/generation replaces redundant
old snapshots; no sequence, revocation, trust or migration-state reset. Stable
owner lock spans same-directory publication and reopening; uncertain writes
freeze the owner. Staging is never a recovery fallback. Old binaries must stop
before upgrade and cannot write new format. No parallel store/authority.

54 service-lease/file tests passed52.46s, including six real child exit points,
four repeated compactions, single writer, missing/corrupt primary refusal and
700 signed renewals followed by durable revocation. Seven Windows owner-process
tests passed175.16s, including source migration, two instance rotations, custody
restarts and revocation. Scoped Clippy -D warnings passed32.71s. Repro:
`cargo test --locked -p novovm-network --features iroh-transport,tls-transport --lib product_service_lease -- --test-threads=1`
and `cargo test --locked -p novovm-relay --features iroh-transport --test rendezvous_owner_process -- --test-threads=1`.

Original installed node was stopped, encrypted journal backed up locally, then
upgraded under same config/custody. Before16,239,293bytes/3097frames/sequence22941;
after18,648bytes/sequence22943. First new checkpoint exactly matches previous
latest plaintext hash, new anchor matches prior chain tail. Trust/revocations/
prior endorsement and four protected files unchanged. Restarted the same node
again; no re-enrollment or phone identity replacement. Mailbox data not rewritten.
Evidence: artifacts/service-compaction-{before,migration,protected-before,
protected-after,node-process}.json, encrypted before.journal, node stdout/stderr,
tests[-v2..v5].log, windows-rename.log, clippy[-v2].log, owner-tests.log.

Failures retained: direct Win32 unsafe code rejected by crate policy, replaced
with pinned atomicwrites0.4.4; open data handles denied replacement even unlocked,
confirmed standalone before closing them under stable ownership; two assertions
wrongly opened locked files and were corrected without weakening locks. One
immediate stopped-node copy met delayed OS lock release; later authenticated
backup succeeded before upgrade. No claim of power-cut/all-filesystem testing,
freshness against full old backup replay, long-term production or anonymity.

## 2026-10-14 Android bounded mailbox session gate

SUPERVM network `9333e9df`, merged build `c0d3d01f`; NOVOCHAT `bf86aec`.
PKL110 Wi-Fi / PCLM50 cellular, same original ten-minute grant, no USB tunnels.
Five interruptions recovered without manual reconnect; all ten new messages
received durable device receipts before grant end. Follow-up messages after
rounds 2 and 5 also delivered. Force-stop/reopen retained all ten on both phones,
old contacts/history and four protected identity/key files per phone unchanged.

Approximate seconds from cellular enabled to authentication/inbox/sender receipt:
R1 9.11/12.23/13.81; R2 29.28/31.83/33.40; R3 17.18/19.60/47.07;
R4 14.20/19.41/23.61; R5 13.06/15.35/17.70. R3 deliberately took sender offline
after StoreReceipt; recipient independently fetched before sender returned.
Phone monotonic/host anchor mapping assumes unchanged host wall clock; no P95
claim. A/B completed 24/22 tasks, normal expired/tasksFinished=true at grant end.
Endpoint binds1/1; QUIC opened2/6, reused22/15; sessions ready23/20, finished22/16.
292/234 ended RPCs median965.5/1022.5ms includes Watch and errors, not deliveryP95.
E1 intentional network cuts, one cellular E211 connect failure and end-grant
E215 remain in evidence. New legal attempts resumed; Delivered never regressed.

Android/core: 3 mailbox regressions, scoped Clippy/fmt/build passed. Windows node
first build omitted iroh-transport and failed; corrected feature build passed.
Phone logs and latency/reopen/install/outage records are local NOVOCHAT artifacts/
mailbox-session-*; detailed contract in NOVOCHAT DELIVERY_AND_MAILBOX.md.
All observed authenticated paths used Relay with third-party Iroh dependency.
Five-round foreground gate passes this batch; low latency, long-term production,
L4 independent replicas, self-owned transport and anonymity remain unaccepted.
Installed service.journal near16MiB now makes safe bounded compaction the next
node-lifetime blocker; never reset signing/revocation history to keep it running.

## 2026-10-14 Bounded authenticated mailbox sessions

Input `f42510b9`, NOVOCHAT `41c3f57`. Real phones retained three newly delivered
messages across reopen, but QUIC reuse took milliseconds while each signed offer
and Finished/request/finish cycle took seconds. This slice extends the SAME
NVMBOX02 with explicit BeginSession/EndSession. Existing single-request clients
remain supported; old NVMBOX01 rejects negotiation, with no silent fallback.
One fresh Finished channel serves up to 64 requests under the unchanged 30s
stream/offer authority. Each request checks carrier authority and capability;
session counters/nonces never reset between requests. Cancellation/transport
error poisons the client session. EndSession completes before pooling transport.
Expired offers, bad identity or revocation gain no new permission from reuse.

Validation: four mailbox_runtime real QUIC tests (legacy and session on one
owned listener, 64 KiB multi-frame requests/responses, duplicate receipt, wrong
read key, revocation and 64-request budget); four mailbox_rpc tests (notification
race, canceled watch, live revocation, session poisoning and envelope replay),
scoped Clippy -D warnings passed. Initial two negative fixtures dropped the
entire server endpoint immediately upon deliberate rejection; peer waited until
scope expiry. Fixtures now keep the endpoint through its existing close barrier,
allowing the deliberate close to reach the peer. Product deadlines unchanged;
original failed log retained. Local logs: mailbox-session-runtime-tests.log,
mailbox-session-fault-tests[-v2].log, mailbox-session-clippy.log.
Android session wiring/latency not yet accepted. No finance, carrier replacement,
new node/account/scheduler, vendor Push or anonymity claim. Existing four-peer
admission, durable journal rotation, real L4/independent replicas remain open.
## 2026-10-14 NOV 完整条件效应计划：功能接通，整数性能未签收

输入 `a80306cc`；开发中快进到 `f42510b9`，保留独立信箱更新。
AOEM 仍为 `7fae6f32` 对应随仓 FULLMAX，DLL SHA256
`d81cf004f22a4e0ad1aa2e2e4dc5f1b85078a66d4af31c9828c306baaaefebd2`。
本轮未修改 AOEM，不新增内核 NOV 指令、GPU 旁路、账本或签者。

实际路径：原 RPC/native_pipeline → `full_effects` 编译 → 原 exec/bindings
opcode113/AOIP0 v10 → 常驻八个受限纯程序 → AOIR0002 一次最终结果 →
原 `stage_results` 编码/批树 → 原共识/发布。费用、窗口、饱和诊断计数、
失败后余额/nonce、最后诊断选择、逐笔分桶/回执数值由计划输出。
Host 仅编译不可变规则与依赖、检查输出标签/身份、物化规范记录，不重算
交易成败/余额/费用，不再委托 `finish` 收尾。CPU 默认及其 finish 仍保留。

并行策略：不可变父状态及所有报价的保守上界决定是否需要共享容量前缀；
AOEM 重算相同充分条件并返回必须为真的有效位，不缓存成功结论。容量敏感
路径只串联真实费用金额/计数依赖；纯入账及已逐笔舍入的效应平衡归并。
相同政策/报价的分桶只在 AOEM 算一次，各笔仍按自己的实际结算位选择；
不是聚合后舍入。分桶资源行容量1，避免把一个不变量填充为1024行。

实际反例：A→B10000/fee46，B初值45、B→C1/fee45。容量45时A被拒，
B手续费成功但转账资金不足；容量91时两笔成功。两组状态、费用、诊断、
nonce、回执及根与CPU一致，整数重算0、第二业务回调0、完整委托1。
另有九组窗口/暂停/报价/资金/计数/容量/诊断边界和完整u128原算子对照。
初始误用 exact-div 被真实测试拒绝，已使用原 floor-div，不放宽检查。

验证与性能原始记录位于
[resident-complete-effects-20261014](../artifacts/audit/resident-complete-effects-20261014/)。
完整测试与构建身份见该目录 `validation.json`。本机 Windows/插电，四个真实
原节点/独立AOEM库、单RPC、65536笔真实签名、1024付款方×64nonce、共享收款方，
批上限1024、在途8192，诊断关闭。终点仍为四节点最终回执HTTP返回；签名生成、
启动、冷核对不在计时窗，冷恢复另验。业务失败负载与成功TPS分列，不能以拒绝
凑吞吐。旧a803和新f425含独立网络增量，不把全部差异归因于业务编译。

| 样本检查点 | CPU TPS / P95 / P99 ms | 整数 TPS / P95 / P99 ms |
| --- | --- | --- |
| 冻结旧节点a803 | 9176.67 / 1172.94 / 1304.60 | 7572.04 / 1425.18 / 1602.04 |
| 首版完整计划（文件final-*，不是最终交付） | 9244.82 / 1255.03 / 1464.47 | 1688.51 / 5314.95 / 5627.74 |
| 去冗余计划（optimized-*，状态标签修正前） | 10351.52 / 1040.96 / 1046.73 | 2792.51 / 3512.18 / 3577.22 |
| 最终交付构建（delivery-*） | 9284.46 / 1041.13 / 1161.09 | 2777.71 / 3354.28 / 3602.79 |

上述每轮65536成功，0业务失败、拒绝、未完成，完整经济/历史查询/冷恢复通过。
负向首版和修正样本全部保留；单轮不构成稳定CPU提速证明，整数性能明显未达标。
最终节点 SHA256 `d254ad719a20be8fc1dc32b3ce964784bb553b9d3b61bf540684a4a118777a8c`，
测试程序 `23ab95c1b4aca50db6c36337000767539088de0f9aa4112be627de7b15fa79c2`。
最终构建已复跑：原单入口/失败/重启4.26秒、真实A/A/B13.46秒、离线/追赶/重启
49.82秒；三项实际整数对照全部通过；native_pipeline 395通过/61忽略（相关六项
原生忽略测试已单独运行），fmt、定向Release Clippy -D warnings、原节点构建通过。
负载始终使用同一冻结driver，最终CPU7.058679秒、整数23.593570秒；P95/P99含
HTTP观察调度。相对冻结旧节点CPU +1.17%、整数 -63.32%，不签收稳定CPU提速，
更不能用CPU保留掩盖整数回退。测试结束的relay连接关闭记录保留，不当作交易拒绝。
49份原草稿SHA复核0变化。远端CI须以提交后的实际运行结果为准，本机通过不等于CI通过。

边界：现行 Transfer 完整业务裁决已接通，不等于所有主链工作设备化。
AOEM 内部仍按层回读/上传；没有新连续GPU驻留或完整业务证明验收。
未重新做物理设备诊断，正式测量关闭诊断的 physical_gpu_verified=false 表示
本轮未观测，不将 Vulkan 路径名当硬件证明。签名/捕获、规范编码、SHA256树、
共识发布仍是原实现。大候选仍受既有节点/导出/字节预算限制，4096笔完整计划
容量未验收。废弃预测编译器dataflow.rs删除，Git可恢复；不删除legacy/runtime
或草稿。CPU仍默认，整数不可因本项功能通过就晋升生产。

## 2026-10-14 Mailbox continuous connection and durable notification positions

Input `a80306cc`; implementation `e073bca2`. No finance/AOEM/consensus changes.
Existing self-owned listener selects explicit `NVMBOX02` for continuous mailbox
transport; `NVMBOX01` remains supported without silent client fallback. A completed
inbound stream hands its admitted QUIC connection to the next independent operation.
Idle wait is bounded at 60s; each new stream retains its own 30s IO budget, signed
service offer and Finished-bound authentication. This is sequential stream reuse,
not unrestricted multiplexing or indefinite business authorization.

Store schema 0 migrates to 1 transactionally without replacing existing objects,
capabilities or TTL. Each new Put advances a private mailbox sequence in the same
transaction. Duplicate Put does not advance it. Recreation changes generation;
future positions in the same generation reject. Watch proves read capability,
accepts at most four distinct boxes and waits at most 10s. It subscribes before
the durable snapshot, wakes after commit and rechecks expiry/revocation. Waiting
holds no storage lock. Notify is advisory: Fetch, device durable receipt, dedup
and fixed TTL remain authoritative. No global traffic counter or chat identity is
sent to subscribers. Do not write schema-1 files using the old schema-0 binary.

Validation: mailbox_runtime 2 (including seven sequential real QUIC operations
on one connection), mailbox_store 5 (migration, restart, failed transaction,
dedup, TTL/recreation), notification 2 (commit before/during wait, wrong capability,
future cursor, cancellation and live revocation), existing connection-reuse 3;
scoped Clippy -D warnings passed. Ignored local logs: mailbox-realtime-tests.log,
mailbox-notify-tests.log, mailbox-reuse-tests.log, mailbox-realtime-clippy.log.
The first reuse filter selected zero tests; corrected iroh_transport::reuse
actually executed all three. Android deployment and latency pending at this commit.

Not production/privacy signoff. Node admission still bounds four connections
and existing approved clients. Scale/fairness, journal rotation, independent
L3/L4 replicas, self-owned carriers and qualified anonymity remain open. During
sync the remote AOEM LFS download stalled; its financial DLL is an unhydrated
pointer, not verified by these network-only tests.

## 2026-10-14 Guarded cross-program composition (partial step 2; fee/effect compiler OPEN)

Input SUPERVM `32aee585`; generic AOEM adapter committed/pushed as `7fae6f32`.
Original node -> exec -> bindings -> opcode113 now uses AOIP0 v10 and AOIR0002
for its existing integer prediction plan. The SAME resident pure programs can
be composed with distinct signatures, prior-output references, Boolean execution
guards and explicit fallback values. Local inputs+outputs<=16 is unchanged.
False guards skip execution/domain checks, rather than evaluate both branches.
Final exports use the same context/resource/sequence single-take/free contract.
No NOV instruction, Host-loop callback, new shader, ledger or consensus was added.

**Not complete:** current product predictions still use one Transfer program.
Fees/window/diagnostics/settlement refusal and final effects remain in `finish()`;
the second CPU callback remains. New cross-program capability is NOT evidence
that those business rules have been compiled. Internal levels still read back,
bind and upload; no continuous GPU residency or full business proof is claimed.
Next work is directly the fee/conditional-effect compiler using this adapter,
not another DAG, result channel, device diagnosis or network rewrite.

The requested discriminating fixture now exists and passes on real AOEM:
A->B10000 fee46; B starts45 and sends1 fee45. Treasury remaining45 rejects A,
but B's own fee succeeds and its business fails for missing funds. Remaining91
makes both succeed. Full balances/nonce/fees/diagnostics/receipts/root match CPU.
**Negative case still records one finish repair; positive records zero.** This
exposes the remaining work instead of hiding it behind two capacity refusals.

Checks: 17 AOEM resource tests including actual skipped/selected divide-zero,
failure latch, binary ownership, bounded distinct-arity parsing and compatibility;
65 exec tests passed/25 ignored; node pipeline395 passed/60 ignored; Release
node build, fmt and scoped Clippy -D warnings passed. Full-u128 oracle passed.
Actual integer original-node single-ingress failure/restart5.74s, A/A/B14.07s
(three honest original processes plus fault peer), offline quorum/catchup/restart
48.52s passed. Windows SDK core/header updated,117 checksum entries verified;
Linux and sidecars unchanged.49 drafts hash-identical.

### Finite same-condition regression comparison (not stable speedup)

Windows Release, AC before/after, diagnostics off, no concurrent builds/tests;
single RPC, four original validators,65536 signed transactions,1024/batch,
1024 payers x64 nonce, shared recipient,8192 inflight. Same frozen driver
`07090a3c...`; old node `63aaf36c...`/DLL `f85e7ebf...`, final node
`6527d5ad...`/DLL `d81cf004...`. Each row:65536 success,0 business failure,
0 permanent reject,0 unresolved; economics/history/cold recovery passed.
Business failures are exercised separately by correctness fixtures, not counted
as successful TPS. P95/P99 include first submission to slowest-node receipt HTTP
observation, not pure consensus/device latency. One sample per cell:

| Path | Before TPS | After TPS | Before P95/P99 ms | After P95/P99 ms |
| --- | ---: | ---: | --- | --- |
| CPU default | 9894.86 | 9646.13 | 1009.732 / 1154.621 | 1113.256 / 1136.245 |
| Integer | 7975.13 | 8302.40 | 1211.984 / 1451.224 | 1292.362 / 1487.775 |

CPU -2.51%, integer +4.10%; tails are mixed/worse. No stable acceleration
signoff and no resolution of earlier CPU variation. Do not compare the single
integer sample to historical best as an equivalent benchmark.

Failed setup evidence retained: first product run rejected the not-yet-updated
SDK trust manifest; frozen integer run initially used a renamed DLL path that
selected the Linux manifest entry. Corrected manifests/path matching (no hash
checks bypassed), then reran successfully. An initial native test lacked the
required device selector; unchanged suite passed after explicit selectors.
Reports and exact artifacts: [validation](../artifacts/audit/resident-integer-composition-20261014/validation.json).
New delivery CI is separate from local checks; no mainnet/complete-plan signoff.

## 2026-10-13 Explicit integer dependency plan (partial step 2, NOT complete candidate execution)

Input SUPERVM `c48dd7bd`; AOEM generic adapter pushed as `6cedc1e3`.
The original node now compiles constant/prior-output bindings instead of advancing
`ComponentState` after each integer wave. Exec submits one bounded AOIP0 v9
plan through the SAME opcode113/resident outcome operator; final AOIR0001 result
ownership, full-width checks, failure latch and single-take sequence are retained.
AOEM receives an explicit acyclic dependency graph, not NOV callbacks or rules.
No new node, signer, ledger, shader or consensus protocol was introduced.

What is **not** finished: the rows are still predictions; global fee capacity,
ordered repair, receipt/effect generation and `finish()` remain in the existing
AOEM CPU callback. Internal dataflow levels still read back/bind/upload limbs;
this is not continuous GPU residency. Complete plan/proof/production acceptance
remain OPEN. Next implementation must compile the fee window, failure priority,
per-transaction rounding, capacity and effects, not wrap this finish callback.

The same-candidate real-AOEM fixture now includes incoming funds, spending,
consecutive nonce, a business failure consuming nonce, and a subsequent spend
(four dependency levels). It matches CPU balances/nonce/fees/receipts/state root.
Treasury overflow with dependent repair also matches, **but that repair still
uses finish**, so the test is not evidence of complete fee-plan execution.
Normal production diagnostics time the binary plan; old v7 JSON detailed profile
is test compatibility only and cannot explain v9 device time.

Verification: AOEM 15 resource tests including real DAG and v7/v8 compatibility,
plus rejected-predecessor/no-result/poison retest; exec 64 passed/25 ignored;
node pipeline 395 passed/60 ignored; node Release build, fmt and strict scoped
Clippy passed. Bundled full-u128 oracle passed. Real integer-mode single-ingress
failure/restart, A/A/B (three honest original processes plus fault peer), and
offline quorum/catchup/restart passed (5.77s, 13.26s, 47.78s respectively).
No physical four-machine or Linux integer-mode qualification is implied.

### Frozen CPU regression investigation (separate from final performance)

Same frozen load driver, AC, single ingress, 65,536 signatures, same economics
and four-node final receipts; two rounds per old/new node × old/new DLL, reverse
second order. Old node `92160d3e…`, new c48 delivery node `dca03e2e…`;
old DLL `3c65bd83…`, binary-result DLL `9e39b9d…`. Pooled successful TPS:
old/old **9110.33**, old/new **9435.86**, new/old **9617.31**, new/new **9157.75**.
The same new/new pair varied **8437.17 → 10012.90**. Effects change direction
across combinations; this does not identify a stable culprit or prove no
regression. Prior negative CPU samples remain valid observations and unresolved.
This uses the final c48 delivery node, not the earlier `4a03f542…` checkpoint.

### Final paired original-node performance

Frozen baseline is c48 delivery node `dca03e2e…` / DLL `9e39b9d…`.
Final node **`63aaf36c…`**, DLL **`f85e7ebf…`**, same frozen driver
`07090a3c…`; full hashes in validation. Windows Release, AC before/after,
diagnostics off, no concurrent builds/tests; original single RPC, four validators,
65,536 real signatures, batches 1024, 1024 payers × 64 nonces, shared recipient,
8192 in flight. All 8 rounds: **65,536 successful, 0 business failures, 0 permanent
rejects, 0 unresolved**; economics, history eviction and cold recovery passed.
Business failure behavior is verified by correctness fixtures, not counted as
successful load. P95/P99 below include client submission and slowest-node HTTP
receipt observation, not pure consensus/device time.

| Round | Successful TPS | P95 ms | P99 ms |
| --- | ---: | ---: | ---: |
| baseline-cpu-1 | 9146.88 | 1266.813 | 1424.077 |
| baseline-cpu-2 | 9094.54 | 1164.488 | 1228.883 |
| baseline-integer-1 | 8983.48 | 1118.689 | 1285.898 |
| baseline-integer-2 | 8974.59 | 1156.538 | 1299.389 |
| final-cpu-1 | 9219.52 | 1136.906 | 1242.703 |
| final-cpu-2 | 9631.30 | 1049.928 | 1249.098 |
| final-integer-1 | 8960.33 | 1167.152 | 1335.828 |
| final-integer-2 | 9031.90 | 1121.964 | 1257.791 |

Pooled (total successes / total measured seconds): CPU **9120.63 → 9420.91**
(+3.29%), integer **8979.03 → 8995.97** (+0.19%). These are finite observations,
not stable acceleration. Integer tails do not improve consistently; the common
load has little intra-candidate dependency, so it does not demonstrate a speedup
from removing dependent-wave resubmissions. CPU default remains preserved.

SDK: Windows FULLMAX core/header only advanced; ABI/hash/capability load verified,
117 checksum entries matched. Linux and sidecars retained, not requalified.
49 protected drafts hash-identical. Source/contract, all 16 load reports and three
fault/recovery reports: [validation](../artifacts/audit/resident-integer-dataflow-20261013/validation.json).
Current CI for this delivery must be checked separately; local tests are not CI.

## 2026-10-13 Unified integer binary result slice (not complete algebraic plan)

User approved the four-part [mapping and order](NOVOVM_ALGEBRAIC_PLAN_INTEGRATION.md)
before code changes. Input SUPERVM `8f8896b0`, AOEM `5e555e1a`; preserved remote
SUPERVM changes through `57869267` during measurement, then fast-forwarded the
independent mailbox commit `a53c9590` before delivery. AOEM adapter is pushed as
`8f12c805`; no NOV business was added to AOEM. Original node/exec/controller,
fees, state tree, consensus and publication algorithms were not replaced.

The same opcode 113 now accepts explicit AOIP0 v8 Run against the existing
resident i1024 resource. Native typed rows become bounded AOIR0001 bytes;
exec consumes them through an exact context/resource/request-sequence single-take
buffer with the existing DLL free contract. Normal integer execution no longer
materializes, serializes or parses the numerical JSON result tree. Full 1024-bit
values, shape/tags/fault locations, execution completion and failure poisoning
remain checked. A 1024-row/five-output binary result is 671,784 bytes. This is
not zero-copy: native rows, binary buffer and final typed rows still exist.
No allocation/peak-memory reduction or new facade timing was measured.

The existing `aoem_integer_v7` configuration names the resource lifecycle;
its normal Run explicitly uses v8. Old v7 and opt-in profile JSON remain
compatible, not a silent fallback. Missing binary capability fails explicitly.
CPU remains the default. Windows FULLMAX SDK was updated (DLL SHA256
`9e39b9d93542f73a04cd8305bfc8d04539d6a11175f4c549276b5660959bf0c9`);
Linux packaging was not changed. All 117 SDK checksum entries match.

### Real product comparison and regression boundary

All eight [raw reports and validation](../artifacts/audit/resident-integer-binary-20261013/validation.json)
are retained. Windows Release, AC confirmed before/after, diagnostics off,
four original node processes/databases, single RPC, 65,536 pre-signed transfers,
1024 payers × 64 nonces, shared recipient, batch 1024, in-flight 8192.
Unchanged endpoint: four matching final receipts plus economics/cold recovery.
No concurrent builds/tests during formal runs. Two baseline/final interleaved
pairs per mode; measured candidate node hash `4a03f542…`, frozen baseline
`92160d3e…`. These eight samples precede the final mailbox fast-forward;
they must not be relabeled as measurements of the subsequent rebuilt binary.

| Mode / pair | Old → new successful TPS | Old → new P95 ms | Old → new P99 ms |
| --- | --- | --- | --- |
| CPU 1 | 9914.85 → 8363.96 | 1176.607 → 1437.937 | 1269.597 → 1474.296 |
| CPU 2 | 8987.36 → 8818.61 | 1099.813 → 1275.586 | 1276.841 → 1408.067 |
| Integer 1 | 7267.56 → 8859.44 | 1382.070 → 1152.529 | 1427.110 → 1419.153 |
| Integer 2 | 7718.82 → 9689.84 | 1387.718 → 945.039 | 1442.117 → 1411.415 |

Total successes / total time: integer **7486.39 → 9256.05 TPS (+23.64%)**;
CPU **9428.35 → 8585.27 TPS (-8.94%)**. Every run completed 65,536 successes,
zero business failures, permanent rejects or unresolved transactions. One old
integer run recorded one transport error; retained, not hidden. Business
failures are tested separately. P95/P99 include four-node HTTP receipt observation.
Integer improvement is a finite paired result, not stable capacity. CPU regression
is observed and unexplained; unchanged CPU source is not proof of no regression.
Do not attribute it to power, Windows or unrelated commits without evidence.

### Correctness and remaining work

- Format and strict node/exec/bindings Clippy pass; exec 63 pass / 25 ignored;
  node pipeline 395 pass / 60 ignored. AOEM resource suite 12 pass includes
  actual GPU v8/no-JSON, context identity, single take and old lifecycle tests.
- Installed DLL binary/JSON parity and full-u128 business parity pass; malformed
  length, shape, identity, fault and padding negatives plus poison/no-fallback
  tests retained. No economic width or failure-prefix rules were reduced.
- Final packaged binary single-ingress failure/restart test passed (5.79s),
  three honest original processes plus one fault-peer A/A/B passed (13.28s),
  offline quorum/catch-up/restart passed (48.42s). These are same-host functional
  checks, not multi-machine or production acceptance; all raw reports retained.
- 49 pre-existing legacy/runtime drafts have unchanged hashes. Incoming network
  work preserved. Final remote CI is not yet verified; local tests are not CI.

**Not complete:** node still advances component states between integer waves;
`finish` still runs fee settlement, prefix repair and state/receipt staging in
an AOEM CPU callback. No Host loop was moved into AOEM. Next integration must
express these dependencies, conditional effects and failures as a readable
generic plan, then bind its result to the existing candidate/publication path.
This result-channel slice does not sign off complete GPU business execution,
business proof, million-TPS capacity or production readiness.

Final mailbox integration note: `a53c9590` changed dependency feature resolution
and the linked node hash. Its private `StoreReceiptV1::committed` was only used
by Iroh RPC but compiled without that feature, failing default strict Clippy.
Gate the private function/imports with the existing `iroh-transport` feature;
no mailbox protocol or behavior change. Strict node/exec/bindings Clippy then
passed. The performance table remains the pre-fast-forward checkpoint, not a
claim of remeasured final-mailbox-build throughput. Final build/smoke identity
is separately recorded in validation JSON.
The delivered node hash is `dca03e2e…`; the retained original test driver
`07090a3c…` launched that exact binary and passed single-ingress failure/restart
(5.84s), A/A/B (13.90s), and offline catch-up/restart (48.30s) again. These
`delivery-*.json` reports supplement rather than overwrite the measured checkpoint.

## 2026-10-13 Distributed Mailbox v1: shared native runtime, not Android acceptance

Input `57869267` / NOVOCHAT `ab84a8b`. The existing bounded SQLite mailbox
prototype moves from NOVOCHAT to `novovm-network/src/mailbox_store.rs`; the app
re-exports it rather than maintaining a second server implementation.

Runtime chain: `rendezvous_self_owned_owner::run` -> `run_self_owned_node_v1`
-> the existing stable Iroh listener -> explicit `NVMBOX01` service selection
-> existing V2 signed offer and Finished-bound peer authentication ->
`mailbox_rpc::serve_mailbox_v1` -> the single bounded durable store worker.
The existing `NVLSRQ02` rendezvous protocol remains unchanged. Unknown service
versions fail, without probing another protocol or dropping identity binding.
The same service accepts a platform-owned L3 or L4 store. Android contribution
lifecycle/admission is not yet wired; no new global registration authority exists.

- Register retries retain the client-generated mailbox and capabilities; Put
  retries retain object/digest/absolute expiry. Fetch does not delete. Consume
  requires the device's read capability and matching digest; duplicate Consume
  is idempotent. Renew changes mailbox retention only, never object TTL.
- `StoreReceiptV1` signs the exact committed object/expiry after SQLite commit.
  It is storage evidence, not a device receipt, future uptime or NOV entitlement.
- Limits: 64 KiB/object, 2 MiB/256 objects per mailbox, 64 MiB/8192 objects and
  256 mailboxes per node. Consumed tombstones count against object quotas until
  original expiry + 300s; these conservative v1 limits require product sizing.
  Disk is capped at 128 MiB plus a bounded rollback journal; FULL is strengthened
  to SQLite EXTRA. Private directories are supplied by the platform owner.
- Sweeps and reads reject expired objects; backward local clock readings fail.
  No claim of detection of a restored valid disk backup or physical flash erasure.
  Payloads are E2EE ciphertext; node-side metadata wrapping/secure key deletion
  remains a production gap rather than being implied by SQLite secure_delete.
- `mailbox_replicas` selects at most two durable failure-domain-independent
  candidates plus one opportunity. Unknown/correlated operator, device, network
  or power domains cannot satisfy redundancy. This requires owner-established
  evidence, not trusting self-reported IPs or claiming physical HA from fixtures.
- Optional `mailbox-seal` uses [RFC 9180](https://www.rfc-editor.org/rfc/rfc9180.html)
  through [hpke 0.13](https://docs.rs/hpke/0.13.0/hpke/), classical
  X25519/HKDF-SHA256/ChaCha20Poly1305 with replica-bound AAD. This library states
  it has not been audited. No PQ, ratchet forward secrecy, anonymity or completed
  cryptographic review is asserted; the app must authenticate mailbox key ownership.

Validation: eight dedicated component/integration tests passed, covering real
loopback QUIC through the existing self-owned host, offline store/reopen/fetch,
lost-ACK duplicate operations, full quota, wrong capability/digest, revocation,
injected SQLite write failure without StoreReceipt, TTL, HPKE tampering and
failure-domain selection. Native Windows relay compilation and network clippy
passed, as did 47 existing service-lease regression tests. All identities/payloads are explicit fixtures. Logs are ignored local
`artifacts/mailbox-suite-final.log`, `mailbox-clippy-final.log`,
`mailbox-relay-check.log` and `mailbox-lease-regression.log`.

Reproduce with `CARGO_TARGET_DIR=<SUPERVM>/target/runtime-rebuild`:
`cargo test -p novovm-network --locked --features iroh-transport,mailbox-seal
--test mailbox_runtime --test mailbox_store --test mailbox_seal
--test mailbox_replicas -j 1 -- --test-threads=1`.

Remaining gates: existing Android owner mailbox key/bundle exchange and automatic
deposit/claim/receipt loop; physical L3+L3+L4 copies and repair; process/power-loss
tests; disk wrapping/rotation and online resource policy; Privacy-qualified paths;
latency distributions. The running laptop and phones have not been upgraded by
this component validation. Mailbox then content-free Push is the mainline;
neither further rendezvous latency tuning nor L2/L1 rollout is a prerequisite.

## 2026-10-13 Android cross-task reuse closure and L3/L4 mailbox direction

Transport input `2a0f8442`, NOVOCHAT product commit `ab84a8b`. The existing
Application owner retains one verified contact association plus the bounded
control/data QUIC pool. Each new task uses fresh business authentication and
its own authority/deadline; expired advertisements are never redialed.

PKL110 Wi-Fi / PCLM50 cellular, one original 600s engineering grant per device,
no reconnect clicks or data reset: each process bound once, created 17/16 tasks,
and emitted 13 control + 13 data connection reuse events. Twelve reused tasks
per phone completed fresh data authentication; the last task was deliberately
force-stopped for the restart check and is not a completed thirteenth sample.
Discovery-start/RPC-connect counts were 6/5, not one discovery per task.
Eight messages (four each direction) have device commits and sender durable
receipts; both reopened apps retained all eight and their persisted statuses.
No new Connect was granted after reopening; this checks storage, not automatic
restart of the engineering permission. Original node/identity/history preserved.

An 18.23s cellular outage recovered without reconnect UI, including queued
messages and the next new pair. First durable sender receipt after the mobile
data enable command took 91.627-91.773s / 92.479-92.645s, from monotonic clock
anchor intervals. These are not measured from Android validated-network-ready.
Retained-task data authentication was 4.812-6.433s / 4.338-7.802s (12 samples
each); it must not replace the total recovery latency or imply P95 compliance.
Failures retained: Wi-Fi task6 E216/stage4, task7 E215/stage2, task8 E3/stage3;
cellular task6 E1/stage4, task7 E215/stage3. Existing legal successor tasks
continued; no retry whitelist or authorization budget was widened.

NOVOCHAT regression: Application 68 passed / 1 ignored, Kotlin 35 passed,
Python 46 passed, Clippy/fmt and normal Android build passed. Local ignored
evidence is NOVOCHAT artifacts/cross-task-*; full call chain, exact package hash,
phase table and restart limits are in its docs/implementation/AUTOMATIC_RECOVERY.md.
External Iroh Relay remains involved. This limited round signs off cross-task
connection reuse, not new five-round recovery, low latency, anonymity or full
transport independence. The previous component evidence is preserved below.

Per the user's explicit scope, shift the mainline to Distributed Mailbox v1.
L4 phones must implement temporary ciphertext storage using the same L3/L4
protocol, not merely forward packets. Placement uses real availability/resources
and independent failure domains, with bounded replicas, original IDs/TTLs and
per-recipient-device consumption distinct from per-node replica state. A storage
receipt is neither Delivered nor a NOV payment. Sleeping phones cannot be the
sole reliable copy. First single-L3 verification is an implementation step, not
a permanent L4 exclusion or multi-replica claim. No mailbox runtime, L4 storage,
replication or settlement is claimed delivered by this documentation update.

## 2026-10-13 Bounded cross-operation QUIC retention

Input `8f8896b0` / NOVOCHAT `c640c6f`; only generic transport and adjacent tests.
`IrohEndpointOwnerV1::with_reusable_operation` opts into fixed purpose lanes.
The existing total admission limit stays 4; at most 2 idle connections retain
those permits. Complete frames, FIN, send completion and current authorization
are required before retention. Each new scope/stream has its own deadline and
requires fresh application authentication. No business channel, replay state or
signing lease is inherited. Failure/cancellation/network change evicts the pool;
owner expiry still closes real sockets. `retained` never dials a cached address;
ordinary authenticated-address `accept` can race a fresh dial with idle accept_bi.

Windows serial Iroh regression: 45 passed (23.87s), final accept-race wiring
followed by all 3 adjacent reuse tests passing (0.39s). Tests cover purpose,
operation expiry, cancellation, rejection, owner revocation and network change;
existing partial-frame, FIN and socket-close regressions passed. Ignored local
logs: artifacts/cross-task-transport-tests.log and cross-task-reuse-tests.log.
Phone cross-task delivery, latency, anonymity and independence are NOT signed
off by these component tests. After bounded product verification, development
moves to Distributed Mailbox v1 instead of indefinite rendezvous optimization.

## 2026-10-13 统一整数资源实机诊断与结果所有权交接

输入主链 `ead86349`，快进保留独立手机文档 `a7379623`；AOEM 输入 `1bfa7037`。
按用户已授权的通用适配范围修改 AOEM，不加入 NOV 业务。原产品仍为
`novovm-node/native_pipeline → novovm-exec → aoem-bindings → AOEM`，CPU 默认不变。
这次补齐此前未交付的实际绑定设备和设备时间，不再用选择偏好或机器显卡列表代替。

### 同一资源、同一完成边界的真实证据

复用 AOEM 整数资源实际持有的 Vulkan runtime，显式能力
`compute_checked_integer_resource_profile_v1` 加 AOIP0 v7 命令4 RunProfiled；
原 Open/Run/Close 结果不变，诊断另有严格 version1 五字段外壳，旧库缺能力明确
拒绝而不静默回退。资源/上下文身份、完整形状、失败锁存和预算继续检查。
实际命令队列的 timestampValidBits/period 用于模运算和回绕判定，在原 fence
完成边界读取、不加 WAIT 或逐批额外全设备等待；不支持/读取失败/无法排除多次
回绕时 GPU 耗时为空，不以 Host 等待冒充。默认关闭诊断，不创建查询计时工作。

四个原节点分别从其完成的资源取得 **NVIDIA GeForce RTX 5090 Laptop GPU**，
vendor `0x10de`、device `0x2c58`、discrete type2、queue0、64位、1ns/tick。
这是驱动报告的绑定设备，不是抵抗恶意驱动的硬件远程证明。RPC 明确验证范围，
未开启诊断时仍标未观测；费用/有序修复继续在 AOEM CPU 上，不能声称整笔业务
或完整证明已搬入 GPU。设备名/耗时仅在本地诊断，不进入交易、根、回执或签票。

改前/改后诊断各为真实65,536笔，四节点各64波、65,536有效行=容量行，本组无
padding放大。每节点64批的 GPU 命令累计约0.014s（每批约0.22ms），不是 shader
独占时间，也不含上传/回读。改前 AOEM 回读/结果组装0.605–0.623s，改后
0.293–0.298s；门面解码0.275–0.285→0.132–0.142s；完整门面exchange约
1.708–1.780→1.057–1.070s。upload约0.114–0.153s，dispatch约0.024–0.028s。
这些是嵌套累计墙钟，不能相加当整链关键路径；exchange还包括AOEM状态包装、
编码与读取，不把余量全称调度等待。两次带诊断TPS为8,228.207/8,465.807，
P95/P99反而从1221.961/1295.463升至1343.046/1394.668ms，不能混入正式对照。

### 依据成本完成的改动（不是再造算子）

原数值结果已经是完整 JSON Value 行，却在 AOEM 外壳组装时再序列化成第二份树；
exec 解开 state-read 和逐行反序列化时又深拷贝整树/行。现改为所有权转移，
复用现有统一结果通道。指针身份回归验证共享门面不再深拷贝；原严格字段、32个
u32 limb、完整1024位数值、错误状态、费用/nonce/失败前缀校验保留。没有改 shader、
数值宽度、业务算法或输出格式，也不是设备内存零拷贝；仍有真实 wire 编解码和
上传/回读。新版 profile 外壳同样移动结果，避免诊断本身重新复制这份大对象。

### 正式同条件对照及正确性

Windows Release，外接电源每轮前后为1，四个原节点/独立库，单RPC，65,536笔
真实预签名，1,024付款方×64 nonce、共享收款方，批1,024、窗口8,192。两组CPU及
整数旧/新交错；关闭计时，无并行编译/其他测试负载。终点仍是四节点均返回一致
最终成功回执；P95/P99含HTTP观察调度，不是纯共识时间。

| 正式组 | 旧 TPS | 新 TPS | 旧 P95/P99 ms | 新 P95/P99 ms |
| --- | ---: | ---: | --- | --- |
| CPU 1 | 10,171.323 | 9,376.203 | 1013.300 / 1046.569 | 1166.350 / 1275.061 |
| CPU 2 | 9,160.391 | 10,124.326 | 1086.985 / 1251.945 | 945.040 / 1118.496 |
| 整数 1 | 7,702.537 | 8,390.054 | 1276.409 / 1454.972 | 1186.132 / 1388.408 |
| 整数 2 | 6,879.530 | 7,570.662 | 1517.272 / 1670.296 | 1430.220 / 1527.368 |

总成功/总时间：CPU **9,639.424→9,735.914（+1.00%）**，单组方向相反，不签收
稳定提速；整数 **7,267.808→7,959.325（+9.51%）**，两组提高且尾延迟下降，支持
本片有限收益，但仍慢于CPU，不是GPU整链优于CPU或稳定主网容量。各轮均65,536
成功、0业务失败/拒绝/重试/传输错误；完整经济、历史回执/缓存淘汰及冷恢复通过。
实际减少结果复制而非改变费用/发压窗口/终点。未测可比较的峰值RSS，不宣称整机
内存收益；生命周期有界，无新增交易正文/日志队列。正式样本保留所有轮次。

最终原node普通回归395通过/60显式忽略，exec61通过/24显式忽略；Release构建、
根fmt、node/exec严格Clippy通过。AOEM FULLMAX整数19通过/1显式忽略、时间戳回绕
通过，新库/旧库实际资源能力与普通Run兼容通过。固定适用ML-DSA向量90/90通过。
最终整数节点失败交易/重启5.76s、A/A/B13.17s、离线追平/重启48.27s、完整费用
失配修复及全宽边界通过；功能测试可与非负载验证并行，不算性能样本。A/A/B为
三个诚实原进程加受控故障peer，不冒充未决写入中断或四实体机器。49草稿哈希未变。
AOEM修复以 `5e555e1a` 推送其main，Windows FULLMAX核心已同步；旧DLL与旧样本保留，
Linux和全部sidecar未重新签收。源码、DLL、节点与测试二进制哈希及所有样本见
[本片原始证据](../artifacts/audit/resident-integer-device-20261013/validation.json)。
安装路径首测曾被总索引旧哈希正确拒绝；同步两处SDK加载索引后，原严格校验
下真实profile/plain复验及随仓路径四节点单入口/失败/重启5.63s通过。未关闭
哈希验证，未将先前外部构建路径通过冒充包安装成功；完整失败原因保留在证据中。
本项不签收完整GPU业务、完整业务证明、百万级TPS或生产发布。保留原快路径、
报价与批树优化、全部旧证据和49份草稿；不再扩为网络/共识或发压器重写。

## 2026-10-13 NOVOCHAT 五轮前台连续恢复（仍有 85 秒长尾）

手机修复提交 NOVOCHAT `e8572ed`，实际通信输入为本仓 `afe21c64`。
同步本台账前先快进保留另一机器的 `ead86349`，其中金融执行改动不是本次
手机验收输入；本片只改本台账，不修改通用网络、金融、共识或 AOEM。

按用户要求，仅用固定版本 libp2p/SimpleX 源码核对当前生命周期与待发/ACK，
未引入依赖、迁移框架或另建网络。NOVOCHAT 原 OperationOwner 确定性复现并
修复：业务、双方签名完成和实际外层收尾成功后，较短质量采样到期仍使整体
操作报错。现在只丢弃到期样本；原授权、短操作期限、Privacy、代次、历史
归属及真实收尾错误继续检查，不扩大可重试集合。新收尾固定事件沿原遥测链。
旧手机 E7 缺底层类型，本轮也没有 quality_feedback_expired 实机事件；不能
把这项已验证缺陷当作历史 E7 的唯一根因。

PKL110 Wi-Fi / PCLM50 蜂窝在同一未延长的 600 秒用户工程连接许可内，完成
五轮至少 18 秒断网及每轮后续新消息，全程无人工重连。原 AFTER3 两条恢复，
新二十条交付；每端十一条持久发件确认和十一条收件提交。重开不点 Connect，
逐屏核对本轮二十二条消息及发送确认/本机存储状态；不清数据或重建联系人。
两端各 17/21 个任务后 expired、tasksFinished=true。全过程没有 E7，正常
许可末端仍保留 E8/E1 首错，不冒称从未发生失败。

蜂窝 T0（开启流量命令区间，不是网络可用时刻）至发件确认依次约
27.1 / 31.5 / 33.7 / 85.4 / 27.4 秒。第四轮首次约 61.9 秒数据认证后失败，
后继任务约 82.0 秒重新认证、85.4 秒确认；完整记录保留该失败和重复发现。
任务等待、会合和健康会话复用仍需优化，不能声称 P95 或即时性已达标。

每进程 bind=1；原自主 L3 330 次只读采样、跨十二个寻址实例，公钥/入口
保持稳定。Publisher 与旧来源镜像仍关闭，实际路径仍含第三方 Iroh Relay；
自主发现成功不等于自主传输或匿名性。节点运营许可未重建或延期。

App 局部回归 Rust 37、Python 46、Kotlin 35 通过，fmt/Clippy 与正常包构建
通过。两个旧消息 fixture 首跑失败、修正故障注入目标/既有类型断言后通过，
不冒称全栈首次零失败。完整源码位置、APK 哈希、逐轮区间、资源与限制见
NOVOCHAT `docs/implementation/AUTOMATIC_RECOVERY.md` 和
`docs/implementation/OPEN_SOURCE_RECOVERY_REFERENCES.md`。

**本组五轮前台连续性及原消息恢复通过；Automatic Recovery v1 整体、低延迟、
全天运行及自主匿名传输继续未签收。** 下一片继续原 Application owner / NOVONET
会合链，减少已测得的重复发现与认证，不再建设一套开源替代系统。

## 2026-10-13 最终批树复用与冻结程序供电对照（有限削减，非稳定容量）

输入 `main@bba8945b`。保留报价复用、原 node/exec/AOEM 及全部经济/共识规则，
只修改现有状态树与相邻回归。`finish → input.stage` 原本就是批接口，不重做批树。
单个互异 Put 批由 `change_many` 直接构造最终子树，没有中间版本；现在跳过随后
无用的可达性筛除/节点再解码。删除、重复键及混合批仍走原筛除。固定空根域的
SHA-256 改为同字节常量，并用原域重新计算校验；节点内容哈希、父路径校验和
预算不删。2,048 键反例对照保持 4,095 次父读取及 4,095 个最终节点，少做至少
8,189 次固定域哈希和 4,095 次可达性解码/集合操作；规范根和全部节点字节一致。
这不是新 AOEM lowering、GPU 树执行、数据库或第二条节点路径。

### 同一旧程序的漂移：本机确实处于不同供电条件

先以原 `58a29df7` 冻结节点 `6420d6c0…`、相同 `c040a30c…` DLL复测，
65,536 笔/四节点/单入口等原条件不变，关闭计时：6,761.398、6,238.902 TPS。
Windows `GetSystemPowerStatus` 随后实查 `ACLineStatus=0`、电池49%、平衡方案。
用户接电后实查 `ACLineStatus=1`；同一冻结程序正式样本 8,100.799 TPS，查询数
反而从724,992/858,112增至1,016,832。单独开启阶段诊断后为8,583.864 TPS，
树累计0.439–0.441s，回到前期约0.44s；原报价轮旧程序诊断为0.706–0.710s。
验签、收尾、HTTP与持久化墙钟也同时下降，不支持将差异唯一归因于查询数量或树。
没有修改系统电源设置。旧历史样本缺供电记录，**不能断言此前所有回退都由电池
造成，也没有分离CPU频率/调度等待或物理磁盘延迟**；本轮已证明不改代码也会
恢复一大部分表现，不再把约6K当作唯一固定基线。

### 插电后同条件代码对照

正式 Windows Release，原65,536实签/1,024付款方×64 nonce、共享收款方，批1,024、
在途8,192；旧/新/旧/新，关闭计时、无并行编译/负载，每轮前后确认外接电源。
终点仍为四原节点的全部一致最终成功回执，延迟包含HTTP查询观察。每轮65,536成功，
0业务失败、0拒绝/重试/传输错误；四库完整经济、历史查询及冷恢复均通过。

| 正式组 | 旧 TPS | 新 TPS | 旧 P95/P99 ms | 新 P95/P99 ms |
| --- | ---: | ---: | --- | --- |
| CPU 1 | 8,915.270 | 8,700.035 | 1130.641 / 1342.651 | 1251.902 / 1271.964 |
| CPU 2 | 8,249.714 | 8,833.384 | 1401.480 / 1618.047 | 1153.809 / 1477.689 |
| 整数单组 | 6,682.916 | 7,936.067 | 1596.866 / 1872.734 | 1205.711 / 1267.452 |

CPU总成功/总时间为 **8,569.589 → 8,766.203 TPS（+2.29%）**，第一组回退，
不签收稳定提速；整数仅一组，不将其约18.75%外推成稳定GPU收益。独立诊断旧/新
为9,543.381/9,571.757 TPS（不混入正式合并），树累计0.441–0.447→0.409–0.411s，
共同收尾0.631–0.633→0.588–0.593s。它们是嵌套墙钟，不能相加或称整链瓶颈已清。
算法少保留一份可达性集合/叶解码副本，未扩大预算；未测可比较的进程峰值RSS。

复用已有逐笔原始观察离线计算，四轮CPU平均客户端未完成数约7,674–7,686，
8,192窗口满额约78–80%时间。平均首次提交到四节点观察为0.861–0.931s。
这说明当前指标与该反馈窗口紧密耦合，**不单独证明客户端是唯一瓶颈，也不能
由此推算AOEM容量或把窗口加大后当作代码提速**；本片未改发压器或确认条件。

395项普通回归通过/60显式忽略、Release构建、严格Clippy通过。同一最终程序
单入口失败语义/重启1.69s、A/A/B11.52s、离线追平/重启48.15s及真实AOEM整数完整
经济修复3.29s通过。故障功能测试部分并行，不能当性能样本；A/A/B仍是三诚实原
进程加受控故障peer，不冒充未决写入崩溃或实体四机。49草稿SHA256未变。
完整样本、源码/程序/DLL哈希及原始报告见
[本片记录](../artifacts/audit/resident-tree-final-puts-20261013/validation.json)。

GPU身份只读核查定位到 AOEM `ApflResidentModelDevice::runtime_identity()`及整数
资源实际共享的runtime；v7结果仍未暴露该绑定信息。**本片未修改AOEM/SDK，设备
身份和设备时间戳尚未补齐**，不能用设备偏好替代证据。后继须通过原统一语义资源
契约接出，不新增GPU执行旁路或直接扩宽严格Open/Run解码。当前CPU和整数均保留
原入口；统一结构计划、完整证明及百万级主链目标未由本片签收。
输入提交CI `38374221621` 本轮最近查询仍in_progress，不记通过；交付提交CI另核。

## 2026-10-13 同一交易路径的报价不变量复用（有限工作量削减）

输入 `main@5a4475a2`，冻结对照为 `58a29df7` 的已记录 Release 二进制；两者之间
只有移动网络证据文档。原 node 的完整经济路径不变，本片只修改 direct-NOV 报价、
批预测至结算的交接及相邻测试。没有更换执行器、AOEM SDK、共识、状态树或网络。

原成功交易在预测、收尾报价和结算重验共生成三次相同报价。现在复用原 AOEM
分量任务，把完整请求/政策/候选时间绑定的私有报价结果随预测交给共同收尾；
正常及失效修复都只生成一次。政策 attribution 字符串每候选生成一次，回执仍
保留原完整字段。报价对象不可反序列化，不能用短 quote_id 授权另一笔交易；
完整字段和时间重绑定反例已验证。未改变公开报价验证接口、交易/根/回执 codec。
每笔资金、国库容量、费用舍入、诊断阈值、失败、nonce 和后继修复继续按实际前缀
处理，不把失败当 Nop、不合并总额收费。新增候选内报价/请求副本有线性内存成本，
由既有批数量和输入界限约束，不是零分配；没有扩大队列或在途限额。

这是明确不变量复用，**不是把不透明回调宣称为完整代数 lowering**。CPU 仍由
原常驻 AOEM 任务计算；整数预测仍经 `node → exec → bindings → AOEM` 的 opcode113
AOIP v7 常驻入口，使用同一报价及有序收尾。没有新 GPU 旁路，也未将费用/树迁入 GPU。
随包 v7 控制结果不提供实际设备身份或设备时间戳；physical_gpu_verified 仍为 false，
business_gpu_active 仍未知，本片不修改 ABI 或把偏好 5090 当作设备使用证明。

本机 Windows Release，四个真实原节点/独立库、单入口、65,536 真实预签名转账，
1,024 付款方×64 nonce、共享收款方，批 1,024、在途 8,192。以下八轮全部关闭计时，
旧/新交错，逐笔延迟仍是首次提交至最慢节点最终回执 HTTP 返回（含观察调度）。
每轮 65,536 成功、0 业务失败、0 拒绝/重试/传输错误，四库经济及冷恢复全部通过。

| 组 | 旧 TPS | 新 TPS | 旧 P95/P99 ms | 新 P95/P99 ms |
| --- | ---: | ---: | --- | --- |
| CPU 1 | 6,174.542 | 6,678.458 | 1548.646 / 1659.380 | 1434.326 / 1493.233 |
| CPU 2 | 6,485.333 | 6,433.815 | 1453.956 / 1465.934 | 1573.716 / 1610.425 |
| 整数 1 | 4,958.600 | 5,323.981 | 1890.096 / 2014.856 | 1863.531 / 1952.076 |
| 整数 2 | 5,234.064 | 5,000.972 | 1840.939 / 2055.606 | 2102.104 / 2355.462 |

总成功数/总计时合并：CPU **6,326.123 → 6,553.854（+3.60%）**；整数
**5,092.610 → 5,157.424（+1.27%）**。第二轮均回退，尤其整数尾延迟变差；
不选首轮宣布稳定提速，不平均百分位。冻结旧版在本组也低于上轮约9K，未定位
跨组波动原因，不将其归因于新代码，也不与历史最高单点计算性能损失。

另有独立 CPU 计时诊断，不混入上表：旧 6,252.998 / 新 6,366.744 TPS。
各节点 CPU execute 累计约 1.319–1.328s → 1.083–1.118s，共同收尾（包含树）
1.169–1.177s → 0.941–0.969s，树 0.706–0.710s → 0.669–0.682s。
这些是嵌套且跨线程重叠的墙钟，不是可相加的关键路径或设备 kernel 时间。
报价计数反例证明原正常路径三次、新路径一次，同时变化的余额/国库仍重新裁决。

Release 构建、393 项 native_pipeline 普通回归（60 项显式忽略）、lib/tests
Clippy `-D warnings`、真实 AOEM Vulkan 全宽整数及完整经济/依赖修复两门通过。
同一最终二进制的单入口成功/业务失败与重启（1.84s）、A/A/B（11.38s，三个
诚实原节点加受控故障 peer，重启后继）及离线追平/重启（48.09s）通过。
这不是未决写入中途强杀、实体四机或生产压力验收。49 项草稿逐路径 SHA256 未变。
负载、阶段和故障原始证据、源码/二进制/DLL 哈希见
[本片验收记录](../artifacts/audit/resident-quote-reuse-20261013/validation.json)。
这不签收百万级、物理 GPU 加速、完整业务证明成本、Linux/公网或生产发布。
不变的资源/等待/树/持久化成本继续沿同一产品定位，不能由一次复用推定全部已解决。
`58a29df7` CI `38360636995` 实查 cancelled；本片远端结果以本片提交的实际 CI 为准。

## 2026-10-13 发现 RPC 复用移动长期端点（长尾与连续性仍未签收）

输入本仓 `5a4475a2` / NOVOCHAT `f4cb62d`；手机接线提交 NOVOCHAT
`089b8c7`。本仓本片仅更新交付记录，没有修改通用网络、主链、AOEM 或金融
语义。App 的 `DiscoveryRoundConfig.endpoint → run_node → with_connection_scope
→ IrohEndpointOwnerV1.with_operation` 直接复用既有通用能力。控制、发现 RPC、
数据使用同一端点，各自保留原期限、签名、撤销、Privacy 与独立对端认证。

三批正式 PKL110 Wi-Fi / PCLM50 蜂窝样本，两端每进程 bind 总数始终为 1，
原 PERSIST4 R3 两条已恢复。后两批节点各 330 次采样、各跨 12 个自主寻址实例，
公钥和监听入口稳定；未开启 Publisher/旧镜像，第三方 Iroh Relay 仍参与。
不能把复用 Endpoint 当作 QUIC 连接/认证缓存，亦不等于自主匿名传输。

前两批各 R1–R4 及每轮新消息完成，R5 到原用户许可末端/结束仍待发，后续新
合法许可恢复原 ID；不能跨许可拼成五轮连续通过。第二批 R2 认证仍达约
178.4 秒、持久发件确认约 182.0 秒。新固定阶段记录显示多个发现 RPC 在发布/
领取、IO/截止时失败；部分双方在线租约对应相同实例，不能只归因于每 60 秒
换实例。原 90 秒有效期、短操作预算和恢复名单均未扩大。

第三批 R1/R2/R3 双向交付，但随后 Wi-Fi task14 在持久确认/收件之后出现 E7，
任务剩余 18.931 秒、用户许可剩余 136.935 秒，自动状态 blocked。E7 仍为
App 收尾相关文字后备分类，首个底层类型/具体调用尚未定位；不是已证明的
Iroh finish 缺陷，也不是用户许可正常到期。AFTER3 两条保留待发，重开确认
已送达状态与未发送状态均保持，未忽略收尾异常或更换 Message ID。

App 真实 QUIC 发现 22 项及后补负例 1 项、两轮保留端点丢 ACK 1 项通过；
Python 36、Kotlin 35、Flutter 聊天 27 项及严格 Clippy/fmt 通过。测试夹具
停机竞态与错误身份断言修正前的失败仍记录，不冒称全栈零失败。正常包已
原位安装，原密钥/历史保持。完整逐轮表、阶段计数、构建哈希与证据边界在
NOVOCHAT `docs/implementation/AUTOMATIC_RECOVERY.md` 最新节。

**正常发现重复 bind 消除已验证；五轮连续恢复、Automatic Recovery v1、
长尾改善与 1/5/10 秒 P95 均未签收。** 继续定位 App 收尾首错及 RPC/对端
会合生命周期，不新增协议或用放宽授权掩盖问题。

## 2026-10-13 移动长期端点与网络代次接线（整体延迟未签收）

`a5ecac32` 的 `IrohEndpointOwnerV1`、`d4e27c83` 的 Android 网络变化通知，
以及 `3f87a8b9` 的首错保留已接到 NOVOCHAT 原 Application owner。短期业务
权限不延长，每次对端认证继续独立；同一端点不等于同一 QUIC 会话缓存，也不
提高隐私资格。通用能力留在本仓，手机生命周期与加密待发在 NOVOCHAT。

第一版正式 Android 原位安装后，三轮 Wi-Fi/蜂窝断网均恢复，初始/三轮/追加
共十条消息有持久回执和重开证据。两端保留端点实例不变，各只执行一次该端点
绑定，跨 17/22 个任务至原许可正常结束；节点发现 RPC 仍有临时 bind。蜂窝
三次 T0→认证约 41.1/74.8/41.0 秒，T0→本机发件确认约 46.3/78.8/46.9 秒；
**相比旧 33.9 秒样本没有总体提速证据**。控制认证到数据认证约 3–4 秒，
主要长尾仍在本地载体就绪后到发现/连接对端。样本少，不计算可靠 P95。

通知版第二次实机在首次断网时出现 E8/blocked，两条原消息保留。首错早于
恢复流量约 17.94 秒，尚未进入任何 network_change 调用；旧日志没有最底层
Rust 错误类型，不能宣称本次 E8 唯一原因已证实。App 确定性复现了 Task
检查与 MobileRouteLease 检查间的网络代次竞态，并精确分类，未放宽所有 E8。
通用层同时保留已经返回的认证/协议错误，避免被随后网络撤销覆盖；操作
成功则继续最后验权，撤销后的成功仍拒绝。

组件：初始 Iroh 36 项，网络通知新增两项及跨 runtime 项，首错新增两项和
既有临时撤销端口释放项均通过；各片严格 Clippy/格式检查通过。首错测试用
真实端点验证 active 归零、owner 显式关闭后同 UDP 端口可重绑；旧临时错误
路径仍为 Drop 清理，不混称全部路径具备同等显式屏障。App 最终网络边界
11 项通过，先前 App 全组的偶发发现预算失败及测试准备期限失败记录保留在
NOVOCHAT `docs/implementation/AUTOMATIC_RECOVERY.md`，不能写全栈零失败。

第三版恢复了上述两条旧待发，并在两轮中断中送达四条新消息，重开保留。
两次认证约 35.5/31.0 秒、持久发件确认约 39.3/34.9 秒，端点继续复用；
但后续 cell task 11 出现 E11（carrier scope expired）并 blocked，第三轮
未执行。外层任务/用户许可/在线租约尚有效，不代表内部更短作用域没有到期。

`5791a71b` 将 IrohControl 的真实本地截止改为封闭类型标记；仍先检查调用方
授权。既有 service deadline 分类仅接裸标记和合法 Transport 包装，签名/
身份/Local 拒绝及同名文字不能因此重试。真实 finite scope/同期撤销等定向
7 项及严格 Clippy 通过，不修改节点预算或用户许可。手机后续样本另行记录，
不能用该组件修复覆盖第三版连续性失败。
最终第四版（受审通信输入仍为 `5791a71b`）COLD/R1/R2 六条双向交付，R1/R2
蜂窝持久发件确认约 37.1/117.3 秒。R3 观察到原许可截止约 149.2 秒，仅完成
晚到的控制认证，数据连接未完成，两条原消息仍待发。最后 cell/Wi-Fi 分别
19/18 任务后正常 expired，未见此前 E8/E11 永久停止，但三轮连续性及即时性
均未签收。两端保留端点始终实例 1；发现 RPC 的独立临时端点仍增长 bind 总数。
下一片应接通已有 owner 的节点 RPC 短操作，不增加调度器、不放宽期限/认证。

本机观测文件曾遇 Windows 短读替换冲突，续采合并仍有 Wi-Fi 23 条内部轨迹
缺口，未伪造连续证据；双端交付事件齐全。工具替换有限重试新增四项回归后
20/20 通过，不把它当手机连接修复。两机重开保留历史、确认及原待发。
实际手机路径仍经过第三方 Iroh Relay，Publisher 保持关闭。持续资源、电量、
多联系人并发入站、全自主传输、匿名路径与 Automatic Recovery v1 整体继续
分别验收；P95 1/5/10 秒仅为目标，不宣称已经实现。

## 2026-10-13 同一产品耗时与有界调度修正（百万级与 GPU 加速仍未验收）

本提交输入 `41986d75`，冻结产品对照为 `b097c195`；这两个版本之间原主链代码未改，
同期移动网络更新完整保留。仅在原 `novovm-node/native_pipeline` 增加默认关闭的
`NOVOVM_PROFILE_STAGES=1` 分段计时，并修正原 RPC 控制循环的短工作交接；没有另造
执行器、共识、网络、账本，也没有修改 AOEM。49 项 legacy/runtime 草稿不参与提交。

**本次结论：AOEM CPU 并发确实已使用，但主链并未交付百万级统一异构吞吐。**
CPU 最终两轮的四节点实际回调峰值为 24/24/24/24 和 24/24/18/24，不能叫纯串行，
也不能拿这个计数当 CPU 利用率或百万 TPS。费用归并、状态树收尾、候选编码、归档
复核和耐久元数据等仍有顺序工作；Vulkan 分支只覆盖整数预测，CPU 收尾仍在。
百万级目标不变；本轮有限约 9K 成绩不是 AOEM 能力上限，也不是整链容量上限。

第一轮 8.029s 窗口里，各节点控制休眠累计 5.55–5.88s，业务 CPU 执行累计
0.813–0.831s，其中归并含状态树约 0.720–0.725s，状态树约 0.436–0.441s；
HTTP 服务约 1.26–1.48s，候选持久化步骤约 1.06–1.09s，候选编码约 0.584–0.594s。
这些是**各线程嵌套/重叠墙钟累计，不可相加或直接相减推出提速，不是完整关键路径
因果证明**。计时起止由原负载夹具采样；状态中 max 是进程生命周期最大值，非区间差。

原控制循环只按 HTTP/鉴权进度判断休眠。现在也观察实际执行/耐久票/决定/高度等
固定标量变化；有进展后先短暂 `yield_now` 让出 CPU，连续无进展达到 64 轮或 250us
任一条件即回到原 1ms sleep。单有 pending 不重置预算；不缩短共识超时、不增大
队列或在途预算、不减少票数/验证/耐久。该方案增加控制轮询成本：诊断样本约
4.3–4.7 万轮，对照约 0.6–0.8 万轮；不是零成本唤醒或 CPU 利用率签收。

所有正式对照关闭计时：Windows Release，四个原节点/四库，单 RPC，65,536 笔
真实预签名成功交易；1,024 付款方各 64 nonce，共同收款方，批上限 1,024，在途
8,192。计时到四节点最终成功回执 HTTP 返回；签名生成、启动、冷恢复不入分母。
P95/P99 是逐笔首次提交到最慢节点最终回执，含 HTTP 和查询观察调度。

| 模式 | TPS | P95 ms | P99 ms |
| --- | --- | --- | --- |
| 冻结 CPU 1 | 7,719.044 | 1,315.612 | 1,501.751 |
| 冻结 CPU 2 | 7,912.725 | 1,539.485 | 1,588.945 |
| 最终有界调度 CPU 1 | 9,232.390 | 1,152.006 | 1,192.190 |
| 最终有界调度 CPU 2 | 8,750.994 | 1,171.079 | 1,202.007 |

按总成功数/总秒数合并为 **7,814.685 → 8,985.249 TPS，+14.98%**；不平均 TPS，
不把两轮百分位平均成合并百分位。这是有限本机对照，不是百万级整改完成。
各轮均 65,536 成功、0 业务失败、0 拒绝/重试/传输错误；四库完整经济/根/QC、冷恢复
和计时外历史缓存淘汰查询通过。业务失败另用真实单入口小负载验证，未充入 TPS 分子。

失败实验完整保留：通知 + `park_timeout(1ms)` 两轮 7,478.061 / 7,193.889 TPS，
完整夹具约 234/236s（冻结约 63s），已从活动源码撤回；其内部等待原因未定性为
共识错误或特定 Windows API 原因。不能只选前面的 9,002.305 有计时样本宣称提速。

整数诊断轮（开启计时，不能与关闭计时的 CPU 表直接作严格百分比）：7,132.006 TPS，
四节点每个 64 次整数接口调用累计 1.867–1.969s；整个整数执行含原 CPU 收尾累计
2.751–2.857s，CPU 收尾约 0.712–0.717s。`integer_wire` 覆盖统一 exec 入口内编码、
AOEM 调度/后端/等待、结果读取与解析，**不是纯 GPU kernel 时间**；没有据此签收
GPU 加速或物理设备身份。CPU 保持默认；统一 ZK 资产保留，本片没有测完整业务证明。

证据与所有中间轮次见[本轮记录](../artifacts/audit/resident-stage-profile-20261013/validation.json)。
本机 Release 节点构建、390 项 native_pipeline 回归（60 项显式忽略，不冒充执行）、
Release lib Clippy `-D warnings` 通过。真实单入口成功/失败及重启、A/A/B 三诚实
原节点加一个故障 peer、离线配额/在线法定人数确认/追平/重启分别通过；没有清锁、
替诚实节点出票或降低 QC。以上二进制在 `41986d75` 加本片上构建和测量；提交前
快进 `5791a71b`，保留两笔独立移动网络/发现修复，不把未复测的整仓版本混作同一
压测二进制。49 草稿逐路径 SHA256 全部一致；本轮未执行 Linux/公网/百万级负载。

下一刀针对**已实际进入原节点的统一整数调用边界与 CPU 收尾**继续细分成本，复用
已有批语义/常驻资源，识别编解码、设备传输、排队、内核、回读各项；不能把本轮
控制调度改善、开 GPU 开关或增大压测分母当作百万级交付，也不重造执行器/共识。

## 2026-10-13 原节点常驻整数业务接线（混合执行，非提速/生产签收）

输入 `58d7381d`，独立修复 native-auth 锁文件为 `f125f646`，再安全快进到
`96ee12a8dd72555917d1c50175d02cd28b15ddae`，保留同期移动端网络更新。本片只改
SUPERVM 原 node 业务/常驻 owner/部署选择及相邻验证；AOEM 通用接口仍为上一节
`1bfa7037`，没有往内核加入 NOV 规则。49 项原 legacy/runtime 草稿继续保护。

用户新增可用操作：在同一 `native-resident-v1` 部署配置显式设置
`"execution_backend":"aoem_integer_v7"`，真实 `nov_sendRawTransaction` 经原
鉴权/APFL、精确父绑定和依赖计划，在常驻 compute owner 调用
`novovm-exec/resident → aoem-bindings → aoem_execute_ops_wire_v1` opcode 113 / AOIP v7。
原有全宽 Transfer 资产从旧统一语义测试提取为共享源码，不重新定义余额/费用规则。
每个 owner 保留一个程序/固定形状资源；同组件逐波依赖，无冲突组件在同波执行。
计算结果进入原候选、round-bft journal、AOEM 原子持久化、发布和 RPC 查询，未另立
节点、签者、链头或权威状态库。默认不写该字段仍是原 `cpu_callbacks`。

范围必须准确：这是 **Vulkan 整数预测 + 原 AOEM CPU 有序费用/纯入账归并及有限
失配修复**，不是全部业务 GPU。设备调用失败不转 CPU，资源锁死待明确恢复；经济
依赖的有限修复与后端故障 fallback 不同。nonce、逐笔费用舍入、业务失败、国库
溢出及父状态/发布权限不变。配置不改变 genesis/protocol commitment，不重标旧票。

资源 ABI 当前验证 `vulkan_spirv`，未回传同资源的物理设备身份；软件 Vulkan ICD
仍可能被选择。因此 RPC 只声明整数后端实际活跃，`physical_gpu_verified=false`，
整数配置的 `business_gpu_active=null`；CPU 配置为 false。本机可见 RTX 5090 Laptop、
驱动 592.01，测试请求 name=5090/vendor=0x10de，**这些偏好不是物理设备回读证据**。
整数行计数排除 padding，但包括完成后被弃用的候选，不能当最终确认 TPS 分子。

当前限制：显式整数配置批上限 4,096，固定容量 padding 在长依赖链下会放大工作量；
v7 同步调用没有取消/硬截止契约。Linux 随仓旧库缺 v7，显式选择必须拒绝，不静默
回退。完整证明/证明最终性、隐私/PQ 接线、其他原产品业务和跨平台生产资格仍未
由本片签收；保留原统一 ZK 能力，不再新增后端专属证明服务。

初始同条件测量全部保留：Windows Release，单入口，四个原节点进程/四独立 AOEM
库，同一个 c040a30c DLL；65,536 笔真实预签名、1,024 付款方各 64 nonce、共同
收款方、批上限 1,024、在途 8,192。计时到四节点全部成功最终回执 HTTP 返回；
签名生成/启动/冷恢复不入分母。逐笔 P95/P99 包含有界查询观察。每轮成功 65,536、
业务失败/拒绝/重试/传输错误均为 0；另有小负载独立验证业务失败，不以拒绝充数。
四库完整经济、根、QC、冷恢复以及计时外 1,024 笔缓存淘汰历史查询全部通过。

| 初始版本/模式 | 两轮 TPS | 两轮 P95（ms） | 两轮 P99（ms） |
| --- | --- | --- | --- |
| 冻结 CPU 对照 | 9,425.139 / 9,155.680 | 1,029.395 / 1,099.599 | 1,094.073 / 1,173.839 |
| 初版接线 CPU | 7,783.464 / 8,704.649 | 1,279.676 / 1,129.237 | 1,527.999 / 1,151.625 |
| 初版整数混合 | 6,062.372 / 6,414.414 | 1,601.488 / 1,493.670 | 1,656.948 / 1,556.403 |

按总成功数/总计时合并分别为 9,288.456 / 8,218.323 / 6,233.427 TPS；不平均 TPS，
不把各轮百分位平均后冒充合并百分位。整数模式四节点每轮各完成 65,536 非填充行、
64 波、0 失配修复、0 执行失败。该有限负载**证明真实业务已接通，不证明加速**。
初版 CPU 差距也未忽略：最终源码已撤回不必要的 CPU `speculate` 重构，使该函数
与输入 main 逐字一致；此举不冒充已定位全部性能差异，最终版本复测另列。

原始结果及逐笔文件位置/哈希统一保存在
[本片证据](../artifacts/audit/resident-integer-business-v7-20261013/validation.json)。
冻结对照可执行文件的准确 SHA/来源、所有中间样本和最终样本分别记录，不因目录
名称含 `58d7381d` 就假称旧可执行文件由该提交重新构建。没有上传签名私钥/数据库。

最终源码复测（与初版分开，不丢弃任何慢样本）：

| 最终组模式 | TPS | P95（ms） | P99（ms） |
| --- | --- | --- | --- |
| 最终 CPU 1 | 8,506.528 | 1,277.214 | 1,311.364 |
| 紧邻冻结 CPU 3 | 9,107.662 | 1,081.414 | 1,219.867 |
| 最终 CPU 2 | 8,175.681 | 1,297.353 | 1,415.816 |
| 紧邻冻结 CPU 4 | 8,479.919 | 1,252.464 | 1,289.510 |
| 最终整数混合 1 | 6,440.430 | 1,565.730 | 1,763.963 |

最终 CPU 合并 **8,337.824 TPS**，紧邻两次冻结 CPU 合并 **8,782.587 TPS**，
约低 **5.06%**；全部四次冻结 CPU 合并 **9,028.441 TPS**，约低 **7.65%**。
最终整数样本仍没有加速。上述性能差异均未签收，不能宣称 CPU 无回归或 GPU 提速。
有限只读复核确认各组 workers=4、批 1,024、同 DLL/genesis，CPU 不创建整数资源；
CPU 执行/归并算法未改，每节点均 64 批、65,536 组件、0 修复/失败。
新增分支/观测计数不足以证明差距根因。回执观察次数也已逐轮保留，不能将其增多
直接判作原因；chainStatus 后端检查在计时之后，不是正常计时回执循环的新增工作。
**下一步限定同一真实路径的阶段成本定位及针对性修复，不用重复整轮跑分代替定位。**

最终二进制 SHA256 为 `99d04143573e94b6f4322d5dad3296c9e985a6b7841b84c60bd46a16d2a36ebd`。
该版本 Windows Release 验证：

- native_pipeline 纯测试 **388 passed / 60 ignored**；两项显式 Vulkan 真实完整宽度/
  完整经济效果对照通过，覆盖依赖、费用舍入、国库溢出导致的后继修复与资源拒绝。
  旧库缺 v7 的实际 owner 启动拒绝通过；节点构建及严格 lib/tests Clippy 通过。
- 单 RPC 入口真实四进程功能 **7 成功 / 1 业务失败**（4.48s 全夹具），保留签名、
  nonce/重放、费用、历史查询、冷恢复及后继；未启用旧 Host 许可。
- A/A/B 三个诚实原节点加受控故障 peer **通过**（13.91s），验证三诚实签者 QC，
  冷重启后继续后继。不清锁/重置签者/外部代造诚实票，不扩大为写入中途崩溃验收。
- 离线节点场景 **通过**（48.76s 全夹具）：在线法定人数发布 **41.437s**，加回执/
  经济核对 **41.725s**；最长观测无进展 **10.235s**；第四节点追平 **2.436s**；
  冷重启 **1.982s**、后继 **0.518s**。16 批传播 **65ms**，最终 34 成功、0 业务失败。
  保留批 2、原 8s 传播门、90s 无进展/240s 总窗口，不拿故障夹具换算正常 TPS。
- 最终整数负载四节点各 **65,536 非填充行 / 64 波 / 0 修复 / 0 执行失败**，
  同一候选由原共识授权发布；这些只是执行观察，不是完整 ZK 证明。
- 独立 native-auth 先前 CI 失败是依赖锁文件未同步；`f125f646` 修复保留全部断言
  与 `--locked`，Linux host 9 项及严格 Clippy 通过，guest 仅锁解析通过、不宣称
  生成证明。GitHub Native auth `38350151227` success；本片新提交 CI 另行跟踪，
  不把已有运行或本机通过写成最新主链 CI 全绿。

## 2026-10-13 移动端长期 Iroh Endpoint：通用载体节点

补充 Android 网络变化接口：`IrohEndpointOwnerV1::network_change(control)`
保留端点，由原短操作授权提交接口重查请求。两项新增及扩展的跨 runtime
测试通过；本地真实 QUIC 在通知前后继续传帧，端口、身份与绑定计数不变，
关闭后端口可重新绑定。check、严格 Clippy、fmt 通过。通知成功不等于网络
在线，也不会保证立即打断既有 Relay 退避；手机对照性能另行验收。

输入 `main@95e33995`（含其他机器的主链回执更新 `191f5f89`），仅修改
`crates/novovm-network/src/iroh_transport.rs` 及本轮网络文档。没有修改 AOEM、
账本、共识、UCA 或交易执行。产品边界见
[传输策略第 13 节](NOVONET_TRANSPORT_POLICY_V1.md#13-2026-10-13移动端长期端点与短操作分开管理)。

新增 `IrohEndpointOwnerV1::bind / with_operation / close`，在同一活跃 Tokio
runtime 保留一个端点。每个短操作继续独立验权及原期限检查，所有 scope 共享
入站接受互斥锁和 4 个连接配额。控制、数据各自新建 QUIC 连接并认证，不复用
另一对端的信任或会话。配置不匹配、跨 runtime、撤销和旧 control 到期仍拒绝。
显式 close 封禁准入、等待旧操作退出，再走原 4 秒排空/5 秒 OS 关闭屏障；
关闭取消/失败不报成功，不在 owner 仍持有资源时释放端点配额。

Windows 本机 `iroh_transport::tests` **36/36 通过**（新增 8 项），包括真实
嵌套数据连接结束后控制流继续双向传帧、同一端点后继新授权、真实拨号到期、
入站竞争、运行中关闭、撤销、配置错配、跨 runtime 以及取消关闭。绑定完成
恰逢撤销时的 inline 关闭也实测原 UDP 端口可立即重新绑定。`cargo check`、
严格 Clippy、fmt、diff 检查通过。复现：`cargo test -p novovm-network --features iroh-transport --lib iroh_transport::tests -- --test-threads=1`；`cargo clippy -p novovm-network --features iroh-transport --lib -j1 -- -D warnings`（本机显式使用 SUPERVM target）。后续 App 实机接线另行记录。

以上是通用载体组件结果，**尚不代表手机提速、长期运行或 Persistent Mobile
Transport v1 产品签收**。内部节点发现仍可创建短作用域；总 bind 计数包含
这些真实创建，不能从主端点实例未变推出整轮零建连。没有增加重试预算、
降低 Privacy、放松错误白名单或改变回执含义。
## 2026-10-13 AOEM 通用常驻整数资源与原 exec 门面

用户明确授权的通用适配已提交 AOEM `1bfa70372e2e60851c6d0ab972dea19678b0fa8c`
（XujueKing SSH，main）。保持 `aoem_execute_ops_wire_v1` opcode 113，新增 AOIP v7
Open/Run/Close，复用既有 APFL 整数结果算子、Vulkan/SPIR-V 与会话注册表；不加入
NOV 余额/费用/nonce 规则，不新增 C 导出、调度器或 CUDA 后端。旧 v1–v6 保留。

原 `novovm-exec` 增加独占常驻门面：严格检查实际 DLL 能力、程序/输入形状、
owner/resource ID 和完整 i1024 结果；共用固定结果键，不按批重建 GPU 设备。
未知调用、读回或结果格式失败后停止复用，不重试、不读取旧结果、不静默转 CPU。
原 CPU 交易路径不变；**本片没有让真实主链候选调用 v7，也没有接通完整业务证明**。
下一接合点是原常驻 compute owner 的依赖组件/业务计划，而非 RPC 控制循环；
需保留精确父状态、冲突顺序、费用归并与业务失败语义，不能把依赖交易误当独立行。

Windows FULLMAX 核心和头文件更新自上述 AOEM 提交，核心 SHA256
`c040a30cce119c6d8da7b6609ec1302d9210a708a70cae3ace75a99008eb0539`。
Windows 旧插件和 Linux 包保留各自来源与哈希；Linux 尚不包含 v7。严格 capability
拒绝旧库，不用“源代码已支持”冒充所有机器均已更新；本次不是全平台发布。
各组件及旧资格证据边界见 [SDK 当前基线](../aoem/RUNTIME-BASELINE.md)。

实测与限制：

- AOEM 18 项整数回归、1 项未发布 Vulkan buffer 清理测试及无特性能力测试通过；
  FULLMAX 构建通过。清理测试是合成调用记录，不是物理设备丢失注入。
  AOEM 严格 Clippy 被 199 项既有 C raw-pointer lint 阻断；仅命令行允许该项后通过，
  仍有 25 项既有警告，不记作严格检查全绿。
- exec 59 单测、1 个不可跨线程所有权 doctest 和严格 Clippy 通过。旧 DLL 拒绝测试通过；
  新 DLL 在 RTX 5090 Laptop 上的 v7 重复输入/完整宽度/拒绝/关闭及旧 v5 回归各通过 1 项。
  bindings resident 32 项通过（其中 receipt mock 不算真实证明）；固定 ML-DSA 6 组
  90 向量全部通过，不扩大为 FIPS 认证或主链 PQ 集成。已退役私有 profile 的 op98/op99
  仍在零工作、零输出边界拒绝，不将公开 envelope 当 ZK。
- 同一 256 行整数资产，五组交替 v5/v7 测量的重复调用中位数 **111.024ms → 1.811ms**；
  首次 Open **3182.145ms** 单列。计时含 wire、状态写/读及 JSON 解码，不含输入编码。
  这是生命周期成本对照，不是 kernel 性能、业务 TPS、ZK 或 CPU/GPU 相对提速。
- 与上节历史回执节点相同的 Release 节点/测试二进制，新 DLL 单入口四进程功能通过
  （2.33s）。同条件 65,536 笔、1,024 付款方各 64 nonce、共同收款方、批 1,024、
  在途 8,192，四节点成功最终回执 **7.733475s / 8,474.328 TPS**；P95 **1,144,044µs**、
  P99 **1,162,076µs**，成功 **65,536**、业务失败 **0**、拒绝/重试/传输错误 **0**。
  四库全量经济/QC/状态根、冷恢复，以及计时外 1,024 笔造成的真实缓存淘汰和再次恢复通过。
  整个夹具 62.31s 不是 TPS 分母。旧 DLL 本轮一份样本为 8,712.032 TPS，当前约低 2.73%；
  每版仅一轮，不据此宣称稳定提速、稳定容量或已证明无性能退化。
- 直接加载已复制的 SDK DLL 后，实际 A/A/B 分票/重启/后继回归通过（11.16s），
  离线 peer/健康法定人数/追平/重启回归通过（48.00s 全夹具）。没有降低票数、
  清锁、改变超时或复用测试预造的诚实签名；不是物理多机或生产网络验收。
- v7 每 context 的 64MiB 限制只覆盖驻留程序和逻辑 GPU 缓冲区；不覆盖驱动开销或
  原有全进程结果状态。每个 exec owner 复用一个键，但不断创建新 owner 仍会保留最后结果，
  不能宣称全进程内存完全有界。生命周期采用常驻 owner，不用频繁重建规避失败。
- 现有安装探针强制同时启用六个保留插件时，新核心 61.4s、旧核心 60.066s 均无输出
  超时，已核实并停止本次测试进程；不是生产服务。两次使用相同探针/插件/模式，
  仅核心路径与 hash 改变。保留为继承的全插件组合资格缺口，未查明具体等待原因；
  不据此声称新核心回归，也不把未通过配置签收。正常主链配置不强制开启所有插件。

原始 [负载](../artifacts/audit/aoem-integer-resource-v7-20261013/load-result.json)、
[功能](../artifacts/audit/aoem-integer-resource-v7-20261013/single-ingress-result.json)、
[ML-DSA](../artifacts/audit/aoem-integer-resource-v7-20261013/mldsa-acvp.json)、
[A/A/B](../artifacts/audit/aoem-integer-resource-v7-20261013/aab-result.json)、
[离线恢复](../artifacts/audit/aoem-integer-resource-v7-20261013/offline-result.json)。
逐笔观察在本机 `artifacts/audit/resident-rpc-load/29112-1791867004081517600/observations.json`，
SHA256 `06727e122158c52696bb1925c2feb62faa59037787346eaad77c4350ce6cb453`。
没有上传密钥/数据库、修改旧签票日志、启动真实主网或签收隐私/GPU业务/完整证明。
R1/R2、原多资产与 Execute 等目标继续保留；生产发布目标仍进行中。

## 2026-10-13 主链历史回执与真实缓存淘汰恢复

输入 `main@0688b760`，保持原 `novovm-node/native_pipeline → novovm-exec → AOEM`
常驻 CPU 快路径及 `novovm-consensus/round_bft`。49 项既有 legacy/runtime 草稿
在快进前后逐文件 SHA256 相同；未启用旧 Host 许可、修改真实账本或新建分支。

原 `nov_getTransactionStatus` 增加 `[tx_hash, positive_block_height]` 可选定位。
复用同一 AOEM 归档读取及已验证链前缀、候选和证书检查，不新建账本或全历史扫描。
只有 hash 时仍查最近 65,536 项；显式高度始终查归档，`not_in_block` 仅表示该块
没有该交易。损坏/缺失证据返回错误，查询排队不是 finalized。最多 8 个查询，
全体每控制轮最多推进一次归档读取，保留签票/投影的 I/O 优先机会。

Windows Release，本轮节点 SHA256 `f6b470d6723d447fe7b9e336fc8434ccae5a0496bd82408093c550ea219cafc6`，
随仓 AOEM DLL `01779fe4fc77535749d265f9ffc233d9c1486e526d9f5d0dc34fc6d49a056674`：

- native_pipeline **385 passed / 57 ignored**；node lib/bin/tests 严格 Clippy 通过。
- 原单入口四进程夹具通过（2.48s）：7 成功、1 收费业务失败；重启前后各核对
  24 份完整历史回执、8 个块内不存在结果，并拒绝错误高度。此夹具不是 TPS 测量。
- 单入口 65,536 笔实签负载：1,024 付款方、每方 64 个连续 nonce、共同收款方，
  最大批 1,024、在途 8,192；四节点全部成功最终回执用时 **7.522470s / 8,712.032 TPS**。
  业务失败 **0**、准入拒绝 **0**、重试 **0**；P95 **1,105,661µs**、P99 **1,262,310µs**，
  从首次 HTTP 提交到最慢节点回执响应，含观察调度。签名预生成/启动/冷恢复不计时。
  四库全量经济、状态根、3 签 QC、冷恢复核对通过。这是一个当前 CPU 样本，
  没有同条件修改前对照，不作提速或稳定容量声明。
- 原负载计时及四库经济核对之后额外提交 1,024 笔，**不计入 TPS**。最早一块
  1,024 项真实淘汰，四节点 hash-only 返回 `not_in_recent_index`，带高度仍完整查回；
  再次冷重启后保持。额外批核对回执/费用/nonce/四链头，但没有重做 66,560 笔全量经济 oracle。
- 同一节点二进制的 A/A/B 分票/重启/后继场景通过（11.28s，三个真实诚实进程
  加一个受控故障 peer）；离线 peer 隔离、在线法定人数推进、追平和重启场景通过
  （47.93s 全夹具耗时，不是正常吞吐或单独追平延迟）。未改变计时器、门限或锁规则。

原始功能与负载报告：
[single ingress](../artifacts/audit/mainchain-receipt-history-20261013/single-ingress-result.json)、
[load and eviction](../artifacts/audit/mainchain-receipt-history-20261013/load-history-result.json)、
[A/A/B](../artifacts/audit/mainchain-receipt-history-20261013/aab-result.json)、
[offline](../artifacts/audit/mainchain-receipt-history-20261013/offline-result.json)。
逐笔原始观察留在本机 `artifacts/audit/resident-rpc-load/24796-1791865677122624000/observations.json`，
SHA256 `0da9267db111684b82faba759a025406e8acd1d14de9415a39d79486012e4770`；未上传密钥/数据库。
尚未验证持续历史查询压力下的吞吐、RPC 层数据库损坏注入、Linux 本轮及实体多机。

GPU/完整业务证明没有因上述结果而签收。用户本轮另行授权 AOEM **通用**常驻数值资源
适配，禁止 NOV 业务内置；复用现有统一 wire/Vulkan 并接原 exec 门面，源码、FULLMAX SDK、
真实主链调用和性能分别核验。旧 SDK 未升级时不启用或静默回退。原多资产、Execute、
隐私/PQ 与发布验收仍属同一产品目标；本节不构成生产可发布结论。

## 2026-10-13 自主发现接入真实 Android（阶段交付）

基础设施提交 `e7379b22` / `3e015dfe` / `89601224`；NOVOCHAT 运行时接线
`4f612c9` / `a35e270` / `76ffa34` / `c5f8739`。笔记本原 DPAPI 节点已显式
迁移为自主 owner，签名/存储密钥、日志和撤销历史保留；旧 Publisher 与来源
镜像进程关闭。长期监听没有 24h 测试运营许可，90s 寻址实例每 60s 更新。
实部署二进制为 `3e015dfe`，手机使用后续兼容 V2 的帧期限分类修正。

两台原包 Android 在 Wi-Fi / 蜂窝通过自主声明与新鲜在线租约完成普通输入框
双向 E2EE 及持久确认，原联系人、身份及待发 Message ID 保留。此处证明
无需 Publisher 的身份/发现路径；实际传输仍经批准的第三方 Iroh Relay，
不证明独立传输、自主匿名或整项 Automatic Recovery v1。连续恢复的逐轮
结果及失败样本见 NOVOCHAT `docs/implementation/APPLICATION_DELIVERY.md`。

最终手机样本：原两条待发恢复，前两次蜂窝中断约 17.265s/18.132s 后分别
约 19.71s/22.53s 恢复认证，四条新消息取得持久确认；第三次 15.313s 中断后
客户端任务停滞，另两条保留待发。原任务超期无新轨迹，不能把旧 running
快照计作健康在线。两端重开后原消息/确认保留，连续恢复为 **2/3 未签收**。
同时自主节点 100 次采样跨 9 个实例，地址和节点公钥不变，Publisher/镜像
仍关闭。不是节点持续监听失败的直接证据，客户端等待根因仍需单独定位。

实机促进两项客户端修复：两端缓存旧实例不同导致寻址槽位不一致；活动槽位
容量与跨代已消费防重放记录混算。组件红绿回归后修正，签名/撤销和防重放
保持。另以真实帧计时器复现精确类型丢失：SUPERVM 的私有构造
`IrohFrameDeadlineV1` 经发现或认证业务包装后分别诊断 E215/E216，只有持久
消息 owner 可在旧流关闭后重新授权恢复原 ID。未改变普通 IO 分类，不按
错误文本重试；身份/本地授权包装仍拒绝。旧手机 E1 不能单凭这些组件回归
认定唯一原因。载体 28 项、精确业务期限 2 项、连接生命周期 12 项及严格
Clippy 通过，原失败与修复前红测保留。

节点日志 16 MiB 安全轮转、完整备份外部防回滚、已有 UCA 原账户所有权绑定、
开放服务计费、L4 真转发/贡献结算、自有载体及匿名仍是明确未完成项。

## 2026-10-13 自主节点真实入口与稳定监听（手机验收前）

在 `e7379b22` 基础上交付 `novovm-rendezvous-service` 本地 v2 自主策略。
调用链为 Windows DPAPI owner → `run_self_owned_rendezvous_v1` →
`with_iroh_owned_listener_v1` → 原 V2 Offer + Finished 绑定身份握手 + 原寻址 RPC。
长期节点/传输密钥不随短租约轮换；90 秒有界寻址实例每 60 秒切换，新请求
原子选择实例及其签名租约，旧流最多 30 秒且不续期。未增加聊天专用线协议。
默认入站上限 4、既有记录/字节/身份资源边界保留，不引入全网许可或链 RPC。

`--migrate-self-owned OLD_CONFIG NEW_CONFIG` 为显式本机迁移，保留原 DPAPI
节点/存储密钥、加密日志、序号预留、背书与撤销历史；新策略也受 DPAPI 绑定，
旧入口不能重新启动，新旧状态损坏/丢失均拒绝。初次节点使用
`--provision-self-owned CONFIG`；普通运行使用 `--foreground CONFIG` 或原 SCM。
这不是把旧许可延期：接收方仍须明确确认自主节点公钥，验证新鲜在线签名。

Windows 真实可执行进程 **2/2**（133.88s）：新节点无 Publisher；旧节点迁移、
跨两次实例切换、原缓存获取新实例、端点地址/公钥不变、进程重启后恢复、
并发 owner 拒绝、旧入口及缺失日志拒绝。通用租约 **47/47**，Iroh **28/28**，
network/relay all-targets Clippy `-D warnings` 通过。日志均在忽略的
`artifacts/self-owned-*.log`。尚未据此签收两 Android 或第三方载体独立性。

仍有限制：加密日志 16 MiB 到限安全停止，安全轮转未交付；完整旧备份防回滚
仍需外部新鲜度依据；本机客户端资源许可为运营者配置，尚无开放服务计费接口。
自有 Iroh Relay、UCA 既有账户绑定、L4 实际中继及匿名路径不在本次签收范围。

## 2026-10-13 自主入网、统一 UCA 与 L4 奖励的架构边界

本节的运行时后续：基于 `c7b32c93`、认领 `e82e90d4`，既有 ServiceLeaseBook
已增加显式 `SelfOwnedTrustV1`，与原 Publisher 策略共用 V2 Offer、在线租约、
CAS 持久化和撤销；旧策略的非零签名门限保持。新增 V2 本地 checkpoint，V1
恢复字节不变；显式迁移候选保留旧背书历史、序号及撤销，待平台原子提交。
本地已核对公钥与历史定位地址只允许取得新在线声明，不复活过期声明、延长
旧流或授予 Privacy；当前载体/服务握手仍须验证双方持钥。
Windows 租约/来源专项 **46/46**（15.42s），network all-targets Clippy
`-D warnings` 通过；日志 `artifacts/self-owned-{leases,clippy}.log`。
这是通用运行时组件，手机/常驻启动入口及完整 Self-Owned Discovery v1 尚未签收。

输入 `4670da2c` / NOVOCHAT `3cbe565`。已完成定向源码复核和
[迁移契约](NOVONET_SELF_SOVEREIGN_IDENTITY_V1.md)，未改运行时或操作手机。
节点自签记录已存在；当前 managed 手机链仍需配置 signer 来源，不能声称
已经摆脱 Publisher。统一 UCA 原语已有，NOVOCHAT 本地根尚未绑定既有 NOV
账户。L4 opt-in 预算组件已有，实际中继、持久贡献计量及 NOV 到账未接通。

自主入网不要求官方许可、持币或逐消息链查询；可选社区背书只约束其服务，
不具备全网封禁权。用途密钥、资源限额、撤销及默认 Privacy 继续有效。
本轮验证限定文档差异/链接和源码符号核对，无新增编译、实机、匿名或结算
验收；Automatic Recovery v1 的未签收状态不变。

## 2026-10-13 镜像持续监听与 E214 拒接机制

输入 `d57afa6a`，认领 `5121d67f`。实际调用链为 Windows mirror owner →
`with_iroh_listener_v1` → 原 Iroh scope/endpoint → V2 来源分发。
监听身份仍来自 DPAPI，签名来源仍由独立发布者签发，短请求限定 3 秒；
每次来源变更先持久校验，再停止旧的在途来源请求。普通客户端 120 秒作用域、
连接/端点/速率上限、签名、撤销、Privacy 和错误分类保持。

固定 noq 的 closed endpoint 会拒绝新 incoming。真实 QUIC 故障注入保持一条
未应答连接进入排空，新拨号得到 origin=3/code=2（CONNECTION_REFUSED），
尚未进行 NOVOVM Offer 交换。这个用例证明一个可发生的拒接机制；历史手机
样本 18 没有对应服务端日志，不能断言其唯一原因已被证明。

镜像不再每 110 秒拆除/重建端点，而在当前原始运营期限内持续监听；
110 秒仅为脱敏生命周期检查点。日志最多保留 128 个事件，不含公钥、地址、
联系人、消息、密钥或原始远端错误。停止/过期仍退出，不使用扩大重试掩盖故障。

Windows 验证：载体 **28/28**，租约 **41/41**；镜像/发布者/寻址 owner 三组
独立进程共 **15/15**，network/relay all-targets Clippy `-D warnings` 通过。
镜像用例实际跨越 110、220 秒，**51 次刷新，0 失败，1 次绑定，0 次停止**；
原镜像身份、地址与期限一致。日志为忽略目录中的
`artifacts/recovery19-{carrier-reviewed,leases,process-final,clippy}.log`。

保留两项夹具失败：初版排空用例误等 `is_closed()`（表示完全关闭），修正后
通过；旧镜像对照用例的节点夹具过早结束，15 次刷新失败无 QUIC 诊断，不能
计作 E214 复现。该对照仅显示旧端点约 52ms 的停止到重新就绪窗口。最终用例
延长的是夹具运行覆盖时间，不改变客户端尝试或授权预算。

当前 V1 配置仍有 24 小时初始运营限制。用户最新要求不再另建测试续期系统，
因此本片没有新建 24 小时镜像、重新发邀请、修改旧期限或清除手机状态。
正式长期服务与初次信任的迁移、手机跨网恢复、自托管 Iroh Relay、24 小时实测
和匿名性均未签收；后续遵循统一产品运行契约。既有手机原待发 ID 保留。

## 2026-10-13 UDP 操作系统关闭完成屏障

输入 `33903231`，认领 `0e133992`。未改代码先运行旧黑洞 QUIC 用例，首轮
复现 `local driver retained UDP socket`。进一步核对本机固定 netwatch 0.19.3：
`UdpSocket::Drop` 通过 spawn_blocking 关闭 OS socket，但不等待该任务；只等待
actor/QUIC driver 并不能证明端口已释放。新用例占住 Tokio 唯一 blocking
worker，旧实现仍在 0.06 秒报告关闭成功，因此确定存在缺失的 OS 完成屏障。

补丁位于固定 `vendor/iroh`：`IpTransport::close_handle` →
`Transports::ip_close_handles` → `EndpointInner::bind/close_inner`。
保留每个已配置 IP socket 的 Arc，停止并等待 actor/QUIC driver 后，逐个显式
等待 `UdpSocket::close()`，最后才返回 Some(drained)。不增加 socket、不编辑
全局 Cargo registry、不改 wire/签名/隐私/重试/消息模型。原五秒总关闭上限
覆盖该等待，超时或取消仍失败；Drop/abort 不宣称 join 完成。Relay-only 无 IP
socket 时该集合为空。原始导入 SHA256 清单保持，补丁文件目录记录已更新。

Windows 验证：载体 **25/25**（10.71s）、租约 **41/41**（15.50s），
network/relay all-targets Clippy `-D warnings`、改动文件 rustfmt 与 diff 检查
通过。旧黑洞用例加强为保留 endpoint/connecting handle 时立即 bind；与阻塞池
用例连续 **30 轮、60/60**，不使用 sleep 或 bind 重试冒充关闭完成。
原失败日志保留：`artifacts/recovery18-{drain-before-1,os-close-red}.log`；
通过日志：`recovery18-{carrier,leases,clippy,close-repeat-*}.log`。

本片修复并验证此具体 OS close race，不宣称所有平台/上游任务已完整审计。
NOVOCHAT 样本 17 的约 95 秒恢复含多次任务/对端认证交接，不归因于单次 close；
历史手机 E64、长期连续性、自主载体和匿名性保持未签收。正式手机结果由
NOVOCHAT 单列，通用网络能力仍沉淀在 SUPERVM。

接入 `2cf514f6` 后 NOVOCHAT 来源专项 14/14、正式 Android 构建和两机原位
升级通过。样本 18 一次蜂窝中断 13.550 秒后约 24 秒重新认证，Wi-Fi 新消息
持久确认，但蜂窝随后 source connect 阶段 E214 blocked：origin=3/code=2
（QUIC CONNECTION_REFUSED）、offerStep=0/sourceCommitted=false。未继续第二、
三轮；原待发消息保留。这与旧 E64 同属粗协议类别，但不证明旧样本皆为同因。
镜像启动约 330 秒时拒接，接近 110 秒 epoch 交接；没有节点逐连接事件，不
将其作为已证因果。应进一步验证初始拒接/停止接入与已认证流协议错误的区别，
本片不扩大重试。Wi-Fi 20 次任务后原许可自然到期；所有本轮节点进程已定向
停止，授权历史保持。手机详情在 NOVOCHAT `AUTOMATIC_RECOVERY.md` 样本 18。

## 2026-10-13 来源刷新首错的有界协议诊断

输入 `b774d32a`、认领 `b116cb6d`，NOVOCHAT `cab7edd` 手机来源刷新 E64 尚未
精确分类。`iroh_protocol_diagnostic_v1` 将 VersionMismatch、TransportError、
ConnectionClosed 分开，记录标准 QUIC 0–17/TLS 256–511，其他码统一 65535；
不输出远端原因、帧内容、身份或地址，不授予任何重试。原 E64 粗分类 API 保留。
V2 offer 写请求、读响应和解码增加 `ServiceOfferStepV2` context，保留原始
anyhow 类型与载体 IO 标记；签名、版本、撤销、预算与 finish 行为均未修改。

Windows 专项 1 项、租约/来源全组 **41/41**（15.39s）、network all-targets
Clippy `-D warnings`（21.83s）通过。载体全组 **23/24**（10.34s），既有
`bounded_drain_joins_local_drivers_without_waiting_for_unreachable_peer` 一次报
local driver retained UDP socket；单独立即复测 1/1（0.20s），不能据此抹去原
失败或宣称回收稳定。诊断变更未调整任何关闭代码/测试预算，后续保持待办。
证据：`artifacts/recovery16-{protocol,carrier,drain-recheck,leases,clippy-final}.log`。
手机首错、消息交付与多轮恢复由 NOVOCHAT 单列，当前没有安全拒绝转重试。
`ebd85ec4` 接入后的样本 16 完成三轮约 18 秒蜂窝断网，认证恢复约 64/30/29 秒；
两端各 18 个任务后按原许可正常 expired。E64 本轮未复现，根因未签收；已释放
源码认领。仍为测试发布者/自有寻址控制面与获准的第三方 Iroh relay 承载。

## 2026-10-12 端点关闭归属与本地完成屏障

输入 `fc3eab44`，认领 `e3cfef8c`。NOVOCHAT 样本 11 在来源交换后出现 E83：
旧 scope 将 endpoint.close 放到 detached task，五秒超时后 Application 会关闭
所属 runtime，任务线程退出无法证明后台关闭完成。源代码另确认 Iroh 1.3.0
开始 close 后的 is_closing 会阻止 abort。首个只检查 Drop 后 UDP 端口的本机
用例在原版即通过，不将“取消一定泄漏端口”作为已复现结论。

`with_iroh_scope_v1` 改为 inline 关闭；原五秒监督中，远端 QUIC 排空最多四秒，
随后仍必须等待本地 actor（超时则 abort 后 await）与 QUIC task tracker 退出。
本地完成后才释放 scope/许可并复核原授权；远端未排空记录计数，不能当作业务
送达证据。业务流 finish、真实回执与原错误保持。取消或本地关闭超时仍失败，
没有将 E83 加入自动重试名单，也没有延长原任务。并发第二个 close 返回 None，
不伪装为另一个 owner 已完成。

最小底层补丁固定在 `vendor/iroh` 1.3.0，保留 BSD3 许可、原 revision/package
checksum 和逐文件 SHA256；只修改 endpoint.rs/socket.rs，network 路径依赖让
手机/节点用同一补丁，根 workspace 排除此第三方包。详见
[补丁来源与契约](../vendor/iroh/NOVOVM_PATCH.md)。未修改全局 Cargo 缓存、
协议/密钥交换、外部服务选择、金融/AOEM/共识。

Windows 载体 **23/23**（11.27s）、租约 **41/41**（15.38s），network/relay
all-targets Clippy `-D warnings` 通过。新增真实 QUIC 用例覆盖取消已开始的
close、端点停止、运行时仍在时 UDP 端口释放；真实 UDP 黑洞/三秒 initial RTT
使远端排空未完成，100ms 测试预算后等待本地回收并验证端口立即可重新绑定。
这是故障夹具的参数，不改变产品 RTT 或四秒预算。首轮编译缺 Duration 导入
已修正，原失败日志保留。日志 `artifacts/recovery12-{before,carrier,leases,clippy}.log`。
手机仍须以正常 APP/原消息验证，不由组件测试推定 E83 或全天候恢复已签收。

## 2026-10-12 新连接引导记录与既有在线租约分开读取

输入 `1eb60c31`，认领 `db5ec3ee`。`ServiceLeaseBookV1::bootstrap_routes`
从同一已认证 book 返回仍在原来源授权内的 grant 原记录，供新连接取得
服务 offer 使用。旧 `routes` 优先返回尚有效的 current 在线租约，接近到期时
可能只剩 50ms；将其作为新发现轮次的期限会导致新流提前结束。新增读取
不延長/重签记录，不重置 current 或序列；拨号仍须 check_route，业务 RPC
仍须新流在线租约，不能用 bootstrap 延长旧流。没有新增目录或协议。

Windows 租约回归 **41/41**（15.18s），含两个新增入口边界测试：短在线
租约仍有效/过期后都可用未到期引导记录开启新握手；持久恢复、来源先到期、
撤销/旧任期/时钟回退均保持拒绝。network all-targets Clippy `-D warnings`
通过。日志 `artifacts/recovery5-service-lease.log`、`recovery5-network-clippy.log`。
NOVOCHAT 使用新 API；实际双手机恢复耗时单独验收，未改变生产能力位，
不签收常驻、隐私、跨网或全球可达性。AOEM/金融/共识保持不动。

## 2026-10-12 连接失败的固定类型诊断

输入 `b5a30aad`，认领 `1650c74a`。`iroh_transport::iroh_connection_failure_kind_v1`
只读检查 Iroh 的 `ConnectError` / `ConnectionError`，将超时、reset、远端正常/
异常关闭、协议/TLS 认证、本地关闭/拒绝、资源耗尽分类给产品层。没有输出地址、
身份或远端 reason 文本；这些类别不是自动重试许可，不改变连接和来源验证流程。
特别是远端 code 0 关闭仍不能仅凭诊断自动重试。Windows 载体回归 **21/21**，
network all-targets Clippy `-D warnings` 通过；日志 `artifacts/recovery3-carrier.log`
与 `recovery3-clippy.log`。NOVOCHAT 的手机 `source_rejected/E8` 原因需装包后
再核对，本片不推测已修好真实恢复，不签收匿名、独立载体或全天候服务。

## 2026-10-12 预备任期交接与来源传输耗尽

输入 `9cfaf15b`，认领 `e1ec8b5f`。NOVOCHAT 上轮实机节点报 `service lease
rollback`。新增确定性回归复现：新任期提前签名之后，旧任期继续签发在线租约；
新 grant 的时间早于旧 current，原统一排序误拒绝合法交接。

调用链仍为 `rendezvous_owner::run_inner → run_owned_rendezvous_epoch_v2 →
ServiceLeaseBookV1::replace_sources`。保持独立发布者验签、source/grant 时间
不倒退、持久序列及撤销历史；只有新实例和新传输公钥同时变化、授权序列严格
大于已消费在线序列，才允许提前准备的 grant 早于旧在线续约。同任期在线续约
仍使用原 `newer` 检查，不重置日志、时钟或身份。相同序列、旧 grant 时间、单独
换实例/公钥、假签名和撤销复活均拒绝。节点/客户端复用同一规则，wire 未变。

`refresh_bootstrap_sources_observed_v1` 在原两轮/两节点/12 秒预算内耗尽纯
传输尝试后，检查原 scope、实际尝试过的镜像许可期限和墙钟未倒退，再返回
`BootstrapTransportUnavailableV1`。外层可在原用户许可内退役旧任务后重开；
协议/签名/存储、撤销、过期仍返回终止错误。类型不能由外部随意构造，不按
错误文本判断。失败不写来源缓存、不使用旧路由；已提交撤销不会因丢 ACK 撤回。

Windows 本机验证：租约/来源刷新 **39/39**，镜像许可 **1/1**；真实节点
进程 **5/5**、独立发布者 **5/5**、镜像 **4/4**；network/relay all-targets
Clippy `-D warnings` 通过。真实进程增强覆盖预备新任期时继续请求旧任期、
切换后通信及重启身份/序列保持。首次新增进程探测复用了已 ACK 的寻址键，
被去重拒绝；改用每次独立的测试对象后通过，没有降低产品去重检查。来源新
故障用例初次因测试闭包生命周期编译失败，修正后通过；不计入通过样本。
日志在本机忽略目录 `artifacts/recovery2-{before-network,network,process,
owner-final,clippy}.log`。手机原消息/断网恢复由 NOVOCHAT 单列，未预报结果。
无 AOEM/金融/共识、默认 Privacy、载体自主性或匿名能力位变更。

## 2026-10-12 在线租约生效等待与差分校准策略

输入 `0eb59a2b`，认领 `f634fb43`。原 `accept_offer` 的来源、撤销持久化与严格
有效期检查保持；新 `accept_service_offer_with_activation_v2` 复用同一 owner，
只为签名、身份、端点、序列和授权上限均通过的未来在线租约提供最多 2 秒等待。
等待时不发布未来租约，不前移 book 时钟；释放锁后等待，持续检查原取消/期限、
实际时间、路由与撤销，达到生效时刻才调用原 `accept_renewal`。失效清单不等待。

Windows 本机 network 租约相关串行回归 **36 通过**，其中新增 6 项覆盖生效前
不授予权限、伪造/过期/超界/错身份/等序不同记录、撤销耐久与等待中撤销、取消/
时钟回退、冻结时钟耗尽单调预算。首次并发整组为 31 通过/5 项真实载体超时；
相同源码串行重跑全部通过，保留原失败记录，不将并发压力稳定性记为已通过。
network all-targets 严格 Clippy 与差异检查通过。

新增 [网络时间策略](NOVONET_TIME_POLICY_V1.md)，明确四时间戳、误差区间、独立
来源、初始信任与防回滚。**有界等待不是分布式校时完成**；版本化认证测量、
统一时间接口和多源实现仍待开发。NOVOCHAT 的普通循环与手机结果单列；无
AOEM、金融账本、共识、网络配置或能力位改动。

## 2026-10-12 多路径自主通讯架构文档

基线 `6f05f98f` 与 NOVOCHAT `9f0f7f3`，核对现有传输、来源/授权、overlay
及上一轮真机记录，逐行阅读用户 386 行参考并核对官方资料，形成
[NOVONET 多路径自主通讯架构与传输策略 v1](NOVONET_TRANSPORT_POLICY_V1.md)。
明确开源技术与第三方服务运营分离、原组件复用、隐私优先、有界学习/切换、
原 Message ID/设备回执、整条依赖链去单点和分阶段验收。

附件的反向回执待办已由上一轮完成，设计顺序改为普通输入框自动收发 →
通用接线/诊断与持续节点 → 自主载体 → 多路径及合格隐私路径。本文约束
不等于各适配器上线。本轮仅文档与链接/差异核对，无新运行验证、网络配置
或部署，原验收结果与未完成项仍按下一节及对应实测记录解释，能力位 0。

## 2026-10-12 正常 Android 跨网原消息回执恢复

通用网络输入 `51d50bf2`，手机接入与证据记录在 NOVOCHAT `9f0f7f3` 的
`docs/implementation/APPLICATION_DELIVERY.md`。两台正常包原位升级，保留
实际身份/联系人/授权历史；PKL110 Wi-Fi、PCLM50 蜂窝，无 ADB 数据隧道。
09:06 新版本第一次尝试恢复上轮反向原消息的签名持久回执，未创建替代消息；
09:08 两端重启后，两个方向的正文各保留一条，各自发送方送达确认仍保留。

此轮复用前次 `c11c4e3c` 构建的发布者、寻址节点、来源镜像程序，实际哈希
与前次记录一致，原 DPAPI 身份、配置和期限未重置；新客户端与原 V2 服务
互操作通过。结束后定向停止三个测试进程，保留密钥与历史。客户端 Application
回归 29 通过/1 显式忽略，Flutter 38、Android JVM 12、严格检查及普通包构建
通过；通用网络测试见下一节。未变更 AOEM、金融、共识或主机网络配置。

这是显式工程模式的有限真实样本，尚未接普通聊天输入框的全自动在线循环。
节点/镜像使用已明确批准的第三方 Iroh carrier，消息实际直连/中继尚未逐次
测量；不计纯自有中继、匿名、系统常驻或 24 小时稳定性。手机未刻意注入
断网/丢 ACK，本次成功不证明所有重试分支均在手机发生，也不能反推上次失败
原因。发布者文件缺失退出、镜像许可自动续权及路径统计仍待处理，能力位 0。

## 2026-10-12 手机来源刷新预算与安全重试

输入 `c11c4e3c`，认领 `5c701cdd`。上轮 NOVOCHAT 正常 APP 跨网已实际
收到双向正文，但蜂窝端来源刷新间歇失败，原消息回执恢复未通过。本片拆开
原来共享的 3 秒窗口：单次连接至多 5 秒、读取至多 3 秒、结束流至多 3 秒，
全部服从同一 12 秒刷新预算和调用方原始期限/撤权。最多两个已批准镜像、
两轮顺序尝试，轮间 150–405ms 有界抖动；复用同一 endpoint，不重置作用域。

`refresh_bootstrap_sources_v1` 保留原接口并调用唯一 observed 实现。新增固定
事件只报告阶段/原因，不含 IP、公钥、原始错误或聊天数据。Iroh 仅给实际
连接的 timeout/reset 增加可重试标记；本地策略、身份、TLS/协议错误不混成
不可达。读取仅接受原载体 IO 标记或阶段超时作为重试条件；坏版本、签名、
撤销校验或存储拒绝终止，不能消费旧缓存降级。完整撤销先持久化，即使结束
流失败也不撤回。镜像权限过期不拨号，不因为重试扩张信任根/期限/候选。

Windows 真实 loopback QUIC 新增故障测试 4 通过：响应丢失、读取超时、
坏签名/错误版本、原期限/许可及退避撤权。租约回归 30 通过（含这 4 项）、
原 Iroh 20 通过；network all-targets/no-deps Clippy `-D warnings`、改动 Rust
格式及 diff 检查通过。初版测试闭包所有权编译错误已修正并复跑。
日志 `artifacts/bootstrap-recovery-{tests,lease,carrier,clippy}.log`。

这不是手机成功率、NAT 直连或匿名保证；NOVOCHAT 需按新的输入树构建并
实测原消息回执。本片未改发布者/镜像/节点 owner，前轮发布者 `os error 2`、
镜像许可自动续期、系统常驻/24 小时和实际数据路径统计仍待处理。没有改变
AOEM、金融、共识、路由器或默认 Privacy，能力位保持 0。

## 2026-10-12 受保护来源镜像与已激活邀请导出

输入 `4e637a12`，认领 `468122df`，功能 `776640cb`。新增通用 Windows
`novovm-bootstrap-mirror` owner，独立随机 DPAPI 传输/存储密钥；只分发
已有签名来源，不拥有节点发行权。镜像公钥在固定许可内跨重启恢复；普通
客户端和寻址任期的临时身份不变。节点只在激活/持久接受后原子发布来源，
包括撤销；预备任期不得作为手机当前路由。原 V2 和加密 CAS 日志继续复用。

真实镜像进程专项 **4 通过**，发布者 **5**、节点 **5** 回归通过，原载体
**20 通过**，network/relay all-targets 严格 Clippy、目标 rustfmt/diff 检查
通过。覆盖双客户端、镜像重启后的原邀请、新任期来源和原认证 RPC、撤销后
重启拒绝旧源、伪造/缺失密钥、固定许可过期/变更拒绝。父测试进程不签清单。
日志与具体调用链见 [来源镜像契约](NOVOVM_BOOTSTRAP_MIRROR.md)。

此前一次切换窗口拨号超时已记录：已签名不等于仍在线。最终测试先观察
新任期来源再拨号；不能把这一同步条件当作产品自动重连通过。镜像每次交换
有并发、频率和期限上限，源更新先落盘再清理旧请求；过期/无效源不回退。

本轮 loopback 直接 QUIC，无第三方 relay、无系统服务安装/手机操作。
邀请外层镜像/根/DNS 仍须可信带外交接，内层签名不能为外层授信。镜像
许可最多 24 小时、配置不可自行延长；许可/地址自动续权、不同服务账号隔离、
系统重启/24 小时、日志轮转和完整旧备份外部新鲜度尚未交付。
正式 APP 的 Wi-Fi/蜂窝消息及匿名路径仍未验收，能力位保持 0。

## 2026-10-11 独立受保护签名发布者入口

输入 `03406bd1`，认领 `712d8f88`，功能 `ba30bcf4`。在既有 `novovm-relay`
增加 `novovm-bootstrap-publisher`；节点可 `--provision` 托管公钥后再启动网络。
一次明确登记固定部署策略，新任期自动验证节点签名/公钥/记录/TTL/中继许可，
原签名清单先加密 CAS 持久化再原子交付。不自授根，不改变原 V2/节点认证/
寻址 RPC，不改 AOEM、账本或共识；NOVOCHAT 所固定的 network 输入树未变。

发布者独立随机 DPAPI 密钥与加密签名日志，节点只得到签名结果；同机当前用户
真实两个进程验收通过，不等同于跨 Windows 账号隔离。当前每部署策略限定
一个节点，保留两代记录，不能从配置更新自动替换节点/根/期限；在线撤销
先持久 tombstone，再发布空候选来源，删除请求或重启不能复活。复核并测试
撤销第一阶段提交后的恢复，不能以同一毫秒发行不同清单。节点 proposal
改为原子发布，避免真实进程读到半份记录；平台私有文件 helper 共用。

实测：发布者进程专项 **5 通过**，原节点进程回归 **5 通过**；发布者单元入口
**3 通过**、节点单元入口 **2 通过**，两 binary 的 DPAPI 项重复，不相加。
预登记加入后再次运行发布者进程 5 项通过；严格 relay all-targets Clippy、
目标 rustfmt、diff 检查通过。分别覆盖提前续权、稳定身份/递增序号、双方
重启后原 PUT/GET/ACK、在线撤销、精确重发、防伪造/等序号变更、策略变化和
丢失密钥；单元覆盖 TTL/relay/时间/丢日志及撤销提交中断。
日志 `artifacts/publisher-{check,process-first,final-tests,provision-tests,clippy}.log`。

本轮路径仅本机 loopback 直接 QUIC，无公网或第三方 relay，无手机操作。
未安装新 SCM 服务，未验系统重启/24 小时；发行端单签，阈值协调、发布者
专用服务账号安装、16 MiB 日志轮转、完整旧备份外部新鲜度仍待实现/验收。
手机常驻来源镜像、镜像端点续权和可信邀请发行仍是下一接缝；正式 APP 的
Wi-Fi/蜂窝消息未验收，不能把授权文件当成已经激活的手机可达路径。
[详细契约](NOVOVM_BOOTSTRAP_PUBLISHER.md)。

## 2026-10-11 已批准镜像的离线来源刷新组件

集成复核至 `5fb8d997`：先执行调用方授权回调再获取缓存锁，避免 Application
同一缓存重入；签名来源持久保存后，遵守原载体双向 finish 完成契约，避免
首个客户端中断镜像的连续服务。NOVOCHAT Application 28 项本机测试通过，
包括两个 Application owner、独立镜像与寻址进程、全部旧地址过期、第一镜像
不可达、缓存恢复后的两轮 E2EE 消息落盘/回执；默认 Privacy、错签、撤销、
取消拒绝通过。此为 NOVOCHAT 专项证据，不是 SUPERVM 全工程/手机跨网签收。

输入 `0809a806`，认领 `80d77d22`。`product_bootstrap_refresh.rs` 使用既有
Iroh 载体和 ServiceLeaseOfferV2 前导。最多两个明确批准的镜像，每个等待
3 秒，服从调用方原始任务期限/取消；镜像只返回签名来源，不能签在线租约。
客户端固定公钥、来源 ID、候选界限、时间/序号、防撤销复活及持久 CAS 不变。
完整更新保留本地来源类型/优先级，遗漏来源被拒绝，先落盘再暴露新地址。
断连或超时可选择第二个已批准镜像；协议/签名/撤销失败不回落旧地址。

镜像地址由明确邀请提供，许可最多 24 小时并加密持久保存；旧来源过期不等于
镜像地址获准续命，镜像过期后不得拨号。请求只有固定前导，不上传聊天身份、
联系人/消息；镜像仍能看到来源 IP 和时间，普通工程路径不具有匿名资格。
这只是有界分发组件，不是全网目录、全网反枚举实现或正式发布者部署。
平台镜像密钥托管、镜像端点更新、授权发布进程及真实手机接线仍须验收。

本机租约专项 26 通过；bootstrap 过滤回归 25 通过（两组有重合，不能相加），
network/relay 严格 all-targets Clippy 通过。日志
`artifacts/bootstrap-refresh-{network,final,clippy}.log`。后续 NOVOCHAT 实际
Application 进程测试另列结果；本组件结果不签收手机跨网/SCM/24 小时运行。

## 2026-10-11 Windows 托管 owner 与跨任期预授权

输入 `fde02bdf`、认领 `eb1e55d3`。新增 `novovm-rendezvous-service` 作为
SCM 平台 owner，仍调用同一个 `run_owned_rendezvous_epoch_v2`、Iroh、
ServiceLeaseHost 和原 V2 寻址协议；没有替换 node 产品控制器或金融功能。
CurrentUser DPAPI 封装独立签名/存储材料，owner 加密日志先预约全部任期序号，
新实例的外部授权提前准备，激活前重读来源并核对持久撤销。原有界 CLI 保留。
SCM 自动启动、专用虚拟账号、两次有延迟的崩溃恢复、停止取消及程序绑定 UDP
防火墙安装代码已提供；仅 Status 实测和语法检查，未安装/重启验收。

本机 Windows 最终验证：平台 owner 单测 **2 通过**（真实 DPAPI 篡改拒绝、
无发布者时常驻退避和停止、重复 owner 不改活跃状态）；新增真实 executable
进程专项 **5 通过**；原有 executable 回归 **4 通过**；服务租约回归
**24 通过**；network/relay all-targets 严格 Clippy 通过。测试使用明确夹具
发布者及本机直接 QUIC，无第三方 Relay 或公网跨网投递；未重跑完整金融工程。
日志 `artifacts/node-owner-final-process.log`、`node-owner-lease-regression.log`、
`node-owner-clippy.log`。Rust 文件格式、PowerShell parser、diff 检查通过。

测试发现并修复两个实际生命周期问题：墙钟毫秒截断导致自然结束偶发被当成
授权过期，改为提前 250 ms 停止服务；ready 通道关闭抢先遮蔽缺少回复的真实
错误，改为等待载体完成并保留类型，只对缺少回复退避。原始失败分别保留于
`node-owner-process-first-expiry-failure.log`、`node-owner-first-retry-failure.log`。
一次重复构建遇到 Windows 已运行测试 exe 的 LNK1104，串行等待后重跑通过；
没有结束无关进程、放松验收或升级无关依赖。

用户确认暂无正式发布者，先实现与独立进程验证。发布者是已配置的服务信任
策略，不是全网官方审批门槛，也不要求用户购买第二台主机。正式发布/手机
端点更新、SCM 虚拟账号实际托管与开机恢复、24 小时及手机 Wi-Fi/蜂窝验收仍
未完成。两个日志各 16 MiB，安全轮转/完整备份防回滚未交付。详情见
[Windows owner 契约](NOVOVM_RENDEZVOUS_WINDOWS_OWNER.md)。认领释放。

## 2026-10-11 寻址服务入口与单一协议实现

输入 `68eacb80`，认领 `09f5d39b`。NOVOCHAT 通用 bound_session / discovery_control
迁回 network，保持既有线格式与回归测试；客户端仅导入。既有 supervm-relay
通过共享 selector 显式选择 rendezvous-v2，复用签发 host、加密日志、来源
验证、Iroh 载体、Finished 及不透明寻址存储，没有新增聊天中继协议。

真实 executable 两客户端双向 PUT/GET/ACK、重复 ACK、删除与连续两次重启
通过；序号耐久恢复、旧来源/错误根/缺失恢复/未经许可/旧线格式/未知身份
拒绝；在线签名撤销保存墓碑并终止活跃服务。进程专项 **4 通过**。第一次撤销
测试误将立即收到 QUIC 正常关闭作为前提，发生载体超时；改为有界拒绝迟到
数据并核对服务退出/耐久撤销，失败日志保留，不把终止时间当公网恢复性能。

Windows 网络 lib 串行 **823 通过、11 原有忽略**，默认 feature selector
**7 通过**、TLS-only check、network + relay 严格 all-targets Clippy 通过。
日志 `artifacts/rendezvous-launch-{network,process,default,tls-only,clippy}.log`。
非完整 node/金融链或双手机跨网验收。运行入口只承载 5–110 秒授权任期；
密钥托管/Windows 常驻安装 owner、跨任期来源发行与自动监督、长期容量仍待完成。
普通工程路径不授予匿名资格，无隐私路径仍失败关闭。详见
[运行契约](NOVOVM_RENDEZVOUS_ENTRY.md)，认领释放。

## 2026-10-11 耐久节点签发与 V2 签名来源刷新

输入 `e9e7dc3a`，认领 `bf99ad39`。通用实现位于 network 的
`product_service_host.rs` / `product_service_file.rs`，由既有
`product_service_lease.rs` 导出。ServiceLeaseHostV1 仅接收已经附接耐久存储的
ServiceLeaseBook，复用已有根信任、来源、签名记录及序号校验。签发先将完整
授权状态 CAS 提交并 sync，再交付租约；丢回包/节点进程退出不会复用序号。
服务端只持节点签名密钥，来源须由独立受信发布者签名，不能自授新根或续权。

显式 `NVLSRQ02/NVLSRS02` 前导返回有界签名来源及在线租约，最大 16 KiB，
不探测或回退 V1；原 V1 API 供显式旧调用保留，不由新客户端使用。客户端
accept_offer 按已批准来源 ID 合并，保留其他来源、当地 source kind/priority；
新来源 ID、换锚、回滚和复活撤销均拒绝。有效签名撤销先保存，即使节点不再
有租约也不能继续聊天 IO。在线租约最多 120 秒，可缩短；来源最多 10 分钟。

ServiceLeaseFileV1 使用独立外部存储密钥、XChaCha20-Poly1305 随机 nonce 和
链式 AAD，持有 OS 文件锁；create 与 open 分开，缺失/破损/错密钥/旧 CAS/
不确定写入停止，绝不自动重置。每次写入重验有界日志后追加并 sync。日志上限
16 MiB，达到上限停止，尚无压缩/轮转；不是无限运行/性能签收。平台安装 owner
仍须提供私有、备份排除目录和密钥保护，本模块不在磁盘存密钥；完整有效旧备份
回放与恶意管理员回滚仍需外部 freshness anchor。现有 Windows WSS 安装器未
接到该 Iroh service host，不把测试进程称为完成安装的自主节点。

Windows `cargo test -p novovm-network --features iroh-transport,tls-transport
--lib product_service_lease -- --test-threads=1` **24 通过**：两次真实子进程重开、
真实 loopback Iroh V2 交换、签发先落盘/丢回包、独立来源保留、签名刷新/撤销、
错根、错密钥、残缺日志及写失败等。严格 all-targets Clippy 通过，日志
`artifacts/service-host-{tests,clippy}.log`。未重跑全网络套件，未操作金融/AOEM/
共识、手机或路由器，不宣称匿名、生产安装或手机跨网签收。认领释放。

## 2026-10-11 节点授权耐久恢复契约

输入 `22d6d9d4`，认领 `d88394d0`。`product_service_checkpoint.rs` 是已有
ServiceLeaseBook 的本地恢复格式，不新增网络协议；恢复时使用外部信任政策重验
历史签名及有界状态，保留节点撤销、序号、签名源和已持久化时间下限。允许读取
过期历史，但过期来源不再提供连接资格。续租/来源替换先原子保存再发布；
未知写入结果或 CAS 失配后，本 book 停止提供路由，不能恢复旧内存授权继续连接。

平台实现 `ServiceLeasePersistenceV1`，负责加密与准确比较旧状态；附接时也核对
落盘状态。历史导出不能自报新信任锚。Windows 租约专项 **15 通过**，包括真实
loopback 载体、重开后旧序号拒绝、撤销不复活、过期来源不可拨号、错误信任锚、
篡改及写失败停止；严格 all-targets/no-deps Clippy、格式和差异检查通过。
日志 `artifacts/service-checkpoint-{tests,clippy}.log`，本轮未重跑全网络测试。

持久时间下限在授权变更时保存，并非每次只读路由检查都刷盘；同密钥的完整旧
备份仍需外部 freshness anchor 检出。本组件不持有客户端密钥、不替平台落盘，
不证明节点安装签发/源更新服务已交付，也不代表匿名或双手机跨网。认领释放。

## 2026-10-11 签名寻址服务授权与在线续期

输入 `f110c5f7`，认领 `5d0c76e5`。`product_service_lease.rs` 复用
`product_directory` 的签名清单/记录，显式新增 `IrohRendezvousV1` 类型；
其 URI 包含规范编码的临时载体公钥/端点及服务实例，不冒充旧 WSS/QUIC。
独立提供的信任锚验证来源，最多 4 来源/2 个候选/8 锚，来源授权最长 10 分钟，
节点记录最长 120 秒。真实载体的版本化租约前导交换在原寻址握手之前；旧服务
不会静默协商成功。自签节点不能给自己增加信任或续期上层授权。

`ServiceLeaseBookV1` 拒绝伪造、弱公钥、回滚、同序号异文、错实例/载体、
超额候选和时间倒退；更新来源可撤销节点，进程内墓碑禁止重新引入。
`ServiceLeaseIssuerV1` 仅使用服务自己的密钥签发严格递增序号，不含用户或钱包
身份；生命周期由现有 carrier owner 控制。调用者仍负责路由政策、实际双方
持钥握手及取消。此功能不授予匿名资格，不改账本、UCA、交易、共识或 AOEM。

Windows `novovm-network --features iroh-transport,tls-transport --lib
-- --test-threads=1` **790 通过、11 项原有忽略未执行**；新增租约专项
**12 通过**，含实际 loopback QUIC 连接连续三次签名续期；all-targets/no-deps
严格 Clippy 通过。首次 socket 夹具遗漏关闭同步导致失败，修正测试 ACK 屏障后
通过，没有放宽生产超时或校验。日志位于本机忽略的
`artifacts/service-lease-{tests,full,clippy}.log`。

`product_node_overlay.rs` 仅补新增枚举的测试夹具穷尽分支，原产品 WSS 路径
继续显式拒绝不支持的载体。来源抓取/可信安装包、加密耐久节点缓存和跨进程
防回滚尚未交付；issuer 重启须由安装 owner 提供不回退的序号基础/新签名来源，
不能把进程内测试称为设备重启通过。NOVOCHAT 主应用及双 Android 跨网另验。
认领释放。

## 2026-10-11 弱网节点参与与只读网络自检

输入 `afd03228`，认领 `4f28ecc7`。新只读脚本
`scripts/windows/Get-NovovmNodeNetwork.ps1` 调用同目录 `NovovmNodeNetwork.psm1`，
区分本机协议禁用、地址/默认路由条件；不发公网请求，不修改网卡/路由器。
39 项夹具断言通过，真实本机 WLAN 为 IPv6 禁用、私有 IPv4/默认路由可用。
用户明确节点不得以 IPv6、公网入站或用户改路由器为前置，因此输出保留
主动外连候选资格，自检不接入任何运行时阻断门禁。地址范围不能证明 NAT
类型或外网可达；后续应用仍复用既有认证可靠载体与隐私政策。
本片不是新 NAT 协议/主应用投递交付，不改身份/交易/共识/AOEM。
详见[安装与弱节点说明](NOVOVM-NETWORK/WINDOWS_RELAY_INSTALL.md)，认领释放。

## 2026-10-11 Windows 中继安装与持久防火墙

输入 `adbf7f07`，认领 `7e22f00`，实现与本节同提交。用户将临时验证改为
持久放行。新 `scripts/windows/install-novovm-relay.ps1` 在安装/升级时自动
配置唯一专属规则，路径来自 OS Program Files，程序绑定 `supervm-relay.exe`，
仅 TCP 443 入站，覆盖全部 Windows 网络类别。重复安装复用，卸载定向清理；
不关闭防火墙、不改出站/路由器、不变更 node/网络协议/账本/共识/AOEM。

真实 UAC 安装及重复安装成功，持久/生效规则逐项读回一致，安装二进制哈希
与已验上游一致。正常用户启动已安装 duplex-v2 节点，PKL110 Wi-Fi 身份握手
通过，探针整命令 0.316 秒，本机正对照也通过，ADB 隧道为零。节点 120 秒
正常退出，规则继续保留。19 项策略夹具断言及三个脚本解析通过；未实际卸载
或重启系统。工程证书只用于探针，未放宽主应用 WebPKI/节点 pin；未发送聊天，
不计作 Wi-Fi/蜂窝双机或匿名验收，不交付自启动服务/全节点 GUI 安装器。
源码位置、真实调用链及本地证据见
[Windows 中继安装](NOVOVM-NETWORK/WINDOWS_RELAY_INSTALL.md)。认领释放。

## 2026-10-11 单会话 WSS 字节流与受保护消息接入

输入 `1e15ed8f`，认领 `d9a67eb7`，源码 `60546962`。通用实现为
`novovm-network/src/duplex/byte_stream.rs` 与原 `duplex/mod.rs` 导出，
调用原 `ProductRelayClientV1::connect_with_control → into_pipeline →
NodeHandshake → E2eSecureChannel → NovoRUDP Data/Done`。没有平行中继
协议，也未改变既有 pipeline、daemon、AOEM、账本、共识和交易网络。

同一流固定会话、单中继及独立 peer pin，强制验证外层 TLS 证书；严格
检查连续 envelope/frame 序号、累计字节位置、session/domain。原期限和
授权覆盖 DNS/连接及后续 IO，裸 EOF、失序、换会话或撤权永久失败。
write 只表示本地准入；flush 只表示匹配的中继转发准入，不能代替对方
接收或落盘。认证 Done 是半关闭，完成要求双方 Done、所有本地准入
确认和实际 worker 退出。IO 正常关闭后丢弃可在原期限内排空 Done；
提前丢弃会取消，线程退出前保留配额，不声称错误返回即同步回收。

至多四个 worker，16 KiB 块、各方向 64 KiB 队列，另有一个待入队块和
原 pipeline/TLS/socket 的独立缓冲；不宣称这是整个进程内存上限。
入站队列满会暂停 pipeline 推进，必须持续消费，不承诺阻塞读者下仍
无限独立发送。没有自动发现、路由资格、重连、匿名或聊天业务逻辑。

Windows `cargo test --locked -p novovm-network --features
iroh-transport,tls-transport --lib -- --test-threads=1` **778 通过，11 项
原有忽略未执行**。新载体 **10 项**包含：真实 WSS 双向各约 192 KiB，
缓冲满后恢复且逐字节一致，认证半关闭及连续三次关闭/重开，撤权缓冲
拒读与唤醒，真实 daemon 断线不算 EOF，缺席 peer 原期限，连接取消
后真实配额/注册回收，四 owner 上限和第五个未拨号，以及 TLS finish
后立即 Drop 并确认线程真实 join。全套串行避免既有 TLS scope 测试
之间的进程配额竞争，未放宽生产上限。严格 all-targets/no-deps Clippy
及限定文件格式检查通过。日志为本机忽略的
`artifacts/novochat-byte-stream-full.log` 和 `novochat-byte-stream-clippy.log`。

NOVOCHAT 在这版源上复用原受保护 outbox/inbox、身份/联系人/路由许可
及签名持久回执，授权 TLS/WSS 专项 **22 通过**，其中新 WSS **8 项**。
真实本机 loopback daemon 支持消息落盘与丢回执后的原 ID 查询恢复；
同进程存储重开和显式故障注入不是手机进程重启或实际丢包证据。
正常应用任务、受信节点/端点交换、双 Android 跨网及自主匿名仍待接通。
默认 Privacy 仍拒绝普通中继；当前本地 UCA 持钥不等于生产链上授权。
认领释放。NOVOCHAT 的完整调用链与复现见其
`docs/implementation/OWN_NETWORK_PROTECTED_DELIVERY.md`。

## 2026-10-11 既有 duplex 中继部署入口

输入 main `7febd4ad`，认领 `443fa1eb`，源码提交 `c76e81e2`。限定修改
`novovm-network/src/product_relay_launch.rs` 与导出、轻量 `supervm-relay`
及节点 `novovm-product-relay` 两个 main，不改任何 daemon、wire、身份政策、
账本、AOEM 或共识。历史单位置参数和显式 `legacy-json` 仍进原实现；
精确 `--runtime duplex-v2` 只进 `duplex::product_relay_daemon`，读配置、
TLS、绑定或协议错误均直接返回，不回退旧实现。两个 daemon 的历史报告
版本都为 2，模式与实际协商证据单独核对，不拿数字相同当兼容。

Windows 单线程启动选择器测试 **6 通过**，覆盖严格参数、单次分派和真实
坏配置不调用 legacy。轻量部署 binary debug 构建通过。日志保留在
`artifacts/novochat-relay-launch-tests.log`、`novochat-relay-cli-build.log`。
另一启动入口 `cargo check --locked -p novovm-node --bin novovm-product-relay`
通过（包含原有 RocksDB 依赖编译），日志 `novochat-node-relay-entry-check.log`。
network 与轻量 relay 的 all-targets/no-deps 严格 Clippy 通过，日志
`novochat-relay-launch-clippy.log`。未把 node 的 check 宣称为其部署运行验收。

NOVOCHAT 在本机 IPv4 loopback 真正启动本仓 `supervm-relay.exe`，两个
独立客户端完成节点签名挑战握手、原 E2E 会话、各 8 条 Data 和 8 条
接收 ACK，双向 16 个密文摘要逐一一致。Ack 复用原 NovoRUDP frame，
绑定会话/方向/完整 Data，明确仅认证接收核对，不是持久聊天回执。
新服务拒绝缺失、旧和混合子协议且新版正对照成功；错 pin 未就绪、
前后正确 pin 均认证成功；另启动真实 legacy 服务，确认监听后由新版
客户端以缺失 required subprotocol v2 明确拒绝。两服务正常退出，
新版活动连接为 0。协调器有原 120 秒总期限和精确子进程回收。

客户端与协调器属于 NOVOCHAT 原诊断工具扩展，Rust **7 项**、Python
**9 项**及客户端严格 Clippy 通过。原始报告在 NOVOCHAT 本地忽略目录
`artifacts/relay-checks/ee66e79417c24639bb681aaecc824f8b/report.json`，
`passed=true`；部署 binary SHA-256 为
`ac24be61a5a71b8ebf0a991995c7734e6698e0bfaef6a906e30d7e8b29949cc8`。
密钥、构建产物、逐帧证据不提交。不是双手机/公网/生产 UCA/匿名验收，
也不是全项目安全审计；未触碰默认 Privacy 或旧交易网络安全边界。
下一刀为通用有界 WSS 单会话载体和 NOVOCHAT 受保护消息投递接合，不能
把自动重连的 worker 队列拼成同一 TLS 流。文件认领释放。

## 2026-10-11 可选 TLS 认证流与共同帧接口

同轮格式收尾：NOVOCHAT 的 `cargo fmt --all` 也访问本地路径依赖，发现并
纠正 `lib.rs` 中 TLS/transport 模块声明的字典序。仅调整声明顺序，没有
语义或 feature 变化；依赖输入守卫正确拒绝未提交的排序，随后固定到
包含本次格式收尾的提交，不放宽守卫。原测试证据范围保持如下。

输入 `60d4f3b3`、认领 `30cf2468`，限定既有 `novovm-network`、根 lockfile
和两份台账。`tls-transport` 默认关闭；仅接收调用者已授权的 owned IO，
没有 DNS、拨号、Tor 或路径许可。TLS 1.3 使用独立临时 Ed25519 raw public
keys、双方真实 CertificateVerify、规范素数阶公钥校验及固定 ALPN，关闭
0-RTT、会话恢复、SNI。真实 exporter 仅在模块内生成原
`VerifiedTransportBinding`，不开放外部证据构造或密钥导入。

`CarrierStreamV1` 的私有 variants/halves 只包装真实 Iroh/TLS 流，复用
原二进制 envelope 编码。每帧最多 16 KiB，写入必须 flush；帧中途取消
永久使双方失效并立即关闭 owned IO。固定期限最长 120 秒；每进程至多
4 个 TLS scope、每 scope 4 条已准入连接。每条成功流的运行时监督任务
在空闲时仍检查期限/撤销，Drop 关闭 IO 并取消任务；已取消任务的内存由
Tokio 调度回收，不将连接上限误称为任意瞬时任务内存的硬上限。
完成要求双向 close_notify 且没有尾随数据，裸 EOF 不算成功。

Windows 网络 lib `--features iroh-transport,tls-transport --test-threads=1`
完整 **762 项通过、11 项原有忽略未执行**。补充声明 A 公钥却由 B 签名的
真实 TLS 反例后，TLS 专项 **11 项通过**，包括错误/缺失身份、错 ALPN、
跨连接重放、双工大帧、半帧取消、篡改/尾随/截断、空闲到期/撤销及释放。
严格 all-targets/no-deps Clippy 与只启用 TLS feature 的 lib 编译通过；
日志为 `artifacts/novochat-tls-full.log`、`novochat-tls-focused-final.log`、
`novochat-tls-clippy-final.log`、`novochat-tls-only-check.log`。

此为本机组件证据，尚非 Android、真实 onion 网络、主应用或匿名路径验收。
底层 IO 的来源必须先由调用方授权；TLS 持钥不替代 UCA、联系人固定密钥、
隐私资格或持久收件回执。NOVOCHAT 负责后续接线，认领释放。

## 2026-10-10 本地未注册身份的受限授权

输入 `133b1c38`，认领 `54375526`；限定 `novovm-adapter-api`、根 lockfile
及两份台账。新增 `local_identity`，保留既有 UCA Ed25519 primary key ref。
签名固定新域、版本、可信根与引用、用途、设备、完整范围摘要、实际主体公钥、
challenge、独立撤销 epoch 与签发/到期时间。规范强钥及 prime-order 检查、
`verify_strict`，拒绝根/主体复用。证明与验证结果不提供网络序列化或 Debug，
签者无通用签名和私钥读取接口；明确启用既有 ed25519-dalek 的密钥清零能力。

验证必须由受保护调用者提供可信 root pin、精确预期绑定和当前活动 epoch，
不能从证明自身认可签发者。请求在签发时最多五分钟，授权最长二十四小时，
过期不续期。根持钥证明本身不负责请求消耗、持久时钟、撤销或平台授权；
这些仍须由 NOVOCHAT 本地受保护所有者实现。多个 scope 的独立证明不互相
替换，也不使用旧账户 create/rotate 签名字节授予聊天权限。

Windows `cargo test --locked -p novovm-adapter-api` **20 项通过**，含新增
10 项真实签名反例和原包回归；严格 all-targets/no-deps Clippy、相关源文件
格式及 diff 检查通过。日志 `artifacts/novochat-local-identity-api-full-tests.log`
与 `artifacts/novochat-local-identity-api-clippy.log`。没有测试整个交易节点、
AOEM、远程 CI 或手机；此处只交付公共密码契约，认领释放。

根/授权不得发送给联系人、发现或中继节点，稳定引用可能关联会话。
此能力是本地未注册身份授权，不是链上 UCA、全局撤销同步、硬件证明、
匿名路径或 PQ。原链上账户/交易入口和生产能力门禁保持。

## 2026-10-10 流 IO 失败与协议错误分类

输入 `983fe493`，认领 `898b9db5`；限定既有可选 iroh 载体及交接台账。
新增外部不可构造的 `IrohStreamIoFailureV1`，仅真实 QUIC 流读写错误和
明确的对端 STOP 产生该标记，并保留原始错误来源。帧长度、尾随字节、
授权、失败锁存及本地 finish 状态错误仍保持原分类。它不代表对端主动取消，
也不授予重试、身份或隐私降级权限；NOVOCHAT 诊断须另核双端事件。

Windows 网络 lib 串行 **751 通过、11 原有忽略项未执行**（44.33 秒）。
新增真实本机 QUIC 断连、超长帧/尾随、STOP 三项回归，加强原有边界与撤销
反例；没有外网故障或手机验收结论。日志为
`artifacts/novochat-stream-io-network.log`。严格 all-targets/no-deps Clippy、
单文件格式和差异检查通过，日志 `artifacts/novochat-stream-io-clippy.log`；
认领释放。
未改变 wire、默认路由、身份政策、交易或共识。

## 2026-10-10 显式仅中继载体约束

输入 `d8490975`，认领 `d1b1f47`；仅 `iroh_transport.rs` 和两份交接台账。
`IrohConfigV1::with_relay_only` 必须提供批准的中继，使用 iroh 的
`clear_ip_transports` 真正关闭 IP 载体，而不只是隐去地址提示。发出的地址
不含 direct，拨号先验证原始候选边界再剔除 direct，无批准中继直接拒绝；
与 IP bind 的两种调用顺序都拒绝。原默认自动模式、身份绑定、准入限额、
3 秒入站与上层 8 秒认证期限保持。单中继不能作为合格匿名路径，
HTTPS 上线探测和显式 UDP DNS 仍可能存在，不宣称全程无 UDP。

本机网络 lib 串行 **748 通过、11 原有忽略项未执行**，严格
all-targets/no-deps Clippy 通过。新增配置冲突、真实 endpoint 无 IP socket /
候选过滤、等待上线时撤销并回收准入三项反例；测试使用本地不可用中继，
没有据此声称公网中继通信成功。证据：
`artifacts/novochat-relay-only-network.log`、
`artifacts/novochat-relay-only-clippy.log`。认领释放；NOVOCHAT 另行构建并执行
手机自动模式/仅中继对照，当前握手失败根因尚未确认。未改 NOV 交易、
账户、共识、AOEM，也未完成主应用、恢复或合格隐私路径验收。

## 2026-10-10 临时 endpoint 接入与有界不透明寻址

输入 `30ff2f4c`，认领 `4407ade1`；仅 network 的 `iroh_transport.rs`、
`opaque_rendezvous.rs`、模块导出及台账。`accept_inbound` 不要求预先通过
电脑传入客户端每次新建的 transport key；只建立 TLS endpoint 身份，上层
必须在同流核验独立允许列表身份、TLS binding 和双向 Finished，才可访问服务。
入站/出站共用每 scope 4 条活跃准入（含建立中），一个 accept 消费者；
入站 TLS/首个双向流共用 3 秒，取消/超时关闭已建连接。该上限不是 iroh
内部 pending-Incoming 队列上限，当前公开 builder 未接入同等队列配置；
生产抗滥用与持续服务尚不能据此签收。

通用 rendezvous store 不做文件/网络 IO，不解析内部密文。调用者提供真实
已鉴权 owner、随机 slot 能力和新服务 instance，并负责持续 TTL 清理。
限制单记录 4096 字节、TTL 30 秒、16 条有效记录/64 KiB、每 owner 8 槽，
实例最多 64 槽/120 秒。首次写入绑定 owner，更高修订可替换；同修订同内容
只返回原期限，过期/已消费不能复活；ACK 必须精确匹配修订和摘要。
墓碑不驱逐，时钟回退永久拒绝；满额时拒绝，不能用淘汰高水位放过重放。
这不是聊天信箱、UCA 准入或恶意节点删除证明。

本机验证（`CARGO_TARGET_DIR=<SUPERVM_ROOT>/target`、`CARGO_BUILD_JOBS=1`）：
`cargo test --locked -p novovm-network --features iroh-transport --lib -- --test-threads=1`
为 **745 通过、11 原有忽略项未执行**；严格 all-targets/no-deps Clippy 通过。
增加 5 条真实载体反例与 14 条存储反例。首轮误用默认并行测试，711 通过、
34 条既有全局 runtime 状态测试失败；纠正为仓库串行方式后通过，不覆盖失败日志。
本地证据 `artifacts/novochat-rendezvous-{network-tests,network-serial,clippy}.log`。
认领已释放；NOVOCHAT 的端到端加密地址、节点 RPC 和跨网设备证据另行记录。
未改 NOV 交易、共识、账户、AOEM，也未签收主应用、移动切网或合格隐私路径。

## 2026-10-10 可靠载体双工帧与有界二进制密文

输入 `4d8b928c`，认领 `a3c2367f`、`728e58b0`；仅原 network 的
`iroh_transport.rs`、`duplex/product_relay_wire.rs` 与重导出/台账。
`split_io` 返回借用式独立读写，不转移或暴露 endpoint/Connection 所有权；
两方向共享永久失败状态。半帧取消、IO 错误和已观察撤销会关闭整连接，
成功方向不能清除另一个方向的失败。释放 halves 后可由原流完成双向 FIN。
原单流读写沿用同一保护逻辑，不通过取消半帧来轮询另一方向。

紧凑 codec 复用既有 NVRLY002 Data 字节，编码借用 envelope，按完整长度
在分配/复制前检查 16 KiB；解码借用预检后才复制，拒绝 JSON、Delivery、
尾随字节和畸形长度。codec 只处理格式，不证明身份或密文真实；调用者仍须
验证 BoundSecureChannel。新增真实 8 KiB 加密载荷测试确认二进制可承载，
JSON 数字数组形式超过上限；未降低控制消息额度或改变原 relay 格式。

Windows / Rust 1.94，显式本仓 target、单编译作业：网络 lib 含 iroh
**726 passed、11 原有 ignored 未执行**，严格 all-targets/no-deps Clippy
通过。新增 3 项真实 QUIC 双工/半帧取消/门禁测试、5 项 codec 测试；双向
24×16 KiB 帧后可正常收尾。日志 `artifacts/control-duplex-tests.log`、
`artifacts/control-duplex-clippy.log`。未改交易或启动产品节点，不是生产
信令、自动发现、Android 生命周期或合格隐私路径验收。认领释放。
## 2026-10-10 可选可靠 NAT 载体与连接身份绑定

输入 `7ced1724`，认领 `264cc3ca`，仅 network、lockfile 与台账。
`iroh-transport` 默认关闭，固定 iroh 1.3.0（禁用默认 features，选用已有
AWS-LC TLS provider）。不启用 N0 默认发现、IP 发布、端口映射或隐式 DNS；
显式配置最多 4 个 HTTPS/443 relay、DNS 字面地址，并在 bind 前检查本地授权。
载体独立随机密钥，NOVOVM 设备身份签名覆盖实际 TLS exporter、双方端点、
用途/版本和原 E2E 握手；双向加密 Finished 后才提供业务 channel。

每作用域最长 120 秒、帧 16 KiB、流接收窗 64 KiB、发送窗 128 KiB、
一个双向流、8 个远端 NAT 候选。进程最多 4 个 endpoint，包括仍在关闭的 owner。
取消或失败在尚未开始 graceful close 时通过 Drop-abort 取消后台任务；正常
关闭由独立任务持续驱动，超过 5 秒返回清理未完成并保留许可，不能丢弃底层
close future。上游关闭耗时没有公开硬上限；abort 后任务释放需调度推进，
不宣称返回瞬间已无在途网络包。半帧读写/finish 被取消会作废并关闭连接。

Rust 1.94 / Windows，`CARGO_TARGET_DIR=<SUPERVM>/target`、`CARGO_BUILD_JOBS=1`：
`cargo test --locked -p novovm-network --features iroh-transport --lib -- --test-threads=1`
**718 通过、11 原有忽略项未执行**；同 feature 的 all-targets/no-deps 严格 Clippy
通过。新增 6 个载体测试（真实 QUIC/13 KiB 帧/端口释放/半帧取消）与 16 个身份
绑定测试。首轮 peer ID 长度误算已由现有编码器对照修复；失败日志仍保留。
证据为忽略目录 `artifacts/iroh-full-tests.log`、`iroh-clippy.log`。

边界：这是可选传输组件，未接产品 node、生产 UCA 或主应用。端点地址目前由
受信控制层提供，仍需签名发现/续期、Android 生命周期、独立 relay 与隐私路径。
8 次 accept 预算不限制上游 pending Incoming：库默认 65,536 个等待对象及
100 MiB 额外缓存（不含第一包）；正式公网服务仍需独立准入 owner 与更严端点
级限额。单次无效握手有 3 秒预算且计入总次数，不无限等待。默认隐私仍不得
用该直连/单 relay 载体。TLS/QUIC 或独立设备密钥均不等于匿名或抗量子验收。
lockfile 新增依赖并更新 5 个既有包版本（hashbrown、indexmap、webpki-roots、
zeroize、zeroize_derive）；原协议与交易源码未改。认领释放。

## 2026-10-10 NOVOCHAT 所需连接取消与共享期限

以下为上一片；最新可靠载体组件结果见本节前的新增记录。

代码输入 `4fd30a959`，本机认领 `88609f6d`；仅既有
`crates/novovm-network/src/duplex/product_relay_client.rs` 增量。
原 `connect` API 保留，新增 `connect_with_control`；产品可传快速、非阻塞的
授权/取消检查，控制函数保留至连接后的每次底层 IO。DNS、TCP、TLS、WS 和
节点签名握手共用绝对连接期限，不因换地址重置。最多 8 个未完成 OS DNS
工作线程、8 个返回地址；取消后迟结果只丢弃，解析线程不持有 socket 或身份。
TCP 使用非阻塞 Mio 单 socket，受控 IO 最长 50ms 轮询，取消成为终止错误，
不当作空闲重试。部分写进度先交 rustls 记账，随后拒绝新 IO；不重发已写字节。
IPv6 HTTP Host authority 保留方括号。

Windows / Rust 1.94.0，显式 `CARGO_TARGET_DIR=<SUPERVM>/target`、
`CARGO_BUILD_JOBS=1`：`cargo test --locked -p novovm-network --lib -- --test-threads=1`
为 **696 通过、11 原有忽略项未执行**；`cargo clippy --locked -p novovm-network
--all-targets --no-deps -- -D warnings` 及限定 rustfmt/diff 检查通过。
日志位于忽略目录 `artifacts/novochat-network-regression.log`、
`novochat-network-clippy.log`。首轮短写测试错误假设 Windows 必然拆包，改为
测试专用首次真实短写限制，验证对端全部字节及 EOF 后无尾随重复；没有更改
生产写语义。新增测试还覆盖 TLS 停滞取消、DNS 迟结果、解析上限、DNS 前拒绝、
socket 等待中取消及取消后禁止继续 IO。

边界：OS DNS 本身不能强杀，其许可直到解析真正结束才释放；根证书及 CA
文件读取只能前后检查。多个解析地址仍顺序尝试，首地址可能耗尽预算，未实现
Happy Eyeballs。已发出的字节不可撤回。这些是通用传输组件验证，不是跨 NAT、
手机通信、UCA 授权、匿名路由或 NOV 交易验收；未改产品入口及交易/共识路径。

## 2026-10-10 原生 UCA 账户记录与候选状态恢复

输入 `f731060e0146b51914565fce9baa75baf7df5c02`。native adapter 新增
`uca_state`，复用既有 `UcaAccount`；版本 1 记录包含根绑定、状态、代次、
独立 V4 行政 nonce 及最多 64 条授权承诺（含未过期撤销墓碑）。单账户
记录最多 32 KiB，单次授权最长 65,536 个块；这是候选格式上限，不是
生产租约/收费政策。当前仅 Ed25519，其他算法及强制私有/PQ/ZK 模式
拒绝，不降级。NUCA/1 命令为固定 70/78 字节，方法、应用域、期限和
承诺都在原完整 V4 双签内。承诺不含聊天公钥、联系人或原始网络证书；
它本身不能证明匿名性，公开账户/用途/时间仍会关联。

纯 reducer 校验当前账户及完整双签后，在副本上同时产生授权变更与下个
nonce，保留完整父记录比较，任何失败不修改输入。撤销墓碑保留至原期限，
禁止租期内用同一承诺重新授权而复活旧凭证；过期项只在成功候选中回收。
没有新建数据库、全局身份缓存、真实账户或余额；也未替代交易失败时应由
既有执行器完成的费用/nonce 规则。

节点在原 `module_state` 增加可选 `native_uca_accounts_v1`，逐账户映射
到现有物理/共识记录。未启用时不序列化空字段，旧状态图像保持；显式
登记版本字段，拒绝未知字段、错误所有者、超大/错误版本记录。旧分片
写法遇到新账户状态直接失败，避免账户状态从持久化中消失。尚无生产
注册/导入入口，旧 `unified_account_surface` 数据库不成为原生可信来源。

Windows / Rust 1.94.0：adapter `uca_` **13 项**；node release 新记录
**2 项**；真实 AOEM **1 项跨进程恢复**（另启动专用 worker）通过。真实
库为已固定的 core `01779fe4...056674`，测试库在忽略目录
`artifacts/uca-state-tests/`。父进程写隔离夹具，子进程重开、验证双签撤销
并写候选，父进程再次重开检查 nonce/撤销同时保留、旧交易拒绝、旧根
不变，以及增量共识根与完整状态重算一致。没有发布权威链头，未测真实
断电、跨节点共识或原子费用提交；不得把单账户叶持久化称作完整授权最终性。

既有记录 **5 项**、共识投影 **8 项**、远端入口 **5 项**、创世 **10 项**
通过。adapter all-targets 与 node release lib 严格 Clippy、限定 rustfmt、
diff 检查通过。复现时显式设置 `CARGO_TARGET_DIR=<SUPERVM>/target`：

```powershell
cargo test --locked -p novovm-adapter-novovm --lib uca_
cargo test --locked --release -p novovm-node --lib native_uca_
cargo test --locked --release -p novovm-node --lib real_aoem_native_uca_records_survive_process_restart -- --ignored --nocapture --test-threads=1
```

日志：`artifacts/uca-state-node-tests.log`、`uca-state-aoem-restart.log`、
`uca-regression-*.log`、`uca-state-*-clippy.log`。首轮测试编译发现测试辅助
名称遮蔽及误用不存在的 overlay 构造器，已修正后重编译通过。

**仍未接通：** 注册/根轮换的版本化业务、生产已验证父状态提供方、
V4 候选认证/执行/费用/失败消费/回执/共识发布的一体化接线及移动接入。
下一步须将账户效果与既有费用和规范回执置于同一候选，重新验证精确父
再发布；不能单独把 reducer 结果作为已授权。V3 继续拒绝 V4，历史 Host
执行开关未启用。授权承诺不是匿名凭证，聊天不因此增加逐消息链 RPC。

## 2026-10-10 版本化 UCA 授权载体与完整封包预检

输入 `a66f0501ac681be60231a314de9ac572feae7d61`。本片在已有 protocol
增加 `uca_transaction`，明确为 `NNX1/4`：5 字节头、u32 大端内层长度、
完整规范 `NNX1/3` 交易、u16 大端委托长度、完整 `NUCD/1` 证明。内层
最多 70 KiB、委托最多 359 字节、总封包最多 72,050 字节；先检查边界，
再分配/解码，拒绝嵌套 V4、旧 V1、截断、尾随及非规范内层编码。
本地编码同样限制 args 为 64 KiB。无隐式版本探测或剥掉委托的降级路径。

结构校验要求用途、目标模块、行政设备签者、账户/费用/nonce 所有者及
完整意图与委托一致。V4 ID 是有独立域和长度绑定的全封包 SHA-256，包含
根/设备签名字节；它与内层 V3 intent 不同，后续准入、重放和回执必须
明确使用对应标识，不能把两者混用。未改变已有 V3 编码器及签名字节。

native adapter 的 `verify_uca_transaction_v4` 直接复用已有真实双签和
上下文检查，返回完整交易及 V4 ID，不返回可当作共识授权的伪回执。
可信父状态依旧必须由调用方独立认证；预检不消费 nonce、不写账户或费用。
protocol 解码成功也不证明签名正确。封包包含 UCA 关联，不能发给聊天
联系人/普通中继。NOVOCHAT 在隔离互操作工程准备和校验完整载体。

Windows / Rust 1.94.0：protocol 全部 **39 项**、adapter 委托 **8 项**
通过；两 crate all-targets 严格 Clippy 通过。新增用例覆盖完整封包变更、
真实根/设备签名篡改、撤销用途重新双签、父状态代次/nonce/创世不匹配，
以及边界最大参数。显式设置 `CARGO_TARGET_DIR=<SUPERVM>/target` 后复现：

```powershell
cargo test --locked -p novovm-protocol --lib
cargo test --locked -p novovm-adapter-novovm --lib uca_delegation
cargo clippy --locked -p novovm-protocol -p novovm-adapter-novovm --all-targets --no-deps -- -D warnings
```

节点 release `remote_native_ingress_` **5 项**通过，其中新增真实签名 V4
在本地、远端、V3 摘要入口均被拒绝的回归，且无 pending payload 或 nonce
预留。node release lib 严格 Clippy、限定 rustfmt 与 diff 检查通过。

此前 AOEM 环境缺口本轮已解决：通过现有 SSH 身份取得 LFS batch 下载
授权（明确 `Accept: application/vnd.git-lfs+json`），下载 44,048,384 字节，
SHA-256 为 `01779fe4fc77535749d265f9ffc233d9c1486e526d9f5d0dc34fc6d49a056674`。
校验后回填本仓 LFS 对象缓存并执行
`git lfs checkout aoem/windows/core/bin/aoem_ffi.dll`；checkout 哈希再次相符，
跟踪指针没有变更。没有替换成旧库，未保存/输出临时授权头或签名下载 URL。

恢复精确 DLL 后，历史 `unified_account_surface_cut_a_` **4 项全通过**，
包括此前受阻的 ML-DSA-87 密钥生成、签名、轮换及绑定持久化。该组仍是
历史 Host 比较测试：仅在测试进程设置 `NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1`，
在独立临时库执行后恢复原环境，不在生产启动器放开。命令为
`cargo test --locked --release -p novovm-node --lib unified_account_surface_cut_a_ -- --test-threads=1`。
这只解决本机该项密码/历史接口验证，V4 当前仍只支持 Ed25519 预检，
不因此声称已交付 PQ 委托、匿名证明或真实手机授权。

**仍未接生产执行。** 当前节点 V3 decoder 明确拒绝 V4；本片只增加节点
拒绝回归，没有启用新的 RPC/候选执行分支。真正的 UCA 生命周期、父状态
来源、原子 nonce/费用/授权提交和最终回执仍须在原候选执行链成套接入。
不能因为“原生交易已携带证明”就解除 `ua_*` 写门禁；不能从旧独立 router
快照制造可信上下文。本片未签发手机权限、部署节点或执行真实资产操作。

## 2026-10-09 授权接入前的原生交易编码修复

输入基线 `a34ca4a96f3928d97a049643d20c865f0d0585b1`。继续追踪 UCA
可信状态时发现：`postcard::from_bytes` 不检查未消费输入。两项新增回归
在修改前真实失败，分别复现合法交易附加 1 字节仍被解码、链 ID 使用
非最短整数编码仍被解码。尾随授权封包被忽略不等于伪造签名成功，但会使
不同原始输入被视作同一交易，不能拿这种隐式扩展承载 UCA 权限。

`novovm-protocol::tx_wire` 现要求消费完整载荷，并用序列化输出逐段比对
原始字节，拒绝非规范编码；不分配第二个完整交易缓冲。V3 和只用于明确
拒绝旧认证的 V1 都执行此规则，V2/未知版本继续拒绝。正常编码器、签名
消息和意图承诺不变；准入范围收紧，未来节点发布需保持全网一致，不能
把这次修复作为异构版本热更新或历史非规范交易迁移已经验证的证据。

Windows / Rust 1.94.0：协议全部 **35 项**测试、protocol all-targets
严格 Clippy 通过。三项新增协议测试覆盖尾随/拼接、非最短整数、V1/V3
截断及版本边界；既有 Transfer/Execute/Governance 正常编码回归保留。
复现：设置 `CARGO_TARGET_DIR=<SUPERVM>/target` 后运行
`cargo test --locked -p novovm-protocol --lib` 和
`cargo clippy --locked -p novovm-protocol --all-targets --no-deps -- -D warnings`。

节点 release **10 项**限定回归通过（`remote_native_ingress_` 4 项、
`nov_native_ingress_rejects_` 5 项、
`signed_native_wire_maps_to_adapter_verifiable_canonical_ir` 1 项）。新增节点
测试使用真实签名，确认 4 种畸形输入在本地/远端入口和规范摘要入口均拒绝，
没有 pending payload 或 nonce 预留，随后同 nonce 的规范交易仍正常入队。
以上三个过滤器分别用于 `cargo test --locked --release -p novovm-node --lib
<filter> -- --test-threads=1`，不启用旧 Host 业务执行。此为本机入口测试，
未发送真实链交易或声称原子账户提交已经接通。

`cargo clippy --locked --release -p novovm-node --lib --no-deps -- -D warnings`、
限定源码 rustfmt 与 diff 检查通过；构建过程原有多 bin 共用文件的 Cargo
manifest 提示保留，不将其记录成新增源码警告或本轮修复范围。

真实状态核对：`unified_account_surface.rs` 的旧 router 快照是独立文件/
RocksDB，当前 `NovNativeExecutionModuleStateV1` 没有其账户根与设备撤销
状态；候选父状态验证位于 `native_candidate_live_parent.rs`、
`native_candidate_auth.rs`，耐久 nonce 属于同一原生候选/发布链。不能
把旧 UCA 快照塞进委托 context 就认定它经过共识认证。本片先收口实际
输入缺陷，尚未交付状态接入。剩余实施须成套绑定版本化原生授权载体、
同一父状态内的 UCA 生命周期、原子 nonce/费用/状态提交和最终回执；
不另建旁路授权库，也不解除生产写门禁。

本轮再次定向补取锁定 AOEM 主 DLL；下载未完成，停止本次卡住的 fetch，
checkout 仍为 133 字节 LFS 指针，没有更换版本。未部署节点、签发手机
权限或执行资产操作；ML-DSA 和真实双机通信仍未验收。

## 2026-10-09 单交易 UCA 委托证明（未接生产准入）

输入基线 `fbcb16a7416e3fa15f01c1c6b137d4b40d9da74d`。protocol 新增
`NUCD\x01` 有界封包：最多 359 字节，账户标识最多 128 字节，高度窗口
最多 256 个块；绑定链 ID、创世摘要、账户、撤销代次、应用范围、行政设备
公钥、现有 native V3 完整意图承诺及用途。仅预留设备授权/撤销两种用途，
不是新 native wire 版本，也不允许据此转账。未知版本、截断和尾随字节拒绝。

native adapter 负责真实 Ed25519 根签名和设备交易签名的严格验证；本地准备
入口在调用根签名器前检查设备签名和所有约束。可信父状态由调用方提供，
不能从远端封包自行声明。根算法不是 Ed25519、要求其他执行安全策略、
缺失根绑定或状态不匹配时拒绝，不自动降级。该片不声称实现抗量子或匿名性。

原 node 的 native wire -> TxIR 转换及两个辅助函数原样移到既有 native
adapter，node 重新导出原函数；三段函数体与输入基线逐字比较一致，继续
使用原签名字节及意图承诺。没有重写节点、执行所有者或账户状态机。

当前规范执行链尚无 UCA 委托父状态提供方、原子 nonce 消费及状态回执。
`VerifiedUcaDelegationV1` 仅表示在传入上下文下验签通过；对同一上下文重复
验证仍允许，不能当持久去重、accepted/finalized 或设备权限。生产 `ua_*`
写门禁和直接签名者准入保持。账户根关联封包不得发送给聊天联系人/中继。
NOVOCHAT 在隔离互操作工程直接复用；没有发行手机授权、部署节点或资产操作。

Windows / Rust 1.94.0：protocol 全部 32 项、native adapter 新增委托 6 项
通过；adapter all-targets 严格 Clippy 通过。节点 release 五项既有拒绝回归
通过，覆盖链域、旧签名、完整 signed wire 篡改及未经授权的主体/费用/nonce
所有者。另有 remote-native-ingress 三项回归通过（入队前验签、外层摘要/
签名不匹配及流内冲突拒绝），一项 signed-wire 到可验签 TxIR 的正向回归
通过，共 9 项节点限定回归；node release lib 严格 Clippy、限定源码
rustfmt 和 diff 检查通过。复现时显式设置 `CARGO_TARGET_DIR=<SUPERVM>/target`：

```powershell
cargo test --locked -p novovm-protocol --lib
cargo test --locked -p novovm-adapter-novovm --lib uca_delegation
cargo clippy --locked -p novovm-adapter-novovm --all-targets --no-deps -- -D warnings
cargo test --locked --release -p novovm-node --lib nov_native_ingress_rejects_ -- --test-threads=1
cargo test --locked --release -p novovm-node --lib remote_native_ingress_ -- --test-threads=1
cargo test --locked --release -p novovm-node --lib signed_native_wire_maps_to_adapter_verifiable_canonical_ir -- --test-threads=1
cargo clippy --locked --release -p novovm-node --lib --no-deps -- -D warnings
```

AOEM 主 DLL 仍缺锁定 LFS 二进制，前片的 ML-DSA 环境失败未解决；本片
Ed25519 验证不调用该 DLL，不能用它替代 ML-DSA 验收。下一片接既有
node -> exec -> AOEM 的规范身份状态所有者、原子 nonce 消费和可信回执，
然后移动身份安全域才能消费真实授权。本片验证不等于完整项目安全审计。

## 2026-10-09 NOVOCHAT 直接复用 UCA 编码（非移动授权交付）

输入基线 `5dc902e3d6e26f350c7a5b1da363835d6a9e6ef4`。用户决定由原厂
NOVOCHAT 直接适配 SUPERVM，旧钱包和第三方 SDK 不作为前置。本片只把
现有主密钥引用派生及 `signature_v1` 所有权签名字节编码提取到已有
`novovm-adapter-api::uca_key_binding`，原节点改为引用同一实现。未改密码
算法、授权政策、交易/共识或奖励规则，未另建身份状态机。

Windows / Rust 1.94.0：共享编码 2 项测试通过（固定字节及域/主体/操作
绑定）；原节点 release `unified_account_surface_cut_a_` 的 Ed25519、
secp256k1、错误签名不污染状态 3 项通过，ML-DSA-87 项未通过。原因是
随仓 `aoem/windows/core/bin/aoem_ffi.dll` 仍为 133 字节 LFS 指针，实际
44,048,384 字节二进制未补取完成，故不能认定本机 ML-DSA 验证通过。
所需 SHA-256 为 `01779fe4fc77535749d265f9ffc233d9c1486e526d9f5d0dc34fc6d49a056674`。
限定源码 rustfmt、diff 检查及 node lib 严格 Clippy 通过。

复现：显式 `CARGO_TARGET_DIR=<SUPERVM>/target`，运行
`cargo test --locked -p novovm-adapter-api --lib` 和
`cargo test --locked --release -p novovm-node --lib unified_account_surface_cut_a_ -- --test-threads=1`。
后者是历史 Host 比较测试，须仅在测试进程设置
`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1`；测试使用独立临时数据库，结束恢复
环境。默认门禁下初跑为 1 通过/3 拒绝；显式测试 opt-in 后为 3 通过/1
环境失败。不修改默认门禁、不在生产启动器设置该变量，不把这些历史
测试视为原生交易已接通。首次使用 `primary_key` 过滤器选中 0 项，未计入
证据。`cargo clippy --locked --release -p novovm-node --lib --no-deps -- -D warnings` 通过。

生产 `ua_*` 写操作仍拒绝未接通的规范原生交易委托；原所有权签名不是
应用/设备/期限/撤销授权。本片范围收口，ML-DSA 环境缺口保留。下一片须
接规范委托和可信状态回执，再接移动密钥所有者；未发行真实手机授权、
未部署节点、未执行资产操作或 NOV 结算，不宣称双机或匿名通信通过。

## 2026-10-04 证明关系复核与原节点能力误报纠正（非新证明交付）

基线SUPERVM `1904335456`、AOEM `2331fb3c47`。本次只读核对相关密码关系与
统一入口，修改原节点的能力查询，不修改AOEM、交易计算、共识或最终性。

- `nov_getPrivacyCapabilityStatus` 原先只凭请求/环境中的 `prove` 或 `verify`
  声明，就报告 `privacy_proof_capable/available_by_capability`。本方法没有
  实际生成/验证证明，也没有已接通的证明查询服务；该输出不能作为可用性证据。
  修正后保留原RingCT/ZK/ML-DSA声明与来源，将其明确列为未验证声明；实际
  查询仍是 `read_gate_only`，证明查询不可用，实际可用证明系统列表为空。
  这不是关闭一个已可用的证明服务，不改变读取鉴权或M2信息遮蔽。
- AOEM统一op96–99及 `GraphKind::Zk` 的NTT/MSM/Hash能力保留。限定源码核对
  显示：FFI默认Groth16电路是 `a*b=c`；Halo2当前电路是累加关系，未在电路中
  验证输入证明；真实Bulletproof范围证明和经典CLSAG/BP+验真也不能顶替隐藏
  Merkle路径或完整NOV业务关系。这不否定这些原有能力，不恢复RISC0产品旁路。
- 已撤回profile3的 `parse_zk_merkle_membership_v1` 是普通见证检查，没有查到
  可直接复用的对应私有电路；其叶、nullifier与树节点调用自定义
  `proof_contract_digest32`（AOEM `crates/ffi/aoem-ffi/src/lib.rs`），未找到该摘要
  具备密码学安全性的依据。不能把旧摘要直接写入真实电路就宣称修复。
  Groth16本地临时setup也不等于已固定的生产验证参数。

下一密码修复需要明确新的关系/哈希/证明版本及参数信任契约，保持旧不可靠
profile拒绝；先决定要证明的业务关系，再复用AOEM后端，不能拿另一固定关系
换标签交付。NOV规则留在SUPERVM，AOEM只承载通用证明能力。本次未选择新密码
方案或修改链协议，未新增ZK、GPU业务或TPS成绩。

远端状态补正：上一提交 `1904335456` 的CI `37179579101` 已完成，活动Rust、
安全、保留组件和LAN检查通过；Node项目检查仍跳过。这个结果不等于本次修改
的远端CI，也不等于主网或完整ZK验收。

本机Windows/MSVC release：`cargo test --release --locked --target-dir target
-p novovm-node --lib privacy_capability_status -- --test-threads=1 --nocapture`
4/4通过，覆盖两种参数别名、单prove/verify、伪造成功字段、隔离环境声明及
未配置状态；同harness的既有 `privacy_audit_policy_locks_m2_query_surface`
1/1通过。`cargo clippy --release --locked --target-dir target -p novovm-node
--lib -- -D warnings`、workspace fmt与限定diff检查通过。Cargo既有重复binary
目标警告仍保留。本轮未重跑四节点、Linux或吞吐，不把查询单测当作产品ZK
验证；49项原草稿哈希不变。认领收口，无新增证明协议或密码后端选择。

## 2026-10-04 已授权AOEM私有profile安全封堵与原节点回归（非ZK恢复）

SUPERVM基线 `da95316c` 加本节提交；用户明确授权修改AOEM通用密码/语义契约。
AOEM先保留远端新增的AI等成果，基于 `2c154bc9` 修正；代码提交
`1377cd1e7c1d43a4bf81cb16a169319772cf3e84` 已推送，后续仅补正SDK文档声明。
**本次撤回不可靠能力，不将封堵冒充已经补齐真实私有ZK。**

### 实际改变

- AOEM op98私有成员profile3的wire v1–v4在GPU/资产使用和写输出之前拒绝；
  op99也不允许注册该profile。外部验真拒绝AORF v1–v3的私有封装，有无见证
  均拒绝；不能退成公开见证披露来假装保留隐私。
- 公开profile1/2保留GPU公开管线/路径诊断，严格返回
  `envelope_integrity_only_not_zk`，`proof_verified/accepted/cryptographic_proof_verified=false`。
  校验公开Merkle路径有用，但不能授权私有资产、业务结果或最终性。
- SUPERVM同步FULLMAX和严格消费者，4个重复C入口改为包含同一已修正源；
  private成功样例、清单能力和验收说明改为撤回/反例，历史记录不删除。
  CI加入无需GPU的公开伪造/改标签反例及真实SO精确拒绝检查，不移除原产品检查。
- 原 `novovm-node → novovm-exec → bindings → AOEM`、常驻CPU快路径、
  ZK专用类型/Vulkan图及其它密码实现保留；未恢复RISC0产品服务，没有新的
  节点、账本、共识或NOV专属内核业务。主链最终性含义不变。

### 已执行范围

| 本机检查 | 实际结果与边界 |
| --- | --- |
| AOEM Rust安全反例 | 5通过；wire版本、资产注册/输出、重算摘要、改标签及公开对照 |
| 外部C反例 | Windows/WSL各12个私有变体全拒绝；无需可信生成器或GPU |
| Windows FULLMAX及公开GPU诊断 | 核心和全部sidecar构建；实际RTX5090 Laptop/Vulkan smoke、verify、重复service和asset生命周期通过，不是业务ZK |
| Windows worker私有请求 | 5/5 unsupported、无proof写出；预期进程退出1，不当成成功证明 |
| SUPERVM严格SDK消费 | Rust与worker smoke通过；bindings resident 32/32、exec resident 3/3；后者为门面回归，不单独算实库计算 |
| 原节点真实RPC与恢复 | release构建；同机四原进程混合与单入口各通过，均7成功+1业务失败；2/4不能发布，四库经济/nonce/回执/冷恢复通过 |
| ML-DSA真库 | Windows 3/3，44/65/87正反例；旧90向量/9互操作记录未重新签收 |
| Linux/WSL FULLMAX | 核心、14个sidecar文件与6个KMS/HSM别名构建并同步，逐文件SHA256相同；未跳过feature，构建失败/重试原始日志保留 |
| Linux新SO安全拒绝 | op98 wire-v4及op99 profile3注册返回精确退役错误、零处理/成功/写入，19个输出键调用前后均不存在；worker 5/5拒绝 |
| Linux新core宿主回归 | bindings32、exec门面3、额外实库计算2、PQ3全通过；合计40中13真库、27纯/mock，不宣称全sidecar功能或Linux原节点已验收 |
| 检查与保护 | fmt、修改示例严格Clippy通过；49项原草稿哈希不变，不混入提交 |

Linux限定新core回归通过；本轮未新建Linux node/重跑其RPC或实体安装。
WSL仅暴露llvmpipe，目标NVIDIA GPU验证NOT EXECUTED；macOS未打包。
本轮GitHub CI尚未签收，不把本机结果等同于远端全绿。
程序/库哈希、实际原始报告、scope与重试记录见
[本轮证据](../artifacts/audit/aoem-private-proof-containment-20261004/verification.json)，
随包来源与兼容范围见[SDK基线](../aoem/RUNTIME-BASELINE.md)。Linux规范打包
使用原FULLMAX脚本，终端诊断器失败后采用正常尺寸PTY、以Clang保留release
优化完成构建；不是改源码/降配置绕过。AOEM源码与已推送`1377cd1e`相同，
当前`2331fb3c`只补正SDK文档/schema能力声明。
旧统一package gate对ABI/canon必须为false的历史条件仍与V3基线冲突，未报全门通过。

没有重测65,536笔，不报新TPS或“性能已恢复”；没有新私有收付、完整NOV业务
证明或GPU业务签收。下一步仍须在原统一语义契约中接真正独立可验的密码关系，
保留候选绑定与有界生命周期；须明确证明版本/关系/可信参数，不能将公开摘要
修补或换标签作为修复，也不能用旧后端直连服务替代。现场数据与49草稿保留。

## 2026-10-04 ZK统一语义接线前复现独立验真缺口（接线未签收）

基线 `main/origin/main@cb4d3c38`，本机 Windows/MSVC。用户要求继续将已有 ZK
语义接回原节点；本轮实际核查后发现不能仅补调用即签收。没有改交易、共识、
GPU实现或SDK，没有另建证明器。AOEM兄弟仓只读；通用内核修复另待用户授权。

### 已实测的反例，而非根据接口名称推断

最小C用例直接包含**未修改**的随包 `hosted_resident_proof_verify.c`，使用其
已有 `NO_MAIN` 条件；只使用虚构公开数据，不加载AOEM、不调用GPU、不访问链。
私有成员profile3取深度0、`root != leaf_commitment`：按该实现的关系定义，
零深度根必须等于叶承诺，因此不存在满足该公开语句的成员见证。

| 原外部验证器的输入 | 实际返回值 |
| --- | --- |
| 无私有见证，公开构造封装/摘要的错误关系 | 0，接受 |
| 直接篡改上述封装 | -1，拒绝 |
| 篡改后仅重算checksum | -1，拒绝 |
| 篡改后重算全部公开摘要和checksum | 0，接受 |
| 更改预期public input、不重新绑定 | -1，拒绝 |

主任务独立重编复跑同样结果。**反例程序exit=0表示成功复现缺陷，密码安全验收
为FAIL。** 同版本原Rust proof smoke和原C完整verifier smoke仍各exit=0；这说明
既有正常/简单篡改用例不足以证明对不可信证明者的独立有效性，不是缺库导致失败。
源码、命令、输出、SDK和源文件SHA见
[复现记录](../artifacts/audit/zk-semantic-boundary-20261004/verification.json)与
[最小反例](../artifacts/audit/zk-semantic-boundary-20261004/verify_without_witness.c)。
首次主任务C编译漏include目录而失败，补原头文件目录后通过，记录未抹掉。

### 精确影响范围与保留资产

- 当前op98的profile1/2/3不能表达完整NOV交易关系；私有profile的外部验证
  在没有见证时只核公开封装/摘要一致性。AOEM源`ffi/aoem-ffi/src/lib.rs`
  的`verify_fixed_profile_resident_proof_v1`在AORFv3分支也只查结构和checksum。
- **生成端有真实成员关系检查；本反例没有证明AOEM生成器会接受错误见证。**
  问题是恶意来源可绕过生成器，而接收方当前外部验证不能识别伪造关系。
  不能仅补深度0比较就宣布一般私有成员证明问题已修复。
- 原`GraphKind::Zk`、`WorkloadHint::Zk`、NTT→MSM→Hash图及常驻Vulkan后端
  真实存在，予以保留。其图/队列结果不自动成为完整NOV密码证明；本结论不
  否定AOEM其它Groth16/Halo2/Bulletproof等真实密码实现，也未审计其全部安全性。
- 随包DLL哈希匹配manifest的`56e9da15`来源。只读比较到AOEM本地`34d66a51`
  后续ffi差异为隐私接口增补，相关ZK实现未变；不把未重建的兄弟仓当运行库。
- 当前resident `ComputeOwner`的FIFO同时承担鉴权和交易计算，不能将长证明
  同步塞进去。wire输出读回是进程级surface、不是会话隔离数据库，生命周期
  接入还须复用现有有界预算与唯一I/O owner；本轮没有为此新建服务。

本轮仅提交反例及交接事实，不改生产路径，不声称新增业务ZK/GPU或TPS。
现有CPU快路径保留，49项原草稿哈希不变。待明确授权后，先在AOEM的通用语义
与独立验真契约内修复，再回原node接线；不恢复RISC0旁路，不在AOEM加入NOV
账户/费用/共识业务，不以再造固定profile演示替代真实关系接入。

## 2026-10-04 撤除后端特定证明旁路，保留原节点与统一 ZK 入口

代码基线 `387bf0b7a11cd0421322e283e51c5364ac530b5a` 加本节所在纠错提交。
用户明确要求及时删除最近错误接线，避免后续继续按独立 RISC0 产品路线开发。
`crates/novovm-node` 一直是产品总控；Rust `main()` 仅是可执行文件的启动函数，
不是需要另建或恢复的产品架构。本轮不改原 node bootstrap 或既有 ops-wire 入口。

### 实际撤除与保留

- 删除 `native_pipeline/proof/service.rs`，以及仅验证该服务的
  `consensus/resident_rpc_process/proof.rs`。撤下 `nov_proveBlock`、
  `nov_getBlockProof`、`nov_chainStatus.business_proof` 和相关启动/轮询/关闭。
- 删除 resident 的后端特定 `proof` 配置。旧字段包括 `null` 也明确报
  `unknown field proof`，不能静默忽略、自动选另一个后端或假装已迁成统一证明。
- 撤下仅供该服务使用的 `IoProofClient` / ProofRead/Write / pipeline claim；
  原候选、共识、查询的 I/O 行为不变。没有删除任何运行数据或已保存附件。
- 保留原 AOEM 专用 ZK/GPU、统一 wire/exec/bindings、APFL/常驻快路径、完整
  关系/guest/反例。node 的直接 receipt 适配和不透明附件 codec 仅在
  `cfg(test)` 编译，不再导出为活动产品证明入口；附件实库回归仍由 CI 执行。
- 原始证明和性能报告全部保留；被删除源码可从 `387bf0b7` 恢复。其历史证明
  时间不代表 AOEM 统一 ZK/GPU 性能，也不能作为重新启用后端直连服务的依据。

后续仍须沿既有 node 的专用语义任务、资源生命周期、输出和业务关系核对接入；
不以此清理宣布新 GPU/证明交付，不将固定 profile 重标成另一种业务证明。

### 本轮验证

本机 Windows、原节点 release，本节源码在同一机器验证：

- `cargo check -p novovm-node --all-targets --locked`、对应严格 Clippy
  `-D warnings`、release node/测试构建、根工作区 fmt 和 diff 检查通过。
  Cargo 仍提示原有多个 binary 共用同一源文件，不把它说成零告警构建。
- `native_pipeline::`：376通过、0失败、57忽略；新增旧配置三种形态拒绝通过。
  保留的不透明附件实库测试1项通过，覆盖多片、重开、不可覆盖与损坏拒绝。
- 四个真实原节点混合RPC回归1.83秒通过：8笔最终回执（7成功、1业务失败），
  两票不能出头、经济/nonce/回执和冷恢复/后继通过；四节点共8次退役接口
  调用全部拒绝，调用前后链头/pending/已有回执不变。
- 同版单入口传播/失败/重启1.32秒通过；A/A/B恢复11.11秒通过，后者仍是
  三个真实原节点加一个受控故障peer、三诚实签者QC，不重置签者、不外造QC。
- 装配边界检查通过；49项已有草稿逐文件SHA256与开工前一致。原node bootstrap、
  exec/bindings/AOEM SDK均无改动。删除源码可由Git恢复，真实数据未触碰。

程序/库哈希、8次拒绝结果及本地原始报告位置/哈希见
[清理验证摘要](../artifacts/audit/direct-proof-retirement-20261004/verification.json)。
原始报告留在摘要列出的 `target/` 路径；提交的摘要不是完整原始报告。
关闭测试进程时的OS 10054/TLS close_notify日志保留，不称零错误日志。
本轮未重跑65,536笔性能，不产生新的 TPS/P95/P99、GPU/ZK、Linux或公网验收声明。
推送后的GitHub CI另行核对，不用本机结果代替远端结果。

## 2026-10-04 历史：原RPC候选直接后端证明服务（已撤除）

**本节记录 `387bf0b7` 的历史实现和实测。上节已撤除其产品入口；下列 RPC、
配置示例与当时的下一步不是当前部署或开发指令。原始证据保留，不改写测试事实。**

基线 `main@d44779b237a0de766eee48e20f24fe7da468aa81` 加本节所在提交。
原 `novovm-node/native-resident-v1` 新增显式、可选的同候选证明服务；不是
另建节点、执行器、链头或签者。**用户现在可以为原RPC已确认块请求完整NOV
业务关系证明、查询其验真/保存状态，重启后重新读取并验真同一份证明。**
当前BFT最终性没有改变，不能把可选证明成功扩展成全链已由证明最终确认。

### 实际路径与复用

`nov_sendRawTransaction` 仍进入原APFL传播/常驻CPU执行/耐久round-bft核心。
`nov_proveBlock [height]` 从同节点的 `ArchiveRead` 取得已发布、QC/父链/正文
绑定已验证的候选；后台单一证明owner复用原认证、`NovTransferPlan`、父状态
批读取与 `execution_proof_input()`，生成已有完整 `NVEXIN01` 关系输入。
guest重新检查签名、nonce、完整业务/费用关系，不信任Host提供的最终写集。
预期 `NVEXEC01` journal 独立取自已发布候选的计划/状态/回执/statement承诺。
经 `novovm-exec::resident::ReceiptSession → aoem-bindings → AOEM` 生成
`AORCP002` 后，再按操作员固定image和预期journal验真；不增加另一套prover。

只在验真成功后，经**原唯一AOEM I/O owner、原CandidateStore数据库**写入
独立候选附件命名空间：64KiB分片及完整标记在同一次同步WAL原子批中写入，
随后完整读回核对。附件绑定candidate/document/image/journal，不能更新链头
或授权签票；同值可重读，不同值不可覆盖。未知原生写结果冻结原存储会话，
RPC以 `persisted=null/outcome=unknown_or_unverified` 报告，不谎称未落盘。
冷启动后的查询重新读取归档和附件并调用密码验证，不仅检查哈希或保存标志。

### 本机真实结果（同机WSL Ubuntu 24.04，release）

四个真实原节点、一个钱包入口，node0显式配置证明；两个连续高度各一笔真实
签名NOV交易、四库费用/nonce/余额及回执一致。仅第一块生成完整证明。
未启用 `NOVOVM_ALLOW_LEGACY_HOST_EXECUTION`，`RISC0_PROVER=local`，
清除 `RISC0_DEV_MODE`；没有修改/构建兄弟AOEM仓库。该轮证明为CPU后端。

| 实际观察 | 结果 |
| --- | ---: |
| 生成＋独立固定承诺验证 | 281,958 ms |
| 请求至已验真耐久附件的HTTP观察墙时 | 281,990 ms |
| AORCP002证明大小（不含256B候选绑定头） | 1,687,932 B |
| 证明仍pending时，第二笔至四节点最终回执 | 58 ms |
| 四库冷重启后，对旧证明重新密码验证 | 85 ms |
| 冷恢复服务总耗时 / 重新生成耗时 | 96 ms / 0 ms |
| 独立测试进程另开AOEM会话验证正例 | 78 ms |

独立测试进程从原node0数据库读回证明，以可信image和归档承诺验证；错image、
**真实第二块journal**、篡改证明、截断证明四项均被真实后端拒绝（status=-4），
不是因缺少库而拒绝；随后原正例再验通过。非法高度、未配置后端、忙时另一
高度请求拒绝，重复同高度不会启动第二份证明。完整测试284.43秒通过。
原始结果、程序/库/guest哈希和公开节点日志见
[同候选证明证据](../artifacts/audit/resident-candidate-proof-20261004-v1/linux/result.json)。
日志中的迟到旧高度消息诊断、关闭时TLS close_notify提示保留，不称零错误日志。

真实后端SHA256为 `cfdd4469ec588e5e274d647fad143fa8de129077d125805289e6ec8b2b5201ee`，
既有完整NOV guest SHA256为 `316be9300d357cfad9372a13118f048e0830c1b67b7cc72fb36baf653a9d1515`。
未从本地未审查runtime草稿重建guest，也未把普通SDK/AORCP001改标签冒充此后端。

### 使用边界及尚未接通项

在原resident配置增加可选字段，路径相对**配置文件目录**解析，未配置则关闭：

```json
"proof": {
  "library": "reviewed-proof/libaoem_ffi.so",
  "guest": "reviewed-proof/novovm-transfer-guest.bin",
  "image_id": [1144495143, 3612026493, 4005071765, 1634377336, 1879036010, 4171784104, 1710951644, 1407711336]
}
```

这里只演示已验证guest的pins，须部署对应真实资产并核对哈希；配置不下载资产，
普通随仓SDK不自动变成证明SDK。`nov_proveBlock [1]` 异步申请，
`nov_getBlockProof [1]` 只读已保存证明或查询任务，当前返回摘要/状态，**不是
证明字节下载API**。`nov_chainStatus.business_proof`显示本机owner状态；交易
回执仍 `finality_kind=BFT_durable/proof_verified=false`，不自动改变原语义。

- 单owner最多一个任务，准入/输入/附件有界，没有自动跟随每块生成或隐藏队列。
  未完成任务非耐久，崩溃后可重投；完成附件耐久。原生证明不可取消，正常退出
  等待其完成；并发上限不等于CPU时间或native工作内存硬上限。应限制此显式
  实验RPC的访问，尚不是可向不可信公网开放的付费证明服务。
- 批捕获及密码计算在证明owner；共享I/O上的有界完整附件读写不可抢占，仍可能
  与签票ACK争用。58ms后继样本只证明没有同步把长证明塞入控制循环，**不证明
  满载无争用、批量证明容量或证明加速**。不复制数据库，不另立存储owner。
- 本轮完整关系是已复用的direct-NOV V3关系，不是Execute/所有资产，也不是隐匿
  资产或抗量子证明签收。证明仅保存在配置节点，自动跨节点传播/下载尚未接入。
- **APFL到业务GPU尚未完成**：产品计算仍明确报告AOEM语义V2 CPU callback。
  已核对AOEM opcode113/AOIP0v5的checked整数GPU能力；当前FFI执行逐次创建
  device/operator，内部resident整数对象的复用生命周期未见可用公开ABI，输出
  key还需明确有界复用/释放契约。opcode114旧32B签名固定链域参考不能代替当前
  96B签名V3业务。下一片应沿同一候选补通用常驻契约并按依赖波次接实际业务，
  保留费用/nonce/失败/归并和证明绑定；不在SUPERVM另写CUDA或GPU执行器。
- 未重跑65,536笔负载，本节**没有新的TPS/P95/P99结论**；沿用下节历史测量边界，
  不以单笔证明/两笔功能结果签收吞吐。后续须分别测GPU业务、证明及联合满载。

本机补充回归：WSL native_pipeline纯回归378通过/58 ignored，真实AOEM附件
多片写入/重开/冲突与损坏拒绝通过，原单入口混合失败及冷重启回归2.01秒通过。
同版WSL A/A/B再验12.51秒通过。Windows最终release构建/测试构建、纯回归
379通过/58 ignored、真实附件存储及原单入口混合失败/恢复1.48秒通过；严格
node/all-targets Clippy、fmt及装配边界检查通过。首次WSL补充检查因命令行
变量转义启动失败，未运行测试，修正后重跑通过；原启动错误日志保留。
不以本机结果代替GitHub最终CI。旧49草稿和历史性能/失败证据继续保全。

## 2026-10-04 CI状态纠正与A/A/B观察交接

当前基线 `4e971c8c2f8cd34b222915695cebb47ed3b1d562`。
GitHub CI `37163386705` 已 completed/failure，Rust作业 `111321325493`
失败于实际原节点集成步骤。已独立读取失败日志及 artifacts API：A/A/B等
故障peer的Decision观察表超时，三个诚实节点快照均高度1/耐久决定1，但该
运行没有上传归档附件，不能凭快照补签收QC或其未执行的重启/后继步骤。
原失败日志、限制及复验见[观察交接证据](../artifacts/audit/resident-aab-observer-recovery-20261004-v1/ci-failed-job.json)。

修复仅在测试观察者：沿现有`RequestDecision`按精确父/域每250ms轮询一个
诚实peer；不改生产网络/共识、90秒门限或三票要求。故意遗漏每高度首次
已验真Decision但保留值指纹，仍拒绝冲突决定；后续原签名QC与三原节点
实际归档/回执/后继冷恢复核对。Windows release 11.21秒、WSL Ubuntu release
12.30秒通过，不是GitHub hosted同环境结果或多机验收。CI增加失败证据
白名单上传，排除配置/密钥/数据库。有限补传不回退。

本节修复提交 `cb883bd7`，原始失败/本机复验归档提交 `d44779b2`。
后续同候选完整证明交付见上节；GPU业务能力另按实际接口验收，不以测试
观察修复作为证明/异构执行完成。`d44779b2` 的GitHub运行 `37169117806`、
Rust作业 `111338311221` 中原节点集成步骤于2026-10-04 02:01:08 UTC通过，
A/A/B证据上传步骤也成功；这关闭该提交的hosted A/A/B观察缺口，**不是整轮CI
全绿或后续证明代码的CI结论**。本记录核对时该运行其余步骤仍进行中。
该hosted运行的原始公开附件及逐文件哈希已保全为
[GitHub A/A/B复验证据](../artifacts/audit/resident-candidate-proof-20261004-v1/github-aab/summary.json)。

## 2026-10-04 健康 peer 遗漏输入补传与故障等待交接

基线 `main@b9c612c323440a06cdbfc28435c7b807803e7283` 加本节所在提交。
仍是原 `novovm-node/native-resident-v1`，真实单 RPC 与负载共用同一 APFL、
常驻 `novovm-exec → aoem-bindings → AOEM`、耐久 round-bft/发布核心。
本轮只补输入传播交接，不重造节点、执行器、正文协议、共识或账本。

### 用户路径的实际变化

- 首次传播返回精确的**本地预约接收方**。RPC 对被配额跳过的 peer 留下提示，
  对方恢复额度后，直接复用原 pending 中的完整签名交易构造定向 APFL 批；
  不等下次碰巧轮到有输入的提案者，不向已经预约的 peer 周期盲重发。
- 新传播与补传轮流共用原来**一个** offer，接受后仍进入同一 channel/Taken
  通道、原来的全局和每 peer 配额。提示不保留第二份交易正文，不新增重试队列；
  每个既有 pending 仅加64位遗漏标记（池满时最多512KiB元数据），另有固定64项计数。
  确认或 nonce 失效清除原 pending 时同步清提示；未获 channel 接受的过时补传
  offer 丢弃重建，不复活已经确认的交易。接收方继续独立严格鉴权。
- 这属于**交易池缺输入**补交接。有效提案缺精确正文仍用原 `RequestBody(body_id)`；
  已决定高度落后仍用原 `RequestDecision`、归档证书与按需正文，没有混成同一语义。
- `received` 仍是非耐久内存接收。提示清除仅证明本地预约，不证明远端收到/执行。
  **已预约后准备失败、发送丢失/TTL、远端拒绝等情况不属于本轮遗漏提示补传保证**；
  钱包仍须保留未确认原文。稀疏尾部补传仍会扫描有界 pending，未签收故障大负载成本。

### 同条件故障对照：在线确认与离线追平分别计时

Windows release、同一AOEM DLL、同机三个原节点在线＋一个原验证者缺席；
集合仍4人且QC需3签。原32笔/16高度、每批2笔；第四节点后来无钱包重投追平，
四库重启后再确认2笔。旧/新EXE都使用本轮相同诊断夹具；运行顺序新1、旧1、新2。
保留8秒新批传播门、90秒共同高度无进展门、240秒总故障窗口；**生产计时器未改**。
传播门从总预约批数中扣除补传批数，不能靠重复接纳刷够原16个新批。

| 指标 | 冻结旧版 | 新版1 | 新版2 |
| --- | ---: | ---: | ---: |
| 三在线节点32笔确认及余额核对 | 156.633秒 | 43.055秒 | 42.920秒 |
| 最长观测共同高度无进展 | 22.519秒 | 10.376秒 | 10.323秒 |
| 第四节点归档追平及核对 | 2.425秒 | 2.432秒 | 2.443秒 |
| 冷重启及旧结果核对 | 0.700秒 | 0.723秒 | 0.697秒 |
| 重启后后继确认及核对 | 0.514秒 | 0.521秒 | 0.526秒 |

这些是含HTTP观察和夹具核对的有限故障样本，不是正常吞吐或精确内部提交时间；
上轮约122秒在线/126秒整体及早期90秒失败记录不覆盖。
新版两轮均只有离线提案者轮值的h4/8/12/16进入round1，其他初始高度在round0决定；
每份归档QC仍为3签，34笔均成功，冷库父链/回执/余额/nonce一致。
约10秒长间隔与这些高度的Propose/Precommit观测相符；短暂 follower 缺精确正文
仍有出现，但本样本没有引发额外决定轮次。**不据采样反推每个旧空轮的完整内部原因**。
故障余额检查用实际回执费用核对付款方/收款方；不冒充下面大负载的独立完整经济oracle。

A/A/B在同一新版通过（11.09秒）：三个原进程＋只签自己冲突提案的故障peer，
真实A/A/B prevote、nil precommit、诚实3签QC、确认后重启和后继均核对。
仍不称为单入口缺批与A/A/B联合注入，也不是未决签票/写入中断崩溃验收。
上轮CI `37160632235` 实为A/A/B握手诊断 `network status busy or poisoned`
提前失败，离线测试已通过；本轮新增非阻塞 `try_status` 区分WouldBlock(None)
与Poisoned(错误)，夹具只在原4秒内重试前者，后者立即失败。原CI日志保留。

故障原始报告、轮次与有限观测分析见
[故障对照](../artifacts/audit/resident-input-repair-20261004-v1/faults/fault-comparison.json)。

### 正常吞吐：全部六轮保留，吞吐接近但尾延迟未改善

沿用原单node0 RPC、四节点最终回执观察；每轮65,536笔真实预签名NOV转账，
1024付款方各64个连续nonce、共享收款方、最大批1024、64高度、8192在途。
Windows release/同DLL，计时包含验签、业务费用/nonce、网络、3/4QC、耐久发布、
四节点回执HTTP返回；不含钱包签名、启动及冷恢复。P95/P99由逐笔首次提交至
最慢节点回执HTTP返回计算，含查询观察。负载源码未改，不把补传/拒绝计入TPS。

| 顺序 | 成功TPS | P95秒 | P99秒 |
| --- | ---: | ---: | ---: |
| 旧1 | 8611.467 | 1.135485 | 1.203273 |
| 旧2 | 8295.212 | 1.101612 | 1.123634 |
| 新1 | 8017.108 | 1.264136 | 1.447706 |
| 新2 | 7951.416 | 1.362349 | 1.676896 |
| 紧邻旧3 | 7325.071 | 1.361044 | 1.539555 |
| 紧邻新3 | 8063.880 | 1.280961 | 1.293051 |

初始四轮新版相对旧版合并 **-5.52%**，因此增加且只增加最后一组紧邻对照；
最后一组为+10.09%，**不能只选偏低旧3宣布提速，也不能隐藏初始退化**。
全部六轮按总成功数÷总时间合并：旧 **8038.733** → 新 **8010.535 TPS**（-0.35%）。
由全部逐笔观察重新合并的P95 **1.224225→1.286485秒**（+5.09%），
P99 **1.396279→1.452020秒**（+3.99%）。本机样本吞吐接近，但没有稳定提速或尾延迟改善签收。
均顺序运行，计量期间无并行构建/其他压测；首次旧1/2用冻结测试EXE，最后旧3与新版
使用相同新测试EXE，负载实现未改。新1/3没有触发补传，新2确有7批/7168个接收方输入
补传预约；不能把正常下降全部解释成补传流量，也不把本地预约次数当远端送达数。
健康路径仍有额外mask/逐行pending查找成本，具体占比未profiling，不凭猜测归因。

六轮各65,536成功，业务失败/准入拒绝/重试/HTTP事件/未决均0；全部24份独立数据库的
完整经济oracle、费用/nonce、3签QC、状态根和冷恢复核对通过。此负载不是混合失败吞吐；
小功能回归另核对8笔中7成功/1业务失败、错误签名、2/4不发布及重启后继，不混入TPS分子。
原始报告、完整逐笔观察压缩及EXE/DLL哈希见
[六轮性能对照](../artifacts/audit/resident-input-repair-20261004-v1/comparison.json)。

### 本机验证与剩余边界

- `cargo fmt --all -- --check`、全workspace/all-targets Clippy `-D warnings`、
  原节点release构建及lib测试构建通过；原同名多binary源Cargo提示仍保留，未取消检查。
- native_pipeline纯回归370通过/56 ignored（下述分组单独运行），HTTP回归18通过，
  真实AOEM分组47通过，network状态相关14通过（含Busy/Poison反例）；两项原产品
  RPC失败/重启回归、离线两轮、A/A/B以及六轮负载均通过。未宣称运行全部ignored负载。
- 恢复装配检查通过（21个解析后产品成员、3个保留组件、无legacy依赖）；49项原草稿
  SHA256/长度仍一致，AOEM兄弟仓及runtime/legacy源码未改，原历史证据没有删除。
- 本轮有限缺批故障整改收口；接下来沿同一原产品继续APFL统一执行结构、GPU业务与
  完整证明接线，不扩成另一套网络/共识重写。未确认交易非耐久、已预约发送丢失、
  单入口缺批+A/A/B组合故障、故障高负载尾延迟、真实多机/公网仍有未验证边界。
  本节不签收GPU业务、完整业务ZK或正式生产。约10K仍是CPU阶段参照，不是性能终点。

## 2026-10-04 离线 peer 资源隔离、真实 A/A/B 恢复及单入口吞吐复测

基线 `main@bc00ace18bfce2d8789356e380f3afc9bc35cd11` 加本节所在提交。
仍为原 `novovm-node/native-resident-v1` CPU 实验配置，复用原 RPC、APFL 结构批、
常驻 `novovm-exec → aoem-bindings → AOEM` 和 `novovm-consensus/round_bft`。
未切回旧 Host 路径，未修改共识协议、签名/费用/nonce、生产计时器或资源总上限。

### 实际修复及限制

- 原传播队列虽轮询公平，一个离线 peer 仍能保留全部8个共享正文槽至30秒TTL。
  现在接受**新**传播任务前限制每 peer 的未完成义务；四验证者配置每个远端最多2槽。
  满额 peer 不被许诺这次新扇出；所有 peer 满额则返回背压，调用方保留原 offer。
  既有发送不驱逐，不伪造 Taken；ACK仍绑定 peer/session/scope/fragment/queued/TTL。
- 钱包已接受的原始交易仍留在原 pending，传播排队标记不等于交易寿命。
  这是有界尽力传播，不保证每个 peer 的 mempool 必达；健康 peer 也可能暂时缺批，
  之后依靠正常提案正文/轮换和已决定归档推进。**自动补传与故障尾延迟仍待改善。**
- 新增占用统计最初在每轮 poll 克隆 peer 字符串/更新映射。首版测量退化后，已改成
  `nov_chainStatus` 按需快照；保持配额和流控不变，不在空闲热循环构造诊断映射。

### 同条件性能：全部八轮保留，不把样本差异签成稳定提升

Windows x86_64、Rust release、同机四个原节点/四个独立AOEM库、同一DLL；
固定 node0 单HTTP入口，其余节点HTTP准入均0。每轮65,536笔真实预签名交易，
1024付款方各64个顺序nonce、共享收款方，最大批1024、64高度、最多8192在途。
计时包含严格验签、业务/费用执行、传播、3/4 QC、耐久发布及四节点最终回执查询；
不含钱包预签名、启动和冷恢复。P95/P99为首次提交至最慢节点回执HTTP返回，含查询观察。

| 轮次 | 成功TPS | P95秒 | P99秒 |
| --- | ---: | ---: | ---: |
| 旧版1 | 9262.036 | 0.977174 | 1.057862 |
| 首版统计1（中间版本） | 9221.476 | 1.141521 | 1.207691 |
| 旧版2 | 9657.458 | 1.122488 | 1.186554 |
| 首版统计2（中间版本） | 8411.362 | 1.258022 | 1.457383 |
| 旧版3 | 8881.170 | 1.015423 | 1.068986 |
| 最终版1 | 9019.611 | 1.028805 | 1.191039 |
| 旧版4 | 8237.706 | 1.350425 | 1.496974 |
| 最终版2 | 9110.104 | 1.228109 | 1.312714 |

先旧1/首版1/旧2/首版2，再旧3/最终1/旧4/最终2，均顺序运行且不并行构建/压测。
合并TPS用成功总量÷总耗时，合并百分位由完整逐笔观察重算：

- 首版统计两轮合并8797.809，对照旧1/2的9455.615为 **-6.96%**；失败的性能结论保留。
- 最终两轮合并 **9064.632 TPS**，P95/P99 **1.191039/1.304600秒**。
  同期旧3/4合并8547.345，样本为+6.05%；全部四轮旧版合并8978.568，最终仅+0.96%。
  全部旧版合并P95/P99为1.167359/1.456461秒。基线本身明显波动，**不签收稳定提升**。
- 八轮均65,536成功、业务失败0、准入拒绝/重试0、未决0，全部32份数据库经济/QC/冷恢复
  审核通过；这不是混合失败吞吐。最终第1轮保留1次 node1 回执**查询连接超时**，
  后续四节点观察完整，不把它隐藏为零网络错误或算成准入重试。

原始结果、完整逐笔观察无损压缩、错误events和EXE/DLL哈希见
[性能证据](../artifacts/audit/resident-peer-isolation-20261004-v1/comparison.json)。

### 故障验收口径

- 离线反例：三个原节点在线，第四个原节点缺席，验证者集合仍四个且需3签。
  旧EXE在8秒传播门停于8批、没有TTL过期；新EXE可在TTL之前接纳16批有限扇出。
  随后必须完成全部32笔在线最终回执，第四节点无钱包重投追平，再四库冷重启并完成
  后续2笔，逐库审核父链、执行承诺、原始QC签者与唯一链头。不能把本地接纳数当送达。
- 首次新版本在总90秒时只有30/32完成，完整失败报告保留；第二次原预算69秒全过。
  加逐高度诊断后另一轮124秒全过，32笔在线确认119.829秒，最大观测无进展间隔22.461秒。
  多个故意缺席提案者和缺正文健康提案者会累积空轮；首轮无末尾状态，不能声称已充分证明
  它与后轮完全同因。故障fixture改为共同已发布高度进展才重置90秒无进展窗口、总240秒；
  **8秒传播门不变，生产超时不变，所有交易仍须完成**。这些秒数不是正常负载TPS。
- A/A/B：**三个真实原节点＋一个仅签自己冲突提案的拜占庭测试peer**，不是四个原binary。
  捕获并验签诚实节点真实的round0 A/A/B prevote与nil precommit；由原节点自然换轮、
  独立执行B并自行形成三诚实签者QC，然后原库重启继续下一高度。没有外造QC、清锁、
  清签票日志或把故障身份当全新诚实签者重启。延迟旧提案仅排队重发，未签收接收处理。
- 最终按需统计EXE复跑：离线传播16批138毫秒，32笔在线确认122秒，完整故障流程
  126.21秒通过；第四节点追平、四库重启和后续2笔均通过。A/A/B为11.12秒通过。
  原两项RPC小批回归也通过，含7成功/1业务失败、2/4不能决定和重启继续交易；
  这些小批功能结果不冒充失败负载TPS。最终node/test/DLL哈希均列于证据文件。
- 本轮冷重启发生在已确认之后；**不是新做了原子写入中途断电/未知ACK的进程级验收**。
  公网/多机、GPU业务、完整ZK、隐私资产和原产品全部业务的接入仍未由这些测试签收。

故障原报告与失败说明见
[故障证据](../artifacts/audit/resident-peer-isolation-20261004-v1/faults/fault-summary.json)。
本轮不删除旧实现或证据，49份受保护草稿不变。CI加入相同原节点故障用例；本机通过与
Linux CI结果分别记录，不能沿用之前一次CI状态宣称本提交全绿。
本机最终验证：工作区fmt、全目标clippy `-D warnings`、原节点release构建、
resident纯回归362项、显式真实AOEM回归47项及上述4项原进程功能/故障用例通过。
装配边界检查仍为原产品21成员、保留组件3成员，无legacy依赖；这不扩大产品签收范围。

## 2026-10-04 常驻并发准入：真实单入口成功吞吐 5988→8446 TPS

基线 `main@ed2bb415a969a424a51d605b5a3e3845bccd4e86` 加本节所在提交。
交付范围仍为原 `novovm-node` 的 `native-resident-v1` 实验配置，不是默认主网
或全产品签收。钱包仍调用原 `nov_sendRawTransaction`；首次验签不再在 RPC
控制线程逐笔执行，而经同一 `CandidatePipeline/ComputeOwner → novovm-exec::
resident → aoem-bindings::resident → AOEM` 常驻会话批量计算。节点间 APFL
准入也使用该入口，接收方仍独立严格验签。业务、费用、nonce、QC 与耐久边界未减弱。

### 实际改变

- 原批鉴权图复用于逐行准入结果：坏签名只拒绝对应 RPC 行，不污染同图合法行；
  候选执行入口仍整批拒绝坏签名及重复交易，没有把准入结果提升为执行资格。
- HTTP 支持有界延后响应：最多8连接、原3秒期限/请求字节限制不变；等待签名
  时继续控制/网络/查询推进。验签完成后在控制侧重新核对当前投影、nonce预留
  和池额度，之后才返回 `received`，不是先接收未验证任务再补鉴权。
- 准入最多一个在途图，复用原compute owner，不新建Host验签执行器；计入保留
  字节并预留当前候选预算，不占候选批槽。取消HTTP等待者不撤销已接收native任务，
  结果按不可复用token消费，不能交给后来的连接。已运行图不可抢占，非零成本承诺。
- APFL借用行可直接验签、流式比较完整规范字节和计算正文承诺；缓存仍绑定原
  公钥/签名字节，不以不含签名的canonical hash命中代替验签。新入池时才投影
  那一行raw；既有BatchPlan/packet/proof仍整体物化canonical正文，不是零展开。
- 实际读取/派发进展不再强制睡1ms；纯等待/WouldBlock仍让出。每连接每轮16KiB
  读取额度不变，没有放宽连接、消息、验签、网络或最终确认限制来获取数字。
  新状态 `rpc_authentication` 显示同owner和检查进度，**不作为唯一交易分子**。

### 同条件单入口65,536笔

Windows x86_64 / Rust1.94 Release / Core Ultra9 275HX / 24逻辑线程，四原node
独立进程及AOEM库，使用相同随仓DLL、WSS/E2E与负载配置。冻结ed2原node/test
binary，旧1→新1→旧2→新2；计时期间没有本任务其它构建/重测试。
1024付款方×64nonce、共享收款方，最大批1024、在途8192；批内付款独立、收款
可归并、跨批nonce有序，不是高冲突负载。仅node0 HTTP准入65536，其余为0。
计时含验签、业务、网络、BFT、四库耐久及四节点最终回执HTTP观察，不含预签名、
启动和末尾冷恢复。P95/P99为逐笔首次提交至最慢节点回执HTTP返回，包含查询调度。

| 轮次 | 成功TPS | 窗口秒 | P95秒 | P99秒 | 成功/业务失败 |
| --- | ---: | ---: | ---: | ---: | ---: |
| ed2旧版1 | 5872.747694 | 11.159342 | 1.723848 | 1.736567 | 65536/0 |
| 并发准入1 | 8120.749558 | 8.070191 | 1.360964 | 1.572067 | 65536/0 |
| ed2旧版2 | 6108.893335 | 10.727966 | 1.746097 | 1.809007 | 65536/0 |
| 并发准入2 | 8798.688252 | 7.448383 | 1.225821 | 1.254719 | 65536/0 |

总成功/总秒合并 **5988.493423 → 8446.136868 TPS（+41.0394%）**。
两轮尾延迟均降低，但只是本机有限样本，不是稳定主网容量或历史WSL10K的同条件
复现。收益来自本轮合并改动，未单独测定并发验签、流式处理、轮询各自的贡献。
四轮全部64×1024块，零业务失败/永久拒绝/HTTP传输错误/重试/未完成；四库核对
65536回执、付款nonce64、收款65536、费用2949120、完整状态根和逐块3签QC，
冷重启后相同。新旧链头/状态及回执根相同，未改变协议或证书域。

新两轮每节点各64个准入图、65536行独立鉴权；图未完成期间控制轮询分别为
`[243,411,454,391]`、`[280,355,409,436]`，证明真实产品等待期间继续推进。
它不是主线程所有工作均已并行的证明；JSON解析、字典组装、池决策仍在控制侧。
陈旧父状态拒绝和停机连接重置日志保留，不把HTTP错误0扩大为全日志无错误。
原始报告、SHA256、完整统计及初次功能失败见
[本轮对照](../artifacts/audit/resident-async-auth-20261004-v1/comparison.json)。

### 功能与未完成边界

混合 `[坏签名,合法,同nonce冲突,合法,查询]` 保留原输入和独立结果。
初次功能测试的固定“三块”断言失败：在图上限2行时，拒绝行也消耗检查额度，
两笔合法交易可分别进入小块。这不是JSON-RPC数组的原子块承诺；不重排输入隐藏
碎片、不新增跨HTTP全局屏障。验收改为核对实际连续高度、每块1..2笔、唯一完整
交易、各节点相同块哈希/回执及head/state_version，并保存实际分组；完整经济、
冷恢复和后继整批检查保持。失败密集负载的TPS尚未测量，不能拿拒绝数补吞吐。

最终功能实测分组为重启前 `[1,1,2,2]`，重启后追加一块2笔；8笔中7成功、
1业务失败，四节点回执/余额保持，2/4不能确认、3/4推进和第四节点追平通过。
单入口功能测试另行通过同样7成功/1业务失败与重启继续高度。最终Release
native_pipeline纯测试356通过/54显式ignored，18项HTTP测试、47项显式真实
AOEM/耐久/恢复测试通过；root fmt、workspace/all-targets Clippy `-D warnings`、
Release构建和装配边界检查通过。未声称全部旧node业务回归通过。
新两轮负载的node binary与最终功能测试相同；测试binary后续只修正上述块型
断言，负载和计时未改，两个test binary哈希分别记录在对照中。
此前e0版本Linux CI有旧路径冷重启RPC读取超时；本轮未通过增加超时绕过，
本机通过不等于该问题已关闭，也不预先宣称本提交GitHub CI全绿。

`received` 仍为非耐久内存准入，HTTP期限到达不保证任务未曾接受，客户端应按
原签名重投查询。离线peer的全局出站槽/TTL限制、真实原节点A/A/B分票恢复尚未
签收，不能用本轮正常负载和组件恢复反例代替。下一故障切片在这条同一快路径
验证上述边界，不退回旧慢执行或另建控制器。
CPU后端仍为 `aoem_semantic_v2_cpu_callbacks`；完整代数opcode114、GPU业务、
完整业务ZK及联合运行未完成，约8K/10K均非最终目标。EVM/nETH/多资产/治理/
Execute未全部接入该实验配置。49既有草稿SHA256未变，AOEM兄弟仓、runtime/
legacy未修改；未删除后续成果。Linux/多机/公网未由本机结果签收。

## 2026-10-04 APFL：原节点结构批接入（功能通过，吞吐提升未达成）

基线 `main@e0b162fec38d899ad7ce0d9058116330c29b5a87` 加本节所在提交。
本轮在原 `native-resident-v1` 实验配置中接入真实签名 V3 Transfer 的 APFL
结构批：原 RPC 选批/传播/提前正文 → 原 controller/channel → 同一常驻
`novovm-exec::resident → aoem-bindings::resident → AOEM` 鉴权与业务编译，
再经原 round_bft 耐久决定、查询及恢复。不是另一套节点、共识或权威状态。

### 已接通与未接通的具体边界

- `native_pipeline/ingress/apfl.rs` 的 `NVAPFLV3` codec v1 对九个业务字段
  使用共享值、首次出现顺序字典/索引或稠密残差列；96B原公钥/签名字节逐笔保留。
  支持一般收付款方、金额、资产和费用字段，不依赖压测公式，不由接收方重新签名。
  旧APFL gate的32B seed重签与USDT Execute模板不是本交易格式，不予接入。
- 解码先核对长度、索引、字典规范性和展开预算，再复制字段；上限按原始交易
  字节计量，不允许用压缩绕过有界资源。错误签名、链域、重复交易仍各自拒绝。
- 不可变批次的借用视图经原AOEM计算会话逐笔严格鉴权，原业务编译直接读取
  同一视图。保留独立节点验签、同nonce竞争、完整费用/失败消耗nonce、精确父
  状态与owner/permit约束；并未引入一个Host验签线程池或改变共识票数。
- 新transport tag11/12/13分别携带Body/EarlyBody/Transactions；显式payload
  v1。旧tag1–10可读，旧节点拒绝新tag，因此参与节点须一起升级，暂无自动协商。
  规范V3签名字节、交易/正文承诺、候选及日志域不变；结构分片ID与正文ID分开。
- 网络仍为现有NovoRUDP Data封装、E2E、二进制relay及WSS/TLS；**不是本轮
  新接通原生UDP DATA/REPAIR/ACK全套传输**。没有以APFL取代共识或业务验证。
- **仍有展开边界**：RPC内存池保留raw，gossip准入逐行投影并调用原strict admit；
  进入既有BatchPlan时仍整体生成canonical V3正文，以复用packet/存储/证明契约。
  原业务请求仍逐行编译。不是端到端零展开，也未签收opcode114完整NOV执行。
  CPU回调后端保持 `aoem_semantic_v2_cpu_callbacks`，没有新增CUDA/GPU/ZK旁路。

### 单入口65,536笔同条件复测

同机Windows x86_64、Rust1.94 Release、Core Ultra9 275HX/24逻辑线程，四个
真实原node进程、四AOEM库、同一随仓DLL、WSS/E2E。冻结e0原node/test binary，
与本提交最终binary交替旧1→新1→旧2→新2；计时期间无本任务其它构建/测试。
1024付款方×64nonce、共享收款方，批上限1024、在途8192；每轮实际64×1024块。
批内付款方独立、收款可归并，跨批同付款方nonce有序，**不是高冲突负载**。
仅node0 HTTP准入65536，其余为0；各节点仍独立鉴权执行。计时从首次HTTP提交
到四节点全部返回一致最终回执，含验签/执行/网络/BFT/四库耐久及查询观察，
不含预签名、启动和末尾冷恢复。P95/P99为逐笔首次提交到最慢节点回执HTTP返回，
含有界查询观察调度；不等于纯共识时间。未更换分母或降低确认/耐久门槛。

| 轮次 | 成功TPS | 窗口秒 | P95秒 | P99秒 | 成功/业务失败 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 原版单入口1 | 5555.196244 | 11.797243 | 1.756735 | 1.790363 | 65536/0 |
| APFL单入口1 | 5743.386749 | 11.410689 | 1.518991 | 1.779112 | 65536/0 |
| 原版单入口2 | 5960.672260 | 10.994733 | 1.562519 | 1.737910 | 65536/0 |
| APFL单入口2 | 5701.765376 | 11.493984 | 1.788310 | 1.822622 | 65536/0 |

按总成功/总秒合并 **5750.795806 → 5722.500382 TPS（-0.4920%）**。
本样本没有吞吐或尾延迟稳定改善证据；不能以第一轮小幅增加宣布提速，也不把
历史6381.73或WSL10401最高样本当成本轮共同基线。结构接入已交付，性能目标
尚未完成；约10K仍是CPU参照而非最终目标。

四轮均零业务失败、永久拒绝、HTTP传输错误、重试、未完成交易；完整四库核对
65536回执、1024付款方nonce=64、收款65536、费用2949120、全状态根及逐块3签QC，
冷重启更换PID后结果一致。跨版本链头/经济结果保持一致。另有混合失败功能验证，
但这四轮**没有测量失败交易吞吐**。源码和状态诊断仍有陈旧父/上下文拒绝，退出
时有连接重置日志；“HTTP传输错误0”不能扩写成全网/全日志没有任何错误。

新两轮每节点 `apfl_view_transactions_total=65536`，证明实际完成的业务计算
使用APFL来源视图；该计数一般可包含重算/陈旧候选，**不作为唯一最终交易分子**。
最终确认仍由独立回执和冷库oracle验证。`apfl_transport`显示本负载的应用消息
编码约比对应canonical交易字节少24.3%；包含应用头但不含TLS/relay开销，重复
编码/转发也会累计，不是全链路流量测量。小批次应用头可能使编码大于原文。
原始四轮、功能报告、binary/DLL哈希与复跑口径见
[比较汇总](../artifacts/audit/resident-apfl-20261004-v1/comparison.json)。

### 验证与下一实际缺口

最终Release native_pipeline默认341通过/53显式ignored，其中46项真实AOEM、
鉴权/父绑定、耐久签票/未知ACK及恢复测试单独运行通过；两项原binary RPC功能
测试各8笔（7成功/1业务失败）通过，保留2/4不确认、3/4推进、第四节点追平、
错误签名/nonce拒绝、费用守恒、冷重启后继续高度。APFL/raw对照覆盖不同付款方
长度/收款方/金额/费用、成功失败/nonce、完整状态/回执根与证明输入字节一致。
**证明输入一致不等于已生成或验证ZK**。codec另覆越界/截断/非规范字典/伪报大小。
root fmt、workspace/all-targets Clippy `-D warnings`、node Release构建及装配
边界检查通过；不声称完整原node所有lib或Linux/多机回归已通过。

本轮没有解决首次RPC同步验签、控制线程字典组装、反复raw投影/解码，未将其中
任意一项猜测为唯一瓶颈。下一片应在同一测量路径消除这些往返，复用常驻鉴权
owner而非新造引擎，并继续真实节点A/A/B分票及离线背压验证（本轮未签收）。
非耐久pending、离线peer全局槽等TTL限制不变；业务GPU、完整业务证明及联合
运行性能未接通，EVM/nETH/多资产/治理/Execute也未全部接入此实验配置。
49项既有草稿SHA256逐项未变，AOEM兄弟仓/runtime/legacy未修改，未删除资产。

## 2026-10-04 R3 续作：单入口传播与真实 RPC 提前正文（CPU，未签收生产）

基线 `main@d431a74bef8bee628202c35c3f9ae52533253fb8` 加本节所在提交。
本轮新增用户能力：在既有 `native-resident-v1` 实验配置下，只向一个原
`novovm-node` RPC 提交原始签名交易，其余三个节点通过同一双工网络接收、
独立鉴权和执行，四节点最终回执一致，冷重启后可继续交易。不再要求钱包向
四个 RPC 重复提交；不是另建节点、签者、账本或交易演示程序。

### 实际接线与边界

- `native_pipeline/service/rpc.rs` 复用既有 early-body／精确父绑定流程，
  当前候选处理期间可提交下一批正文；仍经 `novovm-exec::resident →
  aoem-bindings::resident → AOEM` 同一常驻计算／存储 owner。
- 投影追赶不再全局拒绝严格验签后的内存准入；选单仍必须等到精确父状态投影。
  `received` 明确为非耐久、未执行、未最终确认，陈旧 nonce 由投影淘汰，
  实际执行再次核对状态。并未取消 nonce、费用或父状态检查。
- 同 hash 的重试仅在**原始字节完全等于已验签 pending**时复用鉴权；
  canonical hash 不含签名，故篡改签名仍必须重新验签并拒绝。
- 同一 controller/channel/duplex 新增独立有界原文传播通道和逐 peer 流量窗口。
  transport 新 tag 9/10 有显式 payload 版本；旧 tag 1–8、共识签名字节和
  耐久日志域未重标。旧 decoder 拒绝新消息，节点须一起升级，未做自动协商。
  `TransactionsTaken` 只证明完整正文交给有界消费端，不代表验签、入池、
  落盘或最终确认；接收方仍严格验签，收到的正文不无限重复 gossip。

### 同条件前后复测与单入口负载

Windows x86_64、Rust1.94 Release、Core Ultra9 275HX（24逻辑线程），
同一 AOEM DLL、四真实原 binary 进程／四库、同机 WSS/E2E。冻结基线 binary
与测试程序后，交替运行旧1→新1→旧2→新2，随后单入口两轮；计时期间没有
本任务其它构建或测试。共同负载为1024付款方×64个nonce、共享收款方，
65,536笔真实预签名交易，信用窗口8192、每批最大1024，每轮实际64×1024块。
批内付款方独立、收款为可归并信用，跨批同付款方 nonce 有序，费用按原规则归并；
不是高冲突写负载，也不将余额不足拒绝计作成功 TPS。

计时从第一次 HTTP 提交至四节点全部返回一致最终回执，包含验签、AOEM执行、
网络、BFT、四库耐久和 HTTP 查询观察／重试；预签名、启动及完整冷恢复在窗外。
P95/P99 为逐笔第一次提交至最慢节点最终回执 HTTP 返回，包含观察调度。

| 实测轮次 | 成功TPS | 窗口秒 | P95秒 | P99秒 | 成功／业务失败 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 修改前 fanout 1 | 4200.725397 | 15.601115 | 2.958796 | 2.973627 | 65536 / 0 |
| 修改后 fanout 1 | 6741.703644 | 9.720985 | 1.240330 | 1.276609 | 65536 / 0 |
| 修改前 fanout 2 | 4763.965070 | 13.756608 | 1.730105 | 1.748826 | 65536 / 0 |
| 修改后 fanout 2 | 6722.499437 | 9.748755 | 1.282213 | 1.297113 | 65536 / 0 |
| 修改后单入口 1 | 6676.655433 | 9.815693 | 1.421735 | 1.459562 | 65536 / 0 |
| 修改后单入口 2 | 6111.752674 | 10.722947 | 1.469146 | 1.498623 | 65536 / 0 |

前后同为四路客户端 fanout，两轮按总成功／总秒合并为 **4464.65 → 6732.09 TPS
（+50.79%）**。旧版55296／45056次投影滞后可重试响应，新版均0；耗时没有扣除。
单入口只对node0提交65536次，其余三个节点HTTP准入数为0，但仍要求四节点全部
最终确认；合并6381.73 TPS。这是不同提交拓扑，不能混作同条件前后提升比例。
两轮组样本保留全部波动，不声称长期主网容量或高于历史Linux最高样本。

六轮均无永久拒绝、未完成交易、传输失败或业务执行失败，四库各自完整核对
全部回执、1024个nonce=64、收款65536、费用2949120、完整经济状态根及逐块3签QC；
四进程冷重启换PID后全部回执和链头不变。新四轮传播统计无drop/expired，
并不代表离线／丢包场景已经验收。前后最终链头和经济承诺逐值一致。
证据及完整计时口径：[比较汇总](../artifacts/audit/resident-rpc-ingress-20261004-v1/comparison.json)
（含六份原始报告、源文件／binary／AOEM哈希和复跑入口）。

提前正文确实进入产品：新fanout两轮四节点累计 `early_bind_reused=131/108`，
单入口为146/162；但 `early_authentication_completed_before_parent` 分别只有
2/1和1/1。不能将全部复用数说成父提交前完成，或将组合改善全归因提前鉴权。
初次入池验签仍在RPC控制线程，是后续并发接入要处理的明确边界。

### 功能、回归与未关闭项

单入口功能用例选择初始非提案者node0，覆盖正确收付、错误签名、陈旧nonce、
余额不足及下一nonce继续、最终交易重放不重复传播、四库重启后继续；8笔中
7成功／1业务失败，失败nonce与费用守恒正确。原fanout功能用例再次覆盖2/4
不能确认、3/4推进、第四节点追平；新增pending同hash坏签名不能命中缓存反例。
两份功能证据见 [单入口](../artifacts/audit/resident-rpc-ingress-20261004-v1/single-ingress-functional.json)
和 [原RPC](../artifacts/audit/resident-rpc-ingress-20261004-v1/product-rpc-functional.json)。

本机 Release 默认 native_pipeline 328通过／50显式ignored；其中43项真实AOEM
鉴权／父绑定／签票／未知ACK／独立进程恢复测试另行执行通过，实际RPC功能两项
和上述长负载另跑通过。原HTTP13项、root fmt、全workspace/all-targets Clippy
`-D warnings`和装配边界检查通过；CI增加单入口真实原binary回归。
**不声称完整原node全部lib通过**：上一节记录的旧入口失败未改动或隐去。
运行期间仍记录了上下文／陈旧归档父请求拒绝（含旧binary已有文本），
`last_error` 是最近拒绝而非全量事件计数；不能写成“日志无任何错误”。

仍不签收生产：传播是非耐久best-effort，离线peer或丢失Taken可占满有限全局
outbox直到TTL；尚未验收持续故障吞吐。初次RPC鉴权异步化、真实原节点A/A/B
分票恢复、同候选AOEM业务GPU／完整证明及其联合性能继续未关闭。
V3钱包签名仍是原chain_id域，传播的genesis绑定不扩大为跨创世钱包防重放。
既有EVM／nETH／多资产／治理／Execute资产未删除，但并非已全部进入此实验配置。
49项草稿哈希未变，未改AOEM兄弟仓、未删除runtime/legacy或数据。
下一刀应在**这同一入口和常驻核心**消除控制线程首次鉴权串联，保留本轮
单入口与65,536笔对照，同时推进真实节点分票／恢复验收；不是回去维护旧2.x路径。

## 2026-10-04 R3 实际迁接：常驻快链路进入原节点（实验配置，未签收生产）

开发基线 `main@595a092e2a236f42dc012c1f3a605ef5404624c6` 加本节所在提交。
不是新的路线文档：本轮实际代码位于原 `crates/`；原20个显式产品成员未退出，
`runtime/` 不参与产品依赖，AOEM兄弟仓库未修改，49项受保护草稿逐SHA256核验未变。
后续目录清理仍待整合验收；本轮没有删除 runtime、legacy、证据或原产品模块。

### 用户入口与真实调用

原 `novovm-node` binary 增加显式 `NOVOVM_NODE_MODE=native_resident`，配置文件
`NOVOVM_NATIVE_RESIDENT_CONFIG` 的 profile 必须是 `native-resident-v1`，启动必须
显式选择 `NOVOVM_NATIVE_RESIDENT_START=create|existing`。配置路径相对配置文件解析。
它在启动旧控制器前返回，不同时运行两套签者／链头，不启用旧Host许可，不改默认
`full` 模式或把新证书塞入旧V3发布器。这是独立测试创世的实验配置，不是部署授权。

真实路径：原 `native_fresh_rpc` HTTP传输／原RPC方法 →
`novovm-node::native_pipeline::service::RpcLifecycle` → 同一 `ResidentNode`／
controller／CandidatePipeline → `novovm-exec::resident` →
`aoem-bindings::resident` → AOEM V2语义计算与常驻存储owner。
原RPC默认请求大小不变；仅该批量配置显式使用有界4MiB请求、至多1024个JSON请求。

| 已迁入的机制 | 原产品位置与边界 |
| --- | --- |
| 常驻计算／存储、签名鉴权、经济归并、批读批树、父后态复用、后继及提前正文 | `novovm-node/src/native_pipeline/`，固定 `a7db795` 已提交源，不读本地runtime草稿 |
| 轮次内核、收票、pacemaker、耐久签票／决定及恢复契约 | `novovm-consensus/src/round_bft/`；node只提供同owner存储及真实候选适配，不重复实现安全内核 |
| 签票状态、outbox、head、block原子条件提交 | 同一AOEM存储owner；Applied／AlreadyPresent ACK后才输出签名／决定，不拆成多次写入冒充原子性 |
| 增量TLS／WS、多在途转发、乱序结果关联、背压与公平队列 | `novovm-network/src/duplex/`，不再调用旧同步等转发结果接口 |
| 统一门面与证明适配 | `novovm-exec::resident` 复用原wire/GPU门面；bindings迁入AORCP002；同候选journal+独立image pin的prove后verify接口 |

历史wire／存储域字节未因搬目录而重标。共识签名测试工具仅在dev依赖启用。
新配置当前执行后端准确声明 `aoem_semantic_v2_cpu_callbacks`；并非已完成业务GPU化。
完整业务证明关系和AORCP002适配已迁入，但RPC没有自动生成／验证证明，guest构建
及业务GPU仍待接入同候选生命周期。三个业务GPU／证明状态保持false，不以QC充当ZK。

### 65,536笔共同条件CPU对照

Windows x86_64／Rust1.94.0 Release／Core Ultra9 275HX、24逻辑线程，四独立验证者
进程、四库、同机WSS/E2E；同一随仓AOEM DLL。冻结历史源141个Rust文件经LF规范
后与 `a7db795` 一致。交替运行旧1→新1→旧2→新2，计时中无本任务构建或其它测试。
每轮1024个不同付款方、共享收款方、64个nonce轮次；每块1024笔，共65,536笔真实
预签名Transfer。计时含验签、业务、网络、BFT及四库耐久ACK；不含钱包预签名、
启动／创世和末尾冷恢复。不是引擎算术分数，也不是RPC吞吐或稳定主网容量。

| 同机共同负载 | 成功TPS | P95秒 | P99秒 | 成功／业务失败 |
| --- | ---: | ---: | ---: | ---: |
| 历史参考1 | 5671.832428 | 10.9399872 | 11.5546432 | 65536 / 0 |
| 原工程迁入1 | 7249.792579 | 8.3227528 | 9.0397069 | 65536 / 0 |
| 历史参考2 | 6269.637521 | 10.0163769 | 10.4529169 | 65536 / 0 |
| 原工程迁入2 | 6957.259500 | 8.7762979 | 9.4198010 | 65536 / 0 |

P95/P99是积压一次释放后64个高度的完成观察延迟，不是稳态逐笔RPC延迟。
四轮全部经济状态、费用、nonce、回执及冷恢复通过；最终块、状态、回执承诺和
决定哈希逐值一致。合并观察5955.77→7100.51 TPS（+19.22%），只有两轮且依赖图
有差异（socket2历史0.6.1／原工程0.6.3），不归因某个算子优化，不签收长期容量。
原历史Linux 7496.539／10401.483结果保留，不能与本次Windows数字直接作退化比例。
可核对二进制／AOEM哈希、每轮原始路径及计时定义见
[CPU对照报告](../artifacts/audit/resident-fastpath-20261004-v1/cpu-comparison.json)。

### 真实RPC、正确性与尚未关闭的交付项

四个真实 `novovm-node.exe` 进程经HTTP提交V3签名交易，未启用旧Host许可。
功能复测8笔最终交易：7成功、1余额不足失败；2/4不能确认、3/4推进、第四节点
追平，四库冷重启后继续第四高度。完整回执、费用361、余额总额+费用=创世总额、
失败消耗nonce、错误签名与冲突重放拒绝均核对。客户端显式向四节点广播相同签名
输入，不是mempool自动gossip。[功能证据](../artifacts/audit/resident-fastpath-20261004-v1/product-rpc-functional.json)。

随后同一Release原binary完成两轮真实RPC负载。仍是1024付款方×64个nonce、共享
收款方的65,536笔预签名交易；客户端向四节点显式fanout、每节点1个HTTP在途、
总4路并发，JSON批至多1024、交易信用窗口8192，不逐块等待finalized才发下一块。
分子只计四节点均有一致最终回执的唯一成功交易；计时包含HTTP、拒绝重试和查询
观察，预签名／启动和后续完整冷恢复在窗外；没有关闭验签、耐久或修改共识门槛。

| 真实原binary RPC | 成功TPS | 窗口秒 | 逐笔P95秒 | 逐笔P99秒 | 成功／业务失败 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 第1轮 | 4715.747348 | 13.897267 | 1.941938 | 1.990161 | 65536 / 0 |
| 第2轮 | 4633.248963 | 14.144718 | 1.926097 | 1.953998 | 65536 / 0 |

两轮均实际形成64个1024笔块，逐块3签QC，四库完整冷恢复、全部65,536回执、
1024付款方nonce、费用2,949,120、共享收款方65,536及完整逻辑状态根通过。
没有永久拒绝、传输错误或未完成交易，但每节点提交响应累计分别有48,128／44,032
次投影追赶的可重试拒绝（不是这些数量的唯一交易失败）；原签名重试和耗时全部计入。
P95/P99以每笔第一次提交到四节点最慢HTTP回执完成算，含观察调度，不与上表高度
积压延迟混用。仅两轮同机有限负载，不签收持续容量；成功负载的0业务失败不能替代
前述余额不足功能用例。报告保留实际batch/QC/四库经济核对，而非仅打印TPS：
[RPC第1轮](../artifacts/audit/resident-fastpath-20261004-v1/product-rpc-load-1.json)、
[RPC第2轮](../artifacts/audit/resident-fastpath-20261004-v1/product-rpc-load-2.json)。

当前差距有明确边界：RPC仍在控制线程做入池验签，投影追赶阻断准入造成大量
重试，且尚未喂入提前正文接口；这些是实际代码／计数证据，但还不能把核心负载
与RPC窗口之差全部归因于其中任一项。下一性能切片优先复用已有提前鉴权／正文
流水线消除这些串联等待，同时补单入口传播；不是回去优化旧2.x路径，也不以
新GPU算子未完成为由等待。真实节点A/A/B和同候选GPU／证明保持独立未关闭项。

本机验证：root全工作区 `cargo fmt --all -- --check` 和
`cargo clippy --workspace --all-targets --locked -- -D warnings`通过；原binary Release
构建通过。共识round_bft 86项、duplex Release含计时反例211项、bindings resident
32项（含显式真实AOEM计算／存储，不含真实ZK证明）、exec resident 3项、原HTTP
13项通过。最终迁入模块Release默认314通过／49明确ignored（其中真实用例另跑）；
迁入流水线的42项显式真实AOEM测试通过，含未知ACK、独立进程恢复及
连续高度；不是仅编译或空模拟。CI在原产品job增加迁入模块与真实原binary RPC功能
检查，未将ignored长负载或纯关系测试说成GPU／主网验收；本轮未提前声称远端CI绿。

扩大原node全部lib的可选检查**没有通过并已停止**，不列总套件PASS。已单独复现
的旧`nov_swap`期望成功与R1 `7b10c7f3`的Host默认拒绝冲突；另两项native_selection
测试参考仍接受Execute，而R2 `e64a52477`能力门禁只允许Transfer。涉及源码和测试
blob均与基线`595a092`相同。本轮不解除门禁来绿测；其余旧入口失败未一概归因，
完整旧业务回归仍未验收。旧network全lib初次并发测有1项block_range_update失败，
未改代码的串行复测486通过／1既有忽略；不掩盖初次失败或声称根工程所有测试全绿。

仍需分别验收，不提前宣布完整快链路／主网完成：

- RPC大负载已按上表实测；核心夹具的提前正文已启用，RPC目前只接后继准备，
  不能宣称两者入口成本及调度完全相同。交易池本地、非耐久、无自动gossip。
- 最近65,536条回执索引是从耐久归档恢复的查询投影，不是另一个权威账本；超范围
  返回查询未完成，当前不是全历史随机查询产品。非零历史余额不从旧测试账本导入。
- 四真实原节点A/A/B分票注入及恢复尚未执行；已有成套内核／ACK／崩溃反例保留，
  不以2/4、迟到同值票或纯状态机测试冒充这个场景。
- 当前时间为创世时间+高度的逻辑毫秒，尚未签收生产出块／链时间／按日费用策略。
- GPU业务执行、证明队列／证明最终性及联合吞吐未测；CPU基线不是终点。
- EVM、nETH、多资产、治理、Execute原资产未删，但未汇聚到本实验配置，不能把
  NOV贯通用例说成全部产品接口已经统一。Linux／公网／实体多机本轮未测。

## 2026-10-04 路线纠正：CPU快链路不是最终性能目标

本轮基线`main/origin/main@b2f4a66`；只核查已有代码/报告并更新约束，无代码迁接、
新性能或GPU/ZK测试。约10K原始证据属于固定`a7db795`的65,536笔四进程实链路，
不是纯引擎跑分；约7,497与10,401两轮及其限制保留，不提升为稳定主网容量。
当前2.x原RPC路径仍未成套接回后继重叠、提前鉴权、父后态复用、批读取、双工
发送及耐久共识/发布恢复。常驻AOEM和真实业务计算已接回，不说全部退回Host。
下一主线改为完整快链路进入原产品，见[当前路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md#2026-10-04-当前优先级完整快链路与统一异构接入)；不是继续旧结构百分之几的小修。

用户补充：**约10K是CPU阶段参照，不是最终目标；统一语义、GPU性能和业务ZK
仍是核心交付。** 当前原Transfer GPU数值计划仍仅测试接线；原bindings接收
`AORCP001`，后续完整NOV证明适配已为`AORCP002`，该差异须带独立验证和反例
迁入统一门面。现有AOEM GPU/证明能力不因此被否认，也不等于完整NOV关系已经
GPU加速。分别验收CPU最终确认TPS、真实GPU业务执行、业务证明吞吐/延迟；
不混用这些成绩，不等待新GPU工作才恢复已验证CPU链路。本轮没有性能提升声明。

## R2 性能切片：原节点实测、批量树复用与轮询预算

日期2026-10-03；开发基线`main/origin/main@4de28f3b295971de520cc00e8c1bc5b53224ca8b`
加本节所在提交。原node→exec→AOEM产品路径，Host许可0；不改AOEM库、业务费率、
签名检查、QC门槛、协议、状态发布格式或恢复规则。本轮性能结果不替代R3整合。

### 实测口径与发压器纠正

本机Windows x86_64 / Rust 1.94.0 Release，Core Ultra 9 275HX、24逻辑线程、
约63.43GiB内存。四个原节点独立进程／AOEM RocksDB，同机WSS；单入口RPC、
4路客户端、128笔在途上限。256笔真实签名Transfer，32组不同收付款账户各8笔，
同一付款方按nonce冲突，不同账户组分离；费用仍按原规则归并。最大块32笔、
collect=0、tick配置250ms、seal poll=100ms，交易传输Bounded64配置不变。
`durable_receipts=false`是该交易传输ACK选项，不是关闭业务回执持久化。

TPS分子仅计四节点都查到成功回执的唯一最终交易；分母从首笔提交至全部四节点
观察完成。P95/P99采用nearest-rank，包含客户端HTTP和状态观察等待，不是精确
链内提交时间。签名预生成、创世／启动及最后重启验证在计时窗外。正式测量关闭
`NOVOVM_NATIVE_FRESH_TIMING`，期间不并行运行本任务的构建或其他负载。
每轮另核对完整块、QC、nonce顺序和冷重启读回；经济余额由另列的混合交易回归
核验，不把pending计入最终确认。

初次使用原发压器时发现：每提交4笔，先等待最多32次状态请求（每组并发4），
才补新交易。优化后某轮观察占85.53/97.17秒，在途最高98而未达128；较低排队量
和更晚提交会影响尾延迟。因此保留以下试验，但不据此宣称饱和TPS或延迟收益：
原节点2.5865／2.2172 TPS；仅批树2.5487；批树加轮询预算2.6347／2.5704。
它们都是实际短负载观测，不是链容量上限，不能与新发压策略混合求平均。

现有夹具改为`client_refill_policy=credit_first_bounded_v2`：先按原4路补足128
额度，再做有界公平查询；每组查询释放额度就优先补压。没有增加客户端并发、
上限或测试交易数；原300秒截止、每组准入错误保全、观察游标及最终性断言保留。
`capacity_limited_turns`仍指一轮开始就在上限，不静默改旧统计定义。
这是负载器策略版本，不是交易或共识协议升级。新旧节点须使用同一修正夹具重测。

### 同一修正夹具的正式交替对照

执行顺序为旧1→新1→旧2→新2；期间没有构建／其他测试并跑。每轮均实际达到
128笔在途，最大客户端并发4，使用上述相同负载及配置。

| 版本／轮次 | 成功TPS | P95（秒） | P99（秒） | 成功／业务失败 |
| --- | ---: | ---: | ---: | ---: |
| 修改前1 | 2.286307 | 111.598976 | 111.690189 | 256 / 0 |
| 修改后1 | 2.292516 | 111.016636 | 111.092039 | 256 / 0 |
| 修改前2 | 2.252598 | 112.958745 | 113.023444 | 256 / 0 |
| 修改后2 | 2.339251 | 108.719594 | 108.815884 | 256 / 0 |

四轮准入拒绝／准入不确定／已准入未完成均为0，不用拒绝计数制造TPS；业务失败
负载未加入性能用例，0失败不是混合负载吞吐验收。成功／失败语义另有下方实节点
混合回归。网络仍存在限流及背压记录，不能把RPC零拒绝说成网络零丢包。
剔除bootstrap块后，旧两轮及新2的实际批量为`[2,32,32,32,32,32,32,32,30]`，
新1为`[1,32,32,32,32,32,32,32,31]`；每轮合计256笔，不改变冲突条件。

按各版本两轮的总成功数／总窗口合并：`512/225.6175149=2.269327 TPS`，
修改后`512/221.1044538=2.315648 TPS`，仅约**+2.04%**。只有两轮样本，
**不签收稳定性能收益，更不能声称最大端到端瓶颈已经修掉或高TPS目标完成。**
本片实质是可复核基线、已有批树复用及局部等待修正，吞吐交付仍未达成。
旧1最后pool样本111.344秒才见四节点height10已发布，最后成功观察111.971秒，
相差约0.627秒；106.94秒仍有节点等待最后高度提案／收票。因此没有证据把整段
慢速解释为“链已早完成、只有客户端晚查询”。下一次应先细分同路径的提案／
收票等待、重复网络消息、候选验证与发布成本，不凭本片猜测直接放大限流或换GPU。
其中`transaction_status→FinalizedParentViewV1::with_records`仍做workspace／域／
结构marker和认证回执点读，不是每次全历史重放；观察耗时与实际链工作重叠，
不能当成查询CPU耗时相加。seal的8条/来源/秒额度在语义处理前消耗，重复消息
可能挤占，但本片未记录足够排队／处理计数，尚未证明它就是主瓶颈。

冻结原节点SHA256：`9251f2857a4d1c161fd9b0047dd98366ff10ae4b9862f9cac863995373847f97`；
修改后：`b4525b4433f4c305761c34ed41f4f3cc9bc426a5c3bd84f1a64c411c2dd4ec59`。
随仓`aoem/windows/core/bin/aoem_ffi.dll`两版本共用，SHA256为
`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
原／新Release可执行文件本机保留在`target/tps-4de28f3-before/`和
`target/tps-after-bulk-cadence/`；恢复后的`target/release/novovm-node.exe`是新版。

四轮原始报告均为`artifacts/audit/candidate-node-processes/<目录>/transfer-finality-performance.json`，
目录按上表／执行顺序为：

1. `seal-relay-7728-1791036674148700000`
2. `seal-relay-16892-1791036848936613800`
3. `seal-relay-22736-1791037041301633600`
4. `seal-relay-10508-1791037251783675300`

日志`artifacts/recovery/tps-refill-{before,after}-v{1,2}.log`保留；修正前发压试验
保留在`tps-baseline-measure-*`、`tps-bulk-measure-*`及`tps-after-measure-*`，
单独的慢调用诊断保留在`tps-detailed-diagnostic-v1.log`，不并入正式性能统计。
复测入口仍是原`native_candidate_node_cli`中的
`native_seal_main_process::fresh_record_transfers_continuous_backlog_measure_rpc_to_finality`
（Release，`--ignored --exact --nocapture --test-threads=1`），Host许可0、诊断0。
交替旧／新二进制时复用同一个修正后的测试可执行文件，不用不同夹具混比。

### 实际修改及限制

- 原`native_state_tree`复用固定`4de28f3b`中runtime的shared-prefix批构树及反例，
  原节点实际调用，无runtime依赖。重复digest整段按原顺序回退，Delete分隔批次；
  根／最终节点字节、旧节点哈希与入边校验保持。资源上限不放宽，但减少瞬态节点
  后可以成功处理旧算法曾耗尽暂存预算的输入，不能声称资源拒绝轨迹完全相同。
- 诊断中候选artifact重验占service约82%，但每节点service仅18–22秒，不能把
  约110秒总窗都归因于它。原主循环另有约61–64秒轮间间隙，多数约255ms。
  改为本轮只等`250ms.saturating_sub(已用时间)`，不累计deadline、不追赶补跑，
  原每秒限流、drain128、max_ticks计数与RPC处理顺序保留；未新增网络唤醒器。
- 512键组件构建的stage次数4557→1023，替换5279→1023；这是节点编码／哈希
  工作量，不是AOEM或主链TPS，读节点重哈希等其他工作未包含在该计数中。

### 正确性及构建复核

| 本机检查 | 实际结果 |
| --- | --- |
| 原状态树及复用差分反例 | 32通过；1项原有大型容量fixture未执行。含新增迁回的13批树＋5独立旧planner差分测试 |
| 状态记录／delta | 16通过／3原有大型用例忽略；delta反例3通过，错父／根／统计／回执替换删除拒绝 |
| 真实AOEM发布与恢复 | delta四阶段恢复1通过；原fresh Transfer综合恢复1通过，含费用／失败nonce／父绑定及中断恢复，不是新协议A/A/B |
| 轮询／RPC／负载器 | 5个tick预算、12个RPC（含部分请求／有界等待）、7个负载器调度测试通过 |
| 原四进程混合交易 | Host许可0，6笔签名交易5成功／1业务失败；经济oracle、费用守恒、失败nonce、错误签名／链域／重放、查询及重启一致；42.35秒仅夹具总耗时，不作失败TPS |
| 构建与静态检查 | Release原节点构建、workspace/all-targets严格Clippy、fmt通过；原同源多bin提示仍在 |

混合交易证据：`artifacts/audit/candidate-node-processes/seal-relay-7360-1791037432775883100/mixed-transfer-acceptance.json`。
对应日志为`artifacts/recovery/tps-final-mixed-v1.log`；其余本机日志为`tps-bulk-*`、
`tps-cadence-tests-v1.log`、`tps-final-rpc-tests-v1.log`、`tps-refill-tests-v1.log`、
`tps-clippy-v1.log`及`tps-fmt-v2.log`。本机结果不替代新提交的Linux CI或实体多机。

耐久签票日志／pacemaker、新协议决定到原首块／后继／历史父及重启恢复仍须
成套接通；本轮不激活新签票，不宣称真实节点A/A/B恢复已通过。ACK后出票、
未知写结果冻结、原发布完成后才授权下一高度的契约见
[恢复路线R3接合节](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md#r3-第二片新协议决定到原候选的只读接合)。
高性能最终确认、统一异构执行、隐私和PQ目标未因此完成；不删除runtime/legacy
或49项受保护草稿，不另造节点、共识或权威账本。

## R3 第二片：新轮次协议决定与原 AOEM 候选的只读绑定

日期2026-10-03；开发基线`main/origin/main@2da3583cc60088b35bf89e3efe8d1b1e0e7c5094`
加本节所在提交。本机Windows / Rust 1.94.0，原产品workspace、AOEM库和业务规则不变。

**本片新增的是原node库内可调用的候选／决定验证边界，不是新生产签票服务。**
原`novovm-consensus::round_bft`复用该基线已提交runtime的wire、round、collector
及53项原反例；算法、编码、签名域和锁规则不改。裸签票仍为crate私有，未暴露
给node；journal、pacemaker、耐久ACK和状态发布没有迁入，不能称整套协议已接通。
新增`VerifiedDecision`必须核对调用方固定的上下文／本地值、真实提案签名及
同轮同值precommit QC；不反序列化成验证能力，不接受旧V3票改标签。

原`novovm-node::native_round_bft`实际调用该接口：经原workspace加载器重新
读取并验证完整AOEM输入、输出、鉴权、根、回执及完成标记，再绑定新profile和
首块候选statement。statement含完整原块头及执行证据，不含本机namespace、
workspace ID或本地output digest。它不注册／晋升候选、不写链头、不签票，
结果也不能传给现有V3发布器。本片没有新增CLI/RPC或更换默认节点控制器。

| 本机验证 | 实际结果及范围 |
| --- | --- |
| round-BFT规则与决定 | 61/61：53项原规则／反例，加8项决定验证；真签名错域、错轮次／阶段／值、nil、重复／混合票、错误leader和0/1/2-of-4均拒绝 |
| 完整原共识库 | 152/152，包含上一行，不重复累计为独立新增用例 |
| 原AOEM候选实库接线 | 1个聚合用例通过、零忽略、Host许可0；两个独立namespace／workspace执行同一真实签名Transfer得到相同块和值；缺输出、2票、坏签名、错创世、另一真实候选、读回篡改均拒绝；权威链头、创世状态及Host投影不变 |
| 规范值固定 | 为新statement记录固定真实夹具golden；不是旧协议兼容断言，变更编码须升profile／statement版本 |
| 原node封印回归 | 212/212；历史夹具显式Host许可1，仅证明旧行为未破坏，不作为默认旁路许可 |
| 原四进程产品回归 | 1/1、零忽略、Host许可0；6笔签名交易5成功／1业务失败，经济oracle、费用守恒、失败nonce、冲突／重放拒绝、QC、查询及重启一致；全部节点完整块一致 |
| 产品装配 | workspace/all-targets check、严格Clippy及fmt通过；原同源多bin提示保留 |

四进程73.37秒是夹具总耗时，不是出块间隔、交易延迟或TPS。四进程用的是原V3
发布路径，**不计作新协议A/A/B实节点恢复通过**。原迁回A/A/B只在纯状态机层
验证；新协议耐久日志、真实网络分票、写入中断防双签及连续高度尚未验收。

本机日志位于`artifacts/recovery/r3-round-*.log`：core-v1、consensus-all-v1、
candidate-v3、native-seal-v1、four-process-v1、check-v1、clippy-v1和fmt-v1。
candidate-v1首次测试编译缺SHA256 trait导入的失败记录保留，补导入后v2／v3通过。
四进程证据：
`artifacts/audit/candidate-node-processes/seal-relay-22784-1791031520569898900/mixed-transfer-acceptance.json`；
`accepted=true`、`legacy_host_execution="0"`、`production_signoff=false`。
收尾relay连接重置日志保留。CI增加同一规则和真实候选检查；上一笔`2da3583c`
的CI运行37120629021已核实success，本片本机结果不冒充本片推送后的远端结果。

**下一处接合边界：** 当前原首块晋升意图、后继证明、历史父和cold/rooted恢复
硬绑定V3，genesis配置尚无新共识激活profile；因此本片明确拒绝height>1。
下一片要将版本化激活、耐久签票／决定与原D1–D4发布恢复成套适配，不拿新票
调用旧发布、不静默改签名域或清旧日志。runtime journal的transition原子写head、
opening要求已决定即有head、advance按head推进三个假设必须一起处理；原账本
无需重造，但多次跨库写不能声称原子提交。详细契约见[路线R3第二片](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md#r3-第二片新协议决定到原候选的只读接合)。

本片固定集合／Ed25519与现有轮转leader规则，不冒充完整Tendermint证明、PQ、
GPU统一调度、主链TPS或生产签收。runtime已迁纯规则固定为历史来源，不在两边
继续改协议；暂不删除runtime/legacy。全部整合验收后仍必须清理废弃副本、入口和
误导指令，49项受保护草稿逐项SHA256一致，不碰AOEM兄弟仓与实际运行数据。

## R3 首片：现用验证者规则归入原共识模块

日期2026-10-03；基线`main/origin/main@7bfee630fa6f08f1fd932719700697e862d7f5da`
加本节所在提交。本机Windows / Rust 1.94.0，原20个产品成员及随仓AOEM不变。

**本片交付的是实际调用的规则收敛，不是新协议或新增用户交易功能。**
原node的`native_block_seal.rs`中验证者身份、集合构造／排序／去重、权重阈值、
集合验证和哈希移至原`novovm-consensus::native_seal_authority`。node直接重导出
同一类型并删除旧生产实现；现有genesis、签票、QC、存储和查询继续使用该类型。
没有从consensus反向依赖node、账本或runtime controller，没有更换收票／锁／
换轮／决定／发布规则。严格大于2/3、原序列化和哈希域保持，不切回旧ceil阈值。

| 本机验证 | 真实结果和边界 |
| --- | --- |
| 迁接前回归 | 已有迁接前node libtest二进制的封印过滤器211/211；不冒充本轮重新编译旧提交 |
| 新共识契约 | 8/8：固定旧格式JSON、身份／集合哈希、排序、严格阈值、溢出、篡改／重复／弱或非法公钥拒绝；golden由`7bfee630`格式独立计算，不是新实现自产期望 |
| 完整共识库 | 91/91，含上述8项；不是91项新增测试 |
| 迁后原node | 重新编译后封印回归212/212；其中新增1项直接类型身份／读回检查，并另在Host许可0下通过。封印历史夹具用许可1，不作产品旁路开放证明 |
| 真实产品入口 | 现有四原node进程用例1/1、零忽略；显式Host许可0。现有RPC提交6笔签名Transfer，5成功／1业务失败，费用守恒、nonce冲突／重放、错误签名和链域拒绝、查询、QC和重启一致。块交易数为1+6；不是A/A/B不同候选分票场景 |
| 产品装配 | workspace/all-targets check、严格Clippy、fmt全部通过；原同源多bin提示仍在 |

四进程测试实际75.75秒，是测试夹具总耗时，**不是出块时间、交易延迟或TPS**。
本机日志`artifacts/recovery/r3-authority-*.log`；四进程详细证据位于
`artifacts/audit/candidate-node-processes/seal-relay-7708-1791027610463078400/`
及其中记录的节点目录。重复启停时relay连接重置日志保留，不隐藏；测试最终通过。
普通CI增加同一共识契约与node类型接线检查，沿用既有Linux四进程产品回归；
本节本机结果不冒充本次远端CI或Linux实机已通过。

已固定后续[整合及清理契约](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md#2026-10-03-已批准共识有限整合与最终清理)：
耐久决定与D1晋升意图、D2 AOEM发布、D3账本、D4 finality归档分清；不把跨库多次
写当成一个原子事务，不将V3票重标为新两阶段协议。完整内核接入及原节点A/A/B、
写入中断防双签验收尚未完成，本片没有新增通过该故障场景。原发布进程强杀测试
是后续复用对象，不计入本轮已跑结果。高TPS、统一异构、隐私／PQ和生产仍未签收。

本轮移除的是node内已迁接的重复规则，尚未删除runtime/legacy/旧proof目录。
全部整合验收后按替代关系清理废弃实现、入口、脚本和误导指令，不能按月份
删除有用成果。49项既有草稿SHA256逐项一致；未修改AOEM兄弟仓或运行数据。

## R2 第五片：既有 AOEM 统一整数语义接线与原 Transfer 对照

日期2026-10-03；基线`main@15a295b9b44fadd4ba6a4a8f11d1de580585520a`
加本节所在提交。原产品node/exec位置不变；本机Windows / Rust 1.94.0，显式
`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=0`。随仓Windows core FULLMAX仍对应
AOEM source `56e9da15010490ab54435ba6ab1c226f3d739176`，DLL SHA256仍为
`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。

**本片新增的是原exec可调用的统一宽整数执行接口，以及原Transfer规则的真实GPU
对照，不是新的用户交易服务；原RPC的默认交易路径本片未切换。** 回答当前架构：

| 问题 | 当前代码事实 |
| --- | --- |
| 是否走统一门面 | 原真实交易走node→exec→bindings→AOEM；默认Transfer仍是V2 CPU回调图，不是所有业务已编译成AOEM可解释算子 |
| 是否AOEM代数语义 | 原产品已有checked共享credit的局部代数重排；本片另将明确算术、域条件和后置条件编码到既有AOEM整数图，无Host数值解释器。但该Transfer计划仅cfg(test)，不是默认主链完整代数/异构执行已完成 |
| 是否高并发 | 原无冲突分量确有并行；全局费用前缀、逐笔根与回执收尾仍有序。本片149行GPU数值对照不测主链最终确认TPS，也不验证自动CPU/GPU选型 |

复用而非新造内核：`crates/novovm-exec/src/semantic_integer.rs`在已有
`AoemExecSession`内调用`aoem_execute_ops_wire_v1`，使用既有opcode113 /
AOIP0 v5 / APFLOU01 v2 / checked-i1024 / Vulkan-SPIR-V。虽然schema名为
`compute.ai.sgm_infer_v1`，这里执行的是域中立整数程序，不是神经网络或CUDA。
契约见[随仓头文件](../aoem/windows/include/aoem.h)；不新增FFI或NOV专属AOEM规则。
请求和结果检查版本、形状、32 limbs、结果类别与故障字段；u128投影拒绝负数和
截断。独立请求前缀防止同会话串读；执行错误/不完整时不读取旧结果，无CPU回退。

原node的`native_transfer_semantic_tests.rs`只编译测试资产：10个输入、5个输出，
4个顺序域检查、3个算术bank、4组范围/nonce/守恒/收费后置条件。余额、金额、费用
完整保留u128；中间i1024避免将溢出误作环绕。费用拒绝与付费业务失败均输出正确
nonce和业务码；错误nonce/身份或不一致快照属于域拒绝，不冒充失败交易回执。
对照调用原`compute_outcome_v1`，不把Host预计算结果作为GPU输入。输入身份flag
不是密码学鉴权证明；后置条件检查也不是ZK/业务有效性证明。

| 本机验证 | 真实结果和范围 |
| --- | --- |
| 新增纯检查 | exec 4/4 + node 2/2；畸形/截断/错版本/错结果/部分执行拒绝、完整u128投影及固定规则资产。普通命令中两个GPU用例ignored，不计通过 |
| 显式AOEM整数真库 | 1/1、零忽略；同session两次六行，含u128MAX、超过i128、域/反证/后置拒绝和i1024溢出；随后损坏内部图必须报错，不复用旧成功结果 |
| 显式原Transfer真GPU | 1/1、零忽略；同session两次149行，逐字段对照余额、实扣费、nonce和失败类别；含失败优先级/self/full-u128。是数值输入，不是149笔已签名/已finalized用户交易 |
| 普通原有回归 | 原Transfer 76通过/11忽略，exec 49通过/21忽略；包含上述纯检查，不重复累计。GPU用例显式执行结果见前两行 |
| 原Transfer真实CPU并行/归并回归 | 4/4、零忽略；复跑原wave、组件共用session、1024笔checked共享收款、同图归并与失败关闭。证明既有并行分支保持，不是主链TPS |
| 产品装配检查 | workspace/all-targets check、Clippy `-D warnings`、fmt通过；既有同源多bin提示保留 |

真库日志：本机`artifacts/recovery/r2-uniform-integer-real-v*.log`及
`r2-uniform-transfer-real-v2.log`；同149行首次3235.342ms、同session重复137.032ms，
包含逐请求GPU设置/上传/计算/读回；不是纯kernel时间。整数最终为real-v3。
这些只是两次调用样本，**不是稳定基准、出块间隔或主链TPS**；本轮没有重跑四进程
RPC/QC/restart，默认路径未改，其历史证据见上一节。首轮Clippy的取模/布尔简化提示已修，保留v1失败及v2通过
日志。本片CI仅新增无GPU的契约/规则检查；不能把GitHub普通runner绿灯写成GPU实测。

**实际性能边界：** 对照随仓source，AOIP执行分支在每请求调用
`ApflResidentModelDevice::new()`，随后创建operator；其Vulkan runtime/device和
程序资源并未因外层AOEM句柄常驻而跨请求复用。这是此版本/此分支的限制，不能
扩大为“AOEM没有成熟GPU”。默认路径暂不切换；下一步先核对已有常驻表面的适用
契约，并将已验证计划接回原node的访问依赖、完整费用效应和唯一提交边界。
全局日额度/国库归并、冲突重算、父状态根/回执、持久化/QC均不由本次数值对照签收。
没有修改AOEM兄弟仓或SDK；原49项草稿哈希一致。完整R2、高TPS、统一异构调度、
隐私/PQ、Linux GPU/实机、多机公网及生产部署均未因此签收。

## R2 第四片：原 Transfer 同一 AOEM 图内经济归并

日期2026-10-03；基线`main@269c7b9a1d022818e7877bb035f453dcd07555c3`
加本节所在提交。本机Windows / Rust 1.94.0 / 原随仓core AOEM DLL，SHA256仍为
`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
本片不改AOEM仓、SDK、交易/状态/回执格式、共识或收费政策。

**原RPC签名转账现在保留分量并行计算，并在同一AOEM图的最后完成回调中执行
有序费用结算、业务失败处理和受失败影响的后续计算修正。** 不再回到Host逐笔
决定费用结果，也不再为每笔失效预测另提交计算图。全批图成功和结果承诺匹配后，
原node才应用隔离候选效应，走原持久化、父/轮次验证、3/4 QC及发布路径。
这不是新API、另一条转账链，亦不把排队/计算/落盘当作最终确认。

复用原`native_transfer_executor`的冲突分量/checked credit reduction、统一收费
与nonce函数，以及已有最后回调归并模式；新增`native_transfer_business`只捕获
本批涉及的NOV账户、nonce和有界费用输入，不复制完整Store/历史，不携带数据库
或权威写句柄。费用政策、纯报价在提交前固定，动态日限额按真实有序前缀更新。
输出带前态检查的业务效应；费用日志使用最多一条追加的紧凑delta、结果采用流式
摘要，不为每笔结果保留一份512条日志。Host应用前验证余额/nonce/费用前缀及日志
长度/序号；原finalizer错误顺序保持，失败不静默跳过。

**所有权与并发边界：** AOEM回调计算余额、费用、失败结果与下一nonce，但nonce
安装、逐笔root/receipt收尾仍由原finalizer执行。全局费用归并仍按协议顺序进行，
未宣称其已经并行化；无冲突分量仍并行。失败后的共享收款预测目前可在同一回调
重算，不宣称已迁完runtime的免重算credit rebasing。V2 CPU回调和局部承诺不是
统一CPU/GPU完成或业务有效性ZK证明；没有更换原三树/根、发布和共识协议。

| 本机检查 | 真实结果和范围 |
| --- | --- |
| 原四个真实node进程、RPC、QC与实际重启，Host许可0 | 1/1；6笔真实签名交易5成功/1余额不足，费用守恒、失败消耗nonce、串行经济oracle及四节点块/回执/余额一致；错签名、错链域、Execute准入拒绝、nonce冲突和重放检查通过；原3/4门槛不降 |
| 显式真实AOEM回归，Host许可0 | 12/12、零忽略；同图回调线程/单次归并、分量或归并失败关闭、会话复用、1024笔共享收款原并行检查、typed/fullencode/逐笔前缀等价；新增容量拒绝后较小费用仍可成功的同付款方/共享收款反例 |
| 原纯Transfer精确父/候选/恢复夹具，Host许可0 | 1/1；保留原候选持久化、恢复、错父与34候选回收检查，257.39秒是整项夹具耗时，不是出块时间 |
| native_transfer普通回归，Host许可0 | 74通过、10忽略；新增有界捕获、账户缺失与零值、前缀篡改拒绝、紧凑日志及流式摘要；忽略项不计通过，真库检查见显式真实AOEM行 |
| 既有候选经济 / 费用兼容回归 | 5/5及53通过/2忽略；历史初始化显式Host许可1，只用于原规则对照，不替代许可0的产品验收；与其他过滤器重叠，不累计成独立用例总数 |
| 原封印、轮次与Overlay回归，Host许可0 | 211/211；无降低票数、删除反例或放宽默认Host许可 |
| 产品构建 | 全workspace/all-targets check、Clippy `-D warnings`、fmt通过；原同源多bin提示保留 |

四进程报告：本机
`artifacts/audit/candidate-node-processes/seal-relay-19496-1791007021161842000/mixed-transfer-acceptance.json`；
`accepted=true`、`legacy_host_execution="0"`，四节点worker均submitted/completed=1、
failed=0；重启无历史重算。`parallel_overlap_measured=false`、`production_signoff=false`。
该夹具76.07秒包含启动、网络、检查和重启，不用来计算TPS。收尾时有连接关闭日志，
不称日志全无异常。测试日志`artifacts/recovery/r2-business-*-v3.log`，fmt最终为v4。
首次real-v1两项失败来自新增u128容量夹具误放旧JSON根格式；移到原RecordTree
等价夹具后12/12通过，未放宽协议或原断言。早期失败日志保留。CI新增显式原
Transfer executor/dispatch真库检查；本机通过不替代推送后该提交的Linux/Windows CI。

原49项草稿逐文件SHA256一致，runtime/legacy及AOEM源码未修改。未签收完整R2、
主链TPS、GPU/ZK、隐私/PQ、Linux实机、多机公网或生产部署。下一处继续原真实
node路径的逐笔根/回执收尾与批量成本，先定位测量再接回已有批处理成果；不另造
节点、执行器或权威账本，不再扩大清理工程。

## R2 第三片：陈旧未注册候选回收与启动恢复

日期2026-10-03；基线`main@e64a52477c1df282d0921c0d9c59b227b8f45796`
加本节所在提交。本机Windows / Rust 1.94.0 / 原随仓core AOEM DLL，SHA256仍为
`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
只修改原node的候选生命周期及配套测试，不改AOEM、业务规则或共识协议。

**新增能力：原节点不再因反复出现“已落盘、换轮后未注册”的候选而持续占满32槽。**
陈旧完成进入同一常驻AOEM storage owner的清理阶段，交易池不删除；启动在发布
恢复完成后、计算worker开启前清理完整孤儿输出，并续做中断的私有快照回收。
未完成的执行reservation仍走原恢复流程；本片不擅自判废不完整执行或旧abort。

复用原workspace目录、artifact读回、AOEM原子写集合与ledger注册边界。持锁顺序
保持workspace→authority→ledger，验证当前finalized父/唯一AOEM head，并在注册
写锁内按workspace ID检查完整候选集合；任何已注册分叉都保留，不因没选中就删除。
正常产品签票必须先注册，因此保护已签票候选；这不是外部签名不存在的证明。
发布证据与注册关系交叉检查，异常数据拒绝。临时`q/NCU1`日志绑定scope、输入/
输出描述符及槽并检测损坏；私有c/o/v/e/r清理并读回后，slot与q在**同一原子写集合**
释放，不把整张graph误称单一事务。永久g/abort策略不变，同一合法plan可以重建。
共享内容寻址记录/状态树、h发布证据、账本/QC及用户pending均不清理。

| 本机检查 | 真实结果和范围 |
| --- | --- |
| 原纯Transfer真库夹具，Host许可0 | 1/1；34个不同候选实际计算/落盘/回收，catalog不增长；相同plan实际重算落盘两次；3个checkpoint返回错误后重开恢复；错父/创世/损坏日志拒绝；原5笔后继经济与读回保持一致，登记后禁止回收 |
| 原连续record-genesis夹具 | 1/1；该历史fixture初始化显式Host许可1。真实worker陈旧完成进入owner回收，3笔pending、余额、出站票保持；同plan重建一致；lifecycle重新打开回收完整孤儿及部分清理日志，已注册未finalized候选保留；后续原链正常推进，真实签票后也不可回收 |
| 四个真实原node进程、原RPC和重启，Host许可0 | 1/1；6笔真实签名交易5成功/1余额不足，费用/nonce/资产守恒、完整块/BFT证明和重启回执/余额一致，原3/4门槛不降；不是4台设备或吞吐验收 |
| journal codec / 原账本 / Host默认限制 / RPC / pool | 1/1、40/40、5/5、12/12、10/10；codec覆盖合法范围字段位翻转、截断、加字节与跨scope；检查未跳过 |
| 原封印、轮次与Overlay回归 | 211/211，含6个既有worker检查；没有降低QC或取消原检查 |
| 产品构建 | 全workspace/all-targets check与Clippy `-D warnings`、fmt通过；既有同源多bin提示保留 |

真实路径：`原node后台候选完成 → 原父/轮次fence拒绝 → 同一AOEM owner清理stage →`
`原workspace/authority/ledger锁内验证 → 私有快照回收 → 原RPC/候选调度继续`。
清理期间复用既有storage_busy/有界RPC等待，不在主线程拿锁等同一owner；启动清理
同样送owner执行，不嵌套主线程已持有的remote scope。新增状态计数
`candidate_reclaimed_workspaces`仅反映本进程实际完成的回收，不是最终确认计数。

证据：本机`artifacts/recovery/r2-reclaim-*`；最终纯Transfer为`transfer-v2.log`，
连续夹具为`lifecycle-v1.log`，四进程为`product-rpc.log`。四进程机器可读报告：
`artifacts/audit/candidate-node-processes/seal-relay-24644-1791004964826289000/mixed-transfer-acceptance.json`，
`accepted=true`、`legacy_host_execution="0"`、`production_signoff=false`。
首次Clippy仅manual_contains提示，修正后v2通过；保留早期日志，不覆盖证据。
本轮GitHub CI在推送后另核，不把本机通过或前提交CI状态写成新提交全绿。

边界：34次存储循环不是34次网络换轮；陈旧轮次由原fixture注入，中断为checkpoint
返回错误/进程内重开，不冒充真实断电或强杀。四进程测试实际重启但不注入清理中断。
本片解决候选**槽和私有快照**泄漏，不是共享状态树GC；没有性能测量，不签收TPS、
统一CPU/GPU、ZK、隐私/PQ、Linux实机、多机公网或生产部署。既有49项草稿保留。
下一处回到完整Transfer业务效应与AOEM批执行/确定性归并及批量成本，不再扩清理工程。

## R2 第二片：原节点常驻后台候选流水线

日期 2026-10-03；代码基线 `main@09486e3e7326202837e22b401e01be6da4e90bc2`
加本节所在提交。本机 Windows / Rust 1.94.0 / 随仓 core AOEM，DLL SHA256
仍为 `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
本片未修改 AOEM 仓库或 DLL，未恢复目录或建立另一条产品链。

**新增用户能力：原 fresh-chain 节点计算后继候选时，控制循环可以继续接收交易、
查询已 finalized 的回执/余额，并处理真实签名轮次消息。** 实际四进程测试仍用
现有 `nov_sendRawTransaction`、`nov_getTransactionStatus`、`nov_getAssetBalance`，
不是独立转账程序。每节点实际完成1个后台候选，6笔用户交易5成功/1业务失败，
手续费/nonce/余额守恒与原串行经济 oracle 一致，3/4 QC门槛未降低；重启后结果
一致，worker重新常驻但不重算历史完成结果。`queued`、计算完成、落盘均不是最终确认。

复用来源及接线：选择性迁接受保护草稿的 storage owner、owned capture/run/finish、
单候选 worker 和 lifecycle/RPC 接线；原49项草稿逐文件哈希仍一致。正式实现位于
`crates/novovm-node` / `crates/novovm-exec`，沿用原 Transfer 分量、waves、三树格式、
`persist_delta`、收费和签票/发布；不迁入 packet、批读等无关优化，不链接 legacy。
一个 worker 接收一个**批候选**，不是每次只执行一笔；批内原 AOEM 计算并发保留。

实际路径：`真实node lifecycle → owned输入捕获 → 常驻候选worker / exec计算会话 →`
`原AOEM V2 compute → 主循环父/轮次/输出核对 → 常驻AOEM存储owner执行原V3写入 →`
`主循环再次父/轮次核对 → 原候选注册 / BFT签票 / 发布 → finalized查询`。
AOEM非Send句柄只在所属线程创建和释放；新捕获不占持久候选槽，异步完成不能直接签票。
错误签名/nonce/不支持的新Execute为明确准入拒绝，父状态读取/存储故障仍上报，
不能吞成坏交易。执行许可不进入历史共享鉴权，既有已完成Execute仍可读取/幂等恢复。
固定已验证证明可继续转发，但明确仅为 `historical_verified_at_open`，不是实时状态授权；
新签票、重开、晋升仍验证现有真实状态。

| 本机检查 | 结果与范围 |
| --- | --- |
| 原四真实进程 Transfer RPC、QC、经济及重启 | 1/1，Host许可0；四节点worker各submitted/completed=1、failed=0，重启均未重算；不是TPS/多机验收 |
| 原连续record-genesis真实AOEM/网络/恢复夹具 | 1/1（该历史fixture初始化显式Host许可1，不替代上一行许可0验收）；计算暂停时真实RPC/余额、WSS交易与签名Timeout处理继续，发送方持久票重开一致；34个丢弃capture不占槽、错subject拒绝、陈旧计算/落盘均不注册不签票、3笔pool重开保留；后续原链推进回归仍通过 |
| 真实AOEM纯Transfer、精确父、恢复及分阶段执行 | 1/1，Host许可0；包含无锁计算、输入篡改/abort/retire拒绝、完成重用，以及新Execute/混合capture拒绝且无slot/权威变化 |
| 常驻owner / scope / compute | 9/9、6/6、14/14，显式include-ignored执行真SDK用例；普通exec另45通过/20ignored，不将ignored计通过 |
| worker / RPC / pool / typed auth / Host限制 | 6/6、12/12、10/10、3/3、5/5；另真实历史Execute禁算但已完成可读回归1/1 |
| 原耐久账本 / 封印和Overlay | 40/40、211/211（后者含上述6个worker测试，不重复累计） |
| 产品构建 | 全workspace/all-targets check、Clippy `-D warnings`、fmt通过；原同源多bin提示保留 |

四进程证据：本机 `artifacts/audit/candidate-node-processes/`
`seal-relay-2560-1791000619042896800/mixed-transfer-acceptance.json` 及同目录回执/准入。
本地日志 `artifacts/recovery/r2-async-*.log`。初次Host guard回归揭示共享鉴权许可过宽，
修正后 `r2-async-final-candidate_workspace_execution_host_barrier_default_guard.log` 通过；
初次liveness夹具尾部误用普通fresh账本open，原fence正确拒绝，改用现有verified父作用域后
`r2-async-real-lifecycle-v2.log` 完整通过。Timeout仅一张真票，不冒充QC；陈旧轮次为
测试内注入，暂停用于响应性检查，不作为AOEM并行速度或真实换轮负载证据。
失败日志保留，不放松生产检查。CI沿用原真实四进程门，并加入owner/scope/worker/auth检查。
前轮Linux CI `37093118760` 的历史子进程丢失显式许可问题，已由 `09486e3e` 单独修复
并推送，本机33/33回归通过；本片本机通过不等于远端CI已通过。

**仍有限制，不签收完整R2或生产：** 输入捕获、重验、注册仍同步；落盘阶段状态RPC
保留原数量/字节/3秒期限等待，超时需客户端重试，不能保证即时响应。该阶段只继续
纯chainStatus、不可变证明relay和内存gossip，pacemaker状态处理延后。owner队列8项、
候选1批不是统一全局字节预算；Drop等待正常worker退出，不声称可强制取消卡住的FFI。
若候选已落盘才因过期轮次拒绝注册，会留下未注册输出；现有32槽回收不覆盖这类高于
finalized高度的孤立结果，反复发生可能耗尽槽。下一片需精准接回可恢复的未注册候选
回收，不能用增大槽数/降低最终性门槛掩盖；随后继续原路径完整业务效应及批量成本整改。
费用/nonce/树收尾仍有Host计算，V2 CPU回调不等于统一CPU/GPU语义；本片未签收
主链TPS、GPU/ZK、隐私/PQ、Linux实机、多机公网或nightly长跑。

## R2 首片：原节点签名转账、最终余额查询与恢复

日期 2026-10-03；代码基线 `main@7b10c7f3adf7b1e48fd39435044d793077911146`
加本节所在的纠错提交。本机 Windows / Rust 1.94.0 / 随仓 Windows core AOEM
DLL，真实四个 `novovm-node` 进程；不是四台设备、Linux 或生产验收。R1 已收口，
本轮没有搬目录、重启全量备份或等待另一设备。
实际 `aoem/windows/core/bin/aoem_ffi.dll` SHA256：
`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`；本片未更换库。

**用户现在可在原 fresh-chain 节点提交签名 NOV Transfer，等正常共识最终确认，
查询回执及绑定 finalized 块的 NOV 余额，重启后结果保持一致。** 提交、计算、
持久化和 QC 原基础已有，本片不将整个闭环称作新写的算法。实际新增是：

- 接回既有方法 `nov_getAssetBalance` 的 finalized NOV 查询；直接读原 AOEM
  根绑定记录，不读旧 Host JSON 投影。余额为十进制字符串，响应带
  `found`、`finalized_tip_height`、`block_hash`、`state_root`。缺失账户与读取
  损坏区分，后者返回错误，不把错误显示为零余额。仅公开 NOV，不冒充隐私资产查询。
- 有效签名但默认不可执行的 Execute 在共享池准入处明确拒绝，避免入池后
  碰到原 Host guard 导致生命周期停止。RPC 参数不能开许可；旧测试池若已含
  Execute，默认恢复失败且保留记录，不自动删数据。Transfer 仍正常准入。
- 原存储会话已常驻；计算会话现在也由真实节点生命周期持有，在配置不变时
  跨批复用。复用既有 compute 执行/取消/poison 规则，不另造调度器；失败不会
  通过重新 open 隐式清除，未 drain 的任务仍按原契约保留 owner。

实际路径：`nov_sendRawTransaction → fresh pool → node lifecycle/proposer →`
`candidate workspace / native_transfer_executor → novovm-exec → aoem-bindings →`
`AOEM V2 compute → 原 V3 AOEM 持久化 → 原 QC/发布 → finalized 查询/重启恢复`。
复用恢复后的 Transfer 分量与 waves、精确父状态、鉴权、收费/失败、nonce、状态树、
共识及经济 oracle，没有接入第二套 runtime 权威账本。后续 runtime/proof 成果及
49 项原草稿未改；49 项逐文件 SHA256 对照原清单全部一致。

接口用法（沿用现有 fresh genesis/seal/pool 配置及 `NOVOVM_NATIVE_FRESH_RPC_BIND`）：

| 方法 | params | 结果含义 |
| --- | --- | --- |
| `nov_sendRawTransaction` | `["<真实签名交易编码的十六进制>"]` | queued/已接收不是执行成功或最终确认 |
| `nov_getTransactionStatus` | `["<交易哈希>"]` | finalized 后检查 receipt 的业务成功/失败；失败交易也可被最终确认 |
| `nov_getAssetBalance` | `{"account":"0x<20或32字节地址>","asset":"NOV"}` | 指定高度的最终余额快照，非 pending 余额；不能作为后继发布授权 |

真实节点均显式 `NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=0`。复用原四进程测试，
由一个节点接收六笔真实签名交易，网络传播后四节点分别查询：5 笔业务成功，
1 笔余额不足失败，失败收取 46 单位手续费并消耗 nonce，后续同账户交易仍成功。
2/4 无 QC、3/4 真实签名 QC/Decision V3、第4节点追赶，未降低票数要求。
两持久块分别含 1 笔 bootstrap 和 6 笔用户交易；本次 tip 为高度 2。
测试账户 A/B/C/D/bootstrap 最终余额依次为 999569/159/999944/11/1，
加国库手续费 316 等于创世 2000000；四节点重启前后全状态经济 oracle 各自相等。
这些是夹具最小单位，不是主网余额、费率变更或性能数据。

| 本机检查 | 实际结果 |
| --- | --- |
| 原四进程 mixed Transfer RPC 用例 | 1/1；坏签名、错链、有效 Execute 均明确拒绝且不入池；pending 余额不变、nonce 冲突/重放、失败收费、最终回执和余额、重启全部核对 |
| 纯 Transfer 根绑定/恢复真实 AOEM 用例 | 1/1；新增6项陈旧/错误父状态反例均在具体父授权检查拒绝，无 head/catalog/finality/Host投影变化；正确后继继续成功 |
| finalized 读回完整性 | 上述真实用例覆盖余额缺块、物理/共识叶不一致；另1项 typed NOV单测含u128上界/坏类型，均通过 |
| 常驻 compute | 14/14，含显式执行的真实SDK两批复用、配置漂移/失败poison、原取消/未drain/回调边界；无忽略项冒充通过 |
| pool / fresh RPC | 10/10、10/10，含默认许可0与字段无法授权、历史记录保全及查询参数检查 |
| exec普通回归 / Host限制回归 | 43项 / 5项通过；exec普通命令另13项ignored不计通过，其中compute相关真库已由上列显式命令另跑；未称exec所有opt-in均执行 |
| 产品构建与格式 | `cargo check --workspace --all-targets --locked`、fmt、全workspace/all-targets Clippy `-D warnings`通过；未删模块 |

原四进程证据在本机 `artifacts/audit/candidate-node-processes/`
`seal-relay-23892-1790997228580771700/mixed-transfer-*.json` 及对应进程目录。
本地日志 `artifacts/recovery/r2-product-rpc-v1.log`、`r2-rooted-parent-recovery-tests-v2.log`、
`r2-compute-tests.log`、`r2-pool-tests.log`、`r2-balance-unit-tests.log`、
`r2-rpc-tests.log`、`r2-workspace-check.log`、`r2-workspace-clippy.log`。
首次 integration 构建因缓存中 `librocksdb_sys` 的 rlib 不可用失败，单独重建
该依赖并复验原链接目标后通过；未改业务或删除缓存，初始日志保留为
`r2-baseline-rpc.log`。父状态复跑第一次误用 `--ignored` 导致0项，不计通过；
去掉该过滤后实际1/1通过，原日志保留。CI已加入原入口测试与证据上传，
提交前本地通过不等于本轮远端 CI 已通过。

**仍未完成：** 控制循环仍同步准备候选；费用、nonce及回执/树收尾还有Host计算，
V2不透明CPU回调不是完整统一CPU/GPU语义。此片不签收完整R2、主链TPS、出块间隔、
ZK、隐私/PQ或生产部署。Execute、旧账户政策写入和旧立即执行旁路仍默认受限，
未连带关闭原EVM/查询/网络装配，但本片也未重跑其全部业务验收。
下一片在同一node/exec接入已保留的常驻有界后台候选流水线，并迁接完整业务效应，
保持精确父绑定及唯一提交/发布；不要再重写节点或用独立压测程序替代产品。

## 原产品装配恢复验收

日期 2026-10-03。用户批准[整仓清单](NOVOVM_ISOLATION_UNDO_CHECKLIST.md)后，
基于 `face41e3` 在仓内独立验证 worktree 恢复 `ee802713` 原件，再以新增
纠错提交接回 main；未新建开发分支。以下为本机 Windows / Rust 1.94.0
结果，不是 Linux、多机、公网、nightly 或生产验收。

原 1,292 路径全部有原位对应，六目录和配套共 750 原件完整恢复。根 Cargo
恢复 20 显式产品成员，含自动纳入的 EVM core 实际 21；真实 node、exec、
bindings、EVM、共识、网络、Relay、工具及配置重新参与构建。runtime 原实现
与原锁保存在独立 workspace，避免同名网络包冲突；原 proof、归档及49草稿
保留，没有混进草稿，也没有新增权威账本。保留后来的 Release 溢出检查。

| 检查 | 本机真实结果及范围 |
| --- | --- |
| 产品 `cargo check --workspace --all-targets --locked` | 通过；原模块未删减 |
| 产品 fmt / 全 workspace all-targets Clippy `-D warnings` | 通过；Cargo 原有同源文件四个 bin target 提示仍在，不是新增 Rust lint |
| `novovm-node` 实际 binary 构建与入口 | 构建通过；`native_protocol_config_commitment` 模式实际执行成功，未打开生产网络或账本 |
| Host 默认限制 | 5 项无 SDK 子进程反例通过；另1项真实候选 Execute/混合批在预提交前拒绝通过 |
| 原 AOEM Transfer 分支 | 显式 `NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=0` 的原 fresh transfer 执行、恢复、最终性用例1/1通过；不是完整主网或 TPS 测试 |
| bindings / exec | 6 / 41 项通过；exec另11项 opt-in ignored，不将其计作真实库全部签收 |
| 独立 Relay | 48/48通过，含新增接回的Windows共享I/O测试 |
| 原耐久账本 | 默认Host许可为0，40/40通过，含恢复、篡改拒绝和索引检查 |
| 原封印及Overlay回归 | 默认Host许可为0，205/205通过，含本机WSS、换主和重启；不是多台实体机测试 |
| novovmctl | 修正旧测试证书后24单测+9集成通过；真实签名一致性另1/1通过 |
| 恢复的PowerShell检查 | 构建输出解析/陈旧拒绝通过，聚合21项通过；后者只使用合成报告，不是网络实测 |
| 两套workspace边界 | 产品21、保留组件3，均无归档依赖或未经批准的本地跨工作区引用 |
| 保留runtime普通测试 | 592单测+8文档测试通过；75 opt-in项未在该命令执行，不充当全绿验收 |
| 保留runtime真实AOEM | 显式运行批计算、常驻候选流水线、跨进程耐久恢复3项，3/3通过 |
| 保留runtime fmt / strict Clippy | 两项通过，使用显式 `runtime/Cargo.toml` |

首次失败未抹除：

- 完整编译发现原独立 Relay 漏接 node 已有 `product_relay_io`；补模块声明及
  已有 mio/socket2 依赖边，无网络算法重写、无版本升级。首轮并行编译还读到
  尚未写完的新增测试 include，文件完成后全目标复验通过。
- novovmctl 旧证书目标承诺仍为 `f20c3b51…`，而原件代码已经是 `365bfcb1…`。
  单测独跑也失败，非并发污染；后来的协议字段增加未同步旧夹具。使用原有
  `print_current_native_nonce_upgrade_authorization_cli_fixture` 测试入口完整
  重签TEST-key证书，只更新一个JSON。生产校验及错误pin/环境漂移/伪造QC
  拒绝断言全部保留。签名一致性与完整ctl回归通过后才接受。

运行日志在本机 `artifacts/recovery/` 下的 `product-check-*`、`host-guard-tests`、
`candidate-host-barrier-tests`、`product-real-aoem-transfer-no-host-permit`、
`product-relay-tests`、`product-facade-control-tests`、`product-clippy`、
`node-build`、`node-entry-smoke`、`preserved-runtime-*` 和
`preserved-real-aoem-tests` 等 `.log`；原首次失败日志保留。

接回本机 main 工作区后再次通过：全workspace/all-targets locked编译、产品fmt、
ctl 24+9测试、默认限制5测试及真实node binary构建/入口执行。日志为
`main-product-check`、`main-ctl-tests`、`main-host-guard-tests`、`main-node-build`、
`main-node-entry-smoke`；未把仍保留的49项草稿加入提交或用于候选验收。

已知限制：旧Host立即/批/tick/mutator及混合Execute新执行默认拒绝；显式
兼容许可只用于历史对照，CI逐step设置，生产启动不默认设置。默认raw RPC
立即执行因此拒绝，须显式pending-only；旧统一账户政策写操作也受此限制。
旧已完成产物读回仍保留，未改已有数据。未解决所有执行所有权问题，未接完
runtime新成果、统一CPU/GPU、Execute、隐私/PQ或最终确认性能。
未运行完整旧主门、全部旧业务回归、Linux安装、多机、nightly；恢复工作流和
本机通过不能据此称远端CI全绿。下一步按路线R2在原node/exec中继续纠偏。

## 架构审计与恢复路线文档交付

日期 2026-10-03。本轮代码审计基线 `main@eaf4f37`，远端 main 实查一致；
Windows 本机只读核查历史提交、目录和已提交源码，随后仅修改文档及 AGENTS。
详细结论、时间线、目录清单和后续 R1 至 R4 出口见
[架构纠偏审计与开发路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md)。

- `98eebff1` 是 6 月 21 日记录并要求纠正 Host 执行错位的提交，不是错误起因。
  7 月 31 日曾扩大执行所有权签收口径，8 月 4 日修正文案不等于补齐执行。
- `161f64d2` 整体隔离超出用户最新明确的范围；保留/恢复原 node 总控、exec
  门面与整个产品装配，迁接有价值的新实现，而非整体回滚或另造简化链。
- 750 个归档受控文件与搬迁前 `ee802713` 的 Git blob 完全相同，当前基线
  同样一致；没有发现旧受控路径原地与对应归档同时缺失。此结论不覆盖
  未跟踪文件的历史完整性。现有 49 项源码草稿受保护，不随文档提交接受。
- 本轮**未恢复目录、修改 Cargo/CI、构建、重跑业务/性能测试或部署**。
  实际根构建仍为三库；文档交付不增加任何产品完成声明。下一代码任务
  是 R1 原产品装配恢复，须先重新核对双机状态并登记范围。
- 文档复核：新路线的 6 个相对文件链接均存在；对照修改前 SHA256，原 49 项
  源码草稿逐文件字节未变。两项独立审阅复核历史归因、恢复范围及指令冲突，
  补明确冷恢复完整性检查必须保留；这些检查不替代代码构建或产品测试。

下方原切片的实验、失败及验收范围保持原意；其中“下一处/必须在新目录”
等任务指令属于历史，不能覆盖本次用户确认的恢复路线。

## 设备 A：guest曲线电路与严格验签真实证明（2026-10-03，本地验收）

基线`b027e4043c1aab1174da4298ae3f73457a419fc4`与远端一致；其CI
`37074245213`双平台completed/success，Windows14分50秒、Linux8分59秒。
本轮在干净快照`target/runtime-rebuild/candidate-b027e40-curve-v1/`开发，
不构建/提交根49草稿。仅guest固定官方curve25519-dalek 4.1.3
`385adda1fa3b66d9aaa8b8fcc99ddc324d33ba32`与必需crypto-bigint 0.5.5
`3ab63a6f1048833f7047d5a50532e4a4cc789384`。Ed25519仍registry2.2.0、
原verify_strict不动；主guest源码、根/外层锁、业务、原生节点、AOEM仓与
FULLMAX资产不改。guest实际bits32/serial通过目标条件选RISC0域/标量后端，
不靠改签名政策或启legacy_compatibility提速。

同前两轮Linux/WSL、Core Ultra 9 275HX、Release CPU证明sidecar（下节
SHA256 `cfdd4469...`）、Composite/poseidon2，无CUDA、无DEV生成。相同
单笔完整NOV input/152B journal不变；计时期间不并行构建。真实生成
**242.870秒、1,674,684B**，初始独立验证79ms；最终probe再验82ms，14反例
仍拒绝并再次接受原件。sha-only旧image与新image双向拒绝错配receipt，
各自原配对仍成功；DEV生成返回-4且无输出文件。前轮为350.455秒/2,250,432B；
这是有限功能样本，**不是稳定加速比例、主链TPS、出块间隔或批量证明容量**。

- 主guest image：`[1144495143,3612026493,4005071765,1634377336,1879036010,4171784104,1710951644,1407711336]`
- 主program：907896B，SHA256 `316be9300d357cfad9372a13118f048e0830c1b67b7cc72fb36baf653a9d1515`
- 首次producer SHA256：`5ac3e76066a617bf4230f170a099e4a45f13d08f483164389c7b5e33759b066f`
- 最终probe SHA256：`fd333f3ce7cd811d829fd83c180b9d8d6832b447a43375a46a6c1c4cfb6cac10`
- 主receipt SHA256：`0ca027f95f9d548323e828d9387f23e1a06f01a0e3d7ac7d668be73ccec32271`

另加独立`novovm-auth-conformance-guest`，只调用原authenticate_transfer_v3，
不含余额/见证执行，避免后续状态失败冒充验签拒绝。原生未patch实现先检查
固定公开夹具的规范编码、链/账户/nonce前置条件及准确错误层；guest实际
输出6接受/10拒绝，journal绑定完整输入摘要与顺序，没有输入预期结果。
包括多钥/多消息/20与32字节账户、改签名/金额、S=L及S+L、非规范/不可解压
R、不可解压A、identity A/R，以及A=identity/R=B/S=1的弱A-only反例。
两个弱A例先确认ordinary可通过、strict须拒绝；不是随机坏签名假门。

该专项真实生成/验证成功，356.549秒、2,250,244B；生产者退出后独立进程
在DEV=1下验证108ms，错输出/image拒绝后原正例仍成功。其image/journal
独立，**不能当NOV状态转换证明或主链授权**。guest证明成功位，详细错误
阶段来自native预检；R单独门、mixed-torsion正例、全部标量/点边界未穷尽，
不称完整密码审计。expected文件须来自受信native producer，不能取自receipt。

- 诊断image：`[3788971418,596289292,2523777155,577641825,151983570,1374351363,3650584798,2381207193]`
- 诊断program：452932B，SHA256 `06ebcb7a9468ebbed149b9cbe82a3828ae2724bbfab14fec6416cd98b079b10`
- 2914B诊断input SHA256：`9b87d00b8233ef0a69fe1863156301597582eaa291b3ccd52327f3dd7c25bc6e`
- 58B诊断journal SHA256：`815d2c6078bdecc65ad2303673f75b3d490462787479642dfa588c6497dcaed4`
- 诊断receipt SHA256：`86af7ad10470c31d4dbf3737f3b993be8a12fad03fd0581a324290b541afb361`

真实两个guest均locked构建、不skip；加入诊断后主guest image/字节完全不变，
最终probe成功验证初始producer的receipt。fmt/diff检查通过；独立lint target
用SKIP只做probe host all-targets strict Clippy通过，不冒充guest构建/证明。
本轮没有重跑全工作区、Windows真实证明或四节点负载。官方电路没有严格
恒时保证，不外推秘密签名/隐私；经典Ed25519也不是PQ。Composite执行长度
泄漏、SHA复制累计heap、1024上限未实测等上一切片边界继续保留。

日志位于`target/runtime-rebuild/proof-curve-*`：`build-initial`、`build-final`、
`build-final-v2`、`guest-features`、`prove`、`verify-initial`、`verify-final`、
`old-image-reject`、`new-image-reject`、`dev-reject`、`positive-recheck`、
`auth-conformance`和`probe-lint-only`，均为`.log`。主receipt保存为原夹具目录
`receipt-curve.bin`，诊断原件在`nov-auth-conformance-curve-v1/`；复现见
[proof说明](../runtime/proofs/nov-transfer/README.md)。A下一步先实测多笔关系
固定开销与内存，再接同候选显式后台证明/原AOEM耐久恢复；不占执行批槽、
不暗改最终性。S4主链、隐私/PQ/Execute、实体多机/长跑/部署仍未完成。

## 设备 A：同一完整业务关系的 SHA-256 证明加速（2026-10-03，本地验收）

基线`77bb9619e99efa77c2b01dd3283b074aebf70081`与远端一致，其CI
`37070716271`双平台completed/success；CI不构建guest或执行真实证明。
本刀只有独立guest manifest/lock变更及交接文档，sha2版本仍0.10.9，官方
补丁精确revision为`8631fabdea7bdffa97b11868e04e73491d8e5bcf`。实际guest
锁和feature树核对Git源，未启用force-soft；根/外层probe锁及所有业务、
guest源码不改。AOEM仓库、原49草稿与随包FULLMAX不改，不创建分支。

干净HEAD快照`target/runtime-rebuild/candidate-77bb961-sha256-v1/`加上述
两文件，Linux/WSL正常locked构建、无SKIP。CPU Intel Core Ultra 9 275HX，
本轮证明期间没有并行构建；仍为同一Release AOEM 2.3.2 CPU sidecar、
Composite/poseidon2、无CUDA。副库SHA256沿用下节`cfdd4469...`；未重新
生成交易或期待journal，直接使用前轮同一单笔真实签名/扣费/nonce夹具。

| 同输入功能样本 | 前轮未接电路 | 本轮接电路 |
| --- | --- | --- |
| 生成调用（含后端验证/序列化） | 717.393秒 | 350.455秒 |
| 生产者退出后独立验证 | 212ms | 102ms |
| receipt字节 | 4,768,434 | 2,250,432 |

公开152B journal完全相同。14项密码反例全部拒绝、之后原正例再通过；
另外真实旧image拒绝新receipt、新image拒绝旧receipt，两边原配对仍通过。
DEV=1生成返回-4且没有输出文件；DEV=1 verifier依然接受真实proof。
这只是两次功能样本，旧轮有同时lint；**不是配对独占基准、稳定加速承诺、
批量/GPU证明容量、出块间隔或主链TPS**。350秒仍远不足以签收证明吞吐。

- 新image：`[3524215403,1941157418,2598198666,397176697,3779815785,1105963016,3655555763,2967265936]`
- 新组合program：918208B，SHA256 `11df0ed068b1f36069d47571eea98564be616be39994819aa3a19a0c92edf01c`
- 新probe SHA256：`99cb846445f5d39ef1b5fe03d4a617f4961b356d25ad692e8915b33da95d99f5`
- 新receipt SHA256：`5ff2b39687be2bbac4979153fe24726501e261441087c0fa9ffe37a7be8a67a0`
- 原input/journal SHA256仍为下节`78bfa362...` / `6873f4f3...`；不从receipt取信任pin。

补充检查：Linux原生共享关系12测试通过，proof/guest fmt及diff检查通过；
未重跑完整工作区、Windows真实证明或四节点负载。首次从guest workspace
给外部Host包传feature被Cargo拒绝，改用干净根workspace后才计12门；该
native-free lib-test仍有原有read_batch测试import告警，不宣称本刀strict
Clippy通过。Windows tar对历史中文文件名解包失败，已用WSL tar完整重解；
不从缺文件快照构建。原始日志均保留，不豁免或更改既有测试。

边界纠正：默认Composite是STARK执行有效性证明，不是完整隐私能力。
[固定版本官方安全模型](https://github.com/risc0/risc0/blob/v2.3.2/website/api/security-model.md#zero-knowledge-proving)
明确未经递归处理会泄露执行长度，关键隐私用途仍有警告。SHA-512/Ed25519
曲线没有被本刀加速；官方SHA补丁的非对齐复制在当前bump allocator里累计
占堆，最大批量内存、全部SHA边界向量尚未测。不得把单笔通过外推1024笔。

证据：`target/runtime-rebuild/proof-sha256-{build-linux,guest-features,prove,
verify,old-image-reject,new-image-reject,positive-recheck,dev-reject,shared-tests,
shared-tests-v2}.log`；receipt为`nov-transfer-aorcp002-fixture/receipt-sha256.bin`。
旧producer保留为`proof-v2-baseline-probe`。复现仍用[原proof入口](../runtime/proofs/nov-transfer/README.md)，
使用新输出文件名，不能覆盖旧receipt。A继续验签/批量成本，再接同候选显式
后台证明附件与原AOEM持久/恢复；不占原执行批槽、不暗启强制策略。S4主链、
隐私/PQ、Execute、实体多机、长跑/部署未完成，本刀提交后CI另核。

## 设备 A：AOEM 2.3.2 与完整 NOV 关系真实证明（2026-10-03，本地验收）

用户明确授权临时激活AOEM后，在其main保留GPU/APFL远端提交，基于
`7f0c1490`交付通用修复`34d66a5187173c1da9d0ba4a89072b2cf0dfb9e2`，
已用XujueKing SSH推送并核对远端一致。SDK/build/两guest均固定2.3.2，
新`AORCP002`显式拒绝001；C ABI v1符号/所有权不变，无NOV专属业务。
Fake及嵌套assumption拒绝、显式非DEV verifier；Rust Release34测试通过，
Linux直接/插件跨进程真实Fibonacci证明与反例通过。Windows/Linux无后端
边界通过；Windows未执行真实证明，未发布新FULLMAX，也不冒充隐私/PQ。

随后回到SUPERVM `12b2a813058812bbe8be29abbbe0e1ff39505c4d`（其CI
`37046086286`现已成功）。干净HEAD archive加本次三个源/测试差异，冻结在
`target/runtime-rebuild/candidate-12b2a81-proof-v2/`；不构建或提交根49草稿。
适配仅接受002、001/003在native调用前拒绝；producer错误封装仍原库释放。
原probe补旧封装/尾字节反例，不新增证明API、队列或业务执行器。

Linux/WSL实际locked构建当前guest与probe，未使用`RISC0_SKIP_BUILD`；
guest platform/zkos-v1compat均2.2.3。image为
`[2276991241,3165397454,1971004867,473585677,2298952761,2672596182,3695874062,410919473]`，
934332B组合program SHA256
`e03315ffc499e2202557d522270a8abbd09612649cb2434c3ac1be599baa4f2a`。
probe SHA256 `e8027d31d35539858491a3ea75ec1064df7850f072614227874d7ccbbee7452d8`。
旧read-v1 probe/旧image不作为当前版本证据。

夹具用原随包FULLMAX core（SHA256
`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`）
真实验签、执行业务、结算费用并导出期待journal；这是公开测试密钥/父树的
**一笔**NOV直付，不是正式创世、真实用户、多机或已finalized块。
证明单独调用AOEM新Release CPU sidecar，SHA256
`cfdd4469ec588e5e274d647fad143fa8de129077d125805289e6ec8b2b5201ee`；
不覆盖计算/存储core，不更新随包header或发布包。`RISC0_PROVER=local`、
无DEV生成，默认Composite/poseidon2，无CUDA；probe自身为debug不改变
native Release prover。程序、image及期待journal均由可信构建/夹具独立提供。

真实结果：生成并验证成功，receipt **4,768,434B**。生产进程结束后新进程
在DEV=1下仍能验证真proof，**14项反例全部拒绝**并再次接受原正例：7公开
字段、空/追加journal、错image、篡改/截断receipt、旧001及尾字节。
另一个DEV=1生产进程对相同真实program/input返回`-4`，没有生成文件。
未来003仅属适配单测，不混计密码反例。

- input SHA256：`78bfa36213bdd659ab815902c7c5ee286e0aa994fe54c2fd2fd25e445bcc7876`
- 152B journal SHA256：`6873f4f3939c25052d28e32f0a2eb1b72fc5d3ada69caf957e33ae092be2410c`
- receipt SHA256：`c4273ef7e8efbd4f0c8b4d69d36f71abe04b094c4e669d28174c84452be87c52`

本次生成调用717.393秒（含AOEM内部验证/序列化），同进程另验197ms，独立
验证212ms；同时运行过lint，**不是独占性能基准、TPS或出块间隔**。原生
业务执行和生成密码证明不是同一计时。此成本不能接受为高性能证明主网；
没有测批量、GPU、连续证明容量或证明最终性，不把慢证明同步塞入共识poll。

两平台各8 receipt适配单测、适配crate strict Clippy通过，Windows3项
compile-fail、干净workspace fmt/差异检查通过。首次probe strict Clippy在
嵌套guest build失败，日志保留；随后在独立`proof-v2-probe-lint` target用
`RISC0_SKIP_BUILD=1`仅做host lint通过，**该target未用于真实证明**。
首次Windows测试日志目标目录不存在，cargo未执行；纠正日志路径后才计PASS。
本轮没有重跑完整工作区/四节点负载；提交后CI须另核。

证据均位于`target/runtime-rebuild/`：`proof-v2-build-linux.log`、
`proof-v2-fixture.log`、`proof-v2-prove.log`、`proof-v2-verify.log`、
`proof-v2-dev-reject.log`、`proof-v2-tests-{windows,linux}.log`、
`proof-v2-probe-clippy.log`、`proof-v2-probe-lint-only.log`；原始输入/输出为
`nov-transfer-aorcp002-fixture/`。复现命令见[既有proof入口](../runtime/proofs/nov-transfer/README.md)。

本刀解除的是通用后端兼容与完整关系实证阻断；S4主链有界证明owner、同候选
耐久绑定/恢复、批量/异构证明成本与最终性策略仍未完成。A继续这些共享范围，
B独立隐私/PQ不覆盖；Execute、多机/长跑/部署总目标不缩减。原目标仍保留，
工具实查blocked，不能由模型改成active；人工开发继续，自动续跑需用户恢复
原目标卡片。未新建分支、未生成正式创世或发行、未部署生产。

## 设备 A：候选持久后有界只读后态输入复用（2026-10-03，本地验收）

远端复验（2026-10-03）：`6b6196658b183c3016bc8e0071f2ba0cc8a192e9`
的CI `37044398209`已completed/success，Windows 11分45秒/Linux 9分24秒。
已读取两job日志：真库Release Windows666+8/Linux665+8，零失败/零忽略；
fmt、strict Clippy、native-free、普通测试和隔离检查通过。较早的代码提交
运行`37044219584`因后续文档push取消，不当作失败或另一次通过。下面的
in_progress为推送时记录。当前原目标active；本次没有新增runtime代码，
只更新证据/双机认领，S4仍待AOEM活动仓授权，未执行证明或生产部署。

交付更新：代码提交`a7db795cd752791612b8b4a13c26088cf0ce554e`已推送，
本地和远端一致；CI `37044219584`核对时in_progress，暂不称本刀远端全绿。
原目标工具现已恢复active，原总目标不变；下文blocked是本轮开始的历史状态。

基于`bae6703`（CI `37035987145`已成功），20个runtime源/测试文件；未更改
AOEM/经济/交易和共识编码，旧49草稿保留。运行路径是原签名NOV执行、同一
常驻AOEM/存储与controller，不是额外演示链或新数据库。

父捕获见证与真实staged delta移动保留，不克隆整个输入；包括hash的两层
实际节点字节计费。仅候选ID/状态根/statement/document四字段持久ACK核对
后安装单个driver内seed。子按自己的声明和root按需加载、逐边重验，缺项
批读原库；计数不包含持久预检和冷恢复的读取。byte-only lease不保留父
任务槽；动态保留一个最大当前批、静态保留背景+当前两个最大批，避免早
鉴权被缓存永久挡住。超额或最大预留无法计算时不缓存，不改交易成功范围。
独立匹配仅筛选内容源；父块hash/round/QC仍由原controller独立授权。

定位更正：现有successor必须先拿DurableCandidate，不能把其捕获等待
解释成尚未完成的同一父候选写。此刀减少持久父之后的重复读取与I/O竞争，
没有实现父候选持久之前的子执行或完整跨高度推测权限。

新增13门（7前沿、5纯cache、1真库）：含hash/错边/错root/权限/子预算、
partial miss/缺失拒绝、移动分配计费、inclusive双批额度、错误domain/
receipt/version、最大预留溢出、真实两高度2+2签名交易。原128MiB配置明确
回退；既有256MiB配置第二批必须实际命中；极大上限的合法小批仍成功。
三路完整packet一致，费用/余额/nonce/全部声明页和冷重开一致，shutdown
释放全部缓存计费。不声称模拟了native写ACK失败注入。

最终真库Release Windows **666+8**、Linux **665+8**，零失败/零忽略；
Host413、AOEM31、Network211/210、11集成、8文档。fmt、双平台strict Clippy、
native-free library与三成员无legacy隔离通过。冻结快照
`target/runtime-rebuild/candidate-bae6703-parent-seed-dev-v2/`；日志
`seed-workspace-{windows,linux}-v2.log`、`seed-clippy-windows-v4.log`、
`seed-checks-linux-v2.log`。最终binary SHA256：Windows
`1635c42006251144aa3f03ced917d6f708736c4320f939ab68093908247d8c25`，Linux
`f2031cbf64e46aba1d075e4b78ac51d82460332d8f093da196a17acdc239cacd`。

独占顺序新/旧/新、WSL同机四进程、64×1024预签名Ed25519测试转账：
8.742167/8.790665/6.300640秒，7496.539/7455.181/10401.483 TPS。钱包签名、
启动和冷恢复不计时；验签/业务/网络/BFT/四库耐久计时。三轮全经济冷恢复
通过，12个最终head完全一致；新版每节点seed捕获258615节点、库捕获2055
节点/39请求、0执行失败。旧binary为上一刀已验收SHA
`4af9c617059bb6eb915cef225244e3aea4c424ca29c4d0d06bf1f62c77904649`。
新版两报告目录为快照内`target/runtime-rebuild/`下
`controller-load-1024-291-1790963412327824681`和
`controller-load-1024-2075-1790963469326427778`；旧报告位于上一刀快照内
`controller-load-1024-1228-1790963441335082931`。原始日志本快照
`seed-bench-new1.log`、`seed-bench-baseline.log`、`seed-bench-new2.log`。
结果波动大，不宣称稳定提速、主网容量、实体四机或证明最终性。

首轮真实32批冷经济通过但seed全未命中，新断言FAIL：错误把父状态版本
按块+1而非交易数增加，已修。编译初轮缺计数字段、后补测试Debug约束错误
均修复，失败日志保留；没有降低命中/资源/经济门槛。本节提交/CI另核。
原目标模式仍blocked且模型工具不能恢复；已询问用户下一阶段AOEM通用
证明后端切仓授权，当前未切仓。S4、隐私/PQ、Execute、多机/长跑和部署
总目标未完成，未建分支、未开启正式创世/发行/生产部署。

## 设备 A：提前原文与真实节点绑定接入（2026-10-03，本地验收）

远端复验：`bae6703`的CI `37035987145`已completed/success（2026-10-03
本轮核验）；以下in_progress为保留的推送时记录，不是当前结果。

交付更新：本节已提交推送为`bae6703a8764b77976848193e29e533fafab080d`，
本地及远端main一致，旧49草稿保留；CI `37035987145`进行中，未签收远端结果。

基于`1fb606f`，19个runtime源/测试（13个已有+6个新增），其他140个tracked
runtime文件与HEAD一致；旧49草稿保留，不作为本刀构建输入。最终干净快照
为下述同一dev目录，最后测试日志`early-dev-workspace-{windows,linux}-v3.log`。
真库Release Windows **653+8**、Linux **652+8**，零失败/零忽略：Host400、
AOEM31、Network211/210、11集成、8文档。fmt、双平台strict Clippy、native-free
library与三成员无legacy隔离通过。最终Windows binary SHA256
`557bf83933f4e57af97088a973c19193203a9f1f85466b6f2683138f07ddecef`；Linux
`4af9c617059bb6eb915cef225244e3aea4c424ca29c4d0d06bf1f62c77904649`。

新增28项，包括8项真实controller反例：无父真实鉴权、错来源/域/ID、首绑定
不可改、父ACK跨认证/组装回执、未来拒签/当前持久签票、换轮drain/旧代拒绝、
单槽回退、坏签同代重放与冷恢复、持续有效控制流公平、真实单batch重开、
紧预算下Early/Bind缓存清理且live successor票据仍排空。另有真实后台bind
跨shutdown及未消费成功回执的原permit保留，typed真实Job排序与队满返还。
不是扣住native回调的抢占测试，也不假称逐帧网络session pin已贯通。

初轮真实新增经济oracle有3项FAIL：测试误以为max_pay_amount=0免手续费，
实际是自动报价上限，每笔既有费用45；修为两付款方999710/999810、收款300、
nonce2，保留普通执行全records对照和冷余额/nonce。生产经济未改。全量v2
双平台各399/400，旧prefix测试把tag7当未知；补7/8正确分类及截断拒绝，
未知标签改测0/9/255。原失败日志保留，未放宽资源/期限/经济断言。

独立Linux有限负载早/原后继/早模式（同一binary
`fd148e136b6c8cb86e47c661374231a7e73bceab15763d3b9cceef61fac4911f`）：
65536笔分别9.187579411/9.262477893/8.385941412秒，即7133.108/7075.428/
7814.984 TPS；三次全经济冷恢复通过，12个最终head完全相同。报告目录依次
`controller-load-1024-3362-1790957886297414682`、
`controller-load-1024-378-1790957979708235213`、
`controller-load-1024-365-1790958054493169430`（均在快照内target/runtime-rebuild）。
首轮各节点真实auth完成/绑定复用59/59/54/59；父能力前消费5/4/10/5。
不是全量回归并行运行期间的性能数，也不与旧机器时段8400样本直接相比；
当前不足以签收稳定吞吐提升、证明最终性、实体多机或公网容量。

A后续处理父写入阻塞子capture的只读后态seed，保持精确父成功持久ACK和
原预算。S4通用后端、隐私/PQ主链接入、Execute、多机和生产总目标仍未完成。
本次提交及远端CI另核。

### 先前开发记录（保留当时范围）

未提交，`main/origin/main=1fb606f`。仅忽略开发快照
`target/runtime-rebuild/candidate-1fb606f-early-body-dev-v1/`，root旧49草稿保持。
EarlyBody tag7/BindBody tag8独立payload v1，不改原1–6编码；绑定恢复同一
immutable raw的原canonical Body，真实认证body跨父绑定复用原背景permit。
本轮并未修改AOEM/SDK、旧实现或生产参数。

开发证据（不是最终签收）：Windows check、15项相关测试及strict Clippy通过，
WSL check通过。初版Windows真AOEM四进程32×8完成256笔和四库完整冷经济oracle；
各节点提前鉴权started/completed/reused为7/6/7/5，观察到父能力前消费auth
回执为1/1/1/3。后者不是物理写盘/native callback计时。日志
`early-dev-load32-windows-v1.log`；报告目录
`target/runtime-rebuild/controller-load-32-21740-1790957319674055700/`（位于快照内）。
0.610209秒是Windows小负载开发样本，不和旧Linux大负载对比或签收提速。
停止时4条TLS close_notify告警、旧上下文消息拒绝仍保留。

上述初版之后已补持续控制消息公平性、max_inflight/背景额度不可用回退、
同source代退休后重放限制、早后继退休时原文/绑定缓存清理。新真实反例在写，
最终全量双平台、1024×64与旧版对照尚未运行；不能以初版结果覆盖后续改动。
父写入阻塞子capture的只读后态seed尚未实现，S4、隐私/PQ、通用执行与生产
验收保持未完成。

## 设备 A：父状态无关鉴权与精确父绑定（2026-10-02）

交付更新：本节11个runtime文件与三文档现为`1fb606fe7c8b7a7fba3a5f185a62785bcfc9b076`，
已推送main并核对远端一致。CI `37023171845` Windows/Linux均成功；不是
提前原文协议/主链性能或完整产品签收。后续干净early-body开发快照未验收。

基于`0bca25d`；其远端CI `37016049228`双平台成功。本刀仅11个新runtime
源/测试与既有三文档，旧host11/legacy38保留，compute只提交新差异。

先对0bca25d干净导出做一次独立64×1024诊断：四进程、WSL2、24逻辑CPU、
真实WSS/AOEM与完整冷经济oracle PASS，8.409863秒/7792.754493 TPS；带诊断
开销，不与前版非诊断样本比较提速。每节点64个candidate/job精确关联、63对
完整父点，事件1334/1333/1333/1333、0丢失。父decision ACK前接纳/Prepare
约54/52/53/52次，真实首业务回调仅4/1/1/3次，AOEM返回2/0/0/2、耐久
完成1/0/0/2。每节点验签0.327–0.379秒；plan→capture完成2.069–2.214秒，
含增量I/O与排队；AOEM API0.827–0.880秒，finish0.754–0.804秒为其嵌套
部分，不能相加。不同进程时钟不相减，接纳时刻用enqueue前后区间处理。
报告在`target/runtime-rebuild/candidate-0bca25d-successor-profile-v1/`
的`SUCCESSOR_PROFILE_REPORT.md`，原始目录为其内
`target/runtime-rebuild/controller-load-1024-412-1790950434457956118/`。
诊断binary SHA256 `d28fecef57df8f8f0e5b85aeb41c1f7bbfae63c7904266dbd9fd03eb9cce42af`。

实现路径：`NovTransferBody::prepare`只接已验签body、固定policy与预算，
提前推导完整请求/声明，无父根/时间/余额/nonce授权；消费式bind仍走原
BatchPlan全部链域/程序/效应/codec/父形状验证，状态须重新精确capture。
原compile和普通pipeline复用这条路径。新增`try_authenticate_owned`及
`try_submit_authenticated_owned`均接同一驻留compute owner；绑定不重复
验签，不开启额外引擎。背景与原后继共用一个许可，预留完整普通批；原
Permit跨真实在途、未消费reply、外持body、bind和最终回执连续保留。
错误/队满退回原件，新pipeline（即使同库重开）拒绝旧owner body。

新增16项测试与2项compile-fail：原编译器对照/预算/签名/错误context，
双精确父余额/nonce/费用/回执结果，channel丢票/回压/未消费额度/owner，
真实AOEM坏签后继续、错误程序拒绝、预鉴权不持久化候选、查询与数据库
重开完整经济投影、外持body不阻塞shutdown、新实例拒绝旧body。
真实native回调被扣住时丢认证ticket的专项未做；channel门不冒充该证据。

最终源快照`target/runtime-rebuild/candidate-0bca25d-preauth-dev-v1/`：
8个tracked改动+3新文件，其他142个tracked runtime文件等于HEAD；旧46项
无重叠草稿hash未变，controller/load未改，compute旧diagnostics不混入。
真实库Release Windows **625+8**、Linux **624+8**，零失败/零忽略；Host372，
AOEM31、Network211/210、11集成、8文档。fmt、双平台严格Clippy及native-free
library、三成员无legacy隔离通过。最终日志为快照内
`preauth-workspace-{windows,linux}-v2.log`、`preauth-clippy-*-v2.log`、
`preauth-proof-lib-clippy-*.log`。首轮unused import使strict Clippy失败已修；
误将native-free用于all-targets导致native集成测试编译失败，未修改这些
集成门，改按CI既定`--lib --no-default-features`检查，原失败日志保留。
Windows test binary SHA256 `509412c4750be68382d09cbae19216286a0f84d8c0f6d0a9c3a58dc642329c7f`；
Linux `de89eff9579288f3dfac70f102f7ea2fb1aba0004914a0b00047cde91288b9d3`。
AOEM随包库不变。本次远端CI推送后单独核对。

限制：controller/网络尚未使用提前鉴权入口，无本刀主链TPS改善声明。
背景绑定后按普通优先推进，但原background许可仍占至完成回执消费，不能
说可同时再预鉴权一批；AOEM已接受图仍不可抢占。Host逻辑额度不是异常
native保留内存硬上限。下一步接现有controller/HostChannel有界提前原文，
按精确正文/父点消费，并继续解除捕获强制串联，不绕过经济或共识。
S4后端授权、Execute、隐私/PQ、实体多机/持续容量/生产授权等总目标仍未完成。

## 设备 A：一层后继流水线与父最终性重叠（2026-10-02）

基于`ed22c5d`，其CI `37008042238`两项均成功：Linux5分24秒、Windows12分
57秒。本刀仅8个新runtime源/测试文件和三文档；不是生产/稳定容量/S4签收。

同基线独立时间线诊断`candidate-ed22c5d-height-profile-v1/`记录每节点320项
真实事件，0丢失；候选耐久→decision ACK均值43.12–45.92ms，decision→
advance ACK3.34–3.57ms，advance→下块正文就绪66.35–67.66ms。仅用于找到
交接依赖，不把全部等待当作无用开销。提议者原先到提议耐久ACK后才传播正文，
跟随者因而较晚开始计算；新后继正文仍使用既有HostChannel/加密网络路径。

早期干净可变开发快照`candidate-ed22c5d-successor-dev-v1/`、日志
`successor-dev-v1b-load32.log`及`successor-dev-v1b-load1024.log`：

- 32×8：256笔、0.264732秒/967.016437 TPS；四节点各8执行、7次后继
  在父确认前完成、7次精确晋升复用。完整冷恢复经济oracle通过。
- 1024×64：65536笔、8.307132491秒/7889.124204 TPS；四节点各64执行、
  53/54/55/55次后继启动，0提前完成/0完成态复用、0执行失败/陈旧结果。
  这些后继在父确认前被pipeline接纳，未取回完成回执的ticket转入当前路径；
  接纳不是AOEM回调开始，也不是已提前完成。四节点耐久与冷恢复oracle通过。
- 1024报告：快照内`target/runtime-rebuild/controller-load-1024-389-1790946427315380884/`。
  两组仍有旧父上下文拒绝，停止时TLS close_notify告警保留。

以上是**同机四进程、Ed25519测试账户、有限预签名负载**，不是四台实体设备/
PQ/稳定容量；验签/业务/网络/共识/四库耐久均计时，钱包签名和冷恢复不计时。

### 接入、真实反例与双平台回归

`controller/successor.rs`只从本机真实已耐久父候选派生一层输入；只有下一
高度round0 leader可预发正文，仍走原HostChannel/网络。私有后继不进入当前
候选表；完整context、六字段父点、已ACK head及round0全匹配才晋升，原journal
再查签票权限。落败/换轮票据移独立drain，不占同requester当前槽，不挂回后来
同ID正文。尚未获ACK的持久内容不是最终块，孤儿内容不会因重启自动获得能力。

pipeline仅一个后台许可，未消费回执仍计额，普通任务保留槽及最大逻辑内容；
driver每轮普通任务优先、每job一次有界转换。审查发现后台正文可能挤满缓存，
已加当前正文对后继及固定发送引用的抢占，原上限不变。已接受native任务不
取消，继续排空。错误路径保留owned状态/交回退休owner，不在poll线程丢大型体。

新增6项真实AOEM安全测试：提前完成不签未来票/精确晋升、另一父胜出（完成与
未取回票据）、换轮后同requester准入且同ID迟到结果不复活、编码回执晚到、
构造允许的紧正文预算抢占、真实关闭数据库重开后孤儿可读但能力失效。
它们控制回执消费时序而非伪造候选；经济记录与独立普通执行对照，完整费用
守恒仍由四进程经济oracle覆盖。4项准入/调度测试与32/1024×8后继专项纳入
全回归。未知write/进程kill由既有全量门覆盖，未新增“后继执行中进程kill”
或持续ingress下drain专门故障注入，不将相邻门宣称为该专项已执行。

最终源`target/runtime-rebuild/candidate-ed22c5d-successor-v1/`：8个覆盖文件，
其余142个tracked runtime文件与HEAD相同；controller/load不带旧11项Host
草稿，其余47项旧草稿SHA256未变。真实库Release Windows **609+6**、Linux
**608+6**，均0失败/0忽略；Host356、AOEM31、Network211/210、11集成、6文档。
fmt、双平台strict Clippy（含native-free）和三成员隔离检查通过。

保留失败：首次新增测试nonce查询类型错误，改用真实鉴权nonce_identity后
6/6通过；`successor-dev-v1g-preemption-red.log`移除抢占后确实拒绝当前正文，
负对照1失败。随后Linux复用target误运行该负对照旧二进制，输出仍指向dev，
该次355通过/1失败不计最终验收；独立`successor-v1-linux`重编译全部通过。
最终日志：`successor-v1-workspace-windows.log`、`successor-v1-workspace-linux-fresh.log`、
`successor-v1-clippy-windows.log`、`successor-v1-clippy-linux-fresh.log`。

### 冻结二进制新/旧/新交错

构建/其他重测试全部终态后单独顺序运行，仍同机WSL2/24逻辑CPU、原120秒
期限、1024笔×64高，预签名/创世/冷恢复不计入TPS。旧为当前ed22运行基线，
不是更旧ab1或引擎小循环；12份最终head完全相同、各64实际执行/64耐久决定，
0执行失败/陈旧结果/重算。保留旧父点拒绝、未来正文早于本机父耐久时的拒绝
及停机TLS告警，不称日志零错。

| 样本 | 四节点全部耐久秒数 | 唯一交易TPS | backlog P95/P99秒 | 完整冷恢复 |
| --- | --- | --- | --- | --- |
| 后继v1第一轮 | 7.798657914 | 8403.497207 | 7.350784632 / 7.798657914 | PASS |
| ed22对照 | 12.934339380 | 5066.822361 | 12.323403429 / 12.934339380 | PASS |
| 后继v1第二轮 | 7.710410158 | 8499.677534 | 7.428536795 / 7.710410158 | PASS |

约66%–68%的有限负载改善，不是稳定容量/百万TPS签收。第一轮各节点接纳
58/58/53/55个后继，全部在无完成回执时晋升；第二轮接纳59/54/52/58个、无
完成回执晋升58/54/52/57个，另两节点各1次提前完成复用。接纳不能证明native
实际开始时刻；32×8和安全专项证明真实提前完成存在，不外推为每块都已算完。

新冻结`candidate-ed22c5d-successor-v1/bin/novovm-host-successor` SHA256
`31069ad90c9d4598e13c919c2c09ea9e730c944a447a8b467f5d6762bc1e5ebd`；
旧`candidate-ab1fbf0-read-v2/bin/novovm-host-read-batch` SHA256
`c6f39379f8bc5600948052cfae3f752dd3ab3b1bec25168c74b77183b4cb7367`。
Linux AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
新报告在v1快照自己的`target/runtime-rebuild/controller-load-1024-365-1790948617986179547/`
与`controller-load-1024-400-1790948771861333567/`，旧在read-v2快照下
`controller-load-1024-401-1790948684921828480/`；日志依次为
`successor-v1-long-1.log`、`successor-base-long-1.log`、`successor-v1-long-2.log`。

后台只是有界预算及轮次优先，不能抢占已进入AOEM/native I/O的调用，不承诺
当前任务零延迟影响。退役不删除已经落盘的孤儿内容，不冒称磁盘已回收。
下一处A继续关联真实worker进入/完成与父ACK，按实测消除剩余强制串联；
未修改AOEM/SDK/共享业务证明关系，S4后端授权、B隐私/PQ、Execute、多机长跑、
正式部署完整目标保持。本提交远端CI待推送后另核。

## 设备 A：完整声明的精确父输入批读取（2026-10-02）

基于`ab1fbf0816f13a310a514fb8d29ff8d801e13fcc`，该提交CI `37004594892`
已终态双平台成功（Linux7分42秒、Windows12分23秒）。本次只提交十个新runtime
Host源/测试文件与三份既有文档；49项旧草稿（host11/legacy38）按文件摘要
保全，不改AOEM/SDK、网络生产代码、经济/签名/共识版本或原额度/期限。本轮
续作实际完成正确性和同路径性能证据，不以解释数字或状态重述代替目标进展。

### 接入与拒绝边界

`read_state_values`将最多4096个key的digest排序，一次遍历共享路径，保持原
输入顺序和重复项。访问节点先核对hash/codec及来边，再判定认证缺席；压缩
前缀两端查询均缺席时仍查中间命中区间。只读、不stage、不访问未查询子树，
空查询仍不验证父根，保留节点/读取预算。批预算按调用而非全进程，失败首报
顺序可能改变，不宣称与逐点接口所有资源耗尽结果逐字相同。

`OwnedStateInput::read_many`在遍历前检查全部声明权限；为删除而捕获的兄弟
节点不因此变成可读账户，源释放/跨线程后仍只依赖owned数据。NOV完成捕获
一次读取编译器全部声明，包括policy9项、fee33项完整尾页，再由原分页codec
和业务验证构造输入。缺投影项为错误，只有认证的`None`表示不存在；此局部
不可变投影不是数据库、跨父缓存或签票权限。既有编译器已限制声明<=4096，
不缩小合法NOV输入集合；原逐点接口保留，原增量捕获/I/O公平性不变。

新增10项纯树、3项owned边界测试；覆盖旧逐点oracle、乱序/重复/中间命中、
256层、每个旧必读节点缺失/损坏、正确hash错误来边、缓存命中和预算。512-key
夹具读节点由5279降至1023，只是工作量证据。真实AOEM原两高度经济oracle
回归确认32/1024笔每次捕获分别只调用一次批读、恰好107/2091项，精确等于
编译器声明数；之前实际2049项Put批更新也仍生效。余额、nonce、费用、回执
及失败归并代码未改，native/proof复用同一完成捕获关系。

### 双平台与冻结证据

最终快照`target/runtime-rebuild/candidate-ab1fbf0-read-v2/`只含活跃构建源；
十个覆盖文件与工作区逐字相同，其余137个tracked runtime文件等于HEAD。
真实随包库Release Windows **598+6**、Linux **597+6**通过，0失败/0忽略；
Host345、AOEM31、Network211/210、其余11集成及6编译拒绝。双平台fmt、
strict Clippy（含no-default-features）、三成员无legacy隔离检查通过。
v1也全过；独立复核发现原真实查询计数`>2*batch`不能单独证明尾页进入批读，
v2加强为精确等于全部声明数并重跑全量。未改变生产逻辑或降低任何旧门。
日志`read-v2-workspace-{windows,linux}.log`、`read-v2-clippy-{windows,linux}.log`、
`read-v2-real-nov-windows.log`均在仓库`target/runtime-rebuild/`，v1记录保留。

### 同路径新/旧/新交错

原WSL2/24逻辑CPU、四独立OS验证进程/四AOEM RocksDB、真实WSS/E2E；1024个
公开测试账户各64次不同nonce的Ed25519测试转账，共65,536笔，不是真实资产
或用户，也不是PQ/四台设备。编译结束后单独顺序运行，保持原120秒和完整
冷恢复经济/逐笔回执oracle。TPS包含节点验签、执行、网络、共识和四库耐久
ACK，不含预先签名/启动/创世/冷恢复。P95/P99从整个有限backlog释放起算。

| 样本 | 四节点耐久秒数 | 唯一交易TPS | backlog P95/P99秒 | 冷恢复 |
| --- | --- | --- | --- | --- |
| 批读v2第一轮 | 13.720710331 | 4776.429093 | 13.104059752 / 13.720710331 | PASS |
| ab1运行基线交错复测 | 14.781715159 | 4433.585636 | 14.146088635 / 14.781715159 | PASS |
| 批读v2第二轮 | 13.118875195 | 4995.550230 | 12.533330933 / 13.118875195 | PASS |

新样本比中间对照高约8%–13%，不签收稳定容量/百万TPS；旧基线前两次
4576.096/4436.977也保留。全部12份observer均64执行/64耐久、0执行失败/
陈旧结果/重算，完整head（块hash、状态根、回执承诺、决定值和版本）完全相同。
仍有旧父点/collector上下文拒绝，停机TLS close_notify告警保留，不称日志零错。

新冻结二进制`candidate-ab1fbf0-read-v2/bin/novovm-host-read-batch` SHA256
`c6f39379f8bc5600948052cfae3f752dd3ab3b1bec25168c74b77183b4cb7367`；
旧对照为已签收`candidate-86e4c8a-bulk-tree-v6/bin/novovm-host-bulk-tree`，
SHA256 `a52cba1a3762ecf796012764dbb6aabec7cd038d07f69752510a6ab50dce4fda`，
不是更旧86运行版。Linux AOEM仍为
`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
新报告在v2快照自己的`target/runtime-rebuild/`下
`controller-load-1024-386-1790944088944112263/`与
`controller-load-1024-399-1790944188553808826/`；旧报告在旧快照下
`controller-load-1024-1242-1790944123316931722/`。对应日志依次为
`read-v2-long-1.log`、`read-base-long-1.log`、`read-v2-long-2.log`。

独立诊断快照`candidate-ab1fbf0-read-profile-v2/`仅另加既有40项test-only
计时，用全新target重编译；3项计数测试通过、实际348项Host测试。一次同负载
13.367325789秒/4902.700887 TPS、完整冷恢复通过，只作定位。每节点64块
`finalize_capture`由旧诊断0.938–0.966秒降至0.279–0.299秒，`compute_execute`
由2.123–2.151秒降至1.493–1.501秒；`pipeline_capture`仍0.788–0.822秒，
`io_persist_step`1.393–1.639秒。计时包含嵌套，不能相加作CPU或关键路径。
诊断二进制SHA256 `4de7c3e29a606f2b5c0b96533ab9808b6a28b2f4cec1cb7a71081fa3aa6eb6d5`，
该快照报告`controller-load-1024-383-1790944369004409383/`在自己的
`target/runtime-rebuild/`下；`read-profile-v2-{build,counts,long-1}.log`保存原始
结果。诊断源码不合入运行代码，也不以park时间证明某个等待必定可消除。

### 同关系证明与下一处结构工作

相同生产代码（v2只加强测试）真实重建RISC0 2.3.2组合program：931496B，
SHA256 `1a737aa714f76654d6d09e6c65408790e2ea7d292cd2ac140dd4676fe7d12852`，
image `[2059554605,1731202156,985988505,2180991583,242081205,2542116061,3383213490,2657932241]`。
真实AOEM单笔fixture的input/journal摘要与前版相同；证据在`read-v1-proof-*.log`
和`read-v1-proof-fixture/`。未重跑密码prove/verify，旧后端不兼容未解除；构建/
共享relation通过不等于S4，也不能用旧有漏洞guest绕过阻断。

HEAD只读审查确认：现有controller多inflight是当前精确父下的候选，实际负载
pipeline为2个job、1项I/O请求；计算/持久化可交错，但后继仍等待候选耐久、
decision metadata ACK、advance ACK之后才切context/生成下一批。下一处A
认领controller/pipeline/channel的跨高度关联时间线及有界一层后继推测；
尚未实现。只能从本机真实执行且耐久父候选派生私有输入，父正式确认后逐项
重验才可进入签票；当前高度优先、落败父排空、未知写入和恢复不能弱化。
不能只放宽context检查或增加max_batches，也不把预验签等同后继业务已执行。
旧host11归档/诊断草稿不直接合入。B独立隐私/PQ、S4明确AOEM切仓授权、
Execute、多机/长跑/部署等完整目标不缩减；本次新提交CI须推送后另核。

## 设备 A：共享前缀批更新实际接入 NOV 输出（2026-10-02）

本提交基于`86e4c8a`，该前置提交的CI `36996694111`双平台成功；本提交远端
结果须推送后另核。仅7个新runtime Host源/测试文件与既有三文档；AOEM/SDK、
网络生产代码、经济/签名/共识版本、原额度和期限不变，host11/legacy38草稿
仍保留、不混入。本轮是实际代码与验证进展，不是完整目标签收。

### 实际改变及正确性边界

`stage_state_update`将连续且digest不同的Put段按共享前缀一起构造，不再每个
key重建全部祖先后丢弃中间版本。Delete保持原位置，重复digest段仍逐条原序；
不跨Delete聚合、不改变业务执行顺序。整批共用Planner，4096变更/65536节点/
原读取界限不增大；访问节点逐边认证、删除survivor、缺失/损坏拒绝及原树字节
保持。临时节点减少会改变某些资源耗尽结果，畸形输入的首报错误顺序也可能不同，
不宣称拒绝集合逐字相同。空批仍不验证父根；不扫描未访问子树、不增加发布权限。

原逐条`Planner::change`作独立oracle，比较根及完整可达节点字节；覆盖64个父
版本、不同Put顺序、256层、每个旧必读节点删除/篡改、有效hash错误边、稀疏
owned前沿、重复/Delete混合与资源界限。512-key纯树夹具构建/替换stage调用
由4557/5279降到1023，仅是工作量，不是主链TPS。

新增真实AOEM回归使用同一controller签名负载生成器，32/1024笔、连续两高度，
重新验签/编译/捕获/真实回调执行，再核对完整经济oracle及手续费/nonce。
test-only计数确认1024笔实际输出的2049项账户/nonce段确实批处理，两个高度
stage调用4123/4127，可达节点均4103；32笔对应65项。不是Host预造执行结果。

### 完整回归与失败保留

最终干净快照`target/runtime-rebuild/candidate-86e4c8a-bulk-tree-v6/`，7个覆盖
文件与工作区逐字一致，其余138个tracked runtime文件按Git规范化内容等于HEAD。
真实随包库Release：Windows **585+6**、Linux **584+6**通过，0失败/0忽略；
其中Host332、AOEM31、Network211/210，其余为集成和6项编译拒绝。fmt、双平台
strict Clippy（含no-default-features）及双平台三成员/无legacy隔离检查通过。

- v1：新纯树测试错误假定旧调用必定超过新调用3倍，实际按digest排序的旧对照
  仅约2.83倍。改为原输入顺序和精确共享祖先次数，不降低业务/恢复门。
- v2：全量通过且两轮约4048/3992 TPS，但真实费用分页含Delete，整批回退旧
  路径；**不签收v2主链优化**。这也是增加真实执行路径计数回归的直接原因。
- v3：新增两高度夹具遗漏第二高度的非零父hash，真实验证拒绝；修夹具绑定，
  不放宽父点规则。v4全量通过，但strict Clippy拒绝测试中的`>= n + 1`，
  改等价`> n`。这些快照及失败日志全部保留。
- v5：Windows全新快照+外置编译目录使两个网络夹具找不到本地artifact目录；
  它们依赖其他测试先创建目录。v6只使夹具自行创建并规范化同仓目录，原路径
  边界不变；在Linux测试尚未启动的全新目录中，两项Windows反例独立转绿。
- 最初v2诊断复用了旧Cargo二进制，输出落到错误manifest目录且没有profile，
  明确排除该次诊断；随后独立target重编译才取证。不能只据cargo命令名信任版本。

日志均在仓库`target/runtime-rebuild/`：`bulk-tree-v6-workspace-{windows,linux}.log`、
`bulk-tree-v6-clippy-{windows,linux}.log`、`bulk-tree-v6-fresh-artifacts-windows.log`、
`bulk-tree-v6-real-nov-windows.log`；此前各v1–v5日志保持原名，不覆盖失败。

### 同路径交错对照

同一WSL2/24逻辑CPU，四OS验证进程、真实WSS/E2E、四AOEM RocksDB。1024个
公开测试账户各64笔独立Ed25519签名，共65,536笔/64块；全量测试/编译结束后
按“新→旧→新”顺序单独运行，未同时构建。原120秒、验签/费用/nonce、完整
冷重启状态和逐笔回执oracle不变。签名生成/启动/创世/冷恢复不计入TPS，节点
验签、执行、网络、共识和四库durable ACK计入；不是四设备、公网或PQ负载。

| 样本 | 四节点耐久秒数 | 唯一交易TPS | backlog P95/P99秒 | 完整冷恢复 |
| --- | --- | --- | --- | --- |
| v6第一轮 | 14.321378143 | 4576.095914 | 13.674838885 / 14.321378143 | PASS |
| 原86基线交错复测 | 17.490405712 | 3746.968543 | 16.720616826 / 17.490405712 | PASS |
| v6第二轮 | 14.770417527 | 4436.976807 | 14.132601736 / 14.770417527 | PASS |

两轮比中间对照约高18%–22%，属于本机有限负载改善，不是稳定主网容量或百万
TPS签收；此前基线另两次3749.785/4038.898也保留，不只挑最慢值。P95/P99从
一次性释放整个backlog起算，包含前面高度排队，不能当单笔服务延迟。三个样本
所有节点均64次执行/64次耐久决定、0执行失败/陈旧结果/重算，最终块/状态/
回执/决定值相同。仍有旧父点/归档上下文拒绝记录，停机TLS close_notify告警
保留，不能称日志零错误。回调峰值不充当吞吐或所有业务的并行证明。

v6二进制SHA256 `a52cba1a3762ecf796012764dbb6aabec7cd038d07f69752510a6ab50dce4fda`，
冻结在该快照`bin/novovm-host-bulk-tree`；原基线冻结在`...-bulk-tree-base/bin/novovm-host-baseline`，
SHA256 `ab41fb6d77ddbbb5d855122b4355123aa24b90c301fa3c931de1691e881f0165`。
v6报告在其`target/runtime-rebuild/controller-load-1024-397-1790942129441150592/`
和`controller-load-1024-2083-1790942200984141215/`；原基线在自己的
`controller-load-1024-1243-1790942163648666641/`。原始日志分别为
`bulk-tree-v6-long-1.log`、`bulk-tree-base-long-3.log`、`bulk-tree-v6-long-2.log`。
随包Linux AOEM SHA保持`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

诊断单独从相同源码加前述40项test-only计时导出，使用全新target重编译，3项
计数测试通过；`candidate-86e4c8a-bulk-tree-profile-v6`实际输出在自己的目录、
含335项Host测试及完整profile。该轮14.810557534秒/4424.951583 TPS与冷oracle
通过，作为定位而非正式容量样本。每节点64块的business_stage约0.465–0.489秒，
前一个真实但未命中批路径的v2诊断约1.96–1.99秒；compute_execute现在约
2.123–2.151秒，finalize_capture0.938–0.966秒、pipeline_capture0.791–0.832秒、
persist_step1.479–1.949秒。恢复次数35–42也高于前轮，不把不同运行的阶段
变化都归因于树算法。嵌套计时不可相加作CPU或关键路径，park含合法等待。
诊断二进制SHA256 `e1a3b61973bdedaa9496ce9cb5d687231e7bc08abac7de1edf2593fbf2d1fc9b`，
该快照的`target/runtime-rebuild/controller-load-1024-6792-1790942364916087995/`
保留四份`live-*.json`与完整measurement；日志`bulk-tree-profile-v6-build.log`、
`bulk-tree-profile-v6-long-1.log`。这些诊断改动没有混入本次交付代码。

### 证明与后续范围

v4已对相同生产代码真实重建RISC0 2.3.2 guest（v5/v6只修改测试）：组合program
909796B，SHA256 `cbd92a66861ce14c7914e1228e70e1116aa3aa7a164d7f8221357ae926a32346`，
image `[2258603708,954323974,3056129725,1567023945,596580575,4811308,3333476220,1287232694]`。
真实AOEM单笔fixture的input/journal摘要与此前相同；记录在`bulk-tree-v4-proof-*.log`
和`bulk-tree-v4-proof-fixture/`。本轮未重跑密码prove/verify，旧后端不兼容阻断
未解除；不能用构建、relation测试或QC宣称S4完成。

下一处A认领新runtime的owned前沿批读取/完成捕获：当前`finalize_capture`
仍逐key调用`OwnedStateInput::read`重复遍历相同不可变树。减少重复路径构造
与解码必须保持访问权限、每条认证边、缺失/损坏/父根绑定和增量I/O公平性，
不能由缓存授予新父点权限。先独立旧逐点oracle/真实业务对照，再按同路径
阶段与完整负载验收；此下一切片尚未实施。不回旧归档草稿。B独立隐私/PQ认领和S4通用
AOEM后端的明确切仓授权边界不变。Execute、隐私/PQ主链接入、实体多机、
长时容量、正式部署仍未完成；没有新Skill/分支、正式创世、发行或部署。

## 设备 A：阶段实测指向批量状态树更新（2026-10-02，前置诊断）

运行基线`86e4c8a`，CI `36996694111`已终态成功：Linux7分8秒、Windows12分44秒。
只在HEAD导出的诊断快照增加可关闭的测试计时，不合入旧host归档草稿、不改调度。
前一目标轮属进展；本轮实际完成阶段测量，不能以状态重述替代开发。

同一WSL2/24逻辑CPU、四OS验证进程/四AOEM RocksDB、65536笔Ed25519测试转账，
原120秒与完整冷经济oracle均保留：

| 诊断样本 | 四节点全部耐久耗时 | finalized TPS | 冷恢复 |
| --- | --- | --- | --- |
| v1开启36项计时 | 17.821044413秒 | 3677.450013 | PASS |
| 同v1二进制关闭计时 | 18.000255733秒 | 3640.837162 | PASS |
| v2细分finish为40项计时 | 25.796245567秒 | 2540.524738 | PASS |

v2较慢事实保留，不能把整轮差异直接归因于四个计时点或声称稳定容量。计时为
各进程完成操作的累计墙钟，嵌套重叠、不能相加作关键路径或CPU时间；park包含
等待计算/I/O和真实空闲，不自动等于浪费。I/O排队从try_send尝试前到owner取出，
不含内部writer队列。签名提前生成，验签计时；不是PQ或四台实体设备。

v1各node的compute_execute约3.92–3.97秒、finish约2.46–2.48秒；HostChannel
encode/body_id分别不足5毫秒，归档实际恢复仅1–4次、约7–29毫秒。v2进一步
测得finish约3.22–3.41秒，其中input.stage约2.92–3.12秒，费用/业务有序归并
约0.20–0.22秒，变更集约0.02秒，回执承诺约0.04秒。据此优先改树更新，而非
盲加归档缓存。源码当前逐key重建共享祖先，并最终丢弃中间节点；A认领通用
unique Put批更新，根/字节及逐边验证对照、删除/重复key原序与全部资源界限保留。
本节仅诊断证据及在做任务，不是算法或主链提速签收。

原始快照`target/runtime-rebuild/candidate-86e4c8a-stage-profile-v1/`与`...-v2/`
各保留独立源码。v1报告在其`target/runtime-rebuild/`下
`controller-load-1024-414-1790938545430139283`和
`controller-load-1024-411-1790938727929286226`；v2为
`controller-load-1024-405-1790938942352033967`。
日志在仓库`target/runtime-rebuild/stage-profile-v1-long-1.log`、
`stage-profile-v1-control-1.log`及`stage-profile-v2-long-1.log`。
v1 Host SHA256 `8d68127d4b217683c2e6667a8ee41ed3b793b808dc8664e106e8ed7b8cb5acc4`；
v2 `690d52423c6455047656fa205d926d01d61ca5cbff88a193e0ebe69ea8ac6ada`。
AOEM SHA256保持`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
诊断计数3项测试均通过；初次36项Serialize数组编译失败已改为切片序列化，
失败日志仍保留。host11/legacy38草稿不提交，B独立隐私/PQ、S4后端待明确AOEM
切仓授权及Execute/真实多机/容量/部署等总目标不变；未正式创世、发行或部署。

## 设备 A：显式协商的紧凑载体，传输减量与整链性能分开验收（2026-10-02）

基于`97f9322`，其远端CI `36993299183`已实际完成：Windows/Linux均成功，
包含此前失败的原96大帧门；不据单次成功抹去下方旧失败。本轮仅network十三
文件和既有三文档，host11/legacy38草稿、AOEM/SDK、经济/签名/共识规则不改。

### 载体与兼容边界

Data/Delivery的密文不再编码为JSON数字数组。`NVRLY002`加消息tag、固定大端
字段和有界长度前缀，保留原字段/路由/nonce/密文，控制消息仍用原JSON。
整体1MiB上限不变；binary完整长度/UTF-8/尾字节预检后才复制字段，旧JSON
Data/Delivery（包括转义和字段重排）明确拒绝。WebSocket请求和响应必须确认
唯一`novovm.relay.binary.v2`，缺失/错值/混合/重复拒绝，发生在身份注册前；
同一manager的active/offline始终存同一种唯一编码，无旧格式回退或双份缓存。
这是外层载体V2，不是交易V3、PQ、共识或AEAD版本变更；所有端点须同步升级。
客户端原HTTP状态/Accept子串判断同时改为精确状态码/字段、重复拒绝及
Upgrade/Connection token检查，合法header大小写/OWS保留。

实际wire计额、锁外一次编码、原guard跨部分写、TTL、15项累计credit及
100ms写停顿/10秒帧期限保留。192KiB原文得到196720B密文和197111B Delivery，
对比原样本约702993B，减少约72%。固定4KiB发送/64KiB接收fixture仍真实触发
WouldBlock，原大帧guard未释放时第三接收者恢复48B原文（本次约0.332ms）。
固定2 CPU原96大帧门180.671ms通过，无中途重连/丢失；这些均不是主链TPS。

首轮工作树网络测试Windows208 PASS/1 FAIL、Linux207 PASS/1 FAIL：旧延迟请求
fixture的40个200KB大包因不再JSON膨胀，不能满足原>16MiB压力前提。仅把测试
输入增至90个，保留原门槛、累计投递上限与余额断言，未提高生产额度；首轮
失败日志保留。随后HTTP复审修正和完整验收均在下述冻结源码完成。

### 干净快照与完整路径

树`6a3dd1615446b9162996f128b9d49c12040a7792`导出到
`target/runtime-rebuild/candidate-97f9322-compact-carrier-v1/`；按Git规范化内容
逐一核对97个host/AOEM文件等于HEAD、十三network文件等于暂存版，旧草稿未混入。
真实随包AOEM、Release include-ignored串行测试Windows **565+6**、Linux **564+6**
全部通过，0失败/0忽略；network分别211/210。双平台fmt、全targets与Host
no-native strict Clippy通过；Windows隔离脚本及Linux全量metadata三成员/无
legacy检查通过（后者在PowerShell判断，不冒称Linux原生pwsh脚本）。默认
Debug全套本地未重跑，本次新提交远端CI须另核。

同WSL2/24逻辑CPU、四OS验证进程/四AOEM RocksDB、真实WSS/E2E；1024公开测试
付款账户各64笔连续nonce的Ed25519转账。重负载构建结束后同二进制独立两轮，
原120秒门、四节点耐久head和完整冷经济oracle不变：

| 样本 | 唯一最终确认交易 | 四节点全部耐久耗时 | finalized TPS | 全量冷恢复 |
| --- | --- | --- | --- | --- |
| 第一次 | 65536 | 17.825803749秒 | 3676.468165 | PASS |
| 第二次 | 65536 | 17.382711173秒 | 3770.182876 | PASS |

**与前版3565/3637 TPS仍接近，不签收性能突破或稳定容量。** 计时含节点验签、
执行、网络、共识与耐久，不含钱包预签/启动/创世/冷恢复；不是PQ、真实用户或
四台实体机。积压P95/P99分别17.004/17.826秒、16.593/17.383秒，含等待前序
高度，不是单笔服务时间或生产出块周期。每节点64执行/64决定，失败/陈旧/
重算均0；实际回调峰值21/24/13/19和12/24/14/16，不能当作主链吞吐。
仍有`decision not for exact current parent`拒绝，不称零错误。两轮四库冷重开
后64高head、候选、逐笔回执/nonce/费用/全状态根均通过完整oracle并读回核对。
relay各注册4、替换/过期/拒绝0、停机断开4，转发5399/5385帧；全生存期接纳
36349754/36341848B（前版约128MB），包括启动/退出，不换算精确测量窗带宽。
停机所有队列归零，停机TLS无close_notify日志仍保留。

原始日志均在`target/runtime-rebuild/`：`compact-carrier-work-network-{windows,linux}.log`
是首轮失败；`compact-carrier-v1-clean-workspace-{windows,linux}.log`、
`compact-carrier-v1-clean-clippy-{windows,linux}.log`、`compact-carrier-v1-two-cpu-linux.log`、
`compact-carrier-v1-real-backpressure-linux.log`及`compact-carrier-v1-clean-linux-long-1.log`、
`compact-carrier-v1-clean-linux-long-2.log`。两轮独立measurement、四库/live/recovered
报告位于快照内`target/runtime-rebuild/controller-load-1024-406-1790937196329367614/`
和`controller-load-1024-406-1790937265966862080/`，未复用或覆盖上一轮输出。
Linux Host SHA256 `0f727a861d042757186414b77e7abf027a55aebdde7810f18a60cc1390477c38`；
network `ec3c2356bf026b1684ebf9697f11ea5a09eff65c035d0dbdb89e8115419e4385`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

下一处A按同路径阶段数据定位整链等待，而非继续假定网络字节是主瓶颈。
源码候选为HostChannel整块encode/body_id重复编码、按peer整候选归档恢复，
以及AOEM顺序收尾/packet/I/O工作；先区分排队与实际服务，再选结构性改动，
不直接合入旧归档/诊断草稿或放宽额度/期限。B独立隐私/PQ、S4通用后端待明确
AOEM切仓授权、Execute/多机/容量/部署总目标均保留，未创世、发行或部署。

## 设备 A：服务端增量双向处理，真实写背压仍能转发入站（2026-10-02）

基于`2ea0473`，其远端CI失败事实保留在下一节。本轮只改network七文件及
既有三文档；host11项和legacy38项草稿不混入，AOEM/SDK、业务/nonce/共识、
生产参数均不改。服务端认证后改为单owner增量TLS/WS读写，读写各16KiB量子；
写入受阻时仍解析、鉴权准入和转发反向业务，而非仅把原始字节预读到缓冲。
初始握手仍同步，完整单帧内不能插入另一帧，不称整链全异步或TLS零拷贝。

### 原额度、期限与真实反例

同一时刻只持一个Delivery；原额度guard跨部分写保留到该帧实际TLS密文发送
完成，按FIFO密文字节水位区分后来TLS控制输出。完成计数先于同轮credit；
回复与inbox公平轮转，排队加正在写的回复合计最多64项/1MiB，不因出队提前
归还额度。半写错误永久终止、不重发旧密文。原100ms写等待/10秒帧期限不变，
反向读进展不能续期写停顿；从握手第一字节跟踪半TLS记录，KeyUpdate不制造
虚假半帧期限、也不清掉真实半帧原期限。已消费帧在解析和独立写量子之后仍
检查其原期限，不能因read timer转向后继而把过期帧交给业务。

旧HEAD加同一真实反例的冻结快照`candidate-2ea0473-daemon-duplex-red/`
（树`79317626d887e646f6f51f478f375d3fbc2b5612`）实际FAIL：约703KB Delivery
未完成且原guard仍占1项，socket真实WouldBlock后，反向业务不能到第三接收者，
最终100ms写等待断连。首稿新owner在同条件下约0.196ms完成反向转发，原大帧
仍持有额度；随后精确原文、一次收费、heartbeat/outcome、credit与关闭通过。
这是结构性反例，不声称精确复现GitHub慢runner的全部原因。

新增测试还覆盖坏JSON/越界credit后不继续处理后继heartbeat、额度归还、
TLS close_notify完整尾帧/半帧、真实部分写/未知进度、换钥和原期限。
v1真库Release Windows550+6、Linux549+6通过；但strict Clippy发现两条测试
风格问题，且复审发现上面的已消费帧期限缺口，所以v1不作为最终交付。
两处修复后冻结v2，不覆盖红例或v1原日志。

### 最终快照与整路径验收

暂存树`f362ed6e56a15c6a43d7f5fd48aeb48de3eaf091`导出
`target/runtime-rebuild/candidate-2ea0473-daemon-duplex-v2/`；97个host/AOEM文件
等于HEAD，七network文件等于暂存版本。最终真库Release include-ignored、
串行测试Windows551+6编译拒绝、Linux550+6全部通过，0失败/0忽略；network
分别197/196项。双平台fmt、全targets及host no-native strict Clippy通过；
Windows隔离脚本通过，Linux实际全量Cargo metadata的相同三成员/无legacy
检查通过（PowerShell执行检查，不冒称Linux原生pwsh脚本）。默认Debug全套
本地未重跑，本次远端CI推送后另核，不能提前称前述慢runner故障已根治。

同WSL2/24逻辑CPU、四OS验证进程、真实WSS/E2E与四AOEM RocksDB；1024个公开
测试账户各64笔连续nonce的Ed25519转账，不是用户资金或PQ签名。重负载构建
全部结束后，同一二进制独立运行两次，原120秒门不变：

| 样本 | 唯一最终确认交易 | 四节点全部耐久耗时 | finalized TPS | 全量冷恢复 |
| --- | --- | --- | --- | --- |
| 第一次 | 65536 | 18.385189608秒 | 3564.608329 | PASS |
| 第二次 | 65536 | 18.021523978秒 | 3636.540399 | PASS |

**比上一版3677–3703 TPS略低，不签收吞吐提升。** 每节点64执行/64决定，
execution failure/stale/recompute均0，余额/费用/nonce/全状态根/逐笔回执
冷oracle通过；旧父决定拒绝仍存在，第一轮有archive parent不匹配，不能称
零错误。relay各注册4、替换/过期/拒绝0、停机断开4，实际转发5479/5484帧，
接纳127638883/129474411B；两轮停机active/offline队列均0。这些是有限样本，
不是四台机器/公网稳定容量。计时含节点验签、执行、网络、共识与耐久，不含
钱包预签/启动/创世/冷恢复；不能用AOEM回调峰值24替代主链吞吐。

原96大帧门固定2 CPU三次469.531/478.332/480.416ms通过，无重连/丢失；只是
局部样本。原始双平台日志为`daemon-duplex-v2-clean-workspace-{windows,linux}.log`
和`daemon-duplex-v2-clippy-{windows,linux}.log`，Linux隔离为
`daemon-duplex-v2-isolation-linux.log`。合并命令的WSL循环变量展开导致控制台
日志名称后缀丢失并被后轮覆盖，`daemon-duplex-v2-two-cpu-.log`只留第三轮、
`daemon-duplex-v2-clean-linux-long-.log`只留第二轮；工具输出记录仍有各轮结果，
不把后轮日志冒充完整前轮。两轮独立measurement、四库及各live/recover日志
均保留在v2快照内`target/runtime-rebuild/`：
`controller-load-1024-473-1790935094779685808/`、
`controller-load-1024-1345-1790935132437251086/`，已逐一读回核对。

Host二进制SHA256 `301e77416f0d51224508ddc59edb21467184430a3492c0f2b45f64723e26c126`；
网络二进制`7338a3ccc98ed7823c104f7b75d10948b255a1be7b9bdd49e407b2dda0fde8c1`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

下一处A继续network外层Data/Delivery紧凑载体：实际192KiB原文生成196720B
密文，当前JSON数字数组Delivery约703KB；Host消息/分片及NovoRUDP内层已经
二进制，不重写它们。必须显式版本边界/握手确认，考虑离线队列保存唯一编码
及会话替换，保留实际wire计额、AEAD含义、原窗口和期限；字节减少不等于TPS
同比提高，仍以同一路径四节点耐久及冷恢复测量裁决。B独立隐私/PQ、S4通用
后端待明确AOEM切仓授权及Execute/多机/容量/部署总目标不缩减。

## 设备 A：relay 锁外单次编码与发送额度保留，同路径约3677–3703 TPS（2026-10-02）

**后续远端结果：** 提交`2ea0473`的
[`36988488739`](https://github.com/novovm/supervm/actions/runs/36988488739) FAIL。
Linux真库Release原96大帧双工门175项PASS/1项FAIL，daemon生命周期约756ms
内再次`relay socket readiness wait expired`，真实peer断连，Windows被取消。
日志`target/runtime-rebuild/2ea0473-ci-failure.log`保留；不能把本机通过当作
远端修复。后续服务端增量owner草稿仍待真实反例及整路径验收，原期限不放宽。

基于`c09332d`，仅network六文件及既有三文档。host11项/legacy38项草稿仍未
交付，AOEM/SDK、业务/nonce/共识、生产参数未改。前置CI `36984443097`失败
详见下一节，不能沿用旧全绿声明；本次提交的远端CI须推送后单独核验。

### 实际改动及反例

Data/Offer/Response先在全局会话锁外生成唯一不可变`Box<[u8]>`，编码器沿用
1MiB wire上限；回锁将编码与锁等待耗时计入来源/目标TTL重验，再选择当前
目标session。admission只收费一次，拒绝顺序仍为shutdown/来源/路由/尺寸。
active/offline队列只存编码对象，额度直接来自该对象长度，保留原路由和入站
时间。兼容decoded API在交付时本地解码，不是daemon热路径；暂存编码、TLS/
kernel缓冲不计作已被队列限额包住的总RSS，每物理连接owner至多一个编码任务。

daemon所有inbox出口持原session/global guard至写/flush成功或失败；固定10B
栈头后借用原payload，省去再次JSON编码和整帧拼接。不是TLS零拷贝，两次write
可能增加record；写前重验无法撤回写中替换前已发送字节。错误终止旧连接、不
重发，原100ms单次IO及10秒绝对期限不变；完整末record成功后的deadline错误
仍可能由外层终止检查报告，不声称所有错误在内层立即返回。

旧版锁内编码探针实际FAIL，日志`encoded-lock-red-windows.log`与快照
`candidate-c09332d-encoded-lock-red/`保留（树`44d752e25d812969ef141730b2c967887345626b`）。
新版真实dispatch并发heartbeat/快照/目标替换、编码后失效/过期/shutdown、
V1字节与收费、离线TTL、lookahead/写中替换guard及真实TLS半写故障门通过。
新增真实WSS 8×192KiB出站/7×192KiB入站、收齐后逆序ACK，确实写WouldBlock、
写未完时交付入站、峰值8、credit=7；保留原3秒及250ms poll门。原96大帧门
不改。新版固定CPU0/1同门三次459.025/442.882/493.706ms PASS，对照旧版
518.512/503.508/527.423ms；只是本机局部样本，不据此归因远端失败或整链提速。

### 干净版本验收及同路径结果

暂存树`1523a83aa41003c6f3a671e842d0cd0a13c3d101`导出
`target/runtime-rebuild/candidate-c09332d-encoded-relay-v1/`；97个host/AOEM文件
标准化后等于HEAD，六network文件等于暂存版本。真库Release include-ignored
串行测试：Windows531+6编译拒绝、Linux530+6，均0失败/0忽略；实际network
分别177/176项，Windows独立target重编译。双平台fmt、全targets/host no-native
strict Clippy通过；Windows隔离脚本通过。Linux缺少pwsh的原127退出记录保留，
改用真实Linux全量Cargo metadata在PowerShell核验相同三成员/非legacy约束，
通过；不是声称Linux原脚本已运行。本轮默认Debug整套未本地重跑。

同WSL2、24逻辑CPU、四OS验证进程、一真实WSS/E2E relay和四AOEM RocksDB；
1024个公开测试账户各64笔连续nonce的Ed25519签名转账，无并行构建/重负载，
同一二进制两次独立运行，原120秒门不变：

| 样本 | 唯一最终确认交易 | 四节点全部耐久耗时 | finalized TPS | 全量冷恢复 |
| --- | --- | --- | --- | --- |
| 第一次 | 65536 | 17.822761600秒 | 3677.095698 | PASS |
| 第二次 | 65536 | 17.696364915秒 | 3703.359437 | PASS |

样本略低于上轮3727–3882 TPS，**没有吞吐提升签收**；不能因减少复制就推导
整链已更快。每节点64执行/64决定，execution failure/stale/recompute均0，
余额/费用/nonce/全状态根/逐笔回执冷oracle一致。sticky旧父决定拒绝仍存在，
第二轮还有archive parent不匹配；不是错误事件计数，也不称零错误。relay各
注册4、替换/过期/拒绝0、停机断开4；转发5465/5400帧，接纳128796393/
127395143字节。第二轮最终report仍有离线3项/7490B、active为0，保留此事实，
不称停机队列全空。EOF日志保留。计时含节点验签、执行、网络、共识与耐久，
不含钱包预签/启动/创世/冷恢复；不是实际用户、PQ、四台设备或稳定公网容量。

Host二进制SHA256 `a8ba4506dc3763ba0263068303c07d7eebd4e1a4225eaacb8be77bc5e34e2ff0`；
网络二进制`34693b827e615ab333399845a78aa4b63093f01fa9f4babb6f7be071d9c5d6fb`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
日志在`target/runtime-rebuild/encoded-v1-{clean-workspace,clippy}-{windows,linux}.log`、
`encoded-v1-isolation-linux.log`、`encoded-v1-two-cpu-duplex-{one,two,three}.log`、
`encoded-v1-clean-linux-long-{one,two}.log`。快照内`target/runtime-rebuild/`
的`controller-load-1024-422-1790932005864909045/`与
`controller-load-1024-429-1790932080148028044/`保留measurement和relay/report。

下一处A继续network daemon/IO/client：拆服务端整帧阻塞写，真实背压时仍推进
入站；保持原guard、半写offset、绝对期限、credit与关闭语义。外层密文JSON
数字数组膨胀另取实际Data/Delivery字节证据，内层Host/NovoRUDP已二进制，不
重做其wire。B独立隐私/PQ、S4待明确AOEM切仓授权及Execute/多机/容量/部署
总目标不变；本次没有创建Skill/分支、正式创世、发行或生产部署。

## 设备 A：有界多在途转发与增量 TLS/WS，同路径两轮约3727–3882 TPS（2026-10-02）

**后续远端证据：** 交付提交`c09332d`的
[`36984443097`](https://github.com/novovm/supervm/actions/runs/36984443097)最终FAIL。
Linux真库Release的`real_wss_three_worker_large_duplex_fanin_crosses_delivery_windows_without_loss`
在daemon约345ms生命周期内写就绪等待预算耗尽，导致hub连接重置及原会话不变
断言失败；160项网络PASS、1项FAIL，Windows矩阵取消。这不是status争锁空值，
也不是10秒帧期限到期。日志`target/runtime-rebuild/c09332d-ci-failure.log`保留。
同一旧版Linux网络二进制`08c3a61ec3416f7c443aa8c36667c471159abc11f07aefea0ab73b65af843f01`
在本机taskset固定CPU0/1、无并行构建的同一门三轮均PASS，精确数据阶段
518.512/503.508/527.423ms；日志`c09332d-two-cpu-duplex-baseline-{one,two,three}.log`。
不能以这些PASS抹去CI失败或认定锁为其根因；锁外编码改造尚待另行完整验收。

基于`6312da9`，其双平台CI
[`36980054724`](https://github.com/novovm/supervm/actions/runs/36980054724)成功。
只交付network九文件和既有三文档；host11项/legacy38项草稿不混入，AOEM/SDK、
业务/nonce/共识、生产参数和分支未改。本次远端CI须推送后另核。

### 实际交付与边界

已认证连接的常驻owner显式调用rustls非阻塞读写；每poll有界推进读和写，只有
没有进展才等待Mio就绪，不再借StreamOwned隐藏等待或先写N帧再读。最多8个
转发元数据项和1个正在写的WS帧；TLS配置64KiB写缓冲另有有限record开销，
不是声称全部内存严格64KiB。没有新增明文应用队列；原队列原文继续计额。
Delivery实际交给worker才累计归还credit，解码缓存不算消费。原frame/写入/
outcome/heartbeat期限独立保持；原chunk级时间跟踪不冒充精确首字节时间。

Data按source/target/session/sequence/bytes/flags验证并映射至稳定条目和预留号，
不是收到回执就弹队首。乱序、拒绝后已接纳后继、TTL已过的在途项、旧会话迟到
回执均逐项结算。序号在加密提交前递增、绝不回滚。未知部分写关闭旧TLS；保留
原文原入队时间，重新鉴权后再加密，不承诺网络exactly-once。握手wire没有独立
请求号，限制为最多一个握手在途；不能识别恶意relay伪造的同路由同长度旧ACK。
初始连接仍同步，daemon仍整帧写；这是发送流水线，不是整链全异步或证明最终性。

真实WSS服务端收到7个Data前扣住所有ACK：旧同步路径3.26秒FAIL，增量路径
约0.81秒PASS并逆序结算。真实worker门确认原7条/224B在ACK前仍计额，同时
可收7条反向E2E原文，逆序ACK逐项清空；错误/重复ACK、未知半写与close_notify
完整末帧/半帧门通过。原四路由96×192KiB双工、跨窗口与重连门保持通过。

**保留失败：** v1干净Linux全套160网络项通过、1项失败：TLS建连后两端接收窗
强缩4KiB，3秒只收到首大帧64–96KiB；ss显示接收窗限制与重传，客户端仍实际读，
不是已证pump停止读取。仅fixture接收改64KiB、发送仍4KiB，原3秒、实际write
WouldBlock、出站未写完时交付入站、7×192KiB精确原文/credit/outcome门全部保留。
接收窗单变量A/B Linux三轮0.50/0.54/0.50秒，Windows0.36/0.37/0.42秒通过；
清除临时诊断后Linux0.50/Windows0.36秒通过。不宣称4KiB Linux已通过或精确
归因所有TCP机制，未修改生产缓冲/期限。另修复真实TLS close_notify空转边界。

### 干净版本验收与完整交易测量

v2暂存树`501e732780bcf7ad26b32e75b6a3de5c78f9f4f6`导出到
`target/runtime-rebuild/candidate-6312da9-async-relay-v2/`；97个host/AOEM文件
标准化后等于HEAD、九network文件等于暂存版本。完整真库Release串行include-
ignored：Windows516+6编译拒绝、Linux515+6，全部0失败/0忽略；双平台fmt、
全targets strict Clippy、host no-native strict Clippy及三成员隔离检查通过。
Windows独立target重建、Linux日志实际重建三crate；本轮默认Debug整套未重跑。

同WSL2/24逻辑CPU/四OS进程、真实WSS/E2E/四AOEM RocksDB；1024个公开测试
付款账户各64笔连续nonce转账，每轮65536笔不同Ed25519签名的测试交易。
无并行构建/重负载、原120秒门不变，同二进制两次独立运行：

| 样本 | 四节点全部耐久耗时 | finalized TPS | 完整冷恢复 |
| --- | --- | --- | --- |
| 第一次 | 16.883561685秒 | 3881.645427 | PASS |
| 第二次 | 17.585384942秒 | 3726.731045 | PASS |

与上一版3716–3824 TPS接近，不宣称显著提速或稳定容量。每节点64执行/64决定，
execution failure/stale/recompute均0，完整余额/nonce/费用/状态根/逐笔回执冷
oracle一致。sticky last_error仍有旧父决定拒绝，第二轮还有archive parent不匹配；
不能把sticky字段当事件计数或称零错误。relay各注册4、替换/过期/拒绝0，停机
各4断开；转发5385/5426帧、接纳129168826/129327911字节，停机EOF日志保留。
计时包含节点验签/执行/网络/共识/四节点耐久，不含钱包预签/启动/创世/冷恢复；
不是实际用户/生产币、PQ、固定出块周期、四台设备、公网容量或主网完成声明。

二进制SHA256 `28f75a208d1165bb24f732f76af20102b4bb762d256b1cf1ef3c9ab478693259`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
证据在`target/runtime-rebuild/`的`pipeline-v2-clean-workspace-{windows,linux}.log`、
`pipeline-v2-clippy-{windows,linux}.log`、`pipeline-v2-clean-linux-long-{one,two}.log`。
上述v2快照内`target/runtime-rebuild/controller-load-1024-408-1790929501622737248/`
与`controller-load-1024-1292-1790929551287115796/`包含measurement及relay/report。
旧反例`new-client-pipeline-first-outcome-red-windows.log`、v1全套失败
`pipeline-clean-workspace-linux.log`、`new-client-pipeline-fixed-buffers-*-diagnostic-linux.log`
以及`new-client-pipeline-recv-window-ab-{linux,windows}-{one,two,three}.log`保留。

下一处A认领relay/daemon及相邻测试：全局state写锁内将Delivery克隆/编码仅取
长度、发送时再次编码，需以不可变绑定产物消除重复遍历并移出共享锁；保持V1
wire、重新入锁会话验证、完整内存额度和原guard。外层JSON密文约3.57倍膨胀与
控制消息队头阻塞仍未解决，不凭代码观察推断TPS。B独立隐私/PQ分工不变；S4
通用后端待明确AOEM切仓授权，Execute、多机/容量/部署等完整总目标继续保持。

## 设备 A：worker 出站真实读唤醒，同路径两轮约3716–3824 TPS（2026-10-02）

基于`0a0eda8`，其双平台CI
[`36976971751`](https://github.com/novovm/supervm/actions/runs/36976971751)已成功。
本次仅network四文件及既有三文档；host11项/legacy38项草稿保留不提交，
AOEM/SDK、经济/nonce、共识、生产参数与分支不变。本次远端CI推送后另核。

入队和当前连接读waker的安装共用原队列锁，实际通知在锁外。替换/断连先
清注册，旧通知不能替换新连接；已接纳原文仍占原队列。burst后只对实际Active
且含未过期工作的peer重新通知，避免合并通知被outcome读取消耗后又空等；
Idle/Cooldown/纯过期队列不自旋。普通空闲读的shutdown亦被唤醒；不承诺
取消进行中的write/outcome/部分帧，也未缩短其原期限。测试观察器只记录真实
WouldBlock到Mio Poll窗口，不注入延迟或伪造IO就绪，不称精确内核阻塞瞬间。

真WSS已握手后设置合法1秒idle，观测实际读等待窗口再入队：旧unpark路径
1.001678秒，违反250ms测试门；修后5.27/5.61ms。24条FIFO跨原8次burst约
6.05ms，原文和字节精确、无丢失/重复/重连；普通空闲shutdown约0.30ms。
真实会话替换、迟到旧waker、新waker与已缓冲delivery先于唤醒处理专项通过。
这些是局部唤醒证据，不能单独推出主链吞吐或恒定时延。

### 干净版本的验收

导出0a0eda8，只应用这四文件；97个host/AOEM源文件经换行标准化等于已提交
版本，没有归档/诊断草稿。完整真库Release串行include-ignored：Windows
492单元/集成+6编译拒绝、Linux491+6，均0失败/0忽略；两平台全targets及host
no-native strict Clippy、fmt、隔离检查通过。本轮未重跑默认Debug整套，不借
上一提交的Debug结果声称本轮已测。Windows首次共享target输出仅134项旧网络
测试、未真实编译新代码，明确排除；独立`target/windows-verified`重编译后
实际执行138项（Linux137），上述492/491统计只来自有效重建。

同一WSL2/24逻辑CPU/4 OS验证进程、真实WSS/E2E/4 AOEM RocksDB，1024公开
测试付款账户×64次连续nonce转账，共65536笔唯一Ed25519签名测试交易。
同二进制两次独立运行，无并行构建/重负载，原120秒门不变：

| 样本 | 四节点全部耐久耗时 | finalized TPS | 完整冷恢复 |
| --- | --- | --- | --- |
| 第一次 | 17.140182755秒 | 3823.529827 | PASS |
| 第二次 | 17.634378767秒 | 3716.377019 | PASS |

每节点64执行/64决定，execution failure/stale/recompute均0，四头及完整
余额/nonce/费用/状态根/逐笔回执冷oracle一致。last_error仍含旧父决定拒绝，
第一轮另有vote context mismatch；sticky字段不等于事件计数，不能称零错误。
relay两轮注册4、替换/过期/各项拒绝0，终止时各4断开；转发5351/5406帧，
接纳129041366/127436389字节，终止日志仍保留。钱包预签、启动、创世、冷恢复
不计TPS；节点验签、执行、网络、共识和四节点耐久计入。有限积压而非稳定到达，
不是PQ、固定出块周期、实体四机或公网容量，未宣称完整高性能主网。

同二进制SHA256 `b544f9036dbe0df3294446cdfbbcdd457048ad515d766483865458778172a301`；
Linux AOEM `88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
证据在`target/runtime-rebuild/`：`new-worker-wake-arrival-red-windows.log`保留
旧反例；`new-worker-wake-isolated-workspace-windows.log`及
`new-worker-wake-clean-workspace-linux.log`为有效全套；
`new-worker-wake-clean-linux-long-{one,two}.log`为长负载。
快照`candidate-0a0eda8-worker-read-wake/target/runtime-rebuild/`下
`controller-load-1024-438-1790926278660239683/measurement.json`与
`controller-load-1024-418-1790926351390264326/measurement.json`及各自relay报告
保留原始结果。旧缓存`new-worker-wake-clean-workspace-windows.log`不计入本轮。

下一处A拆worker/client逐帧outcome阻塞，落实有界多在途、原文计额、稳定条目
身份、TTL/nonce及精确应答关联；必须真实交替推进读写，不能单纯先写N帧再读。
握手缺独立请求号、旧代迟到结果、拒绝后已发后继及未知部分写均纳入测试。
host草稿继续不混入，B独立隐私/PQ分工不变。S4通用证明后端待明确AOEM切仓
授权；Execute、隐私/PQ、真实多机、稳定容量和部署仍属未完成总目标。

## 设备 A：网络队列唤醒及有界双工修复，同路径三轮约3229–3498 TPS（2026-10-02）

基于`ad3170e`，其双平台CI
[`36971756089`](https://github.com/novovm/supervm/actions/runs/36971756089)成功。
本次仅交付`runtime/novovm-network`七文件及既有三份交接文档；host归档/诊断
11项草稿与legacy旧38项保留未提交。未修改AOEM/SDK、经济/nonce、共识签票、
生产参数、Skill或分支。以下是本机验收；本次远端CI在推送后另行核验。

### 修复的是实际等待边界

- 两平台统一非阻塞Mio等待，原读/写时间预算不变；inbox的data/control队列
  注册真实读唤醒，lookahead继续持有原配额guard，替换会话不得偷放额度。
  可读字节/EOF/真实错误先于队列通知；写不被通知中断。
- relay显式协商`DeliveryWindowV1/DeliveryConsumedV1`，最多15项未消费投递；
  客户端真正向调用方取出7项后汇报累计水位。等待outcome时暂存不算消费，
  重复水位不增加额度，倒退/越界拒绝，heartbeat/新请求/tick不重置窗口。
  **这是relay传输扩展，须两端同步升级，旧端失败关闭，无静默兼容降级。**
  它不是链/交易版本，也不是应用处理、耐久或最终性ACK。
- 同一个连接owner在真实write WouldBlock时也推进对向原始TCP/TLS字节；
  认证及协商后才启用，按需最多68×16KiB=1088KiB。已消费chunk不追加；
  满额/EOF不再订阅READABLE，成功写入进度不被后续错误覆盖。原未完帧期限
  不因心跳、写完成或新预读重置；时间跟踪为原始chunk级，不冒充所有TLS/WS
  帧首字节精确时间。默认512物理连接若全分配，额外载荷上限544MiB加有限元数据。
  此项明确增加有限传输缓冲，但未放大原业务/明文事件队列或放宽超时。
- worker按最多8次/原read_idle时间双预算连续处理待发原文，保留公平、TTL、
  单帧精确outcome相关性、nonce及未知写结果终止规则；仍非全异步发送。

### 先失败再修复，未用TPS掩盖功能反例

真实TCP对照：不接队列waker时客户端1秒期限失败，接上后打断5秒空闲读并
获得可解密原文。脚本socket+真实加密的40×200KB堆积专项验证15项累计上限、
延迟outcome、重复水位和剩余原文；此脚本专项不冒充真实大帧双向网络。

首稿wake+credit在64高曾19.086185秒/3433.687778 TPS，但真实WSS三worker
大帧双向测试两次发生原100ms写等待超时（当轮全库125通过/1失败），因此未
签收。Linux固定小socket窗口双向1MiB写反例原实现0.11秒失败，有限预读后
0.01秒通过；Windows同raw fixture原本可通过，不制造跨平台先红证据。
最终真实WSS/E2E四条路由×24条×192KiB，96条全部精确有序，跨越投递窗口，
原有队列/TTL/20秒测试期限不变，无重连/丢弃/过期；独立Windows网络轮约515ms。
旧FAIL、先前主链120秒FAIL与退化样本均保留，不用新结果覆盖。

### 干净版本的同一路径测量

机械导出ad3170e（运行host仍035004d），只移植network七文件；97个已提交
host/AOEM源文件经CRLF标准化核对相等，排除了未签收归档与诊断草稿。两次
独立运行同一二进制，测量期间没有并行构建/重负载测试；原120秒门不变。
拓扑为同一WSL2主机、24逻辑CPU、4验证节点OS进程、真实WSS/E2E及4个真实
AOEM RocksDB。1024个公开测试账户各连续64笔，每笔实作Ed25519签名及验签，
共65536笔不同**测试转账**，不是实际用户或生产币，也不乘四虚增TPS。

| 样本 | 四节点全部耐久65536笔 | finalized TPS | 完整冷恢复 |
| --- | --- | --- | --- |
| 本轮未改运行代码基线 | 98.331516秒 | 666.480116 | PASS |
| 最终网络修复第一次 | 20.297235秒 | 3228.814208 | PASS |
| 最终网络修复第二次 | 18.736891秒 | 3497.698769 | PASS |
| 最后仅测试断言修正后的最终版本 | 19.907676秒 | 3291.996572 | PASS |

基线既往还测到57.437/63.950秒（1141/1025 TPS），波动不能隐藏，不将单次
98秒样本宣传成稳定倍数。新两轮均64高、65536成功，4头/状态根/回执/完整
余额、nonce及费用冷oracle相符；每节点64执行/64决定，执行失败/stale/重算0。
但`last_error`四节点均仍记录`decision not for exact current parent`；逐高度
快照另见vote/proposal/archive等上下文不匹配拒绝。它们是sticky最后拒绝信息，
不是拒绝事件计数，不得写成“零错误/零上下文拒绝”，本轮没有定量归因。
relay两轮初始注册均4、无替换/过期及source/aggregate/queue/protocol拒绝；
终止时正常断开4个连接，无TLS close_notify日志保留，不能写成无断连日志。

计时不含生成密钥/钱包签名、启动、创世和冷恢复；包含正文构造、验签、执行、
WSS/E2E、BFT与全部节点耐久确认。全部有限积压一起释放，按高度采样，不是
稳定到达流延迟，不是PQ性能、固定出块周期、四台实体设备或公网稳定容量。
两轮测量二进制SHA256为
`1936ae621d587ea8885690f3b150f96b9efc0ba69ffcbb5c46d8f47b4a894f80`；
测试断言修正后的第三轮最终版本SHA256为
`d19601a8ff1c3fb8581dd2bfaf07a7f97b8b953e72dcd4b8ee36402f61ceb16e`，
同样65536笔四节点耐久及完整冷oracle通过，不与前两轮冒称同一二进制。
Linux AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

### 验证与下一处恢复入口

干净版本完整真库Release串行包含ignored：Windows488项+6编译拒绝、Linux487+6，
0失败/0忽略；差一项为Windows专属OS超时分类。两平台strict Clippy（全targets
及host无native模式）、fmt和3成员legacy隔离检查通过。默认Debug通过但会跳过
显式真库/时序门，不能把该跳过当通过。最后一处仅测试断言的Clippy修正不改
上述测量生产路径，随后完整Release复跑；没有随测量修改源文件。

原始证据在本机`target/runtime-rebuild/`，未把大体积artifacts推送：
`network-wake-credit-final-windows.log`保留首稿FAIL；
`network-duplex-read-ahead-{red,green}-linux.log`为raw双工反例；
`network-duplex-final-verified-workspace-{windows,linux}.log`为最终全库；
`network-duplex-clean-{debug-windows,debug-linux,linux-long-one,linux-long-two}.log`；
`candidate-ad3170e-network-duplex-final/target/runtime-rebuild/`下
`controller-load-1024-430-1790924241678803613/measurement.json`及
`controller-load-1024-418-1790924330084568624/measurement.json`对应两轮最终负载。
第三轮见`network-duplex-final-verified-linux-long.log`及同快照下
`controller-load-1024-436-1790924786393775834/measurement.json`。
基线见`network-wake-baseline-linux-long.log`及
`baseline-ad3170e-network-wake/target/runtime-rebuild/controller-load-1024-539-1790922475529193414/`。

A下一处是worker出站真实IO唤醒：现`thread.unpark`不能中断Mio Poll，入队及
burst剩余可执行工作需要同锁安装/检查当前连接waker，不能因通知合并又空等。
再拆逐帧outcome等待，必须保留在途原文计额、TTL、nonce和精确相关性；不以
先写完全部消息冒充安全流水线。A仍持有host11项草稿；B独立隐私/PQ不覆盖。
S4真实证明通用后端待明确AOEM切仓授权，Execute、隐私/PQ、真实多机、稳定
容量与部署入口仍未完成；正式创世、发行与生产部署仍须另行授权。

## 设备 A：真实阶段诊断与网络空等反例，运行草稿继续未签收（2026-10-02）

前一证据文档已推送`d527ad9`，其双平台CI
[`36969824273`](https://github.com/novovm/supervm/actions/runs/36969824273)成功。
**本次仍只提交文档；已交付运行代码仍035004d。** 下列诊断、归档与发送burst
是本机草稿，不是另一台已拉到的新能力；不把新PASS覆盖上节120秒FAIL。
未改AOEM/SDK、legacy、Skill、分支、经济/共识或生产参数，目标保持完整。

### 实际测量改变了下一步

测试专属`NOVOVM_LOAD_PROFILE=1`记录20个固定阶段的完成次数/同步服务墙钟
累计与最大值，复用原100ms观察器及逐head文件；默认关闭，无逐poll文件写。
计时包括等待OS调度，不是CPU时间；不包含尚未完成操作，跨线程/嵌套值不能
直接求和当关键路径，也没有将队列等待混称AOEM计算。三个计数/溢出/枚举测试通过。

同一WSL2主机、24逻辑CPU、原4进程WSS/E2E/4真实AOEM库、1024笔×64高：

| 诊断开启的源码 | 四节点耐久65536笔 | TPS | 每节点compute_execute服务 | 每节点controller_poll服务 | 候选恢复次数/节点 |
| --- | --- | --- | --- | --- | --- |
| 当前共享归档草稿 | 56.756990秒 | 1154.677166 | 3.706–3.773秒 | 0.350–0.355秒 | 71–73 |
| 导出035004d加完全相同诊断 | 58.205762秒 | 1125.936641 | 3.704–3.743秒 | 0.389–0.411秒 | 143–153 |

两轮全部冷恢复经济oracle通过，各节点64执行/64决定，无失败/stale/重算；
诊断表明共享确实减少恢复调用，但已完成的同步服务并未占满整个观察窗口。
它**不证明共享稳定提速，也不证明网络是唯一根因**；此前无诊断的646/803/1240
样本仍保留，不能用这对诊断运行作无扰动统计签收。首个诊断命令误用短名
`--exact`实际0测试，未计入结果；随后去掉exact确实执行1项64高测试。

诊断草稿二进制SHA256
`edfeeff78a079c9f1f8394545577e14699c6d88cf6d2f3701a96018ca20d473c`；
035诊断导出二进制`c4d805cc74dcf4a9a0e9af8e390db897bb85bbd61995bf0973e4e1862b0b89f1`。
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

### 已证发送空等，不等于整链性能修复

当前worker每轮仅发一条，然后即使仍有排队正文也进入recv_event。真实双端
WSS/E2E专项将合法read_idle设250ms，完成握手后接纳8个不同正文：旧实现
1秒内只获得4/8个relay接纳，之后8个原文均精确解密，真实先红而非造丢包。
草稿按最多8次/原read_idle墙钟双预算继续发送，遇pending入站或stop立即让出；
每包仍复查TTL、公平选peer、等待原匹配outcome，失败隔离且不复用已使用nonce。
不修改协议/队列额度/原绝对超时。Windows原反例后绿；最终network全库
Windows115、Linux112项Release通过，0失败/0忽略，含双向/重启/配额/TLS失败。
该时序专项默认ignored，正式CI已有release `--include-ignored --test-threads=1`
会实际执行；不能用默认跳过当通过。不是去掉每帧outcome的全异步网络。

为排除host归档草稿，另机械导出d527ad9的已提交runtime，仅移植上述network
两个文件（LF标准化逐文件相等，无诊断/共享归档/新分支）。真实64高结果仍为
**78.748443秒/832.219629 TPS**，65536笔四节点耐久确认及完整冷oracle通过。
四节点各64执行/64决定、round0，无失败/stale/重算；relay166721826字节、
排队3、队列/source/rate拒绝0，初始注册4、无替换/过期。二进制
`4889291e78025a8120ef430927bdf36b412afd44953a7d066a12e17d4abccb53`。
局部延迟回归修复不能代替主链提速：本轮不提交这项运行草稿为已接受实现。
本轮fmt、workspace all-target strict Clippy、无native strict Clippy和3成员
隔离检查通过；不是对新增诊断后的整个host再跑全库，旧全库范围见上节。

### 下一处唯一认领及恢复入口

A保留13项本机runtime草稿：原host归档6项，加channel/lib/persistence/io/
pipeline/compute/diagnostics共5项，加worker及其tests两项；旧38项仍不改。
继续同一链路的读写就绪/队列唤醒边界：daemon连接先阻塞读，普通输入后只
转发一个inbox项，空闲100ms后才批量排出；多peer扇入可能使投递依赖目标的
反向发送节奏。**这是源码确定的组织方式，尚未量化为78.7秒的唯一原因。**
下一步必须以真实等待/扇入反例验证并解耦，保留ACK先行、有限pending容量、
双向公平、会话代际、TLS部分写终止及绝对期限；不能只缩短timeout或增加缓存。
S4通用后端仍待明确AOEM切仓授权；Execute、隐私/PQ、真实多机、稳定容量及
部署入口均未完成，正式创世/发行/生产仍需另行批准。

本机证据根为`target/runtime-rebuild/`，未随文档推送原始artifacts：
`controller-archive-profile-linux-run.log`及`controller-load-1024-434-1790919881032114356/`；
`controller-035-profile-linux-run.log`及`baseline-035004d-profile/target/runtime-rebuild/controller-load-1024-531-1790920019298103523/`；
`network-burst-red-windows.log`、`network-burst-green-windows.log`、
`network-burst-final-windows.log`、`network-burst-final-linux.log`；
`network-burst-clean-linux-long.log`及`candidate-d527ad9-network-burst/target/runtime-rebuild/controller-load-1024-576-1790920503905873064/`。
另一台只能据此了解状态；草稿/原始证据未推送，不冒充远端可直接复跑。

## 设备 A：共享历史回复试验未签收，保留性能退化证据（2026-10-02）

基于`035004d`；该提交的双平台CI
[`36965164287`](https://github.com/novovm/supervm/actions/runs/36965164287)已成功。
**本次仅提交文档。以下runtime改动是本机未提交草稿，不是GitHub已交付能力。**
不改AOEM/SDK、旧38项草稿、业务/费用/nonce、wire、签票规则或生产参数；
没有创建Skill或分支。A继续独占controller及负载测试，B不覆盖这些本机草稿。

- 同一历史高度的请求共用一个任务，贯穿完整ArchiveRead、StoredBody准备和
  Decision准备；准备途中加入的peer不重新读取/组装。首个请求的未可信父点
  不作为任务身份，恢复依然依据本地已验证耐久前缀；回复逐peer匹配完整context。
- 只共享本channel的不可变PreparedMessage；不缓存StoredCandidate、BatchRequest
  或执行/发布权限。Ready及最后共享根交owner回收；job正文计入prepared正文
  预算，job Decision计入固定槽位。已有缓存命中无需再次恢复/编码。
- 每peer最多一个已接纳请求，改变高/低高度只更新最新请求、不取消其他人的
  任务。按FIFO逐peer分发，重入排尾。RequestDecision只唤醒证书，不主动发
  正文，也不关闭已由该peer RequestBody唤醒的正文重试。
- **先红后绿：** 共享任务初稿从BTreeMap头挑peer，真实库反例0.32秒失败：
  字典序靠前的错误父点peer重入后连续获两次分发，已排队合法peer仍未获回复。
  FIFO修复后三个真实库专项全过（Windows合计0.99秒），不是把旧已发布版本
  与尚未提交的共享初稿混为一谈。
- 最终四个专项各使用4个真实AOEM库、2个实际执行/持久投票高度，不预造QC。覆盖
  1次恢复/1次正文/1次证书准备、共享Arc、两个准备阶段late join、错误父点、
  持续错误重入、升降高度、按需正文及重查不取消正文；头/签票/outbox元数据
  前后相等，查询不产生新执行或票。这些专项是进程内投递，不冒充真实网络。
- 追加两个反例也实际先红：正文在job已准备、Decision尚未分发时，真实RequestBody
  被丢弃；固定缓存171/173槽时，最后回复因Decision预留双计只能到172、缺证书。
  修复为按精确body_id立即服务已准备正文、最后waiter移交预留槽；四专项最终
  1.31秒全部通过。满槽夹具使用真实owner准备的无权限RequestBody占位，不造QC、
  不改额度；它不是prepare队列背压或实际网络容量证明。
- 共享初稿全量真库Release：Windows **471单元/集成+6编译拒绝**、WSL Linux **468+6**，
  均0失败/0忽略。fmt、全targets strict Clippy、无native strict Clippy及
  3成员无legacy依赖检查通过。四进程WSS/晚到追赶/强杀恢复等原门保留。
- 最终两处边界修复后的全量真库Release再次通过：Windows **472单元/集成+6编译拒绝**、
  WSL Linux **469+6**，均0失败/0忽略。它们覆盖最终草稿的正确性回归，
  不改变下面长负载性能未签收的结论。

同机WSL2 `/mnt/d`、24逻辑CPU、四OS进程WSS/E2E、1024笔×64高的首轮
**120.50秒FAIL**：四节点共同只到51，另外三节点到59，未执行冷恢复签收；
H49/53/57有换轮，执行失败/stale/重算为0。relay排队4716帧、数量拒34、字节
拒3，admitted约261MB，无source/rate拒绝和中途重连。不能用全量短门覆盖它。
只添加超时才求值的测试诊断后，同生产代码复跑为62.602105秒、1046.865764 TPS，
65536笔四节点耐久确认及冷恢复通过；256快照round0、每节点64执行/64决定，
relay155624662字节、排队2、队列/source/rate拒0。该PASS不证明首轮失败已修复。
诊断版二进制SHA256
`ff8a8d73e1b8b71fe3a271ca7187874bdb4049c3996f5168300bdbdca3bdd3e3`。

随后在`target/runtime-rebuild/baseline-035004d-archive-check/`机械导出已提交
`035004d`源码，未建分支/覆盖工作树，以同一AOEM库和负载独立对照：
52.844171秒、1240.174633 TPS、冷恢复通过。目录变化会改变嵌入路径/二进制，
不冒充同一二进制。已找到共享初稿重复命中额外排入Prepared回收的具体差异，
改成原地wake，并补无新增回收断言。此版独立长测81.585976秒/803.275311 TPS；
再补提前正文与槽位移交后的最终草稿101.399229秒/646.316550 TPS。两轮都是
65536笔全部四节点耐久确认且冷恢复通过，但不能据此说修复了首次失败、
提高了吞吐或适合合入。最终草稿Linux二进制SHA256
`73c017d0d5817570640e7896a5bcd28e5fc761114dddb5017e9db203fda8c656`，
AOEM保持`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
这些不是同一二进制的重复样本，不作统计显著性或唯一根因声明。

最终草稿前56高已耗时71.923秒，最后8高29.477秒；H61四节点各换到round1并
多执行一次，最终各65执行/64决定，执行失败/stale/业务重算为0。因此不能把
全部退化归因于单次换轮。relay最终排队1528帧、数量拒14、字节拒3、
约226.128MB admitted数据；同机基线排队4、无队列拒绝、
约147.593MB。无source/rate限流和中途重连。阶段/队列开销仍须直接测量，
不能据这些相关现象断言AOEM执行慢或网络是唯一原因。

**后续A认领：** 先在既有真实负载中记录正常完成/停顿时的实际队列、历史查询
和阶段代价，与035稳定基线对照；必要时重做此归档调度草稿，不恢复legacy，
不再凭减少操作数断言性能改善。未找到并验证退化原因前，不提交该运行实现。

明确剩余边界：首次cache miss仍完整恢复候选/准备正文，没有持久body_id可以
安全替代它；不以candidate_id或document digest偷换transport body_id。
`preparing.request=StoredBody`在channel背压期间仍只受既有任务数/单候选边界，
尚不计入prepared正文缓存字节总额；本轮没有新增prepare满额专项，不宣称全部
pending内容已有统一内存上限。长账本稳定容量、S4真实证明、Execute、隐私/PQ、
实体多机与部署仍未完成。AOEM通用后端升级继续等待明确切仓授权。

本机原始日志在`target/runtime-rebuild/`：`controller-archive-fifo-red.log`、
`controller-archive-sharing-windows.log`、`controller-archive-windows-release.log`、
`controller-archive-linux-release.log`。首轮失败日志`controller-archive-linux-long.log`，
目录`controller-load-1024-426-1790917454981326679/`；诊断复跑日志
`controller-archive-linux-long-diagnostic.log`及目录
`controller-load-1024-496-1790917754624403875/`。基线日志
`controller-archive-035-baseline-long.log`，measurement在上述隔离目录下的
`target/runtime-rebuild/controller-load-1024-578-1790917940977004409/`。
原地wake版日志`controller-archive-linux-long-hotwake.log`、目录
`controller-load-1024-491-1790918134946765959/`；最终草稿日志
`controller-archive-linux-long-final.log`、目录`controller-load-1024-499-1790918656940993864/`。
两个新增反例在`controller-archive-boundaries-red.log`，最终专项绿灯在
`controller-archive-sharing-final-windows.log`；最终双平台全回归分别在
`controller-archive-final-windows-release.log`和`controller-archive-final-linux-release.log`。
这些原始文件是本机target证据，
未随文档提交到GitHub；另一台机器须自行重跑，不能把不存在的远端artifact当证据。

## 设备 A：持续控制消息不能饿死真实交易执行（2026-10-02）

基于`fac0e84`；其双平台CI
[`36963188460`](https://github.com/novovm/supervm/actions/runs/36963188460)已成功。
本轮仅新controller、两处测试及既有交接文档；无AOEM/SDK/旧38项/Skill/分支改动。

- **先红：** 实际AOEM、真实签名body、真实HostChannel验证的远端nil prevote，
  仅固定每poll控制消息到达时机。旧调度在60.11秒失败：`retained_bodies=1`、
  `inflight=0`、`executed_batches=0`、`durable_votes=0`。固定协议时钟，不能
  用超时nil票冒充执行进展。原二进制SHA256
  `8297ea964e11b10dabe509ce78da704ececdaeb7eca775fdae5d3c46e401c7d4`。
- **修复：** ACK与执行完成先处理，然后在新入站控制消息之前调度已有执行、
  归档和缺体查询；保持原次数/预算及真实回收背压，不循环等待或在控制线程
  销毁正文。入口captured recovery门保留；已收证据仍先drive consensus再
  pacemaker。刚收到body/offer要等下一poll；不承诺所有队列事件处理顺序不变。
- **后绿：** 默认32和1事件预算都真实执行、持久化本地非nil prevote；重复
  远端票不增权，2/4没有head。重开AOEM日志验证原签名/域/阶段。原cold控制流
  回归也过，专项3项共0.43秒。不是WSS吞吐测试或执行有效性密码证明。
- 全量真库Release Windows **468单元/集成+6编译拒绝**、WSL **465+6**，均
  0失败/0忽略；fmt、strict Clippy、无native检查和3成员隔离检查通过。

修前独立WSL同机四OS进程、1024笔/批×64高度：65536笔唯一耐久最终确认，
79.611634秒、823.196269 TPS，冷恢复完整经济oracle通过。四节点每高仅执行
一次、全部round0，无失败/stale/重算；首8高6.646秒、末8高14.346秒，仍有
随高度变慢。此前116.253秒包含一次换轮，不能将两个旧运行之差算作本轮优化。
不改120秒期限、网络限额或业务内容。修后同一二进制独立两次实测：

| 65536笔 / 64高 | 四节点全部耐久确认 | 唯一最终确认TPS | 完整冷恢复经济oracle |
| --- | --- | --- | --- |
| 首轮 | 57.437175秒 | 1141.003198 | PASS |
| 独立复跑 | 63.949987秒 | 1024.800838 | PASS |

两轮四节点均各64执行/64决定、所有高度round0，执行失败/stale/重算为0；
relay分别150253751/154547939字节，排队1/0帧，队列/source/rate拒绝0，
均4次初始注册、无中途替换/过期。迟到旧context消息仍被拒绝，不称日志无错。
首轮最后8高仍需13.735秒，复跑分段也有波动；不签收稳定容量或归因全部退化。
两轮未同时运行其他Cargo负载；WSL2 `/mnt/d`、24逻辑CPU、同机四进程与四库，
不是四台设备/公网。计时含节点验签、真实执行、网络、共识与耐久确认；不含
钱包预签、进程/测试创世启动和事后冷恢复，不将四份重复执行累加为TPS。
宿主二进制SHA256 `ba224c78754b1236394d5a11965caa561a333e03751b37a894964be3c4f38588`，
AOEM仍为`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

原始证据均在`target/runtime-rebuild/`：`fac0e84-linux-long-baseline.log`、
`controller-warm-starvation-red.log`、`controller-warm-starvation-green.log`、
`controller-fairness-windows-release.log`、`controller-fairness-linux-release.log`、
`controller-fairness-linux-long.log`、`controller-fairness-linux-long-repeat.log`。
基线measurement目录为`controller-load-1024-434-1790914249279808112/`。
修后为`controller-load-1024-13569-1790915342199049443/`和
`controller-load-1024-426-1790915466271056317/`。
已证实的是调度公平缺陷，不是全部长测退化根因。下一处A检查历史Decision查询
仍读取完整candidate并准备正文的开销；不删除持久化/读回来换TPS。
S4旧证明库仍不支持新guest，升级AOEM需明确切仓授权；其余总目标及正式部署
授权边界保持，不宣称高性能、隐私/PQ或生产完成。

## 设备 A：完整 NOV 执行证明关系与修复版 guest，后端尚不兼容（2026-10-02）

基于 `3bac1b6`；其远端CI
[`36960204788`](https://github.com/novovm/supervm/actions/runs/36960204788)
已核验成功。这是新runtime的S4切片，不是恢复旧guest或主网证明激活。
仅SUPERVM内修改，AOEM源码/SDK未变，旧38项草稿保持原状，无Skill/新分支。

### 已实现并验证的关系

- `NVFRNT01`有界父前沿编码：独立父根和编译访问集下重新捕获，包括删除
  压缩需要的兄弟节点。拒绝缺失、夹带、错序、错误权限及错误root；不存在
  必须由认证路径证明，不能由“没有读到”推定。9项边界/正反例通过。
- `NVEXIN01`包含精确BatchContext、原序原始V3、完整政策和原父见证。Guest
  重新执行原鉴权、NOV compiler、nonce校验和同一`speculate→finish→stage`，
  不反序列化外来已鉴权能力或最终写集。原生AOEM scheduler仍在默认native
  feature中；纯guest不链接AOEM/网络，没有第二套经济实现。
- `NVEXEC01`固定152字节，输出plan/candidate承诺、完整后状态根、完整原序
  回执承诺、execution statement、交易数/状态版本。全局费用拒绝、诊断、
  nonce及余额均进入原状态/回执承诺；不只证明费用归并前的预测。
- 公开proof执行只导出journal，不导出可供候选持久化的ExecutedNovBatch。
  期待journal与可信image须由验证端独立选择；父根的链上可信性、物理文档
  摘要、BFT决定和发布权限不在这段关系中。
- 新通用ReceiptSession仅调用SDK已有prove/verify/free，精确可信库路径、
  ABI1/init、进程驻留和单owner；RAII释放成功/失败返回缓冲。7项stub/边界
  测试及!Sync编译拒绝通过；stub通过不等于真实密码证明。`-5`类型化为后端
  不可用，没有trace/fake fallback。C ABI无取消，必须隔离证明worker/进程。

### 真库回归与真实构建

- Windows完整Release **466单元/集成+6编译拒绝**，WSL Linux **463+6**，均
  0失败/0忽略。原6个AOEM经济对照场景增加proof-input/journal相等检查，仍
  与独立经济oracle比较全部余额/nonce/费用/回执/完整状态根，覆盖混合失败、
  自转、账户别名、全局费用容量、溢出及共享credit。它们运行的是本地关系，
  不是密码证明生成；四进程最终性/冷恢复等原门保持通过。
- 12项纯proof测试通过，含坏签名、错误业务/链/nonce、父policy、缺/坏见证、
  非规范编码、每处截断及资源上界。合法变化的parent/config/raw必须改变
  journal，不将其一律拒绝冒充“已认证链上父点”。
- 根workspace fmt/strict Clippy、无native lib strict Clippy、3成员无legacy
  依赖检查通过。根CI增加无native检查，但不把它称为zkVM目标验证。
- 新独立workspace实际编译 **RISC0 2.3.2** guest和probe，外层与嵌套guest
  `--locked`复验通过；guest tree确认`risc0-zkvm-platform`和
  `risc0-zkos-v1compat`均为**2.2.3**。
- 旧1.2.6试编曾成功，但因
  [官方sys_read漏洞](https://github.com/risc0/risc0/security/advisories/GHSA-jqq4-c7wq-36h7)
  被明确淘汰，不签收其image/安全性。新2.x image绑定用户ELF与kernel组成
  的program binary；不能为适配旧AOEM偷换回裸ELF或旧image。

新program为899068字节，SHA256
`1a13500aed1a8f9f9ee17695d3109ad427aa7e3679fee2250df3b3b3a287bb61`。
可信build image words为
`[2710623886,3003392925,656144283,158781046,534824962,4215304921,676798502,4060083415]`。

### 实际未通过：不能据编译/对照宣称证明完成

独立probe用公开测试密钥及测试父树，经随包AOEM库**真实执行**一笔带收费NOV
转账，导出3866B原父输入和152B期待journal，本地guest关系与其完全一致。
此夹具不是生产创世，也不是当前四节点账本的导出。

`RISC0_PROVER=local`、移除`RISC0_DEV_MODE`后实际调用：

| 库 | SHA256 | 新program的真实结果 |
| --- | --- | --- |
| SUPERVM随包Linux core | `88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675` | prove返回`-4`，没有receipt |
| 历史独立RISC0 sidecar | `908a6e198a89e425ddd345d837064e0cb8129721b05729642d0c055f0bacb464` | prove返回`-4`，没有receipt |

因此实际证明生成 **FAIL**，独立密码验证/变异拒绝 **NOT EXECUTED**。CLI已
实现独立verify及12项变异检查，但不能把“代码已写”当成这些门已运行。
旧库1.2.6与新program格式的兼容升级是下一步检查/修复对象，`-4`本身不是
完整根因诊断。补充只读源码确认：AOEM的`risc0_backend.rs::prove_program`
先用1.2.6 `compute_image_id→Program::load_elf→ElfBytes::minimal_parse`，
而2.3.2 build以`ProgramBinary::encode`生成含kernel的`R0BF`组合格式，格式
层确实不匹配。portable C入口将内部错误折为`-4`，不存入带handle的
`aoem_last_error`；没有用另建compute handle伪造该调用的错误详情。
已向用户请求下一步切换AOEM升级通用backend的授权；回复前
不改兄弟仓、不在SUPERVM另造一个绕过AOEM的密码引擎。

原始产物位于`target/runtime-rebuild/`：
`complete-proof-windows-release.log`、`complete-proof-linux-release.log`、
`complete-proof-fixture.log`、`complete-proof-packaged-attempt.log`、
`complete-proof-sidecar-attempt.log`、`nov-transfer-proof/risc0-guest-locked.log`，
夹具目录`nov-transfer-fixture/`。输入SHA256
`78bfa36213bdd659ab815902c7c5ee286e0aa994fe54c2fd2fd25e445bcc7876`，
期待journal SHA256
`6873f4f3939c25052d28e32f0a2eb1b72fc5d3ada69caf957e33ae092be2410c`。
构建锁和复现命令见[证明入口](../runtime/proofs/nov-transfer/README.md)。
独立proof/guest workspace格式检查和跳guest的strict Clippy均通过；随后
清除skip并按双锁真实重建probe，恢复上述新image（bin哈希不变）。最终probe
SHA256为`c67edac67b322cc015b95026afaa610c431111447d83d67e8351af752c39839c`。
跳guest的lint本身不计作真实构建或密码证明。

**仍未交付：** 真密码证明成功与主链有界证明队列/可信父点/最终性政策组合，
通用Execute、隐私/PQ主链接入、真实多机、持续容量及部署入口。未宣称主链
TPS改变、S4封盘、隐私/后量子证明或生产可部署；正式创世/发行仍另需授权。

## 设备 A：未决候选冷恢复与 Linux 小消息停顿修复（2026-10-02）

基于 `ba9ee1f`，仅新runtime及既有文档；旧38项草稿保持隔离，AOEM SDK/源码
未改，不新增Skill/分支，不启动正式创世、发行或部署。
基线CI [`36956208818`](https://github.com/novovm/supervm/actions/runs/36956208818)
Linux1024档在120秒门失败、Windows被取消；较早提交通过不代替本次结果。

### 交付能力与恢复边界

本地日志 `NVSIGN02/NVOUT002` 用5个固定角色引用不可变outbox事件：原提议、
prevote、precommit、locked、valid。引用按revision去重，事件payload独立哈希
不递归承诺snapshot；state-only晚QC保留原始签名字节与提议当时的valid_round
证明。打开时校验签名/域/角色/精确QC/历史引用，再重读同一snapshot与head。
缺历史事件、错误锁角色、旧锁下无合格证明的异值prevote、跨域/坏pin均拒绝。
当前轮形成的新锁不倒推篡改之前合法prevote。1024验证者完整QC仍在原512KiB
单记录和1MiB条件写预算内，没有提高额度。旧NVSIGN01显式拒绝，归档旧决定
可读；没有实现旧活动日志迁移，严禁用删除/重置日志逃过拒绝。

controller从这些引用取原AOEM库中的完整候选，通过原channel owner重建
owned请求，再进同一常驻AOEM执行/持久化流水线。候选ID、原文档摘要、完整
BlockStatement必须与原事件完全相同；旧owner凭据不复用。恢复完成前不
启动计时、不接外部body、不新签票；可以重播已经耐久的原签名。坏数据后
保持失败状态，显式重开前不继续。每步使用既有有界槽位/背压/retirement。
修复了新网络消息不断加入retirement时饿死恢复的问题：每poll先给恢复一步，
再服务有界入站；真正拥堵时仍背压，不建立新队列。

真实证据：

- 唯一持有正文的进程持久化提议/prevote后被 `Child.kill` 强制结束。四个新
  进程没有钱包正文重投，各自真实AOEM执行并确认同一head；再四个冷读进程
  对照原始签名交易、完整回执字节、全状态根、nonce/全部费用记录与资金守恒。
  原提议/prevote outbox逐字节不变。仅进程kill，不冒充断电或实体四机。
- 终止后测试库副本删除candidate marker，启动拒绝、不补marker、不改snapshot/
  outbox/head；另测试删除旧提议事件也拒绝，未清空锁或重置签者。
- 晚QC产生state-only末事件时仍重开全部原票及旧提议QC；用新owner真实重执行
  的候选触发“同轮QC已应用”拒绝，不能误靠过期owner拒绝来证明防双签。
- 旧锁A与新valid B是两个不同原文/状态根的候选：冷controller恰好重执行2批，
  原snapshot和6条outbox不变、没有第7条或head；分别核对完整业务oracle。
  这里QC是明确签名fixture，不是第二次自治网络最终性实验。
- 持续控制消息回归使用真实owner验过的票、每poll一个测试槽，实际进入普通
  接收/retirement路径；恢复仍完成且无新签/新revision。只有注入时机是测试
  hook，不伪造执行、候选或签名。不是吞吐测试。

### Linux 失败、实际修复与性能口径

本机WSL Ubuntu24.04真实`.so`复现原1024×8场景：120.52秒失败，仅6/8高度，
relay未重连/未限流。生产socket未设置NODELAY，而旧网络fixture预设了它；
统一TCP封装现在对发起和接受连接都设置，避免小共识/TLS消息等待ACK。
新的OS属性回归先红后绿。原验签/费用/共识/应用背压/TLS期限与relay额度不变。
同场景修复后 **8192唯一四节点耐久确认 / 5.821531秒 = 1407.190 TPS**，
全部冷恢复oracle通过。它是同机WSL有限负载，不是公网/长期稳定容量；完整
回归期间两平台可能同时测试，不把其中时长用作独立性能对比。

随后独立Linux长负载 **65536笔/64高度，116.253498秒，563.734 TPS**，完整
冷oracle通过。它没有同时运行Windows测试，数据库仍位于WSL的`/mnt/d`
仓库路径，不等同Linux本机ext4或独立服务器。四个会话无中途重连/源字节
限流；relay全生命周期224478877 wire bytes，活动队列字节拒绝2次、数量
拒绝27次、离线peer拒绝3次；四库各65次实际执行、64次决定，重复工作不
计入唯一交易数。P95/P99积压确认110.354/116.253秒，各库冷重开约59.2MB。
这轮接近原120秒门且后段变慢，不能以短测1407TPS签收长负载容量；队列
背压/数据增长/实际阶段耗时仍需分段定位，尚未证明是磁盘或计算单一原因。

原始日志均在 `target/runtime-rebuild/`，不上传测试数据库：

- `linux-load-before-tcp.log` / `controller-load-1024-925-1790909998430697279/`：原失败。
- `tcp-nodelay-red.log` / `tcp-nodelay-green.log`：真实socket设置反例。
- `linux-load-after-tcp.log` / `controller-load-1024-886-1790910185475669786/measurement.json`：修复后首轮。
- `undecided-linux-long-load.log` /
  `controller-load-1024-437-1790911237852188662/measurement.json`：64块独立复测；
  测试二进制SHA256 `77e948e341b52492a2dabf12bbc3500467b1f6f57c0b1d1bf7935ee3b3024ef7`，
  在最后补同轮异值日志拒绝检查前构建，不冒充最终源码重复测量。
- `undecided-windows-full-release.log`：436单元/集成+5编译拒绝全部通过；
  `undecided-linux-full-release.log`：433+5全部通过，均0失败/0忽略。
- 后补独立双候选测试 `undecided-multiroot-windows.log` 与
  `undecided-multiroot-linux.log` 各1/1；只新增测试，生产源码不变。
- 最终补齐“旧锁同轮异值QC不能为prevote解锁”拒绝后，再次全量运行：
  `undecided-final-windows-release.log` **438单元/集成+5编译拒绝**、
  `undecided-final-linux-release.log` **435+5** 全通过，均0失败/0忽略；
  包含新增双候选、强杀恢复、持续控制流及三档签名最终确认负载。
- `undecided-fairness.log`、`undecided-codec.log`、`undecided-crash-first.log`：
  定向开发证据，最终覆盖以上述完整回归为准。

DLL SHA256 `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`；
SO SHA256 `88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
格式、strict Clippy及3成员无legacy依赖检查通过；本次远端CI在推送后核验。
默认debug也通过，日志 `undecided-full-debug.log`；真实库项默认ignored，
实际已在上述Release显式执行，不能把默认忽略当作通过。
该debug轮早于最后同轮异值补丁，最终补丁已由上列两平台Release与strict Clippy覆盖。

**范围仍未完成：** 所有最大体积/最紧额度配置下恢复、物理断电、真实四机、
公网/限流后长跑、部署CLI、动态验证者，均不能由上述测试替代。原文已执行
但尚未被任何耐久签票/锁角色引用的候选，不属于本次重放闭包。原签名重播
允许早于全部候选恢复完成；“缺数据拒绝”不等于“网络零输出”。
下一处A认领S4完整业务有效性证明：核对真实prover，覆盖父根/签名/nonce/
余额/全局费用及最终输出，不以BFT QC或native-auth局部证明冒充。Execute、
经典隐私与ML-DSA主链接入仍在活动总目标内；没有缩减为一条普通转账链。

## 设备 A：同路径最终确认负载与正文按需传输（2026-10-02）

基于 `78183da`；前置 Windows/Linux CI
[`36953220062`](https://github.com/novovm/supervm/actions/runs/36953220062) 已核验成功。
仅新 runtime 和既有文档，旧38项草稿、AOEM源码/SDK、生产参数和部署均不改。

### 产品改动与真实失败

完整正文不再随控制消息盲目重发。初始广播按peer记录HostChannel首次接纳，
背压/拒绝不计成功，仍公平重试；接纳不是交付或最终性ACK。提议/投票/决定等
小消息保持重发，接收者缺正文时按精确body_id请求。归档确认查询只发Decision，
已验证的归档正文由owner准备并保持休眠，收到RequestBody再唤醒定向发送。
没有新wire版本、第二账本、外部代投票、降低验签/QC门或扩大relay预算。

64高度/1024笔批量第一次在约12高度遇到原64MiB/源/60秒限制，反复50ms重连；
该轮relay还在80.823秒异常退出，活动报告读句柄与Windows原子替换冲突只是
嫌疑，不确认为根因。第二轮只修初始正文盲重播仍在原120秒门失败，relay存活
至退出，2486次源限额拒绝/2493次注册，说明不能把问题全归于观察干扰。
补上“确认请求不附带正文”后，原限额与原期限下64高度完整通过。
失败日志保留 `controller-load-long.log`、`controller-load-long-body-once.log`。
daemon现在明确记录run-loop错误链，避免Drop只留下reason=error而丢失根因；
没有借此改报告/限额/网络I/O策略。测量只在relay关闭后读其报告。

### 实测口径和结果

Windows / Rust 1.94 / Intel Core Ultra 9 275HX，24逻辑核、约63.4GiB物理内存；
同机四独立验证进程、四独立AOEM RocksDB，真实WSS/E2E单relay。
每批32/256/1024个独立付款身份汇入同一只收不支账户，每个身份跨块连续nonce。
不是低冲突独立收款方演示：共享credit按已核验代数前提解除假冲突；四库各自
真实AOEM执行，1024档64块各完成65536个业务组件，实际回调峰值均24。
不把配置workers、四份执行、重传或重算当唯一交易吞吐。

| 场景 | 唯一成功且四节点持久确认交易 | 观测秒数 | TPS |
| --- | ---: | ---: | ---: |
| 最终源码全回归，32笔×8块 | 256 | 3.251362 | 78.736 |
| 最终源码全回归，256笔×8块 | 2048 | 4.138310 | 494.888 |
| 最终源码全回归，1024笔×8块 | 8192 | 5.001403 | 1637.941 |
| 完整按需修复首轮，1024笔×64块 | 65536 | 45.173028 | 1450.777 |
| 最终源码独立复测，1024笔×64块 | 65536 | 44.065083 | 1487.255 |

64块首轮relay全生存期接纳149091013 wire bytes，源字节限额拒绝0；注册6次，
即仍有2次连接重建及lower-stream timeout，不能称完全稳定。这里wire计数包含
预热/退出，不能除测量时间冒充精确带宽。四库冷重开后文件总量各约59.2MB，
包含RocksDB日志/元数据/历史，不是长期容量承诺。
最终源码复测接纳140976663 wire bytes、字节拒绝0、注册4次，无中途重连；
P95/P99积压确认42.275/44.065秒，四库各64次真实批执行、0执行失败/陈旧输出/
业务修正重算，回调峰值24/24/17/24。两轮还不足以证明长跑稳定性。

所有交易预先真实签名；父进程在释放整批积压前启动唯一单调时钟，每高度等
四个相同耐久head事件才计时。排除钱包密钥/签名生成、进程启动、测试初态安装
及冷恢复；包括节点验签、正文构造、业务执行、网络（含剩余握手）、共识和
同步持久ACK。采样文件由独立有界owner发布，1ms只是请求轮询周期，不保证精度。
64块首轮积压到确认的P95/P99为43.480/45.173秒，含前序块排队，不是单笔
服务耗时或固定出块时间；8块只有8个等权批样本，不用于稳定尾延迟签收。

四个新恢复进程核对每高度原文、父链、QC、真实完整回执字节；串行业务原语
oracle核对所有余额/nonce/费用Put与Delete，从空树重建完整预期状态根以排除
额外键，资金总额/费用分桶守恒。oracle只作计时外断言，不给执行器预造最终写集。
既有混合失败回归继续保留；本次吞吐负载均成功，不代表失败/隐私/合约负载。

原始证据位于 `target/runtime-rebuild/`：

- `controller-load-first.log`：修复前8块短样本47.931/304.477/1133.842 TPS。
- `controller-load-long-demand-only.log` 与
  `controller-load-1024-18704-1790907920819462200/measurement.json`：64块首轮，
  二进制SHA256 `48e37fdef4223ab38c870f697801c34c888bed2db67a46e7b42b594acd9fc271`。
- `controller-load-full-release.log` 与 `controller-load-*-26908-*/measurement.json`：
  最终代码全套回归及3档短测，原始JSON保存每高度时刻/四节点事实/二进制摘要。
- `controller-load-long-repeat.log` 与
  `controller-load-1024-12788-1790908143077345900/measurement.json`：最终源码64块复测；
  与全套回归相同二进制SHA256
  `3550f047f13d15e004e683315ca60e7744ef88184d3a21b71195ed5db1447034`。
- AOEM DLL SHA256始终为
  `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`，没有重建或替换。

真DLL Release全量 **424单元/集成+5编译拒绝通过，0失败/0忽略**：
24 AOEM、276 Host、11外部集成、113网络。包含原三块2/4不确认、第三加入、
第四归档追赶与冷恢复门；新真通道反例主动丢首发正文及第一次定向补体，应由
重复已验签提议触发第二次请求，取回后在另一真实库执行、自身日志耐久出票。
两验证者仍无head，不把该丢包探针称为完整四节点最终性。格式、strict Clippy、
3成员无legacy依赖检查通过；本次提交CI待推送后核验，不复用前置CI冒充本轮。
默认debug全套也通过（日志 `controller-load-full-debug.log`），其真实库/容量
项按配置忽略，但已在上述Release全部显式执行，不以忽略代替通过。

默认CI的 `--include-ignored` 会运行三档8高度负载；64高度复测使用同一测试：

```powershell
$env:NOVOVM_AOEM_TEST_LIBRARY=(Resolve-Path 'aoem/windows/core/bin/aoem_ffi.dll').Path
$env:NOVOVM_CONTROLLER_LOAD_HEIGHTS='64'
$env:NOVOVM_CONTROLLER_LOAD_BATCH='1024'
cargo test -p novovm-host --release --locked four_process_continuous_signed_load -- --ignored --nocapture --test-threads=1
```

这些环境变量仅控制测试钱包负载，不设生产出块参数；取消两个LOAD变量恢复
默认三档8高度。Linux按已有CI选择随包 `.so`，不照抄Windows动态库路径。

**边界与唯一下一处：** 首次取得了新runtime同路径、批量相关的有限负载TPS，
不是稳定容量、公网四机、RPC客户端体验、百万TPS或生产250ms出块签收；旧
4.299767 TPS负载/拓扑不同，不能直接计算同比倍数。继续未决高度冷重启的
body与原签名outbox恢复、网络等待/限流后的有界恢复实测；快照仅有最新outbox
不足以重放state-only更新前的票，不能通过清空锁重置。S4业务有效性证明、
Execute/隐私/PQ、实体多机、动态验证者与部署入口仍未完成，总目标保持活动。

## 设备 A：自治四进程连续链与历史补块（2026-10-02）

基于 `46e064f`，其 Windows/Linux CI
[`36948570845`](https://github.com/novovm/supervm/actions/runs/36948570845) 已实时
核验成功，不替代本轮验证。只改新runtime与既有文档，旧38项草稿保留，
未修改AOEM源码/SDK、启动正式创世、发行、部署或建立新分支/Skill。

- `HostChannel`常驻owner负责整块编解码、hash、入站签名/QC验证、构造
  本地政策BatchRequest及大对象销毁；控制/正文/回收独立额度。发送按
  peer均分原有全局条目/字节预算，离线peer不能占满健康peer额度。
- `Controller`仅注入原文即可驱动同一pipeline/ValidatorJournal的执行、
  提议、投票、收票、合格计时、连续高度及重试。签票/链头仍在原AOEM
  库实际ACK后可见；pending候选保留，迟到输出不能授权新父或新轮。
- `ArchiveRead`从本机验证过的head按高度读取不可变块、精确outbox/QC、
  完整原始候选。缺块与损坏证据区分，不扫描全历史、不造DurableCandidate。
  迟到节点通过真实网络获得原文后自行执行，不能只复制peer的状态根。
  请求仅调度缓存，不授权head；失败可重试，冷重启允许请求较低高度。
- `try_submit_owned`拒收时归还原输入，已有接纳不误报为拒收；大正文、
  BatchRequest和packet走有界owner回收。额度是逻辑内容/条目边界，不是
  本机RSS或AOEM原生内存测量；整归档恢复仍由I/O owner完整校验。

### 真正失败过的独立进程验收

前两轮三在线进程只完成两块，第三高度换轮停滞，均在原120秒门失败。
发现两个实际队首阻塞：离线peer占满Host发送全局额度，以及广播游标在
该peer背压处不前进；历史应答永久100ms重发进一步增加旧流量。修复
每peer隔离、公平尝试（未接纳仍保留重试）、按请求唤醒历史应答后，
第三轮在**16.55秒**通过，未改测试期限/5秒失败计时/3-of-4阈值。
定向真实WSS反例还验证：离线peer两层队列满，健康peer仍收到已验签票，
`expired_sends=0`，不靠等TTL释放资源。未改NetworkWorker的有界pending门。

`controller_integration`父进程只启动/停止进程和提供测试交易，不构造
proposal/vote/QC或指定决定值。四个独立OS进程和AOEM库真实WSS三高度：
2/4收齐票仍无head，第三加入自行推进；第四从空测试账本启动、逐块获取
归档并执行追赶。6笔唯一原文、每库3批实际执行（合计24次交易执行）；
含1笔业务失败，最终收款350、两付款方nonce均3。四个新的恢复进程独立
核验所有历史原文/证书、块/状态/回执相等。测试种子仅用于夹具。
这不是四台实体机器、可部署CLI、持续负载TPS或250ms出块签收。
原始日志保留在 `target/runtime-rebuild/controller-process-{first,second,third}.log`，
通过轮详细产物为 `autonomous-controllers-23012-1790905674291445900/`。

最终源码Windows/Rust1.94/随包真DLL Release全量通过：**414单元/集成+
5编译拒绝，0失败/0忽略**（24 AOEM、266 Host、11外部集成、113网络）。
包含四进程三块再次执行、实际journal pending ACK窗候选保留、所有旧安全/
费用/nonce/损坏恢复门，不把测试内场景重复计数。fmt、workspace全目标
strict Clippy、3成员无legacy依赖检查通过。日志：
`target/runtime-rebuild/autonomous-full-release.log`。本次远端CI另行核验。
默认debug套件也通过，日志`target/runtime-rebuild/autonomous-full-debug.log`；
其按配置忽略的真实DLL/容量项已在上述Release逐项显式执行，不以忽略代过关。
下一处唯一认领仍为A的新runtime：持续同路径负载、未决高度冷重启的
完整body/outbox重放。已决定账本恢复不等于所有崩溃点活性；当前固定
epoch/set、显式peer/单relay、Transfer V3/Ed25519，S4业务有效性证明、
Execute/隐私/PQ接入、动态验证者和公网多机仍未完成。总目标不缩小。

## 设备 A：加密网络与耐久共识调度接线（2026-10-02）

基于 `b4db3880`；该基线 Windows/Linux CI
[`36944400550`](https://github.com/novovm/supervm/actions/runs/36944400550) 已重新
核验成功，不替代本次改动的CI。仅新runtime、根Cargo和既有交接文档；
隔离旧38项草稿、AOEM源码/SDK、正式创世、发行及运行中服务不改。

### 数据与共识仍走同一条真实路径

- 新 `novovm-network` 局部迁移隔离区的 `novorudp.rs` 帧编解码、
  `product_overlay.rs` 加密握手与E2E，以及 `product_relay*` 的WSS载体、
  client/daemon/IO和回归。没有旧crate依赖、旧节点入口或旧业务执行器。
  帧校验和不是认证；线上worker使用严格Ed25519握手、ECDH/HKDF和AEAD，
  保留独立peer/session/replay边界，不声明网络抗量子。
- 专属网络线程负责阻塞socket操作；启动不等连接，try_send/try_recv不等
  网络。全局/peer条目与字节额度、TTL、公平peer轮转和重连边界保留。
  transport接纳不是收到/执行/最终确认，丢包/过期后仍由上层重发固定消息。
- 不提高192KiB载体payload常量。分片绑定本地链域和完整message hash；
  首片按整消息预留额度，缺片过期、乱序/重复/冲突及坏hash均有回归。
  接收hash按有限chunk推进；大body encode/decode/hash属于assembly/ingress
  owner，不能回到共识poll。64MiB是解码安全上界，不是批准的主网块大小。
- Host开发消息编码承载原始批body、proposal、vote和QC；body引用不授予
  执行/签票权限。每库仍以本地policy建立BatchRequest，经同一常驻AOEM
  pipeline真实执行并落盘，journal核对完整BlockStatement后才能出票。
- collector/pacemaker使用精确context/round/phase和独立签者权重；当前轮
  >2/3跨值票只启动计时，不成为同值QC。依
  [Algorithm 1](https://arxiv.org/pdf/1807.04938) 的合格超时或>1/3同一高轮
  证据暂存换轮，原AOEM日志ACK后采用，保留lock/valid/历史和head guard。
  proposal/prevote/precommit failure deadline不是固定出块间隔；测试毫秒
  数值不是生产参数，也没有加入挖矿发奖。

### 验证、实际反例与边界

Windows Release真实DLL的网络联测已通过：4个独立数据库、同一测试进程，
2笔唯一签名原文各库实际执行（共8次），body/proposal/vote/QC经过实际
loopback WSS/E2E和Host codec。2/4双方确实收齐两票仍无head/高度归档；
第三实际启动网络后3/4各自确认；第四在决定后启动、收body并自行执行、
收票、确认。四库state/head一致、收款150、冷重开保留原outbox和余额。
fixture不在外部替节点预签QC；但它是集中确定步骤的测试驱动，不是生产
controller、4个独立主进程、网络连续高度或性能测量。

网络113项Release回归已通过，0失败/0忽略，原始日志
`target/runtime-rebuild/network-release.log`。包括三worker真正双向收发、
单peer重启不重置健康session和WebSocket原127字节解析边界修复。
计时/追赶真AOEM专项也已通过；首轮fixture在单metadata额度下又等第二个
读屏障造成超时，保留失败日志，修正夹具不扩大额度后通过。
收票先红复现future碎票挤占current预算；独立review又复现future桶可被
单恶意签者占住，阻止诚实同高轮追赶证据进入。最终改成每validator/phase
最高future tip及固定的最高已形成追赶证据，单人更新不能抢别人位置或撤销
已形成证据。明确预留current 2N、future tips 2N、固定witness最多N条，
额外额度才保留历史；5N是本地内存容量条件，不是确认门槛或生产参数。
退休/晋升显式原子核对，旧证据不能静默被覆盖；future方案19项及修复后
真AOEM计时/锁恢复专项已通过。

最终源码Windows/Rust1.94/随包真DLL Release全量重跑：**376单元/集成+
5编译拒绝通过，0失败/0忽略**，含新collector后的四库真实网络和计时/锁
恢复再次验证。分项24 AOEM、228 Host、11外部集成、113网络；不重复计算
单测内场景为新增测试数。fmt、workspace全目标strict Clippy及3成员无
legacy依赖检查通过。原始日志：
`target/runtime-rebuild/network-consensus-full-release.log`。
默认debug套件也通过（其真库项按配置ignored，已由上述Release显式全部
执行），日志`target/runtime-rebuild/network-consensus-full-debug.log`。
本次远端CI须另行核验，前置提交的通过状态不替代本轮验收。

下一处唯一认领为A的同一runtime：接独立进程controller、
有界body/签名输入owner、重发/追赶与连续高度负载，测真实最终确认TPS和
尾延迟。当前没有新节点可部署、执行有效性证明、Execute/隐私/PQ接入或
实体多机/公网签收；全目标保持，不回旧主循环，不以测试通过替代这些要求。
当前collector高轮证据仅统计vote、不计proposal；固定epoch/set和显式peer/
单relay配置，不宣称完整Tendermint实现、动态验证者或relay自动切换已完成。

## 设备 A：同一流水线的链头原子发布与连续高度（2026-10-02）

继续 `main@26fe3777`，前置 Windows/Linux CI
[`36939293078`](https://github.com/novovm/supervm/actions/runs/36939293078) 已成功。
仅新 `runtime/` 与既有交接文档；不改旧38项隔离草稿、AOEM源码/SDK、
正式创世、发行、奖励参数或已运行的服务。

### 同一真实路径的改变

`ValidatorJournal` 现在把签票snapshot、精确决定outbox、不可变高度归档、
链头放在同一个AOEM RocksDB原子条件批写中；候选此前已经完整持久读回，
晋升只发布其真实状态/回执根，不复制第二账本或重新计算业务。签名及新head
都在ACK后才对调用者可见，未知结果冻结。每次普通签票/本地换轮/高度推进
都在同一存储owner操作中检查精确head；不能凭旧ParentPoint或内容凭据出票。
`advance_height` 只从自身已决定head派生上下文，不接受任意父参数；重置
单高度轮次/锁时保留跨块全局签票序号和append-only outbox。

决定值身份绑定完整共识context和执行块hash，不绑定证书签名子集、轮次或
本机outbox位置；不同合法3/4QC不会使下一高度分叉。归档引用精确不可变
决定outbox，避免原子批写重复保存最大QC。该身份hash不是独立有效性证明，
归档恢复仍验证提案、同轮precommit QC、原文/根/回执/执行声明与精确父链。

### 恢复与已复现的反例

冷启动必须从配置的创世锚点验证到head，不接受调用者随意指定检查点。
恢复按有界阶段poll、不保存整段历史数组；总体仍O(history)，单候选完整
恢复仍可能占用I/O owner。正常下一高度推进不调用冷恢复、不扫描历史。
固定epoch/set、每库一个本地签者；不自动接管另一签者或升级旧单高度已
决定但无head的日志。没有整库回滚保护、历史剪枝或动态验证者切换声明。

复审发现真实反例：把两个可变键head和snapshot还原到高度2/待签高度3，
保留实际已决定的高度3归档/outbox，旧冷恢复仍能开启签者。新增真库测试
先报 `partial rollback reopened signer despite retained decided successor`。
现将head及直接后继归档放在同次读取中核对；存在已决定后继则拒绝，
不修复元数据。u64最大高度允许终态恢复，但不允许溢出推进。

### 验证与下一步

真实Windows随包DLL，四个独立数据库、同一个进程、同一常驻pipeline连续
三块：6笔不同签名原文、各库实际执行共24次，第二块含余额不足失败。
逐节点后继状态根/回执/决定身份相同；收款方最终350、两付款方nonce均3。
每个节点使用不同的三签QC；全局签票revision跨高度递增。覆盖不同签者旧
会话的head guard、决定及高度推进分别丢ACK后恢复、关库重开不重执行业务，
以及缺head/决定outbox/候选完成标记/中间块归档和上述部分回滚拒绝。
专项先绿、加入反例后红、修复后绿；不把子测试次数冒充新增测试数量。

最终Windows/Rust1.94随包DLL Release全量复跑：**231单元/集成+5编译拒绝
通过，0失败/0忽略**；fmt、strict Clippy、隔离检查通过。新增8项为chain
codec 3、metadata guard 3、三块真库集成1、最大QC容量1；部分回滚为同一
集成用例中的反向场景，不重复计数。最大容量门真实签出1024份prevote及
precommit，旧/新snapshot与精确决定outbox均编码恢复核对，再为三份链
元数据各保守预留4KiB，通过原有512KiB/值及1MiB总量上限，不提高阈值。
原始本机日志：`target/runtime-rebuild/chain-head-acceptance/release.log`。
本次提交远端CI仍须单独核验；前置提交的全绿不替代本轮远端验收。

这不是实体四机、网络连续最终性、自动pacemaker、执行有效性证明或TPS
测量；未确定2秒/250ms出块承诺，也未接Execute/隐私/PQ或挖矿发奖。
下一处A继续同一pipeline+日志的真实网络、有界收票和计时换轮，再做同路径
持续最终确认测量。B须拉取后重新确认独立隐私/PQ模块认领，不回隔离旧路径。

## 设备 A：同一常驻流水线的耐久签票与单高度决定归档（2026-10-02）

继续 `main@3563f823`，前置 Windows/Linux CI `36935015200` 已成功。本轮
仅新 `runtime/` 与既有文档；旧38项隔离草稿、AOEM源码/SDK未改。新模块
直接使用前一片真实签名→AOEM业务→原子候选写入的输出，没有第二套业务
执行器。计算和存储会话继续常驻；共识metadata共享原有I/O owner及同一个
AOEM RocksDB，另有有界metadata额度，不挤占候选捕获额度，不另初始化库。

`BlockStatement` 不是输入plan ID改名：绑定完整执行上下文、验证者集/epoch、
父决定、原文计划ID、输出状态、逐批回执、执行声明、文档摘要和交易数/版本。
非零声明或QC都不证明业务执行有效性。`DurableCandidate` 只在真实pipeline
完整持久化读回且id/root/statement/document相符后创建；只读私有构造、无
反序列化入口，限同一owner。公开诊断字段不是授权。投票各阶段不重复扫描
完整候选；新会话/重启旧凭据失效，冷恢复StoredCandidate也不自动取得凭据。
同owner内容不可变是缓存前提，不是每次签票时全盘损坏巡检。

`ValidatorJournal` 每次检查固定可信父绑定、精确域/轮次/阶段/锁；签名留在
private pending，snapshot+精确outbox一次原子条件批写并读回后才释放。
已存在完整同结果可幂等确认；陈旧snapshot/部分记录不能补修为成功，冲突
和未知结果冻结会话。恢复校验父点、域、valid QC及最后outbox/签名；不把
缺snapshot但仍有outbox当新签者，不承诺抵御整库回滚或扫描全部历史outbox。

### 不迁移的旧锁规则与新开发格式边界

审查旧 `native_block_seal.rs` 首提案/首票的高度锁，以及
`native_block_seal_newview.rs` 不迁移锁规则，存在反例：四等权验证者中，
恶意leader向三诚实节点发A/A/B后不投票，无法形成3/4QC；网络恢复后仍会
被永久不同高度锁阻止收敛。因此不整体照搬旧控制流，也不能简单删除锁。
新 `round-bft/v1` 的单高度安全内核参考
[Tendermint Algorithm 1（2019修订，第22–67行）](https://arxiv.org/pdf/1807.04938)：
首prevote不锁，当前轮prevoteQC才锁，nil/timeout不清锁；更高valid-round
证据约束换值；迟到QC可更新valid但不二次precommit；只有同轮非nil
precommitQC及本地有效执行提案才形成决定输入。权重阈值重新计算为严格
大于2/3，仅四等权时等于3/4；保留拜占庭权重小于1/3的模型假设。

wire是显式新开发格式：chain/genesis/protocol/epoch/set/height/parent/
round/phase/value均签名绑定，严格Ed25519、规范固定端序、有界解码、
唯一已排序签者；旧签名不可改标签，不能跨轮合票。**不是生产协议激活、
ML-DSA迁移或业务有效性证明。** 本轮没有完整pacemaker的计时资格与高轮
追赶、网络收票或连续链头。metadata按验证者存当前snapshot，仅支持固定
单高度；ParentPoint由调用者可信配置，不能代替实时canonical head权限。

### 实际测试与下一处接入

Windows/Rust1.94真实随包DLL，Release `--include-ignored --test-threads=1`：
**223 单元/集成 + 5 编译拒绝通过，0失败/0忽略**；fmt、strict Clippy、
隔离检查通过。新增47项：wire15、轮次18、声明4、日志codec3、metadata5、
真库共识2，另加私有内容凭据编译拒绝。覆盖签名/域/阈值/phase/损坏编码、
A/A/B换轮、锁迁移、nil/迟到QC和旧轮决定，以及条件写部分记录拒绝。

真库测试在**同一进程四独立数据库**各执行同一真实签名批，精确候选内容与
声明一致，真实proposal/prevote/precommit编解码、3/4QC、决定归档和关库
重开一致；陈旧会话CAS不出票，错误父点与跨owner候选拒绝。另一测试用
共享真实提交函数确定性停在取ACK前，丢弃journal，再以同owner读屏障确认
写入、关库重开，恢复原票字节、拒绝重复prevote、允许合法下一phase。
这是**丢应用回复/关库恢复，不是断电或四机网络**，也未宣称动态链头晋升。

下一处仅 A 的新runtime：同一流水线+签票日志接可信链头原子晋升/连续高度，
随后接收票/换轮调度与真实网络，测同一路径的持续最终确认吞吐和尾延迟。
不能继续累积孤立演示组件代替纵切片；未有新节点、完整最终性、主链TPS、
执行有效性证明、Execute/其他资产/隐私/PQ或部署验收。

## 设备 A：有界常驻候选数据流水线（2026-10-02）

继续 `main@9ccefb58`，前置 Windows/Linux 真库 CI `36932102660` 已全过。
仅新 `runtime/` 和既有文档，旧38项隔离草稿及AOEM源码/SDK均未改。
新控制端非阻塞提交/轮询；独立协调器交错推进签名/计划、父输入捕获、
业务计算、持久化。compute/storage各在所属线程启动一次，正常批次不
重建会话。编译、政策/nonce解码和packet编码在计算线程，非唯一I/O线程。
批内依赖组件仍在AOEM计算；同一compute owner逐个处理命令，不声称
多个候选图并发。候选内容不是链头或签票权限，context仍需上层可信父绑定。

增量frontier保留每个key的路径/删除兄弟游标，有限步推进、每次最多64个
去重hash批读；响应与唯一请求绑定，逐边验证放置，缺节点不当作不存在。
请求/返回数量、顺序、hash与预算错误永久拒绝该捕获；无从根反复重放。
捕获完成只移交owned内容，经济输入检查在计算owner，不把整批计算塞入
控制循环。正常完成/额度释放事件唤醒协调器；100ms只用于失联的后备检查。

整任务保留batch/逻辑字节额度直至执行和回复都释放，满队列返回原输入。
外部查询另有最多 `io.requests` 个、`io.requests * 545` 逻辑字节额度，
不计入内部 `io.bytes`，但共用同一个AOEM存储owner、命令队列及RocksDB；
这不是总堆/原生/数据库内存上限。回复未领不能阻断内部捕获/写入；shutdown
先断开查询提交端，遗留ticket不保活sender。单次原子同步写仍不可抢占，
不承诺硬实时。写结果未知不重试；完成只在真实provider读回与packet绑定
相等后返回。坏签名/nonce是普通拒绝；意外panic/底层故障不当成业务失败。

Windows/Rust1.94真库 Release `--include-ignored --test-threads=1`：
**176 单元/集成 + 4 编译拒绝通过，0失败/0忽略**，fmt/strict Clippy/隔离
通过。新增9批捕获、6计算owner、3流水线单元及1独立进程集成；真实流水线
三个同父竞争候选对照同步调用的packet全部字节，同时独立核对余额/费用/
nonce；坏签名、坏nonce后仍成功，首个ticket丢弃也持久化，重开明确重放
报告already_present。另一进程恢复全部原文/回执/状态及父根不变。
外部查询额度设为1，唯一回复一直不领取，内部各批仍完成、shutdown返回
后再消费该回复。另用显式测试暂停计算owner，验证控制提交和真实AOEM
状态查询继续服务；暂停夹具不是自然业务并发峰值或性能成绩。

**不是连续最终链、主链TPS、多机、执行有效性证明或部署验收。**
下一处仅新runtime的可信父点/顺序、网络和共识合拢；复用这条数据流水线，
迟到结果须重新核对当前权限。Execute/隐私/PQ仍未迁入，不恢复旧容器。

## 设备 A：AOEM 原子耐久候选与常驻有界 I/O（2026-10-02）

继续 `main@2f6193d4`；该前置 Windows/Linux CI `36928929143` 已全过。本轮
仅新 runtime、活动 CI 和既有文档；旧38项隔离草稿、AOEM源码/SDK均未修改。
原始 BatchPlan 随执行结果保留，不接受事后另配 raw body。候选保存完整域、
有序原文/访问声明、回执、新节点清单及其承诺；恢复只产出 StoredCandidate，
无执行/认证/投票/最终性权限。input plan 为候选ID，异结果不能另占同ID。

底层是随包 AOEM 通用 storage-provider wire，非宿主另接 RocksDB。opcode5
对应带 WAL 的同步原子 WriteBatch，内容与完整标记同批。严格拒绝存在的
`AOEM_BENCH_RELAXED_SYNC`（包括空值）；不修改进程环境，部署须冻结环境。
原生写回执坏/未知则永久停用会话，不自动重试、重开或修补已完成候选。
真读回来自 provider，但仍可能命中 RocksDB 缓存；不说成物理绕缓存盘读。

I/O owner 启动开库一次常驻，同一会话处理后续请求；计算会话同样复用，不
逐笔/逐批初始化。非阻塞提交/轮询与请求数、逻辑载荷字节背压，回复未消费
仍占预算，丢 ticket 不撤销已接受任务。只允许一个在途 writer；预检/读回
每64键推进，间插查询，避免两个候选 preflight 后互相覆盖。原子写本身是
不可拆分同步 I/O，不承诺硬实时；预算不等于全部 Rust/原生/DB 内存开销。
恢复整份候选仅用于冷启动，不能放回每 tick；新节点捕获/调度仍待接入。

Windows/Rust1.94 实际 Release `--include-ignored --test-threads=1`：
**157 单元/集成 + 4 编译拒绝通过，0失败/0忽略**，fmt/strict Clippy/隔离通过。
新增9存储、10 packet、8 I/O、1跨进程集成。真实原生写后仅损坏返回 ack 的
故障测试确认数据确已落盘、会话拒绝后续调用且不重试，不是 mock 写成功。
跨进程测试在 writer ACK 后 `process::exit(0)` 不执行 Rust 析构，下一进程
精确核对父子原文/回执/根/费用页/余额/nonce及旧父可读；再篡改文档、删除
子根节点，恢复与显式重复提交均拒绝、不补写，父根仍可读。首次写 ticket
被丢弃，同一 I/O owner 的第二次提交仍报告已有完整候选，再 drain/reopen。

这些是进程退出恢复证据，**不是断电/磁盘故障、多机共识、整链证明或TPS**。
校验新增节点清单及父/输出根不等于扫描验证全部继承子树；底层ABI也不能在
返回前限制外来超长value的分配，须使用本配置管理的有界数据库。没有垃圾
回收或正式创世激活。下一处仅新宿主有界捕获/常驻计算/存储流水线，随后
网络/共识与真正最终确认；Execute/隐私/PQ仍未迁入，禁止恢复旧容器。

## 设备 A：新 runtime 签名 NOV 并行业务批与完整直付结算（2026-10-02）

继续 `main@01afa749`，该前置提交 Windows/Linux CI `36925868084` 全绿。
本轮只写 `runtime/` 和既有交接文档；旧 38 项草稿原位隔离、未改未夹带，
AOEM SDK/仓库未改，没有新分支、部署或创世操作。

`direct_nov_fee` 局部迁移既有统一 NOV 直付规则，不引入 Store/环境查询。
保留 Transfer JSON 投影费额、wire max=0 自动滑点上限、实际只扣基础费、
每笔分桶舍入与 risk 余数、容量预检及诊断/日窗顺序。失败不伪造成功资金
流；费用已结算的业务失败仍收费并消耗 nonce；无效 nonce 则整批拒绝。
完整显式政策（包括 TTL/来源/阈值元数据）必须与精确父记录相等，所有字段
参与 effect commitment。固定程序、语义版本、记录和回执编码也由编译器
核对，不能把任意 nonzero context hash 或 quote_id 当授权。

`nov_transfer_batch` 从真实认证交易自行派生访问集。Host 只编译依赖与
检查 owned 父输入；AOEM 回调完成报价/业务算术，最后回调在短完成锁外做
原序全局结算、失效预测修正、完整逐笔经济回执和一次批树更新。只收不支
账户还须满足 parent+全部请求金额上界不溢出，才解除共享 credit 假冲突；
不满足就保留原序依赖。全局拒付后按真实余额/nonce 修正，每笔最多重算
一次；纯 credit 按实际成功前缀重基，不使用各组件猜测的绝对收款余额。
没有 Host 先算最终写集、没有向 AOEM 塞 NOV 业务 opcode，也没有在回调内
等待其他回调。当前仍是通用 compute 图，不是原生 DAG continuation/GPU。

新 direct-only 记录明确版本化：单树值仍最多256字节，政策/费用记录分别
限定2048/8192字节并分页，不抬原树上限、不扫描历史；缺页/隐藏尾页/非规范
数据拒绝。账户保留 absent/zero 和20/32字节差别，signer nonce 仍共用。
journal 为每笔绑定的输出，不在费用快照里累积历史；没有声称旧逐笔根、
prev-seal 或旧512条 journal 查询兼容。其持久化/索引仍待接入。

本机 Windows/Rust1.94：fmt、strict Clippy、依赖隔离检查通过。默认114项
单元通过、15项显式ignored及3项编译拒绝通过；随后实际执行 Release
`--include-ignored --test-threads=1`，**129 单元/集成 + 3 编译拒绝通过，
0失败/0忽略**。新增15项纯费用测试（含2048组独立对照）、6项依赖/身份
测试、8项分页测试、6项实际随包 AOEM 的签名业务集成。

真实业务测试使用独立串行经济 oracle，不调用生产报价/结算/算术/reducer
生成期待值；逐项对照完整费用诊断、回执、余额/nonce、存在性及最终批根。
源读取器销毁后执行，父节点内容保持不变。64笔真签名向同一账户汇入时为
64个组件、0重算，两次自然回调峰值18/8，整套复跑为23/9；不使用sleep或
barrier造重叠。另验证全局昂贵费用被拒后，后继原预测失败能修正为成功；
收款上界溢出则真实回落为1组件，业务失败仍收fee/消耗nonce。错误政策、
父根、nonce、程序/效应/语义/回执版本及不支持资产都拒绝，计算owner可复用。

**仅签收新签名 NOV 批执行与完整直付经济效应，不签收耐久账本、业务有效性
证明、运行节点、最终性、主链TPS或主网。** receipt batch commitment 也不是
累计回执根或 ZK 证明。Execute/其他资产/隐私/PQ 仍未接新 runtime。
下一处：A 在新 `runtime/` 接 AOEM 批持久化与独立有界 I/O，再合拢新宿主的
收单/共识/恢复；不回旧容器，不占网络控制循环。新提交远端CI须独立查询。
用户已要求持续推进到真实资产主网上线发币；正式创世、分配、验证者及上线窗口尚未批准。
不自动部署、生成正式创世经济参数或替换运行中服务。代码提交与同步遵循用户授权和共同开发约定，不等于上线许可。

运营者入门与待确认参数见 `NOVOVM_MAINNET_OPERATOR_PRIMER.zh-CN.txt`。
用户报告现有四台 Windows 设备及一台阿里云服务器；本轮设备地址、系统、
身份和可用登录方式尚待采集，旧测试 IP 不作为可用连接配置。

## 设备 A：新 V3 批验签与 nonce 输入边界（2026-10-02）

继续 `main@c91f1fac`，该提交 Windows/Linux CI `36923936764` 全部成功，
含实际随包 AOEM 计算/生命周期测试。本轮新增代码只在 `runtime/`，旧 38 项
未提交草稿未改，AOEM 仓库/SDK 未变。不建立新链、分支或部署。

`ingress/wire` 只局部迁移既有 NNX1 V3 Transfer 编码与完整 unsigned intent、
canonical TxIR hash、adapter v2 signing message，不依赖旧协议 crate 或
整个 TxIR。借用解析在分配字段前检查整包预算；只接受20/32字节账户和96字节
Ed25519载荷，拒绝旧版本、其他业务、尾字节与非规范 postcard 编码。12 项
测试包含独立冻结旧 schema/preimage、手写字节向量、真实旧 oracle 签名、
全字段篡改、整数边界和畸形长度，不是只让新 encoder 验证新 decoder。

`authentication/batch` 以显式 configured chain 检查完整签名和 payer，保留
公钥统一 nonce identity；已验对象私有、无 Deserialize/可变 getter，认证
原文/顺序/metadata 经消费绑定到批计划和捕获输入，不再带活数据库或权限。
AOEM ComputeTask 实际调用固定 dalek2.2.0 `verify_strict`，没有旧 adapter
环境选路。弱 identity key/R、S=0 负例先证明普通 verify 接受，再证明新准入
拒绝；这是明确的新准入收紧，非“所有历史输入完全等价”，也不是旧链已升级。
V3 仍只绑定 chain_id、不绑定 genesis hash，后续新签名协议必须明确处理域，
本地 plan 的 genesis/protocol pin 不能补签钱包没签过的数据。PQ 参数/单签
或混合规则没有擅自选择，Execute、Governance、隐私/PQ 不会被当作 Transfer。

审查修正了坏签名误作为计算故障导致 owner 永久 poison 的问题：验证拒绝
作为正常 typed result 返回，整批不准入但通用会话可继续使用；真正任务
异常/超时仍失败关闭。nonce 规划使用原序与 signer 身份，缺父输入不能当0，
失败不修改父 map、不返回成功前缀；仍须由业务编译器从精确父输入提供数值，
不是任意 map 即取得授权，更不是耐久 nonce 预约。

本机 Rust1.94 / Windows：fmt、strict Clippy、依赖隔离通过。默认85单测+
3编译拒绝通过、9项显式ignored；随后 Release `--include-ignored --test-threads=1`
为 **94 单元/集成 + 3 编译拒绝通过，0 失败/0 忽略**。新增2项真实AOEM集成：
64笔真实V3签名、32个签名者的20/32别名共用nonce、精确body绑定、源销毁后
在AOEM内读取父nonce并规划暂存根；同一会话连续拒绝坏签名/错链/重复/超预算
后合法批次仍成功。验签两次自然回调峰值9/8，不是交易执行并行或最终确认TPS。
nonce记录为测试布局，暂存更新未落盘，也没有完整扣费/回执/最终性声明。

下一刀经济迁移已经只读核对：既有 Transfer 投影为 `native_asset/transfer`，
NOV直付基本费额 `40 + min(ceil(args_bytes/16),64)`，args保留规范
`{asset,to,amount}` JSON长度口径。wire max=0 自动解析滑点上限，实际扣基础费；
不能与纯算术的字面 cap=0 混淆。金额/计数容量检查必须在扣款前；国库储备、
累计及三桶是同一资金的不同会计视图，不能重复计供应量。每笔reserve/fee
向下取整、risk接余数，保留已配置分配比例，不擅自硬码70/20/10。NOV直付
不烧币、不立即向验证者派奖、不增加非NOV daily-used。报价失败不刷新日窗；
报价成功后结算失败保留日窗与失败诊断但不动钱。业务失败若费用结算成功仍
收费并消耗nonce；全局结算拒绝要修正后继预测。旧报价TTL/政策有环境读取，
新函数必须接收显式解析并绑定的policy快照，禁止复制环境依赖进worker。
依据：隔离区 `native_transfer_dispatch.rs:127`、`tx_ingress.rs:8715`、
`:8793`、`:8971`、`:9042`、`:9158`、`:9563`，仅供迁移参考、不在旧代码继续改。

## 设备 A：新批输入与真实 AOEM 计算组件（2026-10-02）

在隔离提交 `161f64d` 上继续，生产源码和测试只新增于 `runtime/`；旧目录
38 项草稿不改、不夹带提交。此前新模块 CI `36921696822` 的 Windows/Linux
均成功，不表示本次改动或旧整链已通过远端测试。本次 CI 增加仅下载对应平台
AOEM core 的 LFS 内容并显式执行真库 Release 测试，结果须按新提交另查。

新增批计划绑定链域、创世配置、协议/业务/完整效应版本、原始交易顺序与长度、
精确父块/状态/回执根和状态版本、块上下文、访问权限。预算/线程/路径不是
语义身份。plan 与 captured frontier 私有绑定，拒绝未知键、坏根、漏内容、
越权及溢出；同 plan 两份不同合法 patch 仍有相同输入承诺、不同输出根，
明确不能拿输入承诺冒充正确结果。当前 raw 为未验证字节，没有鉴权声明。

只迁入 checked Transfer 算术，保留 exact 20/32 账户、self affordability、
u128 溢出、业务失败费用和 nonce、quote/cap 拒绝、待归集费额守恒。不导入
旧 Store/调度器/候选；结果明确为全局结算前，未迁国库分配/日窗/完整回执。

新 `novovm-aoem` 只绑定通用 compute ABI，要求显式可信 DLL/SO 路径、8 个
必需符号及 ABI1，无 Host 执行 fallback。任务在 AOEM 回调内计算，非预造
最终写集。错误 session 永久 poison；未知/超时/Host unwind 无法确认排空时
保留整个 flight/session，防迟到回调访问已释放对象，不伪称能自动恢复。
DLL 保持进程驻留，正常 session 销毁；每次加载保留 module reference，有
明确资源代价。随包 `56e9da15` 的 active_count 在 completion 前归零，安全
还依赖 Host Arc/inflight 和 destroy→shutdown→join，而不是只看 active=0。
ABI/symbol 检查不等于 FULLMAX/二进制身份认证。当前 create ABI 可能隐式
读取持久化环境，因此 open 拒绝非空 `AOEM_PERSISTENCE_PATH`、不修改环境；
仍不保证第三方初始化完全无 I/O。未来 compute-only 创建选项是通用需求，
本轮不修改兄弟 AOEM 仓库。

本机 Rust 1.94.0 / Windows 实测：fmt、strict Clippy、隔离检查通过。
使用随包源 `56e9da15` 的 Windows core，实际 DLL SHA256 与 Git LFS oid
一致：`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
默认 63 单测 + 2 编译拒绝通过、7 项显式 ignored。随后 Release
`--include-ignored --test-threads=1`：**70 单元/集成 + 2 编译拒绝通过，
0 失败/0 忽略**，包括 32,768 键容量、5 项真库生命周期和 64 项算子联调。
联调在源 reader 全部销毁后，从批内 fixture raw 在真实 AOEM 回调解码、
读 owned 状态、计算费用/余额/失败 nonce、stage 树更新并验证每个结果。
8 种经济边界重复 8 次，源读取计数不再增长；两次自然回调峰值 12/6，
通用纯 CPU fixture 峰值 8。没有 sleep/barrier 制造计算并发；超时专用
故障测试的等待仅用于控制回调寿命。测试打印的 panic/compile error 是被
断言捕获的故障与 compile-fail 用例，不是把实际失败忽略。

这些任务是独立未签名算子夹具，pending fee 不是国库结算，staged nodes
不是耐久状态，更不是 64 笔主链最终确认。没有新的 TPS/可部署节点/实体
多机/最终性/业务 validity proof 验收。下一步在同一新 runtime 接真实鉴权
和完整业务效应编译，保留代数条件交换与异构执行目标，不回旧代码叠加。

## 设备 A：按用户要求隔离旧源码、独立新工作区（2026-10-02）

迁移源为 `main@ee80271` 加本机未提交工作树。旧源码及关联 Cargo/vendor/
proofs/scripts/config/workflows 移至 `legacy/supervm-20261002/`，迁移前后
1,413 个实际本地文件 SHA256 全部一致。750 个已跟踪文件按原 Git blob
归档；26 个 modified、12 个 untracked 试验仍保留为未提交状态，未夹带。
未删除文件、旧构建输出或运行账本；AOEM SDK、历史文档留在根原位置。

根 workspace 明确排除 legacy，只登记 `runtime/novovm-host`；构建输出
改用 `target/runtime-rebuild`。旧 workflows 完整归档，不再默认触发。
新 CI 只检查新模块和隔离边界，**不代表旧整链门禁通过、也不代表新链可用**。
首个局部迁移为内容寻址树算法，新写 owned 状态输入/frontier 与访问权限边界；
当时安排为在新目录完成签名接入、AOEM 真执行、耐久状态、最终性和恢复纵切片；
该目录限制已由 2026-10-03 恢复路线取代，不再授权整个原产品退出构建。
尚无新主链性能或多机验收，不部署、不发行、不生成正式创世。

本机 Rust 1.94.0 验证：新 workspace `cargo check`、fmt、严格 Clippy 和
依赖隔离检查通过（1 个 active member，无 legacy package dependency）。
25 项默认单测及 1 项 compile-fail 文档测试通过；另显式执行 Release 容量
用例通过：32,768 键、12,779,421 字节树内容，单键更新读取/新增各 18 节点。
它不是持久存储、AOEM 或主链吞吐测试。新增 11 项 frontier 用例覆盖源释放
后的真实线程计算、重复 put/delete、删除兄弟节点、未知/缺失数据区别、权限、
资源边界及 128 组差分；评审发现的公开 reader 绕过已改为私有 adapter，
并有编译拒绝测试防回归。新 Linux CI 尚待远端运行，旧整链门未重跑。
随后 Release `--include-ignored` 全量复验为 26 单测 + 1 文档测试通过、
0 失败、0 忽略；默认构建没有生成或启动旧节点。

## 设备 A：宿主重建路线评估，不签收旧异步原型（2026-10-02）

用户再次质疑个位数 TPS，并明确允许重新实现、保留旧代码按需迁移。核对
`main@0b69fc4`：主循环同步串联、候选准备/持久化与 RPC 耦合仍真实存在。
最新 4.299767 TPS 仍是上一条 256 笔同机四进程短基线，不是系统最大吞吐。

本机新草稿把固定证书重发与实时权限分离，并尝试 owner-local 耐久阶段；
真实编译和 5 owner / 11 RPC / 4 独立调度测试通过。但整段落盘仍使状态
RPC 背压，且原退休器要求 finalized height 及 ledger candidate binding，
不能回收未登记的迟到完成输出；**不将此草稿提交成可用完整流水线**。
新四进程独立观测夹具仅编译/调度单测，尚无新最终确认 TPS。

评估选择重建 fresh 宿主与候选数据平面；AOEM、密码、网络、经济和共识
原语保留按需迁移，旧串联控制流不迁移。全项目重写没有更快的依据；宿主
重建也不预报天数或吞吐。完整依据、A/B 认领和替代纵切片验收见
[历史重建计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md#历史重建评估与迁移决定)。
本条只更新路线与真实证据，不代表活动目标、主网性能或生产部署完成。

## 设备 A：S2 每笔 typed record effects（2026-10-02）

运行代码 `14b32a3`，父提交 `28b0b59`；验证 tree=
`57ac7a601be82616c45bfe3bfd786925a718a38e`。仅本刀 8 文件进入索引快照
`artifacts/audit/semantic-record-effects-index-20261002/`，逐文件 Git blob
核对一致。原 27 个异步试验文件保留、未夹带。仍为同一 Windows 11 / Core
Ultra 9 275HX（24 核）、Rust 1.94.0、同 packaged AOEM DLL；非实体多机。

用已授权且已加载的 raw records 初始化候选内 typed journal；每笔只投影
本笔账户/nonce、固定费用字段、当前与实际淘汰 trace、回执/mirror 等路径，
不再三次编码整稀疏 Store。原业务 helpers、收费和失败 nonce 规则不改；
业务阶段与收尾阶段保持原 delta/count/root/seal 顺序。批末仍执行原
`sparse.changed_records()` 独立完整核验，净 patch 必须逐项一致；不把
合法但不完整的子集当成正确写集，不将 physical-only 字段混入业务 delta。

干净快照 fmt、严格 Clippy（node lib/tests）、Release 构建通过。68 项
Transfer 测试全过（5.24 秒），新增 7 项投影测试覆盖真实费用五种拒绝、
业务失败、零金额账户结构、自转、20/32 字节 signer nonce、u128、跨日和
512 条窗口淘汰/已有 trace 重排。新增真实 AOEM 测试覆盖 10 种场景，
完整批次及每个串行前缀对照旧三次编码算法，整个 Store/回执/mirror 相同；
测试计数断言每笔完整 Store 编码 typed=0、旧 oracle=3，不将此计数当 TPS。

4 项候选门通过：delta 四阶段恢复 5.11 秒、bundle 损坏拒绝 1.08 秒、fresh
Transfer 恢复/最终确认 42.38 秒、无关历史点读 0.40 秒（0 与 1024 个无关
账户/100 历史回执场景均读取 66 个唯一记录）。四进程混合夹具 38.57 秒通过，
6 笔中 5 成功/1 业务失败；串行 worker 改用旧三次编码 oracle，完整块、
费用/nonce/回执、持久 QC 与重启全部一致，不是新算法与自身比较。

同上一条 256 笔耐久 ACK 持续负载参数，诊断关闭，256/256 成功最终确认：
窗口 **59.538107 秒、4.299767 TPS**，观察 P50/P95/P99=
17.538/55.594/58.859 秒；含 bootstrap 块笔数
`1,2,32,32,32,32,32,32,32,30`，state_version 增长 256。全节点完整块/
持久 BFT proof/重启读回一致；完整夹具 93.63 秒。相对前次 3.748382 TPS
单次约快 14.7%，未做统计重复或固定逐块切分 A/B，不能承诺稳定同比提速，
更不是高性能主网签收。签名预生成、bootstrap 不计入测量；观察延迟包含
客户端 HTTP 与轮询，不等于精确提交时刻。

本地 SHA256（原始日志/二进制不上传 Git）：node=
`43331906bd94d8dc215710ea71c091009dd27dac84d3deb8dcd6ecb96a586014`，
harness=`635f01206068a109e84864e581021637814c9a5cc3e3ec44cff57872957fb19d`，
libtest=`1deb14c2814f8326acd734a5a500c12c8a64d6b42e55243f60eec8116b068bdb`。
快照内 `artifacts/audit/candidate-node-processes/`：
`seal-relay-21700-1790882779971174500/transfer-finality-performance.json`，
SHA256=`f7541ee20721afdbe9e1e5d82b38f481fedfd12bf7faa95ec0fdd1964dace6b1`；
`seal-relay-17004-1790882728052399300/mixed-transfer-acceptance.json`，
SHA256=`c1161759680b527b06bdd724368e1de8edcbc658c5a8ce4a4dd2a2cd022a23ed`。

边界：journal/trace order 仍为数组记录，state 逐笔多版本内容仍保留，
AOEM 通用性/codec/经济规则未改；S2 整体、S3、S4、隐私/PQ 主链接入、
实体多机、公网/长跑仍未由本刀签收。下一刀按[重建计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)
合并同次输出的重复三树重建，保留 OutputWritten checkpoint 后的独立
耐久读回；当前目标继续进行，不部署、不发行、不生成正式密钥。

## 设备 A：S2 批内可达写集裁剪首片（2026-10-02）

运行代码 `8f563ff`，验证 tree=`87d4410e0382139eeb98a656616c216b9bee559a`，
由 `8659212` 加本刀 5 文件导出干净索引快照，原 27 个异步试验仍保留且
未夹带。环境、AOEM DLL 与上一条相同；快照位于仓库内
`artifacts/audit/semantic-compaction-index-20261002/`。

`RecordOverlayV1::finish_compacted` 先检查所有 staged node/blob hash 与
codec，再仅沿最终根可达新节点裁剪；新叶还检查 blob 长度及原始 key 绑定。
遇继承子树即停止、无 reader I/O，不删除任何持久内容。Transfer 仅 physical
和 receipts 显式启用，state 原 `finish` 保留每笔 before/after 根对应内容。
prepared 文档不编码 staged 节点全集，三根、逻辑统计、变更见证、回执及
原四阶段完成协议不变。它不是历史数据库 GC，不是任意父根的完整验证器。

干净快照格式、严格 Clippy（node lib/tests）与 Release 构建通过；34 项状态
测试通过（7 ignored，随后单独执行其中新真实测试），包含 13 项新纯测试、
64 轮差分和全部 65 个历史根复查。新真实 AOEM 夹具 0.36 秒通过：24 次
反复更新后 staged nodes `123→4`、blobs `49→2`、chunks `145→5`，实库
确认 117 个新临时节点与 136 个临时分片未写入。这个夹具刻意反复覆盖，
**不是生产 Transfer physical 树的实际减量比例**。

独立进程重开后父/子可读、compact 幂等；故意删除该测试专属库的一个已完成
记录分片后，读取与重放均拒绝，descriptor/completion 不变、再重开仍未修复。
未删除用户账本。兼容边界：compact 已完成后若旧二进制要求 full 临时写集
重放，同 identity 可能拒绝；不自动补写，也不宣称双向降级恢复兼容。

3 项真实候选门通过：delta 四阶段恢复 5.13 秒、bundle 损坏拒绝 1.06 秒、
fresh Transfer 恢复/最终确认 42.54 秒。60 项转账回归全过。四进程混合
串行对照/失败 nonce/回执/重启通过（38.68 秒）；同上一条 256 笔耐久 ACK
持续负载参数、诊断关闭，256/256 最终确认，窗口 68.296136 秒、
**3.748382 TPS**，观察 P50/P95/P99=24.646/65.474/67.872 秒。含 bootstrap
块笔数 `1,3,32,32,32,32,32,32,32,29`；四节点完整块/持久 QC/重启读回
一致、state_version 增长 256，完整夹具总耗时 102.03 秒。与上一条 3.709297
及更早 3.681/3.749 TPS 相当，不是显著提速的证据，也不是逐块同切分 A/B。

本地 SHA256/原始证据（不随 Git 上传日志或二进制）：节点
`ada9ccf88c5ec21e28fcd8bad9ab228922fa60519f520fcac27a983b104408d5`；
harness `0140b9b133148cc6546901e112d47d15f5290beada542e781bcccb9c1c142d4a`；
libtest `bbbab84f570b0af0b4383573593c7558833e91d9ff615da9427d4af79c581467`。
快照的 `artifacts/audit/candidate-node-processes/` 下：
`seal-relay-6096-1790881266824859500/transfer-finality-performance.json`
SHA256=`09e980903a4aa34bbfcf780dc6fca442010038f5f016e60710205df0778ae59f`；
混合对照在 `seal-relay-16672-1790881227946296600/mixed-transfer-acceptance.json`，
SHA256=`c1161759680b527b06bdd724368e1de8edcbc658c5a8ce4a4dd2a2cd022a23ed`。
真实存储故障库在快照的
`artifacts/incremental-state-tests/record-compaction-13552-1790880999301217300`。

结论：减少已确认的临时写入并保持主链语义/恢复，不签收 S2 整体、高吞吐、
实体多机、公网或长跑。最大 journal 多版本仍在保留的 state 树；下一刀按
[计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)做每笔 typed record effects，
去掉逐笔三次整稀疏视图编码，保留批末独立净变化核验。活动目标继续进行。

## 设备 A：S1 checked-credit 条件交换首片（2026-10-02）

运行代码 `d6aa770`，父提交为计划/分工 `fcfff87`。验证使用 Git 索引导出的
干净源码快照，tree=`6f26fae0dc6362ad96729ca0b6926ba54b145610`，等于该
代码提交的 tree；不含本机原有 27 个异步试验文件。它们原样保留、未提交。
快照在仓库内部 `artifacts/audit/semantic-credit-index-20261002/`，不是新
分支、另一个产品账本或新的开发主线。测试后逐文件核对 Git 规范化 blob
一致（Windows checkout 换行不同不作源码差异）。

实现：同一父视图、账户批内只收不支、父余额加全部请求金额上界 checked-u128
不溢出，才允许共享收款跨组件；不满足则保守调度，不拒绝原本可执行的批次。
组件业务计算和原序 credit 前缀归并均运行在 AOEM 回调内；最后到达的回调
归并，不等待其他 worker、不另造线程或 AOEM 业务 opcode。出账/nonce
依赖、收费、失败回执与全局结算拒绝后的 AOEM 重算不变。只比较最终余额
不够，本次还比较逐笔 before/after、compute digest、完整回执与 mirror 字节。

环境：Windows 11 x64 10.0.26200，Core Ultra 9 275HX（24 核/24 逻辑处理器），
Rust/Cargo 1.94.0，Release，同机四进程 loopback HTTP/WSS/AOEM/BFT；临时
测试创世与密钥，未接生产服务。干净快照 `cargo fmt --all -- --check`、
`cargo clippy --locked -p novovm-node --release --lib --tests -- -D warnings`
及 Release 集成构建通过。不是整仓所有测试或 GitHub CI 全绿声明。

- Transfer 60 项通过，包括 11 项新纯效应测试（含 192 组确定性混合差分）、
  真实 AOEM 执行、整 Store/费用/nonce/回执对照。1024 笔、128 付款方各
  8 个 nonce 向同一 32-byte 账户入账：旧 1 组件→新 128 组件，真实回调
  峰值 24，原序结果完全一致。没有用 sleep/barrier 制造重叠。
- 费用暂停/溢出时共享收款会跨组件失效，实际 4 笔重算、5 张图；无此
  失败的成功/业务失败/零金额场景 0 重算、1 张图，全部与串行 Store 相等。
- 4 项真实候选测试通过：record-profile 首次计算/恢复/最终确认（42.19 秒）、
  全局费用容量拒绝、20/32-byte 账户共享 signer nonce、u128 供应量重开。
- 四进程混合交易通过（38.91 秒）：6 笔中 5 成功/1 业务失败，串行 oracle、
  失败 nonce、重放、完整块/QC、重启回执一致。该场景不声称测得并行峰值。
- 同条件 256 笔耐久 ACK 持续负载通过：32 signer×8 nonce、4 客户端、
  outstanding 128、proposal 32/collect 0、显式 transport 64、诊断关闭。
  256/256 最终确认，69.015780 秒，**3.709297 TPS**；观察 P50/P95/P99
  =20.948/65.901/68.007 秒，包含 HTTP 与轮询，不是精确提交时刻。四节点
  完整块与持久 BFT 证明一致，重启读回一致；计量后完成检查总耗时 103.46 秒。
  块笔数含 bootstrap 为 `1,2,32,32,32,32,32,32,32,30`。与旧提交约
  3.681/3.749 TPS 相当，不能声称提高主链 TPS；此负载本身是独立收款方，
  不是共享收款热点，也不是逐块完全同切分 A/B。

Transfer 命令的明确过滤为 `native_transfer --include-ignored --skip
native_transfer_process_serial_parity_worker_v1 --test-threads=1`。该 worker
只由四进程夹具提供显式输入调用；此前在脏工作区直接全选 ignored 测试曾
因缺少 `NOVOVM_TRANSFER_PARITY_INPUT` 失败，未删除或放宽其失败关闭检查。
四进程复跑使用当前构建的 libtest 作为 `NOVOVM_TRANSFER_PARITY_WORKER`，
目标 `native_candidate_node_cli` 的精确 ignored 测试分别为
`native_seal_main_process::fresh_record_transfers_conflict_failure_serial_parity`
和 `native_seal_main_process::fresh_record_transfers_durable_receipts_continuous_backlog_measure_rpc_to_finality`。

SHA256（本地证据，原始日志/二进制不随 Git 上传）：

- 节点：`e14c5c568d5240583500c16802ecd27c01053166ed860e3d083c36f3770a493e`。
- 集成 harness：`11adc1768361d3144ae798e62874041e8ea75f64c1c2dae935239077bf05cd4c`；
  libtest：`7a8714843e3cb9eefae7c7619092ba095d4c1d19e38a6e4189cdbc1b1e611e6e`。
- AOEM `windows/core/bin/aoem_ffi.dll`：
  `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
- 快照下 `artifacts/audit/candidate-node-processes/seal-relay-23156-1790879976150247300/mixed-transfer-acceptance.json`：
  `c1161759680b527b06bdd724368e1de8edcbc658c5a8ce4a4dd2a2cd022a23ed`。
- 同目录 `seal-relay-15280-1790880015221735200/transfer-finality-performance.json`：
  `19072b52604103398f7fb5fccbb438f776dffdffdb1c8ae2873aab20c6175f33`。

结论：S1 第一片语义/真实并行/主链不回退验收通过，不等于通用 OCCC、GPU、
完整流水线或高性能主网完成。实体多机、公网和长跑未执行。封印仍明确是
`bft_decision_v3_with_local_aoem_readback`，`zero_knowledge_execution_proof=false`。
S4 核对确认本刀不改业务配置 pin/逐笔承诺，不需切换协议版本；未来证明
必须绑定可信父根、有序交易、业务规则与全局费用结算后的完整结果。

下一刀按计划继续削减实际存储工作：`RecordOverlayV1` 跨 stage 聚积中间
节点，finish 后仍全部预读/写入/读回；先对 physical/receipt 作显式可达
裁剪并量化，state 的逐笔承诺根保留。物理树本已合并净变化，最大 journal
多版本放大在 state，不能把前两树裁剪包装成 S2 完成。后续再处理每笔
3 次稀疏 Store 编码、状态效应增量和阶段流水线。活动目标保持进行中。

## 设备 A：原始语义架构复核与重建启动（2026-10-02）

用户确认按历史设计纠偏并开启目标模式。当前执行依据为
[统一语义执行重建计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)，根 AGENTS
已规定每次续作先读计划最新短交接，再核对本地/远端和双方认领。
起点 `ba3b6a4`；已有 27 个异步试验改动保留且不夹带提交。

第一切片正在接入真实 Transfer：显式 checked-u128 纯收款效应，只有整批
只收不支、父状态一致且全部请求上界不溢出时才消除共享收款账户的假冲突。
组件计算及原序结果归并均在 AOEM worker 中完成；全局费用失败仍按实际
前态校验并由 AOEM 修复，不改 wire、收费、nonce、回执编码或最终性门槛。
本条为启动记录，尚未记入测试/性能通过，不代表目标完成或生产可部署。
计划/分工先独立同步；具体实现待测试后另行提交，不混入此前退步试验。

## 设备 A：真实候选执行流水线（验证中，2026-10-01 至 10-02）

基线 `5043f4c`，main/远端核对一致。本轮把 fresh 主节点候选计算从主循环
拆开，不改交易、收费、nonce、共识编码或 AOEM 内核，不另建权威数据库。

- 通用 storage owner 在线程内独占原 AOEM provider，显式 client scope 通过
  8 槽命令队列共用；Rc/FFI 句柄不跨线程，不新增 unsafe Send。图准入、
  poison、cancel/drain 与未完成 owner 保活复用原代码，无新的图大小阈值。
- 新候选 begin 在锁内仅验证/捕获 owned 输入，不写 catalog/输入节点；worker
  在无 workspace OS 锁视图内执行原业务与三树归并。输出与耐久块使用同一
  构造器得到只读预览，当前父点/轮次和远端完整 subject 通过后才暂存输入。
  finish 重新取锁核对完整描述符、输入字节/来源、abort/retirement 与容量后
  走原 reservation、持久化、读回及完成标记。旧已暂存工作区保持恢复语义。
  计算仍可产生原 AOEM raw precommit 辅助记录；“不占候选槽”不等于整库
  零写入，也不把辅助记录当作正式状态或最终性证据。
- 每节点最多一项待完成候选（包括未消费 completion）；主线程保有 signer、
  交易池、网络和 pacemaker。父点/轮次过期或错误 subject 的新结果不登记、
  不投票，待回调 drain 后只丢 owned 数据，不占候选槽、不删除原交易池，
  也不 abort 已存在的合法恢复工作区。真实完成队列唤醒主循环，不追加
  固定 250ms 等待；RPC 仍遵守原有连接、容量和预算限制。
- 新机制直接启用在实际 fresh 节点入口。旧同步 API 复用三阶段，用于原
  调用者与恢复测试；不能把同步兼容路径的测试冒称为后台执行证据。

当前格式、Exec/Node lib/tests 严格 Clippy 与 Release 构建通过。Exec 42 单元、
3 新真实 storage-owner、5 旧 scope 及 5 真实 compute 测试通过；节点队列 6、
Send 边界 1、RPC 10、交易池 7、账本 28 通过。真实 NCW2 首次计算/恢复
通过（45.95 秒），包括无 namespace 锁执行、禁止计算视图写控制标记、
篡改/abort/retirement 拒绝、原四阶段完成顺序和预览/耐久产物精确一致。
流水线首版 `9d8edff69f1ca8c9ac01a09490f0068065b0ab03765cbe39b829080e055f26a1`
（节点 SHA256，未提交工作区）新旧创世完整回归通过（233.49/156.68 秒）。
两者均运行真实 owner/worker：暂停计算时 RPC dispatcher 查询/入池、真实
WSS 交易与 pacemaker poll 可推进；注入过期轮次后只丢未暂存结果，34 次
capture/drop 无 catalog 增长，错 subject 无槽写入，同 plan 重算/完成且
预览与耐久 subject 相等，三笔待处理交易重开可恢复。轮次是夹具注入，
不是实测 BFT 换轮；计算期间真实 finalized parent 移动和混合 Execute
的 owned-only 路径尚未专测。时钟等待不丢 completion，也不让 RPC idle 忙循环。

同二进制通用 plan 跨进程恢复通过（3.98 秒），四进程混合转账/串行参考/
费用/失败 nonce/回执/重启通过（45.96 秒）。保持原 256 笔持续负载所有
条件且关闭诊断，虽然全部最终确认、四节点块/QC/重启一致、state_version
增长 256，但测得 **131.232 秒、1.950742 TPS、观察 P95 76.737 秒、P99
128.655 秒**；相较 `5043f4c` 的 3.681/3.749 TPS 发生退步，不能签收性能。
块切分 `3,7×32,10,12,7`（另有 bootstrap），不是逐块同批次 A/B。证据：
`artifacts/audit/candidate-node-processes/seal-relay-14252-1790870285268175100/transfer-finality-performance.json`；
混合对照为 `seal-relay-8160-1790870228970104000/mixed-transfer-acceptance.json`。

继续修复的是数据访问结构：owner 逐点 Get 增加跨线程往返，节点/分片的
不可变内容写前核验和写后读回改为 64 键有界批读。保留每字节/哈希检查、
原 graph 提交顺序、完成标记和不确定写入锁；写后再次发真实读请求，
不是读缓存、原子快照，也不提高交易/队列准入限额。已补独立异步阶段耗时
诊断，不能把旧同步 `successor.prepare.execute` 当成新 worker 耗时。
批读版最终测试二进制 SHA256：
`6a44d39bb848d32f687e781dae79ac8134b8195b8ff91ad535289065f0f47fe4`。
格式、严格 Clippy 与 Release 构建通过；Exec 42 单元、4 真 owner 测试
（包括 135 keys 实际 3 次 owner 命令）、状态存储 5 单元+2 真库恢复/损坏
测试通过；真实 NCW2 候选/四阶段恢复再次通过（45.93 秒）。

同条件关闭诊断复测仍只有 **117.653 秒、2.175892 TPS**，观察 P95/P99
为 78.647/111.205 秒，256/256、四节点完整块/QC/重启一致、state_version
增长 256。块切分 `2,6×32,24,16,8,12,2`（另有 bootstrap）；仍不是相同
逐块切分 A/B，不能宣称稳定增益或饱和吞吐。证据：
`seal-relay-15220-1790871294341262900/transfer-finality-performance.json`。
诊断轮 `seal-relay-1952-1790871127978909600/transfer-finality-performance.json`
为诊断开启，107.046 秒/2.391501 TPS，**不是正式成绩**。其节点 0 日志中
`lifecycle.publication_poll` 102 次累计 21.873 秒、`lifecycle.service_poll`
50 次累计 12.164 秒、完整发布/重新接收 10 次累计 14.035 秒；候选 worker
10 次累计 4.520 秒、finish 10 次累计 8.957 秒。均为 >=10ms 调用的墙钟
统计，嵌套阶段不能相加，也不能把 worker 的总耗时当 AOEM 业务回调耗时。

**结论：功能回归通过不等于性能通过；当前试验代码尚未提交/推送，远端
运行代码仍保留 `5043f4c`。本次只同步文档，不把退步版并入共享代码基线。**
A 继续独占已认领的执行/存储/fresh 生命周期范围，下一步依据真实诊断拆除
固定已确认发布帧的重复完整验证、隔离候选持久阶段与共识主循环的阻塞；
若使用已验证内容复用，必须明确当前发布绑定与新签票/晋升权限的区别，
保留当前指针/意图变化立即拒绝、公开完整 Verify、损坏查询拒绝和重启检查。
不删除验证来凑 TPS，不扩大原限额或期限，不回到“普通低吞吐链先交付”的路线。
物理 LAN、Linux 整链、公网与长跑未执行；不声称全流水线完成、吞吐达标、
多候选投机并行或生产可部署。此前 mixed/完整序列结果对应首版二进制；
批读版的混合失败交易完整序列尚未重跑。

## 设备 A：候选级批次执行与耐久接入重构（2026-10-01）

基线 `c47f6dd`，main 与远端核对一致。用户明确要求从根本架构兑现高性能
主网，停止把低吞吐普通转账链及局部调参当作交付方向。本次直接修改现有
签名 Transfer 主链路径，不新建链或分支，不改 AOEM、不扩容限额或测试期限。

1. 普通 RPC 的同轮已收完整且连续提交，和 Overlay 的现有入池批次，使用
   真正的一次同步 WriteBatch。按原输入顺序逐项鉴权、去重和检查容量；
   内存发布、queued 响应及持久 ACK 均不能先于写入成功。RPC 查询是分组
   顺序屏障，不把后来的提交提前呈现给它；原 8 连接、报文和期限限制保留。
2. 原“连续无冲突段 → 每段新建 AOEM 会话”改成“候选级会话 → 独立账户/
   nonce 连通组件一次图提交 → 原交易顺序归并”。真正业务计算和组件内
   后继输入由 AOEM 回调执行，Host 不先算余额结果。全局费用失败使真实前态
   不同，则使受影响组件后缀失效，同会话逐笔重算，每笔最多额外一次。
   原收费、失败 nonce、回执和输出摘要编码保持。连通组件保守地串行内部
   交易，不等于最大并行依赖 DAG；单个大连通组件可能没有回调重叠。
3. 账本验证结果只在同一物理 DB owner、同一 RocksDB sequence、同创世/
   namespace 内复用，并只在真实 runtime lease 内启用。任意 Put/Delete/
   WriteBatch（包括同字节写）均使其失效；最后 lease 释放清除。没有缓存
   签票权限，当前父/AOEM/候选绑定及原锁不变；显式全历史 audit 清缓存并
   完整复验。此机制不是检测不改变逻辑 sequence 的物理文件损坏的持续
   磁盘巡检；变更后仍完整验账，尚未解决每次追加的历史复杂度。
4. 通用计算会话在失败后禁止复用；超时或 Host unwind 时未 drain 的完整
   session/DLL/回调捕获一起保留，不能释放仍被 AOEM 访问的内存。

已完成格式、Exec/Node lib/tests 严格 Clippy 与 Release 构建。Exec 5 单元+
5 真库通过；Transfer 算术/计划 23、执行器 4、dispatch 5、账本 40、RPC 9、
交易池 7 通过。新增真实 AOEM 128 笔/16 组件一次图，实际 callback peak=13，
同会话第二次提交通过；这不是主链 TPS。7 笔混合状态的成功/业务失败/
收费暂停/结算计数溢出四类全 Store 与 mirror 字节串行一致，后两类实际
重算 5/3 笔。旧 finalizer 两项及旧 wave 真库参考通过。真实 NCW2 5 笔候选
与恢复通过（41.57 秒），1 组件/1 图/peak=1；不再把业务笔数写成回调任务数。
交易池写失败测试是提交前注入，不能宣称覆盖真实 OS fsync 不确定结果。

新旧创世完整四块最终性/重启回归通过（224.37/139.03 秒），包含同 DB
runtime lease 下复用未变化历史，以及写坏旧 QC、历史索引或归档后立即拒绝。
最后仅修正测试日志将业务笔数叫 tasks 的错误文字，重新格式检查及 Release
构建；最终节点 SHA256 为
`2fa4c185ed104f931c1ab1bcd8b788d40f2c185161a8d30e6f99f1ca9903937a`。
最终二进制四进程混合/失败/费用/nonce/回执/重启测试通过（39.17 秒）。
无并行构建/重测、诊断关闭，沿用相同 Windows 同机四进程、32 签名者各
8 笔、4 客户端/单入口、outstanding 128、RocksDB、64 传播预算、32 笔
提案、0 收集窗、显式持久 ACK 和原 300 秒期限；预签名/bootstrap 不计入。

| 原 256 笔持续场景 | 窗口 / 最终确认 TPS | 观察 P95 / P99 |
| --- | --- | --- |
| 第一轮 | 69.544 秒 / 3.681 | 65.742 / 68.715 秒 |
| 复测 | 68.280 秒 / 3.749 | 65.587 / 67.554 秒 |

两轮均 256/256、state_version 增长 256、四节点完整块一致、QC 验证和重启
读回通过，最终池清空。块切分分别为 `2,7×32,30` / `3,7×32,29`（另有
bootstrap），不是逐块完全相同的 A/B。发送队列接纳总数 8046/5303，
四节点有效 ACK 计数分别 `[1487,1694,1694,1377]` / `[1163,1115,1171,1154]`，
均可含重复而非唯一交易数。relay 仍有 readiness 等待到期和连接关闭日志；
没有签收连接稳定性。较旧 2.549/2.681 TPS 有改善，但 **3–4 TPS 仍不满足
用户要求的高性能主网，不标记高性能完成**，不宣称稳定倍率或饱和吞吐。

证据父目录 `artifacts/audit/candidate-node-processes/`：
`seal-relay-15540-1790866311750762500/transfer-finality-performance.json`、
`seal-relay-27964-1790866415471928700/transfer-finality-performance.json`；
混合串行/费用/nonce 对照为
`seal-relay-25316-1790866272621070600/mixed-transfer-acceptance.json`。
实体 LAN、Linux 整链、公网及长账本/长跑本轮均未执行，无生产部署。

同二进制额外诊断轮也通过完整块/QC/重启，报告为
`seal-relay-20504-1790866540077501000/transfer-finality-performance.json`。
诊断开启的 4.057 TPS 不作为正式对照或宣传结果。实际四个节点各 9 个业务
候选均为 1 session/1 graph、无重算；组件数依次为
`3,9,10,19,20,23,24,5,4`，实际回调重叠范围 1–6，证实新执行计划经过真实
签名→组块→AOEM→BFT 路径，而非只在独立算术测试运行。
每节点记录到主生命周期处理 31.368–31.746 秒、prepare.execute 8.015–8.269
秒、发布轮询 4.646–6.761 秒、persist_delta 3.578–3.731 秒；完整 ledger
检查仅 25–26 次至少 10ms 的慢调用，共 0.465–0.474 秒。阶段有嵌套不能相加，
小于 10ms 的调用不在这些求和中，不能解释为总 CPU 账单。候选组件计算
没有达到 10ms 的慢调用；后续重点不再是反复重开计算会话，而是候选状态/
证据的同步处理、发布/共识状态机，以及每次主循环处理后仍整段等待的调度。

当前真实主链仍有同步生命周期瓶颈：fresh 分支提前进入专用同步确认；
旧 full-async/worker 字段也不证明存在真实独立 worker/完成队列。
下一架构阶段必须拆分 owned input 捕获、AOEM 异步计算、原序归并与发布，
在不持 workspace 锁等待的前提下让 RPC/网络/轮次继续推进；不得后台调用
原整段 prepare 后仍用同一把锁阻塞前台，也不得另开第二个权威账本。

## 设备 A：发布轮询的同锁只读验证去重（2026-10-01）

基线 `44b307c`。先用原 256 笔场景打开现有慢调用计时，完整块/QC/重启
通过；每节点记录到主循环处理 77.32–80.26 秒，完整 ledger 校验
18.44–19.09 秒，发布轮询 16.66–28.55 秒。仅记录至少 10ms 的调用，
阶段互相嵌套，不能相加或把残差全部算作某一模块；诊断开启的吞吐不作
正式对照。原报告：`artifacts/audit/candidate-node-processes/`
`seal-relay-23352-1790861307092251400/transfer-finality-performance.json`。

本次仅优化无 capture 回调的 `Scope::Verify`：同一次 workspace/authority
锁内先取 Ready/已完成输出绑定，完整核验账本后复用直接父归档，完整验证
子块一次。绑定必须与真实 artifact 一致，原始交易鉴权、三树变更、父来源/
QC 和末尾 head/父子发布证据仍验证；没有跨调用权限缓存。签票回调、发布
写入、Ledger/Finality 恢复路径保持原读回顺序，不修改 AOEM/交易经济规则。

新增测试复用真实第三块：父块是 NCW1 Execute、子块是 NCW2/V3 Transfer，
不能错误要求两者均为 NCW2。首轮测试因该夹具假设失败，已修正测试条件，
未改变生产校验。修正后格式、Node lib/tests 严格 Clippy、Release 构建通过；
新旧创世完整四块恢复分别通过（222.77/137.86 秒）。真实第三块 Prepared/
Published/Finalized 核对 artifact 与完整 ledger 验证各一次，报告与旧 getter
逐字段相同，未发布候选仍拒绝。在 Published/Finalized 分别覆盖十类真实
AOEM 记录损坏：输入/输出/completion、父来源/Ready、父子发布证据、head、
输出树节点及实际记录 blob。每次独立 public Verify 拒绝、观测证据不变、
恢复原数据后返回原结果；另覆盖缺父 archive/finalized-intent 和未知账本键。
这不是整个数据库写序号审计，也未声称整仓全部测试重跑。真实增量转账
候选另通过（45.63 秒）：5 tasks/peak 2、无全量物化、串行一致及恢复保持。

最终 Windows Release 节点 SHA256：
`71e43d3af710cc1cd8305ea98e0f7c4fc8097b866d5b325769b68c2c43b7c0b1`。
诊断关闭，无并行构建/重测，沿用同机四进程、单入口四客户端、32 个签名
账户各 8 笔、RocksDB、64 传播预算、32 笔提案、0 收集窗、显式 ACK；
预签名/bootstrap 不计入，延迟包含 HTTP/轮询，原 300 秒期限不变。

| 原 256 笔持续场景 | 窗口 / 最终确认 TPS | 观察 P95 / P99 |
| --- | --- | --- |
| 第一轮 | 100.438 秒 / 2.549 | 91.866 / 92.142 秒 |
| 复测 | 95.478 秒 / 2.681 | 91.537 / 95.011 秒 |

两轮均 256/256、四节点完整块/QC/重启通过，状态版本增长 256，业务块
均为 `3,7×32,29`（另有 bootstrap）。报告仍在上述证据父目录，对应
`seal-relay-25848-1790863337934286100/transfer-finality-performance.json`、
`seal-relay-18524-1790863487327081400/transfer-finality-performance.json`。
最终二进制另通过 6 笔混合交易串行/费用/nonce/回执/重启核对（40.04 秒），
5 成功/1 失败，报告 `seal-relay-15876-1790863654726199400/mixed-transfer-acceptance.json`。

两样本略好于前轮 2.501/2.004 TPS，但仍只有约 2–3 TPS，不宣称稳定倍率。
relay 仍出现 readiness 等待到期，全生命周期会话数为 22/33（含准备和
重启），不是已修复压力下连接问题。下一步审核已封印固定证明的重发路径：
重发不是新签票，区分必要动态发布检查与每 tick 重建三树/完整历史的成本；
后者尚未改动，公开 Verify、签票、执行及恢复不得因此失去完整验证。
实体 LAN、Linux 整链、公网、长账本和长跑未执行；没有改 AOEM 或部署。

## 设备 A：fresh 持久接收确认减少交易重传（2026-10-01）

基线 `af76987`。仅修改 SUPERVM，新增显式
`transaction_transport.durable_receipts=true`；默认 false，不改变旧数值预算、
交易/费用/共识规则、300 秒持续场景期限或生产配置。所有 peer 须先升级到
理解新 disposition 的版本；旧二进制会拒绝 code 2，没有能力协商或自动启用。

- `JournalPersisted=1` 原签名字节不变；新 `PendingTransactionPersisted=2`
  只能用于 NativeTransaction。旧 journal 消费者必须先检查类型，不能用
  pending-pool 接收确认完成 journal 义务，更不能将任一 ACK 当成投票/QC。
- 接收端逐项鉴权、核对原文摘要和 delivery id，完成所需最终父点读、同步
  入池与清理后，才为仍在池中的精确原文返回签名 ACK。结果按输入位置对应，
  不用 hash 集合掩盖同 hash 不同 raw；被丢弃/过时/拒绝项不获确认。中途读写
  失败返回错误而不是部分 ACK；此前已成功的磁盘写并不因此被宣称全批原子。
- 发送端重新验证签名、链/路由/delivery id、固定 peer 和本机池原文摘要，
  仅抑制对该 peer 重播，不删除池交易、不推进 nonce 或最终性。每 peer 最多
  1024 条内存记录，随活池清理；缺 ACK 或队列背压按原预算重试。重连/隔离
  清空确认和本窗口 sent，不刷新配额，不丢 staged ingress。ACK worker 使用
  独立 peer 轮转，首 peer 持续积压不能饿死后续 peer。
- ACK 不是当前会话/创世 epoch 的证明，也无 TTL：有效旧 ACK 在 reset 后
  仍可再次接受。正常持久池只因最终化或 nonce 已消费而删除；手工删池、
  回滚备份、复用 chain_id 重建创世不在恢复保证内。对端可谎报自己收到了，
  但不能借此删除本机交易或获得共识权力；不把接收确认称为节点可信证明。

格式检查、严格 Clippy 和 Release 构建通过。Overlay 37/37（含新类型、
篡改/路由、有效签名但错类型、旧字节兼容与公平轮转）、transport 22/22、
配置 17/17、旧消费者 5/5、profile 差异 1/1 通过。真实 AOEM record 转账
候选通过：5 tasks/peak 2，重放不写盘，4 类损坏点读不返回 ACK 前缀，
重开内容/写序号一致；新旧创世四块恢复分别通过。双端真实 WSS 测试验证
先入池再 ACK、重开后原文一致、坏确认不抑制和确认不改变共识状态；此两组
四块测试在最后纯 worker 公平轮转修正之前执行，最终 worker 已复跑完整
Overlay 并进入下述四进程场景。没有声称整仓全部测试重新通过。

同一最终 Windows Release 节点 SHA256：
`ce5d23036c78dc19182349fb73e30c44721f971c33cfc40dde5af0dcd4454bc6`。
硬件、同机四进程、单入口 4 客户端、32 签名账户、RocksDB、每 peer 64
交易预算、每提案 32 笔、收集窗 0 与前轮相同；预签名和 bootstrap 不计入
窗口，观察延迟包含 HTTP/轮询。以下同二进制场景依次独立运行，期间没有
并行构建/重型测试，全部交易、四节点完整块/QC 及重启读回通过：

| 场景 | 窗口 / 最终确认 TPS | 观察 P95 / P99 | 交易队列接纳 / ACK 接受 |
| --- | --- | --- | --- |
| ACK 开启，96 笔分批 | 38.258 秒 / 2.509 | 15.284 / 15.289 秒 | 1939 / 1783 |
| ACK 开启，256 笔持续首轮 | 102.359 秒 / 2.501 | 91.526 / 101.345 秒 | 8314 / 6166 |
| ACK 关闭，同一 256 笔对照 | 293.465 秒 / 0.872 | 289.316 / 292.878 秒 | 145037 / 0 |
| ACK 开启，同一 256 笔复测 | 127.752 秒 / 2.004 | 124.322 / 127.262 秒 | 25559 / 11708 |

队列接纳不是实际送达；ACK 计数可含重复有效确认，不是唯一交易数。三个
256 场景均增长 256 个状态版本，结束时池为空；业务块均为 9 个，开启为
`2,7×32,30`，关闭为 `3,7×32,29`，没有把不同切块称为逐块 A/B 完全相同。
两个开启场景各四节点均实际消费 ACK。关闭默认仍有严重重复传播，不能说
默认性能变快；开启减少重传并改善本次持续场景，但不是稳定倍率或高吞吐。
96 笔与前轮 2.499 TPS 接近，不能宣称所有负载提速。

三个持续场景测量期的断线分别为 2/34/20，peer 隔离 0/188/15，等待 relay
发送结果期间入站缓冲满为 0/3/1；各轮均未见解密错误。开启复测仍有 19 次
10054 和 1 次缓冲满，说明连接稳定性尚未解决。全生命周期 relay（包含准备
和重启，不是仅测量期）转发 18354/100616/33133 帧，接纳 wire bytes 为
97625842/304335121/140831135，均正常停止；两次开启的差异不隐去。

证据父目录 `artifacts/audit/candidate-node-processes/`，对应报告均为
`transfer-finality-performance.json`：96 笔
`seal-relay-18344-1790860120545209300`，256 开启首轮
`seal-relay-16044-1790860204299098400`，关闭对照
`seal-relay-2696-1790860362188408400`，开启复测
`seal-relay-4976-1790860691906695500`。开启组另存
`transfer-recipient-ack-observations.json`，未只以开关为 true 判定新路径生效。

原 6 笔混合/失败/冲突回归（ACK 默认关闭）也通过：5 成功、1 业务失败，
完整 Store/手续费/nonce/回执与串行参考一致，QC/重启及最终重放幂等通过；
`seal-relay-27216-1790860868942836200/mixed-transfer-acceptance.json`。
下一步使用现有分段计时定位残留候选/历史验证停顿和 relay admission 等待，
不继续靠提高队列容量解释低吞吐，仍不签收为稳定高性能或生产可用。

实体 LAN、Linux 整链、公网、长账本和长跑本轮未执行；没有改 AOEM 或部署。

## 设备 A：过滤已消费交易并减少排队与历史验证重复（2026-10-01）

基线 `a1ac0f1`，仅修改 SUPERVM，保留原交易/经济/共识规则、全部鉴权及
300 秒持续负载期限。三个实际路径修正：

- 远端交易先全部验证签名、链域与报文摘要，再在同一个最终父只读视图中
  检查回执/nonce，过滤已确认（含失败回执）或已消费的交易后才入池，避免
  旧交易每轮同步 put 后再 delete。当前/未来 nonce 仍可排队；已有 pending
  仍核对完整 raw。读失败前零新增写入仅指本批准入所需的点读，不是整个
  poll 或多笔磁盘写成为原子事务；后续原 reconcile 与持久失败停机语义保留。
- Overlay 的单 peer/mesh 待发送队列只合并同 peer、class、hash、摘要及
  完整 payload 均相同且原项未过期的 NativeTransaction，释放重复项 permit；
  原 FIFO/TTL 不变。封印不合并，发出/过期/重连仍可重试，这不是 recipient ACK。
- 完整有序历史遍历仅在同一次调用内复用已验证父块，仍逐块验证当前 QC、
  父链接、执行绑定和发布索引，冷入口及每次独立读取完整验证保留。仍为
  O(history)，没有跨调用权限缓存，也未声称所有调用者持有同一锁。

严格 Clippy、Release 构建、格式检查通过。Overlay 31/31（新增 5 项队列
边界）及账本 26/26 通过；record 转账真实 AOEM 夹具通过，RocksDB 写序号
证明重放不写盘、4 类损坏点读不写入前缀、重开不变，首次 NCW2 仍观测
5 tasks/peak 2 且禁止全量物化。新旧两种创世的完整四块生命周期分别通过，
证明每次历史遍历保留 N 个当前证书检查，父证书从 N 次变为 1 个创世 seed；
独立读取之间篡改旧父 archive/record、当前 QC、索引或域信息仍拒绝。

同一 Windows Release 二进制四进程实际测量（未并行重型测试/构建）：

| 场景 | 结果 | 窗口 / 最终确认 TPS | 观察 P95 / P99 |
| --- | --- | --- | --- |
| 原 256 笔持续积压第一轮 | PASS，256/256，完整块/QC/重启相同 | 199.545 秒 / 1.283 | 196.330 / 199.038 秒 |
| 原 256 笔持续积压复测 | PASS，256/256，完整块/QC/重启相同 | 213.411 秒 / 1.200 | 201.992 / 211.823 秒 |
| 原 96 笔短场景 | PASS，96/96，完整块/QC/重启相同 | 38.417 秒 / 2.499 | 16.063 / 16.081 秒 |
| 6 笔混合交易 | PASS，5 成功/1 失败，串行全状态/费用/nonce/回执及重启一致 | 非性能测量 | 不适用 |

证据均在 `artifacts/audit/candidate-node-processes/`：第一轮
`seal-relay-20068-1790857613807309600/transfer-finality-performance.json`，复测
`seal-relay-17680-1790858004910448000/transfer-finality-performance.json`，短场景
`seal-relay-2192-1790857916059662200/transfer-finality-performance.json`，混合场景
`seal-relay-12528-1790857876308600200/mixed-transfer-acceptance.json`。
节点 SHA256：`ccd32c3e2fcab1c6f06cfc32ac55ee8484e0f761d854e21aaae4bfa3c71cbf93`。
硬件/拓扑和显式 64 传播预算、32 笔提案上限、0 收集窗均同前轮，预签名和
bootstrap 不计入窗口；延迟包含 HTTP/观察轮询，并非精确共识提交时间。

第一轮持续场景首次在原期限完成，但仍有 38 次节点断线、133 次 peer 隔离，
relay 注册/断开 58/58，转发 59978 帧、接纳 201224802 wire bytes；四节点
测量日志未见 BadRecordMac/DecryptError。复测仍有 40 次断线、167 次 peer
隔离，relay 注册/断开 60/60、转发 73457 帧、接纳 238019529 wire bytes，
也未见上述解密错误。两轮均为 9 个业务块（3、7×32、29），但不是稳定
或高性能签收，三项
修正也没有分别进行消融测量；短场景比上一轮 2.596 TPS 略低，不能宣称普遍
提速。下一步接通已有 recipient ACK 的持久接收语义以降低每秒重复广播，
必须覆盖坏 ACK、落盘失败、丢 ACK、断线/重启；不能把 relay 入队当送达。
实体 LAN、Linux 整链、公网、长账本和长跑本轮仍未执行；没有改 AOEM 或部署。

## 设备 A：修复 Windows relay 超时语义及服务端 TLS 错误重试（2026-10-01）

基线 `41c454e`。先在未改生产行为的 daemon 上运行三个真实 rustls/loopback
故障反例，全部复现预期：模拟 socket 实际发送 7 字节却返回超时，原
`Stream::write` 吞下写错误后，下一次 flush 重发密文前缀，对端得到真实
`DecryptError`。这是注入的“不确定进度”反例，不是已捕获实测网络的每次
系统调用，不能据此声称全部历史 BadRecordMac 都已归因。

本轮修改两处：daemon 的写/flush 错误永久终止连接，成功读写字节数不被
后置检查错误覆盖；Windows client/daemon 不再将阻塞 `SO_RCVTIMEO` 当作
普通可恢复空闲，而以现有锁定 Mio 1.1.1 的安全非阻塞收发和单方向 readiness
等待保留原预算。实际 OS 超时与 readiness 等待结束严格区分；绝对截止不
因重试续期，clear/begin 不能复活失败连接，虚假关闭事件不能代替实际读写
结果。不新增 runtime，不放宽 TLS/身份验证、队列容量或 300 秒验收标准。
Windows 平台依据见 [Microsoft socket options](https://learn.microsoft.com/en-us/windows/win32/winsock/sol-socket-socket-options)；
就绪事件处理遵循 [Mio Poll 的平台契约](https://docs.rs/mio/1.1.1/mio/struct.Poll.html)。
Linux 保持原阻塞 I/O 与内核有效 timeout，不引入 Windows 依赖。

严格 Clippy 与 Release 构建通过；relay 48 项、Overlay 26 项通过。新增
daemon 6 项真实 TLS/故障测试及 socket 6 项测试，包含真实背压后完整 8 MiB
字节一致性、读空闲后双向通信、EOF、截止不续期及终止后零后续 I/O。
初轮两个新 socket 夹具断言失败，已修正为检查当前 socket 内核选项、在
连接前限制接收窗口后复跑全 48 项通过；未删除背压断言或降低字节要求。
WSL 仅直接编译同一 socket 源码并运行非 Windows 的 3 项测试通过，不是
Linux 整个 node 构建或多节点验收。

第一次持续负载仍记 **FAIL/不可用于性能比较**：relay 在约 35.206 秒以
`reason=error` 提前退出；163 笔入池、35 笔四节点确认，最高确认 h3，
300 秒末池为 `[128,96,96,96]`。退出恰逢本次操作读取实时 `report.json`，
疑似 Windows 替换报告时的共享冲突，但原始退出错误未被夹具保存，不能
写成已证实根因。本轮不读取活跃报告重新运行；没有修改报告失败策略。
证据：`artifacts/audit/candidate-node-processes/seal-relay-26912-1790855059664174200/transfer-finality-observations.json`。

第二次持续负载 **FAIL**：relay 正常存活至测试结束，256/256 入池，300 秒
内四节点共同确认 194 笔，最高确认 h8/state_version 195，各池剩余 62；
末尾 h9 `Prepared` 不算确认。最后入池约 96.711 秒，最后一次四节点确认
观察约 237.450 秒。日志交叉核对 h1–h8 的块/状态/回执根一致，但此失败
路径没有执行最终完整离线 QC/重启验收，没有通过型性能报告。
四节点测量日志的 `BadRecordMac`/`DecryptError`/`cannot decrypt` 均为 0；
断线 18 次（17 reset、1 入站事件缓存达限），peer 隔离 72 次（58 offer
接纳拒绝、13 offline queue 接纳拒绝、1 握手过期）。relay 另有写 readiness
等待到期后的终止输出，不把它冒作普通读空闲。中继注册/断开 34/34、
转发 141740 帧，接纳 wire 370757378 字节；重复传播和背压仍很严重。
这比历史基线少会话抖动且未复现解密错，但完成量仍为 194，不是吞吐提升。
证据：同父目录 `seal-relay-17008-1790855399889062000/transfer-finality-observations.json`。

同一生产二进制的原 96 笔短场景 **PASS**：完整四节点块/QC/重启读回、
96 笔成功回执及 state_version 增长通过。窗口 36.982 秒、2.596 TPS，
观察 P95/P99 为 16.236/16.379 秒，业务块 `3,29,4,28,2,30`；不是稳定
加速或饱和吞吐成绩。报告：同父目录
`seal-relay-3436-1790855753712531900/transfer-finality-performance.json`。
节点 SHA256：`cc1699eea36786d2bf667dcd66ec40ec649e9fb6b9ffcf4a7c79d3ccfa958190`。
两次负载及短场景未并行构建/其他重型测试，环境与下一节相同。

下一步仍需降低实际重复传播和候选/账本检查开销，再复跑持续负载；不能
将本次 TLS 修复当作高性能目标完成。实体 LAN、Linux 整链、公网和长跑
本轮未执行；没有改 AOEM、交易/经济/共识规则、正式创世或运行中服务。

## 设备 A：持续积压负载暴露未完成确认与 TLS 错误恢复缺陷（2026-10-01）

基线 `df20be4`。新增沿用原四进程夹具的持续供给场景：32 个真实签名账户、
每账户 8 笔（共 256 笔）、单入口 4 客户端，最多 128 笔未观察完成的交易；
不再等整批全部最终化才补下一批。保留原 300 秒完成时限、鉴权、费用、
nonce、完整四节点块/QC 与重启检查，不扩大协议或存储容量。每轮最多查询
32 个待观察的交易/节点组合，游标公平轮转；提交明确拒绝与结果不明分开记录。
失败也先保存提交、观察、实际池/高度与耗时证据，不生成成功性能报告。

未改生产代码的第一次运行 **FAIL**：256/256 入池，300 秒内仅 194 笔在
四节点确认，实际四池仍各有 62 笔；没有把当前 `height=9, CollectingVotes`
误当作第 9 块已确认。最高确认高度为 8，业务块笔数为
`2,32,32,32,32,32,32`，四节点各块 hash/state/receipt roots 一致；失败路径
未执行完整离线 QC/重启检查。最大未完成 128，224 笔在上一 nonce 轮尚未
完全观察结束时提交，确有持续积压。最后入池约 140.132 秒，之后约 160 秒
仍未排空，不能只归因客户端查询滞后。单次提交 P50/P95 为 9.73/66.77ms；
最后一次状态采样约 299.342 秒，四节点均未 halted。观察全扫最大 20.938 秒，
故所有延迟仍是客户端观察上界，不是精确落盘时间。

失败证据：`artifacts/audit/candidate-node-processes/seal-relay-4900-1790850830073370600/transfer-finality-observations.json`。
同目录 `acceptance.json` 仅代表 bootstrap 通过，**不是 256 笔通过**。
relay 记录 170 次会话注册、127311 帧转发及多次 `BadRecordMac`/网络超时；
四节点累计交易队列接纳 118468 次、入站尝试 88946 次，重复传播放大已证实。
fresh 路径没有发送 recipient ACK，不能把此轮直接归因 ACK/payload 调度失衡。

定位并修复 client 的独立真实缺陷：底层成功读写的字节数不得被后置截止或
socket timeout 恢复错误吞掉；写失败永久终止该连接，操作 deadline 清理不能
复活，close 不再继续写 TLS。Ping 回写及读取时 rustls 隐式发送的写超时均不
属于正常读空闲；真正的读空闲仍可继续 heartbeat。fresh 主循环现在记录原已
有界的断线、peer 隔离及 worker 错误，不记录交易/envelope payload。不改 TLS
密码、协议字节或信任校验，不宣称它已解释上述 `BadRecordMac` 的全部原因。

客户端 20 项（含新增 7 项故障/恢复测试）及持续供给 3 项纯调度测试通过后，
第二轮打开固定标签耗时诊断，仍 **FAIL**：256 入池、163 四节点确认、四池
各余 93；最高已确认高度 7（state_version 164），尚不能写成第 8 块完成。
证据为 `seal-relay-20000-1790852353279822500/transfer-finality-observations.json`，
同在上述父目录。新日志明确记录 135 次等待发送结果时的入站缓存容量拒绝、
29 次连接 reset 和 4 次不能解密 relay 消息；不是推测中的 ACK 问题。
本轮诊断不能作为 TPS 对照。四进程慢调用累计 `main.lifecycle_poll` 559.64 秒、
`service_poll` 272.87 秒、`ledger.load_verified` 150.25 秒/8497 次；它们嵌套且
跨进程，不能相加或当作墙钟。单次主循环最大 2.857 秒，慢回执查询总计
0.17 秒，不能把客户端等待 283.369 秒解释成 RPC 查询计算消耗。
随后只修 mesh 主动发送调度：每次 offer/ACK/payload 前重新检查已经解码的
入站积压，先由原完整 handler 消费；心跳、到期维护、peer 轮转、响应式握手
及原缓存上限不变。未扩大队列、改变 FIFO、跳过验证或延长 300 秒验收时限。

最终补丁通过严格 Clippy、Release 构建、客户端 20 项及 Overlay 26 项回归
（含新 3 项调度测试、真实双向通信、三节点重启、坏消息隔离和资源退出）。
关闭诊断后的第三轮仍 **FAIL**：256 入池、195 四节点确认、四池各余 61，
最高确认高度 8（state_version 196），第 9 块仍未完成。相比首轮 194 笔，
多出的 1 笔来自首个业务块从 2 改为 3，两轮都只有 7 个业务块，不是多完成
一块或证明提速。错误日志为缓存容量拒绝 5、reset 21、不能解密 8；缓存拒绝
减少但没有消失，TLS 根因也未解决。证据：同父目录
`seal-relay-15588-1790852872397069800/transfer-finality-observations.json`。
另有 159 次 peer 隔离：目标侧 offer admission 拒绝 118、offline queue
admission 拒绝 28、握手过期 13；目标会话压力同样尚未关闭。
此轮同样没有成功性能报告、没有执行最终完整离线 QC/重启验收。

同一最终二进制的原 96 笔短场景通过，包含四节点完整块/QC、96 个成功
回执、状态版本增长及重启读回。窗口 37.235 秒、2.578 TPS、观察 P95/P99
15.792/15.799 秒，业务块 `3,29,5,27,5,27`；仍出现一条 relay `BadRecordMac`。
这是短场景回归，不是持续负载或稳定提速签收，不能抵消上面的三轮 FAIL。
报告为 `seal-relay-13772-1790853239346964300/transfer-finality-performance.json`。
最终节点 SHA256：`1d2523309e6a5e5c6054d46df3fc2fed5e41422cfc82aac087e5df93eb6a8d3c`。

复跑：设置 `RUST_MIN_STACK=33554432`，关闭 `NOVOVM_NATIVE_FRESH_TIMING`，
运行 `cargo test --locked --release -p novovm-node --test native_candidate_node_cli fresh_record_transfers_continuous_backlog_measure_rpc_to_finality -- --ignored --nocapture --test-threads=1`。
它需要独占测试 loopback `127.0.0.2:443`，不停止已有服务；不要同时构建或跑
其他重型任务。剩余工作聚焦实际 relay 的 TLS 解密/断线和单次等待发送结果的
积压处理，再以同一负载复验；不靠提高缓存/时间常量或删除实时安全校验收口。

本轮另复跑实际 AOEM 独立任务重叠（peak 12）、NCW2 首次公开计算（5 任务、
peak 2、禁止全量物化）、候选/串行结果/恢复和累计状态超过旧上限的专项通过。
点读门禁中，增加 1024 账户和 100 旧回执后业务记录读取仍为 66，树节点读取
从 194 增到 654，因此不能说整个路径历史无关。旧快照 9104017 字节对应的
增量文档为 398 字节、1 个变更 blob/155 字节、8 个变更节点，重开与 authority
不变检查通过。这些组件结果不抵消持续负载失败，也不是长链主网签收。

环境为本机 Windows/MSVC Release，Core Ultra 9 275HX（24 核/线程）、
约 63.43 GiB 可用物理内存、Samsung MZVLC2T0HBLD-00BL2 NVMe；四主节点
进程与 relay 同机，未并行构建/重型测试。未执行实体 LAN、Linux 主节点、
公网、断电或长跑；未改 AOEM、经济规则、正式创世、PQ/隐私协议或运行中服务。
高性能交易目标继续进行中，不降低为“能出块即可”。

## 设备 A：交易准入与自动提案分离、真实混合交易串行核对（2026-10-01）

基线 `35f6b74`。补齐一个实际节点能力：fresh 验证节点可显式开启
`transaction_ingress_enabled=true`，在 `propose_successors=false` 时仍通过
原 loopback RPC 接收签名交易、同步落入有界交易池并经原受限 gossip 传播。
新开关默认 false；原 proposer 仍隐式启用交易池，普通 receiver 默认不变。
显式准入要求原 `receive_successors=true`、`follow_finalized_tip=true` 和
完整 pinned fresh V3 配置。RPC 监听仍需原显式配置，不新增公网入口。
入池不扣费、不推进 nonce、不改变 AOEM 权威状态；自动提案仍另行检查配置
和当轮 leader。关闭自动提案不是禁止验证/签票：原 receiver 的权限不变。
严格布尔解析、路径隔离、鉴权、容量、持久化及恢复校验均沿用原路径。

新增同机四进程真实 gate：单入口提交 6 笔签名 Transfer，四节点全部入池
且无新最终块后停止；重新启用提案，从持久池恢复，实际组成一个含 6 笔的
后继块。覆盖同一付款方连续 nonce 冲突、独立付款方、余额不足失败、自转和
失败后的继续花费。重复 nonce 的不同交易被明确 RPC 拒绝，已最终化原交易
重发只返回原回执，不重复扣费。四节点完整块相同、各自 QC 验证通过；重启后
同节点的完整回执、持久证据及空交易池保持一致，不要求合法 QC 子集跨节点
字节相同。没有用手工投票或直接写池替代真实入口/传播/最终性。

只在测试模块注册的 worker 从每个已停节点读取真实创世、全部块/QC 和
AOEM 权威 Store，按实际块顺序/时间逐笔串行重放；逐块比较 pre/post state、
累计回执和块回执根，最终 typed Store 及序列化字节完全一致。仅借用已按
raw/plan/wire/index/count 核对的 ingress 观测，不复制业务结果。它复用同一
业务实现，属于串行调度参考，不是独立协议解释器；另以固定算术检查费用、
余额、独立派生的 nonce key 及全账户资产守恒，防止共同业务错误被一致性掩盖。
含 bootstrap 共 7 笔，费用 316（普通六笔各 45，失败大额一笔 46），按逐笔
舍入后的 reserve/fee/risk 为 218/63/35；A/C 的 next nonce 为 6/1。
每节点重启前后各一次 worker，全检查成功且实际运行恰好 1 项才生成新报告。

Windows/MSVC 严格 Clippy、Release 构建、15 项配置、4 项交易池和 6 项 RPC
回归通过。混合 gate 首轮 39.68 秒，补强停机后无高度 2 finality 的检查后
复跑 39.66 秒通过，均为测试总耗时，不是出块周期或交易延迟。
最终证据：`artifacts/audit/candidate-node-processes/seal-relay-27612-1790849461758219300/mixed-transfer-acceptance.json`；
同目录 `acceptance.json` 定位四个节点，各自 `mixed-oracle-before-restart` 和
`mixed-oracle-after-restart` 输出保存完整核对结果。
节点 SHA256：`6c050980d183b028758a564916b003c57e144dd56896c04b9fd1bd1658735463`；
libtest worker SHA256：`22298b49c7d9032184f6b50ab066800fdf9abfe7024e70e92c6d97a3015e1a5e`。

复跑先构建 `cargo test --locked --release -p novovm-node --lib --test native_candidate_node_cli --no-run`，
将 `NOVOVM_TRANSFER_PARITY_WORKER` 指向本次明确输出的 libtest 可执行文件，
再以集成测试可执行文件运行 `fresh_record_transfers_conflict_failure_serial_parity --ignored --nocapture --test-threads=1`。
需独占测试 loopback `127.0.0.2:443`；端口占用时失败，不停止其他服务。
缺 worker、错误筛选导致 0 项测试或缺输出均失败，不静默跳过。

未设置新开关的原 proposer 配置也复跑 96 笔场景通过：四节点完整块、QC、
重启读回和 96/96 成功回执成立。窗口 43.925 秒、2.186 TPS、观察 P95/P99
18.392/19.084 秒，业务块 `2,30,1,31,2,30`；不宣称本轮提速。
报告为同目录 `seal-relay-14632-1790849501804465000/transfer-finality-performance.json`。
该次 relay 输出出现一条 `received fatal alert: BadRecordMac`；最终全部交易及
恢复仍通过，但告警根因未定位，不能声称网络运行全程无错误。本轮未改 TLS。

本轮是混合正确性闭环，不是并行重叠测量或新的 TPS 成绩。持续积压、长账本、
实体 LAN、公网、Linux 主节点和长跑仍未签收。没有修改 AOEM、经济规则、
正式创世或签名协议，没有部署或替换现有服务；高性能交付目标仍进行中。

## 设备 A：主节点保活账本连接与真实路径复测（2026-10-01）

基线 `651ec4b`。fresh 主节点在完成原启动校验/恢复后，保留现有 Host
ledger 的物理连接，退出时释放；后续逻辑读写视图共用原连接和写锁。
不保留只读快照、签名权限或已验证结果，不改变 AOEM 权威状态所有权。
缺库、非数据库、缺 schema 或普通账本均拒绝，不创建/修补数据库。
同一库的路径别名使用物理路径作进程内 registry key，首次创建与重开一致；
RocksDB 仍收到原路径。原 fresh 隔离、只读限制及实时 QC/历史校验保留。

Windows/MSVC 严格 Clippy、Release 构建、账本相关 38 项测试全部通过，
其中新增 5 项覆盖冷启动保活、已有只读视图看到真实候选登记、共享写锁、
损坏实时拒绝、路径别名、缺库不初始化、最后 owner 释放后可独立重开。
旧创世路径的真实进程离线 leader 换轮/接替测试也通过（117.49 秒），证据为
`artifacts/audit/candidate-node-processes/seal-relay-22716-1790848063350557300/candidate-less-failover-acceptance.json`。
Linux/Unix symlink 分支本轮未执行；不是全库复跑。

两轮相同设置的四进程 Release 测量均 96/96 成功最终确认，四节点完整
块/QC 相同、状态版本增长 96、重启读回通过。保持 Ed25519、显式 64
传播预算、0ms 收集窗、32 笔选单、4 客户端单入口；诊断关闭，无并行构建。
首轮窗口 39.826 秒、2.410 TPS、观察 P95/P99 18.339/18.979 秒，业务块
`3,29,1,31,2,30`；复测 40.160 秒、2.390 TPS、P95/P99 15.047/15.561 秒，
业务块 `3,29,3,29,6,26`。总测试耗时 73.29/72.62 秒。本次短样本比前轮
2.124/2.108 TPS 较好，但块切分和观察延迟不同，不承诺稳定提速比例，
更不能签收为饱和吞吐、250ms 出块或高性能主网。

节点 SHA256：`48a895672099dee131c58553703ffcb7b0577c6362002b8948e0d0a40f9d1d79`。
原始报告位于 `artifacts/audit/candidate-node-processes/`：
`seal-relay-12060-1790847891653522400/transfer-finality-performance.json`、
`seal-relay-4648-1790847979098627400/transfer-finality-performance.json`。

默认关闭的固定标签诊断扩展到候选计算/保存/登记、发布和物理开库。
修改前诊断报告 `seal-relay-16592-1790847083647547900/transfer-finality-performance.json`
通过 96 笔/QC/恢复；四进程中大于等于 10ms 的 `ledger.open_write` 共
104 次、累计 2.511 秒，`ledger.load_verified` 共 1235 次、18.345 秒，
`successor.prepare.execute` 共 24 次、9.566 秒。这些分段嵌套、含四进程
累计且省略短调用，不能相加或当作独立 CPU 时间，诊断结果不作性能基准。
另行尝试过把空闲预算改为 tick 周期扣除工作时间，实测仅 2.079 TPS，
未证明改善，已撤销该实验，不随本次提交改变主循环调度。

完整历史校验仍随历史增长；真实四进程混合冲突/业务失败负载与串行参考、
持续积压及长账本容量尚未签收。实体 LAN、公网、Linux 主节点和长跑未执行。
未修改 AOEM、正式创世、经济规则、签名协议或运行中服务，目标仍进行中。

## 设备 A：同次账本校验复用创世编译结果（2026-10-01）

基线 `37b4c36`。真实主节点慢调用诊断定位到完整账本检查内部反复重建
同一创世账户状态和根。`load_verified` 现在只解析/编译一次不可变配置，
在本次候选、晋升及历史 finality 验证中传递私有、不可错配的配置/编译
上下文；冷入口仍自行编译。每次独立读取重新验证，无跨 tick 缓存。
原 DB 读取、父子关系、QC 验签、authority、pin、索引、head 和未知键
检查保留；没有移除写前/写后的持久化验证。完整历史扫描仍存在。

Windows/MSVC 严格 Clippy、Release 构建、10 项创世单元、5 项 manifest
回归和 record/legacy 各一项完整 AOEM 四块生命周期通过，后两项分别
174.51/141.34 秒。四块历史的连续两次读取、删除历史归档后的拒绝和恢复
后读取，均实际断言每次完整验证 1 次、创世编译 1 次。新增纯测试还覆盖
两个 root profile 的旧结果一致、错误 pin/绑定拒绝、无效配置不能进入
回调及回调错误传播。不是全库复跑，没有改变协议字节或正式链参数。

Release 节点 SHA256：
`17c79bdc697f79628478862dcb0ff48b984f73c014ecf249922ebedbe5c56625`。
性能复测保持现有 Ed25519 交易、显式 64 传播预算、0ms 收集窗、32 笔
选单和 4 客户端单入口；同机四进程，关闭慢调用诊断，不并行构建/重型测试。
两轮均 96/96 成功最终确认、四节点完整块/QC 相同、状态版本增长 96、
重启读回通过。首轮 45.207 秒、2.124 TPS、观察 P95/P99
18.579/19.058 秒，业务块 `2,30,4,28,9,23`；复测 45.543 秒、
2.108 TPS、观察 P95/P99 19.776/20.427 秒，业务块 `2,30,5,27,3,29`。
总测试耗时 78.52/79.05 秒。与同设置前轮 44.342 秒/2.165 TPS 相近，
且本轮数值略低，不宣称吞吐改善；重复编译减少不等于主链高性能签收。
原始报告：`artifacts/audit/candidate-node-processes/` 下
`seal-relay-1588-1790845845462501500/transfer-finality-performance.json`、
`seal-relay-22436-1790845942610807900/transfer-finality-performance.json`。

修改前诊断报告为同目录
`seal-relay-9580-1790844628060215800/transfer-finality-performance.json`：
96 笔和恢复通过，诊断开启时结果不作为优化基准。四进程日志的
`ledger.load_verified` 大于等于 10ms 慢调用共 1653 次、累计 29.051 秒；
这是四进程累计且包含在上层耗时内，不能与主循环分段相加。
完整历史/QC 反复验证和同步候选处理仍是待定位项；没有据此新增权限
缓存或跳过写后读回。实体 LAN、公网、Linux 节点及长跑本轮均未执行。

## 设备 A：标准 ML-DSA 签名与验签配对（2026-10-01）

基线 `a36912e`。落实用户“先采用标准正确实现，再优化性能”的选择，
新增 Host `MldsaSigner`，使用显式 44/65/87 参数和已有随包 AOEM raw/internal
ABI。复用 verifier 官方正例与篡改负例初始化门禁及同一 external-pure
framing；参数、能力、expanded 私钥尺寸和输出签名尺寸失败关闭。每次
签名返回前对调用方提供的可信公钥自验，不把调用方传入公钥本身当成
链上授权，不猜参数、不自动降级。私钥仅借用，保护与擦除仍由调用方负责。
没有改 AOEM、依赖、正式密钥、经济规则、交易 wire 或 BFT 准入。

本机 Windows/MSVC 和 Linux/WSL 各严格 Clippy、15 项单元（含新增 4 项）
及显式 3 项 runtime（含新增 1 项）通过。真实随包库覆盖全部三参数、
两套临时密钥、空消息、空/255 字节 context、256 拒绝、确定性、一次
framing 的 raw ABI 互操作、raw/二次 framing 拒绝、错配密钥和篡改。
模拟负例证明坏输入未调用密码签名、缺能力/错误尺寸/失败或损坏输出
不返回签名且不重试。全仓 fmt 与 diff 检查通过。

实际 core SHA256 重新核对不变：Windows
`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`，Linux
`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
本轮只重跑上述组件门禁，没有重测完整 90 向量矩阵、独立实现矩阵或密码
性能；先前通过记录保留。不是 FIPS 认证、实体 Linux 或主链 PQ 签收。

接入审核确认旧 native V3/nonce 仍固定 Ed25519，封印需覆盖 prepare、
decision、timeout 和 new-view，不能只换 QC；旧 round wire 的 64 项
序列和 4096 字节字段限制也须按新 profile 做字段级有界编码，不能全局
放开。已再次明确询问 ML-DSA-87 强制单签或 Ed25519 + ML-DSA-87 强制
双签；用户批准标准/性能取舍不自动等于批准该协议选择，故本轮未改变
链上接受规则。共享接口和 B 独立复验命令见 [B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

## 设备 A：有界提案收集窗口及实际对照（2026-10-01）

基线 `08d2744`。新增本机可选 `proposal_collect_ms`，默认 0 保持立即提案；
显式值不超过 1000ms 且不超过 round timeout 的四分之一。遇到第一笔实际
可选交易后固定截止，达到选单上限提前提案，到期不足也提；持续新交易不
延长截止。父 workspace/hash、authority、height 或 round 改变重新开窗，
失去 proposer 资格、无可选交易、阶段切换、时钟等待和 halt 时清除。

窗口只存计时与诊断计数，不缓存交易、父块验证或签票权限。每轮仍重新读取
并验证当前 finalized parent，重新从当前池选单/鉴权；原准备、QC、防双签、
费用与发布恢复规则不变。等待期间继续处理传播和控制消息，不创建候选。
这是本机调度策略，不是共识规则，也不是 250ms 出块承诺；同步主循环工作
仍可能使截止时间之后才能再次轮询。默认生产配置与协议均未改变。

严格 Clippy 和 Release 构建通过。窗口 8 项、提案配置 5 项、service 配置
14 项全部通过；record/legacy 完整四块生命周期各 1 项通过，分别
175.42/148.22 秒，包含等待不产生 workspace、传播继续、到期原候选内容
不变及跨阶段剩余 staging 保留的断言。不是全库复跑。
Release 节点 SHA256：
`9abdea0ba08213a44b583c8cc9f2001f9d43ed734d3505431ffec94bc87f617a`。

同一二进制顺序执行三轮原 96 笔、32 笔选单、4 客户端单入口四进程测量，
均为 96/96 成功回执、四节点完整块/QC 一致、AOEM 状态版本增长 96、
重启读回通过。诊断关闭，没有同时构建或运行其他重型测试。

| 传播 profile / 收集窗 | 观察窗口 | 最终确认 TPS | 观察 P95 / P99 | 业务块数 |
| --- | --- | --- | --- | --- |
| 显式 64 预算 / 0ms 对照 | 44.342 秒 | 2.165 | 19.101 / 19.115 秒 | 6 |
| 显式 64 预算 / 250ms | 47.158 秒 | 2.036 | 18.980 / 19.683 秒 | 6 |
| 原低传播限额 / 250ms | 66.894 秒 | 1.435 | 24.588 / 25.482 秒 | 8 |

业务块分别为 `3,29,4,28,2,30`、`10,22,11,21,16,16` 和
`6,13,8,5,4,24,4,32`，均不包含最初 bootstrap 块。两组 250ms 运行状态
证明确有等待：高预算 6 次到期提案；低预算 7 次到期、1 次满批提案。
高预算并未减少块数或提高 TPS；低预算比前轮两次 12 块/1.077–1.155 TPS
有所改善，但不是本轮同二进制低预算零等待复测，不宣称稳定收益。故窗口
保留为显式可选策略，不默认开启，不把通过功能测试写成普遍性能优化。

四节点合计 gossip 尝试分别 8973/7536/1764 次，staging 拒绝 0/0/39 次，
来源限额拒绝 11/4/0 次，实际发送回压均为 0；不借此新增回压实测结论。
短样本含 HTTP 和观察轮询，既不是饱和吞吐，也不代表密码/内核性能。
重复传播、RPC/主循环时序和完整历史验证仍需沿真实路径定位，不能继续
仅调窗口长度或提高限额便宣称高性能交付。

原始报告位于 `artifacts/audit/candidate-node-processes/` 下：
`seal-relay-22684-1790843506986934000/transfer-finality-performance.json`、
`seal-relay-2696-1790843585582490900/transfer-finality-performance.json`、
`seal-relay-24808-1790843667056524900/transfer-finality-performance.json`。
总测试耗时 78.75/81.13/102.85 秒。Linux 节点、实体多机、公网和长跑均
未在本轮执行；高性能/生产签收仍未完成。

## 设备 A：独立、有界且处理回压的交易传播通路（2026-10-01）

基线 `8a9398d`。fresh 交易不再占用 body/round 的每 peer 4 格队列，
也不再在候选切换清理旧控制消息时被一并清空。新增独立调度器，暂存总量
最多 1024 条/16 MiB、单条仍最多 64 KiB；每来源限速之外，入站处理和
出站尝试各有每 poll 数量/字节预算。超过本轮预算的工作保留，超过来源
秒窗口限额仍按原规则拒绝；不是无限队列，也不是减少签名验证。

发送 `false` 不再推进该 peer 的待发交易，重试绑定确切 hash，池重排不
改变重试对象；一个 peer 回压不阻塞其余 peer。正常队列接收不代表远端
持久入池，原池记录不会因发送成功而删除。每秒内已提交 hash 集合有界，
字节不足延期的 peer 在下一轮优先，防止持续小消息让大消息长期饥饿。
gossip 放在本轮控制处理之后，但共享 outbox **不是严格 QoS**；已排队的
交易仍可能影响控制消息。预算约束工作量，不保证同步落盘/验签的墙钟延迟。

配置新增可选 `transaction_transport` 完整对象，所有字段必填且拒绝未知
成员：`per_peer_queue` 1–256、`ingress_per_source_per_second` 1–4096、
`ingress_per_poll` 1–256、`gossip_per_peer_per_second` 1–4096、
`gossip_per_poll` 1–1024、`bytes_per_poll` 64 KiB–16 MiB。不配置时保持
每 peer 队列 4、gossip 4/秒，来源限额及入站单轮数量取原 ingress 字段，
gossip 全局每轮最多 64 次、入站和出站各最多 1 MiB。原 seal/control
限额不变，默认配置和生产参数未提高。`nov_chainStatus.transaction_transport`
报告实际预算、排队、尝试、拒绝和回压计数，不包含交易原文。

gossip 改为借用有序池，仅复制实际选中发送的 raw；提案的原全池复制和
`reconcile_rooted` 全池读取仍存在。链域、签名、摘要、费用、nonce、AOEM、
候选共识字节及恢复状态机不变。待入池 staging 跨候选切换保留，但不是
跨重启持久化；持久保证仍从原交易池的同步写入开始。

本机严格 Clippy、Release 构建通过；调度 15 项、配置 14 项、持久池 4 项
全部通过，record/legacy 完整四块生命周期各 1 项通过（172.24/146.34 秒），
包含真实提案切换后剩余待入池消息仍在的断言。并非全库 933 项复跑。
Release 节点 SHA256：
`f47a61469adc074837d9aabd7e9dd3602949b54444e1a684a7219687a0e8decf`。

同一 96 笔单入口四进程测量已通过两种 profile：保留原低传播限额；显式
队列/来源限额/单轮入站/gossip 来源限额各 64、gossip 单轮 192 次、双向
各 1 MiB。运行时须逐字段证明配置已生效；仍是 32 笔选单、4 HTTP 客户端、
250ms tick、300 秒预算、10 秒 HTTP 超时、无隐式重试。包括原限额复测在内，
三轮均为 96/96 四节点成功回执、完整块/QC 一致、状态版本增长 96、
最终重启读回通过；
诊断关闭，性能运行期间没有构建或其他重型测试。

| Profile | 观察窗口 | 最终确认 TPS | 观察 P95 / P99 | 业务块数 |
| --- | --- | --- | --- | --- |
| 原限额 | 89.149 秒 | 1.077 | 31.151 / 38.556 秒 | 12 |
| 显式 64 预算 | 43.129 秒 | 2.226 | 14.905 / 15.281 秒 | 6 |
| 原限额复测 | 83.136 秒 | 1.155 | 26.456 / 37.275 秒 | 12 |

两次原限额都比上一提交的 1.660 TPS / P95 21.112 秒更慢，不能隐去该退化，
或宣称默认性能已提升。前两轮业务块分别为
`3,13,12,4,3,13,13,3,4,14,10,4` 和 `3,29,6,26,5,27`，原限额复测为
`3,13,12,4,4,10,14,4,2,14,14,2`（不含创世后的 1 笔 bootstrap）。
调度时序和块切分不同，不能把短样本全部差异归因于
某项优化；这些仍含 HTTP/排队/观察时间，不是饱和吞吐或精确提交延迟。

四节点合计：原限额 staging 拒绝 138 次、gossip 尝试 2400 次，复测为
101/2316 次；显式 64 预算 staging 拒绝 0、来源窗口拒绝 20 次、
gossip 尝试 8205 次。这些是
消息计数，不是独立用户交易失败数；所有 96 笔最终确认。重复传播仍很多，
显式提高预算不是传输成本已经优化。三轮实际 `backpressure=0`，故本次
进程测试没有触发发送队列满；回压重试结论来自上述确定性反例测试。

原始报告位于 `artifacts/audit/candidate-node-processes/` 下：
`seal-relay-11088-1790841772019114500/transfer-finality-performance.json`、
`seal-relay-5880-1790841899448837700/transfer-finality-performance.json`、
`seal-relay-18164-1790842021996004500/transfer-finality-performance.json`。
总测试耗时分别 127.37/77.43/121.17 秒。尚未执行 Linux、实体多机、公网
或长跑；**仍不满足高性能/主网生产签收，目标继续**。

静态复核确认旧 gossip 即使空池也推进一秒门限，新调度允许更早传播少量
前缀、同秒补足余量，而且移至本 tick 控制处理之后。提案仍在看到少量
可选交易时立即固定候选。该时序变化与显式 64 场景每批分成小前缀和
剩余交易的结果吻合，但不是全部退化的独立因果证明。下一切片应建立
明确、有界、按父块/轮次隔离的提案收集窗：达到选单目标立即提案，否则
首次可选交易后固定截止，到期不足也提；新流量不得不断重置截止。先用
同路径测量验证能否减少过小候选，而不恢复旧空池计时的偶然等待，不撤掉
回压和公平性保护。重复传播与同步执行/历史验证开销仍需处理。

## 设备 A：后继回读复用同次已验证父归档（2026-10-01）

基线 `74b2367`。仅无回调 `Scope::Verify` 的第二次子候选 artifact 回读，
将本次 bundle 已完整验证的直接父归档显式传给 NCW2 输入验证，避免再读一遍
历史账本。第一次 artifact 读取、父块旧格式回退和可能涉及祖父的冷读取
保持原路径；没有在 workspace、TLS 或跨 tick 缓存归档或签票权限。

候选输入/输出仍实际读取两次并比较；来源摘要、配置、QC、执行绑定、发布
承诺、三树根/统计、父子 height/hash/state/slot/time 及实时 head/h 均保留。
传入错误归档直接拒绝，不能静默重新加载或降级冷路径。签票回调、晋升写入、
恢复状态机、交易格式、AOEM、费用及生产参数不变。

新增检查复用原 record 第三块的 Prepared/Published/Finalized 夹具，而非
模拟证明：原 artifact 读取完整验证历史 1 次，新同次回读 0 次且 artifact
逐字段相同；Published/Finalized 的实际完整 Verify 为 2 次。结合未改的
首次读取和 bundle 检查，正常 NCW2/V3 路径由原 3 次减为 2 次；旧格式冷读取
可能另有检查，不能说所有 Verify 都固定 2 次，更不是端到端历史无关。
反例包含真正有效 QC 的祖父归档误传为直接父、错误执行绑定/承诺/状态根/
回执根/QC，以及内存源字节和三根变更；拒绝后输入、输出、head/h 和 Ready
证据不变。内存负例与原持久 ledger 故障测试分别保留，不混称磁盘故障覆盖。

本机严格 Clippy、Release 构建通过；record/legacy 完整四块生命周期回归
各 1 项通过、0 失败，分别 173.68/147.36 秒，前者包含上述计数、等价和拒绝
断言。这不是全库复跑。随后原参数四进程 96 笔测量通过：96/96 四节点成功
回执、所有持久块/QC 一致、状态版本增长 96、最终重启读回一致。总耗时
92.41 秒，交易观察窗口 57.830 秒、**1.660 TPS**，P50/P95/P99 为
17.981/21.112/21.310 秒；诊断关闭、无其它构建或重型测试同时运行。
这仍包含 HTTP/排队/逐笔观察，不是精确共识提交时间或饱和吞吐。本轮形成
7 个业务块，上轮两次为 8 个，不能把差异全部归因于一次扫描的省除，也
不能从一轮短测量声明稳定提速比例或高性能签收。

证据：`artifacts/audit/candidate-node-processes/`
`seal-relay-22756-1790839914177182800/transfer-finality-performance.json`。
实际 Release 节点 SHA256 为
`180de310ff3c2a648ce47e2a7195cc74713cacac379f9fdb477ee87b867731a9`。
测试继续保持单入口、4 客户端、250ms tick、原传播限额、300 秒预算和
10 秒 HTTP 超时，没有隐式重试；没有执行 Linux、实体多机、公网或长期压力
测试。少一次历史扫描不等于整个主链历史无关，**目标仍未完成**。

下一阶段转向真实交易传播与入池预算。源码核对表明交易 gossip 每轮只取
最多 4 笔并至少间隔 1 秒，入站每 source 8/秒指运输节点而非钱包；交易与
部分控制消息还共用每 peer 4 格 staging。原测量报告中的 `ingress_per_poll=16`
是 body/round 等后续轮询预算，不是交易处理的全局 CPU 预算。提案已有
最多 1024 笔/2 MiB 块体能力，但有少量可用交易即可能准备不可变候选。
因此既不能把 gossip 的 4 等同于链 TPS，也不能仅提高常量宣称高吞吐。
下一刀先保留现有 wire，完善有界交易队列、peer/全局工作预算和发送回压，
保留共识消息服务机会，再测提案可用数/块填充与真实最终确认；本节尚未
实现该调度变更，不将静态限制分析当成全部延迟的因果证明。

## 设备 A：保持共识节奏，空闲期间继续响应 RPC（2026-10-01）

基线 `bf5ee47`。Release 开启诊断的对照也通过 96/96、全部持久 QC 和重启，
窗口 122.851 秒、0.781 TPS、观察 P95 42.811 秒；日志开启不作正式速度成绩。
四节点主生命周期慢调用累计约 66–68 秒，publication 占约 22–25 秒，
ledger 验证嵌套累计约 31–32 秒，不能相加。单次 lifecycle 最大约 2.01–2.29
秒，RPC 则仅有少数 >=10ms 调用。日志保留在
`artifacts/audit/candidate-node-processes/fresh-validator-0..3-22272-*`，报告在
`seal-relay-22272-1790837766669109800/transfer-finality-performance.json`。

据此修正实际 RPC 响应路径：原主循环末尾整段休眠，现有 RPC 时在同一固定
空闲预算内继续调用原非阻塞 poll，每次请求休眠不超过 5ms。预算不随流量
重置；耗尽后不再启动 poll，避免不断到来的查询一直推迟下一轮生命周期。
无 RPC 仍按原方式休眠，max_ticks 终止位置不变。没有新线程、缓存或网络
协议；8 个并发连接、原 HTTP deadline、鉴权、费用和签票规则不变。
5ms 不是系统调度或响应上界；单次 poll 最多处理 8 个同步 handler，仍可能
超过剩余预算，生命周期本身也仍会阻塞 RPC。增加的是空闲期间处理机会，
不是把 250ms 改为出块承诺，也不是执行内核 TPS 优化。

本机严格 Clippy、Debug/Release 构建及 8 项定向测试通过：新增 4 项用确定性
时钟检查零预算、总预算不重置、慢 poll 后停止、错误传播与最大 Duration，
另有原 HTTP/部分请求和计时测试；不是全库复跑。关闭诊断后的原 Release
96 笔场景连续两次通过四节点成功回执、完整区块/QC 一致和重启读回。
首轮总耗时 114.91 秒，交易观察窗口 79.366 秒、**1.210 TPS**，
P50/P95/P99 为 22.331/29.927/35.826 秒。三批各 32 笔的首提交到最后入池
响应分别约 0.102/0.243/0.186 秒，旧基线约 2.648/3.284/4.202 秒。
参数、单入口、4 客户端、gossip 限制和所有最终性断言保持原样；块切分与
调度会变化，不能把短场景比较当成稳定提速比例或准确共识延迟。
同一程序第二轮总耗时 116.54 秒，窗口 82.134 秒、**1.169 TPS**，
P50/P95/P99 为 21.562/29.268/38.914 秒；两轮均没有放宽原 300 秒窗口
预算、10 秒 HTTP 超时、单入口或最终性要求，没有隐式重试。

首轮证据：`artifacts/audit/candidate-node-processes/`
`seal-relay-408-1790838452904303800/transfer-finality-performance.json`；
第二轮同目录下
`seal-relay-9856-1790838607544340900/transfer-finality-performance.json`。
实际 Release 节点 SHA256
`4d4a5fb030d67a490eec06d884998edab09112104662108e3ce85ba8f3debc27`。
这仍是短场景的 RPC 到四节点回执观察结果，包含客户端等待，不是饱和吞吐
或 AOEM 内核吞吐。**高性能目标未完成**。下一处已定位的重复工作是同次
后继 publication 的第二次 artifact 回读又核验父归档全历史；后续可复用
本次 bundle 的已验证父归档，同时保留两次 artifact 读取比较及全部 source、
head/h、根和恢复边界，不能跨 tick 缓存签票权限。本节尚未实现此后续优化。

## 设备 A：用慢调用证据定位主循环停顿（2026-10-01）

基线 `3d586fb`。新增默认关闭的 `NOVOVM_NATIVE_FRESH_TIMING=1` 诊断，
只记录固定阶段名、PID、单调时间和 >=10ms 调用耗时，不记录交易/账户内容。
关闭时不读取计时器；不更改返回值、错误、锁、共识或执行顺序。测试仅向
原测量阶段的四个节点显式转发该开关，其他环境隔离保留。严格 Clippy、
构建和 4 项计时/RPC 测试通过；不是全库复跑。

原 Debug 96 笔场景开启诊断后仍 FAIL：96 入池、76 笔取得四节点成功回执，
随后触发原预算检查，总计 376.77 秒含启动和最后一轮查询扫描。本次不是
HTTP 10 秒超时。四节点末尾日志虽均为 height=9、state_version=97，仍不能
补算剩余回执；完整归档/最终重启未执行，不产生性能成功报告。
证据位于 `artifacts/audit/candidate-node-processes/` 下
`seal-relay-15824-1790836905798318200/transfer-finality-observations.json`
及 `fresh-validator-0..3-15824-*` 的 measurement stdout。

实际慢调用：单次主生命周期最大 8.579/8.608/8.984/8.596 秒。前三个接收方
最大 tick 中，候选准备占约 6.56–6.58 秒；对应 proposer 的提案占 7.813 秒。
四节点常态 publication 核验的已记录慢调用累计约 130–147 秒，而节点 0
RPC poll 慢调用累计仅 0.510 秒、最大 23.209ms；所有节点只有一次
transaction_status 超过 10ms（11.008ms）。这些是 >=10ms 调用的墙钟时间，
嵌套阶段不能相加，不是 CPU 占比，不足以证明上一轮 HTTP 超时的唯一原因。
同线程先处理 RPC 再跑生命周期的代码与本轮证据支持优先检查候选/发布路径，
不支持先增加基本无法命中的 RPC 批处理。

另记录一次 relay `BadRecordMac` 断链告警；源码只读检查未定位此次原因，
不归因于攻击或内存损坏。stderr 为空不等于没有 overlay 断链事件。此项保留
待定向验证，不与性能计时混同。

随后 Release 冷构建完成（2 分 57 秒），关闭诊断、保持原全部参数与断言，
首次完整通过真实路径：96/96 入池并取得四节点成功回执，所有持久块/QC
与交易集合一致，状态版本增长 96，最终重启后归档读回一致。测试总耗时
159.77 秒，交易观察窗口 121.390 秒；实测 **0.791 finalized TPS**，
P50/P95/P99 为 30.606/40.281/42.206 秒。它们包含客户端排队和逐笔 RPC
观察，是确认延迟上界，不是准确共识提交时间，更不是高性能签收。
硬件为 Core Ultra 9 275HX、24 逻辑核、约 64 GiB RAM，Windows 11
10.0.26200，本机四进程 loopback WSS/AOEM RocksDB。原每 peer 每秒 4 笔
gossip、250ms tick、100ms 共识 poll、客户端并发 4 均保留。

成功报告：`artifacts/audit/candidate-node-processes/`
`seal-relay-6172-1790837558407537200/transfer-finality-performance.json`。
实际 Release 节点 SHA256 为
`66d121a1e5aa42a19e635e047c91e8bce6563027f9bcd6679a26ed36488ad1f5`；
报告标记 `slow_call_diagnostics_enabled=false`。这是短基线，不是饱和吞吐、
长期压测、实体多机或生产验收。**目标仍未完成**：接下来在相同 Release
程序中开诊断核对主要耗时，再修真实瓶颈，不能把 Debug/Release 差异当成
算法优化收益，也不能只凭一次 96 笔通过宣称高吞吐。

## 设备 A：同次后继发布核验复用完整账本检查（2026-10-01）

基线 `95b847c`。后继发布的只读 Verify 在原 workspace/authority 锁内，
通过一个 ledger 读取作用域取得目标承诺、直接父归档、发布块与最终性。
原五个顶层 getter 各自进行一次完整历史验证，现合为同次的一次；仍执行
原 `load_verified` 的全部历史/QC、pin、索引和未知 key 检查，未减少验证内容。
ledger mutex 在 getter 返回前释放，之后父来源与候选输入仍能安全读取归档。

结果只用于 `Scope::Verify && capture.is_none()`，不跨 tick 保存，不作为
签名权限；写入、晋升恢复及旧冷路径回调保持原 getter 时序。实时 head、
父/子发布证据、直接父 height/hash、执行绑定和最终输出读回不变。其余
artifact/NCW2 输入验证仍可能调用完整历史检查，不能把“5→1”说成整个
Verify 或整个 tick 只验证一次。AOEM、交易 wire、费用和共识协议未改。

严格 Clippy、构建和格式检查通过；record/legacy 完整生命周期回归各 1 项
通过，0 失败、0 忽略，分别 254.96/284.18 秒。三阶段测试复用现有
record 第三块夹具，直接 bundle 与原五 getter 逐字段比较并统计完整验证
次数；错误 parent/执行目标/链域、损坏或缺失归档/pin、未知 ledger key
须拒绝且不改写证据，均已通过。

原 96 笔四进程 Debug 测量在无其它构建/重型测试并行时再次执行，仍 FAIL：
96 笔全部取得入池响应，64 笔已观察到四节点成功回执；第三批状态查询在
入口节点触发原 10 秒 HTTP 超时（os error 10060），不是本轮触发 300 秒
测量预算检查。整项测试 301.55 秒包含启动/首块与恢复准备，不是交易窗口。
最后四节点日志均为 height=8、state_version=67、stderr 为空；不得据此补算
未观察的回执，或将 96 笔入池当成 96 笔最终确认。完整离线账本/QC 比对与
最终重启尚未执行，没有性能成功 JSON，不填 TPS/P95/P99。

本机原始证据保留在 `artifacts/audit/candidate-node-processes/` 下
`seal-relay-19552-1790836055815978800/transfer-finality-observations.json`
及 `fresh-validator-0..3-19552-*` 日志。旧两轮失败证据也保留，三轮均不能
作为高性能签收。测试退出已释放自有节点，没有停止用户服务。

下一步先测量真实主循环/发布核验/状态查询的分段耗时，定位为何超过客户端
响应预算，再取 Release 同路径基线；不延长超时掩盖问题、不降低验证标准，
不凭静态调用次数继续扩张优化。只读排查还确认当前客户端按同一 tx 对四节点
各发一个查询，通常每节点只有一条；因此本轮不新增基本无法命中的同 tick
RPC 批处理。未执行 Linux、实体多机、公网或长期压测，不是生产性能签收。

## 设备 A：主节点复用 AOEM 存储会话，性能测量仍未通过（2026-10-01）

基线 `d05bfe4`。真实 fresh 主节点的拥有者线程现在显式持有通用 graph provider
会话作用域，首次请求才开库，随后相同物理路径、完整运行/存储配置和有效环境
复用同一 session/database ID。作用域只保留一个 provider，不是新的业务缓存、
执行内核或账本；不同配置/路径及同线程嵌套拒绝。TLS 仅保存 Weak，正常退出
和异常展开在普通线程执行阶段释放所有者，避免 Windows TLS 析构时等待 worker。

所有引用共享原有 poison 状态，提交不确定后不能通过另取句柄或重新 open
绕过失败关闭；未排空异步提交仍保留完整原 owner。无作用域的调用保持原
开关库语义。每次 WorkspaceStore 的路径隔离、协议 pin、workspace/authority
锁、实时 head、QC、根和数据校验不变；没有缓存签票权限。AOEM 源码和随包
库、交易格式、费用、共识和生产参数均未改变。

Windows 严格 Clippy、构建通过；执行适配库默认全库 41 通过、0 失败、8 忽略。
另显式执行 14 项 graph/provider 生命周期测试，0 失败、0 忽略，其中 6 项
运行真实随包 AOEM（其余与默认组有重叠，不重复相加）。新增覆盖相同 graph ID
重试、路径别名、多个 handle 及 scope 内保活、正常退出/展开后真实重开、
配置/环境漂移拒绝、共享 poison 和提交前校验拒绝不误 poison。
另显式运行旧 Execute fresh 主节点四进程兼容回归，1 通过、0 失败（383.34 秒），
覆盖连续出块、重启和离线历史追赶；原始日志保留于
`artifacts/audit/candidate-node-processes/fresh-validator-0..3-6828-*` 与
`seal-relay-6828-1790834580673721100/`。这是正确性回归，不是 TPS 测量。

原四进程 record Transfer 测量在无其它构建/重型测试并行时重跑，仍 FAIL：
64 笔尝试均取得入池响应，61 笔已观察到四节点成功回执，随后触发原 300 秒
预算检查；第三批未提交，完整 96 笔、离线账本核对与最终重启验收未执行。
测试总耗时 360.74 秒含首块启动/恢复与最后一轮 RPC 扫描；预算在轮询轮次
边界检查，不能称为精确 300 秒中止。四节点最后日志均为 height=7、
state_version=65，不能把日志推进代替剩余逐笔回执和归档验证。
本轮四个 stderr 均为空，未重现 `CURRENT` rename 拒绝；这不是已经证明
所有 Windows 存储故障消失，也仍未判明上轮拒绝访问的持有者。

保留证据：`artifacts/audit/candidate-node-processes/` 下
`seal-relay-28308-1790834144487219000/transfer-finality-observations.json` 及
`fresh-validator-0..3-28308-*` 日志。整套运行的 owner LOG 文件数分别为
10/10/9/7，包含准备和进程重启；旧失败节点的 108 份 LOG 作为历史证据保留。
不以 LOG 数替代吞吐；没有生成性能成功报告，不填 TPS/P95/P99。

只读检查发现下一处明确重复开销：successor publication 的 Verify 路径在
同一 authority 锁内，通过多个 getter 重复调用 ledger `load_verified`，
每次都完整核对历史/QC/keys。下一刀优先在同次验证中复用一个已核验的只读
账本读取结果，保留全部 head/h、直接父、输出与 readback 约束，不跨 tick
缓存授权，不减少校验内容。RPC 逐 hash 重建 record reader、每 poll 清理池
也有重复工作，但尚无 profiler 占比证据，不把静态代码分析当精确耗时分解。
本轮未执行 Linux、实体多机、公网或长期压力测试，不是生产性能签收。

## 设备 A：有界批量选单通过，真实最终确认测量暴露存储失败（2026-10-01）

基线 `395bf19`。自动选单保留交易池既有 identity/nonce 排序，但不再每加入一笔
就复制并重新鉴权整个前缀。选单阶段按原始签名交易逐项验证，在同一父状态
读取作用域内，每个触及的签名身份只点读一次起始 nonce；receipt/reservation
去重、链域、能力、请求派生、nonce 连续性与原有 2 MiB 块体限制均保留。
父域、标记或所需记录读取失败直接中止，不能当成坏交易跳过或 nonce=0；
跳过交易不消耗本轮临时 nonce，交易池不改变，最终 prepare 仍整批重新验证。

节点配置新增 `proposal_max_transactions`，默认 16，显式允许 1–1024；这是
本地提案预算，不改变原共识上限、交易格式、创世、费用或 AOEM。0、越界及
错误 JSON 类型均拒绝。池本身仍排序，不把整个流程宣称为完全 O(n)。

本机严格 Clippy、格式检查通过；69 项回归通过，0 失败、0 忽略：67 项
配置/选单/转账/恢复定向测试（220.35 秒），record/legacy 各一项完整四块
生命周期（278.61/314.77 秒）。其中新增 8 项验证旧前缀算法选单一致、别名
同 nonce 域、每身份一次点读、计数/字节上限、读错传播及池重开不变。
独立四进程 record-v2 Transfer 测量已执行，但失败，不能作为性能验收通过。
测量只用全新临时创世、32 个已注资测试发送者、3 批各 32 笔、固定一个 HTTP
入口、真实 WSS gossip/AOEM/BFT；不把签名生成或启动计入计时，也不将同一
交易直接复制提交到全部节点绕过传播。原 250ms tick、8 连接 RPC 和每 peer
每秒 4 笔 gossip 限制保留，P95/P99 明确是四节点 RPC 观察确认的延迟上界，
包含提交/排队/观察开销，不是精确共识提交时间。

首次测量共尝试并获得入池响应 32 笔，尚未观察到四节点最终确认，节点 3 即因
AOEM owner RocksDB 打开失败而退出，随后 RPC 连接被拒绝。原始错误为
`status=-4 / Failed to rename 000442.dbtmp to CURRENT / 拒绝访问`；没有生成
`transfer-finality-performance.json`，不报告 TPS、P95/P99 或 96 笔成功。
本机证据保留于 `artifacts/audit/candidate-node-processes/` 下
`seal-relay-21496-1790832955253837700/transfer-finality-observations.json`，以及
`fresh-validator-3-21496-1790832954105282000/` 的 measurement stderr 和 RocksDB
`LOG`。前者包含所有已发请求结果及部分观察，不通过自动重试掩盖不确定提交。
整项测试耗时 74.71 秒，含启动/首块与恢复，不是交易确认耗时。

只读排查确认 Host 的 `WorkspaceStore::open` 每次创建新的 AOEM graph/session，
失败前数据库多次关闭后重新恢复；记录中的正常关闭均已完成。当前没有证据
判定哪个句柄导致 Windows rename 拒绝，也不能归因于杀毒软件或断言数据库
已损坏。固定 AOEM 源的 provider 仅在单个 context 内管理句柄，无跨 session
物理 DB 缓存；单独添加第二个长驻 graph 会造成 LOCK 冲突，不是可用修法。
后续应在既有 Host 会话作用域内核对 provider 复用与共享 poison/恢复语义，
再单独重跑测量，不降低最终性或失败回执检查，不修改 AOEM 业务边界。

复现入口：`cargo test -p novovm-node --test native_candidate_node_cli
fresh_record_transfers_measure_rpc_to_finality -- --ignored --nocapture --test-threads=1`。
需要随包 AOEM、独占测试 loopback `127.0.0.2:443` 与现有原生构建环境；Windows
测试进程设置 `RUST_MIN_STACK=33554432`，不得停止用户服务释放端口。它是显式
诊断/测量门，不是已通过的默认 CI 或实体多机验收。

## 设备 A：根视图进入实时签票与首次晋升准备（2026-10-01）

基线 `3157bef`。record 父块的候选登记、后继签票、父块换轮签票及首次晋升
准备已复用根视图，不再为这些动作加载完整父 Store。workspace → authority
→ ledger 锁顺序不变，authority 从实时验证连续持有到回调结束；历史缓存仍
不能授权签名。当前最终块、无 pending、QC、head/h、当前与直接前块的
输入/输出和三树验证保留，损坏数据不降级；明确旧格式才走原冷锁内回调。

首次 prepare 使用严格 scope；已有 verifier 确认 parent/candidate/完整 proof
完全相同且持久 intent 有效后，才走原 cold 重试检查。它不是 pending 签票许可，
也没有扩大直接重复 prepare 原先允许的高度范围；标准 resume、发布、账本
完成及 finalize 状态机未改。交易编码、根、费用、AOEM 和生产参数也未改。

本机 Windows FULLMAX 最终通过 51 项回归，0 失败、0 忽略：49 项转账、费用、
nonce、增量恢复、旧格式、交易池及无候选换轮定向测试，加 record/legacy
各一项完整四块生命周期。定向组耗时 208.79 秒，record/legacy 最终完整回归
分别 265.08/294.81 秒；均含故障和网络恢复，不是出块间隔或 TPS。严格
Clippy `-D warnings`、格式和补丁检查通过，未宣称全库或远端 CI 已通过。
新增护栏覆盖真实登记、proposal/vote/decision、父轮次 timeout 签名、服务
启动/轮询、首次 prepare、发布/账本完成、最终化恢复及自动提案；签名 subject
与完整父参考相同。实际 OS try_lock 验证回调期间 authority 被持有，正常
返回和错误返回后均释放。
未登记、陈旧父、pending、坏 head/h/输出/三根/直接前块均不得进入签名回调。

首轮 record 回归发现新夹具错误要求：删除子块输出 reservation 后，父轮次
仍应成功。实际共享 catalog 要求 completion 必有 reservation，因此应共同
拒绝；已按原安全语义修正测试，生产校验未放宽。仅子块 completion/chunk
缺失仍要求父轮次可用；每个故障逐字节恢复，拒绝不能改写源证据。

第二轮整段护栏进一步发现发布轮询仍通过 parent_artifact 间接读取旧 NCW1
Execute 父块全状态。现已在原 authority 锁内，用已验证的历史 finalized archive
及原始输出/输入/QC/h/三树验证父来源，核对父 workspace、链、高度及 hash。
只有明确旧输出格式冷兼容；不能用 strict no-pending tip getter 替代历史父验证。
原 head=父或精确目标的条件、原子提交/readback、不确定结果锁保留与恢复均保留。
发布中断测试要求真的到达 AfterLedgerCommit，不能把物化护栏报错当故障注入通过。

第三轮完整自动提案护栏又发现 service config 为核对直接祖先而加载旧块全状态。
现改为复用 live capture 在同一锁内已验证的祖先 workspace/hash；历史根视图
未验证 lineage 或祖先不符直接拒绝，只有明确 Cold 视图保留原兼容分支。
显式准备、相同请求重试及真实自动提案均加物化护栏；不存在的祖先和真实但
高度错误的 workspace 都必须在创建候选前拒绝，不改变交易/根/签名协议。
新反例最初只接受 lineage 报错，但自指祖先会被更早的配置校验拒绝；已分别
断言两道拒绝原因，并保留零新增候选检查，没有放宽生产校验。

下一步直接处理选单逐前缀重复鉴权和 16 笔限制，并复用现有独立节点 HTTP
提交/最终确认测试，增加 record-v2 Transfer 连续批次的 TPS/P95/P99 测量。
账本完整验证仍随历史增长，须以该路径实测；旧独立 nonce 证明格式不是
record Transfer/BFT 性能测量前置。首块、旧格式、Execute/混合批、启动及
部分重试仍有冷路径，不据此宣称端到端历史无关或主网高吞吐。
本轮不代表实体多机、公网、Linux/nightly 或生产验收。

## 设备 A：最终块点读接入交易池与回执查询（2026-10-01）

基线 `077ba6d`。fresh 生命周期缓存已改为经过验证的最终块视图；record 创世
下，真实签名提交的已确认/nonce 检查、回执查询及交易池清理只读取所需记录，
不再物化完整父 Store。nonce 和回执分别交叉核对状态根与累计回执根，缺失
对象标记或损坏 blob 报错，不冒充 nonce=0 或未知交易。旧格式保留冷兼容。

整批交易池检查复用一次有界读取器，所有查询成功后才一次同步写入删除；
后段读取失败也不能删除前面的交易。已确认重复提交、业务失败回执、已消耗
nonce 拒绝、queued/unknown 返回语义不变。发布中断仍先恢复再缓存最终块，
没有把严格最终块读取提前到 pending 启动发现，也没有赋予缓存新签名权限。

本机 Windows FULLMAX 本轮重新通过 46 项 Node 回归，0 失败、0 忽略：44 项
转账/费用/nonce/增量恢复/旧格式/交易池定向测试，以及 record、legacy 各一项
完整四块最终性、发布重试、服务重启、后继提案与退休测试。新增真实夹具在
禁止 Store 物化的护栏内比较成功/失败回执、新签名者 nonce 与完整参考；
删除所需标记或叶 blob 后，查询失败且池中 7 项在重启后全部保留，恢复后
仅清理已确认/nonce 已消费的 5 项、保留待确认的 2 项，重启及重复清理一致。
独立有效哈希的物理/共识投影冲突被拒绝，查询不改变父 authority 或输出。
真实生命周期还覆盖已确认重复提交、已消费 nonce、未知交易与待确认去重。

严格 Clippy `-D warnings`、格式和补丁检查通过。定向组耗时 126.88 秒，
record/legacy 完整测试分别 306.41/293.16 秒；它们包含故障恢复，不是
出块间隔、吞吐基准或多机成绩。没有修改根/交易编码、经济规则、AOEM 或生产参数。

剩余：启动发现、登记/签名/晋升仍有全量边界，账本完整验证仍是 O(history)。
下一切片优先去掉签名 live scope 的全量父加载，但须保持 authority 锁贯穿
实时验证与签名回调。自动 16 笔限制、增量 nonce 证明及持续最终确认
TPS/P95/P99 尚未完成；实体多机、公网、Linux/nightly 和生产验收不在本轮证据内。

## 设备 A：当前最终块根视图接入后继候选与提案（2026-10-01）

基线 `3676141`。record 创世的当前父加载、选单鉴权、纯 Transfer 后继创建及
退休时的当前父检查已使用三树点读；没有构造只填部分字段的假 Store。
账本当前 tip 在一次读取锁内完整验证，拒绝未完成晋升，并逐项核对 QC、
当前块及保留直接前块的输入/输出摘要、唯一 live head、h 发布证据和三树。
不要求读取已退休更早 workspace。历史缓存只用于纯计划，
不能授权新候选或签名/发布；创建前及退休后均重验 live authority。

NCW2 会重算完整描述符，包含 parent_snapshot；NCW1 的该冗余字段原本依赖
完整 typed Store，此处不伪称重算，实际输入字节仍由最终化输出摘要绑定。
旧 profile、旧输出、旧 reservation 与 Execute/混合批次保留明确冷兼容，
损坏状态不能靠降级放行。没有修改根编码、经济规则、AOEM 或生产参数。
这一路径验证来源、根节点和所需记录，不是全部历史叶/blob 的完整性巡检；
未经读取的深层数据损坏仍需实际访问或单独完整检查才能发现。

本机 Windows FULLMAX 最终通过 46 项 Node 回归：44 项转账/费用/nonce/增量
恢复/旧格式/交易池定向测试，加 record 与 legacy 各一项完整四块最终性、
发布重试、服务重启、后继提案与退休测试，0 失败、0 忽略。新增护栏证明
父加载/鉴权/NCW2 创建无需 Store 物化；过期/缺失 head、缺 h/根节点、
坏签名/nonce、重绑 Ready 的 NCW2 描述符篡改及损坏直接前块均拒绝且不修复。
初跑曾误将 NCW2 反例挂到 Execute 旧输入，已移到真实 finalized Transfer；
完整回归随后发现直接前块损坏未阻止退休的退化，已补验证后重跑全组。
严格 Clippy `-D warnings`、格式与补丁检查通过。最终 record/legacy 完整测试
分别耗时 303.71/290.66 秒，含故障与网络恢复，不是出块间隔或 TPS。

剩余：生命周期/池/状态查询缓存、登记/签名/晋升仍有全量边界；账本完整验证
仍是 O(history)。自动 16 笔限制、增量 nonce 证明和同一路径持续最终确认
TPS/P95/P99 尚未完成。本机回归不能替代实体多机、公网、Linux/nightly 或生产验收。

## 设备 A：首次 Transfer 执行和输出保存使用增量（2026-10-01）

基线 `5066a50`。已有三树父引用的 record 创世纯 Transfer 候选，首次计算、
准备、保存和读回不再构造完整父/子 Store；复用原整批鉴权、AOEM 通用计算、
冲突原序和统一费用。状态与回执根来自三树更新，本批 nonce/reservation/回执
点读核对，准备与保存均验证精确变更、根和统计，落盘用新 reader 验证。
本地 V3 文档与原冷 writer 逐字节一致，没有修改交易、共识或根编码。

旧 V1/V2/inline reservation 与明确容量超限仍可走冷兼容，但复用同次计算结果，
不重复调用 AOEM 或结算费用；已有预留必须精确重现原摘要，其他错误不降级。
测试护栏同时覆盖原 read_store 和 materialize_update，显式冷调用必须被拒绝。

本机 Windows FULLMAX 已通过 41 项定向 Node 测试：record 候选 7、三树文档 3、
增量重放容量 1、旧 Transfer 5、旧 inline 2、点读鉴权 3、变更见证 3、受限
访问集 10、workspace 编码/锁/容量 4、输出容量 1、整批鉴权先于输出 1、完整
四块最终性/发布重试/服务重启/后继/退休 1；没有失败或忽略。
真实 NCW2 首次公开执行的五笔签名交易实测 peak_inflight=2，无人为等待；
余额、费用、nonce、成功/失败回执和根与串行参考一致。首轮新增夹具未提供
足够手续费而失败，已补足测试资金并增加费用/业务失败区别断言，未改经济规则。
新 V3 和旧三种格式均覆盖四阶段中断、重新打开、原预留和 authority 不变。
严格 Clippy `-D warnings`、格式与补丁检查通过；完整四块测试耗时 332.30 秒，
未放宽原期限，这不是出块间隔或 TPS。
另核对随包 Windows/Linux core 哈希与 SDK 基线一致，并复跑 Windows ML-DSA
Host runtime 2/2，未重新测量密码性能或宣称交易/封印已接入 PQ。

剩余全量边界：live-parent capture、首块/旧输入、Execute/混合路径、提案/池、
晋升和完整历史账本验证。默认创世、自动 16 笔、AOEM 和生产参数不变；增量
nonce 证明和同一真实路径持续最终确认 TPS/P95/P99 尚未完成。不是端到端
O(touched)、全库、实体多机/公网/Linux/nightly 或生产签收。

## 设备 A：后继候选输入改为已最终化父状态引用（2026-10-01）

基线 `5434536`。这一步让 record 创世的纯 Transfer 后继在保存与恢复候选输入时，
不再加载整本父业务状态。新增本地 NCW2 输入：携带父区块、原 BFT 最终性证书、
已发布输出文档的原始字节和三树引用，不放置默认或不完整的 Store。它不是交易
wire、主链协议或抗量子版本变更；首块、Execute/混合批次和已有 NCW1 仍保留冷路径。

父引用核验链是原始输出摘要、账本中该高度的 finalized execution/晋升承诺、
保留的 AOEM 发布证据 `h`、prepared 三树绑定，以及区块中的状态/回执根和执行
证据。不能只靠几个根哈希或 prepared 标记授权。历史核验不要求父块仍为当前
head；入候选、签名和晋升仍须在原锁顺序下独立核对当前 authority，不新增 head。
相关账本配置、区块、证明和执行绑定合并为一次完整历史核验，未取消历史检查。

`load_execution`、完整输出重入执行和块候选读取，已连通 NCW2 输入与 V3 输出
的点验路径。旧 NCW1 保留原 204 字节 descriptor 编码及原全量父摘要；NCW2 使用
独立 magic/schema/摘要域，workspace ID 不变。已有 reservation 不升级或降级，
只有首次预留前明确的 8 MiB 容量超限才可选原冷输入；坏来源、缺证据或存储错误
不触发降级。源输出字节嵌入子候选后，不依赖已退休父 workspace 的输入/输出 chunks。

本机 Windows FULLMAX 本轮通过 40 项定向测试，无失败或忽略：record 候选 7、
三树存储 3、旧 Transfer 5、旧 inline 恢复 2、workspace 编码/容量/锁 4、点读
鉴权 3、变更见证 3、受限访问集 10、输出容量 1、整批鉴权先于输出 1、完整四块
流程 1。严格 Clippy `-D warnings`、格式和补丁检查通过。首次静态检查发现
新枚举栈空间过大，已将两种输入都改为 Box 并复验，没有放宽告警门槛。

真实后继覆盖 NCW2 Reserved/PartialPayload/PayloadWritten/Ready 四阶段中断、
重新打开和原 descriptor 恢复；整段禁止全量物化，authority 与 output reservation
保持不变。公开执行/块候选读取也在该护栏下与完整冷结果比较；显式冷适配必须
被护栏拦下。篡改原始 source 字节、执行或晋升绑定、QC 仅留 2 票、错误但重新
提交哈希的高度计划、根和统计均拒绝。缺 prepared/root 数据拒绝；未触及的旧
blob 缺失不伪装为全历史可用，真正点读及冷读拒绝。

完整 record 四块流程通过（330.32 秒为整项测试耗时，不是出块间隔或 TPS），
没有放宽原期限。该 fixture 的第三块在 record 分支改为真实 Transfer，旧 profile
仍为 Execute；第二块 workspace 退休后，仍保留的第三块 NCW2 可在禁止物化
护栏下恢复，覆盖“直接父候选 chunks 已清理”的边界。首两块与后继路径同时
覆盖旧 NCW1 和新 NCW2，发布中断、BFT 决策、服务重启、后继和退休检查保留。

仍未完成：候选创建的 live-parent capture、首次计算与输出准备、提案/交易池和
晋升还有完整状态读取；账本证书验证仍为 O(history)，创世配置也仍会编译核对。
点验不保证未访问历史 blob 的可用性，真正访问缺失记录或显式冷导出必须报错。
不宣称端到端 O(touched)、最终确认 TPS、全部 Node 回归、四机/公网/Linux/nightly
或生产签收。默认创世、16 笔自动选择、收费规则、AOEM 及部署状态均未改变。

## 设备 A：候选输出恢复按变更校验（2026-10-01）

基线 `269c53b`。这一步减少的是“已经算完的候选被再次读取或恢复”时的历史
读取，不是新增收费或交易种类。显式 record 创世的纯 Transfer 输出新增本地
`novovm-candidate-record-document/v3`：记录精确、有序且唯一的变更路径，从
独立验证的父三根重放物理/共识状态/累计回执树更新，核对输出根与记录统计；
输出恢复、重复 `execute`、`load_execution` 和块候选产物读取使用该校验路径，
不重执转账业务，也不扫描输出的全部旧回执。

鉴权共用原签名与 nonce 规则，点读本批 signer 的父 nonce、reservation 和回执；
整批通过后才允许 AOEM 提交。物理值仍与共识树交叉核验，必需字段或 blob 缺失
不能默认为空。输出变更还受已鉴权批次的访问集约束，必须保留祖先回执、准确
追加本批回执并更新对应 nonce；prepared 标记和路径见证本身不授予最终性。

这里的 V3 仅是本地持久文档版本，不是新交易 wire、共识协议、链根或 ML-DSA
版本。旧 V1/V2/inline reservation 仍要求原字节精确恢复，错误 V3 不降级放行；
合法大输出可在首次预留前选择原冷格式。没有第二个 authority head，没有更改
默认创世、自动 16 笔选择、统一经济规则、AOEM 或既有 BFT 晋升边界。

本机 Windows FULLMAX 本轮验证：点读鉴权 3/3、受限访问集 10/10、增量见证
3/3、真实 record 候选 7/7、三树文档 3/3。新 V3 与旧 V1/V2 输出均覆盖
Reserved/Partial/Written/Completed 四阶段恢复：完整写入后不重执业务，旧预留
摘要不升级；余额、原统一费用、nonce、当前回执与冷全量参考一致，候选执行
不改变 authority。输出点验测试禁止调用全量物化，另确认冷读会触发该护栏，
不是只检查计数或字段。错父根、遗漏/重复/越权变更、旧回执改删、统计篡改、
缺 blob/完成标记、无父引用的 V3 拒绝；V2 标记不能替代 V3 标记。

完整 record 创世四区块生命周期通过（259.22 秒是整项测试耗时，不是出块
间隔或 TPS），覆盖本机服务决策、晋升、重启、后继和退休，未放宽原期限。
严格 Clippy `-D warnings`、格式与补丁检查通过。没有重跑全部 Node 库或
四台实体机器、公网、Linux 安装实机、nightly 长跑，不以这些局部门禁宣布生产签收。
旧 Transfer 回归 5/5、旧 inline 输入/输出恢复 2/2、原整批鉴权先于输出门禁
1/1 也在本轮复跑通过；以上共 35 项，无失败或忽略。
增量校验不扫描未触及历史 blob 的可用性；显式冷导出和最终晋升仍做完整检查。
输入 NCW1 仍全量验证，首次输出 compute/preparation、晋升及最终父对象仍有冷
物化/扫描。不能称为端到端 O(touched)、持续最终确认高吞吐或生产签收；后续仍
需消除这些真实热路径的全量工作，并测量同一路径的持续吞吐和 P95/P99 延迟。

## 设备 A：持久三树进入真实转账执行（2026-10-01）

从 `934c2c3` 继续：显式 record 创世的纯 Transfer 候选，现直接从同一 AOEM
数据库的物理记录树、共识状态树和累计回执树读取声明的账户/nonce/费用/窗口
记录；执行阶段不再编码完整父 store 或重新导入两棵共识树。完整批次仍先鉴权，
AOEM 通用 callback 计算转账，原序费用结算与冲突屏障不变。每个共识相关点读
同时核对物理值与共识投影，已有回执核对其承诺，缺 blob 不能当成不存在。

内部持久文档 `novovm-candidate-record-document/v2` 将三根、三组父根和 profile
纳入同一文档摘要。各树使用不同 role 的 prepared ID，验证 descriptor、完成
标记与输入/执行承诺后才允许文档完成；原 output digest→晋升 intent→单一
authority head 链不变。它不是新交易 wire、共识算法或链根版本。旧文档 v1、
旧 inline 快照继续按已预留的原字节恢复，不重写旧 reservation。

物理增量同时精确更新有效记录数及 NRB1 blob 字节数，不能当成实际磁盘占用；
结构删除、重复路径、缺失父结构/旧 blob 拒绝。单次树 staging 有限额，但按
有序批次分段，不将 4096 条记录限制误当成整块合法写集上限。执行局部只缓存
不可变 node/chunk，最多 16384/4096 项，调用结束丢弃，不缓存权限或缺失结果。

本机 Windows FULLMAX 已验证：真实候选 3/3、三树文档与恢复 3/3、增量统计/
物理 patch 4/4。新增已验签真实 AOEM 转账点读门禁 1/1：空历史读取 194 个
node、66 个 chunk；增加 1024 个未触账户和 100 条历史回执后为 654/66；两边
均只读同一组 66 个 record key，未读任何无关历史 blob。计数从父三树预建后
开始，不含冷导入/冷验证，不是端到端 TPS。输出与同 codec 冷执行的完整状态、
回执、费用、nonce、三树根一致；错 state 根、错 receipt 根及缺付款方 blob 拒绝。

完整 record 创世四区块生命周期复跑通过（260.41 秒为整项测试耗时，非出块
周期或 TPS）：现有本机服务收票/决策、转账、晋升、重启、后继和工作区退休
继续通过，未放宽原期限。严格 Clippy `-D warnings`、格式及补丁检查通过。
旧 Transfer 回归 5/5；另一个真实旧 record/v1 输出四阶段恢复测试通过（5.45 秒）：
Reserved/Partial 只恢复原预留字节，Written 仅补完成标记，Completed 不重执行；
余额、费用、nonce、单回执及权威状态保持正确，篡改预留摘要拒绝。
通用 record/物理布局普通回归 12 通过、1 个大型夹具未重跑（其中包含上述
4 项统计/patch 测试）；旧 inline 输入/输出恢复 2/2。没有重跑全部 Node 库。

未完成范围仍明确：输入捕获/鉴权、输出物化与完整恢复校验、最终父对象、池/
提案/退休流程还会加载或扫描历史。混合 Execute 候选仍走冷兼容路径；本刀
没有消除整条主链全量扫描，没有更改默认 16 笔、Execute 数值域或 nonce 证明。
下一步继续将三树引用传递到这些真实热入口，并实测签名提交到最终确认的持续
吞吐及 P95/P99；不能把点读门禁或缓存命中作为主链高 TPS 签收。

## 设备 A：新创世 record 根与交易内增量执行（2026-10-01）

在 `b971c58` 上新增显式 `novovm-fresh-genesis-config/v2`。该配置使用独立
state record-tree / cumulative receipt-tree / AOEM evidence profile，创世 pin、
块哈希、首块登记、QC、归档最终性和父子连续性均绑定同一 profile。旧创世
v1、旧根和旧块哈希字节不变，禁止原链切换或混搭；默认配置/生成器未改，
没有批准或部署任何生产创世。这里的 v2 不是交易 wire 或决策票 V3 的版本。

真实 fresh 候选的 Transfer 段现加载受限视图：完整账户标识的 NOV 余额、
触及的签名 nonce/预留、当前回执、既有统一费用字段与最多 512 条追踪/日志
窗口；保留其它资产和未触及记录。AOEM 通用任务仍负责转账计算，Host 按原序
结算费用及合并。先提交业务/nonce 增量形成中间根，再生成 semantic seal、
追踪与最终状态根，最后加入累计回执树，避免自引用。Execute 仍是有序屏障。
真实鉴权仍在整个批次 AOEM 提交之前；候选鉴权不再克隆整个历史 nonce map。

本机 Windows FULLMAX：访问集/六种费用分支/权限边界 7/7，记录投影/大整数/
诊断排除/累计回执与有界冷根缓存 8/8，根版本与创世/QC 混搭拒绝 13/13。真实新 profile
五笔独立/冲突/失败交易门禁通过，观测 AOEM callback `peak_inflight=2`；
与同 codec 逐笔单任务调度的完整 store、回执逐字一致（不是独立业务实现，
也不是新旧 codec 字节相同）。覆盖费用守恒、失败耗 nonce、OutputWritten
中断后仅补完成标记、3/4 本地签名决策、晋升及后继旧 nonce 拒绝。
另以总额 `u128::MAX` 的新创世执行真实转账、构块与重开，金额保持精确；
两项新 profile 实测均通过。旧 Transfer 回归 5/5，ledger 21/21、seal
129/129（后两组包含部分根版本测试），不是重跑全部 Node 测试。

恢复回归曾发现既有测试把不同节点的具体 certificate hash 当作唯一决定。
代码本已允许同一目标的不同合法 QC/签票子集；测试现额外保留前三节点原始
归档证书并检查重启不变，同时逐节点验签、检查状态哈希与本机存档一致、
比较完整 decision target/区块。未修改共识逻辑或放宽签名阈值。

完整生命周期首次复跑还暴露重复冷构树超出区块体接收测试原有 30 秒预算。
保留期限，新增纯函数有界 memo（状态/回执各最多 64 个输入哈希→根）；每次
仍重新序列化并哈希实际 typed 内容，未缓存 authority/验证标志或原始状态，
仅复用完整冷计算成功的根，错误不缓存。不能把 memo 命中当成无历史扫描。
完整新 profile 生命周期复跑通过（239.54 秒为整个回归耗时，不是出块时间）：
含四个区块、同机服务通信、决策签票、晋升、重启和后继恢复，原有区块体接收
30 秒期限未改。随后两项真实候选测试再次通过（2/2，6.81 秒）。
严格 Clippy `-D warnings`、格式检查和补丁空白检查通过。

最后数值域复核发现旧 Execute 的全状态 JSON 证据无法表示大于 u64 的余额；
新 record 创世/Transfer 支持这些值，直接沿用旧 Execute 会 panic。现对包含
Execute 的新 profile 候选在 AOEM 预提交前检查父状态，并在每个 Execute
屏障前重查（前序 Transfer 可能刚产生大额余额）；超出旧域明确返回错误，
不发布候选或晋升权威状态。旧 finalizer 也在 nonce/receipt 修改前对前后状态
做可失败的域检查。此时是“不支持该 Execute 候选”，不是收费成功或失败回执。
这没有迁移 Execute 全部数值语义：旧业务日志中的大额 `json!` 和派生负 delta
的 i64 下界仍需单独修复，不能将真实 Transfer 的 u128 精度验收外推到全部模块。
数值域单测 3/3，新增真实候选测试覆盖“父状态已超域”和“Transfer 后才超域”
均拒绝且权威创世保持原样；最终新 profile 候选组 3/3（8.24 秒）。

仍未完成：候选边界保留全量加载/克隆，每段从完整父状态冷导入两棵共识树，
结束后冷重建校验；两棵共识树节点尚未作为热路径引用持久化。每笔只序列化
受限视图，但视图包含本段交易及有界窗口，不能称为最终单交易最小写集成本。
旧 V3 nonce witness 不适用于新根，v2 对应证明尚未接入，不能冒充 ZK 执行证明。
默认 16 笔选择和持续最终确认性能仍待完成；没有宣称 TPS 提升、四台实体节点、
Linux/公网或夜间长跑签收，目标保持进行中。AOEM 源码和随包库本轮未修改。

## 设备 A：候选状态记录存储接入（2026-10-01）

在 `a76adf8` 上将真实候选输入和执行输出的完整 store 内嵌快照，改为同一
AOEM 数据库中的不可变记录与版本化引用。账户按资产、nonce、历史回执和
其余动态 map 分项存储；后继候选复用父记录，只新增变更 blob/树节点。
`RawValue` 保留 u128 整数，完整原路径放入受哈希保护的记录，不截断长资产名。
缺块、错误 hash/count/路径、未知或缺失字段均拒绝，不能读出“半份完整状态”。

这是**物理存储编码更新，不是共识根升级**：V3 state/V2 receipt、签名、
nonce、统一收费及 BFT 校验继续沿用；完整本地记录根含 namespace/诊断信息，
不得用作 QC 状态根。prepared marker 不授予最终性；仍由原 BFT 晋升流程更新
同一个 AOEM authority head。旧 workspace 回收不删除共享不可变记录；新版本
也能精确恢复已有 inline 候选，不把旧未完成 descriptor 换成新编码冒充重试。

本机 Windows 新随包 FULLMAX：真实签名 Transfer 候选 5/5；旧 inline 输入/
输出四阶段恢复及篡改拒绝 2/2。大状态文档测试 1/1：9,104,017 字节 typed
store 的引用文档约 400 字节（两次运行 396/402），改单余额仅新增 1 blob
（155 字节）和 8 节点；
关闭/重开后旧新状态精确一致，另一资产保持不变，authority 未变化。这是合成
历史的存储测试，不是 9 MB 主链历史的最终确认 TPS。

组件普通测试 19 通过（含 3 项既有状态根测试）；显式 record 跨新进程恢复、
缺 blob 拒绝修补、40 版本恢复和两个大状态组件门禁通过。record 门禁包含
16,384 条/10,059,776 字节记录，单键 15 节点/1 blob；纯 tree 门禁包含
32,768 keys/12,779,421 字节，单键 18 节点/1,974 字节。

完整候选/BFT/生命周期回归 **34 passed / 0 failed / 3 ignored**（488.73 秒；
ignored 为由父测试显式调用的进程 worker）。第 4 高度改用真实签名 Transfer，
通过 prepare/decision QC、晋升中断、生命周期重启、回收旧 workspace 后读回；
复核付款/收款余额与实际手续费。新 finalizer 接口的原适配与旧实现 store/
receipt/mirror 字节一致，真实 AOEM 成败/中断测试 2/2。Node lib/tests 严格
Clippy、全仓格式检查、diff check 通过；最终金额精度/缺失字段/路径编码测试
3/3、Transfer 候选复跑 5/5。不是重跑全库或公网/Linux 验收。

尚未完成：计算和校验仍全量物化、克隆并重算旧共识根；默认自动提案仍选
16 笔，历史增长后的持续最终确认吞吐尚未签收。8 MiB 仍约束单条 record 和
输入/输出文档，原交易体上限不变；fresh 候选累计 store 不再内嵌于文档预算；
legacy 入口的旧 snapshot 限制未宣布消除。尚无不可变节点 GC，不删除旧版本。
下一步才是 touched-state 执行、独立增量共识/回执根及对应 nonce 证明版本，
不能把本次 record root 改个名字就升级为共识根。未改 AOEM 源码、未部署。

## 设备 A：ML-DSA FULLMAX 随包库交接（2026-10-01）

本次 `aoem/` Windows/Linux FULLMAX 固定 AOEM 源提交 `56e9da15`，不是当前
AOEM main 的所有后续改动。两平台实际 core 各通过 B 原脚本的严格 **90/90**
官方 sigVer 子集，另各通过 9 组独立实现双向/私钥导入互操作和 4 次批验。
两平台 ML-DSA sidecar 也各通过相同 90/90 与 9 组互操作；完整 core、14 个
插件及 6 个 KMS/HSM 别名已构建并同步，未用精简库替代 FULLMAX。
公开摘要与范围见 [SDK 基线](../aoem/RUNTIME-BASELINE.md)。旧库 78/90 保留为
历史失败，不能改写；此次不是 FIPS 认证，不包括 preHash/externalMu 子集。

Host 显式 runtime 测试已改为 44/65/87 全部必须接受官方正例，且全部运行
篡改/错误 context/旧 raw 签名负例。旧库在新断言下真实失败，新随包 core
Windows/Linux 各 **2/2** 通过；不再把 65/87 被阻断计作修复成功。两平台
真实 AOEM 并发/取消/持久图回归各 **3/3**；Windows 真实签名转账候选 **5/5**。
64 个 Host 使用符号双平台均保留；全历史导出并非完全不变，详情见 SDK 基线。

沿用用户已接受的标准正确优先取舍，不再把并行构建时耗测作密码性能成绩。
仅本机 Windows 与 WSL 验证，实体 Linux/公网/夜间长跑未因此签收；主链
交易和封印仍未接入 ML-DSA。主网签名参数和强制/混合规则待用户确认，不能
将已有 UCA Mldsa87 标签擅自扩展为已经批准的全链规则。

经典隐私 JSON v2 源码另在 AOEM `38f602ec`，**不在此次 SDK 中**；AOEM
`a40c8dc0` 另修复了后续 FULLMAX 打包自动附带 ML-DSA 许可证与来源说明。
主链隐私/防双花、增量根晋升、最终确认吞吐以及已知 RISC0 guest 风险仍未完成。

## 设备 A：真实候选 NOV Transfer 计算接通（2026-10-01）

在 `fc36514` 上接入 fresh 签名 Transfer（NOV 资产、NOV 费用），不开放尚无
执行器的 legacy ingress。整个候选先完成签名、链域和 nonce 鉴权；连续的
无冲突转账由 AOEM 通用 workers 实算，冲突段和 Execute 之间保留原序屏障。
Host 使用原统一计费与分配，先实扣一次费用，再归并 AOEM 绝对 after 值；
拒费不改变金额，业务失败保留已结费用并消费有效 nonce。20/32 字节账户完整
保留，同一公钥的 nonce 身份不分叉。AOEM 没有新增 NOV 专属接口或业务代码。

本机 Windows：最终 5 项真实签名候选测试在原随包 DLL 与新 FULLMAX
`56e9da15` core（SHA256 `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`）
均为 5/5；覆盖混合 Execute/Transfer、余额和手续费失败、结算计数溢出后
后继花费、完整长账户。候选实际 callback peak=2，没有用 sleep 制造并发。
组件/调度 25 项、既有收费守恒 6 项通过；新库显式并发组件 1/1，peak=8。
既有 candidate_workspace 整套 24 passed / 3 子进程 worker ignored，包括
创世、继承、进程恢复、传播和 quorum（含前三项新测试，427.76 秒）。最终源码
Node lib/tests 严格 Clippy 与 diff check 通过；没有重跑全库并冒称新全绿。

Transfer 费用投影已进入协议指纹：须使用一致的新鲜创世配置，不混用旧协议
pin，不迁移旧测试资产。**状态增量编码尚未晋升到主路径，候选仍有全量克隆、
8 MiB 累积快照与自动 16 笔选择上限；持续最终确认吞吐尚未签收。** 下一步是
将已验证的增量根接入同一候选/最终性恢复链路，不以组件并发替代最终确认。

## 设备 A：经典隐私跨进程接口源码已交付（2026-10-01）

AOEM `main@38f602ec` 已使用 XujueKing SSH 推送。固定 monero-oxide
`9e11f5c0` 的 CLSAG + Bulletproofs+ 数学实现，增加通用真实钱包见证、收款
扫描/再次花费以及 Host 精确费用适配；不引入 NOV 余额、发行或共识规则。
现有 `aoem_privacy_execute_v1` 增加严格 JSON v2，profile 为
`clsag_bpplus_edwards_u64_v1`，没有新 C 导出或第二条内核执行入口。

钱包 8/8、engine 5/5、FFI 6/6 通过。Windows debug DLL 的独立新进程不调用
prove，也能验证正例；错误域/费用/环/证明、截断、伪造 verified、GPU 路由和
批内重复 key image 均拒绝。旧 v1 仍保留已发布 ABI 的本进程 cache 门禁，
不将其改成资产授权。完整全仓 Clippy 尚未通过，存在既有 GPU uninit_vec 阻断。

**这不是已同步的 FULLMAX/主链隐私资产。** v2 始终声明未落账、非抗量子、
未验证环成员的链上资格、未检查历史双花。Host 必须从可信账本解析环、域与
费用，原子保存 key image/输出并进入最终性。Linux v2、钱包持久安全、主链
收付/重启及独立安全审计仍待完成。当前 ML-DSA FULLMAX 构建源为较早
`56e9da15`，不包含这次隐私扩展，不能混用两个交付状态。

## 设备 A：AOEM ML-DSA 标准修复源码已推送（2026-10-01）

AOEM `main@56e9da15`（XujueKing SSH）已提交并核对远端一致。新唯一实现为
固定 upstream `mldsa-native 2.0.0 / 834a90d5`，上游数学源码原样保留；没有 NOV
专属逻辑。旧草案实现不再是源码可选路径，已有 raw/internal M'、尺寸、别名
及确定性 rnd=0 签名契约保留，导入私钥先做完整一致性校验。

Windows/MSVC 与 Linux/WSL 本地 release C ABI 各通过 90/90 适用 NIST sigVer
（18 正、72 负）、4 次批验及 9 组独立实现双向/expanded-key 互操作；另排除
90 个 prehash/external-mu 样本，不计作通过。Adapter 17+4 测试与严格 Clippy
通过；Windows COFF、Linux 独立 probe 均确认 portable/AVX2 三参数集的密钥和
确定性签名字节一致。未新增 CPU 检测公开导出，修复了内部桥接与公开验签
符号同名问题。构建使用 PATH 中 Clang，不写死设备目录。

用户明确接受标准正确版本的 Linux 性能取舍，继续接通交易/封印后优化。
五轮同机 C ABI 微检查中，44/65/87 的 Windows 验签为 19.88/32.13/49.80 us，
旧库 52.19/85.10/139.01 us；Linux Clang 为 17.16/28.30/43.90 us，旧库
15.08/23.52/36.19 us。签名新增私钥校验且有拒绝采样差异，不拿单消息结果
冒充通用吞吐。这不是主链 TPS，也不是 FIPS/CAVP 认证或实体 Linux 安装验收。

**尚未替换本仓库 `aoem/` 随包库，未发布 FULLMAX，未接入主链 PQ。**
原 B 的旧库 78/90 失败仍有效；新 DLL/SO 打包后须复跑相同 Host 门禁再交接。
隐私收付组件与 engine 接线正在单独验证，不夹带到此次密码修复提交中。

## 设备 A：NOV 实扣手续费及并行增量组件（2026-10-01，本机验收）

沿用统一费率，修复 direct NOV 费用只入结算而未扣付款方的缺口。现在先检查
付款余额和所有金额/计数容量，再扣费并沿原比例结算；不足或溢出不部分划款。
失败业务保留已结算费用，回执 before-state 包含扣费前状态；相应协议 pin
已变化，混用旧/新经济规则的节点不能作为同一协议运行。没有新增费率或正式资产。

新增组件将无冲突转账计算交给真实 AOEM workers，按原顺序输出；冲突划分、
业务定义仍属于 Host。增量状态节点和候选根使用既有 AOEM 存储，不另建权威
账本。异步取消/异常时保留完整 callback owner，未知提交不放开物理写锁。

Windows/MSVC debug 当前整套源码：Node 全库 **801 passed / 0 failed / 10 ignored**，
606.61 秒；Node lib/tests 严格 Clippy 和 workspace fmt 通过。Exec 普通单测
40 passed / 0 failed / 3 ignored。随后显式执行新增 ignored：状态 2/2、转账
1/1、Exec 3/3 全过。32,768 keys 的当前树 12,779,421 bytes，单键更新只读取/
暂存 18 个节点、1,974 bytes；40 个候选根重开恢复通过。真实转账 callback
观察到 peak_inflight=8、串行参考一致，不是仅队列计数，也不是最终确认 TPS。

日志：`artifacts/audit/fee-conservation-regression/final-isolated-20261001-055423/`
及 `components-20261001-060642/`。默认测试账本通过低优先级路径隔离；保留
原设备旧账本，未迁移或删除。mainline/UCA 测试补用已有 owning-thread scope，
修复 Windows TLS 退出等待；没有关闭 AOEM 或放宽断言。使用原随包 DLL
`e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`。

**边界：Transfer 的新计算/根编码尚未接入 fresh 候选与最终性，主链原全量
快照/选择上限未因此解除；不得宣称高并发主链完成。** Linux/实体多机、签名
入口到最终确认的持续吞吐仍待后续交付。本记录不含 AOEM 密码库替换。

## 设备 A：密码库修复的性能约束与隐私取舍（2026-10-01，进行中）

用户在了解 PQ 隐私尚无完整安全比较基线后，明确授权先完成经典隐私收付闭环：
新隐私格式、真实钱包见证、收款后再花费及链上防双花。隐私证明明确非抗量子，
ML-DSA 交易/封印工作独立继续。这不是已实测 PQ 太慢。当前 RingCT 缺少真实
钱包花费见证，独立验证仍阻断；不能直接删 cache 门禁。具体格式/集成待交付。

AOEM 首个串行 ML-DSA 修复候选（RustCrypto `ml-dsa 0.1.1`）在 Windows 和
Linux/WSL 的 C ABI 均通过 90 个适用 NIST sigVer 用例（18 正例、72 负例）及
9 组独立实现双向互操作，覆盖 expanded key 导入。但同机 Linux 微检查出现
验签约慢 3 倍、签名约慢 10 倍，故拒绝晋升并撤回实现和临时二进制；没有
同步进 SDK，不保留可选 fallback。该比较仅用于发现严重退步，不是主链 TPS。
第二候选 `libcrux-ml-dsa 0.0.10` 同样通过上述全部互操作及 Wycheproof 边界
用例，但五轮交替测量中 Linux 验签仍约慢 2 倍，亦撤回而未同步；Windows
更快不能抵消 Linux 退步。下一步核对维护中的标准 AVX2 实现及完整标准差异，
不盲目只改挑战长度一行。修复尚未签收。原始随包库和 B 的
78/90 失败证据不变；保留独立标准回归输入，不把历史候选通过改写成当前通过。

PQ 隐私另有前置风险：AOEM 当前 RISC0 guest 使用 `1.2.6`，落在官方
[GHSA-jqq4-c7wq-36h7](https://github.com/risc0/risc0/security/advisories/GHSA-jqq4-c7wq-36h7)
描述的 guest `sys_read` 证明健全性漏洞影响范围内；不能直接作为钱包证明的
可信基线。需要升级并重建可信 guest/image ID 后再评估，不是已经完成修复。
ML-DSA 修复不等于 RISC0、隐私协议、主链 PQ 或 FULLMAX 全部签收。

## 设备 B：修库交接的严格 ML-DSA sigVer 矩阵（2026-09-30 UTC，前轮）

状态：`STRICT NIST SIGVER SUBSET FAIL / 78 OF 90`。基线 `eb84fa3`，认领
`15344d2`；仅增加独立诊断和单元测试，不改生产 verifier、runtime、共享 FFI
或主链。目的为 A 的修库交接提供必须全部通过的互操作验收，而不是再把
“65/87 被正确阻断”的绿色保护测试当成已修复密码库。

固定 NIST ACVP-Server 源提交 `975de31eb83d87039ec88934fdc47d8c312b892d`，
prompt/expected 文件各自 SHA256 强制校验。覆盖六组：44/65/87 的 external
pure 和 internal raw（externalMu=false），共 18 正例、72 负例。每个负例也
必须有 rc=0 和明确 false，能力缺失/ABI 错误不算密码学拒绝。少测组、缺失或
重复用例、篡改 expected/source 均不能接受；没有只跑通过参数集的捷径。

Linux 库 SHA256 仍为
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
44 两组均 15/15；65、87 各两组均 12/15，所有正例被拒，共 12 个失败，
0 ABI 错误，此子集中未观察到负例误收。runner `accepted=false`、退出码 1；
没有修改期望把兼容性问题掩盖。原始结果：
`artifacts/crypto-b-acvp-eb84fa3/final-acceptance.json`（本机，不进 Git）。
可移植脚本、固定输入获取与复跑命令见 [B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

新增判定器单测 9 passed，Python 全部脚本单测 27 passed，fmt/diff check 通过；
这些是工具逻辑测试，不是实际密码库/主链通过。没有覆盖 preHash、externalMu、
keyGen/sigGen、完整 FIPS 认证、主链或实体多机。修库后须先达到本子集 90/90，
并继续复跑 Host 门禁与既有负例；目前仍由 A 协调 AOEM 修复。

隐私外部证明准入和钱包见证接口也仍由 A 协调，主链协议交接未完成。
本轮不部署、不生成正式密钥、不发行资产，`production_ready=false`。

## 设备 A：承接密码库及隐私接口修复（2026-10-01，进行中）

用户已明确授权 A 修改、构建和测试 AOEM 密码库，并将 B 发现的隐私接口阻断
统一交 A 处理。范围与双方编辑权见交付主线；不部署、不发行、不替换运行中服务。
SUPERVM 已同步 `eb84fa3`；此前执行/状态/计费改动保留，未混入本次文档提交。

A 原样运行 B 的 `privacy_portability_probe.py --backend Cpu`，Windows 随包
DLL SHA256 `e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`：
同进程正例通过，两个独立新进程正例拒绝；`accepted=false`，退出 1。
证据在 `artifacts/privacy-portability-a-windows-original-20261001/acceptance.json`。
这确认阻断不是 Linux 单机设置；原始失败保留，不改判 PASS。

只读核对 AOEM `eb688235`：canonical 入口要求整笔交易的本进程 prove-cache
命中；底层 RingCT verifier 的环签名仅验证 `tx.extra`，未绑定完整输出、费用
与输入承诺，也未核对外层/签名内的两份 key image 一致。既有 prove ABI 生成
临时密钥、合成环和同值输入/输出，不接收钱包已有输出的花费见证。
范围证明和承诺求和原语存在，不等于已证明花费者拥有相应账本资产。
因此不能通过删除 cache 检查来声称跨节点隐私安全完成；外部证明准入仍关闭。
后续必须明确通用完整交易绑定与既有输入见证契约，复用一个 canonical 入口，
不得临时拼接新密码协议或将 Host 自报“已验证”当作证明。

ML-DSA 通用最终标准实现修复及官方向量/独立实现互操作回归进行中；尚未交付
新 SDK，不把源代码修改或同库自签自验记为标准互操作签收。

## 设备 B：RingCT 跨进程准入阻断复现（2026-09-30 UTC，前轮）

状态：`CANONICAL RINGCT PORTABILITY FAIL / INTERFACE HANDOFF REQUIRED`。
基线 `9ad30fe`、认领 `914b19a`；本轮只增加诊断脚本和测试，不修改 AOEM、
生产 FFI、隐私协议、主链 wire 或共享候选执行路径。没有改验收期望来签 PASS。

原有 SDK confidential-transfer 完整 C 样例本机通过（`failures=0`），但它只能
证明同进程样例可用。新增 `scripts/aoem/privacy_portability_probe.py` 通过
canonical `aoem_privacy_execute_v1` 实测来自另一个进程的同一份证明字节；
不调用 legacy verify、不重新 prove、不注入准入 cache、不继承生成进程内存。

同一 Linux 库 SHA256：
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
`Auto` 和 `Cpu` 两组均为：同进程生成/准入 accepted=true；producer 退出后，
两个独立冷启动 verifier 的正例均 accepted=false，原因是
`ringct_transaction_not_admitted_by_canonical_prove_path`，在进入 engine 前拒绝。
每组 PID 不同、输入 payload 摘要相同、verifier prove_calls=0。正例没有完成
跨进程密码学验证，不能作为跨节点隐私交易路径交付。

每进程五个篡改/缺失证明负例同样在 admission 拒绝，不能据此宣称密码学
负例验证已覆盖。runner 保留 `accepted=false`、返回 1，`production_ready=false`。
本机证据：`artifacts/crypto-b-privacy-9ad30fe/final-Auto/acceptance.json`、
`final-Cpu/acceptance.json`；包含完整响应与库/脚本/输入摘要，未提交这些 artifacts。
可移植复跑命令、边界和最小需求见 [设备 B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

新增判定器单元测试 11 项通过，脚本目录全部 Python 单测 18 passed；只证明
失败不会被判成成功，不代表 portability PASS。workspace fmt/diff check 通过。
没有复跑完整主链或多机，没有替换已安装 runtime，没有生成正式钱包/资产。

下一接口依赖：AOEM 对外部公开证明的独立 canonical 验证/准入、已有输出花费
见证契约，以及 A 所有的安全 Rust 绑定。用户已确认由设备 A 统一协调这些
接口，尚未交付；B 不越权修改。不得用 Host 自称已验证或 cache 注入
旁路替代。钱包收付、守恒/防双花、费用与主链最终性仍未完成。

## 设备 A：Windows 复现 B 的 PQ 兼容性阻断（2026-10-01，同轮并行记录）

已在 main 保留双方历史合入设备 B 的 `9ad30fe`，合并提交 `e41c198`；没有新建
分支。A 进行中的执行/状态/计费改动仍在工作区，本节不将其记作交付或全库通过。
复跑采用 B 原样提交的 prover 测试和官方公开向量，Windows 随包 DLL 未替换：
`aoem/windows/core/bin/aoem_ffi.dll`，SHA256
`e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`。

`cargo test --locked -p novovm-prover`：11 passed / 0 failed，2 runtime ignored。
另显式执行 `cargo test --locked -p novovm-prover --test pq_signature_runtime --
--ignored --nocapture --test-threads=1`：2 passed / 0 failed。实际矩阵与 Linux 一致：

- ML-DSA-44：自签自验、tg1/tc11 官方正例通过。
- ML-DSA-65：自签自验通过、tg3/tc43 官方正例拒绝，组件阻断使用。
- ML-DSA-87：自签自验通过、tg5/tc70 官方正例拒绝，组件阻断使用。

因此已经跨 Windows/Linux 复现，不是仅 B 机器的本地设置报告。
只读核对 AOEM 包来源 `a951273c` 的 `Cargo.lock`：`pqcrypto-dilithium=0.5.0`。
AOEM `crates/adapter/aoem-adapter-crypto/src/quantum_resistant.rs` 使用该实现；
依赖中 dilithium3/5 的挑战值分别长 48/64 字节，但 clean 和 AVX2 的
`poly_challenge` 仅把前 `SEEDBYTES=32` 字节传给 SHAKE，签名与验签同用此规则。
这符合旧草案，而不是最终标准要求的完整挑战输入，解释了 44 通过而 65/87
自签自验通过、外部正例失败的差异；[NIST 变更说明](https://groups.google.com/a/list.nist.gov/g/pqc-forum/c/y8ul-ZcVWI4)
明确列出这项草案到最终版的修改。修复应在 AOEM 通用密码实现，不通过修改
Host framing、放宽验签或默认降到 44 规避；具体修复仍须完整官方向量回归，
不能据定位就宣布新实现符合全部 FIPS 要求。此次仅只读核对，未构建/修改 AOEM。
这两项绿色 runtime 测试证明保护门禁有效，不证明 65/87 兼容。
未选择或降级主网参数集，未修改 AOEM 仓库或替换任何正在运行的服务。
A 承接定位与修复协调；若需改兄弟 AOEM，先取得明确范围授权。原执行主线继续
独立回归，节点完整全库尚未签收；高并发主链、隐私资产和 PQ 主链接入均未完成。

## 设备 B：PQ 独立验签与标准兼容性门禁（2026-09-30 UTC，前轮）

状态：`LOCAL PQ COMPONENT GATE PASS / ML-DSA-65/87 INTEROP BLOCKED`。
本机基线 `10774c9`，认领提交 `fba6bfd`；本节结果对应随本记录提交的 prover
独立组件改动，不修改主链交易/封印 wire、通用 FFI、AOEM、网络或经济参数。
不能将本节组件通过视为主链抗量子、隐匿资产闭环或生产就绪。

新增 `novovm-prover::pq_signature::MldsaVerifier`，显式参数集、可信公钥及
context；构造时验证 NIST 官方正例和篡改负例，不仅做同库自签自验。
验证遵循 external pure 的标准 context framing；错误长度、缺失能力、ABI
尺寸不匹配、运行时错误及无效签名均拒绝，不猜算法、不自动降级。

随包 Linux AOEM source `a951273c`，库 SHA256：
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
44/65/87 自签自验均成功，但官方正例只有 44 通过；65/87 各自官方正例被拒，
新组件对它们返回 `RuntimeIncompatible`。因此测试变绿代表**阻断正确**，不是
65/87 标准兼容，也不是选择 44 作为默认主网算法。未确定内核根因、不改内核。
用户已确认由设备 A 协调 AOEM 修复；新 runtime 必须重跑同一互操作矩阵。

本机 `cargo test --locked -p novovm-prover`：11 passed / 0 failed，runtime 专项
2 ignored；另行显式 `--test pq_signature_runtime -- --ignored --nocapture
--test-threads=1`：2 passed / 0 failed。覆盖官方正例门禁、生成签名、篡改
消息/context/密钥/签名、截断/追加、旧 raw 签名拒绝及参数/能力错误。
`cargo clippy --locked -p novovm-prover --all-targets -- -D warnings`、workspace
fmt check、diff check 通过。未重跑 node 全库、实体多机或远端 CI。

原始日志：`artifacts/crypto-b-round1-10774c9/` 的 `unit-final.log`、
`runtime-final.log`、`package-final.log`、`clippy-final.log`（本机，不进 Git）。
可移植证据与复跑命令见 [设备 B 接口交接](NOVOVM_CRYPTO_B_HANDOFF.md) 及
其中链接的官方公开向量。样本不是完整 FIPS/CAVP 认证；未来主链还须完成
批准的消息域/账户绑定、交易与封印集成、恢复、降级拒绝和性能验收。

隐私侧只读核对发现：SDK 推荐的 `aoem_privacy_execute_v1` 尚未暴露到现有
Rust bindings，prove 示例缺少钱包已有输入的花费见证接口；不能把同进程
证明样例当作可重复收付隐匿资产。共享 FFI 仍交 A 协调，不引入旧 verify
旁路，不修改兄弟仓库。`production_ready=false`，不部署、不发行。

## 离线 leader 换轮证据验收（2026-09-30 UTC，前轮）

状态：`LOCAL VERIFIED OFFLINE-LEADER FAILOVER SMOKE PASS`；完整本机 runner 已通过。
本轮只改进程测试和报告，不改生产 pacemaker、quorum、签名、超时或 AOEM。
仍为 `b8aad0a` 加未提交工作区，未提交推送、部署或替换旧安装 runtime。

前轮失败记录仍保留且仍为 `accepted=false`，不修改成成功。
只读检查其三个 seal 数据库：轮次 0/1/2 均有本地 timeout；节点 0/2 在轮次 1
保存提案与 prepare 票，节点 3 没有轮次 1 的提案/票。全体最终形成轮次 3 QC。
通过原存储读取 API 重验决策、提案签名及 NewView admission，三份均有效，
包含 3 份 prepare 票、3 份决策票、轮次 2 的 3 份 timeout 和 3 份 NewView。
没有离线节点签名，最终块与决策哈希一致。证据：
`artifacts/failover-round-b8aad0a/old-evidence-verified.json`。
这是旧数据只读取证，不算一次新的进程恢复通过；原失败后的重启步骤仍未执行。

旧用例把“期望首个替补轮次完成”写成 `round == 1`，但真实定时器、投票与
消息到达不保证固定完成轮次。证据能说明发生了额外 quorum timeout，不能还原
每条消息的延迟原因，也不表示已解决延迟或任意故障活性问题。

新验收不只是将断言改为 `round > 0`：
- 保留原 180 秒最终化期限、3/4 quorum、同一交易/完整历史和离线节点不参与要求。
- 通过原 API 加载持久决策、签名提案和非零轮 NewView admission；绑定已最终化块、
  高度及原 authority，验签并检查该轮 proposer 确为原排班 leader。
- prepare、decision、preceding timeout、NewView 的所有签名者都不能是离线节点。
- 内存负例删减 timeout/NewView/decision quorum、破坏签名、错高度或块绑定必须拒绝。
- 重启后逐节点重验并比较完整 witness，不仅比较最终块；报告记录实际轮次、
  首个替补与实际 prepare proposer，避免把后续轮次误记为轮次 1。

新普通主进程专项 `artifacts/failover-round-b8aad0a/run1.log`：1 passed / 0 failed，
295.25 秒；本次实际轮次 1，离线阶段 60.221 秒。三节点最终化同一块，
内存负例均拒绝，重启后完整 witness 和历史不变。严格 Clippy、fmt、diff 通过。
旧数据轮次 3 的只读验签与本次新运行轮次 1 分开记，不混作两次新进程运行。

完整证据：`artifacts/local-readiness-failover-witness-b8aad0a/acceptance.json`，
`accepted=true`，`production_ready=false`；运行前后源码摘要均为
`4a52f77ffbc9040bdf9af60df53d0d8e54246ba32d87160ee9a131b6ab6bf7f0`。
节点全库 766 passed / 0 failed / 7 ignored；网络全库 487 / 0 / 1；
CLI 默认组 5 / 0 / 8；runner 自测 7 项、控制器自测 1 项、fmt 和严格 Clippy 通过。
连续恢复 1 passed（433.467 秒），离线换轮 1 passed（295.426 秒），
分区恢复 1 passed（81.922 秒），交易池故障 1 passed（248.205 秒），
数据库启动/账本发布打开故障 1 passed（49.306 秒）。其他 ignored 不计通过。
此次完整 runner 中离线阶段 58.974 秒、实际轮次 1，重启后完整 witness 保留。
完整 runner 结束后只更新文档和安装提示，未修改程序或测试、未替换旧 runtime。
下一步继续本机 AOEM/seal 运行中写入故障、后续 ledger 检查点和长稳；
之后仍须 clean release、公网准入、实体多机和批准的创世/运维验收。

不宣称网络性能、首次替补轮次时延、任意分区/拜占庭恢复或生产就绪。

## 数据库启动与账本发布存储故障（2026-09-30 UTC，前轮）

状态：`LOCAL DATABASE STARTUP / LEDGER PUBLICATION STORAGE FAULT SMOKE PASS`。
专项已通过并复测；完整本机 runner 因旧离线 leader 用例失败，不能签完整 PASS。
仍为 `b8aad0a` 加未提交工作区；本轮只增加验收，不修改生产执行、AOEM、
共识签名、quorum 或网络协议，不替换已安装 runtime。

专项 `artifacts/storage-startup-b8aad0a/run5.log`：**1 passed / 0 failed，49.38 秒**。
使用普通 `novovm-node`、真实 AOEM、四个独立状态目录和认证回环 WSS。
每个故障卷仅挂载在本轮夹具自己的数据库路径，私有 mount/network namespace、
64 MiB tmpfs；实测内核 `EROFS`（30）和 `ENOSPC`（28），不填宿主盘。

- AOEM authoritative provider `owner.rocksdb` 与共识 `seal-db`：只读或磁盘满时
  冷启动非零退出，不能完成服务启动或发布最终确认；恢复读写/容量后，AOEM
  候选重放结果、seal 签名记录和 ledger 逻辑记录均不变。
- 通用 `persist` 仅作对照：当前 fresh 状态不由它承载，故障时启动仍可成功，
  但不能越过 quorum 确认。它会被打开、可能改写 LOG/MANIFEST 等元数据，
  不声称完全未访问，也不将该对照算作权威存储 fail-closed 验收。
- ledger 在未形成决策前只读打开，不能把只读状态下启动成功判为缺陷。
  测试在只读 ledger 上让普通三节点实际形成并归档 3/4 决策，随后发布路径
  打开可写账本因 EROFS 失败退出。再次填满该卷并重启，恢复发布因 ENOSPC
  失败；两次都在写入 promotion intent / authority 之前。原决策验签通过且
  记录不变，账本未产生新逻辑记录，没有虚假最终确认；不是已打开 DB 的
  数据批写或 WAL fsync 中途故障验收。
- 恢复的是故障后的同一数据库文件，不从故障前备份回滚、不跨节点复制状态。
  继续普通 3/4 最终化，确认后重启，再让第四节点追赶；四份完整高度一历史一致。

夹具早期失败暴露了上述 persist/ledger 打开语义和 AOEM Snappy 编译差异，
不是生产缺陷。AOEM 用自身 prepare/readback 及后续最终化验证，加 SST/非空 WAL
摘要保存证据；不使用缺少 Snappy 的宿主 RocksDB 代替 AOEM 读取执行数据。
seal/ledger 则用只读逻辑记录摘要验证，避免把无关日志元数据变化误判为丢数据。

完整 runner 新增显式 `database-storage-startup-faults` 门禁；默认 ignored 不计通过。
完整结果：`artifacts/local-readiness-storage-startup-b8aad0a/acceptance.json`，
`accepted=false`。节点库 766 / 0 / 7 ignored、网络库 487 / 0 / 1、CLI 默认
5 / 0 / 8；runner 自测 7 项、控制器 1 项、fmt 和严格 Clippy 通过。
连续恢复通过（458.836 秒）；离线 leader 门禁失败（368.651 秒）：
`native_seal_failover_process.rs:90` 要求 prepare round=1，实际为 3。
三个普通主进程日志均记录高度 3 已最终化、相同区块及决策证书哈希；但本次
未执行该用例后面的重启断言，不能据此改判 PASS。为何未在轮次 1 完成仍待定位；
没有修改原断言、重跑到绿或把失败删除。

同一源码另行显式执行 runner 被中断而未运行的三个门禁：分区恢复 1 passed
（81.321 秒）、交易池故障 1 passed（251.245 秒）、本轮数据库故障 1 passed
（48.654 秒）。补跑证据：`artifacts/storage-startup-b8aad0a/followup-gates/acceptance.json`；
只表示这三个专项通过，明确 `full_runner_accepted=false`，不替代失败的完整结果。
完整与补跑期间源码摘要前后均一致：
`6c24ef878eefce1596a92a49e6699dfb08d97c4f89ec0e9229fb8dcdf26d57ea`。
结束后仅更新文档和安装提示。下一步先定位离线 leader 轮次断言，再重跑完整验收。

不覆盖 AOEM/seal 运行中写入故障、后续 ledger 提交检查点、物理掉电、任意 I/O
损坏、备份恢复、长期 soak 或实体多机。上述项目及公网准入、clean release、
正式创世/运维验收仍待完成；不提前上线发币。

## 交易池真实存储故障与恢复（2026-09-30 UTC，前轮）

状态：`LOCAL TRANSACTION-POOL STORAGE FAULT SMOKE PASS`；完整本机 runner 已通过。
仍为 `b8aad0a` 加未提交工作区；本轮只增加测试与 runner，不修改生产执行语义。
使用普通 `novovm-node`、真实 AOEM、四份独立状态和认证回环 WSS；没有 runtime
故障开关。用户/网络/mount namespace 独立，挂载私有 8 MiB tmpfs，只填测试卷，
不填满宿主磁盘；没有隔离或挂载能力即失败，不回退到宿主文件系统。

专项 `artifacts/storage-fault-b8aad0a/run4.log`：**1 passed / 0 failed，258.07 秒**。
本次实测：
- 非 leader 先接收签名交易，随后测试卷实际返回内核 `ENOSPC`（errno 28）。
  第 11 次提交返回 `transaction persistence failed; restart required`，不返回 queued，
  主进程停止新工作并非零退出；之前已返回 queued 的 10 笔交易全部可恢复。
- 释放测试填充文件后重启；最后未 ACK 的交易本次为 unknown。允许其在其他运行中
  已写入但未 ACK，因此只按恢复状态安全重试；不把 RPC 超时当作确定未落盘。
- 将交易池文件系统 remount 为只读，普通节点拒绝启动；池文件摘要及已确认历史
  不变。恢复读写后重新开启四节点，通过原传播/执行/共识路径最终化全部 11 笔。
  本次分布于高度 2/3（4+7 笔）；不强迫待处理交易在同一高度或同一批完成。
- 四节点完整区块/哈希顺序一致；重启后重复提交返回 finalized，池为空，历史不变。
  只读状态查询允许有限忙碌重试；故障写入请求是单次，不由测试暗中重试或伪造 ACK。

初期失败定位为测试夹具：私有 namespace remount 要显式传入 tmpfs source/type，
正常 gossip 不保证全部交易同块，执行中的查询可能超过单次 5 秒。已修正夹具，
未放宽最终性、持久化错误或数据一致性断言，未修改生产 runtime。
本轮只覆盖交易池 ENOSPC 与只读启动拒绝；不涵盖 AOEM/seal/ledger 的磁盘故障、
物理掉电、任意 I/O 损坏、备份恢复或长期 soak。`production_ready=false`。

完整证据：`artifacts/local-readiness-storage-fault-b8aad0a/acceptance.json`，
`accepted=true`，运行前后源码摘要均为
`4af03388a89429380a2921d626346bd33a26dc502bd81466700f77139bf94fc0`。
节点全库 766 passed / 0 failed / 7 ignored，网络全库 487 / 0 / 1，
CLI 默认组 5 / 0 / 7；runner 自测 7 项、控制器自测 1 项、fmt 和严格 Clippy 均通过。
显式连续恢复 1 passed（433.691 秒）、无候选 leader 故障切换 1 passed
（296.378 秒）、分区恢复 1 passed（80.376 秒）、交易池存储故障 1 passed
（255.516 秒）。本次复测同样恢复 10 笔已 ACK 交易，重试后全部 11 笔在高度 2/3
最终化；其余未执行 ignored 不计通过。结束后只更新文档和旧安装目录提示。
未提交、推送或替换已安装 runtime；下一步补 AOEM/seal/ledger 存储故障及长稳，
之后仍须 clean release、公网入口、实体多机和正式创世/运维验收，不提前发币。

## 真实 AOEM 主入口分区恢复（2026-09-30 UTC，前轮）

状态：`LOCAL REAL-AOEM MAIN-ENTRY PARTITION SMOKE PASS`；完整本机 runner 已通过。
仍为 `b8aad0a` 加未提交工作区，没有提交、推送、替换 runtime 或上线授权。

本轮只补验收，不修改上一轮 V3 pacemaker、quorum、签名、wire 或 AOEM 语义。
四个独立 OS 子进程通过实际 main 入口、真实 AOEM 和认证回环 WSS 运行。
中继不能查看加密共识载荷，因此故障过滤器仅编译在 `cfg(test)` main 测试程序中，
按已认证来源及消息类型丢弃 ingress；不生成票、不改消息、不注入虚拟时钟。
必须区分：故障阶段使用测试插桩 main 入口，并非未经插桩的发布 ELF。
候选独立执行、确认后重启和第四节点追赶则使用普通 `novovm-node` 二进制。
普通构建没有 fault env 开关或控制文件路径；未停止宿主服务，端口处于隔离 namespace。

专项 `artifacts/main-partition-b8aad0a/run2.log`：**1 passed / 0 failed，82.19 秒**。
验证链：
- 两个节点持有 round 0 Prepared QC/决策锁，另外两个尚未 Prepared。
- 2+2 只交换各组消息，实际 45 秒超时后仍无 quorum、无决策确认。
- 强杀并重开四进程，原 timeout/决策签名可验证，仍不越过分区确认。
- 恢复 Timeout/TC/NewView/prepare，暂扣决策消息；全体进入 round 1，
  原 Prepared 节点保留 round 0 决策锁，其余节点形成新轮决策锁。
- 再次重开并离线一个新轮节点；其余 3/4 合并同目标决策，真实 AOEM
  authority 发布、读回验证及 ledger 最终化完成；原签名字节不变。
- 普通二进制重启保持完整历史；离线第四节点随后追赶，同一块/交易/执行根一致。
  允许不同合法见证轮次的证书哈希不同，但每份证书均验签并绑定同一 execution target。

完整 runner 已新增主入口测试二进制指纹、故障控制器自测和显式分区门禁。
最终证据：`artifacts/local-readiness-main-partition-b8aad0a/acceptance.json`，
`accepted=true`，运行前后源码摘要均为
`d6ab4005eefa3728c7b6972df193d38d119a7e31fbf813424122d1e2874c9207`。
节点全库 766 passed / 0 failed / 7 ignored，网络全库 487 / 0 / 1；
CLI 默认组 5 / 0 / 6；控制器自测 1 / 0，runner 自测、fmt、严格 Clippy 均通过。
显式连续恢复 1 passed（443.631 秒）、无候选 leader 故障切换 1 passed
（295.886 秒）、本轮分区门禁 1 passed（79.870 秒）；默认 ignored 不计作通过。
完整 runner 结束后只更新本节、使用说明和安装提示，未再修改程序/测试源码。
仍不覆盖任意分区/拜占庭组合、长期 churn、断电/磁盘故障、实体多机或公网准入；
`production_ready=false`。下一阶段优先补单机磁盘故障与长稳，再做实体多机。

## Prepared 节点换轮与 2+2 僵持恢复（2026-09-30 UTC，前轮）

状态：`LOCAL PREPARED PACEMAKER / 2+2 SERVICE REGRESSION PASS`。
基线仍为 `b8aad0a` 加未提交工作区；未提交、推送、部署或替换旧 runtime。
完整 runner `accepted=true`、`production_ready=false`，不作为主网上线许可。

红灯先复现：两个节点 Prepared 后停止计时，只能重传两份决策票；另外两个
节点已签 timeout，既不能回签旧轮决策，也凑不齐三份 timeout 来进入新轮。
证据：`artifacts/prepared-pacemaker-b8aad0a/red.log`。

本轮修复边界：
- 仅 V3、已有可验证持久本地决策票、尚未归档完整决策时，Prepared 节点继续
  参与本地计时、Timeout/TC/NewView 及同一不可变候选的新轮 prepare。
- 仍须达到原加权 quorum 才推进持久 active round，不更换候选或释放高度锁。
  初次 Prepared 尚未持久签决策时，不先超时越过原签名入口。
- 原 Prepared QC pin 与原决策签名不随 active round 改写；重传使用原 QC 对应
  的持久 proposal/admission，而非混用新轮 proposal。重启重验完整历史见证。
- NewView quorum 携带的最高 QC，先全量验签、匹配本地已执行候选，再经既有
  持久导入 API 保存证据；不绕过非零轮历史 admission。迟到 QC 超时门禁保留。
- 不改 NOVORUDP/共识 wire、quorum、V1/V2 签名语义、APFL、AOEM 或 ledger。

新增两项真实回环 WSS、四份独立 seal 数据库的服务级回归，合成已执行候选：
选择性丢包构造两份旧轮决策票与两份 timeout 的 2+2 分割；两侧不足 quorum
均不能推进或确认。恢复消息交换后，Prepared 节点参与超时，TC/NewView 将
active round 推到 1，原决策仍使用 round 0 见证；其余节点取得 round 1 QC。
分别覆盖迟到 QC 和重启后迟到 DecisionVoteV3，不重新签旧轮决策。
在 timeout、TC 换轮及 prepare 恢复点重开服务/数据库；删除原决策锁、marker、
QC 或 proposal（含同时删除锁与 marker）必须拒绝恢复且不产生新的持久记录。
再离线一个新轮节点，剩余 3/4 合并同目标决策并归档；原签名锁字节不变，
确认后重启不改归档。专项明确断言没有由合成夹具触发链最终化。

V3 服务专项 **10 passed / 0 failed**，证据为
`artifacts/prepared-pacemaker-b8aad0a/service-final.log`。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 766 passed / 0 failed / 7 ignored（761.135 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.544 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式连续运行、追赶与恢复 | 1 passed / 0 failed（423.090 秒） |
| 显式无候选 leader 离线换轮 | 1 passed / 0 failed（286.660 秒） |
| runner 自测、格式、严格 Clippy 与 diff | PASS |

完整证据：`artifacts/local-readiness-prepared-pacemaker-b8aad0a/acceptance.json`。
运行期间源码摘要前后一致：
`869426955cc6efd500007bb7e10b273f6faaa04547f2b8d34e51809c1b51ef75`。
结束后仅更新文档，未再改代码；未执行的 ignored 项不计通过。

当前只签具体同候选 round-0/round-1 的 2+2 服务级恢复，不签任意分区或轮次活性。
真实 AOEM 主进程中的分票故障注入、不同候选/更高轮缺失历史 admission 的恢复、
持续故障与长稳、磁盘/备份、公网 RPC、clean release、实体多机及正式创世/运维
仍待完成。下一步先在本机补真实 AOEM 主进程分票/分区故障验收，不提前发币。

## 迟到 QC 与跨轮决策分票恢复（2026-09-30 UTC，前轮）

状态：`LOCAL LATE-QC / MIXED-ROUND SERVICE REGRESSION PASS`。
基线仍为 `b8aad0a` 加未提交工作区；未提交、推送、部署或替换旧 runtime。
这不是所有跨轮分割场景的活性签收，也不是主网上线许可。

新增回归先复现真实调度缺陷：节点已持久签下本轮 timeout，却在收到迟到的
prepare QC 后被置为 Prepared，随后 V3 决策签名被既有超时门禁拒绝，服务停机。
红灯日志为 `artifacts/cross-round-split-b8aad0a/red.log`，错误为
`native seal signer has already timed out this height/round`。

修复只在 V3 round driver 的 poll 中增加调度保护：恢复持久超时记录后，尚未
Prepared 的节点不把迟到 QC 作为旧轮的新决策签名入口，继续原 Timeout/NewView
流程。已经 Prepared 的节点保留原见证和决策票。没有放宽 timeout、quorum、
NewView admission、候选/高度锁或持久 watermark，也不改变 V1/V2 行为或 wire。
本地安全记录损坏仍报错，不用吞掉签名错误的方式伪装恢复。

新增两项真实回环 WSS、四份独立 seal 数据库的服务级回归：
- 一个节点先在 round 0 Prepared，其他三个丢失 QC 后超时；迟到 prepare QC
  不导致停机、不落盘 Prepared pin，也不改写原 timeout。
- 另一用例重开超时节点的服务，再交付携带旧 QC 的 DecisionVoteV3，同样不重签。
- 三节点经 TC/NewView 在 round 1 为同一不可变候选形成新 QC，旧节点保持 round 0。
- 只交付两个来源的决策票不能确认；再让一个新轮节点离线，剩余 3/4 合并不同
  round 见证下的同一 V3 decision target。各节点保留自己的 QC，原决策锁字节不变。
- 重开服务与 seal store 后归档及锁保持一致；仍明确断言链未被测试夹具最终化。

该专项使用合成的已执行候选，不是 AOEM 重执行或主节点进程故障注入。
它证明具体的 round-0/round-1 服务调度与持久确认路径，不证明任意网络分区活性。
专项 V3 服务组 **8 passed / 0 failed**（包含新增 2 项）。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 764 passed / 0 failed / 7 ignored（720.73 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.60 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式连续运行、追赶与恢复 | 1 passed / 0 failed（428.18 秒） |
| 显式无候选 leader 离线换轮 | 1 passed / 0 failed（285.29 秒） |
| runner 自测、格式、严格 Clippy 与 diff | PASS |

完整证据：`artifacts/local-readiness-split-round-b8aad0a/acceptance.json`。
运行期间源码摘要前后一致：
`cc54175d6ba05a4f7b3a6e745b553fafd9f9cd9b1fc009b4b6a46514402feb87`。
结束后仅更新文档；未执行的 ignored 项不计通过，`production_ready=false`。
下一步仍先做本机门禁：2+2 prepared/timeout 僵持、其他候选/轮次分割，及带真实
AOEM 的主进程分票故障注入。长稳、磁盘/备份、公网 RPC、clean release、实体
多机和批准的创世/运维仍阻断上线。本轮通过不取消这些剩余项。

## 主节点无候选 leader 自动换轮（2026-09-30 UTC，前轮）

状态：`LOCAL AUTOMATIC CANDIDATE-LESS LEADER FAILOVER PASS`。
仍为 `b8aad0a` 加未提交工作区；未部署、替换旧安装包、提交或推送。

候选前 pacemaker 已接入显式启用连续运行的 FreshChainLifecycleV1。
重验当前最终父块后，在原持久 round store 中处理 Timeout / NewView；
quorum 超时证书逐轮推进，NewView quorum 授权替代 leader 从持久交易池
构建并实际执行下一候选，再交给原 V3 服务。保留候选 owner、watermark、
链/高度/创世、签名和 quorum 校验；不改变 wire，不清除已锁候选。
若 NewView 指向已有 prepare QC，不把它当成无候选重新提案许可。

专项首次运行发现换轮控制消息挤满每 peer 的共享队列，导致块体不能交接。
现将控制消息与块体/交易队列分开，每个绑定 peer 每组仍最多 4 项，统一 poll
预算公平处理；enqueue 不执行或签名。候选服务打开后重验并持久接入 NewView
证书，避免已验证的换轮证据在交接处丢失。坏传输绑定、未知来源、时钟倒退、
损坏父状态、2/4 不足 quorum、重启后不能提前签新 timeout 等负例继续通过。

新增真实主进程用例：四个独立数据库先具备高度 2，之后不启动高度 3 的
round-0 leader；把真实签名交易经回环 RPC 交给另一节点，不人工创建候选。
剩余 3/4 自动换到 round 1，完成本地 AOEM 执行、prepare/decision 和最终化；
三份完整区块/最终性一致，离线节点仍停在高度 2。强制终止三进程并重启后
最终历史保持一致。这是本机隔离网络上的进程故障，不是实体机器断电。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 762 passed / 0 failed / 7 ignored（738.34 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.52 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式三高度连续运行、追赶与恢复 | 1 passed / 0 failed（456.13 秒） |
| 显式无候选初始 leader 离线换轮 | 1 passed / 0 failed（286.39 秒） |
| runner 自测、严格 Clippy、格式及 diff | PASS |

默认忽略的 CLI 项只显式运行上列两项；其余未执行项不计通过。
完整 runner 期间源码摘要前后一致：
`5d909b456305151a2826f30980adcc2c02e061d779eb50a6c848247a78a6928b`。
结束后仅更新验收文档，未再改代码。
最终证据：`artifacts/local-readiness-pacemaker-final-b8aad0a/acceptance.json`。
专项通过证据：`artifacts/pacemaker-handoff-b8aad0a/acceptance.json`。
队列问题的失败证据保留在 `artifacts/pacemaker-focused-b8aad0a/`。
首轮全库失败保留在 `artifacts/local-readiness-pacemaker-b8aad0a/`：旧测试把
未来时间父块与真实系统时钟混用，被新时间保护暂停。已改用显式测试时钟，
同时断言未校时不处理、校时后仍拒绝损坏消息；生产时间保护没有放宽。

本轮只签“最终父块之后、尚无候选时，初始 leader 缺席”的自动恢复。
跨轮 prepare/decision 分票或已有候选停滞、频繁 churn/长稳、磁盘故障与备份
恢复、公网 RPC、clean release、实体多机和批准的创世/运维仍待完成。
`accepted=true` 仅指本机矩阵，`production_ready=false`；下一步先补跨轮分票活性。

## 无候选换轮作用域与候选交接（2026-09-30 UTC，前轮）

状态：`LOCAL PARENT-ROUND / VERIFIED HANDOFF PASS / AUTOMATIC FAILOVER PENDING`。
基线仍为 `b8aad0a` 加未提交工作区；未部署、替换旧安装包、提交或推送。

定位到两层限制：自动提案只允许 next-height round-0 leader；原 fresh round API
还要求已有本地执行候选。本轮先完成第二层的安全基础，不直接绕过候选验证：
`with_verified_finalized_parent_round_v1` 在 workspace、AOEM authority、ledger 锁内
重验最终父块、创世/namespace、AOEM 输出和最终性证明，只允许下一高度的
Timeout/NewView。父块换轮能力与已执行候选能力分离，区块 subject/签名入口
显式拒绝前者。复用现有 wire、quorum、持久化和 watermark，不清除候选锁。

新增 4 项真实 AOEM/独立临时数据库回归：
- 第一块及第三块最终化后，尚无下一高度候选也能建立精确高度的 round tracking。
- 缺初始 leader 的 3/4 timeout 可以换轮；2/4、重复票、坏签名不能推进。
- 关闭并重开 seal store 保留原 timeout/NewView 和轮次；重建计时器不提前超时。
- 错 chain/创世/高度、旧父块、损坏 AOEM 输出被拒绝；watermark 缺失、损坏
  vote/round state 不得重签或改写损坏后的 DB；父块作用域不得签 proposal/vote。

交接正例先证明换轮不改变候选列表、父块和状态，再显式创建/执行新候选。
切换回原 live candidate scope 后，没有 NewView admission 仍不能签 round-1 提案；
取得 admission 后由替代 leader 提案，3/4 prepare/decision 完成新高度最终化。
此为测试显式调度的库级闭环，**不是主进程自动故障切换**，也不是物理磁盘故障验收。

完整本机 runner 实测：node **761 passed / 0 failed / 7 ignored**（676.63 秒），
network **487 / 0 / 1**，CLI 默认 **5 / 0 / 4**，显式三高度连续运行与恢复
**1 / 0**（303.61 秒）；runner 自测、格式和严格 Clippy 均通过。
NewView 35 项、round-driver 16 项专项也通过；完整 runner 期间源码摘要前后一致。
测试均使用隔离网络 namespace；未执行的 ignored 不计入通过项。

最终证据：`artifacts/local-readiness-parent-round-b8aad0a/acceptance.json`；
专项与交接日志：`artifacts/candidate-less-round-b8aad0a/`。
`accepted=true` 仅指该本机回归矩阵；`production_ready=false`。

下一步接入 FreshChainLifecycleV1 的候选前 pacemaker：有界 Timeout/NewView 收发、
quorum 驱动的新 leader 提案、与候选服务的持久轮次交接，再用独立进程验证原
leader 离线后无人工参与继续出块。该自动化门禁、跨轮分票活性、长稳/磁盘故障、
公网 RPC、clean release、实体多机及批准的创世/运维仍阻断生产上线。

## 资金夹具与 V3 决策票交接回归（2026-09-30 UTC，前轮）

状态：`LOCAL READINESS REGRESSION PASS / PRODUCTION GATES PENDING`。
基线仍为 `b8aad0a` 加未提交工作区。下列完整 runner 结果替代历史失败状态；
不是所有生产门禁完成，也不是实体四机、正式 release 或发币签收。

存款正例现在只在测试独立账本中显式注资，签名 raw transaction 按解码后的真实
签名者注资，不再给旧别名或凭空增加 reserve。补验成功存款的账户扣款、
deposit/buy-asset 流程的账户加储备守恒、proof cap 拒绝后余额不变。
新增真实签名入口负例：无余额的合法交易可被准入，但业务 receipt 必须失败，
USDT 账户与储备保持零，不能产生成功存款事件。生产余额、认证和 nonce 校验未放宽。

旧夹具的其他断言同步到实际边界：直接 Ed25519 签名不能因 UCA metadata 写了
ML-DSA87 就报告为 PQ 签名；保留 PQ_REQUIRED 和隐私路径负例。semantic delta
计数包含既有 `native_committed_module_state_v3`，治理用例还核对具体 delta kinds。
执行 receipt 不等于最终性，补发交易执行后保持 `IncludedNonCanonical`。
这些夹具包含明确标记的 `legacy_host_transitional` 路径，不替代生产资金审计。

首轮全库已从 41 项失败降为 755 passed / 1 failed / 7 ignored，但发现真实恢复
缺陷：首张 DecisionVoteV3 暂存 prepare QC 后，同轮询的另一签名者决策票会因
prepare bridge 返回“QC 已见过”被丢掉。该返回值不是“决策票无效”，而且 QC
只在随后 poll 才落盘；丢票会让延迟节点停在 Prepared。
现将通过 bridge 验证的 V3 消息有界暂存，待 decision loop 建立后逐一交接，
成功入队才计 accepted。保留每源配额、签名、链/高度/目标、去重和 quorum 验证，
不修改 wire，不以单票宣布确认，也不把 decision confirmation 当成最终性。

新增真实回环 WSS 用例一次交付两个不同来源的决策票：旧实现稳定复现只接受
1 张，修复后接受 2 张，再加本地票形成验证通过的 3 票证书；仍断言未最终化。
不改原丢失 prepare 的测试过滤规则或超时。V3 服务 6 项回归独立连续三轮通过。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 757 passed / 0 failed / 7 ignored（614.07 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.56 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 4 ignored |
| 显式三高度连续运行与恢复 | 1 passed / 0 failed（301.60 秒） |
| runner 自测、严格 Clippy、格式及 diff | PASS |

最终运行期间源码摘要前后一致。所有 socket/多进程测试均在隔离网络 namespace；
CLI 的四个默认忽略项只显式执行了三高度用例，另外三项不计本轮覆盖。
网络公网 smoke 仍忽略。未连接实体设备、替换旧安装目录或创建正式资产/密钥。

最终证据：`artifacts/local-readiness-deferred-b8aad0a/acceptance.json`
（`accepted=true`，但 `production_ready=false`、`multi_machine_tested=false`）。
定位/负例/失败历史见 `artifacts/funded-fixture-regression-b8aad0a/acceptance.json`；
首轮完整失败报告保留在 `artifacts/local-readiness-funded-b8aad0a/acceptance.json`。

下一步仍先补本机门禁：无候选 leader 故障切换、跨轮 prepare/decision 分票活性、
长时间 soak 与磁盘故障/备份恢复；随后验证公网 RPC 网关、clean release 包、
实体多机以及经批准的创世参数和运维流程。本节通过不取消这些阻断项。

## 重组门禁与 nonce 测试隔离（2026-09-30 UTC，后续回归）

状态：`NETWORK LOCAL REGRESSION PASS / FULL NODE GATE BLOCKED`。
仍基于 `b8aad0a` 加未提交工作区；保留前轮的 RLPx 修复，没有放宽任何生产验证。
本节结果替代下方历史章节作为当前本机门禁状态，历史失败日志保留。

网络剩余重组用例的夹具原先让高度 121 直接引用高度 119，并期待收到远端材料
就自动选成 canonical。现改为连续的竞争分支 119 → 120 → 121，空块 state root
保持连续，通过真实回环 RLPx NewBlock/receipts 交换材料；分阶段断言收包后
主链不变、候选非 canonical/safe/finalized。随后由测试显式注入本地 fork-choice
快照，再检查重组深度/次数、原交易退回待处理、重广播原始 payload。
不删除重组断言，不把入站 header 的 canonical 标记改为 true。
这验证网络材料与本地选链状态机的分界，**不是新增生产外部链共识证明验证器**。

另修复 `cfg(test)` 的宿主 nonce 分配器：计数按规范化账本路径、chain_id 和
签名身份隔离，不再让上一个独立测试库的计数串入新库。新增测试先复现
“新库期望 0、实际 2”，修复后覆盖两库交错分配、持久 floor、路径别名与不同签名者。
初始 nonce 负例保留：nonce=1 拒绝顺序错误，u64::MAX 拒绝序列耗尽；
只纠正错误原因断言，生产端仍要求用户自行提供签名和 nonce。

| 门禁 | 本次实测 |
| --- | --- |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.49 秒） |
| 重组及 RLPx 读取专项 | 每次 11 passed，独立进程连续三次通过 |
| nonce 分配隔离与初始 nonce 负例 | 2 passed / 0 failed |
| novovm-node 全库 | 714 passed / 41 failed / 7 ignored（607.30 秒） |
| 上述 41 项失败逐项独立复跑 | 11 passed / 30 failed |
| network/node lib/tests 严格 Clippy、格式及 diff | PASS |

网络忽略项是显式 live mainnet peer smoke，不伪称已经测试公网对等节点。
节点测试总数因新增用例增加 1；失败从前轮 54 降到 41，不能据此宣布全库通过。
完整节点运行前后源码摘要相同；所有 socket 测试仍在隔离网络 namespace，
不修改宿主服务或其他仓库。

剩余节点失败包括：旧夹具无余额存款却期待成功、Ed25519 直接签名与旧 UCA
算法断言不一致、AOEM semantic delta 数量预期、未最终化材料的 canonical 预期、
旧非零初始 nonce，以及前序 panic 导致的测试锁 poisoned。
独立复跑仍失败的 30 项必须继续逐项核验，不能统称为环境污染；不允许通过
放宽余额守恒、认证、nonce 顺序或最终性来消除失败。

证据：`artifacts/canonical-regression-b8aad0a/acceptance.json`，同目录包含
`network-all.log`、`network-focus-*.log`、`nonce-red.log`、`nonce-green.log`、
`node-all.log`、`node-all-run.json`、`node-isolated-results.json` 与源码/binary SHA256。
旧 `artifacts/local-install-09d1bfe/runtime/` 未替换。本机全量、实体多机、正式 clean
release 和主网上线仍未签收；下一步先处理余额资金夹具与签名/执行元数据回归。

## Linux RLPx 读循环恢复加固（2026-09-30 UTC）

状态：`RLPX READ RECOVERY VERIFIED / FULL LOCAL GATE BLOCKED`。
开发基线为已提交并推送到 `main` 的 `b8aad0a`；本节是其后的本机回归，
未替换旧安装包、部署多机或批准主网上线。

定位到此前网络库 13 项失败中的共同读循环问题：Linux socket 的空闲读取返回
`WouldBlock`（EAGAIN），错误文本是 `Resource temporarily unavailable`。
原实现依赖英文/Windows 超时文本，误将空闲连接按解码失败关闭，导致对端 EOF，
以及缺少后续 headers/bodies/receipts/snap/交易同步请求。

本次只改 RLPx 读错误分类及会话恢复边界：
- 按 `std::io::ErrorKind` 识别 `WouldBlock`、`TimedOut`、`Interrupted`，
  不再用错误文案决定是否重试读取；已有部分读取仍受原 deadline 约束。
- 只有尚未消费新帧 header 的空闲超时可保留会话，并继续请求调度；
  已消费帧片段后的超时必须清理连接及密码流状态，不能将下一字节当新帧。
- EOF、MAC/解码失败及既有 pending request deadline 不放宽；不改 NOVORUDP
  wire、APFL、AOEM、交易准入、余额、nonce、签名或 canonical 信任边界。

新增 9 项回归：脚本化读取器覆盖 Linux EAGAIN、非英文文案、Interrupted、
致命错误文案与错误类型不一致；真实回环 TCP 覆盖空闲后继续解码 ping/pong、
header/MAC/body 间超时断连和请求超时仍有效。连同原 partial deadline 测试共
10 项通过，另连续复跑三次通过，重复次数不增加覆盖计数。
修改生产代码前，新读取器测试 6 项均失败，保留 red 日志，未删除既有断言。

| 门禁 | 本次实测 |
| --- | --- |
| RLPx 读取专项 | 10 passed / 0 failed（其中新增 9 项） |
| novovm-network 全部库测试 | 486 passed / 1 failed / 1 ignored（18.31 秒） |
| 原 13 项网络失败独立复跑 | 12 passed / 1 failed |
| 节点共识/账本/准入/恢复核心组 | 200 passed / 0 failed（86.05 秒） |
| network lib/tests 严格 Clippy、格式及 diff 检查 | PASS |

测试使用隔离网络 namespace 的 loopback，不依赖实体 B/R1/R2，不修改宿主服务。
节点全库本次未重跑，不能把 200 项专项替代前轮 54 项失败的全库结果。

网络唯一剩余失败为
`evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3`：
`reorg_count` 期望 1、实测 0。该旧用例期待收到网络 header/body 后自动更新
canonical；当前入站材料保持 `canonical=false / safe=false / finalized=false`。
需要单独补齐经过验证的 fork-choice 驱动和匹配夹具，不得通过信任任意远端 header
或删除 reorg 断言来刷绿。此失败早于本次修复，见
`NOVOVM_NATIVE_SEAL_SERVICE_V1.md` 的 2026-09-26 基线记录。

证据：`artifacts/rlpx-read-hardening-b8aad0a/acceptance.json`，同目录保留
`red.log`、`read-regression.log`、`repeat-*.log`、`network-all.log`、
`network-isolated-results.json`、`node-core.log` 及构建/Clippy 日志。
全量本机验收仍不通过，继续单机修复；不签多机、正式 clean release 或生产 PASS。

## 单机加固与扩大回归（2026-09-29，尚未通过全量门禁）

状态：`LOCAL HARDENING VERIFIED / FULL LOCAL GATE BLOCKED`。
本开发检查点基于 `09d1bfe`，包含用户准入、历史追赶与单机恢复加固。
提交到 `main` 仅保存开发进度，不代表全量门禁或生产验收通过；未多机部署或正式发币。

本轮补齐两处防护：
- V3 同轮决议消息携带的完整 QC 可以补回丢失的 prepare 流量，仍走完整签名、
  本地执行、NewView、timeout 与安全锁验证；只在 poll 持久化/签名。
  真实回环 WSS 分别仅投递 DecisionVoteV3 / DecisionCertificateV3，验证恢复、
  坏签名拒绝及重启不重签。没有声称解决跨轮次分票。
- FreshChainLifecycleV1 限制块时间最多领先本机 30 秒；后继块体在执行前拒绝
  过远未来时间。已有候选/父块在时钟回退时等待校时，不继续签名/提案。
  检查边界值、整数极值、未来块不创建工作区，以及恢复后原候选继续。

核心专项 212 个不同 Rust 测试通过：共识/账本/准入等 200、AOEM 创世/晋升恢复 3、
CLI 常规 5、连续高度进程 1、其余此前 ignored 的主进程场景 3。
后四项进程测试均显式执行，不能将常规 CLI 的 `4 ignored` 另算通过。
连续进程专项 304.83 秒，晋升检查点组 299.76 秒；包含持久 RPC 入队后强杀、
高度 3–5 连续最终化、原 proposer 离线时逐高度追赶及重启，未绕过 AOEM 执行。
新验收 runner 自测 6 项、打包构建失败保护、严格 Clippy、格式检查通过。
release 主节点已重建，协议承诺查询成功；旧便携安装包未替换。

扩大回归不通过，不能用以上专项遮盖：

| 门禁 | 实测结果 |
| --- | --- |
| novovm-node 全部库测试 | 700 passed / 54 failed / 7 ignored（603.14 秒） |
| novovm-network 全部库测试 | 465 passed / 13 failed / 1 ignored |
| 节点失败用例逐项独立复跑 | 23 转为通过，31 仍失败 |
| 网络失败用例逐项独立复跑 | 13 仍失败 |

节点失败涉及旧查询入口 nonce、费用/执行回执、mapped asset 与账户测试；
其中顺序污染可见 expected nonce=0 而提交 2/3/4，以及 panic 后测试锁 poisoned。
但 31 项独立失败不能归因于顺序污染。网络失败集中在 ETH RLPx peer worker，
独立复跑仍出现握手后 EOF / 缺少预期同步请求，尚未归因，不修改无关协议来刷绿。

新脚本 `scripts/novovm-local-readiness.py` 可重跑 Linux 本机门禁：隔离网络空间、
从 Cargo 构建消息定位二进制、记录源码及 binary SHA256、拒绝零测试和非零退出。
实际完整调用在 node 库失败时以退出码 1 停止；不会将后续未执行阶段记为通过。
测试前后源码摘要相同，`accepted=false`、`production_ready=false`。
网络及 CLI 的上述独立结果在另一个汇总中保留，不伪造成该失败 runner 已执行。

证据入口：
- `artifacts/local-hardening-09d1bfe/acceptance.json`
- `artifacts/local-hardening-runner-09d1bfe/acceptance.json`
- `artifacts/audit/candidate-node-processes/seal-relay-3128940-1790724792838959118/continuous-acceptance.json`
- `artifacts/audit/promotion-process-kill/3082274-1790724764930153446/`

下一阶段仍留在单机：上述失败定位修复、无候选时 leader 离线的 pacemaker/NewView、
跨轮 prepare 分票活性、长时间 soak、磁盘故障/备份恢复、公网网关均未完成。
正式 clean release 仍需独立验收；禁止绕过 clean-worktree 打包检查。
这些项目未闭环前，不切实体多机验收，也不签生产上线。

## 已确认的生产创世边界（2026-09-29）

用户已确认：**生产链从全新创世块启动，不继承测试账本。**
生产初始化采用独立存储与 AOEM 状态命名空间，不迁移测试余额、nonce、
历史块、回执或确认凭证；现有测试数据保留，不授权删除或覆盖。
各生产节点必须核验同一份明确批准的创世配置及哈希，而不是各自生成不同创世。
此决定排除测试链升级/检查点迁移，不替代正式分配、验证集合及密钥配置的确认。
初始化、首块/后继晋升及部分跨库恢复已有实现和本机证据，详见当前结论；
故障覆盖、实体多机及正式配置仍未完成，不能据此标记生产就绪。
实现约束见 `NOVOVM_ISOLATED_PROMOTION_PROTOCOL_V1.md` 的 ancestry/trust-root 部分。

## 当前结论

### 本轮：用户准入、历史追赶与进程恢复

本开发检查点包含基于 `09d1bfe` 的以下变更；既有安装包不包含这些新接口。
实现及操作边界见 `NOVOVM_FRESH_CHAIN_INGRESS_RECOVERY_V1.txt`。

- 新增默认不监听的回环 JSON-RPC：`nov_sendRawTransaction`、
  `nov_getTransactionStatus`、`nov_chainStatus`。用户签名不要求验证者身份。
- 显式 successor 模式下使用创世绑定 RocksDB 交易队列；WAL 同步后才返回 queued，
  去重、nonce 冲突和容量上限可核验。由已验证最终父状态清理已确认条目。
- 非 leader 的入队交易向验证者有界传播；选块时重新验证父状态及 nonce。
- 历史请求绑定 authority/创世/高度；任一持有归档的验证者可发送原 prepare QC、
  DecisionCertificateV3 和块体。接收者逐高度独立执行，不放宽签名或执行验证。

首轮进程测试揭示只补最终证书无法满足原 prepare 门槛；保留验证规则补齐 QC 后，
专项先通过（306.81 秒），最终代码复验 1 passed（308.89 秒）。最新证据：
`artifacts/audit/candidate-node-processes/seal-relay-2590344-1790723312222864970/continuous-acceptance.json`。
包含非 leader RPC 入队后被杀、重启后不重提仍确认、连续高度 3–5、原历史
proposer 离线时落后三高度追赶，以及追上后的强杀恢复。最终进程 PID 为
2882417、2882418、2882419，入队后被终止的进程 PID 为 2878403。

最终本机验收共 205 个不同测试通过，重复运行不另计：
- 共识/账本/创世/守恒/准入等 196 passed（87.88 秒），含 HTTP 慢连接、
  pool 身份丢失、错签名、容量、重启与去重检查。
- AOEM 新创世与恢复组 3 passed（282.09 秒），含 authority/ledger/finality
  三检查点进程强杀。证据目录：
  `artifacts/audit/promotion-process-kill/2547631-1790723282013556601/`。
- 普通主节点 CLI 5 passed / 4 ignored（12.16 秒）；其中连续进程专项已另外
  显式运行 1 passed（308.89 秒），其余 ignored 不计为通过。
- 严格 Clippy、格式、diff 及 CI YAML 语法检查通过；远程 CI 不在本机计数中。

验收摘要与日志：`artifacts/fresh-ingress-history-09d1bfe/acceptance.json`。
中途失败日志保留：历史响应补齐原 prepare QC；旧测试中“future nonce 丢弃”
和“确认期间拒收”断言改为持久排队，同时检查候选、工作区和持久 outbox 不变。
没有通过删除签名、nonce 顺序、执行或最终性验证来放宽验收。

该轮当时仍有代码级上线阻断：无候选时仅 round-0 leader 自动提案，prepare 后分票
活性及块时间未来偏差约束尚需闭环；时间防护的后续进展见本文件顶部。
该轮不承诺 epoch 变更、无限历史快速同步、
物理坏盘恢复或安全公网 API。远程接入必须另配受控 HTTPS 网关并验收。
新增 CI 显式执行进程专项；本机 GitHub CLI 未登录，远程 CI 尚未核验。

### 先前基线证据（本轮实现之前）

本轮 Linux 验收以 `09d1bfe` 为源码基线：官方脚本构建七个 release 程序，
Linux FULLMAX AOEM 核心 SHA-256 与 SDK 清单匹配，安装包 58 个文件校验通过。
安装目录 `artifacts/local-install-09d1bfe/runtime/` 的主节点配置承诺模式与 AOEM
FFI 实际加载通过；未注册常驻服务。前轮 32 项 Rust 测试及打包失败保护测试通过，
本轮新增共识/账本/创世/余额守恒/nonce/协议 pin 门禁 190 passed（80.35 秒）。
记录在 `artifacts/production-acceptance-09d1bfe/core-gates.log`。
传输持久化、recipient ACK、relay 边界和证据校验另有 42 passed（6.49 秒），
记录在同目录 `transport-durability-gates.log`；这两组加连续高度专项共 233 项，
重复运行不另计为新增覆盖。

新增显式 ignored 的连续高度主进程测试，复用已有第一、第二高度真实执行准备。
三个独立验证进程在同一生命周期内从高度 2 连续确认 3、4、5，由剩余一个测试
transport 身份按当前 leader 提交签名交易，该身份的验证进程保持关闭。
逐高度等待三个节点各自 AOEM/账本发布及同一决议，末高度后强制终止并从原配置
重启；核对完整历史最终性证明、末块交易体一致且前两高度证明未改变。
首次专项 1 passed（218.77 秒），证据：
`artifacts/audit/candidate-node-processes/seal-relay-487791-1790719473860146443/continuous-acceptance.json`。
补充进程 PID 证据后，最终版本复验 1 passed（218.28 秒），证据：
`artifacts/audit/candidate-node-processes/seal-relay-755907-1790719706059628974/continuous-acceptance.json`。
三个连续运行 PID 为 943609、943610、943611；重启后完整历史保持一致。
专项严格 Clippy、格式和 diff 检查通过。
这证明有界的本机连续三次后继推进，不代表长跑、所有 leader 故障场景、
落后三个高度的第四节点追赶、普通钱包 RPC 准入或真实多机验收。
为避免遗漏，该用例必须显式 `--ignored` 执行；普通 CLI 测试不会自动包含它。

上述先前基线尚无普通用户持久准入和多高度追赶；本轮实现和证据见本节顶部，
不得以旧安装包或旧 CI 结果代替本轮发布验证。

晋升检查点新增真正的进程终止验收：AOEM authority 提交后、ledger 提交后、
finality 提交后，各在测试专用回调内保持协调器及锁，由父测试 kill 活子进程，
确认非成功退出；另一个新进程使用相同存储和原始证明恢复。三个检查点均通过，
恢复后完整块/最终性证明与原件相同，两次 resume 返回一致且 AOEM 读回验证通过。
专项 1 passed（含三个检查点，18.75 秒）；提取的原独立存储一致性测试另跑
1 passed（23.89 秒），lib/tests 严格 Clippy、格式及 diff 检查通过。
证据目录：`artifacts/audit/promotion-process-kill/23484-1790714643047544700/`，
包含各检查点、被终止/恢复 PID、日志及 recovered.json。仅测试代码使用检查点，
未新增生产故障开关。共识签名为本地测试夹具，恢复调用内部协调器；不替代
主节点在写入途中被杀的组网测试、提交调用内部撕裂写入或物理断电验收。

第二高度补入真实强制终止：等待三个测试主进程都报告第二块持久最终化后，
先确认进程仍在运行，仅通过本测试持有的子进程句柄执行 kill，并要求非成功
退出且未输出正常终止 summary。随后从原配置重启四节点，检查原决议不变、
第四节点追上及四份完整区块/交易体/最终性持久读回一致。专项 1 passed
（123.33 秒），Clippy、格式与 diff 检查通过。证据：
`artifacts/audit/candidate-node-processes/seal-relay-24076-1790714248426878400/successor-acceptance.json`。
这是最终化完成后的非正常进程终止恢复；不是提交中断点 kill、磁盘故障、
物理断电或实体多机验收。未关闭系统服务或按进程名批量终止进程。

独立主进程验收现覆盖第二高度：第一块已最终化后，三个独立 AOEM/账本进程
接收第四个测试身份发送的 nonce=1 签名交易，由第二高度 leader 自动组块，
其余节点从真实 WSS 获取块体、分别执行并形成相同 3/4 V3 决议。随后四进程
从原高度一配置重启，第四节点获取块体并执行追上。四份持久账本读回的完整
第二块、原始交易和最终性证据逐项相同；并非复制预执行的第二块工作区。
专项 1 passed（193.99 秒为测试总耗时），严格测试目标 Clippy、格式检查通过。
证据：`artifacts/audit/candidate-node-processes/seal-relay-23188-1790713927215434900/successor-acceptance.json`。
仅本机回环 WSS、两高度、正常退出/重启、固定测试交易与验证集合。第四身份在
发送阶段不运行验证进程，不存在同密钥并行运行；后续恢复为验证节点。
这不替代实体多机、Linux 安装、硬断电、长跑、通用 RPC 准入或任意历史追赶。

Windows debug 实机四进程复验发现 `native_fresh_genesis_prepare` 在主线程栈溢出，
发生在组网之前。准备入口现使用与确认循环相同的 8 MiB 显式 joined 线程，
不修改 AOEM、全局栈参数或任何验证门禁。修复后真实主进程新创世专项通过
（1 passed，34.45 秒）：四套独立执行/存储、2/4 不确认、3/4 确认并发布、
重启凭证不变、第四节点从归档追上。证据：
`artifacts/audit/candidate-node-processes/seal-relay-22044-1790713255868993300/acceptance.json`。
范围仅第一高度、本机 WSS、正常进程退出/重启，不是连续多高度、硬断电、
实体 LAN 或 Linux 安装验收。普通 CLI 回归 5 passed / 3 ignored（7.06 秒）；
上述新创世专项已另行显式执行，另外两项 ignored 不计通过。Clippy、格式通过。

接续模式启动恢复已接入主节点：以配置中的固定祖先核验最终链，从本地既有
round-driver 所有者绑定定位唯一的下一高度候选，核对账本、完整执行输出及
工作区承诺，再进入原有实时验证/签名服务。不重新选择分支、生成候选或修改
配置。晋升中断时必须有本地完整 V3 决议才允许继续发布；缺凭证、损坏输出
仍拒绝，不回退旧候选签名。关闭接续模式时原严格启动语义保持不变。
本地新创世回归 17 passed（227.73 秒为整组测试耗时），覆盖签名前及签名后
重开、块哈希/工作区/待发送签名记录不变、输出损坏拒绝，以及账本提交后晋升
中断的有/无完整凭证分支。轮次驱动 16 passed（11.18 秒），严格 Clippy、
格式与补丁检查通过。这是本机对象/数据库重开和注入故障，不是独立进程硬断电
或实体多机连续出块验收；也尚未覆盖候选登记后、所有者绑定写入前的崩溃窗口。
远程 CI [36620332992](https://github.com/novovm/supervm/actions/runs/36620332992)
已完成 success，仅覆盖 `f417da9`；不覆盖随后交易体发送、自动提案及本轮恢复。

新增默认关闭的 `propose_successors`（必须同时开启 `receive_successors` 和
`follow_finalized_tip`）：最终化后的 round-0 leader 可从 NativeTransaction
认证传输暂存队列选择最多 16 笔有序交易，以最终父块快照校验签名、链域、
nonce 及块体边界，随后调用已有 AOEM 候选执行。收包不等于交易池接纳，不发
持久 ACK；候选准备后下一 tick 才通过原有 V3 服务签名和广播。逐来源沿用配置
的每秒处理预算，错误输入计入预算；不使用旧宿主投影或旧 pending 执行器。
这是活跃进程内的受限自动提案入口，非 leader 不转发交易、确认期间不缓存
新交易、没有持久交易池。按有交易触发，slot 为父块加一，时间取本地墙钟与
父块时间的较大值，不产生空块，也不保证 250ms 出块。未确认候选的精确
提案恢复见上文；已有防双签锁仍拒绝冲突输入，不能据此宣称连续生产已验收。
发布前还必须核实协议级未来时间约束：本入口的墙钟取值不等于其他验证者已
执行时钟偏差限制，也不能仅凭签名/执行根有效就声称块时间可信。
本地新创世回归 17 项通过（214.03 秒为测试总耗时），真实 WSS NativeTransaction
事件只暂存不执行；错误链域与 nonce 被拒绝并消耗来源预算，预算耗尽时不组块，
下一时间窗重试合法事件后生成与既有候选相同的块哈希，下一 tick 才启动三路
交易体发送。确认期间再次提交事件被拒绝，不伪造接纳 ACK。此用例复用已执行
候选夹具，并注入本地时钟推进限流窗口，不是独立多机全新执行或自动 QC 签收。

接续模式的交易体发送接入原有 V3 调度：本地 proposer 完成签名后，使用已验证
候选的原始交易体向其余绑定验证节点分片发送；轮次/提案变化时替换发送状态，
沿用 5 秒重传。非 proposer 不重新签名或冒充来源。最终化后的无密钥转发器也
可从本地完整决议与执行输出重建相同发送器，继续帮助落后的接收者获取交易体。
此功能受 `receive_successors` 显式接续模式控制；还没有自动交易选择和组块，
也不是跨多个历史高度的追赶协议。发送器仍以各 peer 独立缓存保存分片。
交易体仍要求原 proposer 的认证传输来源；原 proposer 离线时，其他验证者的
凭证转发不能替代该交易体来源，尚不是 proposer 故障下的数据可用性签收。
本地新创世回归 17 项通过（205.42 秒为测试总耗时）：真实 WSS 上三个接收者
均重组出与候选完全一致的原始交易体，覆盖确认服务自动发送及最终化后凭证
转发器恢复发送；未用手动构造的发送器替代这两条发送路径。既有 2/4 拒绝、
3/4 确认和第四验证者重启追赶回归仍通过。此处各确认服务共享 AOEM 执行夹具，
不是四台独立执行节点或最大交易体容量验收。Clippy、格式和补丁检查通过。
另轮次驱动回归 16 项通过（10.88 秒），包含换 leader、重启重放、错误来源和
不完整 quorum 拒绝；不据此宣称新增 body 发送已覆盖 proposer 离线场景。

远程 CI `36614284001` 已完成为 success，覆盖提交 `0734eeb`，证明该提交的
候选作废修复通过远程 gate；不覆盖之后的交易体重传、独立存储、启动跟随和
本轮生命周期改动，后续提交仍需重新运行 CI。

主节点新创世循环接入 `FreshChainLifecycleV1`：默认仍为单候选确认后无密钥转发；
显式配置 `receive_successors: true`（同时要求 `follow_finalized_tip: true`）后才
保留签名身份，接收下一高度的签名提案/交易体。每 peer 四帧队列与公平轮询复用
有界 body inbox；收包不执行、不投票。poll 先核验当前最终性，再本地执行并核对
完整签名主题，成功后将原始认证提案事件交给原有 V3 服务，下一调度才可投票。
这接通了跟随者“最终化后接下一候选”的生命周期入口；尚无主节点自动选交易、
leader 组块/交易体广播、跨高度落后节点追赶或独立多机连续出块验收。
Windows debug 的嵌套集成用例在默认测试线程栈上，于接收完成后的本地执行
验证阶段发生栈溢出（堆分配生命周期状态后仍复现）。集成用例改用主节点既有的
8 MiB 专用生命周期线程，并共享同一常量；未提高主节点预算、修改全局测试栈或
降低执行/最终性校验。不能宣称该深调用链已支持默认 2 MiB 测试线程栈。
按上述主节点线程预算，新创世回归 17 项通过（202.47 秒为测试总耗时）：
真实本机 WSS 在第 3 块最终化后接收第 4 块交易体，经过本地输出验证进入 V3
服务；交接时尚未处理提案，下一 poll 才处理它。覆盖接续关闭、关闭时释放
签名配置、损坏传输对象拒绝、接收丢失后重传及时间倒退后停机。此用例复用
本机既有执行夹具，不等于独立多机重执行，也未通过本控制器完成第 4 块 QC。

新增显式 `follow_finalized_tip: true` 启动选项（默认关闭，仅允许新创世 V3）：
主节点在启动确认服务前，以配置中的高度、块哈希、工作区及前驱为固定祖先，
核验持久最终性链和当前 AOEM 输出后，跟随其已最终化后代。原配置文件、密钥、
验证集合及路径保持不变；旧祖先快照已回收时仍必须有完整不可变账本绑定。
错误祖先/前驱、损坏执行输出及尚未完成的后继晋升均拒绝，不自动修复或改换链。
这是重启链头解析，不是自动连续出块；确认服务仍要求对应的本地持久决议，
跨进程重启接续、未完成晋升自动定位及连续高度调度仍须单独验收。
本地新创世回归 17 项通过（188.89 秒，测试总耗时，不是确认延迟），覆盖关闭
选项保留原高度、显式跟随及幂等解析、错误祖先/前驱、未完成晋升和输出损坏
拒绝，以及祖先快照回收后的链头读取。严格 Clippy 和格式检查通过。

新增独立存储一致性验收：两套不同路径的 AOEM 数据库和账本、不同命名空间及
工作区 ID，从相同创世配置各自执行、确认并最终化三个高度。完整区块及 V3
决议逐项相同，各高度关闭/重开执行会话后仍能验证最终性。测试通过（20.62 秒）。
此用例未复制另一套执行状态或结果；验证者签名仍由本地测试夹具模拟，两次运行
在同一进程顺序执行，不是独立进程或四台实体机共识验收。CI 的既有候选执行
筛选会包含此新用例；当前运行中的旧提交尚不覆盖它。

交易体发送端现在按固定节奏重试：每帧至少 20ms，一轮全部入队后等待 5 秒再从
签名提案开始发送，不改变签名或内容标识；背压保留游标，时间倒退拒绝。
调用者必须在等待期间持续 poll，并在生命周期确认不再需要时释放发送器；
返回 true 只表示当前一轮已入队，不是交付或最终性完成。未新增持久 ACK。
验证：新创世 16 项通过（163.64 秒），真实本机 WSS 第一轮事件被测试故意丢弃，
重建接收状态后通过重复发送同一提案/交易体恢复；重试等待用注入单调时间推进，
实际重传仍经过 WSS。另覆盖发送时间倒退拒绝。Clippy、格式及补丁检查通过。
这是接收状态丢失模拟，不是进程断电、最大块体或独立四机验收。

交易体新增 Product Overlay 发送/接收适配：发送端每次最多入队一帧，背压不推进
游标；接收端绑定验证集合、链和高度，限制 4 个候选及每个认证来源每秒 64 帧，
30 秒固定重组期限不因重复提案/分片续期。完成或失败后保留有界短期记录，
不重复交付同一体；不发持久化 ACK，不把入队完成解释为对端已接收或执行。
这是收发组件，主节点连续高度调度、超时后的应用重传/追赶和独立执行仍未接通。
验证：新创世回归 16 项通过（163.70 秒），本机真实 WSS 收到签名提案和交易体，
并进入既有本地执行结果比对；覆盖错误发送者运行时、每秒预算、4 候选容量、
固定期限释放及时间倒退拒绝。严格 Clippy、格式和补丁检查通过。此处网络用例
是小交易体，本机共享 AOEM；不等同于跨机独立执行、最大交易体或丢包重传验收。

新增签名提案绑定的交易体分片编解码：沿用现有提案签名、轮次证据和认证 peer
校验，64 KiB 分片不放宽 Overlay 的 192 KiB 消息上限，单次重组受 2 MiB 原始
交易总量与 1024 笔上限约束。完整体必须匹配有序交易根和 body digest。
`prepare_received_successor` 复用本地鉴权/执行读回，并逐字段比较签名执行主题，
匹配后才注册候选；有效签名不能替代执行验证。解析本身不写状态或投票。
目前是编解码与接收准备 API，尚未接入节点收发调度；活跃重组数量/超时、跨节点
交易体分发与各节点独立执行仍待接入验收，不是多机自动出块已完成。
本轮验证：完整 candidate_workspace 回归 19 passed、2 ignored（204.23 秒；
两 worker 均由通过的三进程父用例调用），另 1 项交易体解析边界测试通过。
集成覆盖乱序/重复/冲突分片、错误来源/高度、篡改、完整接收后本地输出比对、
有效签名但错误 state root 拒绝；大交易体夹具只测传输，不宣称交易执行通过。
严格 Clippy、格式和补丁检查通过。本地集成复用 AOEM 结果，不是独立四机重执行。

CI `36609145745`（`24bc078`）已结束为失败，不再是超时：主 gate 的旧候选损坏
后作废用例失败，其他同组 18 项通过、2 个 worker 由父用例调用；后续流水线
报告未生成，上传步骤连带失败。原因是 abort 新增前驱保护后无条件读取输入。
修正仅允许经只读账本检查明确属于 legacy 的损坏候选作废；新创世/缺失/损坏
账本不能据此绕过晋升保护。本机原失败用例通过，远程修正版需重新验收。

后继候选准备已串联为 `NovNativeSealServiceConfigV1::prepare_fresh_successor`：
从当前配置对应的已最终确认块推导下一高度，验证完整交易批次，复用隔离执行、
候选注册和旧快照回收，再返回可供现有 V3 服务打开的新配置。身份、密钥、
验证集合、路径和时限不变；不导出密钥，不改磁盘服务配置，不签名或晋升。
调用者须保留相同 slot、时间戳和有序交易批次以重试，交易不能因准备完成就从
待处理队列永久删除。此 API 尚未接入主节点自动调度，不等同于连续出块上线。
下一接入缺口是有界候选交易体传输、各节点本地重执行，以及重启后的高度接续。
验证：新创世 16 项通过（150.76 秒），配置回归 10 项通过；覆盖错误父块/
前驱、无效交易、未确认子块、过期配置拒绝，精确重试及生成配置直接启动 V3
服务。严格 Clippy、格式和补丁检查通过；仍为本机共享 AOEM 测试。

已实现后继候选创建前的旧执行快照回收：鉴权后核验实时最终性与账本绑定，
保留当前块及直接父块，只回收已最终确认高度上的完整、已注册旧候选。
不可变回收记录先落盘，候选槽位最后释放；中断后可继续，旧 ID 不得复活，
已复用槽位不受旧重试影响。历史块、交易/回执索引和 V3 凭证不删除。
本轮新创世 16 项通过（137.54 秒，总测试耗时，不是出块时间），覆盖回收意图、
部分删除、槽位释放后三处受控中断与重开恢复、槽位复用、父输出损坏拒绝、
1–4 高度凭证保留。其余候选执行回归 8 passed、1 ignored（37.31 秒；worker
由通过的三进程父测试调用）。严格 Clippy 通过；回收尚未做进程硬崩溃/断电测试。
未完成/未注册候选仍占用配额，当前状态快照仍有 8 MiB 上限；自动组块循环、
独立多机与长期容量验收尚未完成，不能据此宣称生产就绪。

CI 超时修正采用测试依赖优化，而不是删除用例或放宽截止时间：仅将 test profile
中的 curve25519-dalek、ed25519-dalek、sha2 优化级别设为 3，业务代码仍按调试
构建，显式保留 debug assertions 与 overflow checks；不改变发布构建或 AOEM。
同机同代码的 16 项新创世/连续高度回归从 713.27 秒降到 121.27 秒，全部通过；
152 项 seal 回归全部通过（35.91 秒），其余候选执行/恢复 8 passed、1 ignored
（36.84 秒；ignored worker 已由通过的三进程父用例调用）。增量重编译 41.58 秒。
这证明本地验证耗时改善，不是 TPS 或远程 CI 已通过的证明；远程需重新验收。

后继块确认/落账已移除第二高度特例：按高度原子归档完整晋升与 V3 凭证，
新晋升只能紧接已最终确认的父块，历史区块、交易/回执索引与凭证不可覆盖。
签名、轮次、服务配置和 NVP2 状态晋升复用同一后继流程；历史查询新增
`load_fresh_finality_by_height_v1`。缺失历史或不连续高度拒绝继续，不自动修补。
新创世回归 16 passed / 0 failed（713.27 秒）：同组持久签名库依次确认并最终化
第三、第四块，第三高度使用四服务本机 WSS，第四高度验证重启凭证 relay；
覆盖 2/4 拒绝、索引提交后响应丢失恢复、精确重试、1–4 高度凭证保留及删除
第二高度归档后拒绝后续读取/父状态捕获。Clippy、格式与补丁检查通过。
这是共享真实 AOEM 的本机受控测试，不是独立四机或进程硬崩溃验收。
节点仍需显式指定候选，自动组块循环未接入；工作区最多 32 个候选、单个输入/
输出最多 8 MiB、各自累计最多 64 MiB；旧快照回收进展见上段，运行容量仍未验收。
新增 finalized-by-height 标记拒绝旧读者，不迁移或删除现有测试账本；生产从新创世启动。

CI `36601293073`（旧基线 `14d87b6`）已失败：候选执行/多进程恢复步骤超过
30 分钟，前四个用例通过，未出现断言失败报告；后续流水线报告不存在导致上传
连带失败。不能视为通过，也不代表本轮提交通过。测试执行耗时需要处理后再验收。

父状态读取已支持已最终确认的第二块：保持工作区/权威锁，验证完整父子账本及
当前 AOEM 输出后生成只读快照；后继计划、快照和签名主题改用父高度加一。
新创世回归 16 passed（349.07 秒）：第二块未最终确认时拒绝捕获，第三块真实
AOEM 执行及重读/重试通过，旧 nonce/旧父块拒绝，第三块执行不改变第二块权威头，
父证据损坏后拒绝捕获。严格 Clippy、格式与补丁检查通过。
该阶段第三块仅为隔离候选；后续按高度确认与落账进展见上段。
历史证书校验不等于祖先链或实时权威校验，后两者仍由持锁的账本协调器负责。

第二块已接回节点生命周期：启动时从本地 V3 归档恢复，或收齐确认后自动完成
AOEM 发布、账本索引及最终性保存，随后释放签名服务并传播既有凭证。
恢复必须匹配显式账本路径、父子工作区及已锁定的完整凭证；不重新执行交易。
本轮新创世回归 16 passed / 0 failed（335.33 秒），新增真实本机 WSS relay：
其他三个 peer 收到 prepare 与 decision，重开不新增签名 outbox；最终性 pin
丢失后停止且拒绝重开，不自动修补。严格 Clippy（lib + 主节点）、格式检查通过。
这仍是预配置第一/第二高度候选，不是任意高度自动连续出块；真实多机与硬崩溃
尚未验收。下一交付目标为连续出块、查询、重启追赶，不再扩展外围功能。

第二块新增 `finalize_successor_v1`：在实时核验 AOEM 发布及完整父子账本后，原子写入
绑定既有完整 V3 决议的最终性 pin 与独立账本标记。未发布 AOEM 或账本未完成时拒绝；
读回时仍验证签名、父依赖、执行绑定及所有索引，不接受孤立布尔标志。签名块头不改写。
最终性证据查询与实时状态核验分开：这是固定验证集合下的 BFT 最终性，不是 ZK 证明。
第二块节点自动发布/证书 relay 见上段；不代表连续多高度或真实四机生产验收。
验证：新创世 16 passed（305.83 秒），包含 AOEM/账本未完成时拒绝最终性、错误候选
拒绝、最终性提交前/后受控中断及重试、完整证书读回、原签名块头不变、最终性 pin
丢失/篡改拒绝修补、父输出损坏拒绝实时核验。严格 Clippy（lib + 主节点）和格式检查通过。

`complete_successor_ledger_v1` 新增第二块查询投影提交：在真实 AOEM 指针及输出回读
通过后，原子保存块头/体/执行证据、高度/交易/回执/外部批次索引、累计账本头和
完成标记。首块历史索引保持字节一致，累计数量溢出和已有索引冲突拒绝覆盖。
完整清单核验父子两块；已提交索引丢失不得重建，账本完成后 AOEM 回退不得静默修复。
仅完成账本索引仍返回 `finalized=false`；需执行后续最终性步骤，仍未自动接入节点服务。
验证：新创世 16 passed（279.16 秒），包括提交前/后受控中断及重试、两块共三笔
交易/回执索引、累计头高度/数量/状态版本、逐项删除父子索引后拒绝修补、已完成账本
遇到 AOEM 指针回退拒绝改写。严格 Clippy（lib + 主节点）及格式/补丁检查通过。

第二块新增 AOEM 权威发布/只读核验 API：在工作区及状态锁下核对已锁定目标、父子
真实执行输出和首块原始发布证据，只允许从精确首块指针切至精确第二块 `NVP2` 指针。
重复调用不重执行交易；完成指针缺少发布证据时拒绝修补；旧首块 live-parent API
不得在指针切换后继续提供过期的实时父状态。仅调用权威发布不写第二块账本索引；
索引完成状态依据持久回读报告，仍返回 `finalized=false`，未接入自动服务。
验证：新创世 16 passed（246.89 秒），含切换前受控中断、切换成功但响应丢失后的
重开/重复发布、只读回读一致、竞争目标拒绝、旧发布器不得回退、完成后证据缺失拒绝
修补、父执行输出损坏拒绝，以及账本仍只公布首块。严格 Clippy（lib + 主节点）及
格式/补丁检查通过。这些是 checkpoint 失败注入，不等于真实进程断电验收。

第二块新增 `prepare_successor_promotion_v1`：完整验证实时父最终性、真实 AOEM
执行输出、V3 决议和候选绑定后，原子保存唯一晋升目标、校验记录和独立账本标记。
精确重试保留原目标；缺失已提交证据拒绝修补。进入此阶段后拒绝第二块签名、
登记及工作区 abort，防止待落账执行数据被丢弃。首块已发布状态不变。
这是落账前的持久准备阶段，尚未自动接入服务；第二块最终性、服务恢复和真实进程
中断验收仍未完成，不能把该阶段标记 finalized 或生产就绪。后续晋升进展见上段。
验证：新创世 16 passed（232.36 秒），含真实 AOEM/WSS、两票决议拒绝、错误状态根
及候选绑定拒绝、精确重试、签名/登记/abort 阻断、提交后分别丢失 intent/pin 拒绝
修补，以及首块状态/索引/最终性保持不变；严格 Clippy（lib + 主节点）、格式检查通过。

第二块受控 WSS 多服务确认/恢复测试通过（完整真实 AOEM 集成测试 1 passed，
203.26 秒）：仅轮询两名签名者不得确认，三名形成同一 V3 决议；重建全部服务后，
此前未参与轮询的第四名也获取并持久保存相同证书。逐库重新验证证书与第二块哈希，
服务仍报告未 finalized，首块已发布状态/权威指针保持不变。
范围是同进程四个服务、独立签名库、共享真实 AOEM 执行数据，不是四台独立执行节点，
也不是进程断电或公网验收。本轮不修改生产执行逻辑或运行中服务。

第二块现在接入显式配置的节点确认服务：高度 2 必须提供
`finalized_parent_workspace_id`，启动及每次 poll 都重新取得完整父最终性和真实
AOEM 输出验证作用域；错误父块不得创建签名库。首块无此字段，旧配置不变。
主节点第二块只运行确认与证书传播，不调用首块晋升器，仍报告 `finalized=false`。
第二块权威状态晋升、连续多高度及真实多机确认尚未完成；不能据此部署生产。
本轮验证：新创世 16 passed（147.00 秒），服务回归 28 passed（142.13 秒），
严格 Clippy（lib + 主节点）通过。新增受控 WSS 测试验证单个第二块服务签名者的
启动、提案与重启重放、错误父配置拒绝；不是四节点第二块 quorum 验收。
测试发现并修复轮次调度仍限定首高度的遗漏：现在按实时验证作用域匹配高度，
第二块作用域拒绝高度 1/3，不允许仅修改配置高度绕过校验。

第二块的未签名封印 subject 已绑定父块完整 V3 最终决议验证后的稳定 target，
不使用会随签名子集变化的证书哈希。新证明域与首块/旧路径区分；新创世网络 authority
现在按高度验证证明域，legacy authority 仍拒绝它。生成 subject 不是实时签名授权，
第二块服务接线见上段；第二块晋升仍未接通。
本轮新创世 16 passed（100.05 秒），覆盖真实第二块输出、依赖 target、重建一致、
不完整父决议拒绝和旧 authority 拒绝；严格 Clippy、格式和补丁检查通过。

`register_finalized_successor_v1` 已增加高度 2 的持久候选登记：在工作区、权威和账本锁
保护下重新核验首块真实 AOEM 发布、完整 V3 最终性与第二块执行输出，原子保存候选、
执行绑定、按高度及父块索引；精确重复登记保留原记录。不改权威指针、交易查询索引或
最终性标志。完整清单逐项验证新记录；旧版本遇到新增键会拒绝读取，不能降级绕过。
这是本地候选登记 API，不是主节点第二块投票/晋升接线，也未实现候选清理。
登记回归：新创世 16 passed（109.47 秒），覆盖重新打开后的精确重复登记、错误
创世/父高度拒绝、登记记录丢失后拒绝自动修补、同高度两个候选均未选中、父最终性
缺失时拒绝登记，以及首块权威指针和已发布查询结果不变。严格 Clippy/格式检查通过。

第二块本地签名范围已接通：`with_verified_finalized_successor_v1` 在工作区、权威、
账本锁内核验实时父块与已登记执行输出，绑定父 V3 稳定决议 target，复用原准备票、
决议票、安全锁与持久签名历史。测试复用首块的四个独立签名库：2/4 拒绝，3/4
准备 QC 与 V3 决议成立，重开后原签名和决议读回一致，同高度冲突候选拒签；未登记、
记录缺失、父最终性缺失、错误依赖和未获准换轮均拒绝。新创世 16 passed（132.04 秒），
旧路径父 QC 必需性回归 1 passed（0.44 秒），严格 Clippy/格式检查通过。
范围是**共享一个真实 AOEM 权威的本地多签名库**，不是四个独立节点或网络验收；
没有自动发布第二块权威状态，也未设置第二块 safe/finalized。下一步为网络生命周期接线与晋升。

第二块 wire/quarantine/collector 已验证可复用首块 epoch authority、隔离队列和导入库：
提议编解码及实时执行核对、错误高度/来源拒绝、重复票不增权、2/4 不形成决议、3/4
形成决议消息、重开后身份承诺不变。新创世 16 passed（138.08 秒），Overlay 基础回归
5 passed（3.45 秒），严格 Clippy/格式检查通过。来源身份由 fixture 提供，**未走真实
socket、未接主节点第二块服务**；不能标记局域网/公网或自动连续出块通过。

已修复首块多交易阻断：首块状态版本是从零递增的交易序列，必须精确等于
`tx_count`，不是固定为 1，也不是区块高度。候选登记和封印 subject 使用相同约束。
真实 AOEM 测试已覆盖首块 2 笔交易的完整 V3 最终性及第二块第 3 笔交易的隔离执行、
重开读回和 nonce 连续性：新创世 16 passed（99.38 秒），严格 Clippy 与格式检查通过。
第二块本身尚未取得多节点确认；不能把上述测试称为连续高度最终性通过。
该基线 `4b49890` 的封印完整回归已完成：120 passed（946.61 秒），包含受控真实
WSS、换轮、重启和凭证缺失检查；不是实体多机或公网验收，也不覆盖其后的代码。

连续高度前置边界已补：`load_finalized_genesis_parent_v1` 在工作区/权威锁内核验
首块最终性、账本索引和实际 AOEM 输出，返回不可由外部反序列化伪造的只读父快照。
`successor_plan` 从真实父批次、状态根、回执根生成下一高度输入，并按父 nonce
强制验证签名与身份；旧 nonce、跳号、错误父块和跳高度均拒绝，不修改首块状态。
第二块隔离执行已接入 `create_from_finalized_genesis_v1` + `execute_v1`：
保存独立的证明绑定父状态快照，重读时核验完整父块决议、状态根、回执根和 nonce 域，
不伪造旧生产 envelope，也不改写首块权威指针。历史结果查询不等于实时签名授权。
**第二块节点生命周期接线、确认和晋升尚未完成**；新入口目前只支持高度 1 → 2，
不能宣称任意连续高度最终性或生产可部署。
本轮验证：新创世测试 16 passed（99.67 秒），包含第二块真实执行、完整重开读回、
精确幂等重试、祖先回执保持、储备本金与回执手续费核算、首块权威指针保持，以及
实时最终性缺失时拒绝重新准入但允许读取历史隔离结果；lib/tests/bin 严格 Clippy、
格式检查和补丁空白检查通过；该结果不代表连续高度执行或真实多机验收通过。
共用隔离输入落盘逻辑的中断恢复回归通过（1 passed，24.75 秒）。

首块最终性现使用独立的持久证明记录，不改写原始块头：核验创世验证集、调度
Leader 的提议、完整 V3 决议及换轮依赖，并绑定唯一提交意图、AOEM 读回与完整账本索引。
节点发布后自动保存此记录；只读接口 `load_fresh_genesis_finality_v1` 可重开核验凭证，
实时 AOEM 校验接口同时返回 `finalized`。证据缺失或损坏拒绝修复、拒绝报告成功。
此处是**高度 1 的 BFT 最终性**，安全依赖既定验证集与 BFT 故障权重假设，
不是零知识执行证明、连续高度最终性或主网上线签收；竞争候选清理仍待接入。
验证：新创世 16 passed（94.86 秒）、账本 18 passed（0.91 秒）、四真实主进程
首块确认/最终性/重启/晚加入测试通过（119.10 秒），lib/tests/bin 严格 Clippy 通过。
证据：`artifacts/audit/candidate-node-processes/seal-relay-22484-1790693317220397200/acceptance.json`。
实体局域网、公网和进程强杀验证仍未执行；当前 CI 仍是旧基线，不覆盖本轮。

首块节点接线已加入：显式新创世服务取得本地持久 V3 确认凭证后，自动推进
提交意图、AOEM 权威发布和原子账本落账；启动时按同一凭证恢复未完成提交。
落账后退出签票逻辑，只重传已验证的 prepare QC 和 V3 确认凭证，使慢节点仍可追上。
重传路径不持有签名密钥、不执行交易，每次发送前重新核验本地已发布状态。
本节不声明最终性/连续出块完成；相关真实主进程验证结果见后续本轮记录。
真实四主进程专项通过（105.77 秒）：2/4 不确认，3/4 自动发布 AOEM 与账本，
重启保持相同凭证和区块，第四节点晚加入后从只读重传节点补齐并落账。
普通 CLI 回归 5 passed / 3 ignored（43.30 秒），其中 fresh 四主进程测试已另行显式运行。
证据：`artifacts/audit/candidate-node-processes/seal-relay-11840-1790692635453016300/acceptance.json`。
这仍是本机独立进程/WSS，不是实体局域网四机、公网或进程强杀测试。
最终库回归新创世 16 passed（81.23 秒），lib/tests/bin 严格 Clippy 与格式检查通过。

已接通 `complete_genesis_promotion_v1`：在同一工作区/权威锁内核验 AOEM 发布结果，
再一次同步原子写入正式块头、块体、执行证据、高度、交易/回执和 AOEM ID 索引。
发布标记绑定原始提交意图，重试不重复执行、不覆盖或修复已损坏的索引。
显式只读接口可重开并核验完整首块及所有索引；旧入口仍被隔离，签名候选内容不改。
`ledger_publication_completed=true` 仅代表账本发布完成，`finalized=false` 保持诚实：
最终性对外查询、节点自动接线和连续高度尚未完成，不能据此上线。
本轮真实 AOEM 新创世专项 16 passed（77.89 秒）、账本回归 18 passed（0.90 秒），
lib/tests 严格 Clippy、主节点编译检查、格式检查通过。故障注入仍是进程内检查点。
CI `36575803187` 已通过 funded candidate execution/multi-process recovery，正在
执行 funded real-node/restart parity；它仍只覆盖旧基线 `58e5acd`。

已补齐独立的 AOEM 权威指针发布接口：只接受持久提交目标绑定的完整执行输出，
不重跑交易；在同一 AOEM 库内先保存发布证据，再发布 NVP1 权威指针。
精确重试只读回核验，错误目标、损坏指针或输出均拒绝修复；提交结果不确定时
保留权威锁直到进程退出。测试覆盖发布前失败、提交成功后响应丢失、重试和损坏拒绝。
单独调用此发布接口时，不隐式写账本；账本完成接口见上方最新记录。
仍是显式库接口，尚未接入节点自动循环，`finalized=false`。
上述故障注入是进程内检查点，不冒充真实进程强杀或跨库崩溃恢复验收。
当前版本新创世专项 16 passed（69.76 秒），lib/tests 严格 Clippy、格式检查通过。

正式落账事务的第一步已实现（未接入自动节点循环）：重新核验 AOEM 实时创世与
完整候选输出、本地持久 V3 凭证后，原子保存唯一提交目标及独立 pin；重放保持一致。
新 capability marker 在提交未结束前阻止签名/登记和源工作区中止，坏 pin 不自动修复。
真实 AOEM 新创世专项 16 passed（56.55 秒），包含缺凭证、错候选、重开重放、
损坏拒绝、禁止 abort/继续签名及权威头不变。**这还不是状态发布或最终性完成。**
CI `36575803187` 的 `Four-service real AOEM quorum diagnostics` 已实际通过，
全工作流仍在执行后续步骤；该 CI 基线为 `58e5acd`，不覆盖后续提交。

新增显式操作入口 `NOVOVM_NODE_MODE=native_fresh_genesis_prepare`：需要
`NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_PATH`、`NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_COMMITMENT`
以及现有 `NOVOVM_NATIVE_CANDIDATE_PLAN_PATH` / `NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT`。
沿用 AOEM 生产所有权、协议承诺和独立存储配置，校验配置/计划后预约创世、发布
AOEM 创世状态、执行并登记首块；不导入旧 Host 账本、不替用户生成生产经济参数或密钥。
返回 workspace ID 与完整候选，可用于现有 fresh-genesis seal 服务配置。
真实子进程验证空目录准备、退出后重放结果一致、错误 pin 无账本写入、已有 Host
账本保留且拒绝导入。普通 CLI 回归 5 passed / 3 ignored（44.50 秒）。
实际四节点测试另行显式执行：四个独立 AOEM 库、四成员验证集，2/4 无 QC，
3/4 形成相同哈希的 V3 凭证，重启保存凭证与未晋升创世状态（最终复跑 95.52 秒）。测试发现并修复
Windows 主线程栈溢出；仅确认循环使用 8 MiB 栈的单个 joined 生命周期线程，
没有引入 Host 交易并发调度。该证据为本机 WSS/真实主进程，不是实体四机或公网。
**尚未完成：晋升的节点接线、正式 ledger head/索引发布、连续出块和崩溃恢复闭环。**

主程序新创世接线：`native_execution_pipeline` 在显式 fresh-genesis seal 配置下，
完成原有路径隔离检查后直接进入首块确认循环，不再先调用旧 Host/ledger 恢复。
仍由 `open_configured` 与每次 poll 核验实时 AOEM/候选；只接收 NativeSeal，
不执行普通交易、不发交易交付 ACK、不写虚构的 selected head。沿用 tick 次数/间隔，
输出启动、首次确认和结束状态。此分支只服务已经执行登记的首块，不隐式初始化、
晋升状态或连续出块；显式准备命令与主进程测试见上方最新记录。
新创世库测试 16 passed（49.95 秒），主程序编译、lib/bin/tests 严格 Clippy 通过。

最新服务接线：显式配置创世承诺后，服务可为已执行的创世首块启动并签名；
每次轮询重新核验 AOEM 与候选状态，不保存临时签名权限。修复首块轮次错误依赖
已有区块头的问题，仅允许已验证的新创世视图在高度 1 使用高度 0 起点。
新创世专项 16 passed（49.67 秒），包含真实 AOEM、WSS 服务启动、重开和中止后拒签；
服务回归 28 passed（139.65 秒），严格 Clippy、主节点编译通过。
此处不是完整主进程创世启动或最终性晋升完成。
广义 `timeout` 过滤回归 16 passed / 1 failed：
`ledger_final_missing_enqueued_overlap_cannot_timeout_without_admission` 在入池 fixture
报 nonce identity scheme 为空；这是历史失败记录，后续修复及复跑如下。
该失败后续已定位并修复：四处补齐/入池 fixture 创建临时账本却传入空参数，
导致入口读取默认账本。现在显式传入各自的 `native_execution_store_path`，
不放宽任何生产 nonce 校验。`ledger_final_missing` 3 passed，`timeout` 17 passed
（55.69 秒）；主程序实际构建与 lib/tests 严格 Clippy 通过。这些回归与上方
另行执行的新创世主进程验证分开计证。
CI `36571622688` 四服务测试仍失败：40 秒确认期限内一个节点已确认、两个节点
仍 Prepared，未得到全部确认；具体活性/耗时原因待修复，不计为通过。
后续只推进新创世启动、多节点确认、可恢复落账这一最短可用链路。

四服务失败跟进：原测试本机复跑通过（81.77 秒）；将正确性测试预算与既有
WSS gate 对齐为 120 秒收敛 / 300 秒轮次，保留全部 quorum、重启及权威状态
不变断言，不修改生产超时。增加逐轮耗时后再次通过（81.79 秒）：三轮核验
分别 4468 / 7765 / 8521 ms，20.796 秒得到 3/3 确认。Linux CI 尚待复核，
此数据不是生产延迟指标，也不证明真实多机已通过。

最新创世身份基础切片：已验证的首块隔离执行产物提供由完整创世配置推导的
类型化链身份，不使用首块交易候选哈希或本机路径作为身份。真实 AOEM 测试中，
两个竞争首块的块哈希不同、创世身份相同，重开后保持一致；AOEM 权威创世头未改变。
`cargo test -p novovm-node --lib fresh_genesis --locked -- --test-threads=1`
10 passed / 0 failed（20.91 秒）；lib/tests 严格 Clippy 通过。
后续首块登记切片已接通显式准入：在工作区、权威状态、账本锁保护下重新核验 AOEM
创世头和完整执行结果，将竞争首块原子写入候选图及索引，不设置 selected head，
也不发布交易状态。新能力标记继续拦住普通/旧版账本入口；缺失证据、标记降级、
错误链域/根/时间/版本、错误配置 pin、AOEM 创世头丢失和工作区中止均拒绝。
最终专项 13 passed / 0 failed（28.54 秒，包含真实 AOEM 双首块登记/重开）；
完整账本回归 29 passed / 0 failed（1.20 秒）。lib/tests 严格 Clippy、主节点编译、
格式和 diff 检查通过。该登记切片没有修改旧签名规则或实现最终性晋升。
实体多机、公网和长跑验收仍不能由本机测试代替。

后续首块显式签名切片：新增共同创世锚点的独立 proof profile 与签名库绑定；
每次调用在工作区/权威/账本锁内核验实时状态，通过只读临时视图复用原有签名与防双签锁。
真实 AOEM 专项验证四个独立签名数据库：2/4 不成 QC/决策凭证，3/4 可形成，
竞争首块拒绝双签，决策票及完整 V3 凭证重开恢复通过，权威状态与 finalized 标志未改变。
创世专项 14 passed / 0 failed（40.92 秒），账本 29 passed / 0 failed（1.20 秒）；
严格 Clippy、主节点编译与格式检查通过。完整 seal 回归 150 passed / 0 failed
（481.18 秒）；旧隔离候选签名专项 1 passed / 0 failed（22.71 秒）。
这不是四个独立 AOEM 节点或真实网络验收：主节点服务调度、
连续高度、可恢复状态晋升仍待接入。生产启动继续关闭。

新创世 Overlay 接入切片：已实现显式创世配置到验证者/传输身份的 authority 构造，
以及提案、换轮 QC、round wire、V3 收票器统一的新旧 proof profile 匹配检查。
高度 0 的新创世可接收首块候选，旧模式仍拒绝高度 0；不伪造已执行首块。
真实 AOEM 专项覆盖提案编解码、来源身份拒绝、隔离接收区持久化/重开、本地执行匹配、
V3 编解码与 2/4 不成证、3/4 成证；15 passed / 0 failed（43.59 秒）。
来源身份由测试提供，并非真实 socket 或四机验收。严格 Clippy、主节点编译通过；
最终完整 seal 回归 151 passed / 0 failed（479.39 秒），包含最终新增的旧模式
高度 0 拒绝断言。新创世主节点服务配置/调度接线尚未完成。

**NOT READY FOR PRODUCTION。** V3 决策锁、凭证、线格式、收票器和发送器已实现；
显式库级运行循环已在本机真实 WSS 四独立数据库完成定向验证。
现在显式启用的固定候选主节点服务可接管 V3；这不等于默认启用、连续出块或候选已成为最终块。

## 验收路径

以下表格为当前门禁；后面的旧 CI 段落保留为历史问题记录。

| 项目 | 当前状态 | 上线仍需完成 |
| --- | --- | --- |
| Linux 安装及 AOEM runtime | 本机 PASS，见 `artifacts/local-install-09d1bfe/INSTALLATION.txt` | Windows 同版本包与实际部署配置核对 |
| 共识、账本、创世、守恒、防重放门禁 | 本轮 196 项定向回归 PASS，恢复组另 3 项 PASS | 发布版本完整 CI、独立安全审查 |
| 传输持久化、ACK 与 relay 证据 | 当前基线 42 项本机 PASS | 公网中断恢复、故障域及容量验收 |
| 三个连续后继高度 | RPC 用户交易专项本机 PASS，最终复验 308.89 秒 | 长跑、全部 leader 轮换/故障、持久化容量 |
| 强制进程终止恢复 | 交易入队被杀、追赶后强杀及晋升检查点恢复 | 写入中断、磁盘故障、物理断电覆盖 |
| 交易提交与查询 | 签名用户回环 RPC、持久队列、非 leader 传播与最终回执已实现 | 公网 HTTPS 网关、钱包集成、安全与容量验收 |
| 数据可用性及追赶 | 原历史 proposer 离线时三高度追赶专项 PASS | 实体多机、长历史/epoch 变更及故障组合 |
| 共识异常活性 | 安全拒绝和部分换轮已有回归 | prepare 后分票、无候选 leader 离线、网络分区恢复 |
| 正式创世与密钥 | 待运营者理解并批准 | 资产单位、初始分配、验证集合、独立密钥与备份 |
| 实体多机及公网 | 有设备，当前连接信息待采集 | 四台 Windows 与阿里云的同版本实跑证据 |
| 运维与发布 | 未完成 | 监控、备份恢复、升级回退、容量、发布窗口 |

### 历史 CI 与门禁记录

最新已结束远程运行 `36565639881`（基线 `3b1629a`）仍为 FAILURE：此前失败的
WSS 回归已通过，但 `candidate_workspace_execution_configured_services_real_aoem_quorum_and_restart`
在 12:44:18 UTC 报 FAILED。候选测试组随后于 12:53:21 UTC 触及 30 分钟上限，
Rust 默认捕获的失败详情未能在套件结束时输出；不能据此认定只是 runner 慢。
后续 gate 未执行，缺失上传文件是连带失败。须独立执行该用例并保留即时诊断，
定位其真实失败原因；本机首块签名通过不替代此远程阻断项。
CI 已将该用例独立执行并启用 `--nocapture` 与 backtrace，原候选组仅跳过重复项，
覆盖与安全断言不变；这是诊断可见性修复，不是该失败已解决的声明。

| 项目 | 当前状态 | 完成所需证据 |
| --- | --- | --- |
| V3 签名/归档/有界收发 | `0a0a916` 本地完整 seal 回归 147 项通过，Linux CI 待验收 | 同一提交的本地与 Linux CI 测试 |
| 自动服务接管与版本绑定 | 显式 V3 单候选接入、模式绑定、真实 AOEM 主进程三票确认与重启通过 | 隔离候选自动准入、实体多机验收仍需完成 |
| 共同 proposal/body/context 与独立执行核验 | 待现代码逐项追踪与集成验收 | 不同机器收到同一候选，独立执行及根/回执对齐，错误结果拒绝 |
| 新轮签名调度与分区恢复 | prepare 前故障换轮与返回追赶有服务测试；prepare 后决策分票恢复尚未完成 | 不重签冲突决策，分票/分区/恢复后可进展，确定性测试 |
| 最终祖先、链选择、状态晋升 | 未完成 | 验证规则与 ledger/AOEM 可恢复提交；不能直接翻转 finality 字段 |
| 连续高度、重启与追赶 | 待端到端验收 | 多块连续推进、落后节点恢复、旧状态不被错误接受 |
| 版本/创世/验证集合/密钥方案 | 待确认 | 固定发布参数、密钥不复制多开、回滚边界与操作手册 |
| CI 完整通过 | 待稳定提交完成 | 非 cancelled/queued 的成功运行；本次补 seal 测试进 Rust CI |
| AOEM Windows/Linux 发布材料 | 需重新核验 | 同一版本二进制哈希、Linux 安装运行证据；不向 AOEM 注入 NOV 业务 |
| 真实多机/公网/断网/长跑 | NOT EXECUTED（本目标） | 明确机器、版本、日志、实际推进与恢复的可复核报告 |
| 容量/备份/恢复/运维 | 待真实数据测量 | 磁盘增长、峰值资源、快照恢复、密钥保护、告警与回滚演练 |

## 工作纪律

- `canonical_local`、确认凭证与 `finalized` 分别验收；签名法定人数不替代执行正确性。
- 哈希承诺不自动等于完整独立执行有效性证明；保持实际验证模型与文档一致。
- 合成 fixture、同机 WSS、实体 LAN、公网、Linux 安装、长跑分别记录。
- 每个稳定切片提交推送；CI `cancel-in-progress: true` 会取消同分支旧运行，
  因此不能把“触发了很多 CI”当作成功证据，不为修复状态文案反复重启 CI。
- 在完成线上配置接入前，V3 库接口不冒充可部署的完整节点能力。
- 更新结论应引用具体提交/测试结果；未执行项目不得标 PASS。

## 下一接合点的代码事实

- `native_candidate_node_mode.rs` 的显式本地计划入口最终调用
  `run_nov_native_candidate_execution_plan_v1`，推进的是本地未封印执行头，
  不能直接用于不可信网络候选的试执行。
- `native_candidate_workspace.rs` / `native_candidate_execution.rs` 已有隔离输出持久化，
  返回 `authority_state_published=false`；新增 `load_block_artifact_v1` 可从已校验持久输出
  重建完整块。显式 `register_block_candidate_v1` 现可将已重新校验的隔离输出登记到
  候选图，绑定工作区/计划/输出摘要；不写当前状态头、不提供投票资格或权威晋升。
- `with_verified_block_candidate_v1` 可在工作区、权威状态和账本锁保护下重新核验
  完整输出及当前父状态，并提供限于同步回调期间的只读签名视图。普通账本句柄
  仍拒绝隔离候选；父 QC、既有安全锁与持久化签名规则不变。
  自动服务调度和“决策 -> 可恢复权威发布”的生产路径仍须接合。

## 本次 CI 修复证据

- 首块隔离执行接合：显式 `create_from_genesis_v1` 校验预约配置及真实 AOEM 创世镜像，
  从初始状态执行签名交易，不构造虚假的父交易结果。真实引擎专项通过（14.86 秒）；
  后补异常头/错误根/早于创世时间断言随完整候选回归通过：9 passed / 1 ignored，
  295.44 秒。ignored worker 由已通过的三进程父测试调用；旧候选竞争、检查点恢复、
  登记/签名作用域、单服务及四服务确认与重启均通过。严格 Clippy 与主节点编译通过。
  后补首笔交易成功回执及 state_version=1 断言专项通过（17.84 秒）。
  生成高度 1 的持久
  隔离块候选，重开读取/重复执行输出摘要一致，创世权威头与 Host 状态保持不变。
  错误创世 pin 及通过旧入口重放被拒绝；普通账本注册/签名仍拒绝，不宣称 finalized。
  后续仍需创世信任锚、首块签名域和可恢复最终状态晋升，未自动启用节点运行。

- 创世发布与已有状态验证分离：`verify_persisted_v1` 不提交 AOEM 图、不创建归属文件，
  缺库或缺完成头即拒绝，不隐式重放。它仍获取权威协调锁/更新锁诊断并打开通用存储提供器，
  不是文件系统只读 RPC。真实 AOEM 扩展专项通过（7.85 秒），覆盖验证缺库不创建、
  完整状态验证，以及未完成图状态不被验证入口修复；严格 Clippy/格式检查通过。
  首块接合审计发现旧上下文要求高度 1 的父哈希为零、旧候选不允许空交易、
  旧签名域将高度 1 的交易块作为 genesis。须显式接合新的创世信任锚与隔离执行父状态，
  不能将初始分配伪装成已经执行/确认的交易块。当前启动封锁保持。

- `baa124d` 的 Linux CI `36563137201` 结束仍为 144 passed / 2 failed。
  新增诊断提供具体时序：V2 追赶只完成 7 次调度，38.09 秒超时，最慢一轮 12.62 秒；
  V3 换主只完成 6 次调度，32.70 秒超时，最慢一轮 18.87 秒。V2 的三个活动节点
  已形成同一 prepare QC，但尚未完成 commit 收集。证据表明 debug 验签/持久化调度
  与 30 秒阶段预算不匹配；仍需调整正确性夹具时间预算后重验，不能直接宣称协议收敛。
  后续仅将 WSS 正确性夹具阶段预算改为 120 秒、测试轮间隔改为 300 秒，
  使两阶段预算之和小于下一轮间隔；不改变生产默认配置、票数或安全断言。
  本地定向重验：V3 换主/返回 1 passed（66.24 秒），V2 跨轮追赶 1 passed（28.13 秒）。
  Linux 仍待最新提交重验；debug 用例不是生产确认时延或吞吐验收。

- 显式 AOEM 创世初始状态发布已接通，未接自动启动。读取完整预约配置并校验
  运行时协议 pin；拒绝已有 Host JSON/备份/RocksDB 投影和无归属记录的已有 AOEM 库。
  AOEM 通用图写分块、最后写独立 `NVG1` 创世头，完整读回校验，不伪造交易或回执。
  真实 AOEM 专项覆盖首次发布、重复调用、占用数据保留、错误 pin/归属拒绝、
  无完成头的部分写入重放、有完成头但缺块拒绝修复；扩展专项通过（6.51 秒）。
  首跑捕获单值 512 字节限制，改为接口既有分块大小及 152 字节二进制头后通过；
  扩展夹具曾因同时持有数据库句柄误报，改为各阶段释放句柄，并断言具体拒绝原因。
  同步归属文件缺失/损坏仍安全停机，不自动重建。未知提交结果保留权威 OS 锁至进程退出。
  这是同机真实引擎持久化/故障状态测试，不是进程硬杀、断电或主网最终性；
  普通交易入口依然拒绝该创世头及预约账本，尚需创世信任锚、启动恢复与首块接合。

- 完整创世配置归档与只读重建：5 项新增测试随账本回归共 26 passed（0.99 秒）。
  配置全文、原文摘要、计算所得预约、预约摘要及专用版本标记在同一同步批次写入。
  读回重新编译并核对外部创世 pin 与本地命名空间；证据丢失、重算原文摘要后的
  内容篡改、标记降级、旧测试数据占用均拒绝。不自动修复或升级哈希型旧预约。
  重开测试还原了含显式分配的相同初始状态；不存在路径的只读查询不创建目录或库。
  尚未执行进程硬杀/断电，尚未发布 AOEM 初始状态或解除普通执行写入封锁。

- 全新创世配置编译器（纯计算，不写盘）新增 4 项测试通过，连同 5 项预约测试
  共 9 passed（0.29 秒）。配置明确给出链号、时间、协议承诺、初始 NOV 分配/总额、
  验证者公钥及权重；现有验证集合规则计算门槛，不接受调用者自报 quorum。
  重排不改承诺，语义字段改动会改承诺；重复账户/验证者、金额格式/溢出/总额不符、
  未知字段及测试历史注入均拒绝。全新状态使用已有共识编码计算状态根，所有其他
  模块字段保持 fresh defaults，默认值变化会改变承诺。没有导入测试快照。
  编译器可生成匹配显式配置 pin 的预约输入，但未接 CLI/启动、运行时协议 pin 核验、
  AOEM 命名空间空闲检查、真实状态发布或最终性；不构成生产创世完成。

- 创世预约后续锁审计发现：已初始化账本的 `open` 可能在隔离签名作用域持有
  写锁时被调用；`381532c` 的无条件 schema 写锁会造成非重入锁等待。
  改为仅在 schema 缺失时加锁初始化，并在锁内复查；已有 schema 的打开不获取写锁，
  普通写操作仍在锁内检查创世预约禁写标记。新增有界回归（失败也释放锁并 join）
  随 21 项账本测试通过（0.96 秒）。真实 AOEM 单隔离服务专项通过（15.92 秒），
  四隔离服务 2/4 不确认、3/4 同证书及重开专项通过（81.54 秒，同机单进程）。
  严格 Clippy 与格式检查通过。这不解释早于 `381532c` 的 Linux WSS 失败。
- Linux 失败的两项 WSS 用例在本机单独重跑通过：V3 换主/返回 66.16 秒，
  V2 跨轮证书追赶 28.08 秒（含初始化/收尾，并非单阶段耗时）。尚未确认 CI 根因。
  已增加失败阶段、轮询计数、最大单轮耗时和节点状态诊断，延迟接收者的
  WorkerFailed 不再被忽略；没有放宽 30 秒网络阶段预算、签名门槛或安全锁。
  这些本地通过不能将失败的 Linux CI 改记为 PASS。

- 新增账本级全新创世预约：配置承诺、独立摘要 pin 与专用版本标记同步原子落盘。
  只接受空账本或完全一致的预约重试；已有数据不清理，普通入口及旧写句柄拒绝访问。
  5 项专项与 15 项既有账本测试通过（20 passed，0.77 秒），覆盖重开、配置变化、
  并发抢占、丢失/篡改证据、标记降级与测试数据保留。CI 账本过滤器已扩展覆盖新模块。
  这是库级预约，不是完整创世启动；尚无 CLI/RPC/主节点自动启用，没有发布 AOEM
  初始状态，也未验证 AOEM 命名空间空闲、正式分配或最终性。未执行硬断电恢复。

- 真实 AOEM 四配置服务隔离候选测试通过（82.11 秒）：四个节点分别执行相同计划、
  使用独立宿主账本和 AOEM-owned 状态库/命名空间，2/4 不确认，3/4 同证书，
  服务重开后证书验签通过且哈希保持，完整权威状态指纹不变。四服务在同一测试进程，
  使用同一进程的 AOEM 执行运行时与本机 WSS；不是四个主节点进程或实体多机。
  父块 QC 在夹具中预置，子块投票/确认由服务轮询完成，未执行最终状态晋升。
  首跑在最终指纹断言失败：基线早于其他节点初始化，包含进程级缓存变化；将所有
  指纹采集移到四节点初始化之后并保留全部字段比较，重跑通过，未放宽状态不变要求。

- 显式 `isolated_workspace_id` 已接主节点配置与 `open_configured`：仅 V3、精确非零
  ID，复用启动恢复参数，不自动创建或执行工作区。9 项配置测试、主节点编译、
  严格 Clippy 与格式检查通过。真实 AOEM 配置服务专项 1 passed（15.93 秒），
  检查持久化候选提案、重开不新增签名、中止后停机及权威状态不变。
  这是单服务接合测试，四个网络身份仅提供本机 WSS transport fixture；
  不是四个隔离候选服务形成 QC 或主进程部署证据。27 项服务回归通过（93.74 秒）。

- 服务接合入口新增 `open_with_candidate_view` / `poll_with_candidate_view`：
  不保留临时核验权限，同一轮 prepare 与 V3 调度借用同一视图。错误账本即使在
  轮询间隔内也先拒绝并停机（专项 1 passed，0.46 秒）；既有四节点真实 WSS
  V3 确认/重启/模式绑定回归通过（22.72 秒，候选是合成 fixture）。
  主节点编译、严格 Clippy 通过；服务完整回归 26 passed / 0 failed（94.27 秒）。主节点隔离工作区配置接线
  与真实 AOEM 候选服务联调尚未完成，不作为自动隔离候选运行证据。

- 实时签名作用域专项：2 passed / 0 failed（50.48 秒，真实 AOEM）。覆盖
  合法父 QC 下提案/投票、V3 决策票及数据库重开后原票重放、缺父 QC 拒绝、作用域外拒绝、只读限制、
  父状态变化、候选图中止和工作区中止拒绝，且拒绝过程不修改权威状态。
  测试的单验证者集合只验证签名 API 接合，不作为多节点法定人数证据。
  15 项账本回归（0.45 秒）、严格 Clippy 与格式检查通过；本切片完整 seal 回归
  147 passed / 0 failed（481.26 秒）。后补的 V3 专项断言由上述 50.48 秒运行验证。

- 隔离候选登记专项：真实 AOEM 测试通过（16.26 秒），覆盖 observed 升级、
  不改变 AOEM/账本当前头与余额/nonce/索引、旧执行路径提前拒绝、重开与中止拒绝。
  最终能力标记版本的 15 项账本回归通过（0.46 秒），包含绑定 pin 丢失、记录丢失、
  篡改与能力标记降级拒绝；严格 Clippy、格式与 diff 检查通过。
  完整 seal 回归 147 passed / 0 failed（485.36 秒，在最后能力标记改动前构建）；
  候选执行扩展回归 7 passed / 0 failed / 1 ignored（144.79 秒，同样在该改动前）。
  ignored worker 由父进程用例执行；最后能力标记改动已由上述账本与真实 AOEM 专项补验。
  CI 补入完整账本回归，候选登记用例由已有 funded candidate filter 覆盖。

- 隔离输出块重建：增强的真实 AOEM 竞争分支/完整块等价/GC 后重开/损坏与中止测试
  通过（34.71 秒）；14 项账本回归通过；lib/tests Clippy 与格式检查通过。
  `candidate_workspace_execution` 扩展回归结束：6 passed / 0 failed / 1 ignored，
  135.20 秒；ignored 是由已通过的三进程父测试显式调用的 worker。
- 真实候选 CLI：4 passed / 0 failed / 1 ignored，36.22 秒；该次忽略的是需独占
  回环 443 的 prepare 联调，不代表执行过它。
- 后续新增并显式执行 V3 主进程联调：1 passed / 0 failed，196.81 秒。
  四个固定验证者、独立 AOEM 执行与数据库、WSS/TLS/E2E，2/4 无证书，3/4
  确认且重启保持同一凭证与未封印账本。证据位于
  `artifacts/audit/candidate-node-processes/seal-relay-6976-1790636293581648200/acceptance.json`。
  这是同机真实进程，候选预先本地执行，不是自动隔离候选准入、硬崩溃或实体 LAN。

- 后续 V3 服务接入切片 `0a0a916`：三个新增服务用例通过，主节点二进制 `cargo check`、
  lib/tests Clippy `-D warnings`、格式与 diff 检查通过。完整 seal 回归结束：
  **147 passed / 0 failed，484.53 秒**。该结果不替代实体多机或主网最终性验收。
- 基线 `2c3200a` 的 CI 运行 `36492950313` 已完成并成功。
  此结果不代表后续接入提交的 CI 已通过。开发基线 `ef5f1c1` 的
  CI 运行 `36546464916` 已完成并失败：seal 回归 144 passed / 2 failed，失败为
  `native_commit_catchup_real_wss_future_certificate_without_round_adoption` 和
  `native_seal_service_v3_real_wss_failover_and_returning_leader`。
  后续 gate 未执行，因此报告上传也无文件；不能把缺失上传作为唯一失败原因。
  该运行不含创世预约切片，WSS 换轮/追赶失败仍须定位和修复，不豁免发布门禁。

- 本机 `cargo test -p novovm-node --lib native_block_seal --locked -- --test-threads=2`：
  144 passed / 0 failed，437.97 秒；包含四独立数据库真实 loopback WSS 的 V3 确认、
  重启、错误 runtime、倒退时钟与归档证据丢失测试。候选执行事实仍为合成 fixture。
- 本机 `cargo fmt --all -- --check`、Clippy 与 `git diff --check` 通过。

- 旧基线运行 `36490419982` 的 aggregate 测试打印 21 项 PASS 后仍退出 1：
  最后一个预期拒绝的子进程留下了非零 `LASTEXITCODE`。
- 本机用 GitHub 等效包装 `& ./scripts/tests/adaptive-overlay-aggregate.tests.ps1; exit $LASTEXITCODE`
  复现修复前退出 1；添加套件完成后的显式成功退出后，同样 21 项断言通过且退出 0。
  断言失败仍在此前抛出异常，没有降低负向用例要求。
- Rust CI 新增完整 `native_block_seal` 回归；新提交的远程结果仍须独立验收，
  本机通过不能替代 Linux CI 或实体多机测试。

## 2026-10-15: local capability client for shared L3/L4 mailbox

- `MailboxServiceV1::request_local_v1` reuses the remote executor, node-wide
  admission budget, durable store, capabilities, TTL and notifications. It does
  not negotiate a network session or open another SQLite owner. Embedders retain
  responsibility for live device/contact checks around the await.
- Local/remote commit notification, OFF/reopen custody, wrong capability,
  revocation and shared budget checks: 2 new tests PASS. Full mailbox RPC unit
  regression: 13 PASS (`artifacts/l4-local-rpc-regression.log`).
- This enables local L4 consumption; it does not prove Android adaptive routing
  or 300 ms delivery. No financial runtime or deployed node changed.
