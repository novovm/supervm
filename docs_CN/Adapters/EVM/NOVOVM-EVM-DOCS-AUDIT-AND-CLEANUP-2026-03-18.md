# NOVOVM EVM 文档逐行审计与清理报告（2026-03-18）

## 1. 审计范围

- 目录：`docs_CN/Adapters/EVM`
- 文件总数：`17`
- 审计目标：
1. 标记“当前有效”与“历史归档”
2. 修正会误导开源用户的过时描述
3. 保留可追溯历史，但禁止历史文档充当当前发布依据

## 2. 清理动作（已执行）

1. 重写索引文档 [README.md](README.md)：
   - 新增“当前有效文档”清单
   - 新增“历史归档文档”清单
   - 新增一键闭环自检命令
2. 更新闭环目标文档 [NOVOVM-EVM-FULL-LIFECYCLE-CLOSURE-TARGET-2026-03-17.md](NOVOVM-EVM-FULL-LIFECYCLE-CLOSURE-TARGET-2026-03-17.md)：
   - 补充 2026-03-18 最新 step1~step5 证据状态
3. 修正过时实现描述 [NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md](NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md)：
   - 删除“`eth_getCode/eth_getStorageAt` 仍是 M0 占位返回”的过时说法
4. 更新运行手册 [NOVOVM-EVM-PLUGIN-CONFIG-SETUP-USAGE-2026-03-16.md](NOVOVM-EVM-PLUGIN-CONFIG-SETUP-USAGE-2026-03-16.md)：
   - 去除本机绝对路径示例
   - 增加多机器“方法缺失=二进制版本不一致”的前置校验
5. 为以下历史文档增加归档警示头（防误用）：
   - `archive/NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`
   - `archive/NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md`
   - `archive/NOVOVM-EVM-FULL-MIRROR-100P-CLOSURE-CHECKLIST-2026-03-13.md`
   - `archive/NOVOVM-EVM-NATIVE-PROTOCOL-COMPAT-PROGRESS-2026-03-16.md`
   - `archive/NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md`
   - `archive/NOVOVM-EVM-UPSTREAM-REQUIRED-CAPABILITY-CHECKLIST-2026-03-11.md`

## 3. 文档状态清单（逐文件）

| 文档 | 状态 | 处理结论 |
|---|---|---|
| `README.md` | 当前有效 | 已改为“有效/归档”双区索引 |
| `NOVOVM-EVM-FULL-LIFECYCLE-CLOSURE-TARGET-2026-03-17.md` | 当前有效 | 已更新最新闭环证据状态 |
| `NOVOVM-EVM-PLUGIN-CONFIG-SETUP-USAGE-2026-03-16.md` | 当前有效 | 已补版本一致性检查并去本机路径 |
| `NOVOVM-EVM-PLUGIN-BOUNDARY-IRON-LAWS-2026-03-13.md` | 当前有效 | 保留 |
| `NOVOVM-EVM-FULL-MIRROR-NODE-MODE-SPEC-2026-03-11.md` | 当前有效 | 保留 |
| `NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md` | 当前有效 | 已修正过时描述 |
| `NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md` | 当前有效（规范） | 保留 |
| `NOVOVM-ATOMIC-ORCHESTRATION-LAYER-SPEC-2026-03-06.md` | 当前有效（规范） | 保留 |
| `NOVOVM-WEB30-EVM-SEMANTIC-MAPPING-MATRIX-2026-03-06.md` | 当前有效（规范） | 保留 |
| `NOVOVM-ETHEREUM-PROFILE-2026-COMPAT-BASELINE-2026-03-06.md` | 当前有效（基线） | 保留 |
| `archive/NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md` | 历史归档 | 已加归档警示头 |
| `archive/NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md` | 历史归档 | 已加归档警示头 |
| `archive/NOVOVM-EVM-FULL-MIRROR-100P-CLOSURE-CHECKLIST-2026-03-13.md` | 历史归档 | 已加归档警示头 |
| `archive/NOVOVM-EVM-NATIVE-PROTOCOL-COMPAT-PROGRESS-2026-03-16.md` | 历史归档 | 已加归档警示头 |
| `archive/NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md` | 历史归档 | 已加归档警示头 |
| `archive/NOVOVM-EVM-UPSTREAM-REQUIRED-CAPABILITY-CHECKLIST-2026-03-11.md` | 历史归档 | 已加归档警示头 |
| `archive/NOVOVM-EVM-ADAPTER-STRICT-V2-MERGE-CHECKLIST-2026-03-07.md` | 历史归档 | 文件本身已标注“已归档” |

## 4. 开源发布建议（后续可选）

1. 若要进一步减少噪音，可把“历史归档文档”迁移到 `docs_CN/Adapters/EVM/archive/`。
2. 对归档文档中的本机绝对路径（`D:\...`、`/Users/...`）统一替换为占位路径。
3. 在 CI 增加文档守卫：新文档若包含“当前状态=100%”必须附当日证据路径。
