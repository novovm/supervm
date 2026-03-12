# NOVOVM Unified Account Layer Wiring v1 (2026-03-07)

## Layering

1. Protocol Ingress Layer
   - Responsibility: identify protocol entrance (`eth_*`, `web30_*`, others) and pass normalized route request.
   - Status: done for public RPC ingress + local execution ingress normalization.

2. Account Router Layer (Unified Account Core)
   - Responsibility:
     - UCA lifecycle
     - Persona binding uniqueness
     - signature-domain checks
     - permission checks (`Owner/Delegate/SessionKey`)
     - nonce/replay checks (`persona` default)
     - persona boundary checks (`eth_*` cross-chain atomic reject)
   - Implemented in:
     - `crates/novovm-adapter-api/src/unified_account.rs`

3. Execution/Adapter Layer
   - Responsibility: execute tx after account routing decision.
   - Current adapter integration seam:
     - `crates/novovm-adapter-novovm/src/lib.rs`
     - `NovoVmAdapter` now contains `UnifiedAccountRouter` handle.

## Current Wiring (Done)

- Added unified-account core module and exported API:
  - `UnifiedAccountRouter`
  - `RouteRequest / RouteDecision`
  - `UcaAccount / PersonaBinding / AccountPolicy`
  - audit events and typed errors

- Implemented core constraints:
  - global persona uniqueness (`1 PersonaAddress -> 1 UCA`)
  - conflict rejection event
  - revoke + cooldown + rebind window
  - domain isolation checks (evm/web30)
  - nonce replay protection (policy-driven scope, default persona)
  - role permission checks
  - session-key expiry rejection (`session_expires_at`)
  - eth persona boundary enforcement for cross-chain atomic requests
  - Type4 delegate/session policy gate
  - primary key rotation + audit events (`key_rotated`)

- Added adapter seam:
  - `NovoVmAdapter` now carries a `UnifiedAccountRouter` instance.
  - exposes immutable/mutable accessor for upper-layer wiring.

- Added public RPC ingress wiring:
  - file: `crates/novovm-node/src/bin/novovm-node.rs`
  - methods:
    - `ua_createUca`
    - `ua_rotatePrimaryKey`
    - `ua_setPolicy`
    - `ua_bindPersona`
    - `ua_revokePersona`
    - `ua_getBindingOwner`
    - `ua_getAuditEvents`
    - `ua_route`
    - alias ingress:
      - `eth_sendRawTransaction` (normalized to `ProtocolKind::Eth`)
      - `eth_sendTransaction` (normalized to `ProtocolKind::Eth`)
      - `eth_getTransactionCount` (persona nonce query alias; read-only)
      - `web30_sendTransaction` (normalized to `ProtocolKind::Web30`)
      - `web30_sendRawTransaction` (normalized to `ProtocolKind::Web30`)
  - behavior:
    - all `ua_*` / alias methods are normalized into `RouteRequest` then passed into `UnifiedAccountRouter`.
    - route acceptance uses `RouteDecision::{FastPath, Adapter}`.
    - `eth_getTransactionCount` reuses same persona binding model and nonce scope, returns next usable nonce without mutating router state.
    - route methods support optional `session_expires_at` and enforce expiry for `role=session_key`.

- Added router persistence:
  - store interface:
    - env `NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND` (current default: `rocksdb`)
    - supported backends: `rocksdb` (default) + `bincode_file` (legacy compatibility)
    - path override: `NOVOVM_UNIFIED_ACCOUNT_DB`
    - default path:
      - `rocksdb`: `<chain_query_db_parent>/novovm-unified-account-router.rocksdb`
      - `bincode_file`: `<chain_query_db_parent>/novovm-unified-account-router.bin`
  - storage format:
    - envelope `v1`: `{ version, router, flushed_event_count }`
    - backward compatible with legacy raw `UnifiedAccountRouter` payload
    - rocksdb dedicated column families（已拆分）:
      - state CF: `ua_store_state_v2` -> key `ua_store:state:router:v2`
      - audit CF: `ua_store_audit_v2` -> key `ua_store:audit:flushed_event_count:v1`
      - compatibility:
        - default-CF namespace keys still dual-write（兼容 pre-CF 版本）
        - legacy key `unified_account:snapshot:v1` still dual-write（兼容 rollback）

- Added execution pre-guard wiring (`run_ffi_v2` real execution path):
  - file: `crates/novovm-node/src/bin/novovm-node.rs`
  - behavior:
    - before `run_adapter_bridge_signal`, each admitted tx is normalized into `RouteRequest` and passed to `UnifiedAccountRouter`.
    - default mapping: `LocalTx.account -> uca:local:<account>`, `persona=evm:<chain_id>:<account-address>`.
    - default behavior auto-provisions missing UCA/binding for local tx ingress compatibility, then applies nonce/domain/permission checks.
  - env:
    - `NOVOVM_UNIFIED_ACCOUNT_EXEC_GUARD` (default `true`)
    - `NOVOVM_UNIFIED_ACCOUNT_EXEC_AUTOPROVISION` (default `true`)
    - `NOVOVM_UNIFIED_ACCOUNT_EXEC_SIGNATURE_DOMAIN` (default `evm:<chain_id>`)
  - persistence:
    - execution guard mutates/saves the same router DB (`NOVOVM_UNIFIED_ACCOUNT_DB` or default artifacts path).

- Added native adapter ingress pre-guard wiring:
  - file: `crates/novovm-adapter-novovm/src/lib.rs`
  - hook point:
    - `NovoVmAdapter::execute_transaction` now normalizes adapter `TxIR` ingress into `RouteRequest` and passes it into adapter-local `UnifiedAccountRouter` before state apply.
  - behavior:
    - protocol normalized as `ProtocolKind::Eth`
    - auto-provision default on (`uca:adapter:<from-address-hex>` + EVM persona bind)
    - replay/nonce/domain checks are enforced before transfer apply
  - env:
    - `NOVOVM_UNIFIED_ACCOUNT_ADAPTER_INGRESS_GUARD` (default `true`)
    - `NOVOVM_UNIFIED_ACCOUNT_ADAPTER_AUTOPROVISION` (default `true`)
    - `NOVOVM_UNIFIED_ACCOUNT_ADAPTER_SIGNATURE_DOMAIN` (override default `evm:<chain_id>`)

- Added plugin adapter ingress pre-guard wiring（host-side）:
  - file: `crates/novovm-node/src/bin/novovm-node.rs`
  - hook point:
    - `run_adapter_bridge_signal` plugin branch now runs unified-account route guard before plugin apply.
  - behavior:
    - reuses local-tx normalization (`RouteRequest`) and same store backend.
    - guard is skipped when ingress was already pre-guarded in `run_ffi_v2` (avoid duplicate nonce consumption).
    - optional performance mode:
      - set `NOVOVM_UNIFIED_ACCOUNT_PLUGIN_PREFER_SELF_GUARD=true` to prefer plugin self-guard and skip host plugin pre-guard.
      - host then requires plugin capability bit `0x2` (`UA self-guard v1`) via adapter capability check.
      - host now switches plugin call from `novovm_adapter_plugin_apply_v1` to `novovm_adapter_plugin_apply_v2` in this mode and passes UA self-guard flag (`0x1`).
      - plugin (`crates/plugins/evm/plugin`) now executes in-plugin UA route guard before apply（in-memory router, owner+evm domain+nonce replay gate）.
  - env:
    - `NOVOVM_UNIFIED_ACCOUNT_PLUGIN_INGRESS_GUARD` (default `true`)
    - `NOVOVM_UNIFIED_ACCOUNT_PLUGIN_PREFER_SELF_GUARD` (default `false`)

- Added unified-account audit sink:
  - audit backend:
    - env `NOVOVM_UNIFIED_ACCOUNT_AUDIT_BACKEND` (current default: `rocksdb`)
    - supported backends: `rocksdb` (default) + `jsonl` (compatibility)
  - evidence path default:
    - `rocksdb`: `artifacts/migration/unifiedaccount/ua-account-audit-events.rocksdb`
    - `jsonl`: `artifacts/migration/unifiedaccount/ua-account-audit-events.jsonl`
  - env override:
    - `NOVOVM_UNIFIED_ACCOUNT_AUDIT_DB` (rocksdb)
    - `NOVOVM_UNIFIED_ACCOUNT_AUDIT_LOG` (jsonl)
    - `NOVOVM_UNIFIED_ACCOUNT_AUDIT_DIR` (both backends)
  - behavior:
    - public RPC (`ua_*` / alias) success+reject writes one audit record.
    - `run_ffi_v2` execution guard success+reject writes one audit record.
    - router events are exported by incremental cursor (`flushed_event_count`) to avoid duplicate sink export.
  - query:
    - `ua_getAuditEvents` now supports `source=sink|router`
    - default behavior on RPC runtime is `source=sink`
    - sink query supports `since_seq` + `limit`
    - sink query filters:
      - `filter_method`
      - `filter_source`
      - `filter_success`
  - migration utility:
    - `NOVOVM_NODE_MODE=ua_audit_migrate`
    - `NOVOVM_UA_AUDIT_MIGRATE_FROM=jsonl|rocksdb` (default `jsonl`)
    - `NOVOVM_UA_AUDIT_MIGRATE_TO=rocksdb|jsonl` (default `rocksdb`)
    - migration is incremental by sequence number (re-run will not duplicate already migrated prefix)

- Added UA-Gxx gate automation and evidence output（legacy verification, non-mainline）:
  - node test matrix: `unified_account_gate_ua_g01...ua_g16` in `crates/novovm-node/src/main.rs`（历史验证入口；生产主线不依赖）
  - gate runner: `scripts/migration/run_unified_account_gate.ps1`
  - evidence root: `artifacts/migration/unifiedaccount/`
  - generated summaries:
    - `unified-account-gate-summary.json`
    - `unified-account-gate-summary.md`
  - per-case evidence path follows `NOVOVM-UNIFIED-ACCOUNT-GATE-MATRIX-v1-2026-03-06.md`

## Next Wiring (Pending)

1. Ingress normalization（扩展）
   - done: `public RPC` + `ffi_v2 local ingress` + `native adapter ingress` + `plugin adapter ingress(host-side)`.
   - done（hardening）: plugin-side self-guard 已补齐 standalone 持久化/审计闭环（`crates/plugins/evm/plugin/src/lib.rs`）。
   - standalone env（plugin-side）:
     - `NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND=memory|bincode_file|rocksdb`（default: `memory`）
     - `NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH=<path>`
     - `NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND=none|jsonl|rocksdb`（default: `none`）
     - `NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH=<path>`
   - default performance profile:
     - 默认 `memory + none`，不引入额外磁盘 I/O 路径。

