# C原库恢复后的A侧认证查询

状态：`C_RECOVERY_AUTHENTICATED_QUERY_PASS`。固定程序仍为1ef31893。

A核验`80d23fb6`的九项C公开原件摘要：C新PID24376、LocalService、原库existing、
包/实际AOEM摘要、用途材料不变、41052/42052监听及有效原进度均按回执成立。
没有把原`update_completed=false`或遗失的旧进度改成通过。

2026-11-10 **10:56:41北京时间**，A使用原独立SDK操作员及既有C固定公钥配置，
只发出一次Status查询，退出码0，SDK耗时**44ms**。实际目标
`192.168.71.125:41052`，本地`192.168.71.74:41190`，未改规则/配置或重启节点。
复用已核验3fa操作员及原AOEM，不冒称查询客户端已更换为1ef；目标C的1ef运行身份由
本机回执与外部固定身份查询结合核验，响应本身不远程证明可执行文件哈希。

返回C公钥、链域、genesis/protocol与A保留的2026-11-09 C认证结果一致；
height=0，current_height=1，state_version=0，recovering=false，projection_failed=false。
状态根为`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`，
与该C历史锚点匹配。旧全零报告不是比较输入，历史锚点不是遗失的停机前快照。

本次签收：C新程序原库恢复运行及外部认证查询健康，可继续已批准的D串行更新。
不签收：停机前后完整连续性、全零报告根因修复、长期稳定性、对象DONE、交易或最终性。
原失败文件/历史回执保留，C保持运行，未发资产或新探测对象。

原始输出见`original.json`、`original.stderr`，时间及工具哈希见`execution.json`，
公开结论见`public-result.json`，实际执行脚本见`Read-C-Recovery.ps1`，摘要见`SHA256SUMS.txt`。
没有上传原配置、私钥或数据库；未触发或重跑Full CI。

下一步：[D执行入口](../novonet-seed-retention-20261110/candidate-1ef31893/CD-UPDATE.md)。
