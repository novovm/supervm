# NOVOVM Unified Account Gate Matrix v1（统一账户门禁矩阵）- 2026-03-06

## 1. 目标

定义统一账户迁移门禁的最小可执行矩阵，确保每个 gate 都具备：

- 输入样本（Input）
- 预期结果（Expected Output）
- 失败阻断级别（Failure Level）
- 证据落盘路径（Evidence Path）

---

## 2. 失败级别定义

| Level | 含义 |
|---|---|
| `BlockMerge` | 阻断合并 |
| `BlockRelease` | 阻断发布，但允许继续开发 |
| `Warn` | 预警，不阻断 |

---

## 3. Gate 用例矩阵

| Case ID | Gate | Input | Expected Output | Failure Level | Evidence Path |
|---|---|---|---|---|---|
| UA-G01 | `ua_mapping_signal` | 新增 `UCA-A` 绑定 `evm:1:0xabc...` | 绑定成功，事件 `binding_added` | BlockMerge | `artifacts/migration/unifiedaccount/ua_mapping/UA-G01.json` |
| UA-G02 | `ua_mapping_signal` | `UCA-B` 绑定已被 `UCA-A` 占用的同一 Persona 地址 | 请求拒绝，事件 `binding_conflict_rejected` | BlockMerge | `artifacts/migration/unifiedaccount/ua_mapping/UA-G02.json` |
| UA-G03 | `ua_mapping_signal` | 撤销后冷却期内重绑同地址 | 请求拒绝，返回冷却错误码 | BlockMerge | `artifacts/migration/unifiedaccount/ua_mapping/UA-G03.json` |
| UA-G04 | `ua_signature_domain_signal` | 同一 payload 使用 `eth_sign` 签名后在 `web30_sign` 域验签 | 验签失败，事件 `domain_mismatch_rejected` | BlockMerge | `artifacts/migration/unifiedaccount/ua_signature/UA-G04.json` |
| UA-G05 | `ua_signature_domain_signal` | `typed_data(eip712)` 在错误 `chain_id` 域验签 | 验签失败 | BlockMerge | `artifacts/migration/unifiedaccount/ua_signature/UA-G05.json` |
| UA-G06 | `ua_nonce_replay_signal` | 同一 Persona 连续提交相同 nonce 两次 | 第二次拒绝，事件 `nonce_replay_rejected` | BlockMerge | `artifacts/migration/unifiedaccount/ua_nonce/UA-G06.json` |
| UA-G07 | `ua_nonce_replay_signal` | 非单调 nonce（倒序） | 请求拒绝，错误码固定 | BlockMerge | `artifacts/migration/unifiedaccount/ua_nonce/UA-G07.json` |
| UA-G08 | `ua_permission_signal` | `Delegate` 尝试修改 `AccountPolicy` | 拒绝，事件 `permission_denied` | BlockMerge | `artifacts/migration/unifiedaccount/ua_permission/UA-G08.json` |
| UA-G09 | `ua_permission_signal` | 过期 `SessionKey` 发起交易 | 拒绝并返回过期错误 | BlockMerge | `artifacts/migration/unifiedaccount/ua_permission/UA-G09.json` |
| UA-G10 | `ua_persona_boundary_signal` | `eth_sendRawTransaction` 请求跨链原子操作字段 | 显式拒绝或返回 `web30_*` 路由提示 | BlockRelease | `artifacts/migration/unifiedaccount/ua_boundary/UA-G10.json` |
| UA-G11 | `ua_persona_boundary_signal` | `web30_*` 请求正常单链转账 | 正常执行，不污染 `eth_*` 路径 | BlockRelease | `artifacts/migration/unifiedaccount/ua_boundary/UA-G11.json` |
| UA-G12 | `ua_type4_policy_signal` | Type 4（7702）交易输入（支持模式） | 通过校验并按策略执行 | BlockRelease | `artifacts/migration/unifiedaccount/ua_type4/UA-G12.json` |
| UA-G13 | `ua_type4_policy_signal` | Type 4（7702）交易输入（拒绝模式） | 固定错误码拒绝，事件记录 | BlockRelease | `artifacts/migration/unifiedaccount/ua_type4/UA-G13.json` |
| UA-G14 | `ua_type4_policy_signal` | Type 4 与 SessionKey 混用禁用策略 | 固定错误码拒绝 | BlockRelease | `artifacts/migration/unifiedaccount/ua_type4/UA-G14.json` |
| UA-G15 | `ua_uniqueness_conflict_signal` | 系统扫描发现历史脏数据：同 Persona 地址关联多个 UCA | 进入冲突修复流程，阻断上线 | BlockMerge | `artifacts/migration/unifiedaccount/ua_uniqueness/UA-G15.json` |
| UA-G16 | `ua_recovery_revocation_signal` | 主密钥轮换与地址撤销组合流程 | 状态机正确迁移并记录 `key_rotated/binding_revoked` | Warn | `artifacts/migration/unifiedaccount/ua_recovery/UA-G16.json` |

---

## 4. 通过条件

1. `BlockMerge` 用例 100% 通过。
2. `BlockRelease` 用例 100% 通过。
3. `Warn` 用例至少具备事件与可追溯证据。
4. 全部证据文件路径可被自动汇总脚本扫描。

---

## 5. 与其他文档关系

- 规范来源：`NOVOVM-UNIFIED-ACCOUNT-SPEC-v1-2026-03-06.md`
- 方案来源：`NOVOVM-UNIFIED-ACCOUNT-MIGRATION-PLAN-AND-IMPLEMENTATION-STEPS-2026-03-06.md`
- 台账联动：`NOVOVM-UNIFIED-ACCOUNT-MIGRATION-LEDGER-2026-03-06.md`
