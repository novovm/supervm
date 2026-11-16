# Phase 30：PID13020 / PID38324 共同路径、别名与非局部恢复对照

本轮新增重点是PID38324返回对象与异常上下文的不一致。没有定位首次破坏，没有修改生产代码。全程离线；C未由本轮启动，原库/身份未访问，所有原始转储只读，双中继未恢复。

## 1. 已收窄的共同路径

本轮给PID13020补上匹配875 EXE/PDB后，得到有效活动栈：

`status → profile快照 → collect_seq → Value::serialize → SerializeMap::serialize_value → insert_full`。

这与PID38324的活动路径相同；两者都在insert_full+0x31d、正常返回恢复阶段停止。这里不再重复Phase29的对齐、序言、尾声和unwind一致性检查。后续数据检查只围绕这六个活动函数、当前参数对象、追加条目helper及其EH元数据。

## 2. 新发现：返回对象与捕获寄存器不一致

| 项目 | PID38324 | PID13020 |
|---|---|---|
| 故障前写返回索引的指令 | RVA0x84f359：`49 89 36 / mov [r14],rsi` | 对应RVA0x65f589 |
| 故障RVA | 0x84f36d | 0x65f59d |
| 捕获的RSI | 0 | 4 |
| 返回对象地址R14 | 0x3e48fee360 | 0x3d4f8ee6f0 |
| 转储内存中的返回索引[R14] | **7** | **4** |
| 捕获RAX是否等于R14 | 是 | 是 |
| 输出后续tag | 0x8000000000000005，与原代码写入的None tag相符 | 同左 |
| 目标Map长度 | 1 | 5 |
| 目标Map当前键 | seq，密集条目索引0 | heap页缺失，不能读取 |

PID38324的目标Map进一步核验：桶索引15的桶项为0，与RSI=0相等；控制字节与捕获hash高7位一致；密集条目0的hash也与捕获hash相同。Map当前结构支持新条目索引0，而返回对象的7与之不一致。

两份转储的返回索引都用独立CDB `dq` 与寄存器读取核对，结果与原始字节解析相同。PID38324的现存返回值也不等于另一条“替换已有键”路径使用的捕获R13=120；不能只用选择另一条前驱分支消除该矛盾。

具体局部核对切片为：

```text
0x84f354  call 0x84ed90       ; 追加条目helper
0x84f359  mov [r14],rsi       ; 返回索引写入
0x84f35c  mov rax,8000000000000005h
0x84f366  mov [r14+8],rax     ; None tag
0x84f36a  mov rax,r14
0x84f36d  movaps xmm6,[rbp+20h] ; 故障
```

最后索引写入至故障指令相距0x14（20）字节。这个切片是新的“一致性检查点”，不是已经观察到的首次破坏窗口：转储不能证明0x84f359在历史中按当前寄存器执行过，也不能排除故障后改写返回对象、跳转/上下文恢复或更早状态变化。不能宣布该指令写坏，更不能因7像上一轮索引就认定它是旧值。

这也修正了可做的推断范围：不应只凭异常RIP已经到恢复指令，就默认前驱路径上的每条返回值写入已正确完成。

## 3. 指针别名写入：当前对象没有指向保存槽

| PID38324区间 | 当前地址范围（尾端不含） | 与XMM6保存区相交 |
|---|---|---|
| XMM6保存区 | [0x3e48fee2b0,0x3e48fee2c0) | 基准 |
| 输出对象 | [0x3e48fee360,0x3e48fee3b0) | 否 |
| 输入拥有键 | [0x3e48fee400,0x3e48fee418) | 否 |
| 输入Value | [0x3e48fee460,0x3e48fee4a8) | 否 |
| 目标Map Core | [0x3e48fee580,0x3e48fee5b8) | 否 |
| 密集条目容量区 | [0x140d9e111f0,0x140d9e11530) | 否 |
| RawTable声明分配区 | [0x140d9e14170,0x140d9e14210) | 否 |

PID13020的同类描述符区间也不与其XMM6槽[0x3d4f8ee640,0x3d4f8ee650)相交。它是MiniDump，缺失heap内容，不能据此验证其实际条目和桶内容；描述符声明区间不相交与heap内容已核验是两种不同证据。

追加条目helper的写入布局直接由机器码确定：条目步长104，键位于+0、Value位于+24、hash位于+96。初版离线解码曾错把键从+8读取，修正后seq在条目0得到匹配；这只是离线解析修正，没有改动原始内存或程序。

对两份转储的六个共同活动帧连续区域，各扫描0xe00字节中对齐的8字节指针，未发现落入本次XMM6保存区的指针值；扫描区没有缺页。这不等于没有所有可构造别名：寄存器持有、基址加索引计算、非8字节对齐指针、其他线程、未捕获heap及已经消失的旧指针均不在排除范围内。

当前对象和声明分配区能降低“正常容器追加目标直接覆盖XMM6槽”的嫌疑；它们不能排除历史野指针写入，也不能证明返回索引7是何时形成的。

## 4. 非局部上下文恢复与EH清理

两次故障的ExceptionRecord都为AV、flags=0、chained record=0；活动后续栈均显示当前异常经KiUserExceptionDispatch → RtlDispatchException → 线程根过滤器/UnhandledExceptionFilter → WER等待。该路径是故障后的分派/报告，不是故障前NtContinue或错误RtlRestoreContext的执行证据。

六个共同活动函数的C++EH FuncInfo均为`__CxxFrameHandler3`，本地try-block数量均为0。对直接call可解析的导入跳板，未命中选定的NtContinue/RtlRestoreContext/SetThreadContext/longjmp/setjmp/RaiseException/Unwind导入。这一检查不涵盖全部传递callee、间接调用或过去已经结束的函数，不能推广成“没有非局部恢复”。

新检查的是EH状态变量是否与对象/保存区发生别名，而非重做unwind槽验证：六帧各自FuncInfo的dispUnwindHelp均与其发出的`mov [frame offset],-2`状态标记匹配；每个状态槽都不覆盖XMM6保存区或输出对象。两个现场均如此。

本轮检查了前三个紧邻函数的全部cleanup action：insert_full清理输入Value/键，serialize_value清理其键/Value，Value::serialize清理局部Map/键资源。它们按传入父帧基址访问各自对象，没有得到指向输出索引或当前XMM6保存区的直接证据。本次故障RVA0x84f36d处于正常恢复序列，并不是insert_full cleanup funclet的另一条XMM恢复指令0x84f4a9。cleanup存在不证明它已执行。

profile/status的EH状态数较多，本轮只列出action表前40项，未审查其全部清理函数；交付JSON明确标为不完整，不声称已排除所有异常处理路径。

对每次故障选定的线程栈范围，使用本次ContextFlags、有效CS/SS及同线程栈RSP筛选CONTEXT，只找到与当前故障上下文完全相同的一份。未找到第二份可以证明先前恢复目标的完整上下文。该筛选不覆盖不同标志、其他格式的跳转缓冲区、heap/其他栈保存区、已擦除历史，因此“未找到”不能作为故障前未发生非局部恢复的证明。

Microsoft文档说明RtlRestoreContext可根据异常记录和跳转缓冲区恢复上下文；是否真的调用、使用了哪个目标，需要相应实际记录，不能由API/handler存在反推。

参考：[RtlRestoreContext](https://learn.microsoft.com/windows/win32/api/winnt/nf-winnt-rtlrestorecontext)、[EXCEPTION_RECORD](https://learn.microsoft.com/windows/win32/api/winnt/ns-winnt-exception_record)、[异常分派](https://learn.microsoft.com/windows/win32/debug/exception-dispatching)。

## 5. 本轮缩小到哪里，尚不能下什么结论

1. 两份现场共同落在事件Value重新序列化到Map的路径，不仅仅是泛化的“使用了XMM6”。
2. 当前指针/容器区间与六帧EH状态写入没有形成保存槽别名的直接证据。
3. PID38324新增一个明确的返回值一致性矛盾：Map索引0、RSI0，而输出索引7。应优先解释helper返回、最后返回对象写入、异常上下文与后续内存之间的关系。
4. PID13020的返回值4与RSI4一致，不能把PID38324这项新矛盾套到两个现场并宣称共同根因。
5. 尚无同次执行历史把“首次坏指针写入”或“错误非局部恢复”定位到具体指令。没有可证实的产品修复；不修改代码、不重启节点、不开发新采集器。

原库与身份未访问；本轮只创建离线分析文件。PID38324/PID13020原始转储前后SHA-256匹配，见preservation-public.json。完整地址、原始CONTEXT候选和调试器内存日志留本机target/c-phase30-alias-context-20261116/analysis-private；公开Git只同步本结论及脱敏结构。
