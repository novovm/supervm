# NOVOVM Economics

## 2026-11-05 对外经济、公式及开发的阅读入口

- [经济设计与公式参考](../../docs/NOVOVM-ECONOMICS/NOV-ECONOMICS-DESIGN-AND-FORMULA-REFERENCE-2026-11-05.md)：三份用户附件全文、候选分配/释放/治理、五类业务成本、公式前提和量纲、算术核对、实际源码接线；不把参考保存当参数批准。
- 先读[首发经济契约](../../docs/NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md)、[统一基线第13节](NOVONET-SERVICE-ECONOMICS-BASELINE-2026-10-24.md#13-2026-11-05-全业务成本报价和分配的统一阅读基准)和[订阅日结契约](../../docs/NOVOVM-ECONOMICS/NOV-COMMUNICATION-SUBSCRIPTION-DAILY-SETTLEMENT-2026-11-02.md)。成本、售价、分配分别表述；已定规则不重问，新费率/比例仍待决定。
- 演练仍称NOV，不另造测试币。本文档更新不启用收费/创世/部署，不承诺节点利润或固定法币收入。

## 2026-10-24 当前服务经济入口

- [首发经济契约](../../docs/NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md)：定向引入远端同名文档并修正旧最低 5 USD 分账，不合并其他代码；1 亿总量、9 位精度为设计规则，执行未交付。
- 统一基线第11–13节记录职业经营、多设备、自费测试及公共补助边界：补助购买可验证容量/事先发布任务，自造订单不自动获补助。分配/释放、20%内部10/10及体验预算仍为候选；整体订阅80/20/0已确认。首笔NOV实际账户到账仍须原产品闭环验收。

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
