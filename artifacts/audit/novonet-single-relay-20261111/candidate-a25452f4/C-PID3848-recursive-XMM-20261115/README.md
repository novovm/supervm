# PID3848 的 XMM6/XMM7 嵌套调用审查

## 结论与证据边界

本轮将原 status 的 22 个具有 RuntimeFunction 范围的直接目标向下递归展开，并补充 6 个叶函数或跳板目标，核对嵌套调用、直接尾跳、可解析的间接分派及异常处理元数据。所审查 EXE 路径中的 XMM6/XMM7 使用函数扩大到 12 个。其已解码正常返回路径都包含恢复原保存槽的步骤。

尚未找到第一次异常寄存器变化或可以据此修补的指令。局部保存恢复核对依赖帧基址稳定、保存槽未被覆盖以及嵌套调用各自履约。真实错误恢复的历史目标和内容仍然缺失。公共 JSON 将未解析位置逐项保留，不能将静态局部核对当成整个调用链的运行正确证明。

signature_work 的合法结果表明相应构造取得了合法表，完成时 XMM6/XMM7 的实际数值仍缺直接记录。archive_work 在 RVA 0x77823e/0x778245 复用缓存于向量中的模板，其构造位置没有重新载入空表常量。这两点决定了当前可建立的数据流边界。

## 从四个直接函数继续向下

| 入口 RVA | 原匹配 PDB 对应函数 | XMM6/XMM7 的 RBP 保存与恢复偏移 |
|---|---|---|
| 0x84f050 | Core::insert_full，原直接目标 | +0x20 / +0x30 |
| 0x863990 | AdmissionState::status | +0x1d0 / +0x1e0 |
| 0x5cf780 | NativeLifecycle::network_probe_observation | +0x5e0 / +0x5f0 |
| 0x34b9a0 | LocalKey::with，traffic 快照实例 | +0x1e0 / +0x1f0 |
| 0x1d92e60 | IndexMap::entry | +0x00 / +0x10 |
| 0x15af1d0 | NativeTransferDiagnosticsV1::snapshot | +0x440 / +0x450 |
| 0x15b7fc0 | LocalKey::with，IO 诊断实例 | +0x301d0 / +0x301e0 |
| 0x14ad210 | Vec::clone 的当前实例 | +0x2e0 / +0x2f0 |
| 0x15b5260 | Core::insert_full 的另一个实例 | +0x80 / +0x90 |
| 0x15b1730 | IoTraffic::snapshot | +0x1c0 / +0x1d0 |
| 0x15a57c0 | NativeTransferEvidenceV1::clone | +0x200 / +0x210 |
| 0x1d936d0 | Core::insert_full 的第三个实例 | +0x20 / +0x30 |

这些函数的原 EXE 字节与同次完整转储一致。逐条指令、原序言槽、正常返回地址和静态路径检查的假设保存在 vector-return-review-public.json。所有函数的正常返回静态路径都恢复两份本地保存值。没有发现绕过恢复直接正常返回的已解码路径。保存槽的历史内容及运行期间 RBP 状态仍需另行证据。

root-coverage-public.json 按 22 个入口分别记录可达向量使用函数、间接调用缺口和系统导入。覆盖集合还包含可选诊断与失败处理分支，401 个静态入口的数量仅用于复核分析范围。

## 两个间接 TLS 入口与五处分派表

已解析 status 给 LocalKey::with 传入的静态描述符。RVA 0x34b9d9 的 call [rdx] 对应该描述符内的 EXE RVA 0x11b0fe0。native_io_cost_snapshot 给另一个 LocalKey::with 传入的描述符，对应 RVA 0x15b8007 的间接调用，静态目标为 EXE RVA 0x15b33b0。描述符内容取自同次转储。这个解析给出这些特定调用点的静态目标，执行与否仍需结合分支证据。

已按机器码和类型分派范围解析五个相对偏移跳表。其中 serde_json Value 分派采用其 6 个变体，两个解析分派分别有显式 0x20、0x53 上界。跳表地址、分支目标和 EXE 范围都在本地复核。静态分派集合不代表本次依次执行过这些分支。

同次主 status 对象保存了 signature_work、stage_profile 和 native_network_probe。完整 50 项 key 中，IO 统计和 asset 的几个可选产出缺席。按原源码正常顺序可优先检查此前合法局部构造到 archive_work 构造之间的状态处理。LocalKey::with 和 AdmissionState::status 仍作为静态候选范围审查，其实际执行位置和时机单独标记，避免仅凭函数存在就指定肇因。

## 仍未闭合的具体边界

unresolved-frontiers-public.json 保留 42 个未解析的寄存器或间接分派位置，以及 41 个系统导入调用位置。典型缺口包括 core::fmt::write 中的 RVA 0x1e523f5、0x1e52442、0x1e524dd、0x1e5251e，具体目标取决于当时的格式化参数和 vtable。另有函数指针、动态格式化、panic hook、I/O trait 等分派。其出现于静态集合不证明异常前实际执行过。

系统边界涉及 HeapAlloc、HeapFree、HeapReAlloc、ProcessPrng、VCRUNTIME 的内存操作，以及异常相关的 RtlCaptureContext、RtlLookupFunctionEntry、RtlVirtualUnwind。已读取同次 IAT 目标，系统 DLL 内部的全部路径及历史返回状态尚未审查闭合。

此外读取了各函数的 EHANDLER 元数据，226 个入口指向 EXE RVA 0x23ae7cd 的共同处理入口，再交给系统运行库。异常处理元数据的存在不能充当实际展开事件。异常前是否进入过这些处理器、使用了哪份上下文、恢复了哪些历史向量值，当前终态没有对应记录。

本轮已经解析的直接路径、静态描述符、尾跳及异常入口尚未连接到某个已证明肇因的 Windows→SysV 跳板。AOEM966 历史汇编审查的结论仍只适用于原来检查的边界。尚未解析的动态目标和外部 DLL 执行记录决定了跨 ABI 假设仍待验证。

## 最初变化的位置仍缺什么

当前 DMP 中可以直接取得 archive_work 的异常字段、最终 XMM6/XMM7、当前 insert_full 的保存内容及代码页。此前 signature_work 完成时的向量值、中间每个实际调用前后的向量值，以及已经返回子调用原保存槽的内容都无法从这份终态恢复。旧保存槽已由当前活动帧复用，相关地址和当前所有者在上轮回传中列明。

因此尚不能指定第一处不一致的实际调用、第一次异常写入指令或最小生产修复。针对未知来源的修改也无法建立原故障复现与修复回归的因果对照。与 PID34452 的共同 RBP 根因仍待证。

本轮使用原 a254 EXE、精确 PDB 和 PID3848 的同一 Full Dump 离线审查。DMP SHA256 再次核对为 6c3b7b9dc34ec3c137c2b4e7bcf0f4746cf6ce023db90f6a6dc423dce2613de6。原库、身份和失败现场保留，双中继继续暂停。临时离线解析脚本与原始调试器输出保存在本机 target，公开仓库保存必要标量、指令和边界结论。
