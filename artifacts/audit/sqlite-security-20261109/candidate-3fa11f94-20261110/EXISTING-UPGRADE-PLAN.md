# 固定3fa候选的原库升级方案（已批准，执行结果另记）

实施完成：[B→A实际更新结果](UPDATE-RESULT.md)。以下保留当时计划，后继结果优先。

> 2026-11-10用户已确认执行本版本B→A范围，随后明确正式发布前的开发NOVOVM旧节点
> 可以直接停止/重启/换包，不必逐次申请；实际结果在后继部署回执记录，
> 批准不等于执行成功。
>
> 同日用户进一步明确：B邮箱、Web/nginx、Relay、Docker也属于整个内部开发系统，
> 必要时可直接停服、换包和联动更新，不以服务连续性或PID不变作为开发前置。
> 本次3fa节点包不包含这些应用服务的替换制品，因此不主动改动它们；其状态仅作
> 诊断记录，不因变化自动阻断或回滚节点。原库、用途钥、journal及实际健康验证保留。
>
> 后继授权也允许Agent直接清理无价值的内部测试数据、旧密钥、旧包和中间产物，并在
> 开发需要时重建。下文原库/不换钥范围是本次3fa换包的具体实施选择，不是永久禁令；
> 当前升级不需要重建，继续existing能更快完成并检验真实恢复能力。

本方案不要求先修好EVM插件。主网沿原native_execution_tick、NativeLifecycle/
NativeChannel、NOVONET/NovoRUDP优先/Iroh合格备用及AOEM原存储所有者运行；
不启用Ethereum/RLPx网关模式，不更换账本、身份、共识或费用合同。

## 本次已批准的实际范围

建议先B、健康核验成功后A；C/D本次继续不动。固定程序源码
`3fa11f940b18e54b46e6b927d3d66294bea37088`，只使用[candidate-manifest.json](candidate-manifest.json)
中实际封包的Linux/Windows程序及原AOEM，不使用后来main的未冻结程序替代。

| 操作 | 本方案范围/当前授权状态 |
| --- | --- |
| B固定Linux候选existing更新 | 本版本已批准 |
| B验证成功后A固定Windows候选existing更新 | 同上，严格串行 |
| 必要时精确停止原节点进程 | 已批准；核对程序/PID/账户后执行，不重复申请 |
| A新程序Windows UDP规则 | 仅复制当前已核验的原程序规则，完全同范围；旧规则留作回滚，三类防火墙保持启用 |
| C/D | 本轮不换包 |
| B邮箱/Relay/Web/nginx/Docker | 开发需要时可直接停止和更新；本节点包不含其替换制品，当前不主动改动 |
| initialize、创世/用途钥/资产钥生成或替换 | 不执行 |
| 清库/迁移/覆盖DB或journal、删除LOCK | 禁止 |
| 降共识票数、改认证/超时/资源上限 | 禁止 |

## 固定输入与保留原账

归档位于A：`C:\Users\leadb\AppData\Local\NOVOVM\candidate-sqlite-3fa11f94-20261110`。
Windows ZIP与Linux tar.gz分别为150496796/359238562字节，完整归档、manifest、
node/client/ctl/AOEM哈希见候选清单。AOEM仍为原Windows1b764a36/Linuxd73e4a2f，
SQLite依赖已在新程序静态代码中更新；不需要更换AOEM或迁移主链RocksDB。

| 对象 | B | A |
| --- | --- | --- |
| 原节点版本记录（执行前须现场核验） | 2b97fc8e | 5181da73 |
| 原账户 | novovm（现场读取真实UID/GID） | LocalService/S-1-5-19 |
| 原DB | /var/lib/novovm/selfrun/91129655606029/B/chain-state | C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state |
| 当前版本工作目录记录 | B/novonet-relay-evidence-2b97fc8e-20261109 | A/novonet-probe-5181da73-20261109 |
| 已知进度日志相对该工作目录 | logs-retry-start-1/progress.json | logs/progress.json |
| 新程序独立目录 | /opt/novovm/releases/novonet-sqlite-3fa11f94-linux-x86_64 | C:\ProgramData\NOVOVM\releases\novonet-sqlite-3fa11f94-windows-x86_64 |
| 原UDP端口 | 41050/42050 | 41051/42051 |

以上是既有记录而非本轮实时存活证明。原24小时窗口可能自然结束；实施前区分正在
运行、已证实正常结束、崩溃和未知状态。正常结束复用原manager/日志证明，不把
MainPID=0当成恢复资格，不盲用旧PID、不运行任何含initialize的旧任务。

继续使用原chain域、初始承诺、账户状态、validator/transport/carrier材料及受保护
凭据；不得借升级产生新身份或余额。Linux仍用原EncryptedCredential；Windows
仍由原服务账户解封。配置只改变固定新程序/库/工作日志路径等版本字段，不改业务。
本次不复制运行中DB冒充一致性备份；保留原程序、配置及日志，必要恢复仍打开原库。

## 原部署所有者的执行与健康核验

1. 先核验归档及全成员哈希、当前程序/账户/进程代次、端口、原库路径、用途公钥与
   受保护材料哈希、进度/状态根及B其他服务PID/状态。保存原始证据，不读取或回传私钥正文。
2. 复用原Linux/Windows更新器，将本候选pin与现场原版本映射绑定后跑其原有定向
   守卫；当前写死2b97/ddd等旧映射的脚本不能直接拿来执行3fa。B成功重入的进度在
   logs-retry-start-1，不能误读最初失败的logs目录或再次执行失败重入入口。
3. B新版本独立目录暂存后，核对固定包的实际库与合同。工作/commands按原计划
   root:novovm/0750，日志novovm/0700；显式chmod抵消umask，不改DB/用途材料权限。
   用真实novovm账户做访问预检，保留Type=exec与PID/UID/exe校验。
4. 精确关闭B目标旧实例；若需中断则按中断原库恢复记录，不能假称正常排空。
   没有原实例/端口完全退出的证据，不启动新实例，不删除LOCK。
5. 固定新程序START=existing、原账户/原库/原材料启动。核对实际程序和已加载AOEM
   哈希、fresh进度、原状态根/身份、locator新鲜度、监听归属、独立原客户端认证查询，
   默认探针不执行。B其他开发服务前后状态如实记录，不要求PID/在线状态不变。
   Running或Status不能替代Object DONE。
6. B全部成功后才更新A。不得重跑082初始化任务；复制完全同范围的新程序规则后
   保持三类防火墙启用。仍检查原LocalService、原DB/材料、实际库、根和认证查询。
7. 保留原24小时运行边界，不将其写成永久SCM服务验收；不要因为旧窗口结束重创世。

## 失败与回滚

身份/原库/状态根不符、持续认证失败或进程归属不明时，立即停止
后续主机操作。保全原日志/库；回滚是**程序退回，原账不退回**：停止准确的新实例，
再用各自原程序和原配置existing打开同一份原库，不强开、不清库、不换钥。

回滚日志及回执必须新建；先检查重复回滚/既有日志，再决定是否停止健康新实例，
不能先停后因日志重名失败。发现真实格式不兼容时不得强用旧程序打开，先保全现场。
旧B2b97/A5181仍带旧SQLite安全依赖，退回只为故障恢复，不代表安全修复继续生效。

## 方案形成时尚未执行（后继已按上方回执实施）

没有上传新包至B/C/D、注册/启动新节点、改防火墙、停止节点或访问其实际DB/钥匙。
没有新的四机对象DONE、资产交易、最终性、稳定性或TPS成绩。程序候选、CI、部署、
原生网络验收分别记账；EVM插件专属功能问题不作为原生主网启用的额外门槛。
