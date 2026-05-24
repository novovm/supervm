# SUPERVM AOEM FULLMAX 能力审计 V1

## 目的

本文给 SUPERVM 宿主用户、集成方和审计方确认当前 AOEM 包的完整能力边界。它不是性能宣传，也不是把 AOEM 说成通用 arbitrary-circuit ZK 平台。

当前结论：

```text
SUPERVM 当前接入的是单层 AOEM FULLMAX 宿主包。

Runtime Baseline:
  AOEM FULLMAX Runtime Baseline 2026-05-23
  Windows included and verified.
  Linux included and verified.
  macOS pending, not bundled, not advertised as available.

它包含：
  1. 新 AOEM Compute Native Proof Engine 核心能力
  2. FULLMAX runtime / plugin / sidecar 能力面

它不是：
  Proof-only package
  双层 Proof package + legacy sidecar package
  独立部署平台服务
```

## 当前安装形态

包根目录：

```text
aoem/
```

当前 SUPERVM 已包含加载路径：

```text
Windows:
  aoem/windows/core/bin/aoem_ffi.dll
```

当前平台状态：

```text
Windows:
  included
  已从 D:\WorksArea\AOEM 重新生成并安装 SUPERVM v1.2 FULLMAX bundle。
  artifact = D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\windows\20260523-181655

Linux:
  included
  已从 D:\WorksArea\AOEM 重新生成并安装 SUPERVM v1.2 FULLMAX bundle。
  artifact = D:\WorksArea\AOEM\artifacts\ffi-bundles\fullmax\linux\20260524-103416
  RISC0 recursion artifact 已校验通过。

macOS:
  pending_rebuild_not_bundled
  旧 SUPERVM macOS runtime / sidecar 已从 aoem/ 包内移除，避免误导用户。
```

当前 core hash：

```text
Windows aoem_ffi.dll:
  0879941e58f6879ecfcd7f1c49105fe6ba45ddad588550922307b808adf0a49d

Linux libaoem_ffi.so:
  d72ea8704b13681b91f23ac6714411046b5965ee5aeda5b6a249bcb0574a05ba
```

本地 manifest：

```text
aoem/manifest/aoem-manifest.json
aoem/aoem-sdk-manifest.json
```

## 单层 FULLMAX 包结构

```text
aoem/
  windows/
    core/bin/aoem_ffi.dll
    core/plugins/*.dll
    kms-hsm-plugin/*.dll
    include/aoem.h

  linux/
    core/bin/libaoem_ffi.so
    core/plugins/*.so
    kms-hsm-plugin/*.so
    include/aoem.h

  host-integration/
  worker-adapter/
  schemas/
  acceptance/
  docs/
  examples/
  bin/windows-x86_64/
  bin/linux-x86_64/
  manifest/
```

`host-integration/` 和 `worker-adapter/` 是同一个 AOEM FULLMAX 包内的宿主接入样板，不是第二层 Proof Engine 包。

macOS 目录当前不出现在包内。后续只有在从 `D:\WorksArea\AOEM` 重新生成、复制、校验并验收通过后，才重新 materialize 到 SUPERVM。

## 总调用模型

主推荐模式是嵌入式宿主调用：

```text
SUPERVM host process
  -> aoem_ffi.dll
  -> AOEM FFI ABI
  -> aoem_execute_ops_wire_v1 / typed FFI symbols
  -> AOEM state / output buffers
```

`aoem-proof-worker` 是可选 adapter / reference host，不是 AOEM 的产品边界。

## FULLMAX 能力总览

| 能力域 | 当前 Windows/Linux FULLMAX 状态 | 入口 / 代表符号 | SUPERVM 审计口径 |
| --- | --- | --- | --- |
| ABI / lifecycle | included | `aoem_abi_version`, `aoem_global_init`, `aoem_create`, `aoem_destroy`, `aoem_free` | AOEM 动态库基础生命周期 |
| Capability discovery | included | `aoem_capabilities_json` | 宿主可读取能力 JSON，避免黑盒误读 |
| Typed execution V2 | included | `aoem_execute_ops_v2` | 旧主线 typed ops 仍可用 |
| Wire execution V1 | included | `aoem_execute_ops_wire_v1` | Compute Native / Proof Engine 主入口 |
| AOEM state read/write/snapshot | included | `aoem_state_read_v1`, `aoem_state_write_v1`, `aoem_state_snapshot_v1` | proof/status/metadata 读回和状态交互 |
| Tensor compute | included | `compute.tensor.matmul_f32_v1`, `compute.tensor.add_f32_v1` via `wire_v1` | Compute Native tensor baseline |
| Tensor graph / Graph runtime internals | included / internal | `tensor_build_register_v1`, `aoem_graph_*` internal surface | 作为 AOEM 内部图/张量运行时能力审计；不是 SUPERVM 对外 Graph OS 产品路径 |
| Primitive u32 graph / 算子库 | included | `compute.primitive.u32_v1`, `aoem_execute_primitive_v1` | `sort/scan/scatter/fft/merkle/ntt/gemm` 语义族 |
| GPU primitive path | included / adaptive | `backend_gpu_path`, primitive backend policy | GPU-first / adaptive，不能泛化成所有 workload 性能声明 |
| ZK MSM workload | included | `compute.zk.msm_v1` via `wire_v1` | 已有 GPU-resident / correctness-oriented path；不是通用世界级 MSM 声明 |
| Resident pipeline digest | included | `compute.zk.resident_pipeline_digest_v1` | backend-preview digest workload |
| Resident proof workload | included | `compute.zk.resident_proof_v1` | fixed-profile proof engine |
| Resident asset lifecycle | included | `compute.zk.resident_asset_lifecycle_v1` | setup/list/select/release resident asset |
| Public Merkle membership | included | `profile_id=merkle_membership_v1` | 公开 inclusion proof，高吞吐公开路径 |
| Private ZK Merkle membership | included | `profile_id=zk_merkle_membership_v1` | 隐私 membership profile，隐藏 leaf/path/index |
| External verifier-compatible proof envelope | included | `aoem_resident_proof_contract_v1_le_hex` | proof bytes 可被外部 verifier 解析/验证 |
| JSONL worker adapter | included / optional | `aoem-proof-worker` | reference host / sidecar adapter，不是 AOEM 必选部署形态 |
| Classic hashes | included | `aoem_sha256_v1`, `aoem_keccak256_v1`, `aoem_blake3_256_v1` | 数据完整性 / 链上兼容 hash 能力 |
| Classic signature verify | included | `aoem_ed25519_verify_v1`, `aoem_ed25519_verify_batch_v1`, `aoem_secp256k1_verify_v1`, `aoem_secp256k1_recover_pubkey_v1` | 公链签名验签/恢复能力 |
| Ring signature | included | `aoem_ring_signature_*`, batch verify | 隐私签名能力；按具体路径审计，不泛化 |
| Groth16 | included | `aoem_groth16_*` | prove/verify/batch verify symbols present；具体生产语义按 profile/host path 审计 |
| Bulletproof | included | `aoem_bulletproof_*` | range proof 相关 symbols present |
| RingCT | included | `aoem_ringct_*` | 隐私交易证明相关 symbols present |
| RocksDB persistence | bundled sidecar | `aoem_ffi_persist`, `aoem_ffi_persist_rocksdb` | FULLMAX persistence plugin 随 Windows/Linux 包存在；运行启用按宿主配置/探测 |
| WASM / Wasmtime runtime | bundled sidecar | `aoem_ffi_wasm`, `aoem_ffi_runtime_wasm_wasmtime` | FULLMAX WASM plugin 随 Windows/Linux 包存在；不改变主 ABI |
| zkVM executor | bundled sidecar | `aoem_ffi_zkvm`, `aoem_ffi_zkvm_executor` | Trace/RISC0/SP1/Halo2 相关路径按后端可用性审计 |
| Native circuit / Halo2 | included by FULLMAX contract | `aoem_zkvm_prove_verify_v1` with `backend=halo2` | 原生电路 ZKP 路径，不能与 zkVM VM 语义混淆 |
| ML-DSA | bundled sidecar | `aoem_ffi_mldsa`, `aoem_ffi_crypto_mldsa` | 抗量子签名 sidecar 随 Windows/Linux 包存在；运行启用按配置/探测 |
| KMS/HSM | bundled sidecar | `aoem_ffi_kms`, `aoem_ffi_hsm`, `aoem_kms_plugin`, `aoem_hsm_plugin` | 密钥托管/硬件签名 provider sidecar 随 Windows/Linux 包存在 |
| Linux FULLMAX | included | `libaoem_ffi.so` | fresh Linux FULLMAX 产物已打包，RISC0 recursion artifact 已校验 |
| macOS FULLMAX | pending rebuild | `libaoem_ffi.dylib` | 当前不打包旧 macOS 产物；等待 fresh FULLMAX build |

## Windows / Linux FULLMAX sidecar 清单

```text
aoem/windows/core/plugins/
  aoem_ffi_persist.dll
  aoem_ffi_persist_rocksdb.dll
  aoem_ffi_wasm.dll
  aoem_ffi_runtime_wasm_wasmtime.dll
  aoem_ffi_zkvm.dll
  aoem_ffi_zkvm_executor.dll
  aoem_ffi_mldsa.dll
  aoem_ffi_crypto_mldsa.dll
  aoem_ffi_kms.dll
  aoem_ffi_hsm.dll
  aoem_ffi_crypto_kms.dll
  aoem_ffi_crypto_hsm.dll
  aoem_kms_plugin.dll
  aoem_hsm_plugin.dll

aoem/windows/kms-hsm-plugin/
  aoem_ffi_crypto_hsm.dll
  aoem_ffi_crypto_kms.dll
  aoem_ffi_hsm.dll
  aoem_ffi_kms.dll
  aoem_hsm_plugin.dll
  aoem_kms_plugin.dll

aoem/linux/core/plugins/
  libaoem_ffi_persist.so
  libaoem_ffi_persist_rocksdb.so
  libaoem_ffi_wasm.so
  libaoem_ffi_runtime_wasm_wasmtime.so
  libaoem_ffi_zkvm.so
  libaoem_ffi_zkvm_executor.so
  libaoem_ffi_mldsa.so
  libaoem_ffi_crypto_mldsa.so
  libaoem_ffi_kms.so
  libaoem_ffi_hsm.so
  libaoem_ffi_crypto_kms.so
  libaoem_ffi_crypto_hsm.so
  libaoem_kms_plugin.so
  libaoem_hsm_plugin.so

aoem/linux/kms-hsm-plugin/
  libaoem_ffi_crypto_hsm.so
  libaoem_ffi_crypto_kms.so
  libaoem_ffi_hsm.so
  libaoem_ffi_kms.so
  libaoem_hsm_plugin.so
  libaoem_kms_plugin.so
```

## Proof Engine 子能力

Proof Engine 是当前 SUPERVM 新增并接线的主能力域之一，但不是 AOEM 的唯一能力。

```text
SUPERVM host
  -> aoem_execute_ops_wire_v1
  -> compute.zk.resident_proof_v1
  -> AOEM state
  -> aoem_state_read_v1
  -> proof / verify_status / public_outputs / metadata
```

### `merkle_membership_v1`

```text
public inclusion proof
public root / leaf_hash / path / index
not zero-knowledge privacy proof
```

### `zk_merkle_membership_v1`

```text
private membership proof profile
hides leaf / sibling_path / leaf_index
external verifier-compatible
```

二者长期并存。`zk_merkle_membership_v1` 不替代 `merkle_membership_v1`。

## GPU 与性能边界

当前 bundle 中存在 GPU/accelerated 路由能力，例如：

```text
backend_gpu_path = true
msm_accel = true
compute_primitive_u32_v1_backend_policy = auto;spirv-vulkan;cuda_policy_if_available
```

审计口径必须保持精确：

```text
可以说：
  当前 AOEM FULLMAX 包包含 GPU-adaptive primitive / proof 后端能力。

不能说：
  所有 AOEM workload 都有性能 SLA。
  当前 SUPERVM bundle 已完成通用世界级 MSM。
  当前 proof engine 是 generic arbitrary-circuit ZK 平台。
```

## 与 FULLMAX 矩阵的关系

本文必须与以下文档联合审计：

```text
docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md
```

当前 SUPERVM AOEM 包对该矩阵的处理方式：

```text
保留 FULLMAX 能力域：
  RocksDB / WASM / zkVM / Halo2 / ML-DSA / KMS-HSM / RingCT / Bulletproof / Groth16 / primitive operators
  tensor engine / graph runtime internals

新增并默认接线：
  Windows/Linux SUPERVM v1.2 FULLMAX core + Compute Native Proof Engine host integration

待补齐：
  macOS v1.2 FULLMAX rebuild

不做双层表达：
  Proof Engine 是 AOEM FULLMAX 包内能力，不是第二个包。

不保留旧平台二进制：
  macOS 旧动态库已从 aoem/ 清除，避免误导宿主用户。
```

## 验收命令

嵌入式宿主：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/aoem/run_proof_engine_host_smoke.ps1 -SkipWorkerAdapter
```

期望输出：

```text
SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|profile=fixed_profile_v1|proof=ok|verify=ok|state_read=ok|metadata=ok|failures=0
```

Rust 宿主绑定检查：

```powershell
cargo check -p aoem-bindings
cargo check -p novovm-exec
cargo check -p novovm-node
```

Hash 复核：

```powershell
Get-FileHash aoem/windows/core/bin/aoem_ffi.dll -Algorithm SHA256
Get-FileHash aoem/linux/core/bin/libaoem_ffi.so -Algorithm SHA256
```

## 不可宣称项

```text
not generic arbitrary-circuit proof
not performance-ready claim
not standalone AOEM platform service
not Graph OS path
not dedicated LR path
no new public FFI ABI
Runtime Canon unchanged
Linux runtime included = true
macOS runtime included = false
```

## 审计检查清单

1. 确认 `aoem/manifest/aoem-manifest.json` 与 `aoem/aoem-sdk-manifest.json` 标明 Windows/Linux included、macOS pending_not_bundled。
2. 确认 `aoem-bindings` 能加载 `aoem_execute_ops_wire_v1` 与 `aoem_state_read_v1`。
3. 确认 `aoem/windows/core/plugins`、`aoem/linux/core/plugins` 和对应 `kms-hsm-plugin` 中 FULLMAX sidecar 存在。
4. 确认 `aoem/macos` 不存在旧动态库。
5. 运行 `scripts/aoem/run_proof_engine_host_smoke.ps1 -SkipWorkerAdapter`。
6. 确认 embedded host smoke 输出 `state_read=ok`。
7. 确认 `merkle_membership_v1` 被解释为公开 inclusion proof。
8. 确认 `zk_merkle_membership_v1` 被解释为隐私 membership profile。
9. 不把 worker adapter 解释为 AOEM 必须独立部署的服务。
10. 不把当前 Proof Engine 解释为通用 arbitrary-circuit ZK 平台。
11. 不把 GPU capability 字段解释为所有 workload 的性能承诺。
