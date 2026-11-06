# validator-D：selfrun r2 安装与服务账户用途钥回执

2026-11-06，按用户要求核验本机收到的 ZIP，并执行包内原安装脚本一次。
**结果：安装脚本和 LocalService 临时任务均退出 0；未安装节点服务、未启链。**

## 输入与独立核验

- 输入：`E:\区块链四机通讯\selfrun-9a6153f1-windows-x86_64-r2.zip`。
- 长度：140,196,230 字节；SHA256：
  `57bf8bc6fab08d5b7a85f4d15b2df30bbedcc1fa69d7ca361d3d3f99d63847eb`。
- 本机审阅 main：`f791cfed85f6d49c3e73b7344db975a3c37618f0`。
  ZIP 与该提交的 [C 独立回执](../validator-c-selfrun-install-20261106/README.md) 一致。
  A 原交付页只公布 r1 外层 pin，不将它误写成 r2 的 A 独立 pin。
- 程序源码：`9a6153f1eebda4da5c10829abf77575aa106ba8f`，目标 Windows x86_64。
- release-manifest SHA256 与 [A 原交付记录](../validator-b-program-install-20261102/README.md)
  独立 pin 一致：`58c92085ca738c1e3e280960df9f16d5432ff38fabcf08fa1dca170b905eff5d`。
- 解压前及独立新目录解压后，全部 77 项 artifact 长度/哈希均通过；
  根 checksum 77 项、AOEM Windows checksum 26 项通过，10 个实际构建合同。
  ZIP 的 101 个条目无重复、路径逃逸、符号链接或未列入允许范围的文件。
- r2 原脚本 SHA256：`3edee735b7c208b93a9a65b3a12da0744f8b613af50fa1e506ea5478d928deec`。
  已完整阅读；相对 Git 中 r1 仅修正 `Get-Item -Force` 及文件/目录 ACL 设置。
  本机未修改原脚本、程序或清单。

## 实际执行

Windows UAC 提升后，以 `-HostId D` 调用原脚本；外围 wrapper 只捕获退出码、
原始 stdout/stderr 和只读事后元数据，不代替安装器或钥生成者。

- 北京时间 14:36:01.942–14:36:16.213，安装退出码 0，stdout/stderr 均 0 字节。
- 安装目录：`C:\ProgramData\NOVOVM\releases\selfrun-9a6153f1-windows-x86_64`。
- 新用途目录：`C:\ProgramData\NOVOVM\selfrun\91129655606029\D`。
- 原脚本再次验证源及安装目录各 77 项文件；实际用途命令在
  `NT AUTHORITY\LOCAL SERVICE`（`S-1-5-19`）下执行，任务退出码 0。
- 实际加载安装目录的 `aoem\windows\core\bin\aoem_ffi.dll`，SHA256：
  `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
  标量、统一语义、混合批次验签 KAT 均通过。
- validator/transport 分别生成并 inspect，完整公开身份逐项相同；
  两用途公钥不同，均为 ML-DSA-65、1952 字节，也不同于 C 的对应钥。

| 用途 | 域 | 公钥 SHA256 |
| --- | --- | --- |
| validator | 91129655606029 | `cac6c513f90eb836466ed6a6a175c2c091736192634c536099dd10dfb96cbb96` |
| transport | 91129655606029 | `01663acfa5f718969673922846042817fa359eece95d9fdfc837ebd8014099fb` |

完整公开公钥 hex、运行库结果、ACL 元数据及逐文件原始证据哈希见
[public-result.json](public-result.json)。用途目录受保护，仅 SYSTEM、Administrators、
LocalService 具有权限；两个受保护文件继承该目录 ACL。没有读取或提交私钥内容。

## 保留与边界

临时任务 `NOVOVM-D-KeyPreparation-91129655606029` 已注销，事后查询不存在；
`NOVOVM-D` 节点服务不存在。旧域 167999907280566 的 current-user 传输材料保留，
仅检查其长度和修改时间未变，不把元数据核对说成文件内容的密码学核对。
本机既有 `NOVOVMExecution` 服务运行状态/PID 前后均为 Running/51076；未控制该服务。

未重编译、重跑旧四机认证、安装/启动节点、创建链库或创世、初始化余额、操作资产。
KAT 自身不测试签名/建钥；后继原用途命令另有真实生成/解封回执，但未发生共识签票。
本结果不等于 SCM 节点生命周期、新域四机联网、完整 CI、主网安全或性能通过。
等 A 汇总公开用途材料与共同配置，不自行启链。

## 公开证据

[original-public-reports.zip](original-public-reports.zip) 共 13 项：8 份原始执行/用途 JSON、
安装 stdout/stderr、独立包验证报告、原 r2 安装脚本和只捕获结果的 wrapper。
保留原始编码和字节，31,398 字节，SHA256：
`5a9e7659b3ff6022182fa83dd280faac2933b8364a55d3a0f2d6521182e634ef`。
仅从明确白名单归档，未含 `.protected`、sealed、credential、数据库或 journal。
公开回执推送不等于 A 已阅读或签收，也不改变已有 CI 的真实状态。
