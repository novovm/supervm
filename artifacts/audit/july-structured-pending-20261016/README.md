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

## 查询交接修复复验（恢复 `3f0a95e4`）

在原 NativeChannel 增加最多8项、每peer1项的已完成请求保留，绑定实际认证
session与完整Request；旧reply等待DONE时不再丢弃下一请求。全局满时不消费
重组结果，其他peer继续；截止不续期，过期/换代精确只读请求无回复终止，
错误Request仍拒绝。没有改变共识、确认终点或客户端5秒请求deadline。

三个反例通过，包含真实socket下延后旧reply DONE交接，随后新请求仍收到
精确回复；另验容量、异peer、会话更换、到期与错误响应。全部pipeline
351通过/43忽略（17.90s）。最终binary的CPU原生四进程两场景通过6.23s，
完整代数同两场景通过15.35s，均含少数不能发布、重放和恢复。

同4096/1024 debug预检随后通过：4096成功、0业务失败、0超时、四节点回执
一致及冷恢复通过。测量窗36.957978s，成功观察吞吐110.828574 TPS，
P95=35.886587s、P99=36.717876s；全测试含经济/重启172.24s。
见[报告节选](query-fixed-4096-pass.json)，仅省略1025账户余额数组；完整原始
报告和逐笔观察保留于恢复现场对应目录。诊断关闭；费用状态全页审计仍未做。

这是解除查询丢失后的debug功能负载，不是正式release同条件容量测试，也不是
历史约十万/百万TPS已恢复。旧版未通过同预检，所以不能计算新旧TPS提升率。
接下来应完成共享原tick接线及正式优化构建对照，而不是把该数字作为性能上限。

最终修复binary SHA256：
`f1199e5c15d219dfde0262431d4ac02ed785e6ff0318b94e7f42d9a1d2df8a9d`

共享 `0fd2ad7e` 已合并另一设备 `f8846f53/c3f370e9` 的同socket客户端
构造更新；本机复验其两项endpoint测试、原node binary check、lib严格
Clippy均通过。没有把该更新覆盖进冻结恢复负载，也未声称远端CI通过。

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
