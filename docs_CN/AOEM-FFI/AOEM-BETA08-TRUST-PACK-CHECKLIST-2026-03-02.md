# AOEM Beta0.8 闭源可验证发布清单（2026-03-02）

## 结论（先回答你的问题）

- 对外发布：继续使用 `DLL/so/dylib`（标准交付形态）。
- 内部研发：保留 `Rust crate` 直连（性能基线与调优主线）。
- 运营策略：双路径并存，发布以 DLL 为准，性能对照以 crate 基线为准。

## 1. 产物清单（Release Pack）

每个平台发布一个目录：

- `bin/`：
  - Windows: `aoem_ffi.dll`
  - Linux: `libaoem_ffi.so`
  - macOS: `libaoem_ffi.dylib`
- `include/aoem.h`（ABI 头文件）
- `INSTALL-INFO.txt`（安装与加载说明）
- `CAPABILITIES.json`（`aoem_capabilities_json` 固化输出）
- `VERSION.txt`（语义版本 + ABI 版本 + 构建时间）
- `SHA256SUMS`（全部文件哈希）
- `SIGNATURE.txt`（签名信息/证书指纹）
- `SBOM.spdx.json`（供应链清单）
- `LICENSE-NOTICE.txt`（许可证与第三方声明）

## 2. 最小信任链（必须具备）

1. 哈希校验：宿主启动前校验 `SHA256`。
2. 代码签名：校验证书指纹与发行方。
3. ABI 校验：`aoem_abi_version()==1`，不匹配拒绝加载。
4. 能力校验：`aoem_capabilities_json` 与宿主期望一致。
5. 版本钉死：SuperVM 仅接受白名单版本。

## 3. SuperVM 启动校验顺序（建议固定）

1. 读取 AOEM 动态库路径。
2. 校验文件哈希（本地 manifest）。
3. 加载动态库。
4. 读取 `aoem_abi_version` / `aoem_version_string`。
5. 拉取 `aoem_capabilities_json`。
6. 与节点配置进行兼容性匹配。
7. 通过后注册执行引擎。

## 4. 基准与封盘口径（防误导）

必须同时公布两条线：

- `native_baseline`：AOEM crate 进程内基线（研发与回归用）。
- `ffi_release_baseline`：AOEM DLL 宿主口径（发布与运维用）。

禁止混写：

- 不允许把 crate 基线直接写成 DLL KPI。
- 不允许把批量吞吐与单笔吞吐混成一个数。

## 5. 安全与运维承诺（beta0.8）

- 漏洞响应窗口：`<=72h`（确认与分级）。
- 热修复发布窗口：`<=7d`（高危）。
- 回滚策略：保留最近两个稳定 DLL 版本。
- 崩溃取证：导出 `aoem_last_error` + 宿主日志 + 版本签名信息。

## 6. 你当前项目的执行策略（落地）

- 对外：只发 DLL 包（闭源）。
- 对内：AOEM 团队继续用 crate 做性能优化与回归。
- SuperVM：默认加载 DLL；开发环境可开 `native compare` 仅用于对照，不进生产。

## 7. Beta0.8 发布闸门（Go/No-Go）

满足以下全部条件才发布：

1. 三平台产物齐全（Win/Linux/macOS）。
2. 哈希与签名校验脚本通过。
3. ABI/Capabilities 兼容矩阵通过。
4. `ffi_release_baseline` 文档已封盘并可复现实测。
5. 回滚版本已就绪。
