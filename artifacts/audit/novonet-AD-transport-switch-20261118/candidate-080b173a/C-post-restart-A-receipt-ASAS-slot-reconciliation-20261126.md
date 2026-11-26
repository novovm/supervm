# C 复核单次重启后的 A 查单与 ASAS 槽位

A 回传的 `ccce0b72` 确认其原 Windows Client 和原查询身份在北京时间 02:59:09–10 认证成功并收到原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4` 的 `Finalized` Receipt，执行成功、高度 1、区块 `9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926`。目标是 A 原库的 loopback 隔离副本，使用修复 `f0507d67`，固定 A 的原节点公钥。`e9e12647` 是下一步 C 原库副本的复验请求。目前未见 A 在本次 C 重启后对 C 现役成功查单的回执。C 于 02:31:44–02:52:24 的现役抓包复算 SHA-256 匹配 `002be15a61b74bd12dc224b2d1fb2c00abecb46310041ba34e9acf2f2c66240e`，原 A 客户端 `41190/UDP` 在此窗口无包。

C 已取得并核对 ASAS 本机观测记录 `validator-F-authorization-observation.json`，SHA-256 `dd96ddc5b9303e5d6d2b22fb4ef5830c64c4ec09522b95fa62e2efdeb49f3cc5`。北京时间 **02:33:40.935** 保存了 Status、`UcaIdentity`、`UcaApplicationGrant` 三类响应，Status 链头和索引均为 2。ASAS authority 配置实际哈希 `8c3d9277e91b0fabb9df570fafdbd558c24acaab962147f1f35fb5a62c7ee7d2` 与当时的钉扎一致，目标为现役 C `192.168.71.73:41052`，固定 C 原节点公钥 SHA-256 `50f87820242802fe4ee7270e26da5fab5c8ba519bc0661d6ca7f3fe3daac5757`，使用 C 本机非验证者查询身份。三类实际响应证明该身份已经建立认证会话。C 于 02:32:24 重启完成，现役只有一个动态查询槽。ASAS 在 A 的本窗探针被观测到之前已经使用了该槽。原记录未保留首次握手的精确时刻和源端口，本次亦未发起查询以检查当前槽位。

C 原 PID 30328、B 原 PID 571511 在线。本次未再重启、未补发、未换钥。
