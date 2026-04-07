# NOVOVM 固定策略程序 Rust 迁移与封盘计划（2026-04-05）

## 1. 目标

把长期稳定、跨平台必需、直接影响生产路径的策略逻辑从 `.ps1` 迁到 Rust；脚本只保留编排入口与参数透传。

## 2. 当前已完成（P0）

1. `novovm-overlay-auto-profile` 已 Rust 化：共享实现位于 `crates/novovm-rollout-policy/src/policy/overlay/auto_profile.rs`，兼容包装位于 `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-auto-profile.rs`。  
2. 生命周期链路已打通：`rollout-control -> rollout -> lifecycle -> novovm-up` 可透传 Auto Profile 参数。  
3. 默认仍关闭自动切换，避免对现网默认行为造成变更。

## 3. 迁移顺序（按生产价值）

1. `overlay relay discovery merge`（从 `.ps1` 迁 Rust 二进制）。  
2. `relay health refresh` 已迁入统一共享实现：`crates/novovm-rollout-policy/src/policy/overlay/relay_health_refresh.rs`，兼容包装位于 `crates/novovm-rollout-policy/src/bin/overlay/novovm-overlay-relay-health-refresh.rs`。  
3. `seed/region failover policy` 已迁入统一共享实现：`crates/novovm-rollout-policy/src/policy/failover/seed_evaluate.rs`、`crates/novovm-rollout-policy/src/policy/failover/region_evaluate.rs`，并由统一入口 `novovm-rollout-policy failover ...` 直接调用。  
4. `risk slo/circuit-breaker` 已迁入统一共享实现：`crates/novovm-rollout-policy/src/policy/risk/slo_evaluate.rs`、`crates/novovm-rollout-policy/src/policy/risk/circuit_breaker_evaluate.rs`。
5. 控制面 `Apply-ReplicaSloPolicy` 已优先调用统一 risk CLI，脚本内置 SLO / circuit-breaker 规则退化为兜底路径。
6. `risk action-eval / level-set` 已迁入统一共享实现，旧独立二进制退化为兼容薄壳。
7. `risk action-matrix-build / matrix-select / blocked-select / blocked-map-build / policy-profile-select` 已迁入统一共享实现，旧独立二进制退化为兼容薄壳。
8. `rollout control` 中与网络策略强相关的其他核心决策段（逐段迁 Rust）。
9. 控制面 `Resolve-RiskActionMatrix` 本地完整矩阵重建已削为 emergency fallback，Rust 不可用时只保留保守全局基线。
10. 控制面 `Resolve-RiskPolicyProfileSelection` 已优先走统一 risk CLI，脚本仅保留 `active_profile/policy_profiles` 查表兜底。
11. 控制面 `Apply-ReplicaSloPolicy` 本地完整 SLO / circuit 判定已削为 emergency fallback，Rust 不可用时只保留按 `grade` 的保守默认动作。
12. 控制面 `Select-RiskActionMatrix / Resolve-RiskBlockedSetMap / Select-RiskBlockedSet` 已削为保守默认 fallback，脚本不再保留 site/region 分层选择与 blocked 覆盖映射重建逻辑。
13. 控制面 `Resolve-RolloutDecisionAlertLevel / AlertChannel / AlertTarget / DeliveryType / DeliveryEndpoint` 已削为保守默认 fallback，脚本不再保留角色级告警路由推导，只保留 `ops-observe/ops-oncall` 两档与 endpoint 地址簿查表。
14. `rollout decision-route / decision-delivery / decision-dashboard-export / decision-dashboard-consumer` 已迁入统一共享实现：`crates/novovm-rollout-policy/src/policy/rollout/*`，统一入口直接执行共享模块，旧独立二进制退化为兼容薄壳。
15. 控制面 `Send-RolloutDecisionDelivery` 本地投递已削为 emergency fallback：Rust 不可用时仅保留显式 http(s) endpoint 的 webhook/im 保守投递，不再在 PowerShell 内执行本地 SMTP 邮件发送。
16. `failover-policy-matrix-build` 已迁入统一共享实现：`crates/novovm-rollout-policy/src/policy/failover/policy_matrix_build.rs`，旧独立二进制退化为兼容薄壳。
17. legacy 平铺 tool 名兼容分发已完成最终收口：统一入口内部直接路由到共享模块，不再经 sibling bin 中转；`commands/shared.rs` 已退役。

## 4. 封盘原则

1. Rust 程序上线后，原 `.ps1` 逻辑冻结，不再增加新策略分支。  
2. `.ps1` 仅保留三类职责：参数装配、进程拉起、故障退出码透传。  
3. 新策略只进 Rust，不再回写到脚本主逻辑。  
4. 任何跨平台能力以 Rust 二进制为准，脚本作为兼容壳。
5. 控制面 fallback 只允许保留保守默认与简单查表，不再在 PowerShell 内重建完整风险矩阵或 profile 选择逻辑。
6. 控制面 fallback 不再允许保留完整窗口评分与 score 命中链；正常主路径的风险判定只由 Rust 内核完成。
7. 控制面 fallback 不再允许保留 site/region 分层风险矩阵选择、blocked set 映射构建或 blocked set 选择逻辑；这类规则只由 Rust 内核主导。
8. 控制面 fallback 不再允许保留角色级 rollout 告警路由推导；正常主路径的 alert level/channel/target/delivery 只由 Rust `decision-route` 主导，PowerShell 仅保留 `ops-observe/ops-oncall` 保守兜底。
9. 控制面 fallback 不再允许保留本地 SMTP 邮件发送或复杂投递类型分支；正常主路径的真实投递只由 Rust `decision-delivery` 主导，PowerShell 仅保留显式 endpoint 的 webhook/im 保守兜底。
10. legacy 平铺 tool 名只允许作为兼容入口存在，内部必须直接进入统一共享模块；不得再经第二层 bin 中转或保留第二套真实逻辑。

## 5. 运行口径

1. 生产默认路径：优先调用发布目录 Rust 二进制。  
2. 开发回退路径：二进制不存在时允许 `cargo run`。  
3. 审计字段与状态文件保持向后兼容，不改现有主线字段名。

## 2026-04-06 PS1 seal update

- `scripts/novovm-rollout-decision-dashboard-export.ps1` and `scripts/novovm-rollout-decision-dashboard-consumer.ps1` are sealed as thin compatibility wrappers; normal main path must go through `novovm-rollout-policy rollout ...`.
- `scripts/novovm-overlay-relay-health-refresh.ps1` and `scripts/novovm-overlay-relay-discovery-merge.ps1` are sealed as thin compatibility wrappers; normal main path must go through `novovm-rollout-policy overlay ...`.
- `docs_CN/NOVOVM-PS1-INVENTORY-AND-MIGRATION-CUTLIST-2026-04-06.md` is the current PowerShell cutlist baseline: keep shells, thin wrappers, and frozen migration scripts are separated explicitly.

## 2026-04-06 batch cleanup seal

- Cleanup execution mode is now batch-based rather than single-file based.
- Root scripts with confirmed Rust parity should be converted by class into compatibility shells.
- Legacy `src/bin/*` policy tools are compatibility surfaces only; they are tracked in `docs_CN/NOVOVM-LEGACY-BIN-RETIREMENT-AUDIT-2026-04-06.md` and should be deleted only by audited batch.
- `scripts/migration/*` is explicitly frozen as a history asset pool and is excluded from the current mainline strategy-core cleanup unless reactivated into the production path.

## 2026-04-07 unified-CLI defaulting

- `novovm-node-rollout-control.ps1` now auto-discovers the unified `novovm-rollout-policy` binary by default; explicit `policy_cli.binary_file` is no longer required to move the normal path onto the unified Rust core.
- Overlay relay discovery and relay health refresh now prefer the unified CLI before legacy dedicated binaries.
- `novovm-up.ps1` overlay auto-profile selection now prefers the unified CLI and only falls back to the legacy dedicated binary for compatibility.
- Early rollout-control resolution paths (`risk-policy-profile-select`, `risk-level-set`) now default to the unified `novovm-rollout-policy` binary through `Resolve-DefaultRolloutPolicyCliBinaryPath`; legacy dedicated binaries are no longer the first implicit choice in early config hydration.
- Remaining rollout-control legacy helper defaults are now resolved through a single `Resolve-PolicyToolBinaryConfig` path; repeated hard-coded legacy release/debug fallback blocks have been removed from the normal config hydration path.
- `docs_CN/NOVOVM-LEGACY-BIN-RETIREMENT-AUDIT-2026-04-06.md` now carries Batch 1 physical deletion candidates for current compatibility-only legacy bins.
- Batch 1 physical retirement is complete: compatibility-only per-tool wrapper bins under `crates/novovm-rollout-policy/src/bin/*` have been removed. The only supported compatibility surface is the unified `novovm-rollout-policy` entrypoint with flat legacy tool-name dispatch.
- Implicit legacy dedicated-binary auto-search has been removed from default helper resolution. If the unified CLI is absent, the path is now treated as missing-default rather than silently preferring deleted wrapper executables.
- `novovm-up.ps1` overlay auto-profile selection no longer auto-discovers the legacy dedicated executable by default.

## 2026-04-07 final seal-audit baseline

- `docs_CN/NOVOVM-UNIFIED-POLICY-CORE-SEAL-AUDIT-2026-04-07.md` is the current seal baseline for the unified policy core.
- Mainline is defined as a three-layer model only: unified Rust normal path, explicit compatibility path, and minimal emergency fallback path.
- Any future strategy extension must enter the shared Rust core first and must not reintroduce hidden script logic or hidden per-tool executable defaults.

## 2026-04-07 收官声明

本轮统一 Rust 策略内核迁移，按主线能力建设口径正式收官。

收官后的固定制度如下：

1. 正常主路径必须走统一 Rust 内核 `novovm-rollout-policy`。
2. 兼容路径仅允许显式存在，不得重新获得默认执行权。
3. PowerShell 仅保留启动、运维、审计与最小保守 fallback，不得重新承载第二套完整策略逻辑。
4. 后续新增策略能力必须先进共享 Rust 模块，不得先落脚本主逻辑。

本轮后续工作统一视为：

- cleanup
- compatibility surface compression
- history asset isolation
- technical debt recovery
- real production validation

这些事项属于收官后工作，不再改变本轮主线已封盘的结论。

## 2026-04-07 next-phase shell migration bootstrap

- A new cross-platform operations-shell crate `crates/novovmctl` has been introduced as the next-phase mainline target after unified policy-core seal.
- `novovmctl` is explicitly constrained to shell responsibilities only: path discovery, config hydration, process launch, and Rust-to-Rust orchestration. It must not reintroduce a second strategy brain.
- Phase 1 scope is intentionally narrow: `novovmctl up` and `novovmctl rollout-control` are bootstrapped first; `rollout`, `lifecycle`, and `daemon` remain placeholders until the primary entry-path migration is stable.
- The intended steady-state architecture is now fixed as: `novovmctl` (runtime shell) -> `novovm-rollout-policy` (policy core) -> `novovm-node` (execution body).
- `scripts/novovm-up.ps1` and `scripts/novovm-node-rollout-control.ps1` now enter mainline through a strict `novovmctl` bridge first; unsupported legacy-only parameters are rejected instead of silently reactivating the old PowerShell execution brain.
- `novovmctl` now carries a first-pass unified output/audit layer for `up` and `rollout-control`: terminal summaries plus JSONL audit envelopes are fixed early so the runtime-shell migration does not create a second logging dialect.
- The first batch of `novovm-rollout-policy` commands consumed by the runtime shell now emit a stable success-envelope with explicit `domain/action` metadata. `novovmctl` has been updated to parse the enveloped `data` payload instead of depending on raw ad hoc JSON bodies.

## 17. 2026-04-07 `novovmctl rollout-control` contract-alignment

- Added queue-driven adapter command `rollout controller-dispatch-evaluate` in `novovm-rollout-policy` to remove the last nonexistent rollout-control input contract.
- Switched `novovmctl rollout-control` from fake `--queue-file` passthrough into real queue hydration:
  - SLO inputs now come from `state_recovery.slo.*`
  - circuit-breaker inputs now come from `state_recovery.slo.circuit_breaker.*` plus top-level queue concurrency/pause defaults
  - policy-profile selection now consumes serialized `risk_policy` plus `active_profile`
- This closes the main `rollout-control` input mismatch between `novovmctl` and `novovm-rollout-policy`; `up` parameter-surface expansion remains the next shell-layer task.

## 18. 2026-04-07 `novovmctl up` parameter-surface widen

- Aligned `novovmctl up -> overlay auto-profile-select` with the real policy CLI contract (`current-profile`, state/timing/profile-set knobs), removing the last fake auto-profile input pair on the startup path.
- Extended `novovm-up.ps1` compatibility bridge to forward the auto-profile state/tuning surface plus `AutoProfileBinaryPath -> policy-cli-binary-file`.
- Fixed the strict-mode shell bug where `DryRun` was referenced without a declared parameter.

## 19. 2026-04-07 `novovmctl daemon` subset cutover

- Added first usable `novovmctl daemon` path: restart-shell + `up` warmup reuse + node watch env propagation.
- Converted `novovm-prod-daemon.ps1` into a strict compatibility shell forwarding to `novovmctl daemon`.
- Explicitly scoped out non-migrated legacy daemon behaviors (`NoGateway`, build orchestration, gateway bind/spool tuning, reconcile path) instead of silently keeping fake support.

## 20. 2026-04-07 `novovmctl lifecycle` subset cutover

- Added first lifecycle-shell subset in Rust: `status`, `set-runtime`, `set-policy`.
- Converted `novovm-node-lifecycle.ps1` into a strict compatibility shell forwarding to `novovmctl lifecycle`.
- Explicitly refused non-migrated lifecycle actions (`start/stop/register/upgrade/rollback`) instead of silently preserving old PowerShell logic.

## 21. 2026-04-07 `novovmctl rollout` subset cutover

- Added first usable `novovmctl rollout` subset with `status` plan inspection.
- Converted `novovm-node-rollout.ps1` into a strict compatibility shell forwarding to `novovmctl rollout`.
- Explicitly scoped out non-migrated orchestration actions (`upgrade`, `rollback`, `set-policy`) instead of silently preserving old PowerShell rollout orchestration.

## 22. 2026-04-07 `novovmctl` phase-1 seal

- Phase 1 is now closed as the cross-platform mainline shell cutover milestone.
- Scope counted as completed in this phase:
  - `up` on Rust mainline shell
  - `rollout-control` on Rust mainline shell
  - `lifecycle` first usable subset (`status / set-runtime / set-policy`)
  - `daemon` first usable subset (`node-only + watch`)
  - `rollout` first usable subset (`status` plan-state shell)
- Scope explicitly deferred to phase 2 and no longer treated as part of this cutover:
  - gateway shell migration
  - reconcile shell migration
  - build orchestration shell migration
  - remote lifecycle / upgrade / rollback orchestration migration
- PowerShell remains only as strict compatibility shells on the mainline path for this phase.

## 23. 2026-04-07 phase-2 initiation: full shell Rust-ification

- Standard completion wording for phase 1 is fixed as:
  - `up`: mainline Rust shell
  - `rollout-control`: mainline Rust shell
  - `lifecycle / daemon / rollout`: first usable subset only, not full shell migration
- Phase 2 is now opened as a separate mainline:
  - `Phase 2-A`: full `lifecycle` Rust shell
  - `Phase 2-B`: full `rollout` Rust shell
  - `Phase 2-C`: full `daemon` Rust shell
- Phase 2 will not mix in:
  - migration/history scripts
  - AOEM build/package scripts
  - non-mainline helper assets
- Phase 2 execution rule is changed from per-action patching to per-command full-package migration.

## 24. 2026-04-07 phase-2 boundary hardening

- Phase 2 mainline scope is limited to full Rust shell takeover for exactly three command domains:
  - `Phase 2-A`: `lifecycle`
  - `Phase 2-B`: `rollout`
  - `Phase 2-C`: `daemon`
- The following lines are explicitly outside phase-2 capability scope:
  - validation line: build validation, integration smoke, dry-run/non-dry-run comparison, compatibility-shell forwarding checks
  - historical/auxiliary shell line: `gateway`, `reconcile`, `build orchestration`, unless later proven to block the production mainline
  - physical retirement line: post-migration dependency audit and final deletion timing for legacy `ps1` shells
- Phase 2 therefore targets shell takeover only; it does not implicitly include validation closure, historical asset cleanup, or physical shell deletion.

## 25. 2026-04-07 phase-2-A package kickoff: full `lifecycle` Rust shell

- `Phase 2-A` is opened as a package migration, not an action-by-action patch stream.
- Target state:
  - `novovmctl lifecycle` fully owns mainline lifecycle behavior
  - `scripts/novovm-node-lifecycle.ps1` becomes a pure compatibility shell
- Required action set for this package:
  - `status`
  - `set-runtime`
  - `set-policy`
  - `start`
  - `stop`
  - `register`
  - `upgrade`
  - `rollback`
- Fixed internal work packages:
  - CLI parameter-surface parity
  - process/state takeover for `start/stop/status`
  - governance/registration takeover for `register/set-runtime/set-policy`
  - lifecycle action takeover for `upgrade/rollback`
  - unified output/audit reuse
  - final shell compression for `novovm-node-lifecycle.ps1`
- Exit criteria for `Phase 2-A`:
  - no remaining mainline lifecycle action depends on PowerShell logic
  - `novovm-node-lifecycle.ps1` only forwards to `novovmctl lifecycle`
  - package sealed before opening `Phase 2-B`

## 26. 2026-04-07 phase-2-A validation closure and handoff to phase-2-B

- `Phase 2-A lifecycle` now has a completed minimum validation loop:
  - `register`
  - `start`
  - `status`
  - `stop`
- The lifecycle start path no longer fails on empty managed ingress input; pre-spawn bootstrap now seeds a minimal `.opsw1` manifest and the node survives the former `NOVOVM_OPS_WIRE_DIR has no .opsw1 files` guard.
- Compatibility-shell validation is also closed for this package:
  - `scripts/novovm-node-lifecycle.ps1` can discover real `novovmctl` build outputs
  - mainline lifecycle parameters are forwarded through the shell bridge
  - out-of-scope legacy parameters continue to fail explicitly
- Clean shutdown is confirmed:
  - process stopped
  - `pid_file` removed
  - subsequent `status` reports `running=false` and `pid=null`
- The earlier apparent stale `pid_file` was a validation-observation artifact caused by a parallel check, not a real lifecycle cleanup defect; no extra cleanup patch is required for package closure.
- `Phase 2-A` can therefore be treated as closed on the minimum mainline validation track, and the next package is now `Phase 2-B rollout`.

## Phase 2-B rollout validation closure note (2026-04-07)

- `novovmctl rollout` now compiles and the compat shell `scripts/novovm-node-rollout.ps1` forwards into Rust without falling back to legacy PowerShell rollout orchestration.
- `status` and `set-policy` passed on the full validation plan fixture.
- `upgrade --dry-run` and `rollback --dry-run` passed on the canary-only rollout validation fixture after batch seeding release/state prerequisites.
- JSON terminal envelope and JSONL audit remained unified across `rollout -> lifecycle`.
- `dry-run` left the lifecycle state hash unchanged for both `upgrade` and `rollback` validation runs.
- The remaining full multi-group gate/state interaction was classified as fixture complexity, not a Rust shell linkage defect.
- Phase 2-B is therefore considered closed for minimum mainline Rust-shell validation.

## Phase 2-C daemon validation closure note (2026-04-07)

- `novovmctl daemon` compiles and the compat shell `scripts/novovm-prod-daemon.ps1` forwards into Rust without falling back to legacy PowerShell daemon logic.
- Minimum validation chain passed:
  - `daemon --dry-run`
  - `daemon --build-before-run --dry-run`
  - `daemon --use-node-watch-mode --lean-io --dry-run`
- `build-before-run` executed through the Rust daemon path and completed successfully when invoked via the release `novovmctl` binary, avoiding the Windows self-lock on the debug executable.
- `dry-run` left lifecycle state unchanged; the lifecycle state SHA256 hash remained stable before and after the daemon validation chain.
- Watch/spool preparation passed in Rust daemon dry-run validation:
  - spool dir created
  - `ops_wire_dir` recorded
  - `ops_wire_watch_drop_failed=true` under `lean_io`
  - `done/failed` directories intentionally omitted under `lean_io`
- JSON terminal envelope and JSONL audit remained unified.
- Phase 2-C is therefore considered closed for minimum mainline Rust-shell validation.
- With Phase 2-A, Phase 2-B, and Phase 2-C all closed, the second-stage mainline Rust shell migration is considered sealed.

## Final closure reference (2026-04-07)

Final closure summary:
- Phase 1 unified Rust policy core: closed
- Phase 2 mainline Rust shell migration: closed
- Formal final summary document:
  - `docs_CN/NOVOVM-RUST-MIGRATION-FINAL-CLOSURE-2026-04-07.md`
