# NOVOVM Core / Plugin / External Capability Layer Map (2026-04-17)

## 1. Purpose

Unify team language and prevent the most mature EVM capability line from being misread as "EVM is the system host."

## 2. Three-layer structure (frozen wording)

```text
NOVOVM / SUPERVM (Host)
|- Core Host Layer
|  |- AOEM execution engine
|  |- Scheduler/runtime/gate
|  |- Budget isolation (network/execution/storage/query)
|  `- Canonical chain / lifecycle / reorg adjudication
|
|- Plugin Layer
|  |- EVM plugin (currently in maintenance mode)
|  |- Future chain plugins (BTC/SOL/...)
|  `- Other execution plugins (AI/specialized protocols)
|
`- External Capability Layer
   |- Standard submit/query interfaces (for example eth_*)
   |- Parity gate / nightly soak / duty report
   `- Operations entry points and audit artifacts
```

## 3. Role definition

- Host belongs only to `NOVOVM/SUPERVM`.
- `EVM` is a plugin capability, not the host.
- "EVM mainline completed" means plugin maturity, not that the system identity equals EVM.

## 4. External communication rules

Recommended:

- "NOVOVM completed the EVM plugin mainline and moved it to maintenance mode."
- "EVM is the first mature plugin capability on NOVOVM."

Avoid:

- "EVM host system"
- "SUPERVM is a modified EVM node"

## 5. Resource allocation rule

- EVM line: maintenance mode (sample feeding, nightly gate, budget stability)
- Main resources: NOVOVM core-layer development and next plugin capabilities
