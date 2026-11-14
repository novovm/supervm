# a25452f4: approved existing updates, then real double relay

## CURRENT 2026-11-14：同次RBP动态采集尚未就绪，不重跑旧隔离样例

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
