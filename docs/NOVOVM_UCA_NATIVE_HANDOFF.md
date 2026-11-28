# NOVOVM 原生 UCA 节点交接

validator-C 维护 SUPERVM `main` 的开户和应用授权节点入口。开户是 UCA 根身份注册，资产账户开户和花费授权沿各自已有契约。NOVOCHAT 与 ASAS 继续分别验证设备持钥，以及同一可信发布状态上的当前权限。

## 注册提交和查询

使用与客户端固定链域一致的 ML-DSA-65 根钥签署 `UcaRegistrationIntentV1`。意图包含 `chain_id`、`genesis`、`identity`、`primary_key_ref`、`valid_from_height` 和 `valid_until_height`。高度下界包含，上界排除。签名及签名上下文随原生 `NUIR` 请求验证。

```powershell
novovm-native-client client-config.json submit-registration request.nuir
novovm-native-client client-config.json registration-status request.nuir
```

SDK 使用 `NativeApplicationClient::prepare_uca_registration`，状态读取使用 `Query::UcaRegistration { request_hash }`。请求哈希是完整签名文件的 SHA-256。超时重试用原始签名文件创建新的传输请求。传输层 `Delivered` 表示节点侧收到对象，客户端继续查询开户状态。

| 原生查询结果 | 含义 |
| --- | --- |
| `Received` | 当前询问的节点将该签名请求保留在内存队列。 |
| `Verified` | 节点完成了一次签验和父状态执行检查。仍等待共识与发布。 |
| `RetryQueueFull` | 节点的 16 个在途槽已占满。本次未排队。在有效期内退避后重试原请求。 |
| `RetryVerification` | 父状态执行检查失败或执行所有者当前不可用。先查询当前身份和高度。对于原签名仍有效的临时失败，退避后用同一文件重试。 |
| `OutsideValidityWindow` | 节点当前候选高度超出签名窗口。客户端根据当前高度判断继续等待或重新签署。 |
| `IdentityAlreadyRegistered` | 签名不同的同一身份申请在节点队列内竞争，其中一份已发布。查询链上的当前 UCA 身份。 |
| `Finalized { height, block_hash }` | 节点从通过校验的已发布归档索引到该请求。 |
| `NotRecent` | 节点未保留该请求的状态。查询链上的身份和当前高度，再决定重试。 |

发布详情以被询问节点的认证响应为准。近期结果投影有界，冷启动从原归档重建已发布记录。应用可用 `Query::UcaIdentity { chain_id, genesis, identity }` 读取当前身份，获得发布高度、状态根、身份状态和根钥引用。队列在途及退避提示只属于服务该请求的节点。共识候选沿原生 V5/V6 业务规则，每个区块最多执行一笔开户。开户请求不获得资产花费权限。

## 原链激活前置

在原创世与原验证者集合上取得共同已发布父锚，独立验证完整头证明、父区块、决策承诺、状态根、回执承诺和状态版本。使用对应候选对同一激活高度和业务指纹生成评审载荷，按原验证者集合签名权重取得所需原签名审批。为每个验证者准备同源节点与匹配 AOEM，确认原库恢复、签票继续及相同激活配置，再按获准窗口切换。同一激活高度前，旧配置继续执行此前业务规则。

## Windows 隔离实测

执行 `cargo test --locked -p novovm-node --lib actual_native_v2_existing_chain_uca_activation -- --ignored --nocapture --test-threads=1`。四个原生验证者实例在一台 Windows 主机的一个测试进程中运行，使用各自的 RocksDB、验证者钥和实际 localhost NovoRUDP/ML-DSA-65 路径。四个入口客户端持有独立的传输钥。使用 Rust 1.94.0、`RUST_MIN_STACK=33554432`，`NOVOVM_AOEM_DLL` 和 `NOVOVM_AOEM_TEST_LIBRARY` 都指向仓库 Windows 测试候选 `artifacts/runtime-candidates/native-transport-mldsa65-20261103/windows/aoem_ffi.dll`。

- 17 笔独立申请，1 笔首次饱和，退避重试后 17 笔确认。满队列时重发一份在途签名保持原请求状态，并得到同一份最终回执。发布高度 5 至 21。按首轮提交开始到已发布查询答复计时，P50 13,831 毫秒、P95 17,649 毫秒，17 笔合计 0.952 笔/秒，包含停提议者和排队时间。
- 高度 5 暂停验证者索引 3 的第 0 轮提议者，保留其余 3 个验证者。索引 0 接任第 1 轮，原提议者未恢复前其余验证者发布了高度 5。原提议者恢复后四个存储在高度 21 对齐发布头。冷重开后四个验证者返回相同链头和同一原始请求回执。
- 同身份两份不同签名申请同时入队，只有一份在高度 2 发布，另一份返回 `IdentityAlreadyRegistered`。高度 22 拒绝以 22 为排除上界的原签名。
- 定向查询协议 25 项通过。隔离用例 1 项通过，耗时 26.64 秒。首轮失败由测试名称格式误带 `0x` 引起，根钥校验按原规则拒绝，修正后重跑通过。

AOEM DLL SHA-256 `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。本次测试运行文件 SHA-256 `8ab88a9afc17e782b1a231622066e1df6350fa244cef0ba7de5edd88be401513`。本地完整测试 stdout/stderr 文件 SHA-256 `0289858ad0026aaac86d37c24ded8e0e20dfd87e6fa8dd8de53b66c0dfd0b88c`。逐请求原始测试输出见 [注册隔离测试记录](../artifacts/audit/native-uca-registration-20261128/registration-observations.txt)。编译时同一工作区保留了其他会话进行中的改动，测试文件摘要仅标识本次运行的产物。原链现役实例及其库未参与本次实测。

## 应用授权、撤销与原子换机

`NativeApplicationClient::prepare_application_grants` 提交根钥签署的 `SignedApplicationGrantV1`。每个授权范围由 `chain_id`、`genesis`、UCA `identity`、`application`、`service`、`device` 共同确定。命令同时绑定根钥引用、预期授权代次和命令有效高度。应用使用自己的固定用途标识和服务标识，使用设备通信公钥对应的用途钥引用。

- `Authorize` 建立或按预期代次更新一项授权，记录起效和到期高度。`Revoke` 按预期代次写入撤销记录。
- `ReplaceDevice` 由 `ApplicationGrantIntentV1::into_device_replacement(new_device, new_device_expected_revision)` 生成。原选择器指定旧设备，原预期代次约束旧授权。另两项字段固定新设备和新授权代次。新设备的授权必须在执行高度可生效。节点按同一发布父根检查两项代次，成功后在同一状态变更中撤销旧设备并授权新设备。任何一项失败时这条命令无状态变更。
- `Query::UcaApplicationCommand { command_id }` 可返回 `Received`、`Rejected`、`NotRecent` 或 `Finalized { receipt, height, block_hash, state_root }`。ID 由 `ApplicationGrantIntentV1::command_id()` 计算。发布回执含旧设备授权 ID、签发者、前后代次、撤销标记及原授权窗口。命令中的新设备授权用其新选择器读取。在两次授权查询中核对相同已发布高度与状态根。
- `Query::UcaApplicationGrant { selector }` 读取当前发布状态，返回 `Pending`、`IdentityMissing`、`Missing` 或 `Present`。存在时含授权代次、撤销、授权窗口、身份状态、签发钥引用及当前身份根钥引用。ASAS、NOVOCHAT 将此读取绑定到已验证的链域和可信当前发布状态，再独立验证对应设备对会话挑战的签名。

```powershell
novovm-native-client client-config.json submit-application replacement.nuap
novovm-native-client client-config.json application-status replacement.nuap
```

原子换机使用独立的 V6 业务指纹，需要当前节点候选及对共同原链父锚的 V6 审批。以前证书绑定的 V5 仍按原业务规则验证，V5 审批签名不能用于 V6。现有评审入口针对原始 V3/V4 规则的认证父锚。如果目标链已激活 V5，继续升级前需提供接续已激活业务历史的后续变更流程。线程内的四验证者和 ASAS 组件测试证据分别对应节点发布与设备挑战，手机系统解锁和跨机登录由联调方取得真机记录。

### V6 节点隔离结果

使用同一条 `actual_native_v2_existing_chain_uca_activation` 命令，改用已核对 Windows FULLMAX 发行输入的 AOEM DLL。库摘要为 `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`，测试程序摘要为 `92b699094ed03afb5db0d61b78b66960f356bf0777983b77ce0a754f60ebb3bd`。测试 1 项通过，耗时 33.08 秒。

- 原库在共同认证父锚上启用 V6。高度 4 的单笔换机在四个独立库同根发布。旧设备授权代次从 1 更新为 2 并撤销，新设备代次 1 生效。错误目标代次先被拒绝并保持原授权和原头，随后同一旧授权的第二份换机申请也被拒绝，竞争目标仍缺失。
- 换机命令 `2a2e1baae90604112036282b850253e4b6d9bde3e88d19745c6144e5da5a79f0`，区块 `2d010aca6d8d92a8e253738fa71573d0d50bd9a7a72bc54e806f55e89c56a623`，状态根 `825a5a15aca63b5841b13c459345781b3897d734c1e287fcc3f637112a5ebfa4`。四库冷恢复后按同一命令 ID 读回相同回执，新旧设备授权仍符合预期。
- V6 下重复通过 17/17 并发开户与饱和重试，高度 5 的第 0 轮索引 3 提议者暂停后，由索引 2 在第 1 轮接任。授权协议与其他协议单测 98 项、原子状态与冷恢复组件 1 项、V5/V6 激活证书边界 1 项通过。V5 审批在给定原规则下仍可验证，尝试以同一签名替换 V6 业务指纹时被拒绝。

完整 V6 测试日志 SHA-256 `8487db2446df4b0e1fb6fa2c6a5390abe08a6e101799fd0b474672bac576f4b6`。提取的原始测试结果见 [换机与开户隔离记录](../artifacts/audit/native-uca-registration-20261128/replacement-observations.txt)。原链使用 `novovmctl native-business-activation` 的 `prepare`、`approve`、`assemble` 核对和审批同一主题 SHA-256。现役部署及真机登录需要各自的验收窗口。
