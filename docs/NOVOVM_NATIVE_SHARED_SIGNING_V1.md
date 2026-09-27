# 原生交易共享签名消息规则 v1

`novovm-adapter-api::native_signing` 提供 TxIR 签名摘要、Ed25519 公钥对应的
20 字节地址，以及 20/32 字节发送者绑定规则。原 Adapter 公开签名函数名保留，
单笔和批量验证复用同一个绑定实现。不增加依赖，不修改 AOEM 内核。

这是既有编码的迁移，不是交易格式升级：历史函数名为 v1，实际签名域仍是
`novovm_adapter_tx_sig_v2`；字段顺序、长度前缀、大小端、可选字段标记、交易类型
与执行策略标签均保持不变。签名覆盖账户主体、费用/nonce 主体、访问列表及顺序、
跨链字段和传入的交易 hash；不覆盖 signature 字段本身。无需重签已有交易或迁移库。

边界：

- 共享模块不执行密码学验签，绑定匹配不能替代验签。
- 真实 Ed25519 校验仍由原有 Adapter/AOEM 路径负责，没有接受调用方布尔声明的捷径。
- 摘要包含传入的 hash，不代表已验证 hash 的规范构造，也不验证链域或执行主体授权。
- 本次没有构建 NOV 证明 guest，没有修改状态根、封印、canonical 或 finalized 标志。

测试冻结迁移前编码作为仅测试用对照，覆盖全部交易类型/执行策略组合、旧编码签名
兼容、各签名字段篡改、访问列表内容与顺序、20/32 字节发送者、有效签名但错误发送者、
损坏及长度错误的签名。真实验签测试只证明签名层行为，不代表该交易业务上允许执行。

本机 Windows 验证命令：

```text
cargo test -p novovm-adapter-api -p novovm-adapter-novovm --lib --locked
cargo test -p novovm-node --lib native_nonce_identity --locked -- --test-threads=1
cargo clippy -p novovm-adapter-api -p novovm-adapter-novovm -p novovm-node --lib --tests --locked -- -D warnings
```

下一步应在 NOV 证明程序边界接入真实签名验证及共享 nonce/执行规则，并对输入、
父状态和输出承诺做闭合验证。本次共享规则抽取不能被表述为完整交易有效性证明、
多机/公网验收或主网最终性完成。
