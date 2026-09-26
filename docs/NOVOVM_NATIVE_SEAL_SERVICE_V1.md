# 原生候选共识服务：主节点接入 V1

这次补的是：**把“负责人失联后，其他验证者接替处理同一候选”的能力接入
`novovm-node` 启动、收包和主循环。** 不再要求外部程序手动调用驱动器。

默认关闭，未部署到现有节点、聊天服务或手机。它目前只完成**一个固定高度、
一个已执行候选的 prepare QC**，不是连续出块，也不是主网最终确认。

## 启动前必须具备什么

1. 每台参与节点已经具备同一链、同一协议配置、同一候选的本地 AOEM-owned
   执行与持久账本事实。候选 hash 和高度必须精确匹配；缺失时拒绝启动，
   不自动下载候选，不凭远端声明补造执行结果。
2. 相同的固定 authority：链域、创世块、协议配置承诺、验证者及权重、
   验证者到传输 peer 的绑定。当前仅支持既有 genesis epoch 1，成员数 2–64。
   文件是完整 `NovNativeSealEpochAuthorityV1` JSON，不是随意填写的成员名单。
3. 每台节点自己的共识签名密钥，以及已经可用的 Product Overlay 配置。
   Overlay 必须为 `duplex`，配置其余**全部固定成员**，本地传输身份也必须
   与 authority 一致；不能为了少开一台机器而删掉那个成员。
4. 每台节点独立的持久 seal store。不得与账本、AOEM/native 状态、交付日志、
   缓存、报告输出或配置/密钥/TLS CA 路径混用。预检也包含已知备份、写锁、
   缓存临时文件；目录和链接须由可信操作者管理，不防启动后的恶意路径替换。
   不要删除签名锁来“修复”启动失败。

共识密钥通过 `signer_key_path` 配置；传输密钥通过 Overlay 的
`identity_key_path` 配置。两种身份应独立保管，不应复用；本服务不会从传输
密钥自动生成共识身份。共识密钥文件必须恰好为 **64 个 ASCII 十六进制字符**，
表示 32 字节 Ed25519 seed，无 `0x`、BOM、空白或换行。不要提交私钥，不要把
同一验证者私钥同时交给多个活动进程。本切片不提供生产密钥生成或分发流程。

## 配置与开启

下面是 `seal-service.json` 模板，**不是可直接运行的网络配置**：链 ID、高度
和候选 hash 必须改成现有账本事实；authority、密钥和 Overlay 必须预先准备。

```json
{
  "schema": "novovm-native-seal-service/v1",
  "enabled": true,
  "chain_id": 1,
  "height": 1,
  "block_hash": "<替换为本地已执行候选的64位十六进制hash>",
  "authority_path": "authority.json",
  "signer_key_path": "keys/validator.hex",
  "seal_store_path": "data/seal",
  "round_timeout_ms": 2000,
  "poll_interval_ms": 100,
  "ingress_per_source_per_second": 8,
  "ingress_per_poll": 16
}
```

`justify_qc_hash` 可省略或为 `null`；若设置，必须为有效的 64 位十六进制值，
并满足既有候选/本地证据规则。不要随意填充。未知字段、重复字段、错误域、
非成员签名密钥均拒绝；配置文件最多 64 KiB，authority 最多 256 KiB。

配置内相对路径以**该配置文件的目录**为基准，不要求各机器同盘符或同父目录。
配置、authority、密钥必须已存在；seal store 可以不存在，但其直接父目录必须
已存在。Windows 路径会做适用于 RocksDB 的规范化，隔离检查仍核对真实路径。

保留本机已经验证的 AOEM/native 运行配置，在原有启动脚本中明确增加：

```powershell
$env:NOVOVM_NODE_MODE = 'native_execution_tick'
$env:NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED = 'true'
$env:NOVOVM_PRODUCT_MAINLINE_OVERLAY_CONFIG = '.\node-config\overlay.json'
$env:NOVOVM_NATIVE_SEAL_ENABLED = 'true'
$env:NOVOVM_NATIVE_SEAL_CONFIG = '.\node-config\seal-service.json'
$env:NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS = '0'
$env:NOVOVM_NATIVE_EXECUTION_PIPELINE_EXIT_WHEN_SUMMARY_VALID = 'false'
```

环境变量中的配置文件路径相对启动目录；JSON 内部路径相对配置目录。
`NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID` 必须与两份配置一致，沿用本机正确值。
这里故意不提供未经本机验证的 AOEM DLL、状态库或引擎启动参数。

`MAX_TICKS` 默认是 1，设为 0 才是不按次数退出；还要关闭
`EXIT_WHEN_SUMMARY_VALID`，避免旧交易总结门提前结束进程。`native_execution_pipeline`
也支持此服务；`full` 模式需另有 `NOVOVM_NATIVE_EXECUTION_TICK_ENABLED=true`。
查询模式不允许开启签名服务。

默认不开启时应同时不设置 `NOVOVM_NATIVE_SEAL_ENABLED` 和
`NOVOVM_NATIVE_SEAL_CONFIG`。只提供配置路径、未明确启用会报错，不会默默签名。
配置不热更新；同一高度已持久绑定的候选、authority 或签名身份不能随意替换。

## 运行中看什么

启动日志为 `native_seal_service_startup`；每轮报告及最终总结包含
`native_seal` 状态，最终总结位于 `product_mainline_overlay.native_seal`。
状态反映最近一次实际调度检查，不是文件实时监视；持久证据故障在后续 poll 检出。

- `height / round / phase`：正在处理的固定候选和当前轮次。
- `chain_id / block_hash / local_validator_id`：核对各机链域、候选和本机身份，不含私钥。
- `prepared=true`、`qc_hash`：取得并持久保存 prepare QC，不代表最终确认。
- `queued_ingress / processed_ingress / rejected_ingress / dropped_ingress`：
  等待、已处理、拒绝和丢弃的消息；`queued_egress` 仅代表本地发送入队次数。
- `halted=true`：本地证据、身份、时钟或运行条件出错，停止签名并让主循环失败；
  错误消息不会继续显示为成功。远端坏消息单独拒绝，不直接停掉整个服务。

每个固定 peer 最多暂存 4 条消息，按来源轮转处理。每来源滑动一秒预算为
1–32 条，每次有效 poll 总预算为 1–64 条，拒绝的消息也计入预算。
`poll_interval_ms` 为 100–1000，`round_timeout_ms` 为 1000–300000，且前者
不得超过后者一半。主循环可能更慢；这些是调度/故障检测参数，**不是出块时间**。

仍不发送 `JournalPersisted` ACK：接收缓存不是持久收件箱，重启要重新收集远端
证据。本地签名锁、换轮证据、候选准入和 prepare QC 继续按原有持久规则恢复。
有界运行结束但没有 prepare QC 会失败，不把“进程正常退出”当成共识成功。

### 四节点掉一台：两种验收不要混淆

四个等权成员的 prepare 门槛是 3/4，只有 2/4 不推进。配置仍保留全部四人。
旧 `NOVOVM_PRODUCT_MAINLINE_OVERLAY_SIGNOFF_REQUIRED=true` 验的是完整网络/交易
交付闭环，其中 E2E 状态要求所有配置 peer 建连，不是 3/4 共识门。
故意停一台时，这个旧门可能失败，即使三个节点已经取得 prepare QC。

专门做故障接替演练时可显式设该旧开关为 `false`，单独检查 `native_seal`
结果；这**不代表**四节点全在线交付验证通过，也不会关闭本地封印故障检查。
正常交付验收不要为追求绿色而取消原有门槛。

## 已验证与未验证

2026-09-26 多进程推进：在真实 `novovm-node` 的 NovoRUDP 一发送端、三接收端
联调中，发现成功转发达到预算后，发送端把尚未执行的交易标为 Dropped。
这不是数据库故障：此前只展示最后的 RocksDB 汇总门错误，掩盖了执行量为零。
现已把广播预算与交易执行资格分离：停止后续广播，但保留待执行交易及正文；
迟到的发送成功通知也不能复活 Dropped/Rejected 或回退已执行区块候选。

本机修复后单轮及两轮发送均通过：三个独立接收进程各持久化 2 个候选块、8 笔
交易和 8 个交易/回执索引，AOEM readback 验证通过。广播预算仍为原值，未放宽
执行/持久化门槛。CI 增加三接收端；门禁保留已收集子进程 stdout/stderr，报告中
给出 `process_evidence_dir`，失败时也尝试上传日志。未完成或被终止的子进程不
保证有完整日志。本验证是回环 NovoRUDP 执行传播，不是四验证者封印或实体局域网。

回归范围：网络 runtime 状态 69 项、主节点流水线 32 项、双节点门禁单测 4 项
通过。网络全库串行测试为 477 通过、1 失败、1 忽略；唯一失败为
`evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3`，期待 reorg_count=1
但实际为 0。将本次唯一网络生产函数恢复为 HEAD 原实现后，该测试仍在同一断言
失败，记录为本机基线问题待修，不宣称网络全库或完整主线门禁全绿。

2026-09-26 后续修复：此前额外流水线测试的 5 项失败已修正，现在该组 32/32
通过，加入主线门禁（沿用 51 字段契约）。测试改为独立子进程、数据目录和节点
配置，不再读本机旧库或把持久路径放在 RPC 参数中。共享进程内传输回放不会将
Propagated 退回 Pending；测试逐项核对实际收到的交易正文、来源和 hash。

旧样例还错误地把执行完成视为 canonical。现在验证真实 AOEM 提交证据、成功业务
回执、持久状态和未封印标志，并反向验证 canonical / full-lifecycle 门拒绝该结果。
样例使用 NOV 储备操作与充足费用上限，不伪造 USDT 储备证明。这仍是既有
`legacy_host_transitional` 兼容流水线回归，不冒充 AOEM-owned 主进程生产签收。

宿主增加当前线程会话生命周期作用域：在线程退出前释放缓存的 AOEM 会话，
避免退出时才清理 worker pool 的等待。真实子进程覆盖正常返回、异常展开与重新
创建会话；没有跳过销毁、强制成功退出或修改 AOEM DLL。该作用域只管理当前线程，
不声称覆盖所有后台线程或物理断电。旧数据库未删除或迁移。

本机专项包含真实 WSS relay、E2E 加密、四个独立存储实例，覆盖 3/4 接替、
2/4 不推进、停启恢复、消息预算、公平处理、错误配置、错误身份和持久 QC
损坏后停签。所用候选执行事实、密钥和 TLS 证书都是测试 fixture；不是现场
AOEM 执行结果。超时用受控单调时钟，网络实际收发。

```sh
cargo test -p novovm-node --lib native_seal_service -- --test-threads=2
cargo run -p novovm-node --bin supervm-mainline-gate
cargo clippy -p novovm-node -p novovmctl --all-targets -- -D warnings
```

已增加[主节点共同候选本地执行模式](NOVOVM_NATIVE_CANDIDATE_NODE_MODE_V1.md)，
两个独立主进程可按同一计划真实执行 AOEM 并重启幂等回放；这不包含封印服务联动。

尚未完成：真实 AOEM 环境下开启封印服务的完整 `novovm-node` 正向多进程运行、局域网多台
实体机器部署、公网混合拓扑、物理断电恢复、连续高度推进、未知候选获取与
独立重放、最终确认和可恢复状态晋升。`proof_sealed`、`chain_canonical`、
`safe`、`finalized` 不因此变成 true。AOEM ABI/DLL/内核和聊天服务未改变。

前置说明：[加密通信适配](NOVOVM_NATIVE_SEAL_ROUND_OVERLAY_V1.md)、
[Product Overlay 主节点生命周期](NOVOVM_PRODUCT_MAINLINE_OVERLAY_LIFECYCLE_V1.md)。
下一步把真实 AOEM 共同候选接入完整节点的封印服务，再做局域网故障接替演示；
不能把本文件当成已经完成部署的证明。
