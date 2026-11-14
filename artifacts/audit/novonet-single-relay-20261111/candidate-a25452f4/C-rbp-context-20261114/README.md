# C：PID34452 同次 Full Dump 的 RBP 专项离线核验

基于 `efb3fbd5`，只分析原 SHA256
`7d910fa6713e399c6bb394f2201fec622771dbc8e0665894ce9b8c115c9231aa`。
没有启动节点、访问原库/身份、跑普通样例、使用 TTD 或修改生产代码。
**首次改变 RBP 的指令未定位，尚无可提交的生产修复；双中继继续暂停。**

## 后续定点核对：入口推算和候选分支状态

继续基于同一文件，没有重启、运行样例或触发新故障。新增公开结果为
`context-branch-public.json`、`instructions-branch-public.json`、`branch-verification-public.json`，
原来的结果和失败日志保留。

### 入口基线是推算，不是已记录的入口寄存器

七次 push 共 `0x38` 字节，加 `0x150` 局部空间，从当前 RSP 推得：

```text
入口 RSP                         0x7739fee008
入口 RBP 的当前保存槽内容         0x7739fee130
七次 push 后的 RSP               0x7739fedfd0
lea rbp,[rsp-0x50] 应建立的 RBP    0x7739fedf80
sub rsp,0x150 后的 RSP            0x7739fede80
失败时 RBP                       0x7739fee220
```

失败 RBP 比本帧预期高 `0x2A0`，但比保存的入口 RBP 高 `0xF0`。
**0x2A0 是这两个现场值的差，不证明曾执行 `add/sub rbp,0x2A0`。**
加载了错误指针、恢复了错误槽或其他状态变化也可能产生这个差；不能按常数搜索命中来归因。
没有入口时刻记录，不能把上述推算包装为实际入口采集。

### 降低三个候选分支的优先级，但不宣称历史排除

已直接从同次 Memory64 读取并核对读取/分支机器码：

| 条件 | 本次最终内存 | 对候选的意义 |
|---|---|---|
| heap+0x10 类型签名 | `0xFFEEFFEE` | 不等于 `0xDDEEDDEE`，按此值不会走 `RtlpHpReAllocateHeap` 分支 |
| 堆栈跟踪标志 | `0` | `RtlpHpStackLoggingEnabled` 的最低两位检查不能通过；按此值不会进入随后 RemoveStack/AddStack 路径 |

此前列出的这三类会暂用/恢复 RBP 的实现，**不能只因存在 RBP 写指令就当作这次实际肇因**。
转储没有更早的条件读取历史，因此也不能证明它们在整个运行期间从未执行。不能用最终值
强行重建先前分支，更不能把这个筛选扩展成“Windows 堆调用链无错”。

### 本次补查指令的范围

从准确函数边界补查节点 RVA `0x84ed90`、`0x84f050` 的完整静态反汇编，仍只有序言 LEA
和尾声 POP 显式写 RBP；没有找到把 RBP 增加 `0x2A0` 的指令。标准 ntdll memcpy 当前
反汇编体也无显式写 RBP，不能因此证明其所有输入或全部传递状态正确。
相关机器码已逐指令与原 DMP 字节比较；结果记录的是静态集合，不是执行顺序。

原栈有限范围中的值搜索只找到错误 RBP 数值的两个位置：`0x7739fedd78` 和
`0x7739fee1b8`。未发现该范围内存在 `0x7739fedf80` 的对齐 8 字节副本。
**数值相同不证明这些槽被用来恢复 RBP**；下方栈也可能已被故障报告路径复用，不能据此
选中硬件写断点或宣布找到首次写入位置。原始搜索记录仅留本机。

有一次局部 `u` 请求从非指令边界开始，出现 `???` 和无效解码；该输出原样保留，
未纳入指令证据。后续改从实际函数边界 `uf` 获取并逐字节核验，不用错误解码制造候选。

本轮结论：候选优先级进一步收窄，**首次改变指令仍未取得**。在“不运行故障环境”的
当前约束下，能做的是静态/快照关联，不能把它升级为不存在的指令历史。
下一次具有因果价值的证据仍须包含同一活动调用中最后正确与首次错误的 RBP，以及该次
恢复源槽/指令前后状态；尚未选择出可证明唯一有效的监视点，本轮不安排盲目复现。

## 六个问题的直接回答

| 问题 | 同次现场的结论 |
|---|---|
| RBP 是否仅是 WinDbg 展开误判？ | 不是仅由 `kv` 推导。DMP ExceptionStream 原始 CONTEXT 就是 `0x7739fee220`；与 ntdll 保存的报告上下文一致，报告入口保存的失败检查参数也独立对应这个错误槽。 |
| 是否未经处理的 CPU 陷阱上下文？ | **不是。** 此失败经 `RtlCaptureContext` 捕获，再由 `_report_gsfailure` 对自身帧做 `RtlVirtualUnwind`，构造失败报告。必须与原始硬件陷阱和 WER 等待线程上下文区分。 |
| 0x2A0 从何而来？ | 故障 RSP=`0x7739fede80`；序言应使 RBP=RSP+0x100=`0x7739fedf80`；实际 RBP=`0x7739fee220`，差 `0x2A0`。 |
| 哪个函数应维护？ | `RtlpReAllocateHeapInternal` 建立自己的 RBP 帧关系；它的被调用者按 x64 非易失寄存器约定保留 RBP。外层 Rust 调用者的保存值不能代替当前函数自己的帧指针。 |
| 保存值是否已被覆盖？ | 当前检查的三个 RBP 保存槽与各自调用者帧关系一致，未发现不一致；但无入口历史，不能证明它们从未被改动。正确 cookie 槽也匹配，不代表整个栈完好。 |
| 是否找到首次异常写入？ | 没有。异常报告证明使用了不一致的 RBP；完整 DMP 仍不含已经返回的子调用执行历史、当时寄存器值或首次写入记录。 |

## 1. 三种寄存器来源严格分开

直接解析 DMP 文件 RVA `0x565a` 的异常 CONTEXT，不调用 WinDbg 展开器：
RIP=`0x7ffce31aa114`，RSP=`0x7739fede80`，RBP=`0x7739fee220`。
RBX/RSI/RDI/R12–R15、其余通用寄存器及 XMM6–XMM15 已完整提取到本机受保护 JSON，
不在公共仓库发布任意寄存器/向量内容。其所有提取字段与 ntdll 地址 `0x7ffce33228e0`
的报告 CONTEXT 相同。

ThreadList 中同一线程的另一份上下文为 RIP=`0x7ffce32b6f34`（等待函数），
RSP=`0x7739fedb38`、RBP=`0x4b8`。这是 WER 捕获期间的等待现场，不能拿来替代异常报告，
更不能将二者的差值当作首次破坏。

进一步核对报告生成机器码：

- `_report_gsfailure` 入口 `0x7ffce32768b0` 把检查失败时的 RCX 保存至 `[entry RSP+8]`。
  对本次调用，它对应 `0x7739fede80`。此槽值与 `[异常RBP+0x30] XOR 异常RSP` 相同，
  与正确 cookie 不同。它不是 `kv` 的计算结果。
- `RtlCaptureContext` 在 `0x7ffce3276c50` 直接执行 `mov [rcx+0xa0],rbp`。
- 报告函数调用一次 `RtlVirtualUnwind` 还原自身帧，然后恢复报告 RCX、填充异常信息。
  自身 unwind 字节为 `01 0c 02 00 0c 01 11 00`，只描述 `0x88` 栈分配，无 RBP 保存/恢复项；
  所查报告函数和捕获函数本身没有显式写 RBP 的指令。

因此，**“错误值只是离线 WinDbg 因缺 .pdata 算出来的”不能解释上述相互吻合的原始内存**。
这仍不证明 Windows 报告路径或硬件在任何条件下绝对无误，只排除了将本次观察完全归因于
离线栈显示的解释。先前“原始异常上下文”应理解为 DMP 保存的报告上下文，而非裸硬件快照。

## 2. 正确 cookie、当前 RBP 与保存的调用者 RBP 是三件事

从 DMP Memory64 直接核对序言、检查代码和 unwind：

```text
RtlpReAllocateHeapInternal entry:
    push rbp, rbx, rdi, r12, r13, r14, r15  # 七次独立 push
    lea rbp,[rsp-0x50]
    sub rsp,0x150
    ... store (module_cookie XOR rsp) at [rbp+0x30]

return check:
    mov rcx,[rbp+0x30]
    xor rcx,rsp
    call _security_check_cookie
    add rsp,0x150
    ... pop other nonvolatile registers
    pop rbp
    ret
```

| 栈位置 | 本次值的关系 |
|---|---|
| `RSP+0x130 = 0x7739fedfb0` | 正确 cookie 槽；与模块 cookie XOR RSP 匹配 |
| `RSP+0x180 = 0x7739fee000` | **保存的是调用者 RBP**，值 `0x7739fee130`；不是当前函数应持有的 `0x7739fedf80` |
| `实际RBP+0x30 = 0x7739fee250` | 实际错误检查槽；XOR RSP 后对应失败检查参数 |

cookie 与保存 RBP 的槽相距 `0x50`，不能把两者混为一个保护点。
函数体已反汇编的指令逐字节与 DMP 校验，显式写 RBP 只有：

1. `0x7ffce31a9dfc`：序言 `lea rbp,[rsp-0x50]`。
2. `0x7ffce31aa125`：尾声 `pop rbp`。

失败报告在检查调用后的 `0x7ffce31aa114`；该尾声 pop 尚未执行。
因此不能把它判为“本函数已从自己的保存槽恢复了坏 RBP”。中途被调用函数的恢复、
更早的局部覆盖或其他状态改变仍须靠实际执行证据判断。

## 3. 调用者、尾调用与候选 RBP 写入

按真实返回地址、函数机器码和 unwind 核对以下保存槽：

| 保存槽 | 当前内容 | 调用者计算 |
|---|---|---|
| `0x7739fee000` | `0x7739fee130` | 节点 RVA `0x84ed90` 帧底 `0x7739fee0b0` + `0x80` |
| `0x7739fee1a0` | `0x7739fee230` | 节点 RVA `0x84f050` 帧底 `0x7739fee1b0` + `0x80` |
| `0x7739fee2c0` | `0x7739fee350` | 节点 RVA `0xa15c10` 帧底 `0x7739fee2d0` + `0x80` |

五个实际返回地址与对应栈间距一致。这里描述的是最终快照中的一致性，**不是入口/退出
历史证明**。节点 RVA `0x84ed90` 的最近符号为另一函数名+偏移，仍以实际函数表边界标识，
不把最近的 `tracing_core` 名字当作源码根因。

`finish_grow` → `__rust_realloc` → `__rdl_realloc` → HeapReAlloc 存在正常尾调用：
`__rdl_realloc` 在尾跳前撤销自身帧、恢复 RBP；因此它不出现在最终物理调用栈中。
这不是调用链缺失或 ABI 错误证据。最终 `RtlpReAllocateHeapInternal` 保存的调用者 RBP
也与外层帧一致，不能据此指控这个尾调用传坏了 RBP。

当前检查范围内，`RtlReAllocateHeap`、`RtlpReAllocateHeap`、
`RtlpCallInterceptRoutine` 和 `RtlpHpStackLoggingEnabled` 没有显式写 RBP；
**它们的子调用并未因此被证明无错**。另外的可能分支中发现合法的 RBP 暂用/恢复：

- `RtlpHpReAllocateHeap`：保存于入口 RSP+0x10，临时用作 heap 参数；尾声从同一槽恢复。
- `RtlpHpStackTraceRemoveStack` 跳入的实现：`xor ebp,ebp`，再从自身保存槽恢复。
- `RtlpHpStackTraceAddStack`：暂存全局 tracking context 指针，再从自身保存槽恢复。

这些是**静态候选**，没有同次分支历史，不能认定本次执行过，更不能将某个 restore
地址标为首次错误指令。现有证据未发现这些保存/恢复偏移本身不配对。
原先 AOEM 汇编缺少部分 .pdata 的问题不在这段已核实的报告/堆函数展开链上，不能套用为解释。

## 4. 下一次取证必须满足的具体条件（本轮未执行）

**不能现在盲目对 `0x7739fee000` 设置写断点。** 那是原函数保存调用者 RBP 的槽，
当前值一致，且其 pop 未执行；它不是存放当前帧 RBP 寄存器的内存镜像。
对它加监视不能保证抓到直接修改寄存器或子调用错误恢复。

如后续批准受控捕获，应先明确要区分的两种机制：

1. 若定位到某个确实活动的子调用保存槽，才针对该次调用的准确线程、栈范围和保存完成后的
   8 字节地址评估硬件写断点；入口正常保存不算破坏，返回后栈槽失效即撤销监视。
2. 直接写寄存器不能靠内存数据断点捕获。需针对已缩小的候选恢复指令，保留执行前后的
   RBP、RSP、源槽及代码；本次尚未证明哪个候选分支实际执行，不能假定已选定唯一断点。

地址随下一次栈/模块布局变化，禁止复用本次绝对地址。不能同时给所有高频堆调用加逐边界
断点，不能回到此前严重改变时序的方案。本轮只评估，未设置断点、PageHeap、TTD或启动节点。
**还缺的是同次失败中，最后正确 RBP 与第一个错误 RBP 之间的真实调用/恢复记录。**
最终转储保留更多内存，仍不能补出不存在的执行历史。

## 复核和数据边界

- `Inspect-RbpContext.py`：直接解析 ExceptionStream、ThreadList、ntdll 保存上下文、
  三个保存槽及五个返回地址；完整寄存器仅写已有受保护目录。
- `Audit-RbpInstructions.py`：用本机已有 Capstone 解码现有离线反汇编，并逐指令对照 DMP
  字节；输出只含静态代码位置，不声称这是运行历史。
- 两脚本实际退出 0；四次新增离线 CDB 退出 0。非故障的 PowerShell 展示命令第一次因管道
  语法错误退出 1，修正后只读展示成功；未因此重跑节点或诊断样例。
- 原转储、原日志和完整寄存器留本机，Git 仅同步公开比较、静态指令及摘要。A签收未确认，
  也不作为 C 继续调查的条件。

API语义依据：[MINIDUMP_EXCEPTION_STREAM](https://learn.microsoft.com/en-us/windows/win32/api/minidumpapiset/ns-minidumpapiset-minidump_exception_stream)、
[RtlVirtualUnwind](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nf-winnt-rtlvirtualunwind)、
[Windows x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention?view=msvc-170)。
这些文档只支持结构和调用约定；具体故障结论来自本次原始转储及机器码，非文档推断。
