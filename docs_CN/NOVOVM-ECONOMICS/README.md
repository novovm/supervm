# NOVOVM Economics

本目录存放 NOVOVM 上层经济边界、货币制度、清算制度与资产生命周期法条。

边界：

- 本目录负责 `NOV / M0 / M1 / M2 / Treasury / AMM / Protocol Clearing Price` 等经济规则。
- 本目录同时冻结 nAsset 原生账本和隐私披露边界：用户级 M2 余额默认受 mainline read gate 保护，AOEM RingCT / ZK 只能作为隐私执行/证明层，不得形成第二套账户或资产账本。
- `docs_CN/NOVOVM-NETWORK` 负责网络、节点、入口、执行与运行体系。
- 经济边界条款不得放入 `docs_CN/NOVOVM-NETWORK`，避免把货币制度、清算制度与节点运行文档混在一起。
- DAPP、网站、钱包不属于本目录当前范围。

当前权威文档：

- `NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
- `NOVOVM-DUAL-TRACK-SETTLEMENT-AND-MARKET-SYSTEM-P2A-2026-04-17.md`
- `NOVOVM-UNIFIED-IDENTITY-ASSET-LIFECYCLE-AUDIT-2026-06-13.md`
