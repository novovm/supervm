# C：a254 精确符号收到，PID37796 原转储续查（2026-11-14）

**已收到并独立核验 A 的原始 PDB，已完成同一 PID37796 的离线符号加载与参数复核。
首次参数错位/首次破坏指令仍未定位，无生产修复，C NOT READY。**

本轮覆盖此前“符号未收到”的状态；先前接收记录保留在
[receipt-before-pdb.md](receipt-before-pdb.md)，不是当前阻塞项。
A 对 e1e3a999 的签收已由0470f343确认；本报告推送不等于 A 已签收本报告。

## 1. 实际交付与加载

[实际哈希、GUID、退出码和限制](verification-public.json)。ZIP 4,709,244字节、
PDB 17,256,448字节均与A manifest完全一致；原EXE 48,457,216字节及原DMP
6,203,358字节的SHA256保持不变。ZIP唯一成员是novovm_node.pdb。

PDB、原EXE CodeView、原DMP模块记录均为
`E16D0B11-47CA-4EBF-B482-C91380CC57F8 / age 1`。
CDB `!lmi` 实际报告 `Symbols loaded successfully`、`private symbols & lines`。
没有 `/i` 强制不匹配加载，没有替代编译。PE checksum为0引起的checksum warning
与GUID/age匹配是不同事项，未据此跳过匹配检查。

原始崩溃仍是2026-11-13 10:15:56 +08:00，PID37796，线程31876，运行1:34:55；
这不是本轮重新运行得到的崩溃。三个离线CDB进程退出码均为0；具体查询失败另列，
不能用调试器退出0宣称所有局部变量均可恢复。

## 2. 故障使用点与最终快照

[公开调试器摘录](debugger-public.txt)保留 `!lmi`、`kn`、`.exr -1`、寄存器及选定槽；
原始日志、DMP/PDB和未筛选内存只留本机。

- 故障指令：`VCRUNTIME140!memcpy_avx_ermsb_Intel+0x68`，
  `00007ffcb1d41268: movzx ecx,word ptr [rdx]`。
- 异常类型读取违规，ExceptionInformation[0]=0、[1]=`ffffffffffffffff`。
- 指令实际源操作数来自 RDX=`8000000000000005`，RCX/RAX=`204496a0000`，R8=2。
  非规范源地址与异常记录中的全1地址分别记录，不篡改异常参数来使两者相等。
- writer的RDI=`e7e2ee648`；该地址处的三字依次为外层返回地址
  `7ff70a4565a0`、`204496a0000`、0。它不是正确的局部String。
- 按原机器码解释的正确String位于`e7e2ee500`，三字为cap=0、ptr=1、len=0；
  Formatter位于`e7e2ee470`，buf仍指向`e7e2ee500`。
  这只是崩溃时的保存状态，不能推出调用期间从未遭到改变。
- `e7e2ee638`恰好也保存`8000000000000005`，仅是值相同。
  没有指令历史证明RDX从该槽读取，不能称已找到来源。

## 3. 精确符号澄清了什么

核心路径已由原PDB命名：`format_inner → core::fmt::write → u8 fmt →
[pad_integral尾调用] → String::write_str → 内联Vec追加 → memcpy`。
`pad_integral`无独立物理帧，与其尾调用机器码一致；该次具体分支的历史寄存器
没有被转储记录，方括号不表示恢复出了一份真实历史帧。

外层能识别FromIterator、FnMut、Vec FromIterator、RpcLifecycle::status、
observe_tick及主循环等公共符号，但**不把最近符号名当成覆盖全部地址的函数名**：

- `0x7614af`显示为`profile::elapsed+0x50df`。
- `.fnent`实际范围为RVA`0x75c4a0..0x76330f`；elapsed公共符号起点为`0x75c3d0`，
  不在该范围内。不能由此声称elapsed函数崩溃，更不能把这一名称当成修复位置。
- 原机器码RVA`0x7614aa`调用`0xc1fe30`；RVA`0xa1659b`调用`0x98bb30`；
  后者RVA`0x98bc15`调用`format_inner(0x1e4ce70)`。
  [从PE函数起点解码的实际调用片段](outer-call-disassembly.txt)保留这些关系。

状态观察/快照与十六进制格式化是可继续审查的路径，具体业务闭包源码行仍不完整。
没有把AOEM、FFI、硬件或Rust标准库定为根因。

## 4. 符号本身的具体缺口

[PDB总摘要](pdb-summary.txt)的Has Debug Info/Has Types为true，**不表示每个模块
都有这些信息**。[16个node编译单元核对](node-module-debug-public.json)全部显示
`debug stream: 65535, # files: 0`。这是明确的模块调试流缺失，不只是把所有缺口
含糊称为“优化掉了”。标准库/其他链接模块的信息不能代替node模块的局部变量。

- `dv /t /v`在核心帧无可列局部变量，node外层报Win32 error87。
- `dt`找不到所需的`alloc::string::String`和`core::fmt::Formatter`类型。
- LLVM按同一PDB查询外层地址返回FileName空、Line=0。
- 所以本报告的String/Formatter布局来自已核验机器码和原始内存，**不是本轮成功
  读取到的PDB类型变量**。不把类型命令失败当成对象不存在。

初次`.frame 10/11`按十六进制选帧，后续用`0n10..0n15`明确十进制；`.frame /r`
输出是unwind恢复视图，易失寄存器可能仍沿用异常上下文。另对`ln`使用显式地址，
避免把`@rip`残留异常上下文误读为外层位置。`ub`从指令中间回溯失败的原记录保留；
改从PE函数起点顺序解码，未修改代码。命令文件附在本目录，路径用<SUPERVM>代换。

## 5. 第一个不一致边界：当前能证明到哪里

机器码在pad_integral的RVA`0x1e52fae`读取Formatter.buf；
`0x1e52fdb`把RDI传给RCX，`0x1e52fde`读取文本指针，`0x1e52fe2`设置长度，
`0x1e52ff5`尾跳writer。writer入口把RCX保存在RDI，随后用它构造memcpy目标。

**静态传递关系明确；该次真实执行在哪一处第一次变坏仍未知。**
现在能直接确认的是“最终Formatter.buf与最终writer使用地址不一致”及memcpy非法源。
不能据此断言错误已经存在于writer入口：writer内部或memcpy执行期间的寄存器破坏、
此前参数错误、保存状态被其他代码改变等，需要时间顺序证据区分。
也不能把最终合法String推断为pad读取时的最后一个已知正确值。

因此未提交修复；没有找到可证明违反ABI的第一条指令，也没有排除生命周期、并发
或机器环境因素。875/3228的相似性保留为关联线索，不等同共同根因。

## 6. 下一项最小验证的对象（本轮未执行）

不要再索取同一原PDB或重复已通过的独立样例。真正缺的是**失败执行上的边界时间序列**：
在同一任务/线程关联下比较pad读取buf前、prefix返回后、尾跳前、writer入口及memcpy
调用前的receiver/text/len和对应保存寄存器槽，区分进入writer前已错与进入后被破坏。
若后续批准受控验证，应只围绕此路径收集有界、私有的地址/寄存器证据，不记录身份
或消息内容；为新诊断构建保留node本身完整调试流，不能把新PDB用于解释本次旧DMP。

本轮没有启动节点、没有访问原库/密钥、没有重跑样例、没有TTD/PageHeap、新制品、
观察器或双中继。原始证据保留；不能把符号接收成功升级为C健康或修复通过。
