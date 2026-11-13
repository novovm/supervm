# C 原始崩溃第二阶段：对象身份与参数传递（2026-11-13）

**新增实质结论：3228 的真实局部 String 与 Formatter 在最终快照中仍相互对应，
但写入端使用的对象地址落在另一块保存寄存器的栈区域。37796 也有局部对象与
实际写入接收地址不一致，另伴随错误文本源指针。首次破坏位置仍未找到，未修复。**

本轮仅离线读取已有三份原始 MiniDump、匹配镜像/符号、源码和公开 GitHub 制品元数据。
不启动节点、不使用 TTD、不访问原库、不修改生产代码、不恢复双中继。C NOT READY；A 不发送。
原始转储重新计算 SHA256 与上一轮一致，见 [文件复核](dump-recheck-public.json)。
四项完整命令输出和原始模块清单沿用[第一阶段](../C-original-dmp-review-20261113/README.md)。
本目录只公开审阅过的代码反汇编、类型解释后的少量地址字段；完整堆栈内存和转储不上传。

## 1. 时间、现场和构建

| PID / 线程 | 崩溃北京时间 | 进程运行时间 | 崩溃时可确认的工作 |
|---|---|---|---|
| 31296 / 6228 | 2026-11-11 21:27:58 | 1:55:34 | IndexMap 插入函数恢复寄存器；上一层调用者未恢复 |
| 3228 / 2960 | 2026-11-12 04:13:40 | 5:02:14 | native tick → observe_tick → RpcLifecycle::status → profile/字符串格式化 |
| 37796 / 31876 | 2026-11-13 10:15:56 | 1:34:55 | 两字节复制；精确 a254 镜像展开的地址链与格式化路径相符，源符号未齐 |

三份都来自 C，不是三台机器。不是启动即失败；这些时间不足以证明随机触发，
也不能把三次都归为同一中继、区块或 GPU 任务。线程号只在各自进程内有意义。
31296/3228 为同一 875 EXE；37796 是 a254 EXE，且本机当时已使用 AOEM966。
因此不能把三份说成完全相同的二进制组合。EXE/DLL 哈希、历史 C/D 差异见第一阶段。

## 2. P0：3228 的 String 是谁创建、谁持有

匹配 PDB 指向 Rust `alloc::fmt::format::format_inner`（编译器源码 commit
`4a4ef493e3a1488c6e321570238084b38948f6db`，alloc/src/fmt.rs:645–650）。
它创建栈上局部 `output: String`，以 `&mut output` 同步调用 `write_fmt`，成功后
才把 String 所有权返回。不是 C/C++ 传入的 String，也不是此处释放后的堆对象。
这说明所示源码的生命周期；最终转储不能证明执行期间从未有外部破坏。

可还原调用链（从创建者向故障点）：

`format_inner → Write::write_fmt → core::fmt::write → u8::LowerHex::fmt
→ Formatter::pad_integral → String::write_str → Vec::append_elements → memcpy`。

`pad_integral` 的末段以 `jmp rax` 尾调用 writer，故异常栈没有独立 pad_integral 帧；
不能把这个缺帧当作断开的调用链。[构造与调用汇编](3228-code.txt)、
[pad_integral 汇编](3228-pad-code.txt)保留这条静态路径。
`uf` 对部分函数提示 flow analysis incomplete，未把静态路径说成实际分支执行历史。

### 最终快照中的对象布局

下列 cap/ptr/len 顺序是由**该构建的机器码**核实，不声称 Rust String 有稳定 C ABI。

| 对象/位置 | 地址 | 最终转储值 |
|---|---|---|
| format_inner 局部 String | 0x755b6ee3d0 | cap=0、ptr=1、len=0，合法空 String 表示 |
| core::fmt::write 局部 Formatter | 0x755b6ee340 | buf 指向 0x755b6ee3d0；vtable=0x7ff7808be610 |
| vtable 的 write_str 项 | vtable+0x18 | 0x7ff77ff37a00，精确匹配 String writer |
| writer 现场 RDI | 0x755b6ee358 | 与上述 String 地址不同，落在 core::fmt::write 保存 RBX 的槽位 |

构造指令在 `0x7ff77ff37d19` 起写入 String 三个字段；`0x7ff77ff37d30` 用
`lea rcx,[rbp-0x20]` 传出地址。core::fmt::write 在 `0x7ff77ff3d269` 起构造
Formatter；该 Formatter 的 buf 在异常快照里仍指向正确局部 String。

writer 从现场 RDI+8、RDI+0x10 解释出的“buffer/len”分别为
`0x755b6ee470`、`0x755b6ee490`。按调用者序言，这两格原本是保存 RDI/RSI 的位置。
两者相加恰为 `0xeab6ddc900`，即最终故障写地址。该处假 cap=`0x7ff7805fb823`，
数值上仍大于假 len，故**只加 len≤capacity 检查不能证明对象身份正确**。
真实空 String 的 len=cap=0；其 ptr=1 不是此处读写故障地址。

机器码 `String::write_str` 入口把 RCX 保存到 RDI，并在复制前形成 buffer+len。
这证明地址的**使用/计算方式**，不是入参时刻的寄存器录像。不能仅凭异常时 RDI
断言它在调用前就已错误，或断言中间调用、寄存器、栈状态从未被破坏。

### 地址形成、使用、首次破坏明确分开

- 形成：writer `0x7ff77ff37a24` 起读取伪 buffer，再与伪 len 相加。
- 使用：VCRUNTIME140+0x1126b，`mov word ptr [rax],cx`；写两字节，源文本是 `b9`。
- 首次破坏：**未知**。仍缺少 Formatter→pad_integral→writer 边界上的运行时
  RCX/RDI/R14 传递及非易失寄存器保持历史。现有 MiniDump 没有首次写入轨迹。

### 所有权、并发与 FFI 审查

原 875 `native_pipeline/profile.rs:276` 的 `hash` 按值接收 `Option<[u8;32]>`，
遍历本地 bytes，逐字节 `format!("{byte:02x}")`，再将 String 收集进 JSON。
snapshot 对 CRITICAL/DELIVERY/CONTROL 容器在持有各自 Mutex/try_lock 守卫期间读取。
这条直接路径没有 spawn/async、extern C、裸指针导出、from_raw_parts 或 set_len；
未发现 C/C++ 保存此局部 String 内部指针的行为。标准库 fmt 内部有类型擦除和 unsafe
实现，不能把它误写为 C ABI 回调，也不能据此宣称整个程序没有 unsafe 风险。

String 缓冲区的正常修改来自 Vec 扩容、复制、长度更新；本轮机器码确认了 writer 的
reserve 分支和复制后长度更新。没有证据证明这个 String 被提前释放，或某个线程
共享它。更早的 FFI/越界/寄存器破坏仍可能存在，但未被当前证据归属到具体函数。

## 3. P1：37796 的 memcpy 参数与精确 PDB 缺口

精确 a254 EXE 用于 unwind 和反汇编，未强制载入 875 PDB。
[参数相关代码](37796-code.txt)、[pad/导入跳板/memcpy 代码](37796-pad-code.txt)。
a254 中没有精确符号的 `aws_lc...+大偏移` 是最近导出符号标签，公开摘录已去掉；
不能据此说这些调用发生在 aws-lc。

| 项目 | 实际值 |
|---|---|
| 故障指令 | VCRUNTIME140+0x11268，movzx ecx,word ptr [rdx] |
| 实际源地址 | RDX=0x8000000000000005 |
| 目标地址 | RCX/RAX=0x204496a0000 |
| 长度 | R8=2 |
| 上一帧复制调用点 | node RVA 0x1e4cc8e，返回点 0x1e4cc93 |
| 数字格式化调用帧保存的源/长度 | 0xe7e2ee42e / 2，内容为 `56` |
| 构造出的局部 String 候选 | 0xe7e2ee500，cap=0、ptr=1、len=0 |
| Formatter 候选 buf | 0xe7e2ee500，vtable 的 writer 项为 node+0x1e4cc60 |
| writer 现场 RDI | 0xe7e2ee648，与局部 String 不同，落在外层保存返回地址槽 |

后一组对象名称由精确镜像布局及与 875 代码结构对照解释，**不是 a254 PDB 源行确认**。
现场 RDI+8 为 0x204496a0000，+0x10 为 0，恰能解释目标地址；RDI 所指的第一格
是返回地址 0x7ff70a4565a0，不是合法 String 容量。真实局部空 String 仍在。

相邻栈槽 0xe7e2ee638 也含 0x8000000000000005；这个数值在其他 Rust 编译代码中
也可作为表示标记。**数值相同不证明 memcpy 的 RDX 就从该槽读取，更不证明
31296 写入的标记跨进程传到了这里。** 参数首次改变的位置仍未知。

PDB 要求：GUID `E16D0B11-47CA-4EBF-B482-C91380CC57F8`，age **1**。
本次检查结果：

- E 盘原 novovm_node.pdb 是 875 的 C94A5E99.../age1，不能代替。
- 本地 target/release/deps 的另一个 node PDB 为 04567E74.../age6，不能代替。
- 原 Full run43155828234 的公开制品列表返回4项，没有标为 PDB/符号的制品；
  只查询元数据，没有下载全部制品，不声称其内容或 A 私有构建目录中不存在 PDB。

[CI 查询结果](pdb-ci-artifact-search-public.json)、[另一份本地 PDB 摘要](local-other-pdb-summary.txt)。
**交给 A 的具体协作项**：从生成 1758e4a7... 原 a254 EXE 的构建目录取得上述
GUID/age 的原始 PDB；不要重新编译代替。本机其余离线分析已独立完成。

## 4. P2：31296 是恢复寄存器的栈寻址异常

匹配 PDB：`indexmap::inner::Core::insert_full+0x31d`，RVA0x65f59d。
符号没有给出该指令的精确源行，不能编造 insert_full 的源码行号。
[完整函数](31296-code.txt)及[unwind 信息](31296-unwind.txt)。
首次误用 `!fnent` 返回无此扩展，原输出保留；改用 `.fnent` 后取得函数表。

函数序言保存8个通用寄存器、`sub rsp,0xd8`、`lea rbp,[rsp+0x80]`，再保存 XMM。
故障 `movaps xmm6,[rbp+0x20]` 位于恢复段，尚未执行随后的栈回收。
现场 RSP=0xe83f7ee180、RBP=0xe83f7ee168；正常帧关系应给出 RBP=0xe83f7ee200，
相差 -0x98。实际操作数=0xe83f7ee188，mod16=8。异常记录地址参数仍为全1，
与计算操作数分别保留。

因此它不是“IndexMap 数据元素地址恰好未对齐”的直接证据，而是恢复 XMM 所用的
帧地址与序言不一致。还不能区别 RBP/RSP 何时改变、ABI 保存违规、栈写坏、异常
控制流或机器执行问题。只有一帧可可靠展开，不足以与另外两份建立共同调用者。

## 5. P3：交叉关联与不能推出的结论

[结构化对象对照](object-comparison-public.json)保存可复核地址与算式。

- 3228/37796 的相近点比“都在 memcpy 崩溃”更具体：逐字节格式化的栈帧仍保存
  正常局部对象/源文本，但实际复制参数与之不一致；一个坏目标、另一个坏源。
  37796 的完整源码归属仍需精确 PDB。
- 31296 是另一处帧寄存器不变量异常，尚无共同上游对象、分配器、释放路径证据。
- 三份均未证明正在执行 AOEM FFI、Vulkan 或 C++ 对象回调；不能推出早先没有发生。
- 不同进程/线程/构建的指针不能当成同一共享对象。数字标记重复不等于因果关联。
- 目前没有已证实的 UAF、双重释放、某条越界写、ABI 错误或硬件根因。
  C 的实测硬件字段见第一阶段；不把未经核实的 DDR4 假设当调查事实。

## 6. 最小修复和独立回归：尚不具备落生产补丁的依据

**没有足够证据选择生产修复位置。本轮不改 String/indexmap/memcpy，不把改写十六
进制函数、对坏指针返回空串或扩大超时作为修复。** 这些会绕开暴露点或掩盖损坏。

下一最小验证按以下门槛收敛（提案，未执行）：

1. 先补齐 a254 精确 PDB，核对同一原 DMP 的局部参数和调用边界，不重新启动节点。
2. 若仍缺时序，后续另行批准的隔离实验只定位 Formatter buf、pad_integral 传入
   receiver、writer 接收 receiver、文本地址/长度首次不一致的边界；同时核对
   中间 helper 的非易失寄存器保持。只盯 String 内存写入不足以捕获寄存器变化。
   必须复用匹配构建的已有路径；不重开 TTD，也不默认再跑全节点两小时。
3. 若找到具体所有权/ABI/写入错误，只修该所有者或边界，并以同一个故障触发输入
   建立“修前失败、修后通过”的独立回归。补充空串/全部256字节/容量边界、并发
   记录与快照一致性验证，但正常格式化测试通过本身不能替代原故障复现。
4. 若无法获得首次变化或可反证触发条件，继续标未解决，不能用暂时未崩溃放行中继。

本轮离线 CDB 分析正常结束；这是分析工具完成，不是节点测试通过。
真实工具结果见[退出记录](offline-tools-public.json)。Git 推送用于通知 A，不等于 A 已签收。
