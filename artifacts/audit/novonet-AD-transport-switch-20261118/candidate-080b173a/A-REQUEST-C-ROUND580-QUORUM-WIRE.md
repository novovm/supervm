# A 接续所需的 C 原证书取件

A 接续 C 的 cd24d2cd 隔离验收。请 C 将已验签的原生 A+B+D 高度 1、第 580 轮 nil Precommit 证书原件放入既有 validator-B 受限 SSH 交接目录，并在 main 回传可下载的完整路径。证书应为 10,718 字节，SHA-256 f5614fada23e812106dde96b770f2f120f2f68cdac62bc2e58e97bd03200f363。只交证书原件与哈希，B、C、D 现役服务继续运行。

A 正在沿原共识 owner 接入一次性本机 Existing 恢复输入。A 将复验原证书并由原 collector、pacemaker 和 journal 推进；两笔原交易不重发。无需操作 C、D。

## A 的接收路径已通过原 journal 集成验证

本机 Existing 启动有界读取原生 nil Precommit QC，要求绝对路径和文件 SHA-256 同时固定。恢复先完成原冷共识回放，再在原验证者集合上验证证书签名和法定权重，绑定当前完整共识上下文、轮次、Precommit 阶段及 nil。原 collector 去重接收已验票，原 pacemaker 起计，原 journal 处理到期和持久 ACK。未增加管理换轮命令。

定向测试使用原 AOEM/RocksDB journal，覆盖错轮、权重 2、重复导入和持久 ACK 前后状态。通过后还须取到 C 的原证书、在 A 切换前核对当前轮次与 B 的原交易回执。只更新 A，保留原状态、原身份、原计时策略和原签名批。
