# 原主链 ML-DSA-65 签票、用途密钥与固定 NOV 供应

输入 `12c25e4c`，开发期间快进同步经济讨论稿 `66e652d2`；实现版本为包含本报告的提交。
环境：Windows x86_64，Rust/Cargo 1.94.0，debug；本报告不是性能、Linux 安装或真实四机资格。
用户要求完整正式实现演练一个月，再经授权以新域重新创世；不建设简化测试产品。
设计与待批参数见[完整资金闭环设计](../../../docs/NOVOVM-ECONOMICS/NOV-GENESIS-AND-FUNDED-SERVICE-CLOSURE-DESIGN-2026-10-25.md)。

## 1. 实际接线与替换

- 原 `novovm-node/native_pipeline/service` → 原 `round_bft` journal/wire →
  `novovm-exec::pq_signature::MldsaRuntime` → `aoem-bindings` → 同一 AOEM。
  不新增共识、执行器、账本、密码实现或 HTTP 入口。原 CPU/整数代数执行与唯一发布 owner 保留。
- 提案、投票、QC 使用 `NOVRBFT2`、版本 2、套件 65，公钥 1952、签名 3309 字节。
  对完整规范原文签名，固定 context `NOVOVM/consensus/ML-DSA-65/v2`；
  external-pure framing 只做一次，不将预哈希称作 pure。KAT 在资源构造时执行，不逐票重复。
- 原 persist-before-emit 不变；日志 `NVSIGN03` / `NVOUT003`、v3 digest 与独立元数据域
  拒绝旧票、旧日志和错误套件。不降低 3/4 门槛，不清空旧域签票日志或复制数据库。
- 原受保护用途 custody 改为 `NVPK0002`：验证者 ML-DSA-65、传输独立 Ed25519；
  原 DPAPI/Linux owner 继续持有密钥。签验使用同一实际 AOEM；秘钥缓冲区按原 FFI 合同释放，
  正常宽度 secret 释放前擦除，失败路径也释放两份输出。没有 DLL 裸传 Rust 容器。
- 原 node/ctl 启动只接受 protected custody，完整路由绑定、配对验证、管理批准及三 pin
  检查先于开库。共识测试 verifier 明确带 test-only 标记，生产启动拒绝；失败不降级 Ed。
- 创世必须准确发行 `100000000000000000` 最小单位，即 1 亿 NOV、9 位精度。
  原共同 staging 比较同父状态的去重账户余额与最终输出，再计真实费用国库一次；
  授权预算、费用分桶、累计成交/诊断不重复当资产。只验结果完整性，不重算业务、修补结果。
  该守恒门覆盖当前 Transfer/费用，不能宣称未实现的锁、预留、应付已经接通。
- 原 JSON **操作配置**上限改为 16 MiB；64 成员客户端只读配置 2 MiB。
  有界读取 `take(limit+1)`，超过拒绝；规范初始化 1 MiB、交易对象 512 KiB、健康输出
  64 KiB 不变。这不是将 APFL 交易或 AOEM 热路径改为 JSON。

## 2. 已完成本机验证

日志在本目录 `logs/`；通过、失败与忽略分别保留，不能只取最好结果。
日志保留命令原始输出，包括结尾空行及旧格式差异中的空白；源码/文档的
`git diff --check` 单独检查，不修改失败日志来消除其空白提示。

| 检查 | 本机结果及边界 |
| --- | --- |
| 原 round_bft | 89 通过；组件 fixture 非真实密码资格 |
| 原 native_pipeline | 440 通过、69 显式忽略；不等于全 node lib 通过 |
| 原 novovmctl | 61 单元、9 创世 CLI、5 用途 CLI、9 nonce CLI 通过；真实库项另跑 |
| 实际 AOEM 创世批准/反例 | 1 通过；只读编译/批准检查，不激活新链 |
| 实际用途 CLI keygen/inspect/拒绝 | 1 通过；临时测试钥，不是 A–D 部署身份 |
| 原受保护启动/三 pin/冷开 | 2 通过 |
| 实际 AOEM owned runtime | 1 通过，含外部门面释放后的资源寿命与上下文反例 |
| 真实 UCA CPU/完整整数结果等价 | 1 通过；不等于性能或全部 GPU 业务资格 |
| 四原服务/四库 UCA | 1 通过；另有下述独立进程测试，不能混称多机 |
| bindings | 31 通过、8 忽略，含 keygen 双输出失败清理与 secret 擦除顺序 |
| exec pq_signature | 13 通过、1 忽略；实际 owned runtime 另跑 |
| purpose custody | 2 通过 |
| Linux 打包脚本夹具（Windows 执行） | 28 安全条件、构建产物拒绝、11 程序合成装配通过；不是 Linux 安装 |

最大形状 fixture：64 验证者、32 UCA × 8 花费授权，规范字节 585065；pretty manifest
11497570，含 peers 的配置 3515458，连初始化配置 15844382 字节，均在相应文本预算内。
仅尺寸 fixture 使用小整数字节，独立全宽 u128 保真测试保留；未放宽 canonical 或交易预算。

### 真实独立进程检查点

`checkpoint-uca-recovery-report.json`：四个当前原节点进程、同一台 Windows、零 HTTP。
三个健康节点推进 40 高度；第四节点 115226 ms 后启动，在耐久高度 1 强杀且 h40
归档仍不存在；原库重启追平、收款后再次消费，最终四方 h41，冷重开一致。
本测试记录 144244 ms，测试 harness 144.53 秒；不是 TPS。
实际 node SHA-256 `1f481097b2a26e4c3371b05264147fdd823404ed2493a2010d092e10874b0a83`，
native-client `395a23dc4444f54da591f8aa6fe375664b56c8c79964b129f26e4985066fb1f3`。

`checkpoint-native-aab-report.json`：三个真实诚实进程与一个故障 peer，实际 ML-DSA-65
签票、A/A/B → nil/换轮 → 三诚实签名 QC → 原库重启后继，13.28 秒通过。
每个归档状态根都核对固定供应 fixture 的储备账户。该 V3 业务兼容回归不冒充 NUAS。
最终重建的程序与复验身份在 `evidence.json`，不能把检查点身份混为最终构建。

两者 AOEM 均为随仓 `aoem/windows/core/bin/aoem_ffi.dll`，SHA-256
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`；本轮未改 AOEM 库。

### 删除旧诊断接线后的最终制品复验

交付身份以 `evidence.json` 及 `final-*.json` 为准：node SHA-256
`730c16538b4b104ef9e19876077b41889dd6ccae328d6b35ebdef1be8a9c781e`，
native-client `d1ba39a9aa79128ba4e16c4987214563ebecf7060cdb4547f59abf326b24ec81`。
`pre-cleanup-*.json` 只保留删除旧适配之前的中间构建，不混作最终程序身份。

最终 NUAS 四进程：健康三方 h40，第四方 113484 ms 后启动；追赶 h1 时强杀且
h40 归档尚不存在；原库恢复、到账再消费后四方 h41，冷开一致，固定三票门槛不变。
报告 142596 ms，harness 142.86 秒；A/A/B 独立进程复验 13.19 秒通过。
费用记录为 41 次结算、1845 最小单位，分桶 1271/369/205；分桶不是额外供应。
无 HTTP、无数据库复制、无钱包向落后端重灌交易；没有据此报告 TPS。

最终 `round_bft_candidate` 两个降级反例、440 项 native_pipeline、六 crate
全目标严格 Clippy 和 `cargo fmt --all -- --check` 均通过。格式检查退出 0、无输出，
命令结果记入 `evidence.json`，不制造一份不存在的输出日志。

## 3. 本轮失败、修正与仍未通过项

- 早期流水线 8 个失败来自旧固定供应/旧日志 fixture；修正真实测试输入和日志定位后复验。
- 首次实际 NUAS 失败：客户端配置仍限 64 KiB，无法容纳四个 ML-DSA 公钥。改原配置
  读取为有界 2 MiB，不放大查询输出/网络消息，重建 node 与客户端后复验通过。
- A/A/B 两次失败是原 fixture 的储备账户白名单遗漏及 `0x` 表示差异。改用原
  `complete_test_supply` 的真实格式，新增遗漏/重复/金额篡改反例，真实进程复验通过。
- CLI 新尺寸 fixture 先遇到编译/类型问题，随后旧 1 MiB oversize 断言失效；改成共享
  `MAX_STARTUP_DOCUMENT_BYTES+1`，超过新界限仍拒绝，不删除断言。
- 严格 Clippy 初跑报 12 处无必要 `to_vec`，修正 fixture 调用而非降低警告级别。
- keygen 清理 fixture 初跑的整数字面量被推断为 i32，导致预期字节宽度错误；
  明确为 u8 后 bindings 31 项通过，原失败日志保留；没有更改实际密码宽度。
- 最终 NUAS 第一次启动命令遗漏 `NOVOVM_AOEM_TEST_LIBRARY`，测试在创建子进程前
  拒绝。补齐与前轮相同的显式随包库路径后复验通过；保留该失败日志，不称为链运行失败。
- 尝试无筛选并行 `cargo test -p novovm-node --lib` 时，大量历史用例失败且剩余用例
  长时间运行，已只终止本轮确认的测试进程；保留完整未完成日志，不宣称全库通过。
  单独复现 `mainline_query_neth_to_nov_swap...` 和 `...rejects_legacy_execute_outside_json_domain`
  均因默认禁止旧 Host 路径而失败，按原历史比较 `NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1`
  各自串行通过。这只解释已抽查两项，不把所有未逐项归因失败都称为已解决。
  正式 native/NUAS 验收仍显式设 0，后续 CI 继续按既有分组条件执行；未调整生产默认值。
- 独立 `proofs/native-auth` 的两条 zeroize 依赖锁条目同步；未升级锁中的版本。
  本机 Windows `RISC0_SKIP_BUILD=1 ... --workspace --locked` 在 methods build-script
  链接失败：`sys_alloc_aligned` 未解析。测试尚未开始，**不算通过**；保留原日志，
  Linux 专项 CI 仍需独立验证。不重造证明服务或移除 `--locked`。

## 4. 旧接线清理映射

`native_round_bft.rs` 是旧 Ed32 创世的接入前只读诊断，源码检索无产品调用，只有旧测试。
其配置无法安全转换成新 ML-DSA 成员，因此删除整个文件及模块导出，连同旧
workspace reader、statement/compiler、候选 wrapper 与手签实现；不留旧函数报错空壳。
CI 原 `round_bft_candidate` 筛选保留两个当前 v2 降级反例：旧公钥/签名长度、
缺失 verifier、旧 magic/version 及错误套件。它们不是旧接口或真实密码运行证据。

实际正向/反例由原路径承担：`native_pipeline/consensus/chain_integration.rs`
候选/发布/损坏/恢复，`consensus/tests.rs` 耐久签票和陈旧 owner，
`native_product_uca_recovery.rs` 真实签票/追赶/再消费；规范消息篡改和票数不足由
`novovm-consensus/round_bft/{wire/tests,decision_tests}.rs` 承担。历史证据和 Git 不删除。

## 5. 发布边界与下一项完整资金闭环

未启动 A–D 新链、未分配部署 NOV、未改 B 服务、防火墙或手机数据；未删除旧链数据。
未取得真实多机、Linux 安装、一个月长跑或新签名 TPS；旧 Ed 性能不能继承。
传输/旧服务回执仍独立 Ed25519，不宣称通信密钥协商或全节点全面抗量子。
用途锁/释放、真实服务资金预留/核销/指定 UCA 到账再花费、动态授权治理、
钱包储蓄/零钱投影、隐私/完整业务证明、移动制品仍有未完成集成。
候选 35/20/20/10/10/5、80/10/10 和锁期未获批准，不写默认值。
测试使用完整供应的隔离 fixture 只验证机制；不构成实际资金分配批准。
正式根和签名域仍由原编译器根据获批清单生成，不填假根、不先上线旧经济再整体更换。
