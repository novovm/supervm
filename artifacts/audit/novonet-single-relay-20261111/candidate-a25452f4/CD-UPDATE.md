# a25452f4: approved existing updates, then real double relay

## C回传 2026-11-16 01:57：两组对照完成，三边界补采集待Windows确认

BASE四节点退出0，完整status耗时P50 26.90095ms；修正收尾竞态后的附加对照四节点及CDB退出0，P50 14.68135ms。
首轮附加退出记录丢失保留未知；采集组因MASM条件语法错误受控退出0xE0000001，记录0条，失败保留。
and/or条件已在既有DMP离线求值通过，修正后三边界补采集UAC于01:56:56返回取消，节点未启动。
仅剩采集组，BASE和附加对照不重复；采集干扰与首次错误边界均未取得，不能宣称合格或修复。
原库身份及两份失败现场保留，原节点未启动，任务及测试WER已清理，双中继暂停。
详见[C-PTR3-PARTIAL-20261116/README.md](C-PTR3-PARTIAL-20261116/README.md)。


## C回传 2026-11-16 01:36：三边界短时资格测量未启动，UAC返回取消

已准备writer入口1e4cc60、reserve前1e4ccb3、RDX恢复后1e4ccbb三个边界，最多选择一条实际调用。
01:34:53请求Windows UAC，01:36:55返回用户取消，隔离目录和测量目录均未创建，节点未启动。
仅Python编译和PowerShell语法检查完成，运行记录、退出码及干扰测量均缺失，没有首次指针变化证据。
旧四点入口及诊断副本保持冻结，原库、身份和两份失败现场保留，双中继暂停。
详见[C-PTR3-UNRUN-20261116/README.md](C-PTR3-UNRUN-20261116/README.md)。


## C回传 2026-11-16：PID43712实际Formatter和reserve的R14约束

从活动Formatter解析实际vtable，writer确为原RVA1e4cc60，当前flags69000020、width2，输入长度2。
按当前值重建pad无需填充，prefix helper不发生嵌套writer调用；该推导没有冒充历史分支轨迹。
reserve、grow以及紧邻分配适配函数本体没有R14写入，已核对原EXE和两份DMP代码一致。
退役分配槽残留与当前cap8/buffer相容，仍不能恢复writer入口和reserve前后的R14历史。
首次非法指针定义未定位，没有修复；44948保持独立调查，没有运行节点或样例，原库身份和现场保留，双中继暂停。
详见[C-PID43712-POINTER-20261116/README.md](C-PID43712-POINTER-20261116/README.md)。


## C回传 2026-11-16：两份故障各自数据流和前轮标准库RVA更正

本轮只读两份既有Full Dump，哈希一致，没有运行节点、样例或采集器。
纠正前轮部分标准库RVA少算0x30000导致比较错误区间的问题，已重新解析实际函数、比对代码并更新原公开证据及SHA。
PID43712活动LowerHex参数槽指向有效两字节栈缓冲区，下游memcpy的RDX非法；长度2分派前缀不修改RDX。
pad临时源指针槽已可被后续reserve帧合法复用，当前值不能补成历史，首次源值变化仍缺记录。
PID44948当前status调用者XMM保存区全零，与坏表不同；首次异常XMM定义仍未定位。
两个运行继续独立归档，没有共同根因结论，没有可证实修复，原库、身份及失败现场保留，双中继暂停。
详见[C-TWO-DATAFLOWS-20261116/README.md](C-TWO-DATAFLOWS-20261116/README.md)。


## C回传 2026-11-15：PID44948与诊断副本43712只读离线对照

两份Full Dump哈希复核，43712异常时XMM6/7仍是合法模板；String描述符cap8/len0、目标区可读写，非法的是复制源指针。
44948本次stage_profile.native_delivery已生成4096个合法hex字符串，支持共同格式化业务路径曾完成。
所查profile、String及复制代码相同，父status诊断改写和配置差异影响仍未排除。
共同根因未证明，诊断引入故障也未证明；43712按独立诊断副本异常归档，不并入44948首次破坏历史。
完整上下文、调用栈、代码页和对象元数据见[C-DUMP-CAUSE-COMPARE-20261115/README.md](C-DUMP-CAUSE-COMPARE-20261115/README.md)。
本轮没有启动节点或样例，入口保持冻结，原库身份和失败现场保留，双中继暂停。


## C回传 2026-11-15 22:49：隔离返回边界采集结束，取得同次调用最后正常点

修正准备工具旧路径后完成一次隔离实验，基线四节点均退出0，采集阶段约38秒目标43712退出0xC0000005。
取得211899968字节Full Dump，1748条同次内存事件连续且全部原始向量匹配合法模板。
第7次status入口1507，最后正常返回点777b65；随后777bb4调用的75c4a0子调用未返回，格式化两字节复制发生非法读取。
archive初始化未到，首次XMM错位仍未捕获，诊断副本影响仍未排除。
采集组提前异常，120秒干扰检查未完成，当前入口冻结，未进入20分钟持续运行、未自动重试。
原库身份及旧现场保留，任务与测试WER清理，原WER核对一致，双中继暂停。
详见[C-XMM-return-isolated-20261115/README.md](C-XMM-return-isolated-20261115/README.md)。


## C回传 2026-11-15 22:02：隔离向量采集未启动，Windows UAC返回取消

本轮准备独立诊断副本的返回边界内存记录，原call及callee返回地址保留。
279处返回边界、两个父帧位置可记录，47处重叠窗口保留为覆盖缺口，运行干扰尚未测量。
22:00:33请求UAC，22:02:36返回用户取消，新隔离根及运行目录均未创建。
没有新增同次调用或首次错位证据，没有启动基线/采集节点，原库身份及失败现场保留。
详见[C-XMM-guard-unrun-20261115/README.md](C-XMM-guard-unrun-20261115/README.md)。
已请求在场确认后重发同一入口。未修生产代码，双中继继续暂停。


## C回传 2026-11-15：PID44948 RawTable构造与insert入口帧归属复核

原.pdata/.xdata及离线CDB确认0x77823e/0x778245属于status，按status.RBP计算写RawTable。
insert_full真正的XMM6/7保存指令是0x84f076/0x84f071，保存区另列，不能混用两个帧的RBP。
signature/archive前五条初始化字节一致并复用同一栈位，已交叉核对首次insert的参数指针。
未取得历史入口XMM及首次坏值指令，ABI违规仍未证实，没有可证实的生产修复。
管理员只读确认残留20952归属LocalService，映像仍缺，Get-Process无法取得该PID，未清理。
详见[C-PID44948-rawtable-entry-20261115/README.md](C-PID44948-rawtable-entry-20261115/README.md)。
本轮只离线分析及只读查询，原节点未启动，原库身份和失败现场保留，双中继暂停。


## C回传 2026-11-15 18:12：原库existing自然故障，取得PID44948完整转储

按本轮授权仅启动一次原a254＋AOEM966、原库existing和原身份，LocalService。
17:57:47至18:12:42，约14分56秒自然退出0xC0000409，子码7，控制器未强杀。
WER Full Dump 227052423字节，SHA-256为50d0a28a2329ab02731c136d050c08476c2be55178060e0da72595a25d784a41。
再次在archive_work首次插入reads_started时申请36TiB＋16字节，坏表32字节与XMM6/7及当前保存槽一致。
本次83条传输记录、容量128、累计淘汰0，故障发生在未满状态。首次破坏指令仍未定位。
运行未附加调试器、未开PT/TTD/额外计时；CDB仅用于退出后的本地离线分析。
三个用途身份文件哈希一致，无新增WHEA，WER原目录已恢复，任务移除，无自动重启。
父44948另有无线程/无句柄子进程条目20952，其归属未确认，端口无监听，未触碰该条目。
详见[C-natural-existing-20261115/README.md](C-natural-existing-20261115/README.md)。
原库由本次授权节点正常运行，历史现场保留，未修生产代码，双中继继续暂停。



## C回传 2026-11-15：断点入口冻结，Clone局部XMM7流及PT旧资格复核

四点/入口计时两套本机启动器增加冻结标记守卫，历史证据保留。
只读查无节点/CDB；原WER Full Dump=2、Count=2保留，当前没有自然故障等待任务。
Clone中XMM7的+0x2b0值先存RBP+0x10，0x15a5c7d分支再临时取XMM14，
最终调用者值从RBP+0x210恢复；局部副本与调用者保存槽分别核对。
同次255记录的四处来源均未匹配坏向量，最终状态不能排除历史输入变化。
CPUID宣告PT支持，既有PT已采集解码，但异常尾部/状态关联资格仍失败；没有新录制。
详见[C-XMM-freeze-offline-20261115/README.md](C-XMM-freeze-offline-20261115/README.md)。
首次破坏仍未定位；原库身份和现场保留，双中继暂停。

## C回传 2026-11-15：三组完整status实测结束，当前计时方案不进入长跑

重新UAC后执行原a254＋AOEM966三组短时隔离，统一启用原生stage计时。
完整status P50分别14.448/26.839/123.526ms，各30样本、单调用计数差无缺口。
入口/返回和四点复用取得34组完整调用、204条事件，响应字段均已持久化。
136次模板检查和34次入口/返回保存恢复检查通过，首次异常仍未捕获。
完整查询时序影响明显，当前方案不具备长期取证资格；历史包序/调度仍缺。
全部节点退出0；CDB退出0并保留控制器收尾请求。任务/测试WER清理，无新增WHEA。
详见[C-XMM-timing-measured-20261115/README.md](C-XMM-timing-measured-20261115/README.md)。
原库身份和失败现场保留，未进入长期取证，双中继继续暂停。

## C回传 2026-11-15：原生status计时已接线，新的三组实测尚未启动

核对原a254已有observe_tick(TickStage::Status)及NOVOVM_PROFILE_STAGES计时。
三组隔离测量将统一启用原生计数器，并修正响应字段赋值前写出的缺陷；运行仍待验。
入口/RET与四点复用硬件槽会新增停顿、存在覆盖窗口，不能称已取得低干扰资格。
历史PID34452快照约66.22分钟；PID3848最后采样218.65秒，不能统一成66分钟故障。
逐包顺序、调度交错及保存槽历史仍缺。Windows UAC在16:10:56返回取消，未启新节点。
详见[C-XMM-timing-load-20261115/README.md](C-XMM-timing-load-20261115/README.md)。
首次错位未定位，原库身份和失败现场保留，双中继暂停；未进入长期取证。

## C回传 2026-11-15：四点实际捕获34次完整调用，原故障未复现

原a254＋AOEM966以LocalService在新库/新测试身份执行三组3分钟对照。
基线、仅附加及修正后的四点组均由原时限正常退出，节点和CDB退出码0。
四点组136次命中，34组同线程/同父帧完整0→1→2→3，实际线程DR配置核验通过，
XMM6/7均等于模板，未捕获首次变化。首轮控制通知未收到造成停滞已保留，非自然崩溃。
四点组报告P50+1.568%、ticks速率-7.226%，仅短时描述；完整查询延迟及回调耗时仍缺。
未做66分钟长跑、未修生产代码；新测试任务/专属WER规则已清理，无新增WHEA。
详见[C-XMM-fourpoint-20261115/README.md](C-XMM-fourpoint-20261115/README.md)。
原库身份和失败现场保留，双中继继续暂停。

## C回传 2026-11-15：构造期六次插入及255条传输记录快照

已将signature_work六次Core::insert_full、可选旧值释放和发布entry/drop逐点关联。
同次native_network_probe的events长度0，但transport.objects长度255，
因此继续聚焦原记录collect到NativeTransferEvidence::clone的两个片段调用目标。
最终活跃VecDeque cap256/head203/len255，53+202只是最终几何关系，不能充当历史命中数。
两次旧Value已由新对象覆盖，实际旧tag仍缺；所查正常返回恢复配对，首次异常仍未定位。
PID34452与PID3848共享insert入口，分别走Vec增长与reserve_rehash，尚无共同破坏指令。
停止扩展静态候选，仅提出四父帧检查点的稀疏捕获评估，尚未执行、干扰未测。
详见[C-PID3848-construction-focus-20261115/README.md](C-PID3848-construction-focus-20261115/README.md)。
本轮只读既有证据，原库身份和失败现场保留，未重启，双中继暂停。

## C回传 2026-11-15：PID3848有限机器码区间及候选调用路径

原a254从signature_work局部初始化到archive_work向量写入的有限清单已完成，
737条指令逐条与同一Full Dump比较一致，含74个静态调用位置及同函数冷路径。
按源码顺序和最终缓存/对象条件，发布后候选路径为8处调用、6个目标，
包含profile快照、network_probe_observation、JSON entry/drop、IO快照和TLS getter。
这是条件一致的候选路径，没有逐调用XMM历史；TLS最终已初始化不能排除本次冷初始化。
signature_work对象合法也不能证明其构造完成时寄存器仍合法，保留其自身插入/释放调用。
首次异常函数及指令仍未找到，没有可证实的ABI修复。原现场保留，双中继暂停。
详见[C-PID3848-status-interval-20261115/README.md](C-PID3848-status-interval-20261115/README.md)。
## C回传 2026-11-15：同次stderr证实一次故障后formatter与上下文捕获

管理员只读取得PID3848本轮stderr摘要，读取报告0，897字节，SHA256两次一致。
实际错误行为memory allocation of 39582418599952 bytes failed。
20帧回溯将core::fmt::write的0x1e524dd关联到Backtrace formatter 0x1e469b0，
首帧是其RtlCaptureContext调用的返回地址0x1e46b62。已解析同次Stderr vtable。
这次捕获/回溯发生在坏表和巨大申请之后；更早XMM首次变化与错误恢复仍缺历史。
原EXE与AOEM966的PE Machine均0x8664，原CPU优化参数未反推。
详见[C-PID3848-runtime-targets-20261115/README.md](C-PID3848-runtime-targets-20261115/README.md)。
最小捕获方案仅列静态预算，实际干扰未测、未启动新运行。原现场保留，双中继暂停。

## C回传 2026-11-15：PID3848嵌套XMM审查与剩余动态边界

从22个原直接目标递归展开，新增嵌套向量使用者、静态TLS描述符目标、
尾跳/分派表和异常处理入口。所查12个向量使用函数正常返回路径均恢复本地保存槽，
帧基址及保存槽历史内容仍缺运行证据。保留42个动态分派缺口及系统DLL边界。
尚未找到首次异常恢复或可修补的ABI错误，不将局部核对升级为全路径安全证明。
signature_work完成时的实际向量值仍未知，archive_work构造复用向量、没有重载常量。
详细调用覆盖和确切缺口见[C-PID3848-recursive-XMM-20261115/README.md](C-PID3848-recursive-XMM-20261115/README.md)。
同一DMP哈希复核一致，本轮离线分析，原现场保留，双中继暂停。

## C回传 2026-11-15：PID3848写表指令与保存槽复用已核对

字段32字节、异常XMM6/7及insert保存值逐字节一致。正常构造写表指令为
status RVA 0x77823e/0x778245；表与当前保存区互不重叠。
同次主status对象50项可读，signature_work、stage_profile、native_network_probe表合法，
可选IO/asset字段缺席。四个直接使用XMM6/7的被调函数保存恢复偏移可配对。
它们旧XMM槽0x53aaceeb20/eb30已由当前insert的XMM7/XMM8覆盖，
旧槽无法作为先前恢复历史。首个异常寄存器变化仍未定位，未修生产代码。
详见[C-PID3848-vector-writes-20261115/README.md](C-PID3848-vector-writes-20261115/README.md)。
本轮离线分析，原现场保留，双中继暂停。

## CURRENT 2026-11-15 A签收2336cfc9：36TiB申请已核实，追同一现场的首次错误恢复

A已下拉C的README、allocation-values-public.json及机器码，并独立重算：
items+1=2084962082369，full_capacity=1824341955268，取整到4398046511104桶，
最终申请39582418599952字节（0x240000000010），align16，与转储保留值一致。
bucket_mask不是2的幂减1；新表ctrl=8、items/mask为堆地址形态，不是正常容量增长。
正常52字节分配被错误元数据变成36TiB+16；提高内存预算、改容器或关闭status都不是修复。
本轮无新WHEA的结论保持，但不从该负结果认定硬件健康。

**A进一步只读检查原EXE，而不是要求C再做BASE：**
使用同哈希原EXE/匹配PDB及原PE函数边界，解析status初始化到故障call之间28个唯一直接call目标。
该直接目标集合内，有四个函数体显式使用XMM6/XMM7，均有对应保存与正常出口恢复指令：

| 原程序RVA | 所有者 | 保存位置 | 恢复指令RVA |
| --- | --- | --- | --- |
| 34b9a0 | LocalKey::with，status在777d35调用，源码为traffic::snapshot | RBP+1e0 / +1f0 | 34ca58 / 34ca5f |
| 5cf780 | network_probe_observation | RBP+5e0 / +5f0 | 5d0a7b / 5d0a82 |
| 84f050 | IndexMap insert_full | RBP+20 / +30 | 84f36d / 84f371 |
| 863990 | AdmissionState::status | RBP+1d0 / +1e0 | 86682a / 866831 |

这是直接函数体静态检查，不证明所有嵌套调用、动态导入、保存槽内容或历史返回均正确。
未找到可以直接修补的漏保存指令；不能把表构造处的正常movups当成首次破坏者。
status在故障前还调用原native_io_cost_snapshot、可选asset诊断序列化及Value替换/释放；
本段直接call没有AOEM入口，不能据此将本次归因AOEM，也不排除更早的内存破坏。
源码status、profile及traffic快照相关片段未见手工asm/跨ABI或直接裸指针写入；
这不是全库安全证明。并发ASAS/手机修改不参与本轮，不为本故障改动它们。

**C直接沿同一PID3848转储继续，不重跑、不开发新工具，也不再等待A签收：**
1. 补回此前同一stderr的实际读取结果/退出码/分配错误原行；若缺失、空或仍拒绝访问如实写，
   不将转储推算改称stderr实测。当前公开提交未包含该结果；它不阻止后续同dump分析。
2. 复用已有离线调试器读取status局部最终主对象及仍有效的子对象，核对native_delivery_traffic、
   native_send_polls/native_receive_totals/native_receiver_queue等字段是否实际构造，结合原分支定位
   最后已证合法表与archive_work错误表之间的区间。对象字段存在只证明构造结果，不补造执行历史。
3. 对区间内原call及上述保存/恢复槽，优先利用仍未被复用覆盖的栈/对象证据：核对
   0x1e5716ef0e0与0x1e5716c9e40实际内存归属/对象布局，以及这组32字节与相关保存槽是否有
   来源关系。只读窄元数据，不导出业务内容；旧栈残值不能自动当活跃帧或确定的过去事件。
   需要检查间接函数时沿同dump的实际导入目标和模块字节，不凭近邻符号命名。
4. 如确定具体函数漏保存、错误栈偏移或越界写，直接报告原指令/源码/对象范围及因果链，
   修原owner并做有针对性的最小验证，不重复正常样例或75分钟BASE。若同一快照不能恢复
   首次转变，明确回证据边界，不扩建记录器、不擅自把某假说登记为已修复。

当前明确的近因是异常空表/向量状态造成巨量申请；首次错误写入或恢复仍未定位。
A本轮没有生产补丁，没有新增测试或诊断工具，未操作节点，双中继仍未发送。
不能因用户要求加快就用重载常量、删除status或吞掉分配失败来掩盖未知内存破坏。

## C回传 2026-11-15：PID3848分配来源已追到空表模板向量

离线完整转储核对请求0x240000000010（约36TiB）、align16。
status构造archive_work并首次插入reads_started时，空表Vec长度0，
RawTable的ctrl=8，mask/items已成为堆地址形态值，扩容算术逐项匹配失败请求。
字段与insert入口保存槽及原始异常XMM6/XMM7完全一致；只读空表常量仍合法，
所查原EXE代码字节一致。首次向量改变的调用/恢复指令未定位，未提交生产修复。
PID3848所查RBP帧关系正常，与PID34452的0x2A0共同根因仍待证。
详见[C-PID3848-allocation-20261115/README.md](C-PID3848-allocation-20261115/README.md)。
原现场与身份保留，本轮仅离线分析，双中继暂停。

## CURRENT 2026-11-15 A签收d9bd6c25：BASE自然分配失败，直接分析同一Full Dump

A已核对run2终态、栈、样本及WHEA：目标3848自然退出0xC0000409/参数7，
其余三进程才被控制器停止；本轮无新增WHEA，不等于硬件排除。不要再执行下面旧CURRENT的BASE。
本轮已取得自然故障完整内存，不重复BASE/硬件筛查、不开发工具、不切包或禁用status。

A实际校验本机原EXE SHA256为1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d，
与C相同，按C模块基址0x7ff7f0950000换算并反汇编：
- status返回地址RVA0x778395前的call对应新建archive_work对象第一次插入reads_started。
  原机器码写入13字节键reads_started，Core位于status帧RBP+0x1c0；不是已有大缓存增长位置。
- Core+0x18为本次RawTable；insert_full在RVA0x84f39b明确additional=1，
  RVA0x84f3a9调用reserve_rehash。其输入表依次为ctrl、bucket_mask、growth_left、items。
- 新表模板RVA0x24f5410起四个qword为静态ctrl指针、0、0、0；正常首次插入应选择4桶，
  layout.size=align_up(4*8,16)+4+16=52，align=16。这只是正常路径预期，不是崩溃实测申请量。
- fallible_with_capacity在RVA0x1d91d66调用分配器，返回空后沿0x1d91db2/dfa进入
  handle_alloc_error，返回位置正是C栈0x1d91e07；容量溢出有另一分支，不应混写。
  此帧RBX保有计算的申请字节数，R14为桶数，R15为ctrl偏移；需在dump正确展开此帧核验。
  LLVM显示的邻近aws_lc导出标签不是这些Rust函数归属，按匹配PDB和RVA定位。
公开机器码见[C-isolated-BASE-run2-20261115/A-original-pe-disassembly.txt](C-isolated-BASE-run2-20261115/A-original-pe-disassembly.txt)。

**C收到后直接做同一转储的离线分析，不等待再次批准，不启动节点：**
1. 用已有提权读取入口只读本轮目标stderr，定位Rust的memory allocation of N bytes failed
   原行及时间/文件摘要。普通账户拒绝访问不是缺失；不改ACL、不导出受保护钥或完整环境。
2. 复用现有离线CDB，打开3848这份SHA256为6c3b7b9dc34ec3c137c2b4e7bcf0f4746cf6ce023db90f6a6dc423dce2613de6
   的Full Dump和原匹配PDB。先kn编号，再.frame /r选择fallible_with_capacity帧，
   读取非易失RBX/R14/R15和保存栈槽、相应机器码；交叉核对stderr申请量。
   不把kv的Args to Child或异常现场RCX/RDX当成历史调用实参；展开不可恢复则明确写未知。
3. 选择reserve_rehash帧，其RSI保存原RawTable指针；只读该地址4个qword，
   对照status帧RBP+0x1d8的RawTable及Core向量长度/容量。结合展开保存槽核验指针，
   不单凭优化后的变量名。确认实际items/bucket_mask/growth_left与新空表是否一致。
4. 若为52字节正常申请失败，检查同一dump已有堆摘要/分配器状态及既有系统资源记录；
   私有内存约67MiB不能证明当时系统commit充足，当前空闲内存也不能替历史证据。
   若为异常巨大申请或表字段异常，优先查这次局部对象/保存寄存器的首个不一致，
   不先归因IndexMap缺陷、AOEM、CPU或此前RBP故障。不从单次abort宣称共同根因。
5. 只回传脱敏结论、命令/退出码、必要标量/窄栈片段与原日志摘要；原始dump留C。
   找到明确源码根因才修原owner并做针对性验证；本指令不要求新实验或新工具。

A目前能证实具体插入点和分配失败分支，尚未读到C私有dump中的真实容量/申请量；
生产代码未作猜测修改，根因未结案，双中继暂停。D不操作，原库/身份保持现状。

## C回传 2026-11-15 12:17：隔离BASE run2自然失败并取得Full Dump

修正两处诊断脚本后，一次a254+AOEM966隔离BASE于12:13:19启动。
目标PID3848约4分钟自然退出0xC0000409，参数7；三个对端随后被控制器停止。
取得213986993字节Full Dump，18线程上下文、异常代码和栈页齐；前后无新增WHEA。
离线栈指向status -> IndexMap/Hashbrown扩容 -> Rust分配失败abort，不等于物理内存不足或已证RBP同根。
真实退出码、负载及公开转储结构见[C-isolated-BASE-run2-20261115/README.md](C-isolated-BASE-run2-20261115/README.md)。
原库身份不动，原始转储仅本机；未重启、未修生产代码、双中继暂停。

## CURRENT 2026-11-15 A核验8f66da37：修既有控制器误停，再执行一次BASE

A已读取启动和终态；本轮135.73秒后四进程均被控制器强停-1，不是自然崩溃。
原a254确有两个独立elapsed判断；main的1f4bd204已改为同次progress_report_due锁存。
因此不重复修改生产源码、不为这次基线换包；该报告字段修正也不等于内存崩溃修复。

C直接修本机原Owner.ps1/Launch-Once.ps1这两个实际阻断，不新建诊断工具：
- 每个进程首次已观察到NOVONET/transport能力后记录这个事实，但该历史事实本身不能
  证明一直健康。后续单份可选novonet缺失不触发杀进程；结合实际进程存活、报告新鲜度、
  同一进程的ticks/收发计数进展及明确错误判定。持续无进展仍按原有界规则失败，
  不移除时间、内存、磁盘、自然异常、错误和负载要求，不扩大既有预算。
- 等待任务结束后再次读取result.json；合法已写终态优先于旧轮询看到的“无结果”。
  区分暂未读到/解析不完整/最终缺失，保留有界等待；不要靠忽略异常或伪造退出0放行。
- 先对已保存9份采样和终态离线核对：peer3缺字段且1709→1941应继续观察；
  死进程、明确错误、持续陈旧/无进展、资源越界仍须失败；已存在结果不再误报缺失。
  这些只验证原控制判断，不运行额外节点、格式化样例、PT/TTD或硬件筛查。
- 修正核对通过且确认旧四进程已退出后，在新证据/独立状态目录执行一次同配置BASE，
  仍用原a254/AOEM966、LocalService及已合格转储规则，无调试器，最多75分钟。
  不复用旧日志掩盖135秒失败、不碰原节点库/钥。若已有新的BASE正在运行则只核对回传，
  不重复启动。启动即回PID/实际账户/哈希/截止，终态按自然退出与控制器收尾分别记录。

脚本仅在C私有target，本回执是明确的必要修正与续跑指令，不冒称A已改C脚本或已启动。
下一份必须优先回实际运行/失败，不再新增准备清单；D不操作，双中继暂停。

## C回传 2026-11-15 11:31：BASE被控制器提前结束，未完成75分钟

LocalService独立身份生成退出0；四个原a254进程实际加载AOEM966哈希一致。
一次运行135.73秒后，脚本把单份报告缺少可选novonet字段误作负载不合格，
终止四个隔离进程，实际退出均-1且明确标记控制器强停；不是自然崩溃。
网络计数仍增长，无新增WHEA、无新DMP。启动助手另有等待后未重查结果的误报，原件全部保留。
未重跑、未修生产代码；原库身份和双中继暂停状态不变。
完整回传见[C-isolated-BASE-20261115/README.md](C-isolated-BASE-20261115/README.md)。

## CURRENT 2026-11-15 A签收d2967fa1：立即执行已有BASE，不再新增工具资格轮次

用户要求直接推进实际阻断定位/修复。A已下拉并阅读C实际CPU8/内存12项结果和退出记录：
CPU到界由控制器强停-1，内存到界请求关闭0，未新增WHEA；这不是硬件排除或节点修复。
另已确认C原PID27928使用过与A相同EXE及旧AOEM而崩溃，后续966库差异不能解释所有故障。
当前要补的是尚未执行的无调试器NOVOVM BASE，不再要求Prime95/WinSAT或PT/TTD新一轮资格。

**C同步后直接做以下已有范围，不再等待A批准或签收：**
1. 先检查是否已有本轮BASE实例；若已在运行，沿用并回实际PID/开始时间，不重复启动。
2. 若尚未启动，使用b8ab48bf已验证的LocalService独立转储目录，复用C已定位的原
   setup helper prepare模式准备本轮独立测试用途材料。核对实际helper来源、退出码及
   同一LocalService账户能解封；不得借用桌面用户封装钥或原节点钥。若这一步失败，
   回传精确命令（脱敏）/错误/入口源码及制品身份，只处理这个实际阻断，不新造准备系统。
3. 使用既定原a254 EXE（1758e4a7...0d0c46d）及AOEM966
   （5743f587...a01c0d52），沿现有隔离负载入口执行一次最多75分钟BASE。
   保留既有NOVONET和周期status构造、同一账户及独立库；原EXE仅按已合格WER目标名
   novovm-base-a254.exe改名并核对字节一致。不得运行CDB/PT/TTD、插桩标准库或新增压力工具。
   没有新增证据前不更换库、禁用status或修改并发，以免第一次BASE已混入多个变量。
4. 启动后立即回传BASE_STARTED及实际开始时间/PID/EXE和加载AOEM哈希/账户/截止时间，
   不要等75分钟结束才让A知道。原现成转储/运行记录足够用于本轮；缺少精确计数写不可得，
   不为获取更多指标增加实现。到界退出、异常退出和人工收尾分别记录。
5. 结束或首次自然异常后回真实退出码、完整转储是否产生、首末可用负载计数、同期WHEA。
   复现后分析该次异常和既有转储差异，再选择有依据的一个变量；不要自动重跑整套测试。
   若无复现，报告本轮覆盖和与历史差异，不宣布硬件正常、程序修复或继续无限长跑。

A已复核现用status到profile snapshot入口：仅删除progress文件路径并不会关闭status构造；
不能用伪关闭对照定位。现有格式化/锁保护片段本身未提供已证实的首次非法写入，暂不改
String/memcpy或跳过观察来冒充修复。实际失败一旦指向具体owner，直接做最小修正与回归。
本次交接不等于C已启动；A未接管C私有目录，不启动A替代C故障现场。不创建新程序包/Full，
不变更原库、身份、BIOS或驱动；D不操作、双中继暂停。手机任务仍由C独立管理。

## C回传 2026-11-15 10:56：A回执已核对；有界CPU/内存正确性筛查结束

C已下拉1aa5fb56并读取A最新24小时退出0证据。A当前停机，旧AOEM与C后继966不同；
但C原PID27928曾使用与A完全相同的a254/旧AOEM且崩溃，不能用后继DLL差异解释全部故障。
A/C负载计数、系统和硬件仍未匹配，不宣布环境根因。
C使用官方SHA256匹配的便携Prime95，无安装/驱动/节点操作：CPU8项自检通过，
到界关闭失败后由控制器强停exit-1；内存12项自检通过，到界正常关闭exit0。
前后无新增WHEA，现无测试进程。短时、有限内存和未定点APIC33覆盖不代表硬件正常。
详情及原始自检输出见[C-correctness-20261115/README.md](C-correctness-20261115/README.md)。
原库/身份/失败证据保留，生产代码未修，双中继继续暂停；诊断程序仅本机target隔离保存。

## CURRENT 2026-11-15 A交接C：签收633f223a，停止扩建诊断工具，转有界对照

推送前并发收到8e7b16b7，A已无损合入并阅读：C保留历史共14条同类WHEA，12条早于
11月调查，不能把它们全部解释为TTD引起；原节点几次崩溃与这些事件没有已证明的即时因果。
C两次WinSAT完成且未新增WHEA，仅为短时带宽观察，不再重复，也不当内存正确性通过。
A原升级回执已有当时加载AOEM的1b764a36...acdce50证据，现磁盘复核一致；C后继AOEM966
为5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52，确实不是同库对照。
本交接补齐A侧证据，不要求C等A再次签收。后续硬件正确性检查优先已有正规工具，
先确认不安装驱动/不改配置且运行有界；需额外系统变更才提出具体方案，不再自制硬件诊断器。

用户要求A交付对照结果，C继续核查硬件和软件原因，尽快定位并修复，不再重复开发诊断工具。
A已读取b8ab48bf完整转储资格及633f223a硬件报告；本机只读实查见
[A公开对照数据](A-environment-comparison-20261115.json)，不是仅用户口述的24小时结果。

- A当前磁盘EXE SHA1758e4a7...0d0c46d，AOEM SHA1b764a36...acdce50；完整值见JSON。
  任务LocalService，2026-11-12 16:13:18启动，11-13 16:13:19.975退出0；
  原owner设置RUN_MS=86400000，与约24小时退出一致。现在无节点进程。
- A自11-08保留的Application1000/1001共64条，无novovm-node/novovm_node匹配；
  System自11-12无WHEA匹配，查询成功。无记录不等于证明硬件或软件绝无缺陷。
- A CPU Ultra9 275HX、Windows26200、BIOS Q7CN37WW、2x32GiB Micron/6400；
  C报告i9-14900HX、Windows26300、N0CN22WW、2x16GiB/5600且厂商不同。
  这些仅是变量，不是硬件损坏结论；不把混插内存直接列为根因。
- A最后progress ticks233204557、executed_batches=0、durable_decisions=0、
  execution_failures=0，同时存在拒绝/过期计数。非资产压力或业务稳定性通过。
  当前磁盘哈希不是历史加载模块证明；精确status调用次数和网络输入序列不可得。
  尤其C后续AOEM966对照不能自动视为与A相同运行库，须比较实际完整哈希。

**C现在直接沿已有工具推进，不等待A再次签收，不让D操作：**
1. 将原部署a254/原AOEM崩溃与后续a254/AOEM966、TTD/资格程序分别列账。
   用已有EXE/DLL摘要、账户、开关、工作量及退出时间核对，不用源码简称替代库哈希。
2. 复用b8ab48bf已通过的LocalService完整转储目录和原准备owner；完成尚缺的
   隔离用途材料核验后，执行既定一次BASE（最多75分钟，无CDB/PT/TTD、无插桩标准库）。
   用独立库和测试钥，不改原库/原身份；novovm-base-a254.exe必须与原EXE字节一致，
   明示改名及隔离输入差异。记录可取得的真实负载/内存/线程计数和目标退出码，
   没有精确计数的字段写不可得，不为补数字另造采集框架。
3. 硬件线同时复用现有Windows事件记录：BASE前后保留WHEA时间、记录ID及错误类别，
   与原异常/转储时间关联。C两条已更正错误只支持继续核查硬件/系统，不证明原崩溃原因。
   可先用本机已有、无需改配置的厂商只读健康信息；不与BASE同时叠加压力测试。
   需要重启型硬件测试、BIOS/电压/内存/驱动/VBS变更时列具体方案，不能一次改多个变量。
4. BASE若复现，保全该次完整转储与事件，按首个实际故障点选择一个有依据的单变量对照；
   若未复现，报告实际暴露时长/工作量与差异，不能宣布修复、排除模块或无限重跑。
   没有真实关闭开关的功能不得伪称关闭；GPU未证实参与时不先把GPU作为靶点。
5. 下一回执重点是BASE是否真实执行、是否复现、WHEA是否新增及对应时间、首个可证实
   的故障位置/剩余缺口、最小修复或判别实验。发现可证实代码缺陷才提交修复与定向回归。

停止扩展PT/MSR/TTD、通用记录器和正常formatter样例资格；现成材料直接复用。
若既有执行入口有具体阻断，仅修该入口必要问题，不再把诊断工具完美当作前置。
此交接不声称C已收读/BASE已运行/根因已找到；A本轮未启动节点或更改系统配置。
C仍NOT READY，双中继暂停；一次性调查材料继续与产品依赖和发布包隔离。

## CURRENT 2026-11-15 07:07：C已实跑File＋CSwitch，故障后包仅控制信息，尾部资格仍失败

用户重新要求弹窗后，C确认新UAC成功，仅执行已冻结资格程序一次，15秒基线＋15秒采集。
WPR启停及导出退出0，ETL367263744字节，目标PID22116/TID32528真实退出0xE0424354
（声明的资格异常，非NOVOVM原崩溃）。线程/模块齐全，20733目标PT、11650次CSwitch；
精确关联均为切入线程，故障切出时无同QPC/CPU的原始PT。故障后三个目标包全部只有
PSB/CBR/PSBEND、无TIP/FUP/TNT，实际0指令；检查的故障前片段仍未进入异常处理路径。
短时吞吐差-3.16%不代表原节点资格；同次FullDump已核验，但无首次RBP错位历史。
因此未放行a254＋AOEM966长跑，不补采、不重启原节点、不访问原库身份、不做修复，双中继暂停。
本轮实际原始结果及离线证据见[C-pt-cswitch-20261115](C-pt-cswitch-20261115/README.md)。
Git用于共享结果，不等待A签收；旧失败、取消和原始私有转储全部保留。

## PRIOR 2026-11-15：C已实测File保留，线程/模块齐全，异常尾部控制流仍未通过

C独立核验旧Memory ETL：29个非零LastDroppedTimes槽逐项匹配各CPU保留PT起点，
全CPU重筛目标仍只有7条，排除仅为Unknown/PID解析漏关联。新File模式实际运行同一
资格EXE，4KiB/PT与SampledProfile不变，fault/fault基线修正线程差异；WPR启停与解码
退出均0，ETL178651136字节、同次FullDump25516559字节，目标退出为声明的0xE0424354。
目标PT14931条，线程创建/退出、34条模块事件及10716条CSwitch保留，尾部间隔从7.742秒
缩为351.3微秒；但末6段实际解码仍无RaiseException/handler路径。容量上界也证明
这份ETL不可能包含完整15秒条件分支历史，不把旁带齐全当作连续指令记录。
短时吞吐差-3.45%非原节点资格。新增原生CSwitch触发已准备，但第二次UAC取消、未运行。
原节点/原库/身份/失败现场保留，未启动a254+AOEM966持续负载、无修复、双中继暂停。
不等A签收；公开结果与取消原件见 [C-pt-retention-20261115](C-pt-retention-20261115/README.md)。

## PRIOR 2026-11-15：C原生PT已实际采集并解码，同次异常关联有证据，故障前窗口仍缺失

用户重新确认UAC后，C只运行新隔离资格进程：15秒基线与15秒PT，自定义资格异常明确标记。
WPR start/stop均0，ETL约20.7MB、FullDump约25.5MB；目标PID28712/TID42560，原异常上下文
与JSON吻合，实际退出0xE0424354（非NOVOVM自然故障）。微软WPT及官方libipt/XED已实际解码
7个目标PT片段，可还原真实分支/调用/返回；同次dump目标.text与EXE一致，最终解码诊断无错。
但最后目标PT距资格异常仍缺7.7426714秒，ETL生命周期旁带不全，Lost=0不能补足该缺口。
短时吞吐差-3.89%仅为微负载粗测，不代表原节点低干扰通过；未启动a254/AOEM966长跑。
原节点/原库/身份/旧现场保留，不重采、不猜测修复、不恢复双中继；C独立推进，不等待A签收。
实际输出、工具固定提交、覆盖缺口及原始公开结果见
[C-native-pt-qualification-20261115](C-native-pt-qualification-20261115/README.md)。

## PRIOR 2026-11-15：高频监控入口已停用，NtContinue关闭冲突已审计，原生PT配置解析通过

C独立核对上一轮：全线程NtContinue守卫在关闭命令清断点前仍有效，实际先q退出；
旧Full Dump系统指令支持远程暂停线程初始化触发守卫的可行路径，但缺同次命中栈，
不把静态路径当实际线程证明或原始RBP根因。两个诊断执行入口已立即拒绝执行，实际退出1；
历史执行副本/失败证据保留，未重跑。WPR真实接受原生PT候选配置（解析退出0），仍未录制；
采集、解码、故障线程关联和干扰尚未验收，不能直接开始66分钟捕获。未找到首次错位，
不改生产代码、不启动原节点/隔离节点、不恢复双中继、不等待A签收。
公开命令、系统代码及边界见[C-monitor-retirement-pt-20261115](C-monitor-retirement-pt-20261115/README.md)。

## PRIOR 2026-11-15：C持续NOVONET隔离负载已执行，断点干扰门槛失败，未捕获首次错位

原a254＋AOEM966在新库启用实际NOVONET/网络诊断与周期状态查询，补上run2未启用该分支的缺口。
目标PID29764在放行前保留进程句柄；一次运行241.625秒，未重复启动。基线20报告、中位6.286秒，
开启持续检查后120秒零新报告，采集门槛失败；未覆盖原66分钟或预定75分钟上限。
请求关闭时NtContinue保守守卫执行q，MONITOR_OFF未出现，恢复对照未完成。目标真实退出
0xc0000354、CDB退出0、三个peer受控退出1，不算自然原始崩溃。2153组完整边界无错位，
末组因退出不完整；不作为修复证据。原库/身份/旧现场保留，不补跑、不猜测修复、不恢复双中继。
公开原始结果、干扰限制及控制失败见[C-sustained-rbp-20261115](C-sustained-rbp-20261115/README.md)。

## PRIOR 2026-11-15：C原a254隔离取证取得8组实际RBP前后记录，原始错位未复现

用户批准修正命令后执行一次；CDB嵌套回调改为$$><，先通过离线语法/字节核验，
再以原a254＋AOEM966、新loopback库和独立测试钥运行一次。8组48条事件取得，
LEA合法建帧、内部子调用返回和cookie前RBP关系均符合预期；未捕获首次异常写入。
63.11秒结束，CDB及3个独立peer退出0，没有强制停止或遗留进程；节点0独立OS退出码
未另行保留，不能写成4个节点退出0。仅覆盖8个选定调用，无完整历史或低干扰资格。
原库/身份/失败现场保留，无生产修复，双中继暂停，不等待A签收、不自动再跑。
公开事件、命令、逐组比较及退出码见
[C-original-rbp-instruction-run2-20261115](C-original-rbp-instruction-run2-20261115/README.md)。

## PRIOR 2026-11-15：原a254隔离运行命中真实分配位置，取证命令失败，未得RBP前后值

C直接以原a254＋AOEM966、独立测试钥和新库启动四个loopback原程序，无人工样例。
被调试节点命中RVA84eed4的原finish_grow调用，但CDB条件语法及回调文件加载失败，
约2.19秒终止。CDB退出0x8007001e不等于节点自然崩溃；三个peer为受控停止，退出1。
命令已修正并完成离线语法核对，未自动重跑；无遗留node/cdb。未取得RBP写前后记录，
未定位首次错位，原库/身份/失败现场保持，不做修复、不恢复双中继。
公开事实及实际失败命令见[C-original-rbp-instruction-20261115](C-original-rbp-instruction-20261115/README.md)。

## PRIOR 2026-11-15：C已实现并验证同次调用采集，仍未获得原故障首次错位

C独立完成MASM调用前后RBP/RSP记录、线程/调用实例/递归关联、SEH独立标记与二进制
校验器。阳性对照确实定位到测试callee的xor rbp指令，不能当作NOVOVM故障。
新建私有堆真实RtlReAllocateHeap测试：批次均值P50单线程+33.30%、四线程+17.35%；
仍不具备原故障低干扰资格，未接原节点。原故障的内部cookie fast-fail也可能不返回，
不能靠外层post-call记录定位首次写入。已保留并修正本工具首轮导出空记录问题。
详情及原始公开输出见[C-rbp-call-capture-20261115](C-rbp-call-capture-20261115/README.md)。
生产代码/原库/身份/Full Dump未动，不重启节点、不恢复双中继，无猜测性修复。

## PRIOR 2026-11-14：同次RBP动态采集尚未就绪，不重跑旧隔离样例

已核对原四进程harness：它是重新编译的插桩EXE、fresh loopback配置，不能等同原a254；
其161秒查询的断点方法已有不合格证据，本轮没有重跑。活动调用采集须按线程创建标识、
activation及call序号关联；合法子函数改用RBP不能误判为破坏。异常展开/非局部恢复未覆盖
时必须标记context_gap。现有工具尚未证明能在保留原时序条件下取得首次改变指令。
本轮只完成[采集可行性和覆盖约束](C-rbp-context-20261114/CAPTURE-READINESS.md)，未取新动态
记录、未启动原节点或隔离负载、未改代码/库/身份。双中继暂停，C独立负责，不等待A审批。

## PRIOR 2026-11-14：C核对RBP报告来源和保存槽，首次改变指令仍未定位

C直接解析PID34452同次Full Dump：异常流RBP与ntdll失败报告上下文一致，报告入口保存的
检查参数也对应错误槽，不是仅由WinDbg kv展开得到。准确地说，这是RtlCaptureContext加
报告帧RtlVirtualUnwind构造的异常上下文，而非未经处理的CPU陷阱；报告帧无RBP恢复项。
本函数和两个上游RBP保存槽与调用者帧关系一致；本函数自己的pop rbp在失败检查之后，
尚未执行。不得误判“自身保存槽已恢复出坏值”，也不能据此证明所有栈内容完好。
已记录具体静态RBP写入候选及取证缺口，未找到首次改变指令，无生产修复。C不重启，
原库/身份/失败现场保留，双中继继续暂停。C独立推进，Git共享不等待A签收。
见[C的RBP专项同次现场核验](C-rbp-context-20261114/README.md)。

后续离线核对：入口RSP推算fee008、保存入口RBP为fee130，本帧应为fedf80、失败为fee220；
0x2A0只是差值，不证明存在加法指令。最终堆签名FFEEFFEE、stack-tracking flags=0，
按此值不选中先前列出的HpReAllocate/StackTrace候选分支，但无先前条件历史，不能全部排除。
新比较已保留；没有找到首次改变指令，保持停止及不做猜测性修复的边界。

## PRIOR 2026-11-14：C完整转储复制/离线核验完成，帧基址偏移证据，未修复

C的PID34452原始Full Dump已在用户确认UAC后复制至本机受保护目录，源/副本哈希一致，
退出码0；原件、ACL、原库及身份未修改。CDB确认c0000409子码2（栈cookie检查失败）。
同一Memory64中，正确栈槽cookie仍匹配，但实际RBP比序言要求的RSP+0x100偏移+0x2A0，
导致检查读取另一处栈地址；不能简化为正确cookie已被覆盖。20线程上下文及指定堆/代码页
已核验。首次破坏RBP的指令未定位，不能归罪ntdll/AOEM或宣布历史崩溃同根。
C保持退出；不重启、不恢复双中继、不发对象。C独立继续离线调查，不等待A审批。
详见[C同次完整转储核验](C-wer-natural-20261114/README.md)。公开Git仅放脱敏比较和脚本，
完整DMP、cookie值、原日志和内存只保留本地。A签收尚未确认。

## PRIOR 2026-11-14 17:13：C自然崩溃，WER生成233MB新Full Dump，未重启

PID34452原库existing单次运行约66分钟后退出，实际码-1073740791/c0000409，
任务结果3221226505。资源保护未触发，不是人为异常；未重启、未放行双中继。
新DMP为233,403,217字节，flags0x221826含FullMemory，SHA两次稳定：
7d910fa6713e399c6bb394f2201fec622771dbc8e0665894ce9b8c115c9231aa。
原件留LocalService受保护目录，进一步复制/离线覆盖核验正在等待Windows UAC。
本次提交尚未分析新异常参数，不能把c0000409名称当作根因；首次破坏指令仍未找到。
详见[C本次自然故障终态与原始公开记录](C-wer-natural-20261114/README.md)。

## PRIOR 2026-11-14 16:07：C一次existing自然故障取证运行已开始，禁止联调发送

用户批准C原库单次运行，原a254/AOEM966实际加载哈希和原身份匹配，PID34452。
LocalService下existing启动，原日志/失败现场保留；首次进度height1、恢复完成、失败0。
WER Full Dump专属规则继续有效。最长6小时至北京时间22:07:24，异常不重启。
首轮只因保守32GiB空间预检停止、未启动；修订为明确18GiB预算后才启动一次。
本轮不发对象、不启中继观察器，A/B/D不得据此恢复双中继或发送测试对象。
当前只是运行中的取证，不是修复或稳定性通过。详见
[C单次自然故障运行](C-wer-natural-20261114/README.md)。

## PRIOR 2026-11-14：C的LocalService WER完整转储资格通过，节点专属规则已配置

按用户新授权，C独立核对a254/AOEM966异常处理与AeDebug/WER，未发现已知阻断项。
唯一独立LocalService测试进程生成11,074,978字节Full Dump；65,536字节堆模式、
代码页、两个线程上下文与SHA稳定性通过，管理员入口/验证器退出0。
仅为novovm-node.exe新增DumpType=2、DumpCount=2及LocalService Profile私有目录，
读回一致；全局WER、AeDebug、ACL不变。测试任务/测试key已清理，回滚入口已准备未执行。
原节点未运行/重启，原库和身份未访问，生产代码不变。不是原始故障复现或根因修复，
尚未取得下一次NOVOVM自然失败完整现场，双中继继续暂停。不等待A批准。
公开资格、配置和边界见[C WER完整转储验收](C-wer-full-20261114/README.md)。

## PRIOR 2026-11-14：C已读取LocalService原转储，9份仍为MiniDump，无完整堆/根因修复

C通过管理员PowerShell7一次只读取得LocalService原目录及System WER清单，退出0；
未改ACL/服务权限，17个文件原件及私有副本哈希一致，原大小/创建及修改时间保持。
9份DMP包括既有5份同SHA、3份更早的不同构建原件及1份已知TTD录制器故障；
全部为MiniDump。PID37796目标记录184B和Writer/pad_integral代码97B/247B仍缺失，
对应三个4KiB页均未采入；不能拿其他转储补齐同次历史。
既有6个缺pdata范围已分类：4个叶/尾跳桩无需本地帧展开，2个SysV主体改变RSP；
后者静态展开缺口仍待核实动态支持，正常返回ABI错误及崩溃因果均未证明。
已准备独立WER完整转储方案，未配置/运行。首次破坏指令未找到，无生产修复，
不启动原节点、不访问原库/身份、不恢复双中继。Git同步不等待A。
详见[C原始转储取证、内存覆盖及准备方案](C-localservice-dumps-20261114/README.md)。

## PRIOR 2026-11-14：冻结逐调用插桩，C独立静态审计原始二进制，根因仍未定位

按用户指令停止优化 fcb63e7d/de8df5ca 采集器，不再修改标准库或重复正常样例。
C只读核验原875/a254、两份包内AOEM及AOEM966，延伸Writer外部调用并核对
Windows→SysV汇编调用边界；所查调用者具有GPR/XMM保存恢复，未发现可归因的漏保存。
汇编候选缺静态.pdata覆盖，保留为异常回溯限制，不等于原崩溃根因。
五份原始崩溃归档均为MiniDump；PageHeap/自检完整转储不能拼成原现场。
当前无node专用WER完整转储设置；LocalService原转储目录读取拒绝，未声称全盘无其他文件。
本轮无运行负载、无原节点/原库/身份访问、无配置或生产代码修改，不重启或恢复双中继。
完整审计范围、未覆盖项、只读输出和原生转储评估见
[C原始二进制审计](C-original-static-audit-20261114/README.md)。Git只同步，不等待A签收。

## PRIOR 2026-11-14：C分区与实际Writer入口已接入，时序资格拒绝，双中继仍暂停

基于fcb63e7d，C独立导出a254和Rust标准库副本，完成A/B/C/D独立事件通道、
真实String::write_str源码入口与pad_integral关联、匹配PDB和静态机器码核验。
未更改部署程序/原库/身份；不是原EXE的入口寄存器历史，不等待A批准。
新方法短时ABBA查询P50 +56.4%、P95/P99 +24.9%，按预设10%门槛拒绝长时复现。
A保留创建/Drop，B完整采样组无错位，但大量调用被明确采样省略；原故障未复现、
首次破坏指令仍未找到。16个新隔离owner退出0不等于修复。失败构建与所有原现场保留。
不启动原节点、观察器或双中继。详见[实际结果与方法限制](C-partitioned-writer-20261114/README.md)。


## PRIOR 2026-11-14：C低干扰采集资格未通过，保留方法失败，不重启/放行双中继

C独立完成一次有界方法资格：原a254源码独立导出、固定AOEM966，TLS定长事件、
无逐边界断点、四个新本机网络owner。首组未使用`-hd`，保留并排除；修正组
P50增加33.1%、P95增加17.9%，结束窗口只剩格式化事件及停止标记，缺少生命周期历史。
因此不以P95单项门槛或正常退出宣布采集器可用于原故障长期复现。
实际Object DONE/完整性及时间拒绝均保留；Carrier/持续淘汰/原输入重放未覆盖。
原崩溃未定位，生产代码、原库、身份、旧失败保留，双中继不恢复。
Git用于同步，不等待A批准。详见[公开报告和下一次运行技术门槛](C-lowimpact-capture-20261114/README.md)。

## PRIOR 2026-11-14：C 独立受控隔离采集，A 签收不是前置条件

用户最新明确 C 独立设计、实施和调查；覆盖下节等待 A 审阅后再推进的门槛。
A 只接收 Git 同步，确需 A 的构建或节点操作时另行协调，不作为调查审批节点。
20e010df 的历史证据边界继续成立，但不阻止取得新的隔离失败证据。

C 本轮完成独立 a254 诊断构建、固定 AOEM966、四个本机回环测试 owner，使用全新
测试库/测试身份；原库、原身份、部署节点与失败现场不动。不使用 TTD，不进行四机
双中继，不提交猜测性生产修复。采集器先验证符号/机器码和完整转储，再允许一次
有界运行。该运行是新联合上下文，不冒充旧输入重放或原二进制。
一次新鲜回环联合运行已结束，目标与三个同伴退出0；未复现原故障。
逐边界断点使唯一一次状态查询耗时161秒，改变了时序，不能算周期查询复现或修复。
不原样重复本轮负载。公开结果、采集缺口和原件哈希见
[C独立采集报告](C-isolated-network-capture-20261114/README.md)。

## PRIOR 2026-11-14：本轮调查终止；请求 A 签收证据边界与采集提案

用户确认以 **20e010df** 为本轮事后调查的证据边界和阶段性终止点。
首次破坏指令未定位，原始崩溃未修复；不是问题关闭或稳定性通过。
停止重复旧 DMP 推演、已通过样例和诊断工具试跑，不新建 Phase7/8。
保留原库、身份、历史失败；冻结生产代码，不启动 C、TTD 或双中继。

本次是**协调提案，不是新诊断结果或执行授权**。C 已 fetch，origin/main 为
20e010df，尚无 A 对本轮的新签收。请 A 通过 Git 回执确认以下边界及方案意见；
不要求 A 代查根因、操作节点或立即发送测试对象。

### 待 A 审阅：下次采集只围绕同一次失败

1. **输入记录。** 选定原网络 owner 入口和周期 status 入口，关联 run ID、单调
   时间、线程、事件序号、消息类型/长度、处理顺序与状态查询触发。逐包数据只允许
   在事先审定的隔离测试身份/负载范围内私下保存；现有真实身份材料、私钥、原库
   和消息内容不得被复制进日志或公开 Git。脱敏丢失的重放信息必须明确标注，不能
   把包哈希/长度称为完整输入。来源不明或超范围数据应停止采集，而非先全量存下。
2. **有限对象追踪。** 只覆盖已涉及的接收/发送所有权移交和 status 格式化对象。
   使用独立 Object ID、Generation、Owner、线程/调用序号，区分对象构造、借用、
   Arc clone/drop、实际分配/释放和地址复用；没有观测到的事件不得推算成事实。
   Arc 引用计数只能作为瞬时观测，不单独证明最后释放或并发安全。记录不得借用
   被观察对象的临时指针，不能使用待调查的格式化路径递归记录自己。
3. **失败边界。** 同一线程/调用关联 Formatter、尾调用前、writer 入口、复制前
   的实际参数及栈/寄存器，连同该次失败的相关代码页和对象堆页保存。缺帧、丢样、
   截断、异常线程变化均明确标记；不得拼接不同运行。完整 DMP 仍不是指令历史。
4. **隔离重放。** 固定 EXE/DLL/PDB、工具链及运行库清单；只使用独立测试状态，
   不读取或克隆保留的原库/身份，不接入 A/B/C/D 的现有服务。输入顺序、定时器、
   随机性和必要对端状态必须明确来源。新产生的测试负载不是原失败负载；包顺序
   一致也不保证调度/堆布局一致。目标是先取得可重复的失败，再验证最小修复。

### 必须先解决的设计取舍

- **原二进制与插桩版本分开。** 不改原 EXE 的调试器边界观测不能自动提供全部
  生命周期；增加源码事件点则形成新诊断二进制，不能再宣称与原 EXE 字节一致。
  请 A 回执选择/修订采集方式，明确覆盖范围和扰动；本轮不实现任一方式。
- **不能承诺确定性重放。** 原运行缺少输入历史，先前四项材料核验不补足它。
  新隔离失败可形成自己的证据链；与旧故障是否同根仍须单独证明。
- **运行条件另行定稿。** A 确认后，再列出固定构建、具体隔离路径/对端、输入
  来源、时间/空间上限、失败触发、证据保留及停止条件。上限停止或没有复现均为
  未解决，不自动重试，不因 A 文档签收就启动 C 或恢复双中继。

### A 回执与后续工作验收

请 A 的 Git 回执明确：`evidence_commit=20e010df`、签收/异议、上述提案的接受项
和修改项、建议采集方式及仍缺的运行前提。C 在收到实际提交后再标记签收；当前
`A_ACK_PENDING`、`CAPTURE_NOT_IMPLEMENTED`、`C_NOT_READY`。

之后每轮必须取得新直接故障证据、以明确证据排除一个假设、建立可重复的原故障
触发环境、或定位首次破坏位置之一；普通样例数量、工具构建、报告/推送本身不算
故障调查进展。本条仅正式终止旧路线并请求协调，不声称达到上述任一技术结果。

## PRIOR 2026-11-14 C failure-evidence gate: historical execution cannot be reconstructed from available material

C rehashed three original MiniDumps and the separate17:08 PageHeap full dump,
then checked captured-memory ranges without disk-image fallback. PID37796 retains
the selected final String/Formatter/writer stack slices, but not the complete
selected writer/pad code or target record. The PageHeap full dump is a different
failure and cannot fill this gap. Exact875/a254 EXE/PDB and AOEM966 are available;
the selected original archive lacks same-failure input and call-boundary history.
[Actual coverage, historical-material checks, preserved collection error and limits](C-failure-evidence-gate-20261114/README.md).
No new workload was run: this is insufficient evidence for deterministic original
replay, not a new negative reproduction result. First corruption remains UNKNOWN.
No production fix, original DB/identity access, node restart or relay release.
Do not substitute passing fixtures or another full dump for execution history.
C remains NOT READY; Git notification is not A acknowledgment. A/D do not operate.

## PRIOR 2026-11-14 C ownership audit: eviction hypothesis unsupported; stop repeating normal fixtures

C independently audited the original 875/a254 source. The 2048-entry delivery
ring stores copied identifiers/scalars, not borrowed network payloads. Eviction
and JSON traversal share its Mutex. The separate 256-entry transport evidence
container clones owned metadata under its own lock; it is not a payload pool.
Actual send ownership uses Arc; receive completion moves owned chunks. The
original tick executes network/poll/status sequentially; profile snapshot precedes
the transport evidence snapshot inside status. No concrete dangling-reference,
out-of-bounds write or race was established in these reviewed paths.
11 of12 selected original source files are byte-identical; this does NOT equate
the binaries, AOEM versions or runtime histories. No new runtime test was run.
[Ownership paths, original-source hashes, DMP correlation and stop condition](C-object-lifetime-audit-20261114/README.md).
First corrupting instruction remains UNKNOWN. Stop extending passing full-ring
fixtures as evidence for this hypothesis; the previous ordinary owner-loop test
proposal below is superseded by the evidence prerequisites in the new report.
No speculative repair, deployed-node restart, original DB/identity access or
double-relay release. C remains NOT READY. Git notification is not A acknowledgment;
A/D are not asked to diagnose, run tests or operate nodes for this return.

## PRIOR 2026-11-14 C Phase6: real retained load and original thread evidence; unresolved

C independently read PID37796's original logs and dump without opening the original
database/identity or restarting the deployed node. Last complete progress showed
zero executed business batches but a full 2048-entry delivery ring with 213510
evictions. The fault's saved iterator has 1340+708 entries; the earlier completed
progress implies1530+518. Missing heap pages prevent identifying the exact record.
Background owners waited in WER-time stacks; this is not a pre-fault scheduling trace.
One new isolated original-status scenario restored the retained window and wrap
shape using the original recorder.384 selected complete boundary groups agreed;
target/debugger exits0. Failed first capture is retained. Old heap history, actual
network timing and the last missing interval are NOT reproduced. No first corrupting
instruction found, no speculative fix, no node recovery or double-relay release.
[Phase6 original context, new evidence and limits](C-real-load-phase6-20261114/README.md).
C continues independently; A receives Git results, not an investigation assignment.
A/D do not rerun or operate nodes for this report. C remains NOT READY.

## PRIOR 2026-11-14 C Phase5: original status context; C investigates independently

C independently traced periodic JSON progress, historical JSON-RPC, native binary
Status and observer file reads. The native authenticated Status does not invoke
the JSON profile snapshot; Watch-Relay reads the file, not an RPC trigger.
One new isolated original RpcLifecycle/ResidentNode context ran with fresh local
test state, loopback-only peers, matching AOEM966 and a concurrent recorder thread.
448 complete formatter/writer groups agreed;33 other capture sequences were
incomplete and are NOT counted as passed. Both process/debugger exited0 and drain
completed. Original first corruption remains unknown, no production fix or relay
release. No deployed node restart, original DB/identity access or TTD.
[Phase5 context, evidence and coverage gaps](C-state-context-phase5-20261114/README.md).
C does not need A to diagnose or approve local investigation; Git coordinates
shared work only. No new A receipt was found after0470f343. A/D need not rerun
this test or operate nodes. C remains NOT READY for double relay.

## PRIOR 2026-11-14 C Phase4: diagnostic build and live boundaries; original fault unresolved

C built an independent optimized a254 node with full node-library/bin debug streams;
EXE/PDB identity and original-source checksums verified. This new node was NOT run.
One isolated original profile snapshot produced64 format calls /576 boundary records:
48 tail calls and16 normal zero-padding calls; debugger and target both exited0.
1,638 offline comparisons passed. Preparation/capture failures are preserved.
No original failure or first parameter corruption was captured. No production patch,
original DB/identity access, node restart or double-relay release. C remains NOT READY.
[Phase4 actual evidence, failures and narrow next experiment](C-boundary-phase4-20261114/README.md).
Do not substitute the new PDB for the original DMP or re-arm old relay windows.
This Git return is not A acknowledgment. D does not operate.

## PRIOR 2026-11-14 C exact-symbol analysis: ZIP received; first corruption unresolved

C received A's original ZIP and independently matched both hashes, PDB/EXE/dump
GUID E16D0B11-47CA-4EBF-B482-C91380CC57F8 age1. CDB loaded exact private symbols.
Same original PID37796 was analyzed offline; no fixture rerun or node launch.
Formatter's saved buf and writer's fault-time receiver disagree, but entry/tail
historical values remain unavailable. All16 node CGUs lack module debug streams
and file records, despite the PDB-wide debug/types flags. No first bad instruction
or ABI violation has been established. The elapsed+50df label is a nearest symbol,
not the containing PE function. No speculative patch or relay release.
[Actual symbol-load evidence, source limits and next minimal boundary check](C-symbolized-original-20261114/README.md).
C remains NOT READY. Original dump/PDB stay private; D does not operate.

## PRIOR 2026-11-14 C receipt: A acknowledged; original symbol ZIP not yet received

C fast-forwarded to 0470f343 and read A's explicit acknowledgment of e1e3a999.
Original PID37796 dump and a254 EXE hashes were rechecked and unchanged.
C's scoped receiving-directory search has not found the 4709244-byte original
symbol ZIP. The existing E-drive PDB is875; two similarly named local regression
PDBs also fail the expected size/hash. None is substituted for a254 symbols.
[C actual receipt and pending symbol analysis](C-symbolized-original-20261114/README.md).
Please deliver A's original ZIP using the pins below. No node/fixture rerun,
production patch or relay release occurred; C remains NOT READY.

## PRIOR 2026-11-14 A handoff: exact original a254 symbols found; C offline next

A fetched and accepted receipt e1e3a999, including Phase2/Phase3's explicit
unknown first bad writer and C NOT READY. The Nov12 deployment/READY statements
are historical, not current running-health evidence. Do not arm observers,
restart nodes or send a double-relay object on those old receipts.

**A has located the exact original a254 PDB**, not a rebuilt substitute:
EXE SHA1758e4a7...0d0c46d; PE CodeView and PDB both
E16D0B11-47CA-4EBF-B482-C91380CC57F8 / age1. Original build EXE matches the
deployed candidate. LLVM summary confirms debug/type information, not stripped;
optimized-out locals may still be unavailable. Full pins and source path:
[A public symbol manifest](A-original-symbols-20261114/manifest.json).

Private/local copy for C (not uploaded into public Git):
`C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-a25452f4-20261112\original-symbols-handoff-20261114\a254-original-node-pdb.zip`

ZIP4709244 bytes, SHA256e20ebc92ec49bda22281ad981723e8ac7a3b38a46ccb73937d929e6bfb43b748;
sole member novovm_node.pdb17256448 bytes,
SHA2568763d5cde5291460fa539430a082f7ace8408b0e75fa677ebd23f4d7e31643be.
Copy it to C's original receiving directory `E:\区块链四机通讯\`, or C's existing
local candidate directory if that drive is absent. No original program ZIP needs
recopy. Git handoff is not proof of symbol delivery; C acknowledge actual hashes.

**C next, one offline continuation:**
1. Verify ZIP/member pins and GUID/age independently. Keep the original matching
   a254 EXE and original PID37796 dump; do not substitute fixture or875 symbols.
2. Load exact symbols in the existing debugger. Return module/symbol-load result
   and actual image/symbol identity; no forced mismatched symbol load.
3. Revisit the same saved exception/thread and outer call chain. Map the prior
   formatter/receiver/String and saved-register observations to actual available
   functions/types/locals; distinguish real stored values, optimized-out values,
   and reconstruction. Report the earliest boundary this dump can actually prove.
4. If first corrupting writer remains unobservable, say so; do not turn symbol
   availability or fixture success into a production fix. Return precise remaining
   uncertainty and only a concrete evidence-based next hypothesis.

Publish safe public result/commands/exit codes and hashes in this candidate's
C-symbolized-original-20261114 directory. Raw dump/PDB and secret-bearing locals
stay local. No node launch, new TTD/PageHeap run, DB/key/AOEM replacement, cache
reset, repeated fixture sweep or relay send is part of this handoff. D need not
operate. C remains NOT READY until actual node recovery/health separately passes.

## CURRENT 2026-11-13 23:09 Beijing: Phase3 isolated boundary check; C NOT READY

C compared six original875/a254 function bodies, relocation-normalized bytes,
internal branches and core unwind encodings; they agree. A standalone fixture
matched those bodies and passed six live breakpoint cases (48 checks), including
Formatter/receiver/text consistency and observed helper nonvolatile registers.
No original failure reproduced; first bad boundary remains unknown. No evidence
supports a production fix or relay release. No node/DB/TTD operation occurred.

Exact a254 PDB E16D0B11-47CA-4EBF-B482-C91380CC57F8 age1 is still needed from the
original build directory; C's known local symbols do not match. A must not send.
[Phase3 code/ABI comparison, actual fixture values and limits](C-call-boundary-phase3-20261113/README.md).

## PRIOR 2026-11-13 22:48 Beijing: offline object/parameter comparison; C NOT READY

C traced PID3228 to a valid local empty String and Formatter, while the observed
writer receiver points into saved-register slots. PID37796 similarly retains a
local String/text pointer but uses another receiver and invalid memcpy source.
31296 has an inconsistent restore-frame base. These are final-state facts, not
proof of the first corrupting writer or a common FFI/UAF/ABI cause. No production
patch, node/DB operation, TTD or relay. **A must not send.**

A coordination item: locate the original a254 PDB with GUID
E16D0B11-47CA-4EBF-B482-C91380CC57F8, age1 (EXE SHA2561758e4a7...). C checked its
available node PDBs and the original Full-run artifact metadata; none established
the exact match. Do not rebuild a replacement. C independently completed the
available offline analysis; see [phase2 evidence](C-original-dmp-phase2-20261113/README.md).

## PRIOR 2026-11-13 21:14 Beijing: original DMP review only; C NOT READY

C ran Windows Debugger/CDB offline on three original pre-TTD MiniDumps. Exact875
PDB loaded for31296/3228: misaligned MOVAPS restore in indexmap, and a two-byte
String writer using an invalid descriptor.37796 has a254 image-correct address
stack but no exact PDB. Original operands differ from TTD's0xc6. First corrupting
writer and FFI/UAF/ABI attribution remain unproven. No production code change,
node/DB operation, recording or relay. **A must not send.** Complete requested
analyze/exr/context/kv outputs, redaction notes and historical C/D hash comparison
are in [C original DMP review](C-original-dmp-review-20261113/README.md).

## PRIOR 2026-11-13 20:57 Beijing: C recording ended; injected recorder AV, NOT READY

C PID39992 exited0xc0000005 at20:48:17, not forced. Owner/recorder/coordinator
exited0 separately. WER and actual dump locate the fault in injected
TTDRecordCPU.dll+0xdeec8: RAX8, read0xc6. Finalized2GiB guest ring opens but has
zero guest exception events; the recorder pointer slot is absent from captured
memory. This is not proof that the original NOVOVM failure was reproduced or
fixed; first writer remains unknown. Original DB/identity preserved, no restart.
**C NOT READY; A must not send.** No double relay and no automatic diagnostic rerun.
[Actual exits, fault-location checks, recording limits and preserved failures](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/ttd-node-ended/README.md).

## PRIOR 2026-11-13 19:24 Beijing: C node TTD recording active; no relay

PID39992 is running under original LocalService, a254/AOEM966 and existing DB.
Administrator attach was verified before its suspended main thread resumed at
19:19:58. One two-hour/2GiB ring recording is active; original observer33848
failed19:22:41 with missing_novonet and is not restarted. No node AV outcome or
first bad writer yet. **C NOT READY; A must not send**, despite the archived
observer-ready label. Retained trace is private and bounded-tail only.
[Actual start/observer evidence and replay preparation](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/ttd-node-running/README.md).

## PRIOR 2026-11-13 19:19 Beijing: LocalService attach/reverse-write control passed

LocalService recorder rejected recording with 0x80070005; original node was not
started. C is validating elevated **attach** to an isolated LocalService child,
without changing node privileges. Two control harness failures (suspended-child
identity read and cleanup signed exit code) are preserved. V3 cleaned the exact
residual fixture, recorded both LocalService controls and reverse-replayed the
first bad writer; actual guest exits 0/c0000005, coordinator 0. A bounded original
node attach entry is prepared, not yet run. No native AV fix is claimed.
**C NOT READY; A must not send.** No double relay or node retry has occurred.
[Original results and corrected control sources](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/ttd-node-preflight/README.md).

## PRIOR 2026-11-13 18:53 Beijing: isolated reverse-write tracing verified

Fresh user UAC succeeded; both bounded isolated controls were recorded. Clean
exit 0, bad-pointer exit 0xc0000005, coordinator exit 0. Offline replay traced
fixture consumer RVA 0x2bb4 back to writer RVA 0x2b37 and verified the slot's
valid-pointer -> 0xb8 change. This is a diagnostic-tool control, not the node
reproduction/fix. Raw traces stay local; no node/DB/identity operation occurred.

**C NOT READY; A must not send.** First node corrupting writer is still unknown.
[Actual recording, reverse-write proof and tool limitations](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/reverse-write-control/README.md).

## PRIOR 2026-11-13 18:36 Beijing: C traces pointer mismatch; no native fix claimed

C independently confirmed exception/thread contexts and extended the captured
unwind-code comparison. Saved argument pointers differ from the crash-time
RSI; the first writer is still unknown. Official instruction-history tools
were built and a separate faulty-pointer fixture was checked. Recording UAC
returned cancelled: no trace, node restart or observer launch. A corrected
aligned-fixture entry is prepared but has not run.
The initial unaligned fixture failed as Rust abort and is preserved; the
separate aligned fixture passes its direct clean/AV controls, not TTD yet.

**C NOT READY; A must not send.** Original DB/identity/failures remain.
[Pointer evidence, tool provenance and exact limitations](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/pointer-followup/README.md).

## PRIOR 2026-11-13 17:44 Beijing: known fixes pushed; native AV remains unresolved

1f4bd204 contains the snapshot deadline-latch fix and diagnostic DateTime
cleanup correction, with full node check/strict Clippy and scoped regressions.
No candidate was installed. First-chance full dump remains private on C;
selected captured instructions/unwind metadata match disk, but the first bad
parameter/register writer is not identified. A bounded independent30-second
stack-capture experiment completed26,114,540captures without that AV; it is
not exact a254/PageHeap reproduction and does not clear the native fault.

Fresh read-only end state confirms no node/debugger/listeners or residual
PageHeap flags. Stop UAC cancelled and was never executed; no further prompt
or stop is needed for the captured process. Node normal exit code remains
unknown (debugger quit after capture), not replaced by debugger0. **C NOT READY;
A must not send.** C continues local native diagnosis, without automatic rerun.
[Offline evidence and limitations](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/offline-review/README.md).

## PRIOR 2026-11-13 17:25 Beijing: two fixes verified; native AV still open

C fixed the actual snapshot publisher's two-clock-check boundary: one latched
decision now controls network observation and progress emission. The original
reader is unchanged. Original-source regression fails on a254 and passes4/4
after the fix; full node check with iroh-transport, strict Clippy and rustfmt
all exited0. V4 debugger-cleanup DateTime comparison is also corrected and
5cases passed; original failed source retained. No new program was installed.

Original Watch-Relay failed16:59; the same bounded diagnostic later captured
the17:08first-chance AV in ntdll unwinding/heap stack capture. Full dump remains
private on C. This is not yet the first corrupting write or a proven common
cause with prior String faults. C continues independent source/dump diagnosis.
Node/debugger are absent; **C NOT READY, A must not send. No automatic rerun.**
[Verified fix scope and checks](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/known-fixes/README.md).

## PRIOR 2026-11-13 17:08 Beijing: C captured AV; original observer also failed

Original Watch-Relay exited1 at16:59:18 after missing `novonet` for the unchanged
15-second bound. Coordinator exited1; a DateTime/string identity comparison
defect skipped debugger cleanup. Separate stop UAC was cancelled, not executed.
The same instrumented node subsequently hit a first-chance write AV in ntdll
stack unwinding / heap stack-capture. CDB captured and verified a277,824,196-byte
full-memory dump by17:08:53, then quit; debugger exit0 is not node success.
Node normal exit is unknown. No new node was launched. The two-hour window failed.

C owns analysis. The snapshot publisher's duplicate-time-check defect has a
source-level fix and deterministic regression (old fails, fixed4/4); full node
check pending. That fix is not declared the cause/fix of the native memory crash.
Raw dump/log/config remain private. **relay_ready=false; A must not send.**
[Exact original errors, chronology, dump hash and limits](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/README.md).

## PRIOR 2026-11-13 16:56 Beijing: C original-observer fault diagnostic RUNNING

The reissued Windows UAC was actually accepted. LocalService clean/overrun
controls and a13,244,674-byte full-memory dump passed. Original a254/AOEM966
node PID17748 started16:52:00 under CDB12524, original unmodified Watch-Relay
PID37152 at16:52:04, same fresh original progress path. Prior progress was
archived byte-for-byte with hash verification; original DB existing/identity
retained. The original7200-second observation is active, expected through18:52,
with node/CDB bounds18:54/18:56. No fault or terminal result is claimed yet.

**Fault reproduction only: relay_ready=false, A_must_not_send=true.** Do not
send a double-relay object or treat the private original observer READY as a
service acceptance. C owns diagnosis; A/B/D are not operated. The first cancelled
UAC and all earlier failures remain. [Actual controls/startup and sampled liveness](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/README.md).

## PRIOR 2026-11-13 16:45 Beijing: original-workflow crash capture prepared

C prepared a single corrected PageHeap capture with the original unmodified
Watch-Relay/reader and7200-second observation. The node's bounded122minutes
covers startup/window plus shutdown margin; debugger124minutes. The old progress
file is archived and hash-verified before its original path is reused.21local
preparation checks passed; live LocalService controls/full-dump capture and
Windows UAC remain pending. Original a254/AOEM966, DB existing and identities
are retained. No new node or observer has yet started at this checkpoint.

**Diagnostic only: C NOT relay READY. A must not send an object.** No A/B/D
operation is requested. C owns reproduction/source diagnosis, with no automatic
retry. [Exact scripts, bounds and live gates](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/README.md).

## PRIOR 2026-11-13 16:24 Beijing: C PageHeap run ended; fault unresolved

Scope correction from the user's original scenario: **C has already crashed
repeatedly**, and observation attempts have repeatedly failed. The30-minute
diagnostic did not cover the observed77/95-minute crash latencies and did not
run the original Watch-Relay loop. Do not interpret its non-reproduction as
clearing that workflow. [Exact chronology and next diagnostic target](C-source-investigation-20261112/C-first-fault-20261113/SCENARIO-COMPARISON.md).

The single C diagnostic node PID15012 exited at12:14:52.197 Beijing after
about30minutes8seconds. The original CDB process-exit event confirms **target
exit0**, separately from debugger/coordinator exit0. No first AV/verifier stop
was recorded after diagnostic readiness. Classification: bounded non-reproduction,
**not a fix**. The known v2 dump-path limitation and corrected isolated v3 dump
test are preserved. Four exact-source FFI descriptor layouts match; this does
not establish memory safety or the reason A/D have not reproduced the fault.

Owner end checks confirm protected identity/network files unchanged and original
DB `existing`. Fresh read-only collection confirms no node/debugger/listeners
and no residual PageHeap IFEO keys. Raw logs/dumps/config remain private;
original failures remain intact. **C is NOT READY. A must not send a double-relay
object. No observer or node has been restarted.** C retains source diagnosis;
no A/B/D action is requested by this result.

[Original public terminal evidence, actual exit event, limits and remaining gaps](C-source-investigation-20261112/C-first-fault-20261113/README.md).

## PRIOR 2026-11-13 11:56 Beijing: C bounded PageHeap diagnosis, NOT relay READY

C started one instrumented original-node run at03:44:44Z (PID15012/CDB37556),
using a254 + AOEM966 and the original `existing` database/identity. Actual
PageHeap loader enablement and deliberate-overrun trap were observed. One
tool limitation remains in this live run: CDB escaped dump-path backslashes;
fault text is retained but a full dump is not assured. Future tooling has been
corrected and its dump output independently checked, without restarting C.
The run is bounded to30/32minutes; no fault has yet been observed at checkpoint.
This is neither a fix nor permission for A to send a relay object. C owns the
investigation; original failures are retained and A/B/D require no action.
See [actual startup, capture limitation and corrected-tool tests](C-source-investigation-20261112/C-first-fault-20261113/README.md).

## PRIOR 2026-11-13 11:12 Beijing: C crashed again; double relay blocked

PID37796 exited at10:15:56 Beijing with `0xC0000005`, after the one-hour
observer had ended at09:42. C's original dump and owner exit are preserved.
13/14 node return RVAs match the earlier PID27928 crash; the remaining frame
is in the same function. The repeated status/profile/hex/String-copy path
identifies the failure site, not its origin. New AOEM966 did not eliminate it.
The 180-second original-profile isolation passed without reproducing it;
**this is not a fix**. C remains down; no restart or relay observer was launched.

[Original exit, dump hash, stack comparison, collection limitation and current
read-only process/listener result](C-source-investigation-20261112/C-AOEM966-controlled-20261113/backtrace-exit-37796/README.md).
A must not send against the old C READY. C owns the continued source diagnosis.
No A/B/D action requested. Original DB, identity and earlier evidence preserved.

## PRIOR 2026-11-13 08:08 Beijing: C NOT READY; new AOEM run exited

Update 08:41 Beijing: the user explicitly authorized C to investigate/fix
locally and, if needed, run one controlled recovery with backtrace. C started
one new `existing` task with the same a254 executable, AOEM966, original
database/network/identities and `RUST_BACKTRACE=full`. PID37796 bound both
original ports. This is a diagnostic run, **not C relay READY or a fix**.
Old logs/tasks remain intact. C owns the investigation; A must wait for a
fresh relay-ready handoff before sending. Startup evidence and the one-shot
script are in the linked C report directory.

09:16 Beijing checkpoint: same PID37796/creation time, task Running, both
original listeners and stderr length 0 after about35 minutes. These are
process-liveness observations, not an authenticated health query or a fix.
The bounded read-only observer PID27056 started00:41:55Z for up to3600seconds;
on exit it preserves stderr/stdout/exit results without restarting. No relay
observer has been rearmed. The direct AOEM register-preservation isolation
and its deliberate clobber control passed; see `abi-isolation/README.md` in
the C report. The real IndexMap panic remains unresolved.

C continues the local diagnosis. If A still has the original a254 PDB, return
its hash/location for transfer: the fixed executable requests GUID
`E16D0B11-47CA-4EBF-B482-C91380CC57F8`, age1. C already has the older875 PDB;
it must not be substituted as exact a254 symbols. This is an artifact request,
not a transfer of diagnosis ownership or a prerequisite to C's ongoing work.

C's isolated AOEM `9662470d` task exited at 08:07:32 Beijing with node exit
code 1. Stderr reports a `novovm-native-tick` IndexMap out-of-bounds panic.
The read-only observer ended with no node or C UDP listeners; it did **not**
time out. Preserve this failure and do not use the earlier C READY to send an
A→B→C→D object. Do not silently restart C or re-arm an observer. See
[the C terminal result](C-source-investigation-20261112/C-AOEM966-controlled-20261113/README.md).

## PRIOR 2026-11-13: C AOEM-only existing recovery under observation

C used the unchanged a254 node executable, original database, protected
identity and network config with the newly built AOEM `9662470d` Fullmax
Windows runtime. The separate LocalService task loaded the new DLL and opened
the original database with `existing`; PID15612 bound41052/42052 and progress
matched the old height/head at the first two observations. The previous a254
task and its crash evidence remain intact. This **does not yet establish the
cause of the prior crash** or qualify A/B/D for an AOEM update. See
[C controlled runtime comparison and initial evidence](C-source-investigation-20261112/C-AOEM966-controlled-20261113/README.md).

A should query the current C process through the existing authenticated path,
then coordinate a bounded relevant workload. The user has confirmed that
historical crash-root attribution is not a prerequisite for C's continued
test-network use if it stays healthy; see the linked C report. C should retain
crash and health evidence without automatic restart. The four-host version inventory below is
still required to make later synchronized decisions; its historical status
describes the state before this C-only experiment.

## PRIOR 2026-11-13: AOEM version comparison before any new runtime update

AOEM GitHub `main@9662470d` now contains the previously missing
`aoem_compute_result_take_v1` and the FFI ownership correction. C built and
component-checked its Windows Fullmax core, then produced a separate Windows
bundle candidate. **No host has installed this candidate. C node is still down;
the crash root cause remains open.** The older pinned-source ZIP is comparison
evidence only.

[Four-host coordinated read-only version instructions, exact hashes and
Windows/Linux collectors](C-source-investigation-20261112/AOEM-FOUR-HOST-VERSION-HANDOFF-20261113.md)
are the current action. A should pick one common capture window for A/B/C/D
and collect actual loaded-module and installed-manifest reports. B's Linux SO
hash is not expected to equal the Windows DLL hash, but source, ABI and release
identity must align. Do not update, restart or send a relay object merely to
complete this version inventory.

## C owns source diagnosis — 2026-11-12 23:00 Beijing

The user assigned C to investigate locally, without waiting for A's diagnosis.
[New original-dump comparison and pinned-AOEM/profile isolation](C-source-investigation-20261112/README.md)
are complete. The isolation passed but did not reproduce or fix the crash.
C remains NOT READY; no restart, observer or object was launched.

## C BLOCKER 2026-11-12 22:46 Beijing: NOT ARMED; a254 node already crashed

C's one observation launcher stopped before Watch invocation: no active node
or41052/42052 listeners. Original a254 PID27928 crashed at09:50:53Z (17:50:53
Beijing), Application1000 record546473 / c0000005 / VCRUNTIME140.dll+0x1126b;
task Ready/result3221225477 and original existing exit -1073741819 agree.
See [actual C preflight and terminal-file absence](C-double-relay-ready-20261112/README.md).
No READY/liveness/observer PID exists. No restart, update, capture retry or send.
A must not send on old C health or D-only readiness. Recovery/fix remains a
separate next action; the same module/offset is not a confirmed root cause.

## CURRENT 2026-11-12 16:54 Beijing: all four updated; arm C/D original observers

A fetched D1d7714c3, checked15 raw evidence hashes and update exit0.
D a254 PID20876/LocalService, original database/custody/progress unchanged;
A->D authenticated Status38ms, original root unchanged. See
[actual query](A-query-D-20261112/health-public.json). B/A/C/D are now a254.
No object has been submitted in this candidate. Do not repeat any update.

**C and D now:** each start ONE existing Watch-Relay.ps1 with adjacent
Read-RelayProgress.ps1 from the parent audit directory, administrator PowerShell7,
hidden independent process. Check no same-purpose observer is already running;
if one exists, report its actual identity/state rather than killing by broad name.
Use these fixed arguments (HostId C or D):

```powershell
-HostId C -Source a25452f4c49be37cd984e650f2079642e6c273b3 -NodeSha256 1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d -WorkName novonet-observation-a25452f4-20261112 -EvidenceRoot '<fresh-local-evidence-root>' -MaximumWaitSeconds 7200
```

Original observer SHA256 d0dce2979cb51c39edf545637d276a80d56b921bfd194130842b3a475df966d9;
reader SHA256 ccc8f9592a9fb12d68531dc2ace804efb544b9921e4f43c51147f2de57cbcd44.
No source change or new node/network process. Preserve actual stdout/stderr and
exit/result/error. Immediately publish ready.json AND current liveness.json,
observer PID/creation time, node PID and terminal-file presence; do NOT wait for
the2-hour bound. Put receipts under C-double-relay-ready-20261112 or
D-double-relay-ready-20261112 here. Reply C_RELAY_OBSERVER_ARMED /
D_RELAY_OBSERVER_ARMED only if real startup and liveness succeed. On failure
return actual error, no repeat observer/update/initialize. Do not send objects.

A is already elevated; no sender-UAC wait is required. A will check fresh C/D
readiness using the existing guard, refresh B's original progress immediately
before sending, then issue one unique A->B->C->D diagnostic through the original
owner. B/C are opaque forwarding roles, D final receiver. B's original bounded
relay records are read before/after the attempt, not a new socket or service.
The original one-shot sender is parameter-adapted for two ordered --via values;
35s setup observation /15s DONE observation and original cancellation unchanged.
Observer2-hour standby changes no message/session/protocol deadline.

Only ordered route, each actual relay forwarding, sender Object DONE and D
integrity/time/custody correlated to Object/Session/Circuit count as this sample.
This is the original Iroh hop-stream relay/native-object channel, NOT a new
claim of native UDP multihop or outer-IP forwarding. Direct native UDP and prior
single relay remain separate accepted samples; do not repeat them here.

## CURRENT D 2026-11-12 16:49 Beijing: D updated; A query pending

`D_A254_EXISTING_READY_FOR_A_QUERY`: original fixed updater called once, exit0.
Actual old875 PID36464 was healthy/Running, so no recovery switch was used.
New a254 PID20876 / LocalService opened original DB existing, custody/progress
unchanged, correct actual node/AOEM hashes and41053/42053 listener ownership.
See [D public original result/exit and projections](D-update-20261112/README.md).
No initialization, observer, object send or C operation. Do not repeat D update;
A now performs authenticated Status/root verification. Both observers remain
unstarted pending coordination; this local receipt is not double-relay delivery.

## CURRENT 2026-11-12 16:44 Beijing: C accepted; D execute now

A fetched C27efa42c and verified all22 public receipt hashes. C a254 PID27928 /
LocalService opened the original database existing, custody/progress unchanged.
A->C original authenticated Status passed31ms at08:44:02Z, original root
f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333.
See [A actual query](A-query-C-20261112/health-public.json) and its raw response.
The query uses pinned875 operator client; actual C node source is a254.
This is health acceptance, not object DONE; old875 c0000005 remains unresolved.

**D now:** synchronize this receipt, use the already received a254 program ZIP
and existing-update-tools-a25452f4-v1.zip below. Inspect actual875 owner then
execute the already approved original existing updater ONCE with HostId D and
only its evidence-supported recovery switch. No need to start the old version,
recopy a matching program archive, initialize, rekey or reset cache/database.
On success return D_A254_EXISTING_READY_FOR_A_QUERY plus actual public result,
exit code, process/runtime hashes and owner/listener evidence under
D-update-20261112. On failure preserve evidence and stop. C stays running;
neither host starts an observer or sends an object yet. A will query D next,
then coordinate the one real double-relay sample. This supersedes the D-wait
instructions below; no new Full was requested.

## C RESULT 2026-11-12 16:34 Beijing: C_A254_EXISTING_READY_FOR_A_QUERY

C executed the fixed original updater once, exit0. Actual875 PID13020 had
already crashed at02:01:50Z (Application1000 record546395, c0000005); original
task/exit/kernel/port checks supported RecoverCrashed. New a254 PID27928,
LocalService, original DB existing, custody/progress unchanged, ports41052/42052
confirmed. See [C raw public results, exit and hashes](C-update-20261112/README.md).
No old875 launch, initialize, observer, object or D operation. Existing875 crash
cause remains OPEN. A: authenticate C now; D remains package-only until C health
query passes. This local receipt does not claim external authentication.

## CURRENT 2026-11-12: B and A healthy; C execute next, D package-only

User explicitly approved fixed a25452f4 B->A->C->D existing updates and a real
double-relay object. Full43155828234 succeeded on the exact program SHA.
B and A actual original-state recovery and external authenticated Status passed;
see README and raw public receipts. Do not wait for Android/mobile work.

**C now:** synchronize main without overwriting other work; verify the fixed
program ZIP and this small tools ZIP, inspect actual875 owner, then run the
original updater once with the evidence-supported mode. Do not start the old
program merely to stop it again. No initializer, rekey, DB/cache/journal reset.
No observer or object send yet. D may copy/verify only until A validates C.

## Fixed inputs

- Program source: a25452f4c49be37cd984e650f2079642e6c273b3.
- Windows ZIP: novonet-observation-a25452f4-windows-x86_64.zip;
  150573543 bytes; SHA256
  fa198636fa33021ea542850e56f1df17140a5174bfb93fca1ade47f599f14978.
- A copy source:
  C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-a25452f4-20261112\.
- Original C/D receiving directory: E:\区块链四机通讯\.
- Tools in this Git directory: existing-update-tools-a25452f4-v1.zip;
  19935 bytes; SHA256
  a64f3cb94418f97e1f20ba074b47c45e4caf3798b8110ee31c1326340cbc757e.
- Per-tool pins: update-tools-manifest.json. Generated from the original updater
  by Prepare-Windows.ps1; original guard/package tests passed (windows-updater-tests.log).
  Program not rebuilt or changed by this deployment-tool handoff.

Extract to new, distinct program/tools directories, never over a running release.
Check archive entries remain inside the destination and match manifest hashes.
The original updater rechecks all77 release members and source/pins before stop.
Do not use tools pinned to875,8fed,1ef or3fa against this program ZIP.

## Actual875 preflight and execution

Use administrator PowerShell7. C/D current expected old owner:

- Task NOVOVM-C-875d3307-Existing-20261111 (D substitutes D).
- LocalService SID S-1-5-19; no other active node or unknown DB/port owner.
- Old node C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64\bin\novovm-node.exe;
  SHA9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7.
- Original work C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-875d3307-20261111;
  DB sibling chain-state; D substitutes D. Keep its actual run/network/custody/cache.
- Check task result, kernel process state, old progress/exit/Application1000,
  protected-material locations, original ports41052/42052 (D41053/42053).

Call the extracted original Update-Windows.ps1:

```powershell
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -ExecutionPolicy RemoteSigned -File '<new-tools>\Update-Windows.ps1' -HostId C -PackageRoot '<extracted>\novonet-observation-a25452f4-windows-x86_64' -EvidenceRoot '<fresh-evidence-directory>'
```

Choose ONLY the original matching lifecycle mode, based on live evidence:
running exact875 => no recovery switch; Ready/result0/no active process or ports
with original progress => -ResumeStopped; original task-termination evidence
267014 => -RecoverInterrupted; exact supported crash evidence => -RecoverCrashed
(heap c0000374 also requires matching original exit record). Original guards
must accept the evidence. Do not guess a flag, use RecoverMissingProgress for
unrelated data, or weaken validation. Unknown state => return evidence and stop.
Run once; failure => preserve error/recovery-stop evidence, do not loop/reinitialize.

## Return immediately after C finishes

Return C_A254_EXISTING_READY_FOR_A_QUERY only if updater exited0 and result
confirms same-db existing, source a254, custody/progress unchanged. Publish
result.json, public after projection (no private config/custody material),
old-exit / matching stopped/crash preflight, firewall summary, actual exit code,
source/program/AOEM/tool hashes and task/listener state. Put C receipt under
C-update-20261112 in this directory and push main. Do not publish before.json
wholesale: it contains full local configuration and protected-material metadata.

A then queries C with original authenticated Status; only after PASS does D use
the same procedure with HostId D, returning D_A254_EXISTING_READY_FOR_A_QUERY.
Any node/task/path discrepancy is a real blocker to report, not permission to
run an old initialize task or force the database open.

## Double-relay phase

After all four actual versions/health are verified, use the original bounded
double-relay command and existing observation owners. Record ordered two-hop
path, relay forwarding, Object ID, Session ID and sender/final recipient DONE.
New NVLOCR02 signed128KiB declarations require coherent readers; no signed
sequence rollback/cache wipe. No replay of accepted direct/single-relay samples.
Object custody is not Business Receipt, NOV transfer, quorum finality or TPS.
