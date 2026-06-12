# NOVOVM 统一身份与资产生命周期审阅

日期：2026-06-13  
范围：`D:\WEB3_AI\SUPERVM` 本地代码与文档  
不在范围：`D:\WEB3_AI\MEV`、外部钱包前端、真实 Ethereum lock 合约部署、安全审计

## 1. 总结结论

NOVOVM 现在已经具备“统一身份 + EVM 产品入口 + native NOV 经济模块”的基础闭环，但还不能声明“用户从 Ethereum 锁 ETH 后自动铸造 NOV 并可全自动赎回”的完整跨链资产生命周期。

可以签收的部分：

- 统一身份注册、主密钥、策略、persona 绑定、EVM 地址绑定、Web30 地址绑定、绑定查询、nonce 查询、审计事件，已经有生产入口。
- EVM 用户路径已经进入宿主：`eth_sendRawTransaction -> gateway -> native pending -> host execution -> canonical receipt -> eth_getTransactionReceipt`。
- native NOV 经济模块已有余额、treasury reserve、fee settlement、redeem、AMM swap、credit vault/mint debt asset 等本地执行与 receipt。
- `account_balance` / `account_assets` 已经能把 native liquid balance、mapped asset、credit vault、treasury exposure 汇总成账户视图。
- 统一账户唯一入口已收敛为 `novovm-node -> mainline_query -> unified_account_surface`；`evm-gateway` 不再拥有本地 UCA store/router，只保留 EVM RPC adapter 职责。

不能过度声明的部分：

- 当前没有发现 NOVO Wallet 前端/钱包应用代码；仓库里是 RPC/节点/gateway 能力，不是完整用户 App。
- ETH 锁仓合约不是已部署真实 Solidity lock contract 路径；当前 `ua_registerMappedLock` 是 MVP 内部 proof digest 校验，目标资产为 `NETH`，不是真实 Ethereum receipt/log Merkle proof。
- 没有发现“Ethereum lock event -> 自动验证 -> 自动 mint NOV”的完整接线。NOV mint 在 consensus token runtime 中存在，但不是直接接在 ETH lock proof 上。
- EVM 合约币可以在 EVM 产品面执行/查询 receipt，但和 NOV native account balance/treasury 是两套状态面，当前没有完整 ERC20 -> native asset 自动映射桥。
- 并发量不能直接引用 README 的 L0/L1 百万 TPS 来代表钱包/gateway 入口吞吐。gateway 当前是单 HTTP loop，EVM pending consumer 默认 16 笔/250ms；native NOV store 是 JSON load-modify-write，适合单进程顺序产品闭环，不适合多进程高并发账本写入口。

## 2. 生命周期现状

### 2.1 注册统一身份

入口：

- `ua_createUca`

实现位置：

- `crates/novovm-adapter-api/src/unified_account.rs`
- `crates/novovm-node/src/unified_account_surface.rs`

生命周期：

1. 用户提供 `uca_id/account_id` 和主密钥信息。
2. `UnifiedAccountRouter` 创建 UCA，状态为 active。
3. `unified_account_surface` 持久化 UCA snapshot，并写审计事件。

状态：可用。

风险：

- UCA 是统一身份核心，生产默认应使用 RocksDB 后端；bincode/file backend 已被环境变量限制为非生产路径。

### 2.2 绑定 NOVOVM / Web30 账户

入口：

- `ua_bindPersona`

生命周期：

1. UCA 已存在。
2. 用户绑定 `PersonaType::Web30`，带 `chain_id` 和外部地址。
3. 后续 Web30 交易通过绑定 owner 解析 UCA。

状态：可用。

风险：

- Web30/统一账户 host 入口必须走 `novovm-node -> mainline_query -> unified_account_surface`，不能从 `evm-gateway` 创建第二份账户状态。

### 2.3 绑定 Ethereum 账户

入口：

- `ua_bindPersona`
- `eth_sendRawTransaction`

生命周期：

1. 用户先绑定 `PersonaType::Evm`，`chain_id=1` 或目标 EVM chain。
2. `eth_sendRawTransaction` 会从 raw tx 恢复 sender。
3. 如果 sender 已绑定 UCA，则可省略 `uca_id`；如果显式提供 `uca_id`，必须与 mainline 绑定 owner 一致。
4. `eth_sendRawTransaction` 通过 mainline `ua_checkRoute` 做只读预检；mainline UCA 不可用或策略拒绝时 fail closed，不 fallback 到 gateway 本地状态。
5. gateway 只作为 EVM RPC adapter 对 raw tx 执行 chain id、nonce、tx type、fork activation、sender、fee 等 EVM 兼容校验；统一账户状态的注册/绑定/策略以 mainline surface 为准。

状态：可用。

风险：

- 外部钱包必须先完成 mainline UCA/EVM persona 绑定；仅传 `uca_id` 不能绕过绑定与策略预检。钱包接入层需要封装注册/绑定引导。

### 2.4 查询账户、绑定、余额、资产

入口：

- `ua_getAccount`
- `ua_getPolicy`
- `ua_listBindings`
- `ua_getBindingOwner`
- `ua_getNextNonce`
- `ua_getAuditEvents`
- `account_balance`
- `account_assets`

生命周期：

1. UCA 查询返回账户、策略、绑定数量。
2. Binding owner 查询可从 Ethereum/Web30 persona 反查 UCA。
3. `account_balance` 聚合：
   - native liquid balance
   - mapped asset active balance
   - mapped asset shadow balance
   - credit vault locked collateral
   - debt outstanding
   - treasury source flow
   - treasury NOV bucket exposure
4. `account_assets` 返回资产列表、pledge、vault、treasury exposure、mapped assets。

状态：可用。

风险：

- 查询口径是“聚合视图”，不是单一资产账本。前端必须展示 component/source，否则用户会误以为 mapped/shadow/treasury exposure 都是可立即提现吗。

### 2.5 转账、付款、使用

EVM 入口：

- `eth_sendRawTransaction`
- `eth_getTransactionReceipt`
- `eth_getTransactionByHash`
- `eth_getBlockByNumber`
- `eth_call`
- `eth_estimateGas`

native NOV 入口：

- `nov_sendRawTransaction`
- `nov_sendTransaction`
- `nov_execute`

生命周期：

1. EVM raw tx 立即返回 tx hash。
2. gateway 后台 consumer 从 native pending 消费。
3. mainline EVM execution 产出 canonical batch。
4. pending tx 标记为 included/onchain。
5. 用户通过 receipt/block/tx 查询结果。
6. native NOV execute 可以调用 treasury、AMM、credit_engine 等模块，并持久化 native receipt。

状态：EVM 产品 v1 可用；native NOV 模块可用。

风险：

- `eth_sendRawTransaction` 是异步提交语义，不应等待执行完成。这符合 Ethereum RPC 习惯，但钱包/产品层必须做 receipt 轮询。
- native NOV 执行 store 当前是 JSON load-modify-write；单 gateway 顺序执行可用，多进程/多 writer 需要收口为单 writer 或 RocksDB/事务化后端。

### 2.6 Ethereum 合约币

现状：

- EVM 插件已支持合约部署、调用、receipt/log、block/tx 查询产品路径。
- EVM 合约内的 ERC20/合约币可以作为 EVM 状态的一部分被执行和查询。

边界：

- 当前未发现 ERC20 balance 自动同步到 `account_assets` 的 native asset balance。
- 当前未发现 ERC20 Transfer event 自动映射到 native treasury/asset ledger 的产品接线。

状态：EVM 内可用；跨入 NOVOVM native 账户资产视图为 partial。

### 2.7 ETH 锁仓合约与 mapped asset

入口：

- `ua_registerMappedLock`
- `ua_getMappedAsset`
- `ua_burnMappedAsset`
- `ua_releaseMappedLock`

当前实现：

1. 用户提供 mapped lock proof。
2. surface 校验目标 UCA 存在。
3. proof 校验要求：
   - amount > 0
   - source asset 必须是 `ETH`
   - source chain 必须是 Ethereum
   - proof format 是 `EthereumLockEventV1`
   - proof payload 必须等于本地 `mapped_lock_proof_digest_v1`
4. 注册后创建 mapped asset record。
5. 目标资产固定为 `NETH`。
6. active 状态计入 `account_balance/account_assets` 的 mapped asset component。
7. burn 后进入 `BurnPending`。
8. release 后进入 `Released`。

状态：内部 MVP 闭环可用。

关键边界：

- 这不是完整真实 Ethereum lock contract proof。
- 没有验证 Ethereum receipt inclusion、log index、contract address、event topic、block finality、reorg。
- Phase4 shadow/no-go 环境变量可阻断 live register 路径。
- `settlement_effect` 显示这是 mapped asset 状态效应，不应直接解释为链上 ETH 已释放。

### 2.8 NOV 铸造

已存在能力：

- `crates/novovm-consensus/src/token_runtime.rs` 中 `TokenRuntime::mint` 可向 NodeId 对应地址 mint NOV，并维护 `minted_locked_total`。
- native treasury settlement 可把支付资产折算成 NOV bucket。
- credit engine `open_vault` 可在抵押后 mint debt asset，例如默认 `NUSD`，不是 NOV。

未完成接线：

- 没有发现 `ua_registerMappedLock(ETH)` 成功后自动调用 `TokenRuntime::mint` 铸造 NOV。
- 没有发现 Ethereum lock event 与 NOV mint policy 的一体化产品流程。
- 没有发现真实外部 ETH reserve 与 NOV supply 的一一约束关系。

状态：NOV mint 能力存在于 consensus runtime；ETH 锁仓触发 NOV 铸造为 gap。

### 2.9 余额赎回

入口：

- native module `treasury.redeem`
- native module `treasury.redeem_reserve`
- mapped asset `ua_burnMappedAsset`
- mapped asset `ua_releaseMappedLock`

生命周期：

1. native `treasury.redeem/redeem_reserve` 从 treasury reserve 扣除资产。
2. NOV 赎回会检查 reserve bucket、min reserve、total reserve。
3. 成功后 credit 用户 native balance，并写 settlement journal。
4. mapped asset 赎回分两步：burn mapped asset，再 release source lock。

状态：native reserve 赎回可用；mapped asset burn/release 内部闭环可用。

风险：

- mapped asset release 不是 Ethereum 链上解锁交易。
- native treasury redeem 是内部账本状态变化，不等于外部资产链上出金。

## 3. 发现的问题

### P0：真实 ETH lock -> NOV mint -> redeem 的完整桥接不存在

影响：

- 不能向用户声明“锁 ETH 自动铸 NOV，NOV 赎回自动释放 ETH”已经完成。

证据：

- mapped lock proof 目前是本地 digest 校验，不是 Ethereum receipt/log proof。
- target asset 是 `NETH`，不是 NOV。
- `TokenRuntime::mint` 没有接到 mapped lock 成功路径。

建议：

- 下一步只做最小产品桥：
  - 固定一个 lock contract address 配置。
  - 只验 `Locked(address indexed owner, bytes32 lockId, uint256 amount, string targetUca)` 事件。
  - 只支持 finalized block 后的 receipt/log proof。
  - 只把 ETH 映射为 `NETH`，不要直接铸 NOV，除非经济模型明确。

### P1：钱包/用户入口缺失

影响：

- RPC 能力已经有，但普通用户没有完整注册、绑定、提交、查询、赎回界面。

证据：

- 仓库未发现 NOVO Wallet app/front-end。
- 现有能力集中在 gateway/RPC/node。

建议：

- 不要先做大钱包工程。先做一个最小 wallet adapter 文档和 1 个 CLI/API smoke：
  - create UCA
  - bind EVM address
  - submit raw tx
  - poll receipt
  - query account_assets

### P1：native NOV 账本写入后端不适合多 writer

影响：

- `novovm-native-execution-store.json` 是 load-modify-write；如果多个进程同时写，会丢更新。

证据：

- `load_nov_native_execution_store_v1` 直接读 JSON。
- `save_nov_native_execution_store_v1` 直接 `fs::write`。

建议：

- 产品部署上先规定单 gateway writer。
- 如果要并发写入，最小改法是把 native execution store 切 RocksDB 或加单进程队列，不要做复杂工程化。

### P1：余额视图需要前端解释 source/component

影响：

- 用户可能把 mapped/shadow/treasury exposure 误解为可立即转账余额。

证据：

- `account_balance` 返回 liquid、mapped、pledge、debt、treasury exposure 多 component。

建议：

- 钱包展示分层：
  - 可用余额
  - 锁定/抵押
  - 映射资产
  - 债务
  - treasury 流水/敞口

### P2：EVM 合约币和 native asset ledger 未完全打通

影响：

- EVM ERC20 可以在 EVM 内执行，但不会自动成为 NOVOVM native `account_assets` 的 liquid asset。

建议：

- 不要做全 ERC20 桥。先选白名单 ERC20，按 receipt log 索引为只读资产视图，再决定是否允许映射为 native asset。

### P2：并发口径需要拆层

影响：

- 容易把 AOEM/consensus benchmark 的 TPS 当成 wallet/gateway TPS。

当前口径：

- AOEM/consensus benchmark：文档内有百万级 TPS seal，但属于内核/consensus plane。
- EVM gateway 用户入口：`tiny_http` 单 loop；默认 pending execution `16` 笔/`250ms`。
- native NOV store：JSON store，单 writer 可用，不是高并发存储层。

建议：

- 对外写法必须分层：
  - “内核/共识吞吐”
  - “EVM gateway 产品入口吞吐”
  - “Ethereum mainnet RLPx 同步吞吐”
  - “native treasury/account store 写入吞吐”

## 4. 端到端产品流程说明

### 流程 A：普通 EVM 用户提交交易

```text
ua_createUca
  -> ua_bindPersona(Evm, chain_id, address)
  -> eth_sendRawTransaction(raw_tx)
  -> gateway validates chain/sender/nonce/policy
  -> native pending
  -> gateway background consumer
  -> mainline EVM execution
  -> canonical batch
  -> eth_getTransactionReceipt
```

状态：已闭环。

### 流程 B：NOVOVM native 支付/使用

```text
ua_createUca
  -> ua_bindPersona(Web30 or native account)
  -> nov_sendTransaction / nov_execute
  -> fee settlement
  -> native module dispatch
  -> native receipt
  -> account_balance/account_assets
```

状态：基础闭环可用。

### 流程 C：ETH 映射资产

```text
ua_createUca
  -> ua_bindPersona(Evm)
  -> ua_registerMappedLock(ETH proof)
  -> NETH mapped asset active
  -> account_assets shows mapped asset
  -> ua_burnMappedAsset
  -> ua_releaseMappedLock
```

状态：内部 MVP 闭环可用；真实 Ethereum lock contract proof/链上释放未完成。

### 流程 D：ETH 锁仓铸 NOV

```text
Ethereum lock contract
  -> verified Ethereum receipt/log proof
  -> NOVOVM mapped lock
  -> NOV mint policy
  -> TokenRuntime::mint or treasury mint path
  -> account_balance NOV
  -> redeem/release
```

状态：目标流程，当前未闭环。

## 5. 并发量判断

当前可负载的层次应这样说：

- EVM mainnet long-sync：已通过 6h/24h soak，说明同步链路可长期运行和重启恢复，不代表钱包请求 TPS。
- EVM gateway 产品入口：默认每轮最多消费 16 笔 pending，cooldown 250ms，理论调度上限约 64 tx/s/进程，实际受 EVM 执行、IO、receipt/canonical store 影响。
- native NOV execution：当前 JSON store 单 writer 可用，不应做多进程并发写入。
- AOEM/consensus：可继续引用已有 seal，但必须注明不是 gateway/wallet 产品入口吞吐。

结论：

- 当前适合产品 v1 小规模真实用户/内部公网节点路径。
- 若要公测高并发，需要先把 gateway/native store 写入口统一成单 writer 队列或 RocksDB/事务后端，再压测。

## 6. 最小下一步

不要继续扩散 EVM fixture、BAL、debug/admin/trace。下一步只做 3 个最小闭环：

1. 钱包接入 smoke：用真实 JSON-RPC 顺序跑 `ua_createUca -> ua_bindPersona(Evm) -> eth_sendRawTransaction -> eth_getTransactionReceipt -> account_assets`。
2. ETH mapped asset smoke：跑 `ua_registerMappedLock -> account_assets -> ua_burnMappedAsset -> ua_releaseMappedLock`，并在输出里明确 `internal MVP / non-settlement`。
3. NOV native treasury smoke：跑 `treasury.deposit_reserve -> treasury.redeem -> account_balance/account_assets`，验证 native 余额和 journal。

如果这三条 smoke 都稳定，才能进入“真实钱包入口接入”。如果目标是“ETH 锁仓合约 -> NOV 铸造”，应单独开一个小阶段，不要混进 EVM v1。
