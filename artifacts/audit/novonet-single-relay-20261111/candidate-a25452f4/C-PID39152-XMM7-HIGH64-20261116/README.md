# PID39152：XMM7高64位与同次现场对象关联

本轮仅离线读取6473e7b1取得的原始Full Dump及a254机器码，不启动节点、不运行新样例、不修改生产代码。原库身份及失败现场保留，双中继暂停。

## 新的直接对象关联

异常items/XMM7高64位为0x18e3c1f7510。扫描本次FullMemory流捕获的内存范围，发现该8字节模式共3处：

| 地址 | 归属 |
|---|---|
| 0x6b96feefc8 | 当前insert_full保存槽的XMM7高64位 |
| 0x6b96fef2a0 | status局部archive_work RawTable.items |
| 0x18e3a626648 | 堆上JSON对象的RawTable.ctrl |

第三处所属对象位于0x18e3a626630，entries容量8、长度8，RawTable mask15/growth_left6/items8。8个字段名是seq、elapsed_ns、wall_unix_us、event、height、purposes、peer、fragment；未导出字段载荷或身份内容。

该对象是数组基址0x18e3a618410、Value步长72、零基下标804的元素。数组描述符容量和长度均2048，位于0x18e37e728e0。其父JSON对象0x18e39344510的字段名events指向该数组；父对象字段为enabled/capacity/evicted/contended_events/scope/events。此结构与原a254 profile::snapshot的native_delivery构造一致。

这证明坏值在同次现场亦被一个结构合法的事件JSON表用作ctrl指针，提供具体对象关联。未恢复到更外层stage_profile的完整父链；不能据此声称已证明该对象是XMM7的历史载入源。扫描只覆盖DMP保留的内存流，不包含历史已释放内容或未捕获内存；异常CONTEXT本身为独立转储流，未计入上述3处。

## 父函数定义与边界

逐条解码原status从RVA0x772120至0x77824c的指令，并与本次DMP代码页逐条核对涉及XMM7的指令。直接定义XMM7的仅一条：

`0x7721f4: 0f103d2532d801 — movups xmm7,[rip+0x1d83225]`

其源RVA0x24f5420的16字节合法模板为全零。这是最后的静态合法定义，不是同次调用中动态观察到的“最后正确值”。后续0x7721fb、0x77712d、0x778245都是从XMM7写内存，不是写XMM7。

紧接模板的首个call位于0x772210，目标0x181810仅c3/ret，没有寄存器写入。第一个非平凡、缺少历史返回寄存器证据的边界是0x77221f→0x1db3c0→尾跳0x1e2d1f0（分配路径）。这是证据缺口的开始，不是发现该分配器有错。区间线性解码包含326个静态call位置，含条件分支，未视作全部实际执行，也未扩大递归审查。

## 指令语义与ABI

复用此前有限调用图中的XMM7候选，仅按新现场代码页核对26处写入：22处MOVAPS/MOVUPS完整覆盖128位，4处PCMPEQD xmm7,xmm7将128位置1。没有在该有限集合发现MOVD/MOVQ、VEX或高64位专写；系统DLL、未解析动态目标和异常恢复历史仍未闭合，不能外推到所有可能执行路径。

完整128位加载/恢复也可以得到“低64位0、高64位指针”，因此不能由结果位型推断只写了高64位。PCMPEQD自比较不能直接产生本次位型，但随后恢复仍需源内存历史。XMM目的MOVD/MOVQ清零高64位；legacy MOVSD寄存器源与内存源的高位行为不同，不能只按助记符分类。VEX清零YMM的高128位也不是清零XMM的高64位。具体语义依据[Intel指令手册](https://cdrdv2-public.intel.com/774492/325383-sdm-vol-2abcd.pdf)。

Windows x64要求被调用者保持XMM6–XMM15的低128位；YMM高128位的易失性不适用于本次XMM7高64位。见[Microsoft ABI](https://learn.microsoft.com/en-us/cpp/build/x64-software-conventions)。本轮未找到实际ABI违规、实际SysV跳转或异常恢复致错证据。函数内合法暂用和正常返回后未恢复必须区分。

status入口保存槽当前为全零，它保存的是进入status之前的值，且缺少历史；不能把它当成后续子调用的保存槽或返回记录。当前insert_full保存坏值发生在archive初始化之后，也不能用于倒推已经返回的旧调用。

## 结论与下一处证据缺口

本轮新增的是异常高64位与事件JSON表ctrl指针的同次现场关联；首次寄存器变化仍未定位。缺少该事件对象构造/复制/释放期间XMM7及保存槽的历史，以及之后实际返回到status的寄存器状态。优先围绕这一具体对象的构造和恢复关系，不扩展全项目清单，不重新启用冻结的调试器路线。

没有可证实的最小修复依据，不修改Hashbrown、String或分配器。最小功能触发组合仍未通过关闭/重开对照证明。

516份报告是四节点合计，故障节点最后完成128份；交易执行和实际归档读取计数为0。GPU未取得直接活动测量，不能表述为已证明没有任何GPU参与。status/profile/archive_work是本次异常暴露路径，不能等同于已找到内存破坏来源。
