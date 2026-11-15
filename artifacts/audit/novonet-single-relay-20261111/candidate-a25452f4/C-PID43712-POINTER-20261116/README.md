# PID43712源指针传播的定点离线约束

## 结果

首次非法指针定义仍未定位。本轮补齐了活动Formatter的实际writer目标和配置，以及reserve局部R14数据流。
完整转储SHA-256仍为6752ad40c591c2a5e9fde9f8d788ae14150cf08463647173a0b14ff585495ebe。
仅分析既有转储、原始a254 EXE和已有诊断副本，没有运行节点、样例、调试器附加或新采集工具。
PID44948保持独立归档，本轮没有将任何43712的值加入其历史。

## 1. 从活动Formatter解析实际动态目标

由仍活动的core::fmt::write帧及0x1e524d9的lea rdx,[rbp-0x10]，定位Formatter为0xf3bd3ee7d0。
其当前writer self=0xf3bd3ee860，vtable=0x7ff608be3610。
vtable+0x18实际指向0x7ff60825cc60，即EXE RVA0x1e4cc60的String::write_str，与活动调用栈及当前String对象一致。
这里已经解析了本现场的实际动态对象和函数指针。

Formatter当前flags=0x69000020、width=2、precision=0。
LowerHex活动第五参数仍指向可读的两字节数字串，第六参数长度为2。
按这些当前值代入pad_integral机器码，正数符号为0x110000哨兵，无alternate前缀，内容宽度2已经满足width2。
对应路径为0x1e52fae进入prefix helper，helper在无符号、无前缀时直接返回，然后0x1e52fde取局部源指针，经0x1e52ff5尾跳实际writer。
这项分支重建以当前Formatter和参数为条件，没有取得历史分支轨迹，未排除Formatter曾被改写。
不能把它标成该次执行全部分支已被动态观测。

## 2. reserve究竟可能怎样影响R14

String在0x1e4ccaa将源RDX保存到R14，0x1e4ccb3调用reserve，0x1e4ccb8再把R14恢复到RDX。
当前R14与非法RDX均为0xbc2520b34e04。
从实际PE函数边界提取reserve 0x24334a0～0x2433510与grow helper 0x2433410～0x243349e，二者本体均没有R14写入。
这项结论仅覆盖本体指令，嵌套调用的寄存器保持仍需历史值约束。

grow helper的分配分支通过0x1db3c0跳到0x1e2d1f0，再经0x1e397a0进入系统堆边界。
这两个本地函数本体同样没有R14写入。
本DMP的IAT解析结果为GetProcessHeap→kernel32 RVA0x23460，HeapAlloc→ntdll RVA0x31240。
这里停止扩展系统DLL函数清单，当前现场没有系统调用前后的历史R14，不能据此归因系统堆。

grow helper还存在重分配分支。该分支固定传alignment=1，重分配实现0x1e2d290的对应路径保存R14后在0x1e2d2c5恢复，保存位置与push/pop布局相符。
高对齐分支确有R14赋值，其进入条件为alignment>=17，与上述固定参数不同。
静态保存恢复相符不能证明历史保存值正确，也不能证明该次执行选了哪一条分配分支。

## 3. 退役分配帧当前残留的局部核对

按String、reserve和grow序言计算的退役栈位置，当前仍有以下数据。

| 位置 | 当前内容 | 使用边界 |
|---|---|---|
| 0xf3bd3ee698 | EXE返回RVA0x24334e2 | 与grow返回reserve的位置相容 |
| 0xf3bd3ee648 | EXE返回RVA0x243346a | 与分配分支返回位置相容 |
| grow结果槽0xf3bd3ee6c8起 | tag0、pointer0x2124b61cd20、size8 | 与当前String buffer/capacity相符 |

这些退役槽残留支持所计算的帧位置及分配路径形状，仍缺同次调用实例的写入历史。
本报告没有把它们升级为已记录的分配入口、返回事件，或reserve期间R14未变的证明。
pad的旧源槽0xf3bd3ee6f0已被后续帧复用，仍无法恢复其尾跳前保存的源值。

## 4. 首次变化仍卡在哪个边界

本次明确解析了实际writer；按当前配置重建的正常路径不需要填充循环，prefix helper也不会发起嵌套writer调用。
现有代码没有显示reserve本体会把某个计算结果直接写入R14。
仍缺两个关键时点的值。

- pad局部源指针重载及writer入口时的RDX。缺失导致无法证明非法源是否已经进入String。
- String的R14保存之后、reserve嵌套调用前后及恢复RDX时的值。缺失导致无法证明变化是否发生在reserve期间。

当前有效上游缓冲区、当前合法String描述符和当前非法RDX，不能代替这两个历史边界。
没有找到首次破坏指令，没有证实ABI违约，没有形成具备证据基础的最小修复。
本轮到此保留取证边界，没有继续扩大静态函数数量或重新启动冻结入口。

## 复核与保护

functions.json列明本轮提取区间、代码SHA-256和R14目的寄存器指令位置。
这些区间逐字节与PID44948对应代码页及哈希已核验的原a254 EXE一致，比较只用于代码一致性。
pointer-constraints-public.json明确区分当前值、条件重建和缺失历史，public-sha256.json覆盖本目录所有公开文件。
仅脱敏元数据与反汇编进入Git，DMP及离线分析程序留在本机target隔离目录。
原库、身份和两个失败现场保留，生产代码没有修改，双中继继续暂停。
