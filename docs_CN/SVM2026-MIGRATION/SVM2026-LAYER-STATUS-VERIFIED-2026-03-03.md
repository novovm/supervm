# SVM2026 分层状态核验（共识/网络）- 2026-03-03

## 1. 核验范围

- 共识层：`D:\WorksArea\SVM2026\supervm-consensus`
- 网络层（轻量抽象）：`D:\WorksArea\SVM2026\supervm-network`
- 网络层（L4 主实现）：`D:\WorksArea\SVM2026\src\l4-network`

## 2. 核验方法

1. 代码结构审计（模块/依赖/TODO）。
2. 定向构建与测试（`cargo check` / `cargo test`）。
3. 与 `ROADMAP.md` 叙事交叉对比。

## 3. 结果结论（用于迁移基线）

## 3.1 共识层：按生产迁移口径，判定约 80%

证据：

- `supervm-consensus` 可独立通过构建：`cargo check -p supervm-consensus` ✅
- 单元测试与文档测试通过：`13 + 2` tests ✅
- 仍存在关键收口项（批量签名验证 TODO）：
  - `D:\WorksArea\SVM2026\supervm-consensus\src\quorum_cert.rs:142`

判定：

- 共识核心逻辑可运行，但距离“生产闭环”仍有收口工作。
- 用迁移口径记为 `~80%` 是合理的。

## 3.2 网络层：核心功能完成，可判“基本完成”；生产口径建议 90~95%

证据：

- 轻量网络抽象可构建+测试通过：
  - `cargo check -p supervm-network` ✅
  - `cargo test -p supervm-network`（2 tests）✅
- L4 主实现可构建：
  - `cargo check -p l4-network` ✅
- L4 主实现测试主体通过：
  - `cargo test -p l4-network` 中单元+集成测试合计 `71` 项通过（含 1 ignored）✅
- 但存在文档测试失败（非运行时主逻辑）：
  - `src/l4-network/src/storage.rs` 文档块解析失败（3 个 doctest fail）
- 仍有外部存储与环境探测 TODO：
  - `D:\WorksArea\SVM2026\src\l4-network\src\storage.rs:159`
  - `D:\WorksArea\SVM2026\src\l4-network\src\storage.rs:205`
  - `D:\WorksArea\SVM2026\src\l4-network\src\node_tier.rs:76`

判定：

- 你说“网络层好像完成了”在“功能主线可用”层面成立。
- 若按生产封盘标准，建议标记为 `90~95%`，不是严格 `100%`。

## 4. 对 NOVOVM 迁移的直接影响

1. 共识层进入 `P1 迁移优先`（先接入，再补收口）。
2. 网络层可作为 `先迁模块`，但需在迁移时同步修复 doctest 与 TODO 点。
3. 不再使用 `ROADMAP.md` 单一百分比做决策，改用“构建+测试+TODO”三证据口径。

## 5. 立即执行（已同意推进）

1. 在迁移台账中将共识标注为 `~80%`，网络标注为 `核心完成/生产待收口`。
2. 将 `l4-network` 的 doctest 修复列为网络迁移前置项。
3. 将 `quorum_cert` 的批量验证补齐列为共识迁移前置项。
