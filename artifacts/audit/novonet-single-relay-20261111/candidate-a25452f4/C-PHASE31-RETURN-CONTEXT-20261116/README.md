# Phase 31：PID38324 输出7 / RSI0 的异常前后分界核对

基于25272f43。全程离线，未启动C、未改生产代码、未访问原库/身份、未覆盖转储、未开发或启动采集器。仅新增离线解析、选定系统处理路径反汇编及脱敏结论。

## 本轮结论

仍未找到首次破坏，不能在“最后写入未按预期完成”“原异常上下文变化”“故障后内存改写”之间唯一选择。本轮找到并正确区分了分派工作上下文，核对完整扩展异常帧范围，解析实际登记的VEH及共同C++帧的AV分支，从而降低几种具体正常处理机制作为解释的可能性；没有据此扩大为所有处理器、线程或历史写入均已排除。

必须保留一个语义边界：7是尚未完成返回的输出缓冲区当前字值，不是已经被调用者消费的返回索引。故障位于恢复寄存器阶段，RET尚未执行。

## 1. 最后写入为何仍不能直接判定“漏写”

PID38324保持Phase30的同次证据：R14=0x3e48fee360，异常RSI=0，转储[R14]=7；Map桶/条目支持索引0。0x84f359为`49 89 36 / mov [r14],rsi`；0x84f36d为故障恢复指令，相距0x14（20）字节。

若新增条目路径确实以当时相同的RSI/R14顺序执行，且之后没有其他写入，[R14]应为0。观察到7说明这些前提至少有一项不能由终态直接成立，但不能反过来证明0x84f359被CPU跳过、没有完成或写错。

PID13020在相应位置是RSI=4、[R14]=4。它提供对照，却不提供PID38324在写入时刻的值。另一条替换键分支的捕获R13也不等于7，但其更早真实分支/调用历史仍未记录。

两份CONTEXT中的LastBranchToRip、LastBranchFromRip、LastExceptionToRip、LastExceptionFromRip均为0，没有最后写入的独立分支记录。转储不是指令时间线，不能用当前RIP替代先前写指令实际执行证据。

## 2. 已厘清：原异常上下文与系统搜索副本是不同对象

从同次RtlDispatchException活动帧及捕获机器码建立指针关系：入口将RDX原上下文保留在R13；分派器为R14创建另一份扩展上下文，再以RCX=R14、RDX=R13调用RtlpCopyContext。后续虚拟展开使用工作副本，frame handler参数仍可收到原上下文指针。

| 角色 | PID38324 | PID13020 |
|---|---|---|
| 原异常上下文 | 0x3e48fedb00 | 0x3d4f8ede80 |
| 原RSI | 0 | 4 |
| 与ExceptionStream完整1232字节一致 | 是 | 是 |
| 分派搜索工作副本 | 0x3e48fed400 | 0x3d4f8ed780 |
| 工作副本最终RIP / RSI | 0 / 0 | 0 / 0 |
| 工作副本RSP | 0x3e48fff9d0，已到线程根 | 0x3d4f8ffd60，已到线程根 |

工作副本在PID13020中RSI从原记录4变为0，是虚拟展开到父/线程根的上下文状态，不能解释为原故障RSI被清零。此前按有效模块RIP筛选CONTEXT的扫描会排除RIP=0的这个副本；本轮通过分派器参数/寄存器绑定识别，未把筛选未命中当作对象不存在。

关键限制：ExceptionStream与进程内源CONTEXT一致，说明本轮没有混用两种角色、解析字段或地址；它们是同一来源的转储时刻副本，并不是“进入异常分派前”和“分派后”的两次独立观测。因此一致本身不能证明原CONTEXT未被更早处理器修改。

## 3. 完整CONTEXT_EX范围：普通复制目标不覆盖输出

本轮解析All/Legacy/XState偏移及长度，核验Legacy.Offset=-0x4d0、Legacy.Length=0x4d0等结构条件，包含完整声明扩展区间，不只检查1232字节基础CONTEXT。

| 区间（尾端不含） | PID38324 | PID13020 |
|---|---|---|
| 原完整扩展异常帧 | [0x3e48fedb00,0x3e48fee210) | [0x3d4f8ede80,0x3d4f8ee5a0) |
| 原异常帧末端 | 恰为故障RSP | 恰为故障RSP |
| 工作上下文完整声明区 | [0x3e48fed400,0x3e48fed8f0) | [0x3d4f8ed780,0x3d4f8edc70) |
| 未返回的输出缓冲区 | [0x3e48fee360,0x3e48fee3b0) | [0x3d4f8ee6f0,0x3d4f8ee740) |

所有上述上下文复制区都不覆盖输出或当前XMM6保存区。按这些已验证范围进行的正常有界复制，不足以解释输出索引变7。普通故障后调用的栈向低地址分配，也不能单独作为覆盖故障RSP以上输出缓冲区的证明。

该范围结论不等于没有野指针、错误长度、其他线程写入或此前已消失的别名。没有取得写入输出缓冲区的历史记录，不能宣布“故障后改写已排除”。

## 4. 原上下文可能经过的具体处理路径

### RtlpSanitizeContext

捕获的完整函数与同机NTDLL参考字节一致。通过原上下文基址的显式写入都在ContextFlags偏移0x30，未发现写RSI偏移0xa8的指令。不能把这个标志清理过程当成已找到的RSI 7→0写入。

### 当前实际登记的VEH

从PID38324捕获的LdrpVectorHandlerList和指针解码逻辑恢复到一项VEH，目标novovm-node RVA0x1e37a80；VCH列表为空。编码cookie只在本机解析，不公开。

该回调完整捕获字节与原EXE一致。它读取异常记录，比较0xc00000fd（栈溢出）；本次异常0xc0000005走jne至0x1e37ad7，xor eax,eax后直接ret，返回VEH的EXCEPTION_CONTINUE_SEARCH=0。在这条AV路径上没有读取ContextRecord指针，也没有写原RSI或输出对象。只看到的栈写入是回调自身的序言/局部状态。

这排除了“该捕获回调按当前AV分支主动把RSI改为0或把输出写7”这一具体解释，不能证明该回调在某时确实调用过，也不能排除以前已移除的回调。PID13020缺少列表/cookie所需数据页，未套用新现场的登记列表。

### 六个共同活动帧的C++EH入口

没有假定__CxxFrameHandler3忽略原ContextRecord：其包装器确实把原R8上下文继续传入内部入口。本轮按VCRUNTIME导出表和捕获代码定位实际包装器RVA0x102a0、转发0x3014→内部0x2b68、上下文栈界检查0xea90。

六帧捕获FuncInfo均与原EXE一致：magic19930522、try_count0、exception specification offset0、EH_flags1；本次记录为AV/flags0。

- 若PTD+0x40为0：内部例程排除C++异常/longjmp/consolidate后，在0x2be3检查EH_flags1并跳至0x2d61，返回frame disposition1。
- 若PTD+0x40非0：记录无unwind标志，随后try_count0、无异常规范且EH_flags未设置掩码0x4，也返回disposition1。

该上下文栈界检查只读取Context.Rsp，未通过原上下文指针写入。给定上述当前输入/元数据，这些普通AV分支继续搜索，不进入选定帧的catch/cleanup路径；没有出现所需的RSI或输出索引写入。frame disposition1与VEH return0是不同枚举，在这里都表示继续搜索。

这仍是捕获代码和数据对可行分支的限定，不是逐次调用历史，也没有清除所有外部处理器、间接调用、其他线程和底层异常原因。

## 5. 为什么缺少更早的独立基线

RtlDispatchException入口在VEH之前有可选异常日志路径。本轮检查两份现场：PEB.NtGlobalFlag均为0，日志位0x800000未设置；RtlpExceptionLog2为null，计数0。没有可用的这类独立“VEH前上下文”日志；本轮没有开启它或启动节点。这里是转储时刻状态，不扩展为整个运行期间从未改变过配置的证明。

原始异常记录、内存中的源CONTEXT、最终输出缓冲区和系统工作副本都没有各自的写入时间戳。原CONTEXT即便曾变化，或输出7在异常之后被写入，现有同一时刻副本不能恢复其前值。CDB的用户异常帧标签也不等于获得了内核陷阱号/错误码或真实故障前寄存器历史。

## 6. 三种解释的当前状态

| 解释 | 本轮支持/限制 | 判断 |
|---|---|---|
| 最后写入未按预期完成 | 按不变寄存器和顺序执行应写0；没有写入时刻记录或有效LBR | 未证实，不能指定0x84f359为首次破坏 |
| 原异常上下文发生变化 | 工作副本变化已解释；同源副本相同不证明历史未变；选定VEH/C++AV/清理标志路径未找到RSI写入 | 未证实，仍缺原上下文进入分派前基线 |
| 故障后输出内存被改写 | 普通完整CONTEXT复制区与输出分离；选定回调/帧AV路径无直接写7 | 未找到具体写入者，未排除任意别名/其他线程/未记录历史 |

因此本轮没有把7归因于WER，也没有把RSI0归因于系统虚拟展开。调查重点仍是输出写入与异常上下文/后续内存的时间关系。只有对应写入前后或进入分派前的独立证据，才能进一步唯一判定；本轮不为补这个缺口开发新采集器或安排运行。

原库、身份未访问，C未重启，没有产品修复。原始转储和历史失败现场保持，分析前后哈希匹配记录见preservation-public.json。原始寄存器、系统数据及日志留本机target/c-phase31-return-context-20261116/analysis-private；公开Git只含脱敏结论和结构。

参考：[Microsoft虚拟展开上下文语义](https://learn.microsoft.com/en-us/windows/win32/devnotes/rtlvirtualunwind2)、[VEH回调与返回值](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nc-winnt-pvectored_exception_handler)、[官方VEH修改上下文示例](https://learn.microsoft.com/en-us/windows/win32/debug/using-a-vectored-exception-handler)。以上文档用于解释机制；具体分支和地址来自本次捕获字节。
