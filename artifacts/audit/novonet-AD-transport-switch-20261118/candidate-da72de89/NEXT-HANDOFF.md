# da72de89 四机收包与原交易恢复交接

固定源码 `da72de891c11e7113e35cc273b32839123035054`。四机候选已齐，收包核验与运行验收分开记录。本交接只收口收包和准备状态，实际切换须沿既有授权执行。

| 主机 | 已核实状态 |
| --- | --- |
| A | Windows ZIP 已在本机独立验哈希。 [回执](../../private-apfl-20261122/A-common-da72de89-received-20261124.json) |
| C | Windows ZIP 95 个成员、92 项发布清单、40 项 AOEM 校验记录通过。原库和原身份的 LocalService SCM 离线计划通过，`existing`、开机自启、失败重启延迟 5／15 秒。C 节点保持停止。 |
| D | Windows ZIP 已在本机独立验哈希，既有验证节点保持运行。 [回执](../../private-apfl-20261122/D-common-da72de89-received-20261124.json) |
| B | A 已在 WSL Ubuntu 24.04 用原打包入口构建并传输 Linux x86_64 整包，A/B 独立校验一致。B 的原验证者保持运行。 [回执](../../private-apfl-20261122/B-common-da72de89-linux-received-20261124.json) |

- Windows ZIP `novovm-common-da72de89-windows-x86_64.zip`，159,288,717 字节，SHA256 `49657f70273ee3070d8a439e172bb75d00e80dcf44e911e7d02b569dee337c72`。
- Linux 包 `novovm-common-da72de89-linux-x86_64.tar.gz`，389,066,859 字节，SHA256 `a725401b06416fedea7de71020d854230542361b49ec0a3cce0960134004dc52`。

## A 接续准备，B 保持在线

目标交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4`。已有认证回执显示 A、B 均为 `Received`，高度 1、第 580 轮、回执索引高度 0。已核验的原签名批保存在 A，文件 `independent-signed.nuapfl`，5,610 字节，SHA256 `df61bc10005fe204947723c1ac3c361dc2e36e61a07b8b877b758dad7e912bd7`。B 在已查路径未找到对应文件。原始签名批及密钥留在 A。 [A 回执](../candidate-eecb753b/A-original-transaction.json) · [B 回执](../candidate-eecb753b/B-original-transaction.json)

1. A 先用既有认证查询重新确认 A/B 的目标交易状态，核对原签名批文件哈希。用已收 Windows 包准备 A 的原库 `existing` 恢复配置，只引用本机原签名批绝对路径及固定 SHA256。准备阶段保留现有进程。若已发现 `Finalized`，改走同一区块的最终性核验。
2. 获批切换后，先只切 A，B 继续在线。A 启动后核对实际 Node/AOEM、原库原身份、恢复日志及该哈希的认证查询。A 未证明交易仍在或已最终确认时，保持 B 在线并停止推进。
3. A 的恢复通过后，按授权由 C 用原 LocalService／原库 `existing` 接续，验证 C 从在线 A 补拉同一哈希。仅在两个独立节点保有此哈希或已核验持久化最终回执之后，再考虑切 B。D 保持现有运行。
4. 取得包含同一哈希的签名提案后，沿同一区块核对 AOEM 执行、三名不同验证者的有效 QC 和持久化业务回执。当前阶段不补发交易、不初始化、不换钥、不启用正式收费。 [恢复与验收规程](../../../../docs/NOVOVM_NATIVE_OPERATOR_CLIENT.md)
