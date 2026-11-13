# C 原始 DMP 离线复核（2026-11-13）

**结论：定位了故障消费者及其坏参数，尚未定位首次破坏者；未修复，C NOT READY。**
本轮只用微软 Windows Debugger/CDB 10.0.26100.9457（与 WinDbg 共用调试引擎）
打开已有 DMP，执行用户指定命令及已有内存/反汇编读取。不开发录制工具、不启动
TTD、不重启节点、不访问原数据库、不修改生产代码、不恢复双中继。

20:48:17 的 PID39992 含 TTD 注入，只用于区分录制器故障；不作为本轮原始样本。
17:08 的完整转储来自 PageHeap/调试器运行，也不混入未插桩样本。

## 一、样本核验

| PID | 北京时间 | 原文件字节 | 类型 | 符号 |
|---|---|---:|---|---|
| 31296 | 11月11日21:27:58 | 6,208,032 | MiniDump，flags0x200121，非完整内存 | 原875 PDB精确匹配 |
| 3228 | 11月12日04:13:40 | 6,159,844 | 同上 | 原875 PDB精确匹配 |
| 37796 | 11月13日10:15:56 | 6,203,358 | 同上 | a254精确EXE可用，匹配PDB仍缺 |

三份模块清单均不含TTD和Application Verifier。路径、时间、大小、完整SHA256及
PDB GUID/age见[dump-inventory-public.json](dump-inventory-public.json)。原文件未覆盖。
875 EXE SHA256 `9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7`；
PDB SHA256 `b46bc1763c06c7370733763391d56e083b2e52cc8dcd370a377be2f7928dac38`，
GUID `C94A5E99-4CFF-4094-8B63-0663EF0F9C21`、age1。CDB实际载入private pdb symbols。
原EXE的PE CheckSum为0；保留CDB checksum警告，不将它误写成GUID不匹配。
37796所需PDB GUID为`E16D0B11-47CA-4EBF-B482-C91380CC57F8`、age1，未借用875符号。

## 二、四项完整输出

每列链接保留该命令的完整输出；所有栈帧/返回地址/内联帧保留，只脱敏本机路径和
未经类型解释的Args to Child栈字。寄存器经检查未包含可识别凭据，按原值保留。
完整原始日志、转储仅留本机；公开日志不包含数据库、密钥文件或任意堆内存。

| PID | !analyze -v | .exr -1 | .ecxr / r | kv |
|---|---|---|---|---|
| 3228 | [完整分析](3228/01-analyze-v.txt) | [异常参数](3228/02-exr.txt) | [寄存器](3228/03-context-registers.txt) | [完整调用栈](3228/04-kv.txt) |
| 31296 | [完整分析](31296/01-analyze-v.txt) | [异常参数](31296/02-exr.txt) | [寄存器](31296/03-context-registers.txt) | [完整调用栈](31296/04-kv.txt) |
| 37796 | [完整分析](37796/01-analyze-v.txt) | [异常参数](37796/02-exr.txt) | [寄存器](37796/03-context-registers.txt) | [完整调用栈](37796/04-kv.txt) |

各PID目录还保留完整调试器会话（含模块/反汇编）。三个最终CDB执行均exit0；这是
分析工具退出成功，不是节点通过。37796首次会话因ProgramData镜像读取权限未成功
映射EXE，仅展开少数帧；保留该私有输出。改用已核验同SHA256的本地原候选EXE后，
展开完整地址栈；未提权、未改文件权限、未重启节点。脱敏.cmd仅是命令记录，不可直接执行。

## 三、实际故障归类

### PID3228：字符串写入使用异常描述符，不能判定UAF

- 线程2960；`VCRUNTIME140.dll+0x1126b`，匹配微软符号为
  `memcpy_avx_ermsb_Intel+0x6b`，指令`mov word ptr [rax],cx`。
- ExceptionInformation[0]=1（写），[1]=`0xeab6ddc900`；RAX同值。
  R8=2，CX=0x3962；是两字节写入，不是0xc6，也不是简单的空指针加偏移。
- 栈含`Vec::append_elements / String::push_str / String::write_str`、
  `core::fmt`、字符串收集、profile路径、`RpcLifecycle::status`、
  `observe_tick`、native tick。优化PDB在profile帧显示`elapsed+0x50df`，
  不能把邻近符号偏移误当成精确源码行；原源码中的hash/snapshot格式化路径是审查落点。
- 实际write_str汇编从接收对象+8取缓冲地址、+16取长度，再相加传给memcpy。
  该现场对象地址0x755b6ee358；+8=`0x755b6ee470`，+16=`0x755b6ee490`，
  相加恰为`0xeab6ddc900`。长度为504045167760，已不像有效String描述符。
- 这是“描述符损坏或接收对象指针错误”的证据，不证明哪个写入造成、不证明
  对象已释放、不证明跨线程竞争，也不证明FFI ABI错误。`!address`未保存目标区域
  的State/Protect信息，不能仅凭其unknown状态声称该块已经被释放。

### PID31296：寄存器恢复地址不对齐，调用栈不完整

- 线程6228；`novovm-node.exe+0x65f59d`，匹配PDB符号
  `indexmap::inner::Core::insert_full+0x31d`。
- `movaps xmm6,[rbp+0x20]`；RBP=`0xe83f7ee168`，操作数地址=`0xe83f7ee188`，
  对16取余8；实际异常记录为读，参数[1]=`0xffffffffffffffff`，不能把它改写为
  推导的操作数地址。两项分别报告。
- 原函数序言与现场RBP/RSP关系不一致；故障处处于恢复XMM寄存器路径。
  不能只凭movaps就判断“调用入口栈没有对齐”。CDB只能恢复一帧，完整kv如实保留。
- 不足以证明具体ABI违约、栈越界写或硬件故障；也不能证明与3228同根。

### PID37796：新版仍有两字节非法读；地址栈可恢复，源行不齐

- 线程31876；`VCRUNTIME140.dll+0x11268`，`movzx ecx,word ptr [rdx]`。
- RDX=`0x8000000000000005`，是实际操作数；异常参数为read及全1地址。
  不混同为0xc6，不擅自修改原异常记录。
- 精确a254 EXE SHA256 `1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d`
  已用于unwind；匹配PDB缺失，保留地址栈，不将875函数名套在新版上。

## 四、源代码与FFI审查边界

只读核对原875源码及当前对应`native_pipeline/profile.rs`：hash接收按值
`Option<[u8;32]>`，逐字节format为String；snapshot读取的记录容器受Mutex/try_lock
保护。该直接路径没有extern C、裸指针或释放回调；三份当前故障栈都未证明一个
正在执行的AOEM FFI帧。以上不排除更早的FFI/unsafe代码破坏，也不是全量FFI审计。
不能因项目包含Rust/C/C++，就把尚未出现的跨语言调用认定成根因。

## 五、C与D比较：区分历史同包与当前未知

- 11月12日C/D更新回执：a254 EXE同为1758e4a7...；AOEM同为1b764a36...。
  可比较这个历史同包场景，不等于两机负载、内存和系统环境相同。
- 11月13日C的37796及后来录制现场已使用AOEM966，DLL为5743f587...。
  本轮可找到的D已加载DLL回执仍是旧1b764a36...；未实时读取D，不能宣称D目前
  仍是旧DLL，也不能称最新C/D运行库完全一致。
- C当前只读环境：i9-14900HX，Windows11 build26300，两条16GiB、配置5600、
  SMBIOSMemoryType34；BIOS N0CN22WW。保留原字段，不沿用未经核验的DDR4假设。
  VCRUNTIME140为14.50.35719.0，与原dump模块版本一致。当前rustc1.94.0
  (4a4ef493e)不自动证明其他机器的构建工具链。XMP、CPU微码状态未确认。
- D的Windows/BIOS/内存/工具链/依赖完整现值未取得；本轮未操作D。请A/D后续仅
  回传这些非敏感只读字段及实际加载库哈希即可，无需再启节点或重做通信。

详见[C环境](C-environment-public.json)、[历史C回执](../C-update-20261112/after-public.json)、
[历史D回执](../D-update-20261112/after-public.json)。未执行内存压力测试，不据此排除硬件。

## 六、最小下一步（提案，本轮不运行新实验）

先沿3228的已匹配PDB与现有栈，离线核对Formatter的数据指针/vtable与String接收
地址如何传递；同时补齐a254原始PDB（上述精确GUID/age），对37796的对应参数
路径复核。旧DMP没有写入历史、完整堆和分配/释放记录：若相关调用前局部数据缺失，
就明确到此为止，不能从坏结果反推唯一写入者。

后续如需动态实验，最小范围应围绕已定位String接收对象在创建、借用、写入前的
身份与长度不变量，复用已存在隔离状态快照用例；此前同类隔离用例未复现，不能只
重复运行就当根因实验。须先从本轮离线核对提出具体、可被反证的触发假设，再另行
决定捕获范围。不再开发另一套录制工具、不默认运行全节点两小时观察。

本轮没有猜测性生产修复。首次破坏者、UAF/越界/ABI具体归因、跨机环境对齐仍未完成。
