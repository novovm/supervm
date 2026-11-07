# START_OBSERVED_082_C

2026-11-08，validator-C 同步 main `2185d5bb`，按原共享传输交接第14节执行一次
新包接线、原初始化及节点启动。使用包内原 `Start-NovonetFixedHost.ps1`，HostId为C。

## 输入核验

- ZIP来自 `E:\区块链四机通讯\novonet-08236129-windows-x86_64.zip`，150,438,312字节，
  SHA256 `38a871d5dc024ba90461a1c6da451ef2b76a39c3b39c905cf7034c9a920d582a`。
- 程序来源 `08236129320da2e71c5fa112d249fa7afc741e78`，manifest SHA256
  `a1a4082dcff2af227738e7e37013ae9a41b2546ecf25474eccda22166cb2f506`。
  独立归档流77项长度/哈希通过，包内共同批准输入pin匹配，脚本原字节与Git一致。
- 原脚本SHA256 `5a4ce2d71dcb62ef3f861aae585ce2097d633d2866a89cc404cf4bd517d0da23`。
- 新解压目录 `target/novonet-082-C-incoming-20261108`，本机LAN `192.168.71.125/24`匹配。

## 实际结果

本次可见Windows UAC完成后，单次调用原启动脚本，退出0。外围脚本仅采集输出、
观察任务及复制明确公开文件。原节点工作目录为
`C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-08236129-20261108`。
程序安装目录为 `C:\ProgramData\NOVOVM\releases\novonet-08236129-windows-x86_64`。

- 实际owner为LocalService，SID `S-1-5-19`。原validator/transport用途材料继续使用。
  共同批准输入中的C验证者公钥与原公开钥逐字节匹配，生成locator的传输公钥与原公开钥匹配。
- 原初始化 `ok=true`、`database_created=true`、`initial_state_persisted=true`，原owner已排空。
  初始根 `f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`。
  协议 `2132270f6d2f70bdf00ff09ddfb062fed56bd22e741e05a3cf7ba0d89ee7bfa5`。
  创世承诺 `f8396bae7c0150eef98ab2f1532057b706a5576b89c149c12561ef5d6b847ec6`。
- 实际AOEM为新安装目录中的core，SHA256
  `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
- 节点已生成locator，传输公钥SHA256为
  `610885e9dc34eaa2c979a95e55bebde247d8f40e23f071f727a7fbf0b4c50e4a`。
- 任务 `NOVOVM-C-08236129-Network-20261108` 在采集时Running，LastTaskResult为267009，
  表示仍在运行。原始观察UTC为11月7日19:38:33–19:38:53，即北京时间11月8日03:38。
- 首份progress为高度1、round0、待交付3、拒绝0、过期发送0、执行失败0、持久决定0。
  initialize及node错误输出均为空。该快照描述启动阶段，后续由A汇总四端实际推进结果。

## 公开回传与后继

[public-result.json](public-result.json)登记输入、实际初始化和观察字段，
[original-public-results.zip](original-public-results.zip)保留六份指定原始输出、观察报告、
启动stdout/stderr、归档核验及原调用/观察脚本，共12项，原始编码与字节完整保留。
逐文件与ZIP哈希均登记在公开结果中。受保护钥、数据库和journal留在本机原目录。

本机保持任务在线。后续恢复沿原库existing入口处理，当前初始化完成记录作为恢复边界。
A继续统一组织四机认证交付、单/双中继及恢复验收。Git推送供A取得本次回执，签收由A确认。
