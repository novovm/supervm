# AOEM beta0.8 校验与打包操作手册（2026-03-02）

## 脚本位置

- `scripts/aoem/build_aoem_manifest.ps1`
- `scripts/aoem/verify_aoem_binary.ps1`
- `scripts/aoem/package_aoem_beta08.ps1`

## 1) 生成本地 manifest（可选但建议）

```powershell
cd D:\WorksArea\SUPERVM
.\scripts\aoem\build_aoem_manifest.ps1 -AoemRoot .\aoem
```

输出：
- `aoem\manifest\aoem-manifest.json`

## 2) 启动前二进制校验（必须）

```powershell
cd D:\WorksArea\SUPERVM
.\scripts\aoem\verify_aoem_binary.ps1 -AoemRoot .\aoem -Variant core
.\scripts\aoem\verify_aoem_binary.ps1 -AoemRoot .\aoem -Variant persist
```

校验内容：
- SHA256（有 manifest 时强校验）
- ABI 版本（默认要求 `abi=1`）
- `aoem_capabilities_json` 关键能力（`execute_ops_v2=true`）

代码侧同步护栏：
- `aoem-bindings` 的 `AoemDyn::load()` 已内置启动硬闸门（ABI 与 `execute_ops_v2`）。
- 即使未先跑脚本，加载到不合规 DLL 也会直接失败。

## 3) 生成 beta0.8 打包模板

```powershell
cd D:\WorksArea\SUPERVM
.\scripts\aoem\package_aoem_beta08.ps1 -AoemRoot .\aoem -OutRoot .\artifacts\aoem-beta08 -Version beta0.8
```

输出目录：
- `artifacts\aoem-beta08\<timestamp>`

目录内自动生成：
- `windows/core|persist|wasm` 目录与 DLL 拷贝
- `linux/`、`macos/` 占位目录
- `VERSION.txt`
- `CAPABILITIES.json`（来自 core DLL）
- `SHA256SUMS`
- `aoem-manifest.json`

## 说明

- 对外发布仍使用 DLL/so/dylib。
- 研发基线仍以 AOEM native（crate 直连）口径为准。
- 发布门禁建议至少执行第 2、3 步。
