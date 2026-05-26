# SUPERVM AOEM confidential_transfer_v1 宿主接入说明

## 定位

`confidential_transfer_v1` 是基于现有 AOEM RingCT 的宿主产品语义包装。

它不是新 ZK 能力，不是重新接 RingCT，也不是新的 Runtime Canon 路径。

```text
SUPERVM host
  -> aoem_ringct_prove_v1
  -> aoem_privacy_execute_v1
  -> RingCT transaction payload / verification status
```

## 和 Proof Engine 的关系

当前 Proof Engine 继续用于：

```text
compute.zk.resident_proof_v1
  merkle_membership_v1
  zk_merkle_membership_v1
```

RingCT 继续作为 FULLMAX 隐私交易能力保留：

```text
confidential_transfer_v1
  -> AOEM RingCT
  -> amount-hiding confidential transfer profile
```

两者不是互相替代：

```text
RingCT:
  更适合金额隐藏转账 / confidential transfer

Proof Engine:
  更适合 membership / eligibility / state proof / proof worker
```

## 宿主示例

```text
aoem/host-integration/embedded_confidential_transfer_host.c
aoem/examples/hosted_confidential_transfer_smoke.c
```

默认快速验收只确认宿主 wiring 和 RingCT 符号存在，不跑重型 range proof：

```text
SUPERVM_AOEM_CONFIDENTIAL_TRANSFER_HOST|profile=confidential_transfer_v1|ringct_symbols=ok|prove=not_run|privacy_execute=not_run|verify=not_run|mode=host_wiring_probe|failures=0
```

完整 RingCT 生成/验证路径需要显式传入：

```text
--run-prove
```

完整路径输出：

```text
SUPERVM_AOEM_CONFIDENTIAL_TRANSFER_HOST|profile=confidential_transfer_v1|ringct=ok|prove=ok|privacy_execute=ok|verify=ok|amount_hidden=ok|failures=0
```

`--run-prove` 会生成 64-bit RingCT range proof，不作为默认秒级 smoke。

## 边界

```text
不新增 public FFI ABI
不改 aoem_ringct_* 底层
不改 proof worker 默认任务
不改 Runtime Canon
不接 Graph OS
不接 dedicated LR
不做新 ZK circuit
不宣称 arbitrary confidential transaction platform
不做 performance-ready claim
```

一句话：

```text
RingCT 已经是 AOEM FULLMAX 隐私能力；
confidential_transfer_v1 是它面向 SUPERVM 宿主的产品化使用方式。
```
