# 主节点共同候选本地执行 V1

用途：操作者明确选择同一份候选计划，让各节点在独立的 AOEM 状态库中执行，
避免普通待处理池因到达顺序、分批时机或本机时间不同而形成不同候选。

这是**一次性本地执行命令**：推进本地未封印执行头，然后退出。不是远端提案
接收器、隔离分叉执行、自动同步、共识投票或最终确认。不要把不可信远端计划
直接交给此入口；不能同时让正常节点和本命令写同一套数据目录。

## 输入与开启

输入是 `NovNativeCandidateExecutionPlanV1` 的严格 JSON：执行上下文（链、高度、
父块、slot、时间）、协议配置承诺、执行前状态根、AOEM 父状态、确定顺序的原始
签名交易和交易 hash，以及计划承诺。它不包含可选择状态库的路径或私钥。
计划须由现有本地 Host API 生成；本切片不增加未经鉴权的 RPC 或网络提案入口，
也不提供自动生成/分发主网创世计划的工具。

保留已确认的本机 AOEM、持久化路径、链 ID 和协议配置 pin，然后明确设置：

```powershell
$env:NOVOVM_NODE_MODE = 'native_candidate_execute'
$env:NOVOVM_NATIVE_CANDIDATE_PLAN_PATH = '.\node-config\candidate-plan.json'
$env:NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT = '<计划自身的64位小写十六进制承诺>'
$env:NOVOVM_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE = 'true'
# 还须保留本机已核对的 NOVOVM_NATIVE_PROTOCOL_CONFIG_EXPECTED_COMMITMENT。
# 使用本机已有的 novovm-node 可执行文件启动；相对路径以启动目录为基准。
```

计划承诺是计划规范编码的承诺，**不是 JSON 文件字节的 SHA256**。两台机器
可以有不同的数据库路径/namespace，不应复制另一台节点的状态库冒充独立执行。
除这两个候选环境变量外，不新增硬编码盘符或统一父目录要求。

安全边界：

- 只给计划路径、没有专用模式/计划 pin，拒绝；查询 override 也拒绝。
- 不开启网络、RPC、Overlay 或签名服务；不能与启用的 seal 服务同时运行。
- 文件必须是普通文件，读取上限 16 MiB；计划本身继续执行既有 2 MiB 正文等边界。
- 计划结构、承诺 pin、链域、AOEM 所有权开关和协议配置在恢复写入前检查。
- 随后执行既有启动恢复、原生交易鉴权/nonce、父状态和确定性执行检查。
  恢复可能修复已有状态；不能把执行报错理解为整个命令从未写盘。
- 成功重复同一计划走幂等回放，不重复执行；不同计划不能覆盖已提交高度。

标准输出是完整执行 JSON，包含 `candidate_execution_plan_commitment`、
`candidate_execution_plan_source=explicit_local_host_input` 和
`durable_block_candidate_committed`。成功退出表示本地命令完成，不代表取得 QC。
`proof_sealed/safe/finalized` 仍为 false。

## 验证与剩余工作

真实主节点进程测试先通过正常入口生成一个签名交易候选，再只提取候选输入。
两个新节点使用各自空目录与不同 namespace 独立运行真实 AOEM；逐项比较完整
持久区块、成功业务回执、交易/回执索引，再重启回放验证不重复执行与头不变。
测试使用本机进程，未复制数据库，不是实体局域网或公网验证。
错误模式、查询覆盖、错误指纹、错误链域、畸形/超大文件均拒绝；重新计算外层
计划承诺后的篡改签名交易也拒绝，不形成执行头或 prepared 候选。

2026-09-26 本机：本专项 3/3、既有封印启动 CLI 4/4、流水线 32/32 通过；
node/novovmctl 全目标 Clippy -D warnings 及格式检查通过。没有重跑完整主线门禁，
上一刀记录的本机 EVM reorg 测试问题仍未在本切片修复。

```sh
cargo test -p novovm-node --test native_candidate_node_cli -- --test-threads=1
```

该测试已加入 mainline gate。后续已另行完成[真实主节点封印联调](NOVOVM_NATIVE_SEAL_MAIN_PROCESS_V1.md)，
覆盖 prepare QC 与正常重启恢复；该需要回环 443 的专项默认不运行。
连续高度、最终确认及权威状态晋升仍不能由本切片推出。AOEM 内核/DLL 和现有聊天服务未改动。
