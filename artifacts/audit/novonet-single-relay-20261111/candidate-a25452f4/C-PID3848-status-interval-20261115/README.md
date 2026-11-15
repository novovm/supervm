# PID3848 的 status 有限区间与调用关系

## 区间边界

本轮将原 a254 status 函数中相关机器码列成完整有限清单，并用 PID3848 同一 Full Dump 逐条比较字节。

- signature_work 局部空表初始化开始于 RVA 0x777105，向量写入位于 0x777126/0x77712d。
- signature_work 完成向主 status 对象搬移的最后一条写入位于 RVA 0x777ba9，随后从 0x777bad 开始后续处理。
- archive_work 空表向量写入位于 RVA 0x77823e/0x778245，清单到 0x77824c 前停止。
- Core 地址为 0x53aaceedd0。RawTable 是 Core+0x18，字段范围为 [0x53aaceede8, 0x53aaceee08)。

完整清单覆盖这段代码的静态分支及其跳到同一 status 函数内的冷路径。original-interval-disassembly.txt 保留地址、机器码和指令。interval-public.json 保留每个调用、分支和终止位置。清单按地址排序，原始执行顺序仍须另有记录。

signature_work 的合法对象结果没有保存其完成时的 XMM6/XMM7。它在初始化后仍执行多次插入、释放及主对象发布调用，这些调用存在改变缓存向量的机会。因此清单同时保留局部构造区间和发布后区间。只从发布完成以后开始查，会遗漏构造过程中已出现、但直到下一次使用模板才暴露的寄存器异常。

## 发布后的转储条件一致路径

主 status 对象已有 signature_work、stage_profile、native_network_probe，缺少 IO 和 asset 的可选字段。转储中的 profile OnceLock 状态字为 0、缓存 bool 为 0，IO cost 的两个值也都是 0。故障线程的 hash RandomState TLS 初始化字节为 1。TLS 中的种子内容没有导出。

按这些条件和原源码正常顺序，下列路径连接 signature_work 发布完成与 archive_work 初始化。

| 顺序 | call RVA | 原目标 RVA | 功能 | 证据级别 |
|---|---|---|---|---|
| 1 | 0x777bb4 | 0x75c4a0 | profile::snapshot，源码及调用位置关联 | 静态目标已解析，stage_profile 结果存在 |
| 2 | 0x777bd3 | 0x1d8a1d0 | 为 stage_profile 查找或插入 JSON 字段 | 静态目标已解析，对象结果支持 |
| 3 | 0x777be2 | 0x3530d0 | 释放被替换的 Value | 静态目标已解析，旧值的历史未保存 |
| 4 | 0x777c31 | 0x5cf780 | network_probe_observation | 静态目标已解析，返回对象已在主对象中 |
| 5 | 0x777c60 | 0x1d8a1d0 | 为 native_network_probe 查找或插入字段 | 静态目标已解析，对象结果支持 |
| 6 | 0x777c6f | 0x3530d0 | 释放被替换的 Value | 静态目标已解析，旧值的历史未保存 |
| 7 | 0x777cac | 0x15ac480 | native_io_cost_snapshot | 静态目标已解析，IO 缓存关闭、产出字段缺席 |
| 8 | 0x7781fd | 0x1e3a8b0 | 取得本线程 hash RandomState TLS 地址 | 静态目标已解析，初始化字节为 1 |

这是 8 个调用位置、6 个不同目标。candidate-after-publication-path.txt 列出了按这些条件选择的连续指令及跳转。候选路径没有逐调用寄存器采样，也没有被标成历史指令轨迹。TLS 最终初始化字节为 1，也可能来自本次先执行冷初始化再置 1，因此不能凭最终字节排除冷初始化；对应调用仍保留在完整清单中。snapshot 条件在发生内存或寄存器异常的进程中只能按其证据范围使用。

RVA 0x75c4a0 的原 PDB 近邻名字显示为 profile::elapsed+0xd0。这里按 RuntimeFunction 边界、原源码调用顺序和 stage_profile 赋值位置关联 profile::snapshot，不将近邻符号名当成准确函数名。

## 每类目标对 XMM6/XMM7 的具体意义

| 目标 | 现有机器码核对 | 尚缺证据 |
|---|---|---|
| profile::snapshot | 函数体没有显式写 XMM6/XMM7，嵌套 JSON 构造可进入 Core::insert_full | 嵌套动态目标、实际保存槽和调用前后向量 |
| index_or_insert | 函数体没有显式写两份向量，内部 IndexMap::entry 会暂用并恢复它们 | 此次具体表操作、系统分配器返回状态及运行期间槽内容 |
| drop_in_place<Value> | 函数体没有显式写两份向量。按新字段的默认 Null 路径可直接返回，其他变体有释放路径 | 这两次被替换旧值的历史参数，不能用最终的新对象冒充旧值 |
| network_probe_observation | 函数体及其快照/clone 子调用确实使用 XMM6/XMM7，所查静态保存恢复偏移配对 | 实际嵌套分支、帧基址和历史恢复内容 |
| native_io_cost_snapshot | 缓存关闭时走简单 None 返回。该返回路径没有 XMM6/XMM7 写入 | 配置缓存的历史读取值。完整可选路径仍列在静态清单中 |
| RandomState TLS getter | 原叶函数只有索引、GS/TLS 地址读取及 ret，共 5 条指令，没有向量写入 | 调用者对初始化字节的历史读取。冷初始化调用仍列在静态清单中 |

仅按发布后候选路径，AdmissionState::status 位于更早位置，traffic::snapshot 的 LocalKey::with 位于可选 IO 路径。它们在全函数静态清单中的存在与本次较小区间的证据分别记录。

当前可继续聚焦两个快照所有者、JSON entry 以及相应分配/恢复边界，同时保留 signature_work 自身调用作为前置候选。已解析的直接目标都位于原 EXE。具体跨 ABI 跳板、动态目标不匹配或错误保存恢复仍没有同次执行证据。

## 已有的实际执行证据放在正确时序

最终活动栈和同次 stderr 记录证明 archive_work 首次插入进入原 Core::insert_full（RVA 0x84f050），再进入 reserve_rehash 与分配失败处理。上一轮已从该 stderr 解析一次 core::fmt::write 到 Backtrace formatter，再到 RtlCaptureContext 的事件，该事件处于巨大申请失败之后。

发布后八个调用没有逐调用的入口、返回向量记录。合法对象结果可以支持正常源码顺序，无法恢复已被复用的旧保存槽。当前没有证据将其中某个函数指定为首次破坏者，也没有可证实的生产修复。

PID34452 与 PID3848 共享 Core::insert_full 的函数入口，但随后分别进入 Vec 重分配与原表扩容路径。这里的机器码清单没有建立共同异常恢复指令。

## 交付与保护

本轮没有扩展新的递归函数审查，复用了此前对各目标的静态保存恢复结果，新增交付是该有限区间的完整代码、调用矩阵及条件分支清单。原库、身份及失败现场保留，双中继暂停。完整转储及离线解析脚本留在本机，公开清单只含相关代码、元数据和判断依据。
