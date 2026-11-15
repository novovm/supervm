# PID3848 的构造期、传输记录快照与同次证据缺口

## 本轮范围和结果

本轮沿原 a254、AOEM966 和 PID3848 的同一完整转储离线核对。signature_work 从 RVA 0x777105 局部初始化开始，覆盖其插入、可选旧值释放、搬移和发布。复用此前已建立的调用图，聚焦 network_probe_observation 的传输记录快照、原 JSON entry/drop 和原 Core::insert_full 两条分配路径。

首次异常 XMM6/XMM7 变化仍未定位。没有发现足以据此修复的 ABI 违规，也没有运行新节点或样例。下述已构造对象、最终仍存活的记录和静态目标，均保留各自证据范围。

## signature_work 构造期间需要保留的具体边界

原局部表在 RVA 0x777126/0x77712d 接收缓存的 XMM6/XMM7。其后六次 insert_full 都调用同一个 RVA 0x84f050。

| 字段 | insert_full 调用 RVA | 可选旧值 drop 调用 RVA |
|---|---|---|
| strict_verified_rows | 0x77726d | 0x7772d8 |
| reused_rows | 0x777402 | 0x77746d |
| uca_batch_calls | 0x7775a2 | 0x77760d |
| asset_prepared_at_startup | 0x777729 | 0x777794 |
| asset_startup_us | 0x7778b2 | 0x77791d |
| scope | 0x777a80 | 0x777aeb |

每次 insert 后，原代码检查返回的 Option tag。0x8000000000000005 对应 None 时跳过旧值释放。转储中最终六个唯一字段与正常新键插入顺序一致，但每次返回 tag 没有记录，不能据此宣称六次 drop 均实际执行或均未执行。

发布过程还包含 RVA 0x777b56 的 index_or_insert 和 0x777b65 的 drop<Value>，随后通过 XMM0-XMM3 搬移局部 Value。最后一条发布写入是 0x777ba9。这些正常搬移没有重载 XMM6/XMM7 模板。

该直线构造区间含 35 个静态 call 位置，目标只有七类，包括分配器、hash_one、Core::insert_full、可选 drop、memcpy 和发布 entry。七个直接函数体中显式写 XMM6/XMM7 的是原 Core::insert_full。它在入口保存向量，在桶探测中合法暂用，正常出口恢复。entry 内部的 IndexMap::entry 同样已在此前审查范围中。分配器和 memcpy 的 DLL 边界及其历史返回仍没有动态寄存器证据。

因此合法的 signature_work 结果与完成时向量值分别保留。局部表完成后可以仍然合法，而某次插入、分配、释放或发布调用已经改变了缓存模板。

## network_probe_observation 的同次结构条件

这次新增读取只涉及诊断对象的类型、长度和结构计数，没有导出对象正文、标识或密钥。

- 最终 native_network_probe 为 8 字段对象。
- 顶层 probe events 数组长度为 0。
- transport 为 19 字段对象，其中 objects 数组长度和容量均为 255。
- protocol_time.peers 保留原三个对端的数组结构，本轮没有导出对端内容。

从当前 insert_full 帧的原返回地址 RVA 0x778395、保存的调用者 RBP 和保存的 RDI，恢复 status/RPC 对象的候选基址，并核对其 input_trace Arc。Arc 的强/弱计数为 4/1，指向的诊断状态仍可读取。最终 VecDeque 容量 256、head 203、length 255，对应两个连续片段 53/202。这个几何关系属于最终仍存活的状态；记录处理线程可能在快照返回后继续运行，因此不能将其直接当成本次每个 clone 循环的历史次数。

原源码 Channel::probe_observation 通过 input_trace.snapshot() 取得 transport。原 network_probe_observation 的以下目标可具体关联。

| 原 call RVA | 原目标 RVA | 关联依据 |
|---|---|---|
| 0x5d0186 | 0x15af1d0 | input_trace 存在时调用 NativeTransferDiagnosticsV1::snapshot；最终非空 transport 支持该源码路径 |
| 0x15af6c0 | 0x1499950 | VecDeque 记录集合转为独立快照数组 |
| 0x1499a64 | 0x15a57c0 | 第一连续片段的 NativeTransferEvidenceV1::clone |
| 0x1499ac4 | 0x15a57c0 | 回绕片段的同一个 clone |

这些边是已有调用图中的原直接目标。objects 中的 255 项是同次快照结果证据；两个 clone 调用点各自的实际命中、参数和向量值仍没有逐次记录。

诊断 State.snapshot 内的 objects、udp_recovery、relay_circuits 三个源 Vec，在最终内存中均为 cap=0、len=0、ptr=8。原源码先 clone 累计快照，再用 records.iter().cloned().collect() 替换局部快照的 objects。完整记录对象的周转因此需要聚焦记录 collect/clone，不能仅因顶层 probe events 为空就当成没有记录克隆。Vec::clone 的空路径仍执行自身保存恢复，其历史帧基址也没有得到动态验证。

## 向量保存、恢复和候选来源

选取的原机器码与同一 DMP 逐字节相符，保存在 selected-machine-code.txt。

| 函数入口 RVA | 保存 / 恢复 | 仍缺什么 |
|---|---|---|
| 0x5cf780 network_probe_observation | RBP+0x5e0/+0x5f0，恢复 RVA 0x5d0a7b/0x5d0a82 | 此次恢复时的 RBP 与槽内容 |
| 0x15af1d0 传输诊断 snapshot | RBP+0x440/+0x450，正常返回路径恢复 | 锁内实际记录范围、嵌套调用返回与历史保存值 |
| 0x15a57c0 记录 clone | RBP+0x200/+0x210，恢复 RVA 0x15a61c0/0x15a61c7 | 此次记录实例及返回前的原槽内容 |
| 0x84f050 Core::insert_full | RBP+0x20/+0x30，恢复 RVA 0x84f36d/0x84f371 | 每次调用独立帧中的原保存值 |

记录 clone 在 RVA 0x15a5887/0x15a58d1 合法读取源记录 +0x2a0/+0x2b0 到 XMM6/XMM7，退出时应恢复调用者原值。本轮仅将这两个已知载入位置与最终仍存活的 255 个源记录作比较，当前没有一组完整 32 字节等于坏表向量。这不能排除过去的输入不同、记录复用或恢复槽损坏。

所选调用直接落到原 EXE 内部。已检查代码中没有找到一个可以证明本次实际执行并违反 Windows/SysV 适配的跳板。正常保存恢复配对仍依赖正确的帧基址和未被覆盖的保存区。Windows x64 对 XMM6-XMM15 的低 128 位要求由被调用者保持。[Microsoft x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention)

## 两次旧 Value 的实际变体仍然缺失

stage_profile 和 native_network_probe 的替换路径分别在 RVA 0x777be2/0x777c6f 对 entry 返回的实际槽地址调用 drop<Value>，再将新对象搬入同一地址。该槽当前已经保存新对象，旧 tag 和旧指针被覆盖。

原 entry 在正常缺失键路径创建 Null（tag 0x8000000000000000），Null 在 drop<Value> 中短路返回。原源码的正常新键路径与该分支一致，但转储没有保留这两次 entry 的返回分支或 drop 入口 tag。旧值是否实际为 Null、是否因异常落入 String/Array/Object 释放，仍缺同次入口记录。不能从最终新对象类型推回旧值。

## 两条 insert_full 分配路径的共同点与差异

- PID3848 在 RVA 0x84f095 检查 growth_left，当前坏表值为 0，对应进入 0x84f38e，并在 0x84f3a9 调用 reserve_rehash。该栈与 36 TiB+16 申请参数同属本次失败。
- PID34452 的已保留失败栈对应 RVA 0x84f354 调用另一条 Vec 追加/增长路径，进而到 HeapReAlloc 并暴露 RBP 帧关系异常。
- 两者共享入口、向量保存和正常恢复代码；当前保留的分配失败栈位于两条不同分支中。共同入口不能证明曾经执行同一条异常恢复，也不能证明源自同一次内存破坏机制。

## 收束后的取证决策

静态目标审查在这里停止扩展。当前缺少的是同一活动 status 调用中的向量值、旧 Value tag、保存槽原值及动态分支记录。

下一步可仅评估四个父帧检查点的稀疏硬件执行断点，复用现有调试器，避免在每个子调用或寄存器写入处停顿。四个已核对指令边界为 RVA 0x7721fb（模板载入后）、0x777bad（signature_work 发布后）、0x777c36（network_probe_observation 返回后）、0x77823e（坏表写入前）。处理器执行断点由调试寄存器控制，实际仍会进入调试器，不能预称低干扰。[Microsoft processor breakpoints](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/processor-breakpoints---ba-breakpoints-)

该选择每个正常 status 调用最多四次命中，按旧 6.29 秒查询节奏约 0.64 次命中/秒。这里只计算设计命中预算，没有实测时序成本。四个点只能定位首次被观察到的异常区间，不能直接证明区间内部哪条指令首次写坏了向量。若 signature 发布后已异常，优先保留构造期及更早路径；若 network 返回后才异常，中间仍含 profile、entry/drop 和 network，须保持该覆盖缺口。

每条记录以线程 ID、调用序号、原 status RSP、返回地址和栈范围关联，保存 XMM6/7、RBP/RSP/RIP、QPC 及断点状态。每次运行单独保留证据，不能把不同失败调用拼成历史。异常离开或缺少某个检查点时，调用标记为不完整。

任何下一次隔离运行须先对比同负载未附加调试器、附加但不开检查点、四点开启三种条件，分别记录报告间隔分布、查询耗时、吞吐、CPU/内存、回调墙钟耗时及目标退出码。64 位向量内容必须直接按 32 字节比较，不能经浮点字符串格式化后再比较。完整四点调用数/全部有入口调用数、每点实际命中数及不完整原因分别报告，不能把稀疏覆盖率写成全指令覆盖率。

本轮没有实施这套检查点，也没有运行干扰测量；当前没有可证明低干扰的动态捕获通道。若实际报告节奏或负载明显偏离无调试器基线，该方案同样停止，不能继续长跑取得无代表性的正常记录。只有出现相邻两个检查点间的变化，才能进一步针对该次证据区间制定下一步。

## 保留状态

本轮只读既有 EXE、精确 PDB、同一 Full Dump 和公开源码。临时离线脚本保存在 ignored target。原库、身份、失败现场保留，生产代码没有修补，节点没有重启，双中继继续暂停。Git 同步用于共享，C 独立调查继续。
