# Docs Index

> 状态说明（2026-04-17）：
> 本文件为历史自动扫描索引（2026-02-07 生成），不能作为当前生产口径入口。
> 当前权威入口请先看：
> `docs_CN/CURRENT-AUTHORITATIVE-ENTRYPOINT-2026-04-17.md`
> 货币与支付口径决议见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
> 原生交易与执行接口冻结稿见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-TX-AND-EXECUTION-INTERFACE-DESIGN-2026-04-17.md`
> P1 封盘与当前已实现边界见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-PAYMENT-AND-TREASURY-P1-SEAL-2026-04-17.md`
> P2-A 双轨制度冻结稿见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-DUAL-TRACK-SETTLEMENT-AND-MARKET-SYSTEM-P2A-2026-04-17.md`
> P2-A 清算路由封盘见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-CLEARING-ROUTER-P2A-SEAL-2026-04-17.md`
> P2-C Stage2 Treasury policy 封盘见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-STAGE2-SEAL-2026-04-18.md`
> P2-C constrained strategy 封盘见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-CONSTRAINED-STRATEGY-SEAL-2026-04-18.md`
> P2-C 正式封盘见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-SEAL-2026-04-18.md`
> P2-D 可观测层封盘见：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md`
> P3 开关决策规范见（Decision Only / Not Enabled）：
> `docs_CN/NOVOVM-NETWORK/NOVOVM-P3-FEATURE-GATE-DECISION-THRESHOLDS-2026-04-18.md`

## 目录结构

SUPERVM/                                                       # 仓库根目录 (2026-01-27 04:15/2026-02-07 05:13)
├── .github/                                                   # GitHub配置 (2026-02-07 04:35/2026-02-07 06:34)
│   ├── workflows/                                             # CI工作流 (2026-02-07 04:35/2026-02-07 04:36)
│   │   └── ci.yml                                             # 配置 (2026-02-07 04:36/2026-02-07 04:53)
│   └── copilot-instructions.md                                # Copilot指令 (2026-02-07 06:34/2026-02-07 07:02)
├── docs/                                                      # 文档目录 (2026-02-06 05:49/2026-02-07 07:05)
│   ├── temp/                                                  # 临时文档 (2026-02-06 06:07/2026-02-06 06:07)
│   │   └── AOEM-PROPRIETARY-LICENSING-ANALYSIS-2026-02-06.md  # 专有许可证分析 (2026-02-06 06:07/2026-02-06 06:08)
│   └── INDEX-DESCRIPTIONS.md                                  # 索引 (2026-02-07 07:05/2026-02-07 07:08)
├── scripts/                                                   # 脚本目录 (2026-02-06 06:50/2026-02-07 03:55)
├── specs/                                                     # 规格文档 (2026-02-06 06:43/2026-02-06 06:43)
├── tools/                                                     # 工具目录 (2026-02-06 14:36/2026-02-07 03:54)
│   ├── python-tools/                                          # 目录 (2026-02-07 03:54/2026-02-07 03:55)
│   │   ├── add-python-to-path.bat                             # Python PATH 临时配置（批处理） (2026-02-06 07:03/2026-02-06 07:03)
│   │   ├── add-python-to-path.ps1                             # Python PATH 临时配置（PowerShell） (2026-02-06 07:02/2026-02-06 07:02)
│   │   ├── setup-path-permanent.bat                           # Python PATH 永久配置（批处理） (2026-02-06 07:01/2026-02-06 07:01)
│   │   └── setup-path-permanent.ps1                           # Python PATH 永久配置（PowerShell） (2026-02-06 07:01/2026-02-06 07:01)
│   └── work-logger/                                           # 工作日志系统 (2026-02-06 14:36/2026-02-06 22:22)
│       ├── bin/                                               # 命令脚本 (2026-02-06 14:36/2026-02-06 18:17)
│       │   ├── changelog.py                                   # 日志变更记录脚本 (2026-02-06 06:50/2026-02-06 18:35)
│       │   ├── query.ps1                                      # 日志查询命令 (2026-02-06 15:05/2026-02-06 15:48)
│       │   ├── start-silent.ps1                               # 日志后台启动 (2026-02-06 14:23/2026-02-06 15:02)
│       │   ├── start.ps1                                      # 日志启动 (2026-02-06 14:06/2026-02-06 15:02)
│       │   ├── status.ps1                                     # 日志状态查看 (2026-02-06 14:11/2026-02-06 15:02)
│       │   └── stop.ps1                                       # 日志停止 (2026-02-06 14:11/2026-02-06 15:02)
│       ├── lib/                                               # 核心模块 (2026-02-06 14:36/2026-02-07 05:16)
│       │   ├── analyzer.py                                    # 日志变更分析 (2026-02-06 14:04/2026-02-06 14:12)
│       │   ├── db_writer.py                                   # 日志数据库写入 (2026-02-06 15:03/2026-02-06 17:45)
│       │   ├── index_generator.py                             # 日志索引生成 (2026-02-06 19:46/2026-02-07 07:05)
│       │   ├── install.py                                     # 日志安装脚本 (2026-02-06 14:04/2026-02-06 18:35)
│       │   ├── note_generator.py                              # 日志笔记生成 (2026-02-06 14:04/2026-02-06 14:16)
│       │   ├── query.py                                       # 日志查询模块 (2026-02-06 15:04/2026-02-06 17:45)
│       │   ├── session_manager.py                             # 日志会话管理 (2026-02-06 14:04/2026-02-06 15:02)
│       │   └── watcher.py                                     # 日志监听器 (2026-02-06 14:04/2026-02-07 06:13)
│       ├── mylog/                                             # 变更日志数据库 (2026-02-06 06:15/2026-02-06 07:00)
│       │   ├── changelog.db                                   # 变更记录 (2026-02-06 07:00/2026-02-06 07:00)
│       │   ├── DEPLOYMENT-CHECKLIST.md                        # 清单 (2026-02-06 06:52/2026-02-06 18:35)
│       │   ├── examples.py                                    # 脚本 (2026-02-06 06:51/2026-02-06 18:35)
│       │   ├── INDEX.md                                       # 索引 (2026-02-06 06:53/2026-02-06 18:35)
│       │   ├── init-changelog.ps1                             # 变更记录 (2026-02-06 06:55/2026-02-06 06:56)
│       │   ├── init-changelog.py                              # 变更记录 (2026-02-06 06:49/2026-02-06 06:49)
│       │   ├── QUICK-REFERENCE.md                             # 参考 (2026-02-06 06:52/2026-02-06 18:35)
│       │   ├── quickstart.bat                                 # 脚本 (2026-02-06 06:51/2026-02-06 17:42)
│       │   ├── quickstart.ps1                                 # 脚本 (2026-02-06 06:51/2026-02-06 17:42)
│       │   ├── README.md                                      # 项目入口 (2026-02-06 06:50/2026-02-06 18:35)
│       │   ├── schema.sql                                     # 结构 (2026-02-06 06:49/2026-02-06 15:48)
│       │   ├── SETUP-COMPLETE.md                              # 安装完成 (2026-02-06 06:51/2026-02-06 18:35)
│       │   └── SUPERVM-CHANGELOG.md                           # 变更记录 (2026-02-06 06:16/2026-02-06 17:42)
│       ├── output/                                            # 日志输出 (2026-02-06 14:36/2026-02-06 14:37)
│       │   ├── PYTHON-EDITION-SETUP-COMPLETE.md               # Python版安装完成 (2026-02-06 14:06/2026-02-06 18:35)
│       │   ├── README.md                                      # 项目入口 (2026-02-06 13:53/2026-02-06 17:42)
│       │   ├── WORK-NOTE-示例.md                                # 工作笔记 (2026-02-06 14:17/2026-02-06 18:35)
│       │   └── 完整实现文档.md                                      # 文档 (2026-02-06 14:31/2026-02-06 18:35)
│       ├── .gitignore                                         # 文件 (2026-02-06 14:39/2026-02-06 15:02)
│       ├── DATABASE-SCHEMA.md                                 # 数据库结构说明 (2026-02-06 15:02/2026-02-06 17:42)
│       ├── index-descriptions.json                            # 索引 (2026-02-06 22:22/2026-02-07 07:02)
│       ├── MIGRATION-COMPLETE.md                              # 迁移完成报告 (2026-02-06 15:13/2026-02-06 17:42)
│       ├── QUICK-REFERENCE.md                                 # 快速参考卡 (2026-02-06 15:14/2026-02-06 17:42)
│       └── README.md                                          # 工作日志说明 (2026-02-06 14:04/2026-02-06 21:22)
├── .gitignore                                                 # 文件 (2026-02-06 04:56/2026-02-07 06:13)
├── LICENSE                                                    # 授权文件 (2026-02-06 04:56/2026-02-06 05:13)
└── README.md                                                  # 项目入口 (2026-02-06 04:56/2026-02-07 06:31)
