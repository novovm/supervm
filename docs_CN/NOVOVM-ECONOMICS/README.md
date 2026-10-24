# NOVOVM Economics

## 2026-10-24 当前服务经济入口

- [首发经济契约](../../docs/NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md)：定向引入远端同名文档并修正旧最低 5 USD 分账，不合并其他代码；1 亿总量、9 位精度为设计规则，执行未交付。
- 统一基线第 11–12 节已记录职业经营、多设备、自费测试与公共补助边界：补助购买可验证容量/事先发布任务，自造订单不自动获补助。3,500 万、80/10/10、体验预算仍为候选；首笔测试 NOV 具体到账验收仅为后续开发规格。

- [统一经济基线](NOVONET-SERVICE-ECONOMICS-BASELINE-2026-10-24.md)：已确认、建议、待定和实现缺口分开；覆盖旧最低 5 USD 分账及原生 NOV 增发/销毁口径。
- [English synchronization](../../docs/NOVOVM-ECONOMICS/NOVONET-SERVICE-ECONOMICS-BASELINE-2026-10-24.md)。
- **设计决定已更新，实施未交付。** 不据此启用收费、改变创世配置或宣布节点到账。

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
