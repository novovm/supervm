<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM FFI beta0.8 集成说明（2026-03-01）

> 对齐英文版本：`docs/AOEM-FFI/AOEM-FFI-BETA08-INTEGRATION-2026-03-01.md`。

## 1. 集成定位

1. AOEM FFI 是 NOVOVM 的执行内核接线层，负责执行能力，不负责覆盖层路由治理。  
2. NOVOVM 主线路径：`novovm-exec -> aoem-bindings -> aoem_ffi.dll`。  
3. 生产基线以二进制输入/输出路径为主，JSON 仅兼容与调试用途。  

## 2. 安装与产物口径

1. 默认核心库：`aoem/bin/aoem_ffi.dll`。  
2. 可选变体：`persist`、`wasm`（按需启用，不改变主 ABI）。  
3. 发布产物建议包含：`aoem-manifest.json`、`SHA256SUMS`、发布索引。  

## 3. 宿主绑定口径（NOVOVM）

1. 运行前硬门槛：`aoem_abi_version == 1`。  
2. 运行前能力门槛：`capabilities.execute_ops_v2 == true`。  
3. 建议启用 manifest 哈希校验，确保部署包与运行包一致。  

## 4. 生产边界约束

1. 覆盖层参数（`secure|fast`、多跳、`relay_*`）属于 node/gateway/plugin 宿主层，不进入 AOEM ABI。  
2. AOEM 保持传输中立与执行聚焦，不在 AOEM 层扩展覆盖层策略分支。  
3. 生产主线默认入口已统一到 `novovmctl`（前台 `novovmctl up`，守护 `novovmctl daemon`）；AOEM 只承接执行面。  

## 5. 关联文档

1. `docs_CN/AOEM-FFI/AOEM-INTRODUCTION-V1-2026-03-15.md`  
2. `docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`  
3. `docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`  
4. `docs_CN/AOEM-FFI/SUPERVM-ZK-PROOF-INTEGRATION-V1-2026-03-15.md`  
