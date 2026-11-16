# Phase 29：PID38324 XMM6 恢复异常离线专项分析

基于39c8bd3f。仅使用现有转储、匹配原二进制和既有历史证据；未启动节点、未访问原库/身份、未运行样例或采集器、未修改生产代码、未恢复双中继。

结论：已还原故障指令、当前保存槽及完整序言/尾声/展开关系。没有发现错误保存偏移、静态槽重叠或展开元数据不匹配；访问异常的直接底层原因及首次破坏仍未定位，没有可证实的生产修复依据。

## P0：异常指令和地址

| 项目 | 同次原始转储结果 |
|---|---|
| 文件 | novovm-node.exe.38324.dmp，229673411字节 |
| SHA-256 | c24346b575d048469a51fad1f897d82bc5947c1c3b5d57e7b2f703a779cc4249 |
| ExceptionCode | 0xC0000005 |
| ExceptionAddress / RIP | 0x7ff770faf36d（原a254 RVA0x84f36d） |
| ExceptionInformation[0] | 0，读 |
| ExceptionInformation[1] | 0xffffffffffffffff；这是记录值，不替换为推算地址 |
| 完整指令 | 0f 28 75 20：movaps xmm6,xmmword ptr [rbp+0x20] |
| RSP / RBP | 0x3e48fee210 / 0x3e48fee290 |
| 计算出的16字节内存区间 | [0x3e48fee2b0,0x3e48fee2c0) |
| 对齐与页边界 | 地址mod16=0，不跨页，规范地址 |
| 转储内存信息 | 已提交、PAGE_READWRITE、Stack；16字节实际存在 |
| TEB栈范围 | [0x3e48f9c000,0x3e49000000)，操作数位于其中 |

ModRM0x75指定RBP基址、XMM6目的寄存器，disp8=0x20；不是写内存，也不是恢复XMM7。故障指令与原EXE逐字节相同，无VEX/EVEX前缀。XMM6–XMM15已从ExceptionStream完整提取，原始值保留本机私有证据；CDB逐寄存器读取十项成功。第一次CDB多寄存器命令语法失败原样保留，修正后无语法错误，不把失败输出算作验证。

在捕获的进程内存中，找到与ExceptionStream相同的ExceptionRecord，以及完整1232字节相同的异常CONTEXT；未发现解析字段错位。ThreadList上下文位于后续WER等待栈，不能与故障上下文拼接。

转储中可读不等于故障瞬间可读。该用户态DMP不包含故障时页表、CR2、CPU陷阱号/错误码、CR0/CR4或段描述符历史，不能从全1的ExceptionInformation[1]单独判定CPU #GP、未对齐、页故障、硬件错误或异常上下文此前是否被改变。

## P1：保存/恢复与当前帧

所属RuntimeFunction：[RVA0x84f050,0x84f46a)，原PDB关联IndexMap Core::insert_full。0x84f36d位于正常恢复序列；RET尚未执行，当前帧仍活动，不是已经返回的子调用槽。

| 步骤 | 原指令 / 结果 |
|---|---|
| 入口 | 八次push，共0x40字节 |
| 局部空间 | RVA0x84f05c：sub rsp,0xd8 |
| 帧基址 | RVA0x84f063：lea rbp,[rsp+0x80] |
| 保存XMM6 | RVA0x84f076：66 0f 7f 75 20，movdqa [rbp+0x20],xmm6 |
| 恢复XMM6 | RVA0x84f36d：0f 28 75 20，movaps xmm6,[rbp+0x20] |
| 其他恢复 | RVA0x84f371恢复XMM7；0x84f375恢复XMM8 |
| 收尾 | add rsp,0xd8；逆序pop八寄存器；RVA0x84f38d ret |

由当前RSP和正常序言推算入口RSP=0x3e48fee328，调用者RSP=0x3e48fee330；入口保存的调用者RBP=0x3e48fee3b0，返回地址=0x7ff77134f3e0，落在SerializeMap::serialize_value调用后。该关系与活动栈展开一致。推算入口不是历史实际入口采样，不能声称观察到了当时的XMM6或保存写入。

展开信息RVA0x297d43c：version1、flags3（有handler标志，无CHAININFO）、prologue0x2b、frame register RBP、frame offset0x80；UWOP_SAVE_XMM128(XMM6,0xa0)、XMM7/0xb0、XMM8/0xc0，以及ALLOC_LARGE/0xd8、八个PUSH_NONVOL均与机器码匹配，展开信息字节也与DMP相同。依据Microsoft x64 unwind规则，XMM6槽为最终RSP+0xa0，即0x3e48fee2b0，与保存和恢复一致。

完整正常CFG在故障点只有一组栈状态：RSP为局部空间基准，RBP=RSP+0x80；只有一个正常RET，栈恢复至入口RSP，没有未解析的显式栈调整或尾跳退出。返回地址来自活动调用者，恢复槽位于本帧已分配区；正常嵌套callee帧位于当前RSP以下，当前调用的出参区域也不覆盖RSP+0xa0。该函数的异常handler RVA0x23ae7cd经导入跳板解析为VCRUNTIME140!__CxxFrameHandler3；handler存在不代表本次故障前执行过异常展开，正常CFG也不覆盖未知非局部恢复历史。

对完整函数中基于RBP/RSP的显式写入进行区间检查：与XMM6保存区相交的仅0x84f076这一条保存指令。保存区、XMM7区、XMM8区相邻而不重叠。此检查不覆盖损坏的指针别名、其他线程、未记录的callee违规或非局部上下文恢复；静态保存/恢复一致不能证明保存槽历史未被写坏。

更关键的是MOVAPS只搬移比特，Intel手册列出的SIMD浮点异常为None。保存槽内浮点位型或指针内容异常，本身不会让这条内存读取因其“值坏”而访问违规。因此当前不能从恢复槽内容直接跳到本次AV根因，更不能把MOVAPS改成MOVUPS当作修复。

## P2：历史对照与共同路径

| 现场 | 原始结果 | 与本次的关系 |
|---|---|---|
| PID38324 / a254 | RVA0x84f36d、insert_full+0x31d；对齐、RBP−RSP=0x80；读、记录地址全1 | profile事件JSON序列化，当前字段seq；XMM6恢复前失败 |
| PID13020 / 875 | RVA0x65f59d、insert_full+0x31d；同样对齐、RBP−RSP=0x80；操作数字节存在；读、全1地址 | 是更早的“对齐恢复异常”样本，不是仅有PID31296那类未对齐故障 |
| PID31296 / 875历史报告 | 相同旧版恢复RVA，但操作数mod16=8、帧关系异常 | 保持独立，不将其未对齐解释推广到38324/13020 |
| PID39152 / 原a254隔离BASE | archive_work首次reads_started插入；XMM7高64位已异常；18TiB+16B申请后FastFail7 | 同一status和insert_full函数，但处于reserve_rehash分支，尚未到恢复指令 |

875的完整insert_full范围[0x65f280,0x65f69a)和a254的[0x84f050,0x84f46a)均完整解码；只归一化相对call/jump与RIP-relative位移后，整个函数完全相等，SHA-256=86255abdbb2994db4282853cdf3c9efdb799bb1a5a4d1db0231b6fac8605db15。寄存器、栈偏移及其他操作码没有被归一化。这建立了13020与38324在同一编译操作和相同异常形态上的直接对照，但尚未证明相同首次原因。

选定当前函数只有直接call，无未解析寄存器call。memcmp导入跳板RVA0x23ae7d3在同次DMP中解析到VCRUNTIME140!memcmp；释放shim0x1db3d0直接跳至原分配器0x1e2d250。未在这些边界发现Windows→SysV转换，也没有共同手写ABI违规证据。两个现场的活动栈都不直接显示AOEM帧；这不能排除更早/并行原生代码影响，也不构成全项目FFI无错证明。

本次原insert_full、profile快照、status三个完整RuntimeFunction均与DMP代码字节一致。PID39152保留的是后续archive/OOM终态，不能把它的坏XMM7或events[804]指针关联当成本次XMM6恢复前的历史数据。

## P3：修复判断及证据缺口

未发现可证实的错误栈偏移、错误保存地址、静态槽复用、unwind不一致或共同ABI违规。故障时上下文给出的地址满足已核验的地址/对齐条件，但异常记录报告的全1地址与该有效地址不一致。当前不能给出造成这一不一致的具体首条指令。

本轮不修改IndexMap、Hashbrown、String、MOVAPS/保存恢复代码或分配器，不重启C，不恢复双中继。离线结论止于以上证据边界。若以后获得硬件/内核陷阱状态或同次实际保存/返回历史，才可能判定故障时访问条件、上下文变化或具体违规来源；本报告不新建采集器、不自动安排运行。

原库/身份未打开，所有原始转储只读。原始FullDump和PID39152/PID13020文件分析前后哈希核对记录见preservation-public.json。完整XMM6–XMM15、异常记录副本地址和调试器内存日志仅存target/c-phase29-offline-20261116/analysis-private，不进入产品或公开Git。脱敏结论、指令和结构结果进入本审计目录。

参考：
- Intel SDM Vol.2B MOVAPS，印刷页4-48至4-51，PDF页782–785：https://cdrdv2-public.intel.com/774492/325383-sdm-vol-2abcd.pdf
- Microsoft x64 unwind：https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64
- Microsoft x64 ABI：https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention
