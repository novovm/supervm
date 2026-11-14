# C：LocalService 原始转储只读取证

基线 `b227089188e910a920848803dd3ed05e59c20190`，2026-11-14。
**管理员读取缺口已解决；没有找到完整原始转储，首次破坏指令仍未定位。**
本轮没有运行节点、负载、录制器或观察器，没有访问原库/身份、改变 ACL、修改生产代码或恢复双中继。
C 独立完成，Git 仅用于同步，不等待 A 签收。

## 1. 实际执行和原件保全

用户授权的 PowerShell 7 管理员进程执行一次 `Collect-ExistingDumps.ps1`，退出码 **0**。
UTC 开始 `2026-11-14T07:29:08.3914097Z`，结束 `2026-11-14T07:29:09.3908228Z`。
执行时脚本 SHA256：`2cf3f4246a739e8a1ac144d5810c181cd0318c5a49cce4df075965344cf71a0a`。

实际原目录：`C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps`。
Owner 为 `NT AUTHORITY\LOCAL SERVICE`；ACL 中 SYSTEM、Administrators、LocalService 有完全访问。
普通用户访问失败不代表文件不存在。此次沿用管理员权限读取，没有取得所有权或修改服务账户权限。
完整 SDDL、各目录枚举结果见 [inventory-public.json](inventory-public.json)。

范围限于上述目录的 `novovm-node*` 文件，以及 LocalService/System WER Archive/Queue
中名称匹配 `*novovm-node*` 的目录直接子文件，不跟随重解析点。
LocalService 两个 WER 目录不存在；System 两个 WER 目录已成功检查。
合计 **9 份 DMP、8 份 Report.wer**。没有声称扫描了所有磁盘或所有账户。

全部 17 个文件记录大小、创建/最后写入 UTC、SHA256；原件只读，私有副本哈希与原件一致，
读取前后原件大小及两个时间戳不变。副本的文件系统时间戳不作为原始时间依据。
私有副本保存在 `D:\WEB3_AI\SUPERVM\target\c-localservice-dumps-20261114T072900Z\files-private`。
**DMP 和原始 WER 内容不进入 Git。** 公开清单只含取证元数据，不含内存字节或身份材料。

## 2. 九份原始文件的范围

全部 DMP flags 为 `0x200121`，无 `MiniDumpWithFullMemory`，无 Memory64 数据流。
这里“MiniDump”指本批有限内存转储，不是仅凭 `.dmp` 扩展名或文件大小判断。

| PID | DMP 头部 UTC | 分类 | 异常模块及 RVA |
|---|---|---|---|
| 23024 | 2026-11-08 16:51:57 | 本轮额外取得的早期原件 | node + 0x11e6f9d，c0000005 |
| 24376 | 2026-11-10 04:25:30 | 本轮额外取得的早期原件 | node + 0xc1067d，c0000005 |
| 7816 | 2026-11-11 05:40:51 | 本轮额外取得的早期原件 | ntdll + 0x118095，c0000374 |
| 31296 | 2026-11-11 13:27:58 | 既有五份之一，SHA 相同 | node + 0x65f59d |
| 3228 | 2026-11-11 20:13:40 | 既有五份之一，SHA 相同 | VCRUNTIME140 + 0x1126b |
| 13020 | 2026-11-12 02:01:53 | 既有五份之一，SHA 相同 | node + 0x65f59d |
| 27928 | 2026-11-12 09:51:01 | 既有五份之一，SHA 相同 | VCRUNTIME140 + 0x1126b |
| 37796 | 2026-11-13 02:15:56 | 既有五份之一，SHA 相同 | VCRUNTIME140 + 0x11268 |
| 39992 | 2026-11-13 12:48:17 | 已知 TTD 录制器故障，独立分类 | TTDRecordCPU + 0xdeec8 |

三份早期原件不是本轮新发生的崩溃；各自 EXE PDB GUID 不同，不能直接套用 875/a254 符号。
其精确符号、源码归属未在本轮完成。它们没有提供完整堆，也不能补成 PID37796 的执行历史。
原始 WER 只做清单/哈希和私有保全，没有把未解析的 WER 内容当作根因结论。

每份转储均存在异常上下文和线程列表：普通八份各 20 个线程，TTD 一份 21 个线程。
已验证所有列出线程的 context 数据位于文件范围内，记录 context flags 和字节数；
这不等于验证所有 CPU 扩展状态齐全或调用栈均可可靠展开。
每份故障 IP 起始 16 字节有实际内存覆盖，不以磁盘 EXE 补读冒充转储代码页。
详细结果见 [coverage-public.json](coverage-public.json)。

## 3. PID37796：原件仍缺哪些页

原件 SHA256 `3acb7ec2450d6132334a1700764c3f50704f6cf4f98f2f9e08e6143c4c7c7ca0`
与既有副本相同，才复用该次失败的虚拟地址。以下范围均为左闭右开。

| 对象/代码 | 核验范围 | 实际覆盖 | 所在 4 KiB 页 |
|---|---|---|---|
| 局部 String | 0xe7e2ee500，24 B | 24 B | 0xe7e2ee000 完整 |
| Formatter header | 0xe7e2ee470，16 B | 16 B | 同上 |
| Writer receiver | 0xe7e2ee648，24 B | 24 B | 同上 |
| Writer 代码 | 0x7ff70b88cc60–0x7ff70b88ccc1，97 B | **0 B** | 0x7ff70b88c000–0x7ff70b88d000 全缺 |
| pad_integral 代码 | 0x7ff70b892f00–0x7ff70b892ff7，247 B | **0 B** | 0x7ff70b892000–0x7ff70b893000 全缺 |
| 目标 delivery record | 0x2044cc156e0–0x2044cc15798，184 B | **0 B** | 0x2044cc15000–0x2044cc16000 全缺 |

本轮没有取得目标堆页、原输入顺序或调用边界历史。故障指令附近可读与 Writer 函数代码缺失
同时成立。完整转储也不能单独证明首次破坏指令；不得拼接其他运行的 PageHeap/TTD 现场。

## 4. 仅对既有六个无静态 pdata 范围分类

没有扩大为所有汇编全审计。原映像哈希再次核对；RVA 均相对对应映像。
输出见 [unwind-classification-public.json](unwind-classification-public.json)。

- 875/a254 的 memcpy 间接跳转桩（0x23a9557/0x23ae7c7）不建立本地栈帧。
- 包内 AOEM/AOEM966 的 `aoem_free`（0x12ff60/0x130290）为返回或尾跳转，
  所查范围无栈分配或非易失寄存器改写。缺少本地 pdata 不构成缺陷，尾跳目标单独判断。
- 两份 AOEM 的源码关联 SysV Keccak 候选主体（0x6e7158–0x6e7b23、
  0x6ee998–0x6ef363）机器码哈希相同：有 `and rsp,-32`、`sub rsp,0x300`、
  `mov rsp,r11` 恢复，并改写 RSI、XMM6–15 别名。**它们不是 Windows 无栈叶函数。**
  SysV 内部约定允许这些寄存器作为易失状态，先前 Windows→SysV 调用者保存恢复检查仍成立；
  不能因此认定正常调用返回违反 Windows ABI。

若 Windows 需要从后两段主体内展开栈，需要可用的展开支持。静态 pdata 未覆盖；
九份转储 FunctionTableStream 的已解析描述符也没有覆盖 AOEM。
这只是**已保存信息中的回溯缺口**，并不证明运行期间没有动态注册。
解析按 [Microsoft FunctionTableStream](https://learn.microsoft.com/en-us/windows/win32/api/minidumpapiset/ns-minidumpapiset-minidump_function_table_stream)
的 descriptor/native table/entry/padding 结构进行，剩余尾字节只接受全零。
Windows 对叶函数、非叶函数及动态函数表的区别见
[x64 exception handling](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64)。

**没有证据证明这两段导致原崩溃，也未完成可提交的 unwind 修复判定。**
若后续验证此独立回溯问题，最小范围是隔离副本的函数表查询及受控栈展开，
不以普通格式化样例替代，不需要启动原节点。本轮未执行该实验。

## 5. 下一次受控取证条件

已准备 [独立 WER 完整转储方案](WER-ISOLATED-PLAN.md)，**未配置、未运行、未验收**。
不能以本次成功读取旧 MiniDump 宣称 WER 完整捕获已通过。
新现场必须先验证完整内存/线程/代码覆盖、精确构建、一次运行关联和保密边界。
原逐包输入和调度历史缺失，新的隔离负载只能是新的复现尝试，不称旧故障重放。

本轮得到新的原始文件清单并关闭访问权限缺口；没有找到首次参数错位/内存破坏，
没有生产修复。C 节点和双中继继续保持不恢复。
