# C Phase 3：调用边界与寄存器完整性（2026-11-13）

**未找到原故障的第一处参数失真边界。** 六个关键函数在875/a254之间，去除PC相对
重定位后的指令编码、内部控制流偏移、unwind固定头/动作编码均一致。匹配到相同
函数体的独立样例完成6组实时断点检查，48项参数/寄存器观察检查通过，未复现原故障。
这是限定范围内的阴性结果，**不是节点修复或C恢复资格**。

本轮只编译/运行独立格式化小程序，不启动NOVOVM、不访问原库、不运行TTD、不更换
DLL、不操作身份、不恢复双中继。没有生产代码修改。原始转储继续留在本机。

## P0：原始a254 PDB仍缺

本轮重新fetch Git，开始分析时origin/main仍为fda6017c，无新的A符号交接。
重新核验E盘PDB为C94A5E99-4CFF-4094-8B63-0663EF0F9C21/age1；本地另一份node
PDB为04567E74-E1ED-4222-8251-304097279D56/age6，均不符合所需符号。

仍需从原a254构建目录取得 **E16D0B11-47CA-4EBF-B482-C91380CC57F8 / age1**，
对应EXE SHA256为1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d。
当前会话没有A原构建目录的访问通道。本机符号未找到不代表A磁盘不存在该文件。
新fixture PDB仅用于fixture，绝不用于解释a254，也没有重新编译原节点来替代符号。

## P1：两版机器码与隔离样例的匹配范围

| 函数 | 875 RVA | a254 RVA | fixture RVA | 字节数 |
|---|---|---|---|---:|
| format_inner | 0x1e47c10 | 0x1e4ce70 | 0x11070 | 399 |
| core::fmt::write | 0x1e4d120 | 0x1e52380 | 0x12690 | 440 |
| u8 hex formatter | 0x1e4b090 | 0x1e502f0 | 0x124b0 | 112 |
| pad_integral | 0x1e4dca0 | 0x1e52f00 | 0x12be0 | 1274 |
| write_prefix | 0x1e4dc40 | 0x1e52ea0 | 0x12b80 | 95 |
| String writer | 0x1e47a00 | 0x1e4cc60 | 0x11000 | 97 |

范围来自原PE的RUNTIME_FUNCTION，不按最近导出符号猜边界。
[完整反汇编](code-comparison-assembly.txt)、[逐函数比较](code-comparison-v2-public.json)、
[内部跳转对照](control-flow-public.json)。875使用匹配PDB命名；a254归属仍是原镜像
代码对应关系，不伪称精确a254源码行已补齐。

归一化仅抹去PC相对call/jump立即数与RIP disp32；保留寄存器、栈偏移、普通立即数。
另核对内部跳转落点、format_inner→fmt、byte→pad、pad→prefix及String vtable的
实际writer目标，共12项跨镜像目标核对。小写数字表和0x前缀也相符；大写formatter
虽有相同归一化指令，常量不同，不混作本次路径。

三份镜像不是整体相同：上层业务、全局分配器接线、外部库和执行上下文未因此等同。
fixture没有AOEM966、并发快照或原节点栈。reserve/分配器及其他外部callee没有被
递归全部验为相同。unwind等值只指固定头与动作编码，未涵盖全部语言异常handler。

## P2：Windows x64 ABI检查

依据微软[x64调用约定](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention)、
[序言/尾声](https://learn.microsoft.com/en-us/cpp/build/prolog-and-epilog)和
[unwind结构](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64)核对。
这里只解释该编译结果，不声称Rust内部ABI永久等同C ABI。

| 函数 | 保存GPR数 | 固定栈分配 | 帧偏移 |
|---|---:|---:|---:|
| format_inner | 6 | 0x58 | RBP=RSP+0x50 |
| fmt::write | 8 | 0x48 | RBP=RSP+0x40 |
| byte formatter | 1 | 0x40 | RBP=RSP+0x40 |
| pad_integral | 8 | 0x58 | RBP=RSP+0x50 |
| write_prefix / String writer | 各5 | 0x20 | RBP=RSP+0x20 |

三份镜像共18个函数记录：push与unwind一致，pop逆序恢复，帧偏移/栈分配一致；
从正常入口RSP mod16=8推算函数体mod16=0，固定分配均容纳32字节shadow space。
byte formatter还在shadow space之后传文本指针和长度。未发现这些生成序列的
固定ABI违规，不能据此证明原崩溃执行时寄存器一直正确。

[静态检查](abi-checks-public.json)、[寄存器列表](abi-static-public.json)、
[unwind明细](unwind-descriptions-public.txt)。六函数未直接写XMM6–XMM15，不能
将外部callee的保持情况一起判为已验。PID31296另一函数的RBP偏差仍沿用
[Phase2原始证据](../C-original-dmp-phase2-20261113/README.md)，未被本次结果解释。

### 尾调用不是缺失一次恢复

pad先准备RCX=receiver、RDX=text、R8=len，再回收0x58并pop8个GPR，最后执行
**48 FF E0（REX.W jmp rax）**。恢复RDI等不会覆盖已准备的RCX/RDX/R8；保留的上层
返回地址让writer直接返回byte formatter。微软[RtlVirtualUnwind文档](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nf-winnt-rtlvirtualunwind)
明确列出rex.w jmp reg为x64 epilogue标记，不能仅凭间接跳转认定unwind非法。

write_prefix也有匹配的保存/恢复。此次6个输入没有符号前缀且两位宽度已满足，
该helper调用前后观察值一致；原DMP最终值不能替代其原始调用入口或分支历史。

## P3：一次隔离边界实测

[样例源码](boundary_fixture.rs)，[编译器](toolchain.txt)为1.94.0/4a4ef493e。
参数opt-level=3、debuginfo=2、overflow-checks=yes；不编译NOVOVM。
begin/end导出仅用于独立调试标记，不是给生产程序新增FFI。
目标函数经过上面的代码匹配，不只凭同一Rust版本宣称匹配。

普通CDB启动fixture一次，无TTD、注入式录制或节点附加，逐点读取：

1. fmt::write入口的预期receiver。
2. 调用byte formatter前的Formatter.buf。
3. pad入口的Formatter、文本指针和文本长度。
4. prefix调用前后的RBX/RBP/RDI/RSI/R12–R15与XMM6–XMM15。
5. pad尾调用准备完成后的RCX/RDX/R8。
6. writer入口的RCX/RDX/R8。

输入 **b9、56、10、7f、80、ff**；前两项对应原DMP文本。每组8项检查通过，共48项；
6组都有完整观察，目标退出0、CDB退出0，无AV。
[观察值](boundary-observed-public.txt)、[逐项结果](boundary-results-public.json)、
[退出记录](boundary-debugger-exit.json)、[断点命令](boundary-check-command-record.txt)。
公开观察日志仅增加事件换行，未改数值。断点RVA仅适用于本次匹配fixture，重编译后
必须重新比对，不能直接套用到其他程序。

b9示例：receiver一直为0x48c18ff690，Formatter为0x48c18ff600，text为0x48c18ff5be，
len=2。尾声前RSP=0x48c18ff4e0，writer入口=0x48c18ff578，相差0x98，正好回收pad帧。
这些是fixture地址，不与原进程绝对地址比较。

限制：6个两位输入、一个普通用户进程；没有长期运行/原节点全栈。XMM观察值本次
均零，未播种非零哨兵，只声称前后相等。断点会改变时序，不能排除偶发并发、外部
破坏或硬件问题。**未捕获第一次失真，也未修好C。**

### 分析过程中的失败记录

- 初次编译的forbid(unsafe_code)拒绝no_mangle标记；未执行。调整lint后通过，
  无unsafe运行时操作。[初次](build-public.json)、[第二次](build2-public.json)。
- 比较器最初信任decoder的disp_size，对部分66前缀SSE只抹去2字节，造成fixture
  pad假差异；按RIP固定disp32修正后匹配。[保留差异](fixture-pad-diff.txt)。
  这是分析器问题，不是生产机器码缺陷。
- 日志解析最初误取命令回显，修正为只取真实CASE数字记录；未重跑fixture。
  [原私有日志哈希](local-log-hashes.json)保留。整理报告的首次大批量写入被工具
  自动审批拒绝，未执行；随后使用较小的证据复制和文件编辑完成，未涉及运行操作。

## P4：交付与最小后续

- 第一处参数不一致边界：**未捕获**。原DMP只有终态，本次fixture边界全部正常。
- 原3228的RDI、37796的receiver/RDX、31296的RBP异常仍成立，不能回填为入口值。
- ABI违规：**未证实**。未把所核对函数通过扩大成全程序/所有callee ABI通过。
- 875/a254所列六函数无重定位以外的指令差异；fixture未加载AOEM966，不能因此
  宣称AOEM与更早的破坏无关。
- 最小生产修复：**暂无证据支持，未提交**。不绕过String、IndexMap或memcpy。

下一最小动作仍是取得a254原PDB，在同一37796转储核对外层调用/局部类型，不重编译
替代。当前没有依据扩大为全节点长时录制，也不把反复执行已通过fixture当新证据。
若后续找到具体前置调用/非易失寄存器破坏假设，再把该条件接入同一独立fixture，
要求记录首个边界差异并形成同触发的修前失败、修后通过回归。

**C NOT READY；A不得发送双中继对象。** Git提交只同步证据，不是A已签收或节点恢复。
