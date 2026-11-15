# ASAS

- Latest product purpose (2026-11-14): ASAS provides free, authorized shared AI programming services, not primarily failover. Physical A providers run NOVOVM nodes. User withdrew ASAS billing: record actual network bytes and model-token usage; NOV remains the existing asset, with any optional NOV reference conversion requiring a defined rule, never a fabricated balance or reward. Follow `docs/SERVICE-NETWORK.md`: no charges, financial reservations, subscription-limit evasion, implicit account-sharing rights, auto online rewards or second ledger. Preserve other NOVOVM businesses' economic rules. Claude usage/allocation code is implemented; native byte/Codex meters and safe multi-A handoff remain unfinished.

- Quota privacy: only A-local operator IPC may view account windows/all B allocations or change/reset limits. B capabilities/events carry only its own authenticated principal+node allocation, never account totals or other B data. `docs/QUOTAS.md` defines raw token scope, resumed-session baseline subtraction, incomplete usage, cycle-reset history and in-flight overshoot; ASAS resets never reset vendor limits. Never turn unknown quota into sufficient balance.

- AOEM Tool Contract is the platform-wide engine-neutral execution standard. Keep its unique versioned schema and policy in `src/protocol.ts`; Codex and Claude declarations must derive from it. ADR-002/003/004 and priority are in `docs/AOEM-TOOL-CONTRACT.md`. Deliver shared official-engine calls to the same B Worker before cross-engine orchestration or new vendor-specific tool systems. It is not a new AOEM compute ABI or chain interface.

- Canonical source exists ONLY in NOVOVM `plugins/asas/service`; do not create an external ASAS checkout, alias or mirrored implementation.
- ASAS is a native off-chain NOVOVM L3 application plugin. Its isolated worker hosts official Codex and Claude engines independently. B is ACODE; project tools execute on B.
- Preserve ACODE's existing Codex and SGM paths. Do not copy proprietary extension code or vendor branding.
- Reuse the original NOVONET network/identity/crypto owners; native NovoRUDP first, qualified fallback only. HTTPS is a development contract fixture, not the production architecture. Never restore retired WSS, invent crypto, or promise blocking immunity.
- Bind sessions, tasks and tool results to the authenticated peer and project. Persist admission before work; ambiguous effects require recovery, not replay.
- Credentials stay in the owning device's private configuration / SecretStorage. No automatic use, copying or redistribution of subscription credentials.
- Distinguish protocol fixtures, actual SDK/CLI inference, cross-device acceptance and long-duration operation.
- User authorized integration in NOVOVM. Preserve chain, consensus, custody and existing service behavior. Do not register non-deterministic Agent I/O as a chain execution adapter. Commit/push focused verified changes for other machines; source synchronization is not deployment acceptance.
