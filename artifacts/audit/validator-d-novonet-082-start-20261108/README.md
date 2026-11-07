# START_OBSERVED_082_D

2026-11-08，validator-D 按 main `fcb5eb37` 的
`docs/NOVORUDP_SHARED_CLIENT_HANDOFF.md` 第 14 节执行一次新包接线、原初始化和启动。
使用新 082 包，不使用 ce1 包；没有重编、换钥、清库或重复旧配对。

## 核验和实际启动

- 输入 `E:\区块链四机通讯\novonet-08236129-windows-x86_64.zip`，150,438,312 字节，
  SHA256 `38a871d5dc024ba90461a1c6da451ef2b76a39c3b39c905cf7034c9a920d582a`。
- 程序源码 `08236129320da2e71c5fa112d249fa7afc741e78`；manifest SHA256
  `a1a4082dcff2af227738e7e37013ae9a41b2546ecf25474eccda22166cb2f506`。
  归档流和新目录解压后各 77 项哈希/长度通过，共 411,023,277 字节；
  根 checksum 77 项、SDK checksum 26 项通过；未覆盖旧程序。
- 原启动脚本 SHA256 `5a4ce2d71dcb62ef3f861aae585ce2097d633d2866a89cc404cf4bd517d0da23`，
  与交接及 Git 审阅脚本一致；共同输入 pin 一致，D 验证者公钥匹配原 D 公开材料。
- 用户完成本次可见 UAC 后，以 HostId D 运行原脚本一次，退出 0。
  外围观察脚本仅作预检查、单次调用和有界结果采集，没有重试初始化或改变原脚本。
- 安装新独立版本 `C:\ProgramData\NOVOVM\releases\novonet-08236129-windows-x86_64`。
  原脚本使用已有 `C:\ProgramData\NOVOVM\selfrun\91129655606029\D` 下两用途材料，
  首次创建 `chain-state`，工作目录为其下 `novonet-08236129-20261108`。
- `owner.json` 实际 SID 为 `S-1-5-19`，账户 LocalService；未重新生成用途钥或访问资产钥。
  `initialize.stdout` 为 `ok=true`、`initial_state_persisted=true`，初始根
  `f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`，
  协议承诺 `2132270f6d2f70bdf00ff09ddfb062fed56bd22e741e05a3cf7ba0d89ee7bfa5`，
  创世承诺 `f8396bae7c0150eef98ab2f1532057b706a5576b89c149c12561ef5d6b847ec6`。
- 实际 AOEM core 为安装目录内 DLL，SHA256
  `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
- 原节点生成 locator；其完整传输公钥与原 D 服务用途公钥逐字节一致，公钥 SHA256
  `01663acfa5f718969673922846042817fa359eece95d9fdfc837ebd8014099fb`。

## 采集时状态，不扩大签收

北京时间 2026-11-08 03:30:38 开始；03:30:53 采集（原 UTC 时间不改写）。
任务 `NOVOVM-D-08236129-Network-20261108` 为 Running，LastTaskResult 为 **267009**，
表示正在运行，不是已完成退出码 0。保持任务在线，未停止或重启；原 24 小时运行上限保留。
这是原节点实网验收任务，不是 SCM 服务安装或生命周期验收。

初始 progress 快照：高度 1、ticks 1、执行批次/持久决定/执行失败/消息拒绝均 0，
待交付 3、已安装 bound peer routes 0；这是初始瞬时快照，不是后续状态或 DONE 证据。
初始化及节点 stderr 均为空。规则限定新 node、D 的 192.168.71.59、UDP 41053/42053、
LAN/B 来源，由原包脚本创建；没有改全局网络、旧服务、A/B/C 或手机。

没有重复旧配对、额外发送业务、清理任何库或 journal、降低法定票数。
本结果仅确认 D 首次原初始化和节点启动已被实际观察；四端通信、单/双中继、恢复、
对象 DONE、资产业务、完整 CI 和 TPS 都不据此记为通过。原库已经创建，禁止再跑本次
初始化入口；后续恢复必须按原 owner 用 existing 打开原库。

## 原始公开结果

[original-public-results.zip](original-public-results.zip) 为 13,633 字节，SHA256：
`f912fc14fa4ec47a51e4834743f7072679cfadc3857d734712b38897826fdd23`。
仅包含指定六份 owner/初始化/locator/progress/node.stderr 原字节、采集任务状态、
启动器 stdout/stderr 和归档验真报告，共 10 项。逐项长度及 SHA256 见
[public-result.json](public-result.json)，归档后已重新核对解压流字节。
未提交程序包、受保护钥、carrier.protected、credential、数据库或签票 journal。
Git 回传供 A 汇总，不代表 A 已签收，也不改变旧失败和其他设备的状态。
