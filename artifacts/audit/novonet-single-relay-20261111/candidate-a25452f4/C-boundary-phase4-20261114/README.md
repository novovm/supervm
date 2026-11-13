# C Phase 4：完整调试构建与实际调用边界采集（2026-11-14）

**首次参数错位仍未定位；原故障未复现，无生产修复，C NOT READY，双中继继续暂停。**

基于 main `31b9651e` 和原源码 `a25452f4c49be37cd984e650f2079642e6c273b3`。
A 在 `0470f343` 签收过 `e1e3a999` 并提供原 PDB；本轮未查到 A 对 `31b9651e`
的新增签收。推送本报告也不等于 A 已签收。

## 实际完成及证据范围

| 项目 | 结果 | 限制 |
|---|---|---|
| 原 a254 优化诊断节点构建 | cargo 退出 0；EXE/PDB GUID、age 一致 | 新构建，仅生成，未执行或部署；不能用于原 DMP |
| 节点自身调试信息 | 33 模块有调试流，32 有源码文件记录 | 1 个生成对象有流但无文件；不是全依赖库都开启 debug=2 |
| 六个关键函数 | 新节点和隔离样例的归一化机器码、unwind codes 均与原 a254 相同 | 屏蔽地址重定位；不证明整个程序或全部调用目标相同 |
| 原 profile snapshot 隔离采集 | 64 次格式化、576 条记录；48 尾调用、16 零填充普通调用 | 一次测试数据快照；无原节点历史、并发网络或原库上下文 |
| 离线记录检查 | 1,638 个比较无失败；程序和 CDB 退出均 0 | 不是 1,638 次压力测试或原故障重现 |
| 首次破坏指令、根因、最小修复 | **未找到、未证实、暂不提交** | 不归因到 ABI、AOEM、CPU 或 Rust 标准库 |

[采集结果](capture-results-public.json)、[全部结构化记录](capture-records-public.json)、
[实际 CDB 输出](capture3-public.log)、[实际命令](capture3.cdb)、
[目标与调试器退出码](capture3-exit.json)。公开日志只来自隔离测试进程；工作区前缀
替换为 `<SUPERVM>`，未上传任何原节点转储、PDB、密钥、原库或未筛选原节点内存。

## 1. 构建与 PDB 核验

[构建命令](build2-command.json)、[实际编译参数](node-effective-flags-public.json)、
[制品 SHA256 / GUID / 模块流](diagnostic-node-public.json)、[源码校验](source-verification.json)。
原源码用 git archive 独立导出到 `target/c-boundary-phase4-20261114/source`，
未切换 main、未改生产源码。Rust 1.94.0 / LLVM 21.1.8，release opt-level=3、
overflow-checks=on、iroh-transport，node 包的库和二进制均 debug=2，最终 EXE 不 strip。

新节点 EXE SHA256 `4c354ca4e8d2d512d283b6934d583caa89455c14e250e0894c828f9f33dd473c`；
新 PDB GUID `634436A3-4966-41FA-9FF4-31FA3FC98D5C` age 1。
PDB 222,294,016 字节；profile.rs / traffic.rs 的源码 SHA256 实际出现在文件记录中。
它与原 a254 的 `E16D0B11-47CA-4EBF-B482-C91380CC57F8` 不同，**绝不强制加载到原转储**。
完整调试信息仍不能保证优化构建所有变量在每条指令处都可恢复。

[隔离构建制品身份](harness-build-public.json)、[依赖锁对齐](harness-lock-alignment.json)。
样例直接编译原 a254 profile.rs 和 traffic.rs，保留真实依赖类型，不替换成假接口。
除新增测试程序本身，锁中所有依赖版本来自原 a254 锁文件。它仅向内存观察记录加入
一条明确的测试事件并调用一次原 snapshot，不启动节点、不打开原库、不访问身份或网络。
新编译完整节点仅供后续诊断；本次实际执行的是独立的小型 snapshot 程序。

## 2. 静态对应与实际边界

复用上一轮 [875/a254 静态 ABI 与 unwind 证据](../C-call-boundary-phase3-20261113/README.md)，
没有再次运行已通过的六值样例或 FFI 压力样例。
新增 [诊断节点映射](novovm-node-function-map.json) 和
[隔离 EXE 映射](c-profile-boundary-function-map.json)：六组函数代码形状及 unwind codes
与原 a254 一致。比较掩去相对 call/jump 立即数及 RIP disp32；不能把它当作全程序二进制等同。
[实际 pad_integral 反汇编](harness-pad-disassembly.txt) 使用隔离 EXE 的真实地址。

每次记录关联线程、调用序号和 RSP；构建 SHA256 在结果及制品清单中固定。
采集点：

1. format_write 入口：String 对象地址及 cap / buffer / len；64 次均为空 String (0,1,0)。
   这是构造后进入格式化的首个观测点，不冒称捕获了所有对象最初创建指令。
   三字布局解释只限本次已核对的 Rust 构建，不作为跨版本稳定 ABI。
2. format_write +0x15d、pad_integral 入口：Formatter.buf 与同一 String 对应。
3. write_prefix 前后：RBX/RBP/RDI/RSI/R12–R15 一致；本次不新增 XMM 动态采样，
   其既有静态/隔离证据不扩展为故障执行时正确。
4. 48 次非填充分支：pad +0xe5（尾声前）、+0xf5（跳转前）、writer 入口，
   receiver/text/length 相同；RSP 恢复 0x98，实际 RAX 目标等于 writer 地址。
5. 16 次零填充分支：pad +0x424 普通间接调用、writer 入口、+0x427 返回。
   参数相同，入口 RSP 为调用前减 8，返回后恢复；该路径**不是尾调用**。
6. writer +0x2e：实际 memcpy 参数，目标等于 buffer+oldlen，文本与长度和上游一致，
   writer 帧减 0x48。零填充分支 oldlen=1 是此前写入字符 0 的正常结果。

尾调用仍须满足调用约定；它恢复当前帧并复用返回地址，不会自动“改变参数传递约定”。
参考 [Windows x64 序言/尾声](https://learn.microsoft.com/en-us/cpp/build/prolog-and-epilog?view=msvc-170)
及 [RtlVirtualUnwind 的 REX.W jmp register 尾声识别](https://learn.microsoft.com/en-us/windows/win32/api/winnt/nf-winnt-rtlvirtualunwind)。
这次记录没有出现先正确、后错位的边界，不能将原 DMP 的错误寄存器归因于这些正常记录。

## 3. 准备失败与采集失败均保留

本轮不是所有步骤首次成功；[原始日志哈希清单](private-evidence-hashes.json)留存每次结果。

- 节点首次构建退出 0，但仅对最终 bin 加 rustc debug 标志，未覆盖 node 库。
  因此补充 package override 后重建；没有把第一次不完整 PDB 当作交付。
- 隔离样例首次构建退出 101：缺真实类型依赖。补齐后，新解析锁选了不同依赖版本；
  编译尚未执行样例时，核对命令精确停止本轮 Cargo 树，退出 1，保留旧锁与日志。
  再对齐原 a254 锁，最终 `--locked --offline` 构建退出 0。
  辅助 metadata 的全目标离线解析因缺 bumpalo 缓存失败；不等于 Windows 编译失败。
- [采集1](capture-public.log) CDB 退出 `2147942430 / 0x8007001e`：MASM 条件不接受 &&；
  两个相同标记函数被优化合并，后设断点覆盖前者；没有取得有效完整记录。
- [采集2](capture2-public.log) 同退出码：首个值 0 正常走零填充普通调用，采集器错误
  要求所有 writer 都满足尾调用的 RSP。接收对象本身一致；这不是节点内存损坏。
  日志随后还记录嵌套退出命令的解析错误，不能将 PHASE4_MISMATCH 当成原崩溃复现。
- 采集3复用同一 EXE（未改测试值），分别记录普通调用和尾调用，改用离线逐项核对。
  CDB 与目标退出 0；实际目标 PID38308、线程27568，2026-11-14 05:15:04.937 +08:00 退出。
  成功后未重复运行。只重新解析已有记录和核验公开文件。

## 4. 未解决项与下一步最小验证

仍缺少**发生原故障那次执行**的参数演变。原 MiniDump 无指令历史，正常隔离快照
也不能补出其历史；当前不能指定第一次写坏指令、共同根因或可验证的生产补丁。
[原 EXE/PDB/DMP 哈希复核](original-evidence-preserved.json)保持不变。
未修改 String、IndexMap、memcpy、AOEM、生产超时或账户规则；原库/身份未访问。

下一项建议（**未执行**）：在不启动部署节点、不使用原库的前提下，用已保留的完整
调试构建/源码信息审查原 RpcLifecycle::status → observe_tick 所有者调用与并发状态，
先确定一个能在隔离进程中保留该上下文的最小用例，再沿相同边界捕获异常。
不能只加循环反复运行这次正常 snapshot 并宣称原节点稳定。若需要运行完整节点或
真实原库，必须另有明确范围；本回执不授权重启或恢复双中继。

[复现资料与分析入口](REPRODUCE.md)仅供审阅；不是要求 A/D 再执行已通过样例。
