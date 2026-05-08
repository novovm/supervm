# NOVOVM Account Protocol KeyAlgo / AccountMode / ExecutionPolicy 分层规范（2026-04-20）

Status: AUTHORITATIVE LAYERING RULE  
Scope: 冻结统一账户中“主体、密钥算法、执行策略”三层主线分工，并把 `AccountMode` 明确定义为可选标签层，避免把基础加密、抗量子和隐私抬升为三套平行账户系统

## 目的

本文件只回答一件事：

`统一账户如何同时承载基础加密、抗量子和隐私能力，而不分裂账户主体。`

本文件不是完整实现说明，也不是在宣称这些分层对象已经全部成为当前主线代码中的一等运行时对象。

本文件的作用是冻结：

- 什么是主体
- 什么是密码学算法分类
- 什么是执行路由策略
- 什么情况下 `AccountMode` 只能作为可选标签存在

## 核心规则

统一账户的规则是：

`account_id 只回答“谁”，不回答“用什么密码学”，也不回答“怎么执行”。`

这意味着：

- `account_id` 是统一主体
- `key_algo` 是密钥/公钥的算法分类
- `execution_policy` 是执行时的路由与可见性策略
- `account_mode` 如果存在，也只是非主线标签，不是能力真相源

因此，不存在三套平行主体系统：

- “基础账户”
- “抗量子账户”
- “隐私账户”

存在的是：

`一个主体 + 绑定能力 + 策略路由`

## 三层主线 + 一个可选标签

### 1）主体层：`account_id`

主体层只定义：

- 谁拥有账户
- 谁拥有绑定关系
- 谁拥有 nonce / fee / audit 主语义

主体层不因以下因素而改变：

- `secp256k1`
- `ed25519`
- `mldsa87`
- 隐私密钥
- 隐私执行策略

### 2）密钥算法层：`key_algo`

`key_algo` 只回答：

`该绑定密钥属于哪种密码学算法。`

最小推荐集合：

```rust
pub enum KeyAlgo {
    Secp256k1,
    Ed25519,
    Mldsa87,
}
```

这一层属于：

- 公钥/签名材料分类
- 算法兼容性判断
- 算法有效性校验

这一层不直接决定：

- 账户是否是隐私账户
- 执行是否必须走隐私路径

### 3）执行策略层：`execution_policy`

`execution_policy` 只回答：

`执行时应该走什么路由和可见性要求。`

最小推荐集合：

```rust
pub enum ExecutionPolicy {
    Standard,
    PqRequired,
    PrivacyRequired,
    Confidential,
}
```

这一层属于：

- 路由选择
- 隐私/保密要求
- 对执行路径的强制约束

这一层不应该反向改写：

- 主体定义
- 密钥算法事实

### 4）可选标签层：`account_mode`（非主线）

`account_mode` 在当前 capability-driven 模型下不是必需层。

如果未来确实引入，它也只回答：

`这个账户需要怎样的展示标签或控制面提示。`

它最多只能属于：

- UI 标签
- 控制面提示
- 非权威元数据

它明确不属于：

- 主体判定
- 密钥算法事实
- 执行路由真相
- 资产归属或隐私真相

如果未来真的需要最小枚举，也只能作为可选标签示意，而不是主线语义层：

```rust
pub enum AccountMode {
    Basic,
    PostQuantum,
    Privacy,
    Hybrid,
}
```

## 必须遵守的流程

这层设计不能只做“分类声明”，必须是：

`声明 -> 校验 -> 绑定 -> 路由`

也就是：

1. 用户声明：
   - `key_algo`
   - `execution_policy`
2. 系统校验：
   - 提供的公钥/签名材料确实属于该 `key_algo`
3. 系统绑定：
   - 将该能力绑定到账户主体
4. 系统路由：
   - 根据 `execution_policy` 选择执行路径

如果未来存在 `account_mode`，它也只能在绑定后作为标签附着，不得参与主线路由真相。

不允许以下伪分类：

`用户自称是抗量子账户 -> 系统不校验就直接按抗量子处理`

## 为什么“隐私”不能只挂在 `key_algo`

抗量子是：

`密码学算法属性`

隐私是：

`执行模式 + 记录可见性属性`

因此：

- 抗量子主要挂在 `key_algo`
- 隐私主要挂在 `execution_policy`
- `account_mode` 最多只能是非权威标签

否则会把：

- 密码学算法
- 可见性策略
- 路由策略
- 展示标签

混成一层。

## 最小对象示意

推荐用以下结构表达绑定，而不是把这些概念写进主体层：

```rust
pub struct AccountKeyBinding {
    pub account_id: [u8; 32],
    pub key_id: [u8; 32],
    pub key_algo: KeyAlgo,
    pub execution_policy: ExecutionPolicy,
    pub account_mode_hint: Option<AccountMode>,
    pub public_key: Vec<u8>,
}
```

这里要点只有一个：

`account_id` 是主体，其余字段都是能力层。

## 当前事实与当前未宣称完成的范围

当前已经成立的事实是：

- 统一账户主体协议已进入真实主线
- 主体绑定、策略、nonce、审计已成立
- 治理扩展层已经存在受控的 `mldsa87 external vote` 路径

当前未宣称成立的是：

- 统一账户主密钥层已经完整实现 `key_algo / account_mode / execution_policy` 一等对象
- 统一账户已经完整实现“基础加密 + 可选抗量子 + 隐私”三路账户能力下沉

当前已进入主线实现并已封盘的最小子集只有：

- `Cut A / KeyAlgo`
  - 已补齐 `KeyAlgo` 元数据
  - 已补齐持有证明校验
  - 已补齐查询与审计可见性
- `Cut C / ExecutionPolicy`
  - 已补齐最小枚举：
    - `Standard`
    - `PqRequired`
    - `PrivacyRequired`
  - 已补齐主线唯一 resolve + enforcement
  - 已补齐明确拒绝路径与审计可见性
  - 已明确禁止 silent fallback

当前尚未进入实现的是：

- `Cut B / AccountMode`（当前默认 `No-Go`，且不是主线必需层）
- `Cut C` 之外的扩展执行策略切片
  - `Confidential` 完整路径
  - 任何会触碰资产账本、隐私资产空间或新状态源的 policy 行为

当前对 `Cut B` 的正式定位是：

`可选标签层，不是主线语义层。`

除非出现无法用 `KeyAlgo + ExecutionPolicy` 表达的真实需求，否则不引入 `AccountMode`。

因此，本文件冻结的是：

`正确分层方向`

不是：

`所有相关代码已经全部完成`

## 建议对外口径

`统一账户不区分基础账户、抗量子账户和隐私账户三套主体；主线能力由绑定密钥算法与执行策略决定。account_id 只负责主体；当前 Cut A / KeyAlgo 与 Cut C / ExecutionPolicy 已进入实现，AccountMode 只作为可选标签存在，不作为主线语义层。`
