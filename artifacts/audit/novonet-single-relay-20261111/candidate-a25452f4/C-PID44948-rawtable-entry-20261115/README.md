# PID44948 RawTable 初始化与调用入口复核

## 本轮结果

使用6120ad44已取得的PID44948 Full Dump、原a254 EXE及精确PDB离线复核。
两条RVA0x77823e/0x778245归属status，按该函数帧基址计算，写入RawTable。
insert_full保存XMM的序言位于另一个函数的RVA0x84f071/0x84f076，保存槽地址另列。
两次表初始化前五条指令字节一致，原始调用布局与当前活动帧关系吻合。
本轮没有取得首次产生坏XMM值的历史指令，ABI违规及可修补根因仍未证实。

## 1. 两个栈帧与四条不同指令

同次原始异常上下文RBP为0xab6f9eed00，位于rust_oom主动abort。
由原始.pdata/.xdata及机器码交叉核对的调用者帧如下，CDB离线.frame结果一致。

| 帧 | 函数RVA范围 | RSP | RBP |
|---|---|---|---|
| status | [0x772120,0x77a5f9) | 0xab6f9ef040 | 0xab6f9ef0c0 |
| insert_full | [0x84f050,0x84f46a) | 0xab6f9eef20 | 0xab6f9eefa0 |

| 指令RVA | 所属帧 | 运算 | 目的地址 |
|---|---|---|---|
| 0x77823e | status | RBP+0x1d8，写XMM6 | 0xab6f9ef298 |
| 0x778245 | status | RBP+0x1e8，写XMM7 | 0xab6f9ef2a8 |
| 0x84f076 | insert_full | RBP+0x20，保存XMM6 | 0xab6f9eefc0 |
| 0x84f071 | insert_full | RBP+0x30，保存XMM7 | 0xab6f9eefd0 |

archive_work内层Core位于0xab6f9ef280，RawTable位于Core+0x18，即0xab6f9ef298。
因此前两条是status构造器对RawTable两段内存的写入。
此前提出的0xab6f9ef178/0xab6f9ef188，来自把insert_full的RBP加到status指令的偏移上。
这两个计算值不能用于确定上述status指令的目的地址。

这里的地址归属由原始指令和活动帧关系确定，转储没有提供执行MOVUPS当时的独立RBP快照。
四个16字节区间的当前原始字节、函数范围及unwind保存偏移均记录在entry-dataflow-public.json。
status自身保存调用者XMM6/7的位置为RBP+0x3e0/+0x3f0，也与上述构造区分开。

## 2. archive_work的实际构造及首次插入

该IndexMap及hasher是status栈上的72字节局部对象，其中内层Core占56字节。
初始化过程先写Vec的capacity=0、pointer=8、length=0，再从XMM6/XMM7写RawTable的32字节，最后写hasher的16字节。
原始空表模板在RVA0x7721e6/0x7721f4载入，合法值为`[module_base+0x24f5400,0,0,0]`。
模板首指针具有PE DIR64重定位，原EXE与DMP按ASLR修正后吻合，剩余24字节相同。

构造前的TLS getter调用位于RVA0x7781fd，目标RVA0x1e3a8b0。
从初始化到本次insert调用，有限区间内的调用如下。

| 调用位置 | 目标RVA | 机器码对应行为 |
|---|---|---|
| 0x77825a | 0x181810 | allocation shim |
| 0x778269 | 0x1db3c0 | 为reads_started键申请13字节，alignment=1 |
| 0x7782f8 | 0x189230 | BuildHasher::hash_one |
| 0x778390 | 0x84f050 | Core::insert_full |

初始化至首次insert的status机器码，对Core前56字节的显式直接写入仅有五条初始化存储。
这项静态结果没有覆盖被调用函数内部的意外写入、历史上下文恢复或此前并发交错。
完整有限清单与原始DMP代码页逐字节核对，见archive-init-to-entry.txt。

在0x778390调用前，机器码设置如下。

| 参数位置 | 来源 | 本次对应地址或用途 |
|---|---|---|
| RCX | LEA status.RBP+0x260 | 0xab6f9ef320，返回区域 |
| RDX | LEA status.RBP+0x1c0 | 0xab6f9ef280，Core |
| R8 | hash_one返回RAX | 本次入口瞬时值没有历史快照 |
| R9 | LEA status.RBP+0x120 | 0xab6f9ef1e0，键描述符 |
| [调用前RSP+0x20] | LEA status.RBP+0x380 | 0xab6f9ef440，Value指针 |

当前insert帧的非易失RDI保留Core地址，R14保留返回区域地址，序言保存的R9键指针吻合。
保存的调用者RBP及返回地址0x778395与status帧匹配。
这是指令、存活非易失状态及栈槽的交叉核对，没有把异常时的RCX/RDX/R8/R9当作调用入口值。

## 3. signature_work与archive_work对照

| 初始化操作 | signature_work RVA | archive_work RVA |
|---|---|---|
| capacity=0 | 0x777105 | 0x77821d |
| Vec pointer=8 | 0x777110 | 0x778228 |
| length=0 | 0x77711b | 0x778233 |
| RawTable前16字节取XMM6 | 0x777126 | 0x77823e |
| RawTable后16字节取XMM7 | 0x77712d | 0x778245 |

对应五条指令逐字节相同，均使用status.RBP+0x1c0到+0x1f8的局部位置。
signature_work六次insert目标均为0x84f050，已有清单保留在5462fb19。
发布阶段搬移局部对象并写入JSON字段，随后archive_work复用该栈位。
因此旧signature_work局部RawTable已经被覆盖，完整DMP也不能恢复其构造期间的历史XMM值。
最终发布对象含6字段，不能据此判断XMM6/XMM7当时仍是合法模板。

## 4. 调用约定与保存区检查

本轮对有限原始调用机器码及两函数unwind信息重新核对。
参数使用RCX/RDX/R8/R9及栈上第五位置，status与insert主体RSP均16字节对齐。
insert序言通过movdqa保存XMM6/7/8，.xdata描述的RSP+0xa0/+0xb0/+0xc0与实际指令匹配。
其正常出口RVA0x84f36d/0x84f371/0x84f375恢复同一保存槽。
status的保存位置同样与.xdata匹配。这些布局符合所检查的Windows x64保存规则。[Microsoft x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention?view=msvc-170), [Microsoft x64 exception handling](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64?view=msvc-170)

本次按内部Rust单态函数的实际机器码记录参数和保存行为。
该直接调用区间没有已证实的SysV转换跳板，历史动态目标、系统DLL内部恢复值及实际异常展开仍缺执行记录。
当前保存槽的坏值与表字段相同，按正常序言路径支持入口寄存器已含坏值的推断。
转储没有独立的入口快照，也不能证明历史保存槽从未受到其他错误写入。
本轮无法据此确认跨ABI违规或Clone恢复失败。

## 5. 两次自然故障的证据范围

PID3848与PID44948均在archive_work首次reads_started插入、同一Core入口和reserve_rehash路径产生36TiB＋16字节申请。
两份表字段形状均为`[8,指针状mask,0,指针状items]`，对应当前XMM6/7及当前insert保存区。
两次的具体指针数值不同，尚无首次异常产生指令。
PID44948传输记录83条、容量128、淘汰0，PID3848最终记录255条。
满载和记录淘汰不是本次故障的必要条件。过期活动仍存在，其因果关系未证实。
无调试器自然复现确认原始运行也会发生此类故障，软件与硬件的根因归属仍需直接证据。

## 6. PID20952管理员只读核验

创建时间2026-11-15T10:12:37.1419770Z，父PID44948，与预期核对一致。
管理员WMI所有者查询成功，归属NT AUTHORITY\LOCAL SERVICE。
WMI映像路径为空、命令行未取得，外部性能记录仍为线程0、句柄0、CPU0。
同轮Get-Process无法获取该PID，进程对象/映像归属尚未完全确认。
原运行任务已移除。
Application Error事件547275发生于10:12:37.4968900Z并记录父进程，WER事件547276发生于10:12:40.7298407Z。
现有事件未直接记录子PID20952，不能仅据时间关系将它归为已确认的WER快照。
本轮仅查询，未对该条目执行清理。详细字段见child-admin-public.json。

## 证据缺口与修复结论

已经确定静态初始化存储指令、正常帧对应目的地址、当前坏表字节及当前保存区的关系。
最早缺失的动态数据仍位于合法模板载入之后、archive初始化之前，包括signature构造中的实际调用返回和之后候选调用。
具体缺少同一次status的入口标识、各实际返回边界XMM6/7、MOVUPS执行当时RBP及寄存器值、已返回子调用的原保存槽。
初始化之后至insert序言之间，也没有完整瞬时寄存器和意外写入历史。
因此本轮没有可证实的修复补丁，未修改Hashbrown、String、分配器或生产源码。
原库、身份和失败现场保留，原节点本轮未启动，双中继继续暂停。
