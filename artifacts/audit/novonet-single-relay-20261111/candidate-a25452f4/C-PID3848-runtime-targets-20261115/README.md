# PID3848 同次 stderr、格式化动态目标与上下文捕获

## 从同次执行取得的记录

使用管理员只读入口读取受保护的 stderr-0.txt，脚本报告读取退出 0。文件 897 字节、23 行，SHA256 为 BDA146E159B99FB6B0E2A370757332FCEACDE941F00EB7EB427F2494EB6CE8EA，两次读取一致。仅导出分配错误行、回溯地址及文件摘要。

实际原行为 `memory allocation of 39582418599952 bytes failed`，与完整转储的 36 TiB+16 字节申请一致。日志中含有 20 个带编号回溯地址。第一轮按函数名过滤没有匹配到名称，第二轮按地址读取后，用同一构建的原 PDB 解析。

回溯中的连续地址包括以下位置。

| 日志序号 | 原地址 | 归属与关系 |
|---|---|---|
| 0 | 0x7ff7f2796b62 | Backtrace 格式化函数中的 RtlCaptureContext 调用返回地址 |
| 1 | 0x7ff7f27a24e1 | core::fmt::write 的间接参数格式化调用返回地址，调用点 RVA 0x1e524dd |
| 2 | 0x7ff7f2780854 | Stderr 的 write_fmt 调用 core::fmt::write 后的返回地址 |
| 3 | 0x7ff7f278e117 | 默认分配失败钩子输出回溯后的返回地址 |
| 4 | 0x7ff7f278e1dc | rust_oom 调用错误钩子后的返回地址 |
| 9 | 0x7ff7f26e1e07 | Hashbrown 进入分配失败处理后的返回地址 |
| 11 | 0x7ff7f119f3ae | insert_full 的原表扩容调用返回地址 |
| 12 | 0x7ff7f10c8395 | status 中 archive_work 首次插入返回位置 |

这段记录将一次真实的间接调用和上下文捕获定位到分配失败之后。此前模板向量的第一次变化仍早于这个记录起点。

## 三类证据分别列示

### 实际执行有日志对应

core::fmt::write 的 RVA 0x1e524dd 通过格式化参数函数指针调用了 `std::sys::backtrace::impl$0::print::impl$0::fmt`，目标 RVA 0x1e469b0。默认错误钩子在 RVA 0x1e3e0fc 写入该函数地址，RVA 0x1e3e112 调用 Stderr::write_fmt。日志中的上述前四个帧地址与这组机器码相互对应。

回溯格式化函数在 RVA 0x1e46b5c 通过原 IAT 调用 ntdll!RtlCaptureContext，下一条指令 RVA 0x1e46b62 恰好是日志第一帧。因此本次 RtlCaptureContext 执行发生在错误报告期间，时间顺序在异常容量计算和分配失败之后。

### 同次制品和对象类型可以解析目标

已执行的 Stderr::write_fmt 实例在 RVA 0x1e30844 将固定 vtable 地址 0x7ff7f311e918 传给 core::fmt::write。对象类型为 `std::io::default_write_fmt::Adapter<std::sys::stdio::windows::Stderr>`，size=16、align=8。

| core::fmt::write 的位置 | 该 Stderr 实例的目标来源 | 原目标 RVA | 执行证据范围 |
|---|---|---|---|
| 0x1e523f5 | vtable+0x18，write_str | 0x1e45910 | 目标确定，各个分支命中次数未记录 |
| 0x1e52442 | 同一 write_str 方法 | 0x1e45910 | 目标确定，分支命中未单独记录 |
| 0x1e524dd | 实际参数 formatter | 0x1e469b0 | 本次回溯日志证明该次回调 |
| 0x1e5251e | 纯字符串快速路径的尾跳，vtable+0x18 | 0x1e45910 | 目标确定，尾跳命中未记录 |

同一默认错误钩子为分配字节数准备的数值 formatter 指向 RVA 0x1e50080。它属于该错误消息的静态参数设置。

该 vtable 与参数目标只适用于这里已经关联的 Stderr 格式化实例。status 更早的 hex/String 或其他 formatter 可能使用不同的对象和参数，相关已返回调用的 vtable/参数历史仍然缺失。

### 静态存在但历史命中未逐项取得

回溯函数的 RVA 0x1e46b9e 调用 RtlLookupFunctionEntry，RVA 0x1e46d4e 调用 RtlVirtualUnwind。IAT 的同次地址已经解析，日志中的多帧回溯与其遍历代码相符。各个 VirtualUnwind 调用的次数、输入和输出 CONTEXT 没有逐项记录，不能把这份日志变成完整的上下文变化历史。

## 上下文恢复假设的当前范围

这里的 RtlVirtualUnwind 接收回溯函数栈内的 CONTEXT，以计算上一帧。官方接口将 ContextRecord 声明为 in/out，并描述其内容为上一帧上下文。实际执行上下文恢复另有 RtlRestoreContext 接口。解析时按具体用途区分这两类操作。[RtlVirtualUnwind](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nf-winnt-rtlvirtualunwind)、[RtlRestoreContext](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nf-winnt-rtlrestorecontext)

本次已关联路径在巨大申请失败后输出回溯，再回到 rust_oom，最终执行 int 29h。这个已经定位的捕获/遍历事件无法向前解释表初始化时的坏 XMM6/XMM7。若要调查更早一次恢复，仍需更早事件对应的证据。

PID3848 的 ThreadList 与 ExceptionStream 在故障线程 34004 上保留相同 RIP、RSP、RBP 及 XMM6/XMM7。RIP 位于 Rust abort 的 int 29h。PID34452 的历史报告则由 RtlCaptureContext 和报告帧 RtlVirtualUnwind 构造 GS 失败上下文，其 ThreadList 另有等待帧。这两种上下文来源分别保留，不能仅凭共同的 API 名称建立原始故障同根关系。

EHANDLER 元数据仍只给出可用处理入口。本次错误报告和最终活动栈没有提供先前一次 EHANDLER/RtlRestoreContext/NtContinue 错误恢复的记录。此前返回子调用的向量保存槽已被当前帧复用，继续作为明确证据缺口。

## 跨平台参考的本地核验

按用户提供的参考核对本轮两份实际制品。原 a254 EXE 与 AOEM966 DLL 的 PE Machine 均为 0x8664（AMD64），哈希分别为 1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d 和 5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52。

PE 架构字段能够确认这两份二进制的目标机器类型。原编译机的 CPU、原始 target-cpu/target-feature、全部跨 ABI 回调约定仍须各自构建记录证明，不能从当前编译器或开发机品牌反推。这份参考纳入制品与调用边界调查，四机全量环境审计没有替代当前主线。

## 最小捕获方式的评估边界

如果后续需要取得首次上游变化，可以将检查限定在原 status 的 signature_work 局部构造到 archive_work 构造区间。静态列举包含 62 个 call 位置和可选分支。候选方式是独立诊断副本上的内联寄存器校验与固定记录，仅记录这些外层调用前后，不对分配器、Formatter 或每个网络事件设置逐调用调试器回调。

候选静态预算为每次活动调用 128 条、每条 96 字节，合计 12,288 字节，用于容纳 62 对调用记录及入口/终点。记录需含线程、活动调用 ID、调用地址/实际目标、XMM6/XMM7、RBP/RSP 和覆盖状态。每处校验的设计预算上限为 32 条指令，128 处合计上限 4,096 条校验指令。若出现循环、递归或并发槽不足，必须记录缺失标志并判定覆盖不完整。这只是静态预算，不能换算为实测 P50/P95 或 CPU 开销。

该候选尚未实现或运行，实际延迟、吞吐、CPU 开销和失败调用历史完整性均为未测。原机器码副本中的活跃寄存器、标志、重定位及 unwind 信息需要逐项保持和核对。source 重新编译也会改变寄存器布局，必须明确构建差异。当前没有满足这些条件的已验收采集入口，因此本轮继续提供证据边界，未启动新运行。

## 交付状态

本轮新增的是同次 stderr 的实际读取与一次动态 formatter/上下文捕获的时间关联。原始 DMP 的 SHA256 保持 6c3b7b9dc34ec3c137c2b4e7bcf0f4746cf6ce023db90f6a6dc423dce2613de6。原库、身份和失败现场保留，双中继暂停。首次上游异常变化、ABI 违约及共同 RBP 根因仍未定位，当前没有具备证据支持的生产修复。
