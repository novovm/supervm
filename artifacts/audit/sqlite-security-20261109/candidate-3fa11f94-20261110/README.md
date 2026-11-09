# 原生主网3fa候选交付：CI及制品完成，A/B已更新

2026-11-10后继：[B→A实际更新回执](UPDATE-RESULT.md)。两机已运行固定3fa，原库
恢复及更新前后认证查询通过；C/D本轮未更新。以下候选冻结时的“尚未更新/上传”
属于当时事实，不再代表当前状态。网络对象/中继DONE仍单独验收。

固定程序源码：`3fa11f940b18e54b46e6b927d3d66294bea37088`。
本记录日期2026-11-10（北京时间）；后继文档提交不改变这个程序身份。
沿原node → exec → bindings → AOEM及NOVONET/NovoRUDP主路径，Iroh保持合格备用。
EVM是兼容插件/适配能力，不代替原生主网；其专属功能问题不作为主网新增前置条件。
只有有证据影响原生主网共用代码的缺陷才按实际影响处理，不按目录名笼统放行或阻断。

## 四类结果分别签收

| 范围 | 本次实际结论 |
| --- | --- |
| 源码与本地安全/存储回归 | 3fa依赖修复及父目录所列检查通过；RocksDB/AOEM、schema、共识、身份、费用不变 |
| 同源码原完整CI | 42609044443，attempt=1，completed/success；14个job成功、2个按范围跳过，不计为执行通过 |
| Windows/Linux固定候选 | 原打包器真实release构建、全部成员hash、包内运行库及7项CLI/NAT用例分别通过 |
| A/B/C/D运行节点 | 后继B→A已实际更新3fa；C/D本轮未更新，详见更新回执 |

[原CI](https://github.com/novovm/supervm/actions/runs/42609044443)由push触发，
最终时间2026-11-09 21:06:40北京时间；本轮再次只读核对同一run/SHA/attempt，未新增、
重跑、取消CI或改变必检。Governance PR Gate observability与Node checks按原范围跳过。
[原状态](original-ci.json)与[job摘要](original-ci-jobs.json)保留，不能替其他程序版本背书。

本次只同步交付文档和公开证据，Rust/依赖/工作流/候选程序均不改。现有ci_scope.py
只对精确白名单文件选择docs；本交接/审计文件不在该名单，推送仍会自动选择full。
不手工追加运行、不加skip、不取消或修改路由；该文档提交的自动结果单独记录，
不把它当成3fa程序又需重新打包的理由，也不在本轮重复等待已成功的原CI。

## 固定平台制品

归档在A本机：`C:\Users\leadb\AppData\Local\NOVOVM\candidate-sqlite-3fa11f94-20261110`。
解包目录在`artifacts/product-overlay/novonet-sqlite-3fa11f94-{windows,linux}-x86_64`。
候选二进制不放入本审计Git提交；冻结时未上传B/C/D，后继已上传并部署B。

| 平台 | 原归档文件 | 字节 | SHA256 |
| --- | --- | ---: | --- |
| Windows x86_64 | novonet-sqlite-3fa11f94-windows-x86_64.zip | 150496796 | 3fc6a8ac67c84cabea906a0f2af39ad01a2f0e10e06bf4b25cf423359d995302 |
| Linux x86_64 | novonet-sqlite-3fa11f94-linux-x86_64.tar.gz | 359238562 | d6ed8d869c46530077e9feb61b4ce0ef83dddcddc907f2e6d8a443825ae3e692 |

原Rust1.94.0、jobs=2、locked release构建；开始/结束均核对干净3fa源码。
复用A原C盘Windows缓存和WSL Linux缓存；未新建开发分支、清理原库或改网络。
Windows77/Linux80个清单成员的路径、长度、SHA及checksums均核验；ZIP成员与tar内容
分别核验。node/client/ctl/AOEM及manifest完整哈希见[candidate-manifest.json](candidate-manifest.json)。
AOEM仍是原Windows `1b764a36…` / Linux `d73e4a2f…`，不是用改manifest冒充新库能力。

包内实际AOEM加载路径/hash、scalar及统一语义/mixed-batch验签KAT通过；实际节点
`--service-contract`核对原native tick/existing，不启动服务或打开实际数据库。
原非业务377字节probe只做prepare/cancel，没有发送。
包内原protected identity/NAT/reconnect及失败反例各7/7通过，Windows19.30秒、
Linux92.95秒；测试使用隔离材料，未访问部署用途钥。这些是本机包内验证，不是四机实网。
详情见[Windows](verified-windows.json)、[Linux](verified-linux.json)及原始日志ZIP。

## bundled SQLite与quota VFS的实际制品证据

`rusqlite=0.41.0`、`libsqlite3-sys=0.39.1`、bundled SQLite **3.54.1**。
源身份为`2026-10-16 14:45:20 555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6ca76`。

核对20个实际打包可执行文件的manifest哈希和版本/源标记；两平台真实node含新SQLite
版本及源身份，20个程序均未命中旧SQLite3.53.2源身份。没有标记的角色不因此断言
“不含SQLite”。Linux实际node还核对三个sqlite版本/源接口与三个quota接口的代码符号，
ldd没有动态libsqlite依赖，和原bundled构建合同及同源码真实SQL/VFS用例对应。

这是**实际二进制静态代码/链接溯源＋同源码真实SQL与quota测试**，不是新增node运行时
版本API，也没有注入/调试运行节点。`node_runtime_sql_version_api_invoked=false`保留。
完整方法与逐角色结果见[sqlite-binary-provenance.json](sqlite-binary-provenance.json)。

## 额外并行失败：分类封存，不扩大主网前置

原Linux默认3失败、Windows全feature26失败、Linux全feature27失败仍为失败；
合计56次出现、34个不同用例，不能写成56种独立网络故障。
每条原始错误及原隔离命令/结果一一映射到[分类明细](parallel-classification.json)。

| 类别 | 出现次数 | 已有证据与保留边界 |
| --- | ---: | --- |
| R1 | 20 | 原日志明确carrier端点额度拒绝；上限4未改，缺当时占用者证据 |
| R2 | 10 | 用例直接断言全局available_permits；模块锁未隔离其他消费者，不代表已排除资源泄漏 |
| S1 | 1 | EVM三个测试共享chain_id且其中存在清理peer状态的调用；冲突机制已找到，原发生顺序未取得 |
| T1 | 2 | EVM/RLPx跟进或响应时限未完成；具体调度/IO/逻辑原因未知，插件专属问题独立处理 |
| T2 | 3 | 原有限scope实际过期，未确定过期前的首个原因 |
| T3 | 1 | TLS预期过期却得到stream failed，终止原因仍未确定 |
| U1 | 4 | journal锁实际拒绝；各测试使用独立临时目录，不能归为同名路径冲突 |
| U2 | 11 | oneshot接收者先关闭，未保存对端初始错误，不一律解释成端点额度 |
| U3 | 2 | 未观察到预期Connecting阶段，首个失败点未知 |
| U4 | 2 | mailbox channel closed，缺服务任务初始错误 |

按原CI真实IO隔离方式，对原匹配源码libtest制品逐条启动独立进程，只执行一次原精确
用例、`--test-threads=1`，先确认筛选恰好命中1项，再核对实际1项执行通过。
Windows26/26、Linux30/30通过；原测试二进制SHA和逐例输出见
[Windows隔离结果](isolated-windows.json)、[Linux隔离结果](isolated-linux.json)。
它们不是打包node程序，也不是全并行或四机网络验收。没有提高生产额度/超时、改认证、
删断言或改业务；隔离通过不把原并行失败改成成功，不声明全部根因或线程安全已结案。
当前必需CI已经成功，不为这些额外插件问题重新启动整轮排查或阻碍原生主网交付。

## 原始证据与下一步

[original-candidate-evidence.zip](original-candidate-evidence.zip)：253项、99322字节，
SHA256 `af66dbcd80293c902a5a0b89992e0a201ccad27be196312a356acd5214de97df`。
封存实际构建、包内用例、逐例隔离stdout/stderr/枚举、CI摘要、分类和原脚本/复用脚本快照；
逐成员hash回读验证。脚本是当时执行记录，不是让其他设备直接运行的部署指令。
不含候选二进制、隔离测试EXE、数据库、journal、部署用途钥或私钥正文。
父目录原始并行失败ZIP保持原hash和原结果；新证据不覆盖旧证据。

下一步执行入口见[本版本existing升级与回滚方案](EXISTING-UPGRADE-PLAN.md)：
固定3fa程序**先B、验证成功后A**已获批准并执行；C/D本轮不动，结果见更新回执。
回滚指换回程序并existing打开同一原账，不是恢复旧余额、清库或重创世；旧程序带旧SQLite
安全依赖，回滚只作故障恢复，不代表修复继续生效。
没有新增Object DONE、业务回执、最终性、持续稳定性或TPS结论。
