# validator-C：selfrun r2 安装与服务账户用途钥准备回执

2026-11-06，按用户提供的 Windows r2 ZIP 执行包内一次安装准备脚本。
**结果：成功，原脚本退出码 0；只安装独立候选目录、准备服务用途钥，没有安装节点服务或启链。**

## 输入验真

- 输入：`E:\区块链四机通讯\selfrun-9a6153f1-windows-x86_64-r2.zip`。
- 实测长度：140,196,230 字节；SHA256：
  `57bf8bc6fab08d5b7a85f4d15b2df30bbedcc1fa69d7ca361d3d3f99d63847eb`。
- Git 审阅基线：`a0d2b4fd`；程序源码：`9a6153f1eebda4da5c10829abf77575aa106ba8f`。
- 独立清单 pin 来自 [A 的交付记录](../validator-b-program-install-20261102/README.md)：
  `58c92085ca738c1e3e280960df9f16d5432ff38fabcf08fa1dca170b905eff5d`。
  解压前逐项验证、解压后复核全部 77 项文件；原脚本再次核对源目录及安装目录各 77 项。
- 根 `CHECKSUMS.sha256` 的 77 项、AOEM Windows 子清单的 26 项均通过。
- Git 中公布的是 r1 外层 ZIP pin，不能拿来声称 r2 外层 ZIP 独立 pin 匹配。
  本次 r2 外层哈希是 C 实测值；其程序清单独立 pin 与 Git 一致。
- r2 脚本 SHA256：`3edee735b7c208b93a9a65b3a12da0744f8b613af50fa1e506ea5478d928deec`。
  与 Git 脚本逐行比较，仅增加 `Get-Item -Force` 和修正文件/目录 ACL 的不同继承设置；
  未更换程序、清单 pin、钥生成者、账户、域或启动范围。本机未修改包内脚本。

## 实际执行与结果

用户完成 Windows UAC 后，调用原脚本，参数为 `-HostId C` 与新解压的 `-PackageRoot`。
外围 wrapper 只保存标准输出、错误输出和退出码，不承担部署或钥操作。

- 执行时间：2026-11-06 14:25:45–14:26:02，北京时间。
- 原安装及 LocalService 临时任务退出码均为 **0**，安装错误输出为空。
- 安装目录：`C:\ProgramData\NOVOVM\releases\selfrun-9a6153f1-windows-x86_64`。
- 新用途材料目录：`C:\ProgramData\NOVOVM\selfrun\91129655606029\C`。
  由原脚本设置受限 ACL，仅系统、管理员、LocalService；不读取或提交受保护材料。
- 实际任务账户：`NT AUTHORITY\LOCAL SERVICE`，SID `S-1-5-19`。
- 原 `native-identity` 分别完成 validator、transport 的生成和 inspect；
  每个用途的生成/读回完整公开身份相同，两用途公钥不同，均为 ML-DSA-65、1952 字节。
- 临时任务 `NOVOVM-C-KeyPreparation-91129655606029` 成功后已注销，事后查询不存在。
  `NOVOVM-C` 节点服务不存在；本次脚本没有安装服务、启动节点、创建链库或激活创世。

| 用途 | 域 | 公钥 SHA256 |
| --- | --- | --- |
| validator | 91129655606029 | `50f87820242802fe4ee7270e26da5fab5c8ba519bc0661d6ca7f3fe3daac5757` |
| transport | 91129655606029 | `610885e9dc34eaa2c979a95e55bebde247d8f40e23f071f727a7fbf0b4c50e4a` |

完整公开公钥（hex）、实际加载路径和规范化结果见 [public-result.json](public-result.json)。
两把钥是本次新域服务用途材料，不替换既有 current-user 传输身份，不授予资产权限。
本机没有重编译、重跑旧四机认证或操作 NOVOCHAT/手机身份、历史与既有服务。

## 运行库与资格边界

实际加载安装目录中的 `aoem\windows\core\bin\aoem_ffi.dll`，SHA256：
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
LocalService 下标量、统一语义与混合批次验签 KAT 均通过。
`qualify-runtime` 自身没有测试签名/建钥；随后的原用途钥命令另有成功报告。
未发生共识签票或资产操作，不据此声称节点服务生命周期、四机新域联网、创世、
完整节点抗量子或生产验收已经完成。待 A 汇总公开用途公钥并交付共同配置；本机不自行启链。

## 原始公开证据

[original-public-reports.zip](original-public-reports.zip) 仅含 8 份原始公开 JSON、
安装 stdout/stderr、原 r2 安装脚本和仅记录退出码的调用 wrapper，共 12 项。
保留原始编码和字节；SHA256：
`d921582349d827be34bced0a278d63db8c06e8118a3d319dc797bd5076f4507d`。
各文件哈希在规范化公开结果中；未含 `.protected`、sealed、私钥、凭据或私有状态目录。
本回执经 Git 回传不代表 A 已签收，也不改变已有云端 CI 的真实结果。
