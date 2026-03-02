# SVM2026 功能迁入 SUPERVM 设计（AOEM 内核寄宿）- 2026-03-03

## 目标

- 将 `SVM2026` 已验证功能迁入 `SUPERVM`，避免双线维护。
- `SUPERVM` 不再依赖 AOEM 源码路径（`../aoem/crates/...`）。
- 执行层统一走 AOEM FFI V2（二进制、typed ABI、内核）。
- 保持宿主工程纯净：只保留最小 AOEM 寄宿集合。

## 设计原则

1. 单一执行入口：迁入后的宿主只调用 `novovm-exec`。
2. 运行时内核寄宿：AOEM 以 DLL + manifest + header 形式寄宿。
3. 口径隔离：吞吐测试口径与功能正确性口径分开，不混写。
4. 渐进迁入：先迁主路径，再迁周边模块，最后退役 SVM2026 对应路径。

## 本次设计输出

- 新增门面 crate：`crates/novovm-exec`
  - `AoemExecFacade::open(...)`
  - `AoemExecFacade::create_session(...)`
  - `AoemExecSession::execute_ops_v2(...)`
  - `AoemExecSession::submit_ops(...)`
