# NOVOVM Account Protocol Phase 2 实施清单（2026-04-20）

Status: AUTHORITATIVE CHECKLIST（Phase 2）  
Scope: `execution subject` 从 `address-first` 向 `account-first` 的主线迁移门禁

## 目标

`Phase 2` 只做一件事：

`execution subject 从 address-first 向 account-first 迁移`

本阶段不扩：

- 账户对象
- 资产层
- 隐私层

本阶段的验收目标是：

- 新执行入口默认以 `account_id` 为主体
- fee ownership 明确归属到 `account_id`
- nonce ownership 明确归属到 `account_id`

## 三刀顺序

1. 真实主线执行入口
   - 先改 `crates/novovm-node/src/mainline_query.rs`
   - 先改 `crates/novovm-node/src/tx_ingress.rs`
   - `nov_swap / nov_redeem / nov_openVault / nov_execute` 这类真实主线路径优先收成 `account-first`
2. Adapter ingress
   - 再改 `crates/novovm-adapter-novovm/src/lib.rs`
   - 把 `tx.from -> adapter_uca_id -> autoprovision -> route` 收敛成“优先显式 `account_id`，地址只作 fallback / 绑定校验”
3. 写入网关
   - 最后改 `crates/gateways/evm-gateway/src/main.rs`
   - `eth_sendRawTransaction / web30_sendTransaction / nov_sendRawTransaction / nov_execute` 全部改为主体、fee、nonce 可显式归属到 `account_id`

## 双轨兼容边界

当前阶段允许双轨兼容，但只允许以下边界：

- `account_id`：主语义
- `uca_id`：迁移期兼容别名
- `from / caller / external_address`：仅作 fallback 或绑定校验，不再作为新增主语义

每一刀都必须给出一组“旧输入兼容 -> 新主体落账”的对照样例，至少说明：

- 输入里是否显式带 `account_id`
- 若未显式带 `account_id`，fallback 如何解析
- 最终 receipt / trace / audit 落到哪个 `account_id`
- fee ownership / nonce ownership 最终落到哪个 `account_id`

## 明确禁止项

本阶段明确禁止：

- 禁止新增只收 `from / caller`、不收 `account_id` 的执行入口
- 禁止新增 `adapter_uca_id(&tx.from)` 这类地址生成主体逻辑
- 禁止新增地址级 nonce ownership 写法
- 禁止新增 fee ownership 只挂地址、不挂 `account_id`
- 禁止再往 dead `crates/novovm-node/src/main.rs` 增加主线语义
- 禁止在本阶段扩 `root / asset / privacy` 对象

## 合并门禁

合并门禁只有一句话：

`任何新增执行路径，如果主体、fee、nonce 不能显式归属到 account_id，则不得进入 mainline。`

PR 级最小验收必须同时满足：

- 新路径显式接收 `account_id`，或明确记录 fallback 到 `account_id` 的解析方式
- receipt 至少能看到主体归属
- trace 至少能看到主体归属
- audit 至少能看到主体归属
- fee ownership 明确归属到 `account_id`
- nonce ownership 明确归属到 `account_id`

建议在 code review 中直接用以下判定语：

`若该执行路径不能回答“主体是谁、谁付费、谁拥有 nonce”，并且三者都不能显式落到 account_id，则该变更不得合入主线。`
