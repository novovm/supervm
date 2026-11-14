# C：原 a254 同次调用 RBP 前后记录已取得，原始错位未复现

按用户要求，修正 CDB 命令后只执行了一次原 a254＋AOEM966 隔离取证。
断点和嵌套回调实际工作，取得 8 组、48 条寄存器事件；所观察的 LEA 写入、内部子调用
返回及 cookie 检查前帧关系全部符合机器码。**没有捕获首次错误写入，没有根因修复。**

## 执行与命令修正

- 北京时间 2026-11-15 03:31:15 至 03:32:18；外部计时 63.11 秒。
- 原 EXE SHA256：`1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d`。
- AOEM966 SHA256：`5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52`。
  CDB 模块加载记录确实包含此 AOEM966 路径。
- ntdll SHA256：`a0f81bf5c610c0ff0a550adc020a58446f214790c748057bb575042e972e6a34`。
- 四个原程序使用既有独立测试封装钥、新建隔离库和 loopback 地址，没有重新编译原程序。
  新库使用 create；没有读取、复制或打开 C 原库及原身份，也没有启动原服务。
- 上限：各节点 60 秒、外部 110 秒、8 个选定活动调用、1000 次硬件断点命中。
  取得 8 组后清除断点，未追加采集、未再次启动。

上一轮仅改条件和路径还不充分。本轮将嵌套回调从 `$><` 改成 `$$><path;`，使命令文件
作为命令块执行并允许后接命令；保留 MASM 条件的 `&`/`|`。相关区别见
[微软命令文件说明](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/-----------------------a---run-script-file-)。
另将十六进制环境下的命中上限写成 `0n1000`，增加外层返回的线程/RSP 过滤及未完成子调用检查，
并只选择旧容量非零的真实 realloc 路径。

启动前只读加载既有完整转储，验证相同的嵌套 include、条件块和指令字节检查：
`NESTED_INCLUDE_OK`、`NESTED_RETURN_OK`、`BYTE_GUARD_OK` 均出现，CDB 退出 0。
这不是人工违约样例，也不是另一次节点运行。随后实际运行没有出现命令加载、表达式或断点错误。

## 本次真实寄存器证据

入口为原节点 RVA `0x84eed4` 的 `RawVecInner::finish_grow` 调用点。
每组通过递增 group、线程 ID、父栈 RSP、堆返回地址及 child sequence 关联；
本次八组线程均为 `0x6e54`。堆返回地址均为 `ntdll+0x5bddd`。
公开原始事件见 [register-events-public.txt](register-events-public.txt)，离线逐组比较见
[analysis-public.json](analysis-public.json)。
对照原日志另行人工复核：外层调用至 LEA 前、LEA 后至子调用前、子调用后至检查前的
RBP 均分别一致。实际反汇编行见 [instruction-lines-public.txt](instruction-lines-public.txt)。

| 位置 | 原始 RVA | 本次核验 |
| --- | --- | --- |
| `lea rbp,[rsp-0x50]` 前 | ntdll+0x59dfc | 字节 `48 8d 6c 24 b0` |
| LEA 后、下一条指令前 | ntdll+0x59e01 | RSP 不变，RBP = 前一记录 RSP − 0x50 |
| 内部子调用前 | ntdll+0x5a05b | RSP = LEA 前 RSP − 0x150 |
| 内部子调用返回 | ntdll+0x5a060 | 同组 RSP、RBP 与调用前相同 |
| cookie 读取前 | ntdll+0x5a108 | RBP = RSP + 0x100，子调用已完成 |

例如第 8 组，LEA 前 `RSP=0x4db2ee510`、`RBP=0x4db2ee670`；
LEA 后 `RBP=0x4db2ee4c0`，这正是合法建帧结果。
子调用前后及检查前 `RSP=0x4db2ee3c0`、`RBP=0x4db2ee4c0`，没有历史现场的 `+0x2A0` 错位。
**合法 LEA 是已观察到的 RBP 写指令，不是原始故障指令。**

没有实际命中 AV、fast-fail、参数不一致或已设置的 context-gap 停止条件。
日志中的初始 `80000003` 是脚本安装前的 CDB 启动断点，不是原始节点崩溃。
最后标记为 `CAPTURE_GROUP_LIMIT`，没有生成 `captured-private.dmp`。

## 退出与证据边界

| 记录对象 | PID | 实际退出码 | 守卫强制停止 |
| --- | --- | --- | --- |
| CDB（调试隔离节点 0） | 32096 | 0 | 否 |
| 隔离节点 1 | 4824 | 0 | 否 |
| 隔离节点 2 | 31132 | 0 | 否 |
| 隔离节点 3 | 42752 | 0 | 否 |

节点 0 的独立 OS 进程退出码没有另外保留，**不能把 CDB 的 0 写成四个节点都退出 0**。
结束后只读进程检查未发现遗留 `novovm-node` 或 `cdb`。
执行器原始 [execution-public.json](execution-public.json) 保留 `original_fault_reproduced=not_determined`；
离线检查根据本次所保留记录得出“未观察到原始故障”，不改写原始执行结果。

本次证据只覆盖八个选定调用中的上述位置，不是全部内部 RBP 写入指令历史。
选定子调用前后正常，不能证明其中每条指令都始终保持同一 RBP；合法临时使用允许恢复。
已知 unwind/restore/continue 设有保守停止，未知非局部恢复仍没有完整覆盖。
本次是新 loopback 状态，未重放历史逐包输入，也未覆盖原故障数小时的运行条件。

没有逐次暂停时长和无调试器对照，因此 **63.11 秒总耗时不代表采集低干扰合格**。
硬件执行断点仍会产生调试异常。此次只证明回调与选定调用的实测记录可用，不能证明原运行时序被保留。
当前不足以给出首次异常指令或最小生产修复；不替换 String、IndexMap、memcpy 或 AOEM。

## 保留与共享

本机私有现场：`target/c-original-rbp-instruction-20261115-run2`。
原始 CDB 日志 SHA256：`595b39834b365e57972a1a6216fc5df3453cdb4c1559397f120a55609599d787`。
原日志、测试配置、封装钥、新库留本地；Git 仅含公开结果、寄存器事件和实际命令文件。
`executed-commands/` 为本次已执行命令的归档，不是允许自动再运行的入口；公开文本统一 LF，
原始私有文件不修改。上一轮 run1 失败及全部历史转储保留。

原库、身份、失败现场及生产代码未改，双中继继续暂停。C 独立推进，Git 用于共享结果，不等待 A 签收。
