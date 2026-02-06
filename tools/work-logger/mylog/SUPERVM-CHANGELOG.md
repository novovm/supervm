# SuperVM 修改日志

> 记录每次对话结束后的修改、创建内容  
> 用于追踪版本演进、架构改动、模块变更

---

## 日志表

| 日期 | 时间 | 版本 | 架构层级 | 所属模块 | 属性 | 修改/编辑内容简述 | 结论 | 动了哪些文件 |
|------|------|------|--------|--------|------|-----------------|------|-----------|
| 2026-02-06 | 14:30 | 0.5.0 | L0 (核心) | 许可证 & 文档 | 阶段封盘 | 1. 回滚错误的 Rust 骨架文件（Cargo.toml + 18 个 src/ 下的文件）；2. 保留空目录 docs/；3. 创建 AOEM 专有化许可证分析文档 | ✅ SuperVM 仓库清理完毕，仅保留 LICENSE + README + docs/；AOEM 双许可证方案可行性已验证（推荐方案 A：改 Cargo.toml，改动最小） | <ul><li>✅ 删除：Cargo.toml（workspace）</li><li>✅ 删除：src/{aoem-core,gpu-executor,l2-executor,...}（8 个 crate 的 Cargo.toml + lib.rs）</li><li>✅ 删除：supervm-chainlinker-api/，plugins/evm-linker/</li><li>✅ 创建：tools/work-logger/mylog/</li><li>✅ 创建：docs/temp/AOEM-PROPRIETARY-LICENSING-ANALYSIS-2026-02-06.md</li></ul> |

---

## 说明

- **日期**: YYYY-MM-DD 格式
- **时间**: HH:MM （工作时间戳）
- **版本**: 当前项目版本号（来自 Cargo.toml 或 package.json）
- **架构层级**: L0（核心）/ L1（内核扩展）/ L2-L4（应用层）
- **所属模块**: vm-runtime, aoem-core, gpu-executor, domain-registry, 许可证, 文档等
- **属性**: 
  - `阶段封盘` = 阶段性里程碑完成，代码冻结等待发布
  - `生产封盘` = 生产就绪，已审计，不接受大改动
  - `测试` = 功能测试阶段
  - `实验` = 实验性特性，可能改动大
  - `验证` = 性能/正确性验证中
  - `修复` = bug 修复
  - `文档` = 文档补充/更新
- **修改/编辑内容简述**: 核心改动点，尽量简洁
- **结论**: 本轮对话的结果，是否符合预期、有无遗留问题
- **动了哪些文件**: 列出被新增、修改、删除的文件清单

---

## 快速导航

### 按架构层级分类
- **L0 (Core)**: vm-runtime, aoem-core, aoem-engine, aoem-backend
- **L1 (Extensions)**: gpu-executor, l2-executor, zkvm-executor
- **L2-L3 (Apps)**: domain-registry, defi-core, web3-storage
- **L4 (Network)**: supervm-network, supervm-consensus

### 按模块分类
- **并发控制**: OCC, MVCC, OCCC 相关改动
- **执行引擎**: AOEM-CORE, AOEM-ENGINE, adapters
- **GPU 加速**: aoem-backend-gpu, gpu-executor
- **隐私/ZK**: ring-signature, bulletproofs, zkvm
- **存储**: state-db, rocksdb, storage-backend
- **网络/共识**: network, consensus, p2p
- **许可证**: LICENSE, Cargo.toml license 字段
- **文档**: README, docs/**, 设计文档

### 按属性分类
- **阶段封盘** 的改动 → 通常是里程碑完成
- **生产封盘** 的改动 → 严格审查，最小化改动
- **实验** 的改动 → 可能大改，需要标注特性开关
- **修复** → 通常影响范围小，优先级高

---

## 使用建议

1. **每次对话结束后**，在本表最后一行前插入新行，记录本次改动
2. **修改内容简述** 用 1-3 句话说清楚核心改动
3. **动了哪些文件** 用列表格式，区分增/删/改
4. **结论** 说明是否完成、有无后续 TODO、是否需要下次跟进
5. **如果改动较大**，可以创建独立的 `tools/work-logger/mylog/DETAIL-{日期}.md` 来补充说明

---

**最后更新**: 2026-02-06 14:30

