# 旧完整CI的高度4超时：保留失败，补足失败现场

完整CI41757929427（facd175b）的Rust任务126338555224也失败了，不能只记录该轮
Windows mirror失败。原`fresh_genesis_main_nodes_continue_three_heights_without_restart`
运行332.81秒，在`continuous height 4 deadline`退出。

## 原证据能确定什么

原上传的172份公开进程证据仍在。三在线节点均确认同一高度3证书
`2b71b0d52d367a248ac0a70e006adae8380611ac09f595d39898901d8ebf0491`。
高度4提交已返回queued，否则会先失败于入队断言。没有随后120秒内的实时状态。

确认日志只在高度变化时打印；高度3时的round0、空池与空闲candidate不是高度4超时
现场。fresh入口没有消费通用progress文件开关，缺文件也不证明节点卡死。
离线第四peer握手超时符合该测试拓扑，不能单独解释三在线节点为何没确认高度4。
**旧失败的具体等待点与根因尚未确定。**

## 只补失败观测，不猜测修复生产逻辑

保留原120秒高度期限、三在线/一离线、逐高度入队、原交易/费用/nonce/共识和重启核对。
仅在原期限已经失败后，对各在线节点原`nov_chainStatus`及对应
`nov_getTransactionStatus`各请求一次，每次总超时1秒，无重试。
原响应/错误、node_index、时间和请求耗时写入同目录
`continuous-deadline.process.json`并输出stderr；原PID仍可由既有process.json关联。
随后仍执行原panic，写文件失败也不吞原失败。

这是失败后的顺序采样，不是跨节点原子快照。期间节点可能继续推进；即使后来已
finalized，也不能倒改原用例失败结果。正常成功路径没有新增请求或等待。
复用既有CI公开进程证据allowlist，不上传配置、用途钥、数据库或原始签名载荷。

## 限定复现和验证

同一Linux原测试二进制、独立网络namespace、CPU0/1、实际候选AOEM库，预先限定三轮：

| 轮次 | 实际结果 | 耗时 |
| --- | --- | --- |
| 1 | 1通过/0失败 | 301.99秒 |
| 2 | 1通过/0失败 | 293.81秒 |
| 3 | 1通过/0失败 | 294.85秒 |

三轮均未复现，不继续重跑到满意、不计为旧超时已修复；新失败快照分支尚未在该
超时反例下实际触发。Windows/Linux原CLI测试目标严格Clippy、格式及diff检查通过。
生产代码、超时、认证与重试规则未改，C/D和B既有服务无新操作。

[原CI日志、172份公开证据及本次原日志ZIP](continuous-recovery-evidence.zip)，SHA256：
`f2a0e6a2be1d74fedf04405ce7482a1cac9c8d3e13662696266e9ff5e0157ac2`。
不含下载失败stderr中的临时URL或任何数据库/密钥。

## 完整CI的版本边界

[41759543306](https://github.com/novovm/supervm/actions/runs/41759543306)已成功，
固定源码为f5a866d825b7180fe3be7acc9340188d5a0dc05a，其中该连续恢复步骤也通过。
这是完整CI通过事实，不是旧超时根因已修复；也不能覆盖后续镜像测试修正和本次
失败诊断。最终包含这些差异的main SHA仍须运行原完整门禁。
四机认证仍使用原d7ef793b制品，不重跑、不启链、不登记新TPS或候选正式发布。
