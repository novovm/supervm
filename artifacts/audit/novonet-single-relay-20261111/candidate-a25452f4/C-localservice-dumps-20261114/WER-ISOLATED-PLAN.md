# 独立 WER 完整转储准备方案（未执行）

本文件仅为配置及验收方案，不是执行脚本或节点启动许可。本轮未写注册表、改变 ACL、
配置 WER、启动 VM/节点或重跑样例。尚未指定并验证可用的独立诊断 Windows 环境。

## 隔离和固定输入

1. 使用独立 Windows VM/诊断系统，不能在当前 C 系统为同名 `novovm-node.exe` 设置全局捕获后
   声称只影响隔离副本。WER 按可执行文件名配置，不按完整文件路径区分。
2. 使用原始 a254 EXE、AOEM966、精确 PDB 和依赖的只读复制件；固定 SHA256、PDB GUID/age、
   Windows build、运行账户、加载模块和完整启动参数。EXE/PDB 匹配不等于拥有完整源码调试信息。
   精确构建证据沿用先前 [原二进制审计](../C-original-static-audit-20261114/README.md)。
3. 只用新的诊断数据库和测试身份；不挂载原库、原密钥或历史私人业务数据。
   不改原 EXE/std，不加载 TTD、PageHeap 或逐调用钩子；新输入顺序、查询周期和并发条件单独记录。
   原故障输入缺失，不能宣称新环境等同原失败执行。

## 待应用的配置（只适用独立环境）

在独立系统保存原配置后，以管理员配置：

| 项目 | 候选值 |
|---|---|
| 注册表键 | HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\novovm-node.exe |
| DumpType，REG_DWORD | 2（完整用户态内存） |
| DumpCount，REG_DWORD | 2 |
| DumpFolder，REG_EXPAND_SZ | 独立系统中本次运行的全新私有绝对目录 |

新目录只授予实际运行账户、SYSTEM 和管理员必要权限；不修改当前 C 的 LocalService 配置或旧目录。
先检查独立系统剩余磁盘空间足以保存全进程内存和余量，不能按当前约 6 MB MiniDump 预留。
每次用新目录、最多一次候选运行、禁止自动重启；首次失败后保全文件并停止。
DumpCount 到限额会覆盖旧文件，不能把它当作永久归档保障。

完整转储设置及服务账户目录权限要求见
[Microsoft WER LocalDumps](https://learn.microsoft.com/en-us/windows/win32/wer/collecting-user-mode-dumps)。
配置前只读核对独立系统 AeDebug、IFEO、应用自有崩溃处理与 WER 策略；
自动调试及自有崩溃报告可能影响 LocalDumps。存在冲突时先记录，不假设配置即生效。
不通过改动当前 C 的调试器/服务配置解决独立环境问题。

## 采集资格与失败现场验收

- 先验证独立系统中的实际转储机制、运行账户可写及完整内存输出；资格验证不是 NOVOVM 根因复现。
  若已有适用资格证据应复用，不反复跑普通 Writer/快照样例。
- 候选一次运行需有明确停止时间、空间上限、退出码和日志。具体负载与上限在独立环境具备后固定；
  本文件不启动任何运行。时间/空间到限即停止并记未复现，不自动重试。
- 异常后保存同次 DMP、WER、事件 1000/1001（如产生）、开始结束 UTC、PID/创建时间、模块/PDB 标识
  及所有原始文件 SHA256。仅凭 PID 不做跨时间关联。
- 验证 `MiniDumpWithFullMemory`、Memory64 实际范围及文件边界；逐项检查异常 context、线程 context、
  目标对象堆页、Formatter/Writer/尾调用代码页。DumpType=2 不是覆盖验收替代品。
- 所有缺失或不可读页单列。匹配磁盘 EXE 只用于辅助反汇编，不能冒充失败时实际代码页。
- 原始 DMP/WER 与可能含身份材料的日志只留私有目录；Git 仅存审核后的公开摘要、哈希和缺口。

完整转储提供同次失败的内存和上下文，**不提供先前指令执行历史**。
即使成功获得完整 DMP，若仍不能找到首次变化边界，应如实记录这一限制，不猜测修复。
