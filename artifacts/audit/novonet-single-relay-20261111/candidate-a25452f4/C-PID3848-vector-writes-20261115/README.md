# PID3848 表字段写入与向量保存路径

本轮从同一份完整转储提取字段原始字节，并沿原 a254 机器码核对初始化写入、保存槽及直接调用边界。转储 SHA256 为 `6c3b7b9dc34ec3c137c2b4e7bcf0f4746cf6ce023db90f6a6dc423dce2613de6`。所有地址属于这一次 PID3848，地址区间采用左闭右开表示。

## 表字段与向量的逐字节对应

Core 位于 `0x53aaceedd0`，范围 `[0x53aaceedd0, 0x53aaceee08)`。前三个 qword 是 entries Vec 的容量、指针和长度，值为 `0, 8, 0`。RawTable 位于 Core+0x18，范围 `[0x53aaceede8, 0x53aaceee08)`。

| 字段 | 表内偏移 | 字段地址 | 原始小端字节 | 对应向量部分 |
|---|---|---|---|---|
| ctrl | 0x00 | 0x53aaceede8 | 08 00 00 00 00 00 00 00 | XMM6 低 8 字节 |
| bucket_mask | 0x08 | 0x53aaceedf0 | e0 f0 6e 71 e5 01 00 00 | XMM6 高 8 字节 |
| growth_left | 0x10 | 0x53aaceedf8 | 00 00 00 00 00 00 00 00 | XMM7 低 8 字节 |
| items | 0x18 | 0x53aaceee00 | 40 9e 6c 71 e5 01 00 00 | XMM7 高 8 字节 |

这 32 字节与 ExceptionStream 原始 CONTEXT 中的 XMM6/XMM7、当前 insert_full 序言保存的两份向量逐字节相等。比较直接读取各处原始字节，不经容量计算。空表常量区域的同期内容仍为 `[0x7ff7f2e45400, 0, 0, 0]`。

## 已定位的表初始化写入

`RpcLifecycle::status` 在创建 archive_work 空对象时执行以下指令。`RBP=0x53aaceec10` 是按本次保存槽和序言核对的帧关系。

| 指令 RVA | 实际地址 | 机器码 | 指令 | 覆盖字段 |
|---|---|---|---|---|
| 0x77823e | 0x7ff7f10c823e | 0f 11 b5 d8 01 00 00 | movups [rbp+0x1d8], xmm6 | ctrl、bucket_mask |
| 0x778245 | 0x7ff7f10c8245 | 0f 11 bd e8 01 00 00 | movups [rbp+0x1e8], xmm7 | growth_left、items |

编译器使用 XMM6/XMM7 搬运空表模板。这一写入用途与字段布局相符。异常表现为模板向量中的值已经变成 `[8, 0x1e5716ef0e0, 0, 0x1e5716c9e40]`，随后插入路径将其解释为表元数据。

这两条指令给出正常控制流中直接写表的位置。完整转储保留的是终态，首次动态写入时间以及是否发生后续覆盖仍缺历史记录。不能把初始化指令本身定为首次破坏源。

status 在 RVA `0x7721e6` 和 `0x7721f4` 载入合法常量。对原 EXE status 函数入口到本次构造位置的静态指令解码，仅发现这两条显式 XMM6/XMM7 目的寄存器写入。需要追溯的区间因此落在这些载入与本次 insert_full 入口之间的调用、恢复或外部状态变化上。现场尚不能指出其中第一个异常事件。

## 当前活动帧保存区核对

| 区域 | 本次地址范围 |
|---|---|
| 新 RawTable | [0x53aaceede8, 0x53aaceee08) |
| 当前 insert_full 保存的 XMM6/XMM7 | [0x53aaceeb10, 0x53aaceeb30) |
| status 保存其调用者 XMM6/XMM7 | [0x53aaceeff0, 0x53aacef010) |

三者两两互不重叠。insert_full 的 unwind 保存偏移为 RSP+0xa0、RSP+0xb0，与实际序言的 RBP+0x20、RBP+0x30 相符。此结论覆盖表中当前活动帧，已返回子函数曾使用的栈空间仍缺历史。

## 直接被调函数中的向量保存与恢复

原 status 在构造位置之前存在 22 个具有静态 RuntimeFunction 范围的不同直接调用目标。下表列出其中显式写 XMM6/XMM7 的 4 个函数。函数完整字节与同次转储逐字节一致，保存指令、恢复指令与 unwind 元数据相互对应。

| 函数入口 RVA | 函数 | XMM6 / XMM7 保存与恢复的 RBP 偏移 | unwind 对应的 RSP 偏移 |
|---|---|---|---|
| 0x84f050 | indexmap Core::insert_full | +0x20 / +0x30 | +0xa0 / +0xb0 |
| 0x863990 | AdmissionState::status | +0x1d0 / +0x1e0 | +0x250 / +0x260 |
| 0x5cf780 | NativeLifecycle::network_probe_observation | +0x5e0 / +0x5f0 | +0x660 / +0x670 |
| 0x34b9a0 | LocalKey::with 的当前单态实例 | +0x1e0 / +0x1f0 | +0x260 / +0x270 |

这些函数保存后临时使用向量，正常尾声从原偏移恢复。静态偏移核对未发现确定的漏保存或错槽恢复。该列表限定为原 status 的一层直接调用目标，其嵌套调用、间接调用、尾跳以及实际执行分支尚未全部重建。因此这 4 个函数是明确的保存恢复审查点，不能将它们列为排他的故障来源。

当前映射没有将这两次模板写入连接到某个 Windows→SysV 跳板。历史跨 ABI 审计仍按其原覆盖范围有效。跨 ABI 归因还需要该次实际调用边界与恢复错误的证据。

## 与 PID34452 RBP 的对照

两次现场共享原 a254 和 AOEM966。PID34452 的失败活动链涉及 indexmap `Core::insert_full`（同一 RVA 0x84f050），随后经过向量增长函数 RVA 0x84ed90、Rust realloc 和 Windows HeapReAlloc，最终在 `RtlpReAllocateHeapInternal` 的检查处出现 RBP 帧关系偏移。

PID3848 在同一 insert_full 的原始表扩容调用分支进入 reserve_rehash，调用点 RVA 0x84f3a9，随后因巨大申请进入 Rust abort。本次已检查的 RBP 帧关系与保存槽相符。共同的 indexmap 上游函数已经明确，进入分配器的子路径分别是表分配和 Vec 重分配。现有两份终态未提供同一个跨 ABI 跳板或同一条异常恢复指令的证据。

## 同次主对象进一步限定执行区间

直接按原 Bucket<String, Value> 的 104 字节布局读取主对象条目，50 个 key 的字符串长度、地址及条目顺序相互对应。主对象容量为 56，长度和 items 均为 50，bucket_mask 为 0x3f。

signature_work 已存在，其容量为 7，长度和 items 均为 6，bucket_mask 为 7，growth_left 为 1。机器码在 RVA 0x777126、0x77712d 用相同 XMM6/XMM7 和相同局部表地址构造该对象，之后将它移入主对象。stage_profile 和 native_network_probe 也已存在。后者容量为 14，长度和 items 均为 8，bucket_mask 为 0xf，growth_left 为 6。

主对象没有 native_io_costs、native_delivery_traffic、native_send_polls、native_receive_totals、native_receiver_queue。这些是完整 50 项 key 清单的结果。正常源码分支下，这些可选处理没有产出条目。对象结果与最终 OnceLock 内存可用于缩小分支范围，仍需与历史寄存器值区别。尤其不能仅因 traffic::snapshot 的 LocalKey::with 存在于静态函数体，就宣称本次执行了它。

按正常指令路径关联对象生命周期，可聚焦 signature_work 先行构造到 archive_work 失败构造之间的后续处理。这个更小的代码范围仍含可选分支，公开 JSON 保存了静态调用位置。signature_work 的完整合法结果支持先前构造正常，当时 XMM 值的直接采样仍然缺失。

对主对象及其 map 类型子对象共解析 16 个表，未找到 entries/ctrl 等于两个异常指针的表。遍历范围没有包含数组内部、已分离或已释放对象，因此两个数值的真实原始对象所有者仍未知。

## 旧保存槽已经由当前帧复用

按 status 的固定 RSP 和三个已返回子函数的序言计算，network_probe_observation、LocalKey::with、AdmissionState::status 的 XMM6 保存地址均为 0x53aaceeb20，XMM7 保存地址均为 0x53aaceeb30。这些是各个调用实例按静态帧关系推算的槽位。

当前活动 insert_full 的序言已经将 XMM7 写入 0x53aaceeb20，将 XMM8 写入 0x53aaceeb30。原机器码的保存指令分别位于 RVA 0x84f071、0x84f06b。此刻的槽内容归属当前帧，先前子调用保存的值已被复用覆盖。把当前槽重新标成先前函数的恢复源会误建历史执行链。

直接写表指令已定位，首次让 XMM6/XMM7 变为异常模板的调用或恢复事件仍缺记录。

## 字段数据断点的适用性

本次 table 的四个 qword 地址均按 8 字节对齐，两个关键值可分别由 bucket_mask 与 items 地址的写监视覆盖。地址依赖此调用的线程栈和帧实例。当前进程已经退出，DMP 中的历史地址不能直接用于新的活动调用。

写监视首先会遇到上面的合法构造指令，其证据价值集中在当时写入值。XMM6/XMM7 更早的首次改变仍须在具体调用或恢复边界取得。当前离线分析没有选择新的运行或采集入口。

## 本轮交付边界

公开 JSON 保留字段偏移、原始元数据字节、具体写入指令、候选调用 RVA、保存恢复指令及元数据。原始 DMP、调试器输出和临时离线解析脚本继续在本机保存。原库、身份及已有失败现场保留。双中继继续暂停。

已经定位直接写表的两条初始化指令，首次让模板向量异常的指令仍未定位。当前没有具备证据支持的生产修复。
