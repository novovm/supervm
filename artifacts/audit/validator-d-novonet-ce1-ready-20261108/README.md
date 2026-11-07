# READY_NOVONET_CE1_D

2026-11-08，validator-D 按 main `698b1916` 的
`docs/NOVORUDP_SHARED_CLIENT_HANDOFF.md` 第 11 节完成独立候选接收。

- 输入 ZIP：`E:\区块链四机通讯\novonet-ce1f54ea-windows-x86_64-r2.zip`。
  150,354,733 字节，SHA256 与 A 独立 pin 一致：
  `79cdfcf8636ddb881b2c9fa56d28c990ef5b12c8c7010f52cff0fea3a2576c9b`。
- 程序源码：`ce1f54ea545e69dc64763f1f24f6048d90e4ef12`。
  独立 manifest pin：`6f9f222f738ca990636572ffcad6256b1cd294b082254912bfbd604f7081978f`。
- 解压到新的本机 incoming 目录，无覆盖；归档路径/重复/链接/未列文件检查通过。
  解压前 77 项文件哈希与长度、根 checksum 77 项及 SDK checksum 26 项通过。
- 包内接收脚本 SHA256：`c4ecbf44ad620f7a9caa605b4770feaabdf9dbdb272243660adf5fc2a237a481`，
  与交接 pin 和审阅的 Git 脚本一致；未修改脚本或候选程序。
- 使用本机 Python 3.12 运行包内脚本，参数 `--host-id D`，新结果目录 `ready-ce1-D`。
  原脚本退出 0，复核 77 项、411,022,253 字节；实际 node feature 含 `iroh-transport`。
- 实际加载新解压包内 AOEM core，SHA256：
  `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
  scalar/unified/mixed-batch 验签 KAT 与 sign 接口形状检查通过。
- 实际 node 离线合同为 `windows-scm-existing-v1`、`native_execution_tick`、`existing`；
  未启动服务、打开数据库或访问 custody。两份程序 stderr 均为空。
- 当前物理网卡 LAN IPv4：**192.168.71.59/24**，接口 16，1 Gbps。
  只读取本轮地址，没有重新清点硬件、校时或更改 VPN/防火墙。

[report.json](report.json) 为原接收报告；`runtime.stdout/stderr` 和
`node-contract.stdout/stderr` 保留原始字节，由本目录 `.gitattributes` 禁止行尾转换。
[public-receipt.json](public-receipt.json) 包含原始文件长度/哈希和 LAN 地址，
[archive-verification.json](archive-verification.json) 为独立归档核验记录。
原始时间戳保留 UTC（2026-11-07 16:17，即北京时间 2026-11-08 00:17），未改写。

本次不重装、不换钥、不移动用途材料、不注册服务、不启节点、不创建链库。
没有操作 C、B、旧服务或手机；C 必须由其本机执行和回报，本回执不代替 READY_C。
没有运行认证探测、单/双中继、恢复或资产业务；READY 仅为包接收离线资格，
不代表实网交付、完整 CI、主网安全、匿名性或 TPS 已通过。
回传 GitHub 供 A 汇总，不等于 A 已阅读或签收。
