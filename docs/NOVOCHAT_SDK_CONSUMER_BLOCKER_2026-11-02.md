# C回报：App通信SDK候选未通过

## 后续回传：该迁接用例为测试判定缺口，整组与手机门槛仍在

C已完成同条件旧16e4576/新06801146对照：原断言两版均失败；有界事件显示两版均已提交真实Endpoint设备回执，但测试仍等待mailbox专用回执及中间Store。仅修App测试判定，并增加必须真实Put/存储回执的独立custody用例，两版分别2/2通过，30秒迁接期限和产品通信逻辑未改。下文“未观察到最终DeviceReceipt”只描述原测试识别范围，不能再作为该请求没有设备回执或新SDK回退的证据。

主App门禁与CI已准备固定受审06801146，全部dirty/diff校验保留；更广回归正在执行，两台仍v108，未完成新APK手机验收。首次持续寄存失败及历史真实停滞继续保留。A不需要因本用例另写传输修复；C负责后续App集成和双机，A继续共享SDK/UCA/资产owner。详细结果与日志哈希见[NOVOCHAT同条件证据](https://github.com/XujueKing/NOVOCHAT/blob/main/docs/implementation/evidence/sdk-migration-20261102/comparison.json)。Git通知不代表A已签收。

交接 `C-CHAT-SDK-20261102`，接收方validator-A。用户已让C回到双手机聊天开发；本轮仅消费者源码/本机回归，没有主链部署、多机连接测试或云服务变更。

C审阅App pin16e4576到共享main06801146的通信输入，并暂时固定candidate进行原App测试。直接依赖novovm-udp-batch已补入App审阅范围。保留原架构、身份、期限、预算及消息ID。主App实际feature为tls-transport,mailbox-delivery。

结果：714项整组观察到163通过、2失败后中止，余项未验。application_cached_v6_l4_migrates_to_v8_without_second_publication独立重跑再次失败（63.52秒），已有trace通过native认证和回程对象入库后，在原验收期限内未到最终设备回执。application_continuous_mailbox_at_deployed_node_rate独立重跑通过（150.85秒），首次失败保留。没有旧SDK同条件对照，不能断言某个上游规则为根因。

原始失败/独立结果及依赖锁证据以[NOVOCHAT审阅记录](https://github.com/XujueKing/NOVOCHAT/blob/main/docs/implementation/SDK_CONSUMER_REVIEW_2026-11-02.md)为准；不含私钥、节点配置或聊天数据。C最终恢复旧批准pin，确认当前candidate被门禁拒绝，不构建/安装v109；两台仍运行原v108，前轮双向文字与设备回执已复验。

请A按唯一network owner分工协助核对该旧App消费者组合的发送/反馈/回程推进。先复用现有用例和有界计数定位，不延长期限、改大窗口、放宽认证或复制另一套SDK。C负责App侧复现/接入，A负责共享传输实现；本报告不是未经定位便要求修改上游。推送不等于A已接收。
