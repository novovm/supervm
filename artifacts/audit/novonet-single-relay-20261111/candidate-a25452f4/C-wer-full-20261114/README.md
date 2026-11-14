# C：LocalService WER Full Dump 资格通过，目标专属规则已配置

基线 `9c3dc5cc`，2026-11-14。本轮用户明确批准在 C 本机先验证独立进程，再配置
`novovm-node.exe` 专属 LocalDumps；覆盖上一轮“仅准备独立系统方案、未配置”的阶段限制。
**未启动/重启原节点，未访问原库和身份，未修改生产代码，双中继不恢复。**

## 实际结果

管理员执行开始 `2026-11-14T07:47:16.5248388Z`，结束 `2026-11-14T07:47:26.6973494Z`。
完整性检查程序退出 **0**，管理员入口退出 **0**；测试任务和测试专属注册表项已删除。
完整转储留在 LocalService 受保护目录，未复制到 Git 或公共云。

| 项目 | 实测 |
|---|---|
| 测试 EXE | novovm-wer-probe-C-20261114.exe，独立于节点 |
| 运行 SID | S-1-5-19，LocalService |
| 测试 PID | 25616 |
| 测试异常 | 未处理的 RaiseException(c0000005)，独立测试有意触发 |
| 测试进程退出 | -1073741819 / 0xc0000005；不是节点失败 |
| 任务 LastTaskResult | 3221225477，同一异常状态 |
| DMP 大小 | 11,074,978 字节 |
| DMP SHA256 | 4b35be13143efdd291d831315ab7ebe49dbb4b5a9f28ef9c4b818fe585056e0a |
| DMP flags | 0x221826，包含 FullMemory |
| Memory64 | 57 个实际内存范围，逐项文件边界检查通过 |
| 线程上下文 | 列表共 2 线程，主线程和等待线程均存在，context 数据边界通过 |
| 堆 | 65,536 字节已知测试模式逐字节一致 |
| 代码 | 测试故障函数 32 字节与故障前记录一致，所在 4 KiB 页完整可读 |
| 文件稳定性 | 大小/修改时间稳定，两次 SHA256 一致 |

这是 **WER 机制资格**，不是 NOVOVM 故障复现。使用 MSVC 编译的独立小程序，不载入
NOVOVM/AOEM、不安装自有异常过滤器、不调用 MiniDumpWriteDump、不注入 TTD、不启用 PageHeap。
调用 RaiseException 生成资格测试异常，不宣称捕获了真实错误指针写入。
原始编译首次调用因命令路径格式失败，未运行程序；改为独立构建目录后编译成功，
只有一个未使用参数警告。独立测试只执行一次。

## WER 适用性核对

- 固定 a254 源码与 AOEM966 源码搜索未发现自建 Windows 崩溃报告器。
  唯一源码命中为 wasm 测试/Android panic hook，与此次 Windows 原程序不同。
- 原 a254 EXE 的精确 PDB 指向 CRT 安装函数 RVA `0x23ac688`、过滤器 `0x23ac698`。
  实际机器码检查：对非 C++ 异常（如 c0000005）跳到 `xor eax,eax` 返回
  `EXCEPTION_CONTINUE_SEARCH`；不是把访问违规吞掉或自己生成转储。
- EXE 导入 `AddVectoredExceptionHandler`。本机匹配版本 Rust 源码中的标准栈溢出 handler
  仅对栈溢出打印信息，返回继续搜索。源码说明不能代替所有运行时动态注册的证明。
- 既有 LocalService WER 原件说明历史异常进入过 Windows 报告路径；不存在已知适用性阻断项。
  不据此承诺所有退出方式（强制终止、机器掉电、自定义处理的新版本等）都会生成 DMP。
- AeDebug 64/32 位原有 VS JIT Debugger 注册保留，`Auto` 均不存在；target IFEO 不存在。
  资格测试实证本轮没有被自动调试器截走。全局 LocalDumps 值为空，前后保持。

静态来源及机器码见 [applicability-public.json](applicability-public.json)；
实际设置前后见 [registry-before.json](registry-before.json)、[registry-after.json](registry-after.json)。
Microsoft 对自定义报告器、自动调试、服务目录及专属设置的限制见
[WER LocalDumps](https://learn.microsoft.com/en-us/windows/win32/wer/collecting-user-mode-dumps)。

## 已生效的目标配置及数据边界

仅新建：
`HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\novovm-node.exe`

- `DumpType` DWORD = **2**。
- `DumpCount` DWORD = **2**，达到上限会替换最旧文件；故障后应及时本地保全，不当作永久归档。
- `DumpFolder` EXPAND_SZ =
  `C:\Windows\ServiceProfiles\LocalService\AppData\Local\CrashDumps\NOVOVM-Full-C-20261114T074716Z`。

目标新目录与资格目录继承同一个服务 Profile 的 ACL，允许 SYSTEM、Administrators、LocalService；
没有修改 ACL 或取得原目录所有权。新目录 owner 为 Administrators。
公开 SDDL 仅删去无关的本机用户组 SID，原始值保留在本机证据目录。
规则按 **EXE 文件名**匹配，同名诊断副本也会适用，不是按安装路径区分。
配置读回验证通过，原节点无需启动即可完成写入及配置核对。

**目标下一次自然崩溃是否实际成功生成完整现场，仍待真实发生后验收。**
当前没有运行中的原节点，因此本轮不启动等待、不重跑负载，也不承诺当前正在捕获。
后续启动属于另一项明确任务；本配置会保留。

完整 DMP 可能含私钥和网络数据。原件只留上述受控目录，Git 只含此测试的结构结果、
哈希及审核后的配置摘要，没有堆内容、原始 DMP/WER 或身份材料。
配置不会新增向云端上传完整转储的动作；未改变系统既有的其他 WER 策略。

## 回滚与后续验收

原目标专属 key 不存在。已准备 [Rollback-TargetConfig.ps1](Rollback-TargetConfig.ps1)，**未执行**。
它只在当前三个值/类型与本次记录完全一致且无子项时删除本次专属 key；发现他人修改则停止。
不删除转储文件/目录，不改全局 WER、AeDebug 或服务权限。

下一次自然异常后，核对同次 PID/创建时间、EXE/DLL/PDB、退出码、WER/事件与 DMP；
重新验证 FullMemory、Memory64、所有列出线程 context、实际目标堆页及代码页。
不能用本次小程序的成功替代目标现场覆盖验收，也不能把完整 DMP 当作此前写入历史。
本次关闭的是完整转储机制验收与配置任务；首次内存破坏、根因及修复仍未解决。

本机私有执行记录：`D:\WEB3_AI\SUPERVM\target\c-wer-qualification-20261114T074651Z`。
测试原件：LocalService CrashDumps 下 `WER-qualification-C-20261114T074716Z\dumps`。
C 独立执行，Git 同步不等待 A 批准，未手工触发完整 CI。
