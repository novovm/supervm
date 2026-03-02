# SVM2026 功能迁入 SUPERVM 架构（AOEM 内核寄宿）- 2026-03-03

## 目标架构

```text
SVM2026 (已验证功能/策略)
        | 迁入
        v
SUPERVM 目标模块
        |
        v
novovm-exec (统一执行门面)
        |
        v
aoem-bindings (FFI 绑定层)
        |
        v
aoem_ffi.dll (AOEM 执行内核)
```

## 职责边界

- `novovm-exec`
  - 对 SUPERVM 提供稳定执行 API。
  - 封装 handle/session 生命周期。
  - 统一并行建议和全局预算策略调用。
- `aoem-bindings`
  - 只负责 ABI 绑定、manifest 校验、DLL 加载。
- `aoem_ffi.dll`
  - 执行内核本体（CPU/GPU/可选 persist/wasm 变体）。

## 禁止项

- 将 SVM2026 整体拷贝进 SUPERVM（只迁能力，不迁历史负担）。
- SUPERVM 模块绕过门面直接散落调用 FFI 符号。
- 在多个模块分叉实现 AOEM 调度逻辑（应在门面统一）。
