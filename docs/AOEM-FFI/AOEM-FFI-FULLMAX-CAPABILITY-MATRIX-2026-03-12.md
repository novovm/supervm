<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI Fullmax Capability Matrix (2026-03-12)

> Release note: this file is the English counterpart of  
> `docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`.

## Snapshot

- Release version: `Beta 0.8`
- Windows fullmax: `artifacts/ffi-bundles/fullmax/windows/20260312-070556`
- Linux fullmax: `artifacts/ffi-bundles/fullmax/linux/20260312-070556`
- macOS fullmax: `scripts/build_macos_fullmax_bundle.sh <stamp> "<release_version>"`
- FFI coverage audit:
  - `artifacts/aoem-audits/zk-crypto-ffi-coverage/20260312-094222/aoem-zk-crypto-ffi-coverage-summary.json`

## Summary

- FFI export coverage audit: `overall_pass=true`, `missing_blocker_count=0`.
- Windows fullmax features:
  - `rocksdb-persistence,wasmtime-runtime,privacy-verify,mldsa,zkvm-executor,risc0,sp1,halo2`
- Linux fullmax features:
  - `rocksdb-persistence,wasmtime-runtime,privacy-verify,mldsa,zkvm-executor,risc0,sp1,halo2`
- macOS fullmax features:
  - `rocksdb-persistence,wasmtime-runtime,privacy-verify,mldsa,zkvm-executor,risc0,sp1,halo2`
- Boundary note (NOVOVM mainline):
  - This matrix covers AOEM FFI execution capabilities only.
  - Overlay routing controls (`overlay_route_mode/region/relay_*`) are governed in NOVOVM host layer, outside AOEM capability matrix scope.

## FFI Capability Matrix

| Capability Area | Representative Symbol | Windows fullmax | Linux fullmax | macOS fullmax | Notes |
| --- | --- | --- | --- | --- | --- |
| Unified Ingress | `aoem_global_init` | ready | ready | ready | One-time process-level init (idempotent), capability registration and plugin prewarm. |
| Generic Execution | `aoem_execute_ops_wire_v1` | ready | ready | ready | Main binary execution ingress for hosts, domain-agnostic (blockchain/AI/OS/robotics). |
| GPU Generic Primitives | `aoem_execute_primitive_v1` | ready | ready | ready | Unified primitive ingress: `sort/scan/scatter/fft/merkle/ntt/gemm`, GPU-first with CPU fallback. |
| Persistence Optional Plugin Component | `aoem_ffi_persist(_rocksdb)` | ready | ready | ready | Persistent backend plugin (RocksDB), plugin load/replace/disable with contract fallback. |
| WASM Optional Plugin Component | `aoem_ffi_wasm` | ready | ready | ready | Wasmtime runtime plugin, independently switchable without ABI change. |
| zkVM Optional Plugin Component | `aoem_ffi_zkvm(_executor)` | ready | ready | ready | Unified route for Trace/RISC0/SP1 backend execution. |
| Native Circuit Proving Engine | `aoem_zkvm_prove_verify_v1` (`backend=halo2`) | ready | ready | ready | AOEM native Halo2 **Zero-Knowledge Proof (ZKP)** circuit prove/verify engine (not a zkVM bytecode VM). |
| ML-DSA Optional Plugin Component | `aoem_ffi_mldsa` | ready | ready | ready | Post-quantum signature capability with 44/65/87 levels and batch verification. |
| KMS/HSM Providers | `aoem_kms_sign_v1` / `aoem_hsm_sign_v1` | ready | ready | ready | Key custody/hardware signing providers (`local|plugin|none`) for key isolation and compliant signing. |
| Classic Hash | `aoem_sha256_v1` / `aoem_keccak256_v1` / `aoem_blake3_256_v1` | ready | ready | ready | Classic hash primitives for host/business direct use. |
| Classic Asymmetric Signatures | `aoem_ed25519_verify_v1` / `aoem_ed25519_verify_batch_v1` / `aoem_secp256k1_verify_v1` / `aoem_secp256k1_recover_pubkey_v1` | ready | ready | ready | Signature verification and pubkey recovery, including Ed25519 batch path. |
| Ring Signature | `aoem_ring_signature_keygen/sign/verify` + `aoem_ring_signature_verify_batch_web30_v1` | ready | ready | ready | Privacy signature capability (membership anonymity proof), Web30 payload + batch verify. |
| Groth16 | `aoem_groth16_prove_v1/verify_v1/verify_batch_v1` | ready | ready | ready | zk-SNARK (Groth16) prove/verify + batch verify, GPU route optimization supported. |
| Bulletproof | `aoem_bulletproof_prove_v1/verify_v1` + `aoem_bulletproof_prove_batch_v1/verify_batch_v1` | ready | ready | ready | Range proof capability, single and batch prove/verify paths. |
| RingCT | `aoem_ringct_prove_v1/verify_v1` + `aoem_ringct_prove_batch_v1/verify_batch_v1` | ready | ready | ready | Privacy transaction proof (hidden amount + consistency verification), single and batch paths. |

## Optional Plugin Hot-Plug Levels

| Capability Area | Hot-Plug Level | Notes |
| --- | --- | --- |
| Persistence Optional Plugin Component | Process-Level Replaceable | Recreate handle or restart process after replacement; missing plugin can degrade by contract. |
| WASM Optional Plugin Component | Process-Level Replaceable | On/off by config, no ABI change. |
| zkVM Optional Plugin Component | Process-Level Replaceable | Backend plugins are swappable; capability routed by params + probing. |
| ML-DSA Optional Plugin Component | Process-Level Replaceable | Algorithm implementation can be replaced while keeping ABI stable. |
| KMS/HSM Providers | Process-Level Replaceable | Provider can switch among `local|plugin|none`; missing plugin yields explicit failure or disable behavior. |

## zkVM Backend Matrix (FFI)

| Backend | Windows fullmax | Linux fullmax | macOS fullmax | Notes |
| --- | --- | --- | --- | --- |
| Trace | ready | ready | ready | `aoem_zkvm_trace_fib_prove_verify` |
| Halo2 (Native Circuit) | ready | ready | ready | Native circuit **ZKP** backend, not a zkVM bytecode VM; used for AOEM-owned circuit proving path. |
| RISC0 | feature-on | ready | feature-on | On Windows/macOS: capability compiled in; runtime still depends on local backend environment. |
| SP1 | feature-on | ready | feature-on | On Windows/macOS: capability compiled in; runtime still depends on local backend environment. |

### zkVM vs Halo2 Boundary

1. `Trace/RISC0/SP1` are classified as zkVM path (VM semantics).
2. `Halo2` is classified as **Native Circuit** path in AOEM, not equivalent to zkVM VM semantics.
3. For external audits, report "zkVM capability" and "native circuit capability" separately.

## GPU Contract Scope

- Unified external primitive ingress: `aoem_execute_primitive_v1`
- Capability fields: `backend_gpu_path`, `msm_accel`, `msm_backend`, `gpu_adaptive_scope`
- Routing rule: adaptive by default, GPU-first, fallback to CPU if unavailable (no ABI change)

Key runtime parameters (verify/prove paths):
- `AOEM_RING_BATCH_BACKEND`, `AOEM_RING_BATCH_ADAPTIVE`
- `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST`, `AOEM_STAGE42_VERIFY_MANY_GPU_ASSIST_THRESHOLD`
- `AOEM_RINGCT_COMMITMENT_GPU_MODE`, `AOEM_RINGCT_COMMITMENT_FULL_GPU_KERNEL`, `AOEM_RINGCT_COMMITMENT_GPU_REQUIRE`
- `AOEM_GROTH16_VERIFY_GPU`, `AOEM_GROTH16_VERIFY_GPU_ASSIST`, `AOEM_GROTH16_VERIFY_GPU_MIN_INPUTS`
- `AOEM_STAGE42_MSM_BACKEND`, `AOEM_STAGE42_MSM_BACKEND_STRICT`

### Primitive Ingress vs MSM/Transformer

1. `aoem_execute_primitive_v1` is the unified primitive ingress for `sort/scan/scatter/fft/merkle/ntt/gemm`.
2. This ingress is GPU-first with CPU fallback; it is not CPU-only.
3. MSM is not exposed as standalone `primitive_kind`; it is routed via dedicated ZK/privacy backend parameters (for example `AOEM_STAGE42_MSM_BACKEND`).
4. Transformer is also not exposed as standalone `transformer_*` FFI symbols; it is routed through primitive graph profiles (for example `AOEM_PRIMITIVE_SORT_PROFILE`).
5. Therefore, "no standalone MSM/Transformer symbols" does not mean "not implemented"; this is by unified-ingress design.

### RingCT Full-Route Kernel Contract

- `AOEM_RINGCT_COMMITMENT_FULL_GPU_KERNEL=auto|bridge|dedicated`
- `auto/bridge`:
  - stable bridge kernel `ringct_ristretto_commitment_balance_msm_bridge_v1`
- `dedicated`:
  - dedicated kernel `ringct_ristretto_commitment_balance_dedicated_v1`
  - dedicated evidence + strict contract path
- Dedicated parallel knobs:
  - `AOEM_RINGCT_COMMITMENT_DEDICATED_PAR_MIN_BUCKETS`
  - `AOEM_RINGCT_COMMITMENT_DEDICATED_PAR_THREADS`

### Transformer Subgraph Contract (FFI)

- Routed through `aoem_execute_primitive_v1` + primitive profiles
- No new standalone `transformer_*` FFI symbols
- Related params:
  - `AOEM_PRIMITIVE_SORT_PROFILE`
  - `AOEM_PRIMITIVE_TRANSFORMER_MINI_AUTO`
  - `AOEM_PRIMITIVE_TRANSFORMER_MINI_MIN_LEN`

## Platform Notes

- On Linux, `librocksdb-sys` may occasionally show `libclang` thread-loading jitter on specific build paths.
- Current fullmax bundles include persist sidecar. If local build jitters, build persist first and then backfill it into the bundle.
- `scripts/export_aoem_beta08_bundle.ps1` is legacy compatibility tooling. Current release-standard packaging is fullmax core + sidecars.
