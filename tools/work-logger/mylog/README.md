# SQLite Changelog 使用指南

> SuperVM 修改日志管理系统
> 
> 功能：自动记录所有修改，支持灵活查询、统计、导出

---

## 快速开始

### 1️⃣ 初始化数据库（仅需一次）

```bash
cd tools/work-logger/mylog
python init-changelog.py
```

**输出**：
```
✅ Database initialized: changelog.db
   Schema: schema.sql
```

### 2️⃣ 记录一次修改

```bash
python ../bin/changelog.py add \
  --date 2026-02-06 \
  --time 14:30 \
  --version 0.5.0 \
  --level L0 \
  --module aoem-core \
  --property 测试 \
  --desc "修复并发控制 bug，提升 TPS 5%" \
  --conclusion "已验证，通过 454 个单元测试" \
  --files aoem/crates/core/aoem-core/src/lib.rs aoem/crates/tests/aoem-engine-smoke-tests/...
```

**输出**：
```
✅ Added: 2026-02-06 14:30 | aoem-core | 测试
```

### 3️⃣ 查询修改记录

```bash
# 查询 aoem-core 的所有修改
python ../bin/changelog.py query --module aoem-core

# 查询最近一周的生产封盘改动
python ../bin/changelog.py query --property 生产封盘 --since 2026-02-01

# 查询 L0 层的所有改动
python ../bin/changelog.py query --level L0

# 导出 JSON 格式
python ../bin/changelog.py query --module aoem-core --format json
```

### 4️⃣ 导出报告

```bash
# 生成 Markdown 表格
python ../bin/changelog.py export --format markdown --output SUPERVM-CHANGELOG.md

# 生成 CSV（便于 Excel 分析）
python ../bin/changelog.py export --format csv --output changelog.csv

# 生成 JSON（便于自动化处理）
python ../bin/changelog.py export --format json --output changelog.json
```

### 5️⃣ 查看统计信息

```bash
# 总体统计
python ../bin/changelog.py stats

# 按模块统计
python ../bin/changelog.py stats --by-module

# 按属性统计
python ../bin/changelog.py stats --by-property

# 同时按模块和属性统计
python ../bin/changelog.py stats --by-module --by-property
```

### 6️⃣ 查看注册的模块和属性

```bash
# 列出所有可用模块
python ../bin/changelog.py list-modules

# 列出所有可用属性
python ../bin/changelog.py list-properties
```

---

## 详细用法

### add 命令（记录修改）

**必需参数**：
- `--date` 日期（YYYY-MM-DD）
- `--time` 时间（HH:MM）
- `--version` 版本号（如 0.5.0）
- `--level` 架构层级（L0, L1, L2, L3, L4）
- `--module` 模块名（如 aoem-core, vm-runtime）
- `--property` 属性（阶段封盘, 生产封盘, 测试, 实验, 验证, 修复, 文档）
- `--desc` 修改描述（1-3 句话）
- `--conclusion` 结论（说明结果、性能提升或问题）
- `--files` 涉及的文件列表（可选，空格分隔）

**示例**：
```bash
python ../bin/changelog.py add \
  --date "$(date +%Y-%m-%d)" \
  --time "$(date +%H:%M)" \
  --version 0.5.0 \
  --level L1 \
  --module gpu-executor \
  --property 验证 \
  --desc "GPU MSM 性能基准：512+ 点自动 GPU 路由，<512 点 CPU 处理" \
  --conclusion "性能基线稳定，无回归；GPU 失败自动降级 CPU" \
  --files src/gpu-executor/src/lib.rs src/gpu-executor/src/msm.rs
```

### query 命令（查询修改）

**查询参数**（都是可选，支持组合）：
- `--module` 筛选模块
- `--property` 筛选属性
- `--level` 筛选架构层级
- `--version` 筛选版本
- `--since` 起始日期
- `--until` 截止日期
- `--limit` 返回最多条数（默认 50）
- `--format` 输出格式（table 或 json，默认 table）

**查询示例**：

```bash
# 查询 2026-02-01 以后的所有 L0 改动
python ../bin/changelog.py query --level L0 --since 2026-02-01

# 查询所有生产封盘的改动（用于发布清单）
python ../bin/changelog.py query --property 生产封盘

# 查询特定版本的修改
python ../bin/changelog.py query --version 0.5.0

# 查询多个模块的改动（需要多次调用或脚本处理）
for module in aoem-core aoem-engine vm-runtime; do
  python ../bin/changelog.py query --module $module --limit 10
done

# JSON 格式用于自动化处理
python ../bin/changelog.py query --module aoem-core --format json | jq '.[] | select(.property == "生产封盘")'
```

### export 命令（导出报告）

**格式**：
- `markdown`：生成表格，可直接复制到文档
- `csv`：用 Excel/Google Sheets 打开，便于分析
- `json`：用于自动化处理、API 集成

**导出示例**：
```bash
# 导出完整的 Markdown 表格
python ../bin/changelog.py export --format markdown \
  --output SUPERVM-CHANGELOG.md

# 导出到当前目录
python ../bin/changelog.py export --format csv --output changelog.csv

# 导出到其他位置
python ../bin/changelog.py export --format json --output ../../reports/changelog.json
```

### stats 命令（统计分析）

```bash
# 显示总体统计 + 最近 5 条
python ../bin/changelog.py stats

# 按模块统计（用于了解各模块改动频率）
python ../bin/changelog.py stats --by-module

# 按属性统计（用于了解各阶段工作分布）
python ../bin/changelog.py stats --by-property

# 同时统计（综合分析）
python ../bin/changelog.py stats --by-module --by-property
```

---

## 工作流建议

### 每次对话结束时记录

```bash
# 1. 立即记录本次修改（在 Copilot 对话结束后）
python tools/work-logger/bin/changelog.py add \
  --date "$(date +%Y-%m-%d)" \
  --time "$(date +%H:%M)" \
  --version 0.5.0 \
  --level L0 \
  --module 你修改的模块 \
  --property 修改的属性 \
  --desc "简述修改内容" \
  --conclusion "本次结果" \
  --files 修改的文件列表
```

### 周报或月报时导出

```bash
# 导出 Markdown 用于周报
python tools/work-logger/bin/changelog.py export --format markdown --output weekly-report.md

# 导出 CSV 用于 Excel 分析
python tools/work-logger/bin/changelog.py export --format csv --output monthly-analysis.csv

# 导出特定时间范围
python tools/work-logger/bin/changelog.py query --since 2026-02-01 --until 2026-02-06 \
  --format table
```

### 发布前检查

```bash
# 检查所有生产封盘的项
python tools/work-logger/bin/changelog.py query --property 生产封盘

# 统计本版本的改动数
python tools/work-logger/bin/changelog.py query --version 0.5.0 | wc -l

# 导出发布清单
python tools/work-logger/bin/changelog.py query --property 生产封盘 \
  --version 0.5.0 \
  --format markdown > RELEASE-NOTES.md
```

---

## 模块和属性列表

### 注册的模块（可用 --module 值）

运行以查看完整列表：
```bash
python ../bin/changelog.py list-modules
```

**常用模块**：
- `aoem-core` - AOEM 核心并发控制
- `aoem-engine` - AOEM 执行入口
- `aoem-backend-gpu` - GPU 后端
- `vm-runtime` - SuperVM 运行时
- `gpu-executor` - GPU 执行器
- `domain-registry` - 域名系统
- `defi-core` - DeFi 模块
- `许可证` - 许可证相关
- `文档` - 文档更新

### 注册的属性（可用 --property 值）

运行以查看完整列表：
```bash
python ../bin/changelog.py list-properties
```

**属性说明**：
- `阶段封盘` - 阶段性里程碑完成，代码冻结等待发布
- `生产封盘` - 生产就绪，已审计，不接受大改动
- `测试` - 功能测试阶段
- `实验` - 实验性特性，可能改动大
- `验证` - 性能/正确性验证中
- `修复` - bug 修复
- `文档` - 文档补充/更新

---

## 常见问题

### Q1: 如何修改已记录的条目？

当前系统设计为**只追加、不修改**（类似 Git 日志）。如需修改，请：

```bash
# 1. 重置数据库（危险！仅在测试时用）
python init-changelog.py --reset

# 2. 重新初始化
python init-changelog.py

# 3. 重新记录所有条目
```

或者直接编辑 SQLite：
```bash
sqlite3 changelog.db
sqlite> DELETE FROM changelog WHERE id = 1;
sqlite> .quit
```

### Q2: 如何备份数据库？

```bash
# 定期备份
cp changelog.db changelog-backup-$(date +%Y%m%d-%H%M%S).db

# 导出为 JSON（便于版本控制）
python ../bin/changelog.py export --format json --output changelog-backup.json
```

### Q3: 如何用 Excel 分析数据？

```bash
# 导出为 CSV
python ../bin/changelog.py export --format csv --output changelog.csv

# 用 Excel 打开 changelog.csv
# 然后用数据透视表、图表等分析
```

### Q4: 如何与 Git 集成？

```bash
# 每次提交前，自动导出 Markdown（可选）
python ../bin/changelog.py export --format markdown \
  --output tools/work-logger/mylog/SUPERVM-CHANGELOG.md

# 然后 git add + commit
git add tools/work-logger/mylog/SUPERVM-CHANGELOG.md changelog.db
git commit -m "docs: update changelog"
```

### Q5: 多人协作时的冲突？

SQLite 有简单的并发控制，但如果多人同时修改可能产生冲突。建议：

1. **定期 export** 为 JSON 或 CSV 备份
2. **指定一个人** 负责记录（或用脚本自动化）
3. **定期 merge** 各人的改动

---

## 故障排除

### 错误：Database not found

**原因**：未运行初始化脚本

**解决**：
```bash
python init-changelog.py
```

### 错误：UNIQUE constraint failed

**原因**：同一时刻对同一模块有重复记录

**解决**：修改时间或模块名
```bash
# 改成 14:31 而不是 14:30
python ../bin/changelog.py add ... --time 14:31 ...
```

### 错误：Module/Property not recognized

**原因**：使用了未注册的模块或属性

**解决**：查看注册列表并使用有效值
```bash
python ../bin/changelog.py list-modules
python ../bin/changelog.py list-properties
```

---

## 文件说明

```
tools/work-logger/mylog/
├── schema.sql              # SQLite 数据库 schema 定义
├── init-changelog.py       # 初始化脚本（仅需运行一次）
├── changelog.db            # SQLite 数据库文件（自动创建，勿删除）
├── SUPERVM-CHANGELOG.md    # 导出的 Markdown 视图（自动更新）
└── README.md               # 本文档

scripts/
└── changelog.py            # 主 CLI 工具
```

---

**最后更新**: 2026-02-06

