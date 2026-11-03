# 原节点用途身份准备（A–D，未启链）

2026-11-03 传输用途候选同步。2026-10-25 的 `12c25e4c` 接线记录与
`424be495` 的 Ed25519 测试签票准备规则保留为历史证据，不能继续指导当前传输钥准备。
当前候选正在执行本机验证；尚不代表 Windows/Linux 平台包、四机认证、重连或启链通过。
本轮只接线和隔离回归，不生成真实部署密钥、
不安装/启停服务、不分配 NOV、不激活创世。B 原信箱、Relay、Web、通信身份均不动。
旧链数据退役范围与回执仍以 [交付对齐](NOVOVM_DELIVERY_ALIGNMENT.md) 为准。

## 唯一产品路径与替换关系

原 `ResidentConfig.signing_key_file` 曾同时供原 journal 签票和 NovoRUDP 持钥认证。
现在配置必须显式提供 `signing_key_file`、`transport_key_file`、`key_storage`。
原 `GenesisValidator` 增加 `transport_public_key`，保留 `public_key` 为验证者签票公钥；
签票与传输公钥均固定 ML-DSA-65 的1952字节、签名3309字节，使用独立生成的用途钥；
各用途集合内不得重复，传输钥也不得与验证者签票钥共用，绑定完整、一一对应。
算法相同不合并持钥权限，不是添加第二个验证者集合。

原 [NativeChannel](../crates/novovm-node/src/native_pipeline/consensus/native.rs)
通过 [薄绑定表](../crates/novovm-node/src/native_pipeline/consensus/native/bindings.rs)
把握手已认证的 transport peer 映射为原 ValidatorSet 身份。
原 [service](../crates/novovm-node/src/native_pipeline/service.rs) 在开库/journal 前
先检查受保护用途、链域、公钥形状及传输/远端绑定，再以同一已加载且合格的 AOEM
核对本机两种用途的钥匙配对及初始化批准；不开第二份逐握手 runtime。
原 endpoint 仍负责握手、重连和密钥确认，journal 仍
用签票钥匙。提案来源必须匹配 proposer；可转发的投票/QC仍独立验证证据签名。
仅持传输钥匙不能产生验证者签票。未改变 3/4 门槛、费用、nonce 或发布权限。

映射摘要同时进入原普通创世承诺及
[UcaGenesisManifestV1 规范批准字节](../crates/novovm-node/src/native_pipeline/service/config/uca_genesis.rs)。
原 protocol pin 包含 `round-bft-v2-mldsa65`、`transport-binding-v3-mldsa65`、
`handshake-v2-mldsa65` 和固定供应；旧未发布配置拒绝，
不猜测、不默认同钥、不混跑旧包。网络端点是可核对的路由配置，不是身份凭证；
现有 `native_peers.public_key` 仍填写验证者签票公钥，由共同映射解析传输身份。

## 离线准备接口

复用原 novovmctl 和原 relay 平台 custody，**不调用信箱 owner 启动**。
`generate` 是显式 create-new 操作；`inspect` 是只读校验，不能把两者都称只读。

准备任何用途钥之前，可先用同一原验签 owner 做无钥运行时资格检查：

```text
novovmctl native-identity qualify-runtime --library <实际AOEM路径> --library-sha256 <批准哈希>
```

这实际执行原标量和统一语义混合批验签 KAT，不创建用途钥、custody、数据库或监听。
`qualification=ml-dsa-65-native-verification-owner` 只证明该平台库的验签运行时资格；
签名接口/尺寸检查不等于实际签名或 keygen 已测，更不是四机认证、启链或全链抗量子资格。
原 Linux 打包器在 staging 前执行此检查，旧库即使提供标量 ML-DSA 也不能冒充统一批验签库。

```text
novovmctl native-identity generate --chain-id <统一新测试域整数> --purpose validator --key-file <本机绝对路径/validator.sealed> --library <实际AOEM路径> --library-sha256 <批准哈希>
novovmctl native-identity generate --chain-id <同一整数> --purpose transport --key-file <本机绝对路径/transport.sealed> --library <同一AOEM路径> --library-sha256 <批准哈希>
novovmctl native-identity inspect --chain-id <同一整数> --purpose validator --key-file <同一路径/validator.sealed> --library <同一AOEM路径> --library-sha256 <批准哈希>
novovmctl native-identity inspect --chain-id <同一整数> --purpose transport --key-file <同一路径/transport.sealed> --library <同一AOEM路径> --library-sha256 <批准哈希>
```

尖括号均为待审核值，不可原样执行。先核对本机服务账户、私有目录和共同 chain_id；
不自行复制别机材料或沿用已退役旧域。已有文件拒绝覆盖，损坏/用途错误/链域错误
均拒绝，无明文回退。原保护层内增加有界用途信封，不另造密码或密钥管理服务。
公开输出含用途、chain_id、公钥、算法及未启链标志，不输出私钥/助记词。
用途信封明确升级为 `NVPK0003` / version3 / suite65；验证者与传输两种用途都必须经
同一原 AOEM keygen/配对签验。配对 challenge 绑定用途、链域和公钥，使用不同 context：
`NOVOVM/validator-purpose-key-pair/v3`、`NOVOVM/transport-purpose-key-pair/v3`。
缺少库资格、哈希不符、旧信封、旧32字节传输钥或用途不符均拒绝；不自动转换、
覆盖旧材料，也不回退 Ed25519。此前 `NVPK0002` 的传输准备命令已失效。
网络入站验签复用资产消费者同一统一语义 `OwnedMldsaVerifier`，从已加载
`MldsaRuntime::verification_owner_v1` 取得，不再走旧 scalar verify ABI。
签名与 keygen 仍使用既有共享 primitive owner；不声称新增了并不存在的统一语义签名操作。
这不是 UCA 管理或花费钥匙，也不宣称所有网络安全边界已抗量子。

原 NovoRUDP 握手使用明确 version2、ML-DSA-65 身份与原有界握手承载；
握手失败不建立可信 peer，重连仍须重新认证。会话内仍由原 AEAD、nonce、重放窗口
和补片机制负责逐包保护，不给每个数据包另加 ML-DSA 签名。
当前 ECDH 密钥协商资格与签名迁移分开记录，不因认证算法变更就宣称抗量子保密。

Windows：复用当前用户 DPAPI，必须在最终节点运行账户下准备/inspect；另行核对私有
目录 ACL。DPAPI 成功不证明部署 ACL 已验收，当前原文件帮助器不检查所有 ACL 策略。

Linux：复用原 XChaCha20-Poly1305 service custody 和 OS credential/文件所有权检查。
先准备节点专用 `CREDENTIALS_DIRECTORY/owner-key`（32字节受保护 wrapping credential），
不得借用信箱/Relay 的 credential。CLI 不替操作者生成或隐式配置该 credential。
原文件 owner 增 `open_existing` 薄适配：inspect 连锁文件缺失也失败，不补建锁或修复状态。
目标目录属最终 UID、0700；密钥/锁文件的原0600约束继续保留。
打包 unit 使用 `LoadCredentialEncrypted=owner-key:/etc/novovm/credentials/node-owner-key.cred`；
用原 systemd credential 准备流程在该节点上单独配置，不运行信箱首装脚本。
`lifecycle plan` 只核对路径/配置/制品/credential绑定，不解密用途钥匙，
因此 `custody_verified=false`；最终 service 账户和同一 credential 下两次 inspect 的
成功回执是独立前置项。模板接线不是 D 实机或 B 安装验收。

`raw_development` 已被原节点启动明确拒绝，真实进程回归也使用原受保护 custody。
严禁受保护材料加载失败后切 raw、自动重置签票日志或沿用清空状态的旧链域。

## A 汇总的共同审核包（目前仍有待定项）

| 材料 | 来源/状态 |
| --- | --- |
| 固定源码、各平台 node/ctl/AOEM 哈希 | 本片测试与 D 后续固定 SHA 构建分别记录，旧包不能沿用新能力资格 |
| 唯一新 chain_id、epoch、timestamp、协议 pin | A 汇总共同草案，实际四机值尚未生成/批准 |
| A–D validator/transport 公钥、权重与映射 | 各机在共同域下本地生成，仅回传公开材料；当前 PENDING_GENERATION |
| 端点、服务角色、资源限制 | 各机只读清点，公开配置审核；不改 B 防火墙/端口 |
| UCA 管理/花费公钥、测试 NOV 分配、费用参数 | PENDING_APPROVAL，不把历史 fixture 当分配表 |
| UCA 签名域、初始状态根、最终创世承诺 | 上述输入完整后，由原编译器计算；当前 PENDING_GENERATION，不填假根 |
| 每机私钥、credential、DB/journal路径、库路径 | 留在各机本地，私钥不进入共同包或 Git |

沿已交付原入口，先 `native-genesis prepare` 取得原规范消息和观测域；审核固定消息后
各管理钥匙持有人用原 ML-DSA-65 合同签名，再 `native-genesis verify-approvals` 调用
同一原编译器计算最终三 pin。生成/签名/审核的具体命令见
[多机交接](NOVOVM_MULTI_HOST_NUAS_HANDOFF.md)。初始化清单或有效管理签名不是启动授权。
四机共同审核一致后仍须单独批准激活；不能各自编造不同创世或由 install 偷偷 create。

## 本片之后各机动作

以下为 2026-10-25 的历史分工记录，不是当前设备清单或执行授权。
各机只能消费 A 后续公布的同一固定源码、各自平台制品与共同配置；
本次用途迁移尚未签收四机认证、平台包、服务切换或启链。

- A：集成、测试、固定源码和公开共同配置；只按授权处理本机材料。
- C：同步固定源码，清点及退役回执，Windows 最终账户保护准备；手机/App 工作照旧。
- D：同步固定源码，在原 Linux 环境补 custody/CLI/包夹具回归，记录实际制品身份。
  用新固定代码重建（node/ctl 协议与配置已变化，不能只替换旧包一个文件）；不安装启链。
- B：本阶段不接管、不安装、不生成身份，不触碰原服务。由 A 后续统一协调。

D 最少运行 `cargo test -p novovm-network --lib existing_owner_read --locked`、
`cargo test -p novovm-relay --lib --locked`、`cargo test -p novovmctl --locked`，
再跑原三个 `scripts/tests/linux-package-*.tests.ps1`。保存失败，不减 feature/验签修绿。
新 Windows 回归和 Linux 待验证界限见本轮审计记录；本片没有新 TPS 或多机运行成绩。
