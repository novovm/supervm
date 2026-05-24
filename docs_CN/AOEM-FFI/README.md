<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI 文档索引（SuperVM）

本目录仅保留 SuperVM 对外发布口径所需的 AOEM FFI 权威文档，避免旧口径混淆。当前 SUPERVM 接入的是单层 AOEM FULLMAX 包，不是 Proof-only 包。

## 当前权威（优先阅读）

1. `SUPERVM-AOEM-CAPABILITY-AUDIT-V1-2026-05-23.md`
2. `SUPERVM-AOEM-FULLMAX-RUNTIME-BASELINE-2026-05-23.md`
3. `SUPERVM-AOEM-PROOF-ENGINE-HOST-INTEGRATION-V1-2026-05-23.md`
4. `AOEM-INTRODUCTION-V1-2026-03-15.md`
5. `SUPERVM-ZK-PROOF-INTEGRATION-V1-2026-03-15.md`
6. `AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
7. `AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`
8. `AOEM-FFI-BETA08-INTEGRATION-2026-03-01.md`

## AOEM 上游权威（能力契约源）

- `docs/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
- `docs/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`
- `docs/AOEM-FFI/AOEM-FFI-BETA08-INTEGRATION-2026-03-01.md`

## 当前 SUPERVM 宿主口径

- AOEM 是 SUPERVM 可嵌入 FULLMAX execution/proof/crypto engine，不是独立平台服务。
- 当前 Runtime Baseline 是 `AOEM FULLMAX Runtime Baseline 2026-05-23`：Windows/Linux included and verified；macOS pending-not-bundled。
- Proof Engine 主路径是 `aoem_execute_ops_wire_v1 -> compute.zk.resident_proof_v1 -> aoem_state_read_v1`。
- FULLMAX 能力面包含 RocksDB / WASM / zkVM / ML-DSA / KMS-HSM / RingCT / Bulletproof / Groth16 / primitive operators 等。
- 当前 SUPERVM 包内包含新构建并验证的 Windows/Linux FULLMAX 动态库和 sidecar；macOS 旧动态库已清除，等待 fresh FULLMAX rebuild 后再重新打包。
- `aoem-proof-worker` 只是可选 adapter / reference host，不代表 AOEM 必须单独部署。
- 当前审计以 `SUPERVM-AOEM-CAPABILITY-AUDIT-V1-2026-05-23.md` 为入口。

## 发布口径规则

- 对外评估优先使用“当前权威”中的文档。
- 旧口径/过程性封盘文档已移至 `archive/`，不属于当前发布口径。
