# 原 tick 结构准入与选批检查点

2026-10-16；恢复输入 `3ed27bec`，实现 `f2d56b5b`。共享源码基线
`a755c814`，随后快进保留另一设备 `37c71924` 的 NOVOCHAT 更新。
本目录保留失败报告，不把恢复现场当成共享 main 已激活原 tick。

## 消除的工作与保留的边界

`AdmissionInput::Apfl` 在 pending 中继续持有原共享结构与行索引，选批及定向
补传从借用的 `TransferView` 构造结构批。去掉了这里的整批 canonical raw
物化、Raw 克隆与重新解码。子批仍构造自己的有界列与签名；最终归档/证明
规范编码仍保留，不是全流程零拷贝。没有改变 AOEM、业务或共识协议。

同 hash 不同签名不能复用鉴权；原顺序、nonce 预留、规范字节上限保留。
新增完整共享 owner 的实际容量计额，一行未移除也计入整个 owner，最后一行
释放才退还预算；与原逻辑字节上限同时执行，不能靠小切片长期保留无界大对象。

## 本机验证

- Windows debug，原四节点进程、原生 NovoRUDP/APFL 单客户端入口，无 HTTP。
- 恢复普通 pipeline：347 passed / 43 ignored；CPU 实际闭环两项通过
  6.57s，完整代数路径同两项通过 15.64s。包括交易经济、重放、重启后后继、
  两节点少数不能发布与错误签名。这里是测试总耗时，不是 TPS。
- 共享代码 pipeline：413 passed / 64 ignored；原 node binary check 与
  `cargo clippy --locked -p novovm-node --lib -- -D warnings` 通过。
  这些检查在快进 NOVOCHAT-only 更新前执行，不签收其新增信箱能力。
- 小负载 32 笔/8 笔批的四节点回执、余额和冷恢复通过，见
  [原始报告](small-32-pass.json)。仅功能预检，不作为产能数字。
- 4096 笔/1024 笔批：冻结旧版与候选都在原生 receipt 请求 deadline 失败，
  分别见 [旧版](baseline-4096-failed.json)、[候选](candidate-4096-failed.json)。
  TPS/P95/P99 为 null；没有成功吞吐改善结论，未跑完 65,536 笔。
- [诊断复跑](candidate-4096-diagnostic-failed.json) 开启原进度快照，不与正式
  关闭诊断的样本混算。发现 poll_query 消费完成请求后检查 reply busy 的
  丢请求竞态；正在原 owner 内修复。它是源码确认缺陷，尚未证明解释全部等待。

首次 32 笔夹具因非法 IO quantum 在发压前失败，改为既有合法值 64；原失败
保留在恢复现场 `target/native-product-load/26768-1792094995932674800`。
4096 失败不通过扩大超时、降低票数、重试伪装成功或取消四节点确认处理。

## 身份与计时口径

冻结旧 node SHA256：
`98a868efb7a74d269c97612d74921d8f4a739a5552c7515ded2f40e5fec1973f`

结构候选 node SHA256：
`3e721df666febd5af1eb4ddfa15ba7803c424a82bf630b583d2fb1299467b911`

既有 AOEM DLL SHA256（所有样本相同、严格 manifest）：
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`

插电；AOEM workers=4，提交窗口8192，单入口非首提案者。签名生成、启动和
APFL 准备不计时；首次提交尝试到四节点全部最终回执响应计时，逐笔延迟含
原生查询观察调度。成功、业务失败、拒绝与超时分列。冷恢复和经济核对另列；
费用总额与账户守恒已核对，不声称枚举审计了全部费用状态页。

原始报告内绝对路径只是当次机器观测，不是部署路径要求。未新增 Linux、
跨机、A/A/B、离线故障、业务 ZK 或生产签收。下一项为查询交接回归及原
节点同条件负载复测，再按[迁入表](../../../docs/NOVOVM_JULY_MAIN_INTEGRATION_MAP.md)
接回整套已验证原 tick；不得继续扩展替代 bootstrap。
