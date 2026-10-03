# SUPERVM 整仓隔离撤销清单

日期：2026-10-03。状态：**用户已批准并执行原产品装配恢复；不代表执行
所有权、主链性能或生产部署已完成。** 本清单落实
[架构纠偏路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md)，不替代执行所有权审计。

恢复原工程，同时保留 10 月 2 日以后全部成果。恢复依据是完整 Git 树，
不是挑选几个“值得保留”的模块，也不是把当前仓库退回旧提交。

## 恢复执行结果

本次变更基线 `face41e3`，装配原件来自 `ee802713`。先在仓内独立验证
worktree 执行，未创建开发分支；核对、构建和测试后以新增纠错提交接回 main。
原 49 项草稿逐文件仍与已保全 SHA256 一致，没有继承到新恢复源码。
清点后增量仅为已推送的清单文档提交，未重启全量备份。

- 原 1,292 个路径全部在原位置有对应文件，未恢复原路径 **0 项**。
  六目录及原配套文件完整恢复，不以模块价值筛选；原归档和后续文件不删除。
  750 个恢复对象中，739 个与 `ee802713` 的 blob 完全一致，11 个必要差异
  逐项列于下表；26 个 Windows 自动换行转换已纠正为原对象，未混入格式重写。
- 原 20 个显式工作区成员恢复，Cargo 另自动识别原有 EVM core，实际 21 个。
  真实 `crates/novovm-node/src/bin/novovm-node.rs` 仍是总入口，exec/bindings
  及原产品依赖接回；不是把 runtime 测试节点改名。
- 原 lock 为装配基准，只为 Relay 的漏接线补入已有 mio/socket2 依赖边，
  未升级依赖版本。Relay 共用 node 已有 I/O 源码，没有重造网络实现。
- runtime 完整保留，新增独立 `Cargo.toml`，其 lock 与 `face41e3` 原根锁一致；
  使用显式 manifest 分别测试两个同名 `novovm-network`。原 proof 独立工程不变。
- CI 恢复原产品检查并保留 runtime 双平台/真实 AOEM 检查。边界脚本检查
  两套批准成员及归档隔离，不再要求整个原产品退出构建。根 Node 项目当前
  不存在，不虚报其已测试；nightly 恢复配置不等于已经执行。

只新增必要写边界限制：旧 Host immediate、batch、pending/tick、通用 Host
mutation，以及候选 Execute/混合 Host barrier 默认拒绝；RPC 参数和旧
ownership 标签不能放行。在加载/锁库/入池或 AOEM 预提交之前拦截。
`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1` 仅用于显式历史对照，不是生产默认；
CI 只给相应历史回归步骤设置它。只读查询、签名后的 pending-only 入口、
EVM 和已有真实 AOEM Transfer 分支未整体禁用。旧已完成产物的读回不在本轮
协议禁用范围内，实际运行账本未触碰，生产仍须全新创世。

具体限制：原 `nov_sendRawTransaction` 默认立即执行会被拒绝，调用者需显式
使用 pending-only；不能因此称默认交易闭环已恢复。统一账户等通过上述
Host mutator 落账的旧政策写操作同样受限，不等于所有 EVM 功能均被关闭。

构建、测试、首次失败与修复及未执行范围见
[本轮验收记录](NOVOVM_PRODUCTION_READINESS_TRACKER.md#原产品装配恢复验收)。

| 恢复原件的有意差异（共 11 个文件） | 原因 |
| --- | --- |
| `Cargo.toml` | 保留原20成员，排除归档及独立组件工程；保留后来的Release溢出检查 |
| `Cargo.lock` | Relay仅新增已有mio/socket2依赖边，无版本更新 |
| `README.md` | 恢复原产品说明，标明恢复范围及当前限制 |
| `.github/workflows/ci.yml` | 原产品覆盖与后续runtime覆盖并存；增加默认拒绝和真实Transfer正向检查 |
| `.github/workflows/mainline-nightly-soak.yml` | 仅旧Host历史对照步骤显式许可，不设置全局许可 |
| `crates/novovm-node/src/tx_ingress.rs` | 旧Host计算/写入路径默认限制，查询和pending-only不整体禁用 |
| `crates/novovm-node/src/native_candidate_execution.rs` | 新Execute/混合Host计算默认限制，保留纯Transfer既有AOEM路径 |
| `crates/novovm-node/src/native_candidate_execution_tests.rs` | 新增真实候选默认拒绝反例，不删旧测试 |
| `crates/novovm-relay/Cargo.toml` | 补共享I/O已有依赖声明 |
| `crates/novovm-relay/src/main.rs` | 接回node已有共享I/O模块，不重写网络 |
| `crates/novovmctl/tests/fixtures/native_nonce_upgrade_authorization_v1.json` | 原有测试入口重签已过期测试证书，不放松生产校验 |

新增Host限制测试文件单独计为新文件；以上不是把本地草稿改名迁入。
恢复差异中的空白检查告警均来自与原blob完全一致的原件，未为消除告警
而改写这些文件。原有runtime/legacy共917受控路径无删除，只有开发规则及
边界检查更新；runtime源码、证明与测试未被本轮替换。

以下清点与保全数据保留其原基线，不改写为本次所有测试已通过的声明。

## 整仓核对结果

隔离前装配基线：`ee8027131bb55d4741248a805e9bc6183235ebcb`。
隔离提交：`161f64d2e8b372d454626069054307b00a34f661`。
本轮现场基线：`main@dc3e4972457594182e4a63b1b400649566dcaaed`，Windows 本机。
清点时远端为 `eaf4f3777cd674764306202a52f9795ef0fe1615`；此处记录清点时点，
不代表发布本清单后的最新 HEAD。

[完整逐路径清单](../artifacts/audit/isolation-undo-20261003-dc3e497-v1/tracked-path-recovery.csv)
覆盖隔离前 **1,292 个受控路径**，无重复、无未解释遗漏；含前后对象 ID、
当前对应位置、计划恢复位置与内容来源、后续修改及本地冲突。
独立只读复核逐行重算通过。以下是汇总，不替代完整清单。

| 隔离变更 | 路径数 | 本次拟处理 |
| --- | ---: | --- |
| 纯搬迁 | 746 | 按原位置完整恢复；原件和草稿不删除、不覆盖 |
| 原件归档而根位置被替换 | 4 | 保存两个版本，逐项恢复原装配关系 |
| 原地修改 | 5 | 撤销过度隔离影响，保留后续有效修改 |
| 隔离未改动 | 537 | 原位置保留当前内容，不回退后续更新 |

六个目录 `crates/`、`scripts/`、`proofs/`、`config/`、`configs/`、`vendor/`
共 743 个文件，隔离前与隔离提交归档的完整 tree ID 相同，当前归档也相同；
见[对象核对](../artifacts/audit/isolation-undo-20261003-dc3e497-v1/identical-trees.json)。
另有 `deny.toml`、nightly 与 native-auth 两份工作流属于纯搬迁。
**相同对象证明原件身份，不证明旧执行逻辑已经正确。**

## 装配与规则的逐项处理

| 文件 | 用户确认后的恢复动作 |
| --- | --- |
| `Cargo.toml` | 恢复原 20 个产品成员及真实 `novovm-node`、`novovm-exec`；保全当前三库装配，不形成第二套默认产品 |
| `Cargo.lock` | 保留两份锁文件，以原工程锁为装配对照，必要依赖调整逐项审核，不丢弃后续 proof 等独立锁文件 |
| `README.md` | 恢复真实产品入口说明，保留当前纠偏状态和后续成果边界 |
| `.github/workflows/ci.yml` | 恢复原产品覆盖，保留后续双平台 AOEM 测试资产；同步纠正 `runtime/check-isolation.ps1` 的 runtime-only 限制，不删除归档误依赖防护 |
| `.cargo/config.toml` | 相对输出目录可保留；无需搬编译缓存，输出目录名不是执行架构缺陷 |
| `AGENTS.md` | 保留最新纠偏规则，明确整树恢复与后续成果保全；不恢复禁止原 crates 的旧指令 |
| `docs/NOVOVM_DELIVERY_ALIGNMENT.md` | 更新当前认领和装配，保留所有后续交接 |
| `docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md` | 追加恢复事实，保留历史测试及失败记录，不提升旧验收结论 |
| `docs/NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md` | 保留后续实现证据，旧 runtime-only 指令继续仅作历史 |

两份纯搬迁工作流同样恢复原位置，但 self-hosted nightly 未运行、host-only
证明关系检查不等于真实 ZK 证明等限制不能因恢复而消失。
原基线 `ee802713` 已包含重建路线文档，不能把其政策原样重新批准。

## 后续成果如何保留而不重做

从隔离提交之后至现场基线共 **33 个提交**；当前另有 **168 个新增路径**，
其中 runtime 165、legacy 说明 2、恢复路线 1，见
[后续路径清单](../artifacts/audit/isolation-undo-20261003-dc3e497-v1/later-additions.json)。
这些统计不替代已有路径的后续 diff，完整 Git 历史同样保留。

先保留全部原件、测试和证据，不在装配恢复阶段筛掉未签收实验。
后续优先复用已有实现及其反例，仅改必要的接口和产品接线：

| 已有成果 | 主要实现来源 | 接回要求 |
| --- | --- | --- |
| V3 鉴权、完整扣费、nonce 与失败回执 | `runtime/novovm-host/src/ingress/`、`business/` | 保留业务结果和拒绝语义，不回退已修缺陷 |
| 常驻 AOEM 计算、有界候选及 I/O 流水线 | `runtime/novovm-aoem/`、host 的 `pipeline/`、`persistence/` | 经原 exec 门面接实际 node，唯一状态 owner |
| 批树更新、认证批读、精确父绑定与只读后态复用 | host 的 `state/`、`pipeline/` | 保留 codec、哈希域及认证测试，不因改目录重写算法 |
| 换轮、防双签、耐久发布、冷恢复 | host 的 `consensus/` | 明确唯一协议和链头，不把新共识直接混旧账本 |
| TLS/WS 双工、唤醒、背压、按需正文与紧凑载体 | `runtime/novovm-network/` | 复用修复，显式处理旧节点和手机协议兼容 |
| 完整 NOV 关系证明、独立验证和 SHA/曲线加速 | `runtime/proofs/nov-transfer/`、AOEM receipt 适配 | 保留证明与拒绝证据，另审主链接线；不是隐私、PQ 或主链 TPS 签收 |

ML-DSA 和经典隐私已有成果也保留，但主要早于隔离，不能误称为这 33 个提交
新完成的能力。AOEM 兄弟仓库不在本轮修改范围。生产仍从全新创世启动，
不制造测试余额迁移工程。

## 本地草稿与备份边界

现场有 **49 项未提交改动**：34 修改、15 未跟踪；legacy 38、runtime 11。
其中 26 个旧受控源已有草稿，另有 4 个原目标位置被替代文件占用。
这些冲突均已标在清单；恢复时不得用旧 blob 覆盖草稿或静默接受草稿。
另一设备的未推送工作尚未核查，必须由该设备同步清点，不能以本机备份替代。

本机保全目录均相对仓库根，**备份载荷不提交 GitHub**：

- `artifacts/audit/isolation-undo-20261003-dc3e497-v1/`：49 份草稿、
  原始补丁、通过验证的全引用 Git bundle、1,479 个当前源码文件和包含本地
  LFS 对象的 1,316 个 Git 元数据文件；2,795 份复制逐一 SHA256 验证。
- `artifacts/audit/full-preservation-20261003-dc3e497-v1/`：用户最初选择全量，
  后要求停止。停止前复制已结束，共 546,471 文件、291,276,514,761 字节
  （约 271.3 GiB）；随后停止全量哈希，不删除已复制内容。
  改为核验必要成果：239,419 文件、80,556,652,293 字节通过源/副本 SHA256。
  `essential-summary.json`、`essential-scope.csv`、`essential-sha256.csv`
  记录结果；`not-rehashed.csv` 明确未重验的编译输出和此前已核验备份副本。

必要校验包括编译输出子树之外的冻结源码、SDK、日志、证明、数据和所有
其他原现场文件。编译输出已复制但未完成全量哈希；“未重验”不授权删除。
Git 的 `FETCH_HEAD` 时间戳在期间刷新一次，内容重新核对仍相同。
这些是**同盘内容副本，不是异机灾备或协调停机后的数据库一致性快照**。
初期 `summary.json` 的排除项描述初期最小保全，不是最终保全状态。

## 确认后的边界

批准本清单后，在独立恢复验证目录核对整仓完整性，再以新增纠错提交接回
当前 main；不创建开发分支，不 hard reset、不盲目整笔 revert，不删除
runtime 或归档。每个原受控路径必须有解释，每项后续成果必须保全可追溯。

恢复原件不允许旧 Host 兼容写路径成为生产默认：需检查直接 Host
dispatch/load/clone/save、旧批入口及其 RPC 路由，不能只打开 ownership
标志便称已修复，也不能连带禁掉 EVM、只读查询、网络和已有正确 AOEM 分支。
这项生产边界检查不要求先完成半年历史审计。

本阶段只验收原工程装配和入口。业务执行所有权、统一 CPU/GPU 语义、
最终确认吞吐、隐私、抗量子、多机及部署分别验收；恢复前清点不含构建，
恢复后的实际测试结果以上方执行结果及台账为准。
