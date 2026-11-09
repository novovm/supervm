# 2026-11-10：固定3fa已按B→A完成existing换包

程序源码仍为`3fa11f940b18e54b46e6b927d3d66294bea37088`；本轮只调整原部署脚本、
对应本地守卫和交接，不改变已固定的Rust程序、AOEM或业务合同。候选原完整CI见
[候选记录](README.md)，不能把本交接提交当成另一个已部署程序版本。

## 实际结果

| 项目 | B | A |
| --- | --- | --- |
| 原实例→新实例PID | 2587563 → 2777931 | 13964 → 25216 |
| 原版本→本版本 | 2b97fc8e → 3fa11f94 | 5181da73 → 3fa11f94 |
| 运行身份 | novovm，UID996 | LocalService，S-1-5-19 |
| 原库恢复 | START=existing成功 | START=existing成功 |
| 更新前/后独立认证Status | 133ms / 62ms | 29ms / 28ms |
| UDP监听 | 41050/42050，新PID | 41051/42051，新PID |
| 本机locator连续样本sequence | 4168→4170 | 5098→5100 |
| 原循环ticks连续样本 | 100017→197316 | 51706→122483 |
| execution_failures | 0 | 0 |

四次独立原SDK认证查询均为原状态根：
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`。
height=0/current_height=1、recovering=false、projection_failed=false；不是新增共识成绩。
两机实际程序及已加载AOEM哈希见[update-result.json](update-result.json)，与固定清单一致。
独立查询使用新3fa客户端和原operator身份/配置；它引用的旧目录AOEM与本次同hash，
不因此把旧node程序留在运行中，也没有临时替换查询身份。

B先核验359238562字节原归档及80成员、实际运行库KAT后，精确SIGTERM旧进程，
原systemd/novovm账户及受保护凭据启动新程序。B认证健康通过后，A复制77成员、
原范围防火墙规则绑定新exe，准确结束旧PID并由原LocalService计划任务入口恢复。
没有重启整机，没有调用旧initialize任务，没有并发打开同一库，没有删除LOCK。
本轮没有使用回滚、重新创世或换钥；旧进程退出按中断恢复记录，不冒称新增正常排空验收。
A三类Windows防火墙保持启用，当前任务为`NOVOVM-A-3fa11f94-Existing-20261110`；
B当前unit为`novovm-B-3fa11f94-network-20261110.service`。沿原24小时运行窗口，
不是永久SCM托管/自动续期已经验收。

## 开发期授权已更新，不以连续在线阻挡开发

用户明确在正式发布前全系统均为内部开发，B邮箱/Web/Relay/Docker也可按开发需要
直接停服、重启和换包；Agent可清理已确认无价值的测试库、旧钥、旧包和中间产物。
最高规则已更新，覆盖历史禁停/禁换/一律保留限制。不是每次都必须清库：本次原库
升级无需重建，直接existing更快。数据重建/换钥须同步关联配置和域/签票状态，不能
把新历史当旧历史连续，也不能靠删除数据把故障记成修复。

原Linux更新器已取消其他开发服务PID/在线状态必须不变的阻断，仍如实记录变化。
本次实际这些服务未变，不是今后必须如此；固定node包没有它们的替换制品，本轮
没有无目的停止它们。密钥/库一致检查用于本次existing承诺，不是禁止今后开发重建。
C/D本轮未换包、未重启、未初始化；本次不修改NOVOCHAT。

## 本地脚本验证与尚未完成的网络工作

Windows原更新器实际固定包、部署、原规则复制、崩溃/正常退出、XML与回滚准备守卫
通过；Linux原更新器13项通过，含新增“应用服务PID变化不阻挡切换，但未计划的
custody变化仍拒绝”对照。没有重跑云端CI、修改必检或跳过公告；正常push仍由原工作流处理。

原逐对象观察已启用，两机22项UDP事件名及中继观察字段存在，控制命令为空，
没有发送新测试对象、资产交易或收到新的Object DONE/Business Receipt。
本机签名locator已更新且采集时未过期，不等于其他节点已取得/接纳它。

仍保留真实网络缺口：A采样的`connection_failures`从5增至13、已安装bound路由仍为0；
`global_auto_discovery=false`、`native_multihop_accepted=false`。B第一份样本有novonet
投影，第二份该可选字段为null；不把缺省字段当作全网通过。更新与认证Status成功
不替代对端发现、节点间对象及最终DONE。下一阶段沿同一owner检查发现/路由失败并
继续真实中继/对象交付，不因本次换包就关闭既有网络故障。

## 原始证据

[original-update-evidence.zip](original-update-evidence.zip)：30项，170289字节，SHA256
`b57495023d925082a399639b4e8234737fbb064e45a902b025d15db194ab98b0`。
包含实际更新stdout/stderr、旧进程退出、两次live进度、前后认证Status、B实际库KAT、
本地脚本守卫及执行脚本快照；逐成员hash回读验证。不含私钥、受保护材料正文、数据库、
journal或完整私有运行配置。本地完整更新记录位于
`C:\Users\leadb\AppData\Local\NOVOVM\update-sqlite-3fa11f94-20261110`。
