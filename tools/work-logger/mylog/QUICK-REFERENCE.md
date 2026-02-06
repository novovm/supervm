# SQLite Changelog - 快速参考卡

## 初始化（首次使用）

```bash
cd tools/work-logger/mylog
python init-changelog.py        # 创建 changelog.db 数据库
```

## 常用命令

### 📝 添加记录
```bash
cd tools/work-logger/mylog
python ../bin/changelog.py add \
  --date 2026-02-06 \
  --time 14:30 \
  --version 0.5.0 \
  --level L0 \
  --module aoem-core \
  --property 测试 \
  --desc "修改描述" \
  --conclusion "结论" \
  --files file1.rs file2.rs
```

### 🔍 查询
```bash
# 按模块查询
python ../bin/changelog.py query --module aoem-core

# 按时间范围查询
python ../bin/changelog.py query --since 2026-02-01 --until 2026-02-07

# 按属性查询（生产封盘）
python ../bin/changelog.py query --property 生产封盘

# 按架构层级查询
python ../bin/changelog.py query --level L0

# JSON 格式输出
python ../bin/changelog.py query --module aoem-core --format json
```

### 📊 导出
```bash
# 导出 Markdown（用于报告）
python ../bin/changelog.py export --format markdown --output report.md

# 导出 CSV（用于 Excel 分析）
python ../bin/changelog.py export --format csv --output report.csv

# 导出 JSON（用于自动化处理）
python ../bin/changelog.py export --format json --output report.json
```

### 📈 统计
```bash
# 总体统计 + 最近 5 条
python ../bin/changelog.py stats

# 按模块统计
python ../bin/changelog.py stats --by-module

# 按属性统计
python ../bin/changelog.py stats --by-property

# 同时统计
python ../bin/changelog.py stats --by-module --by-property
```

### 📋 列表
```bash
# 显示可用的模块
python ../bin/changelog.py list-modules

# 显示可用的属性
python ../bin/changelog.py list-properties
```

---

## 属性值（--property）

```
阶段封盘    - 阶段性完成，代码冻结
生产封盘    - 生产就绪，已审计
测试        - 功能测试中
实验        - 实验特性
验证        - 性能/正确性验证
修复        - bug 修复
文档        - 文档更新
```

## 架构层级（--level）

```
L0  - 核心（vm-runtime, aoem-core）
L1  - 内核扩展（gpu-executor, l2-executor）
L2  - L2 应用
L3  - L3 应用
L4  - 网络/应用
```

## 常见模块（--module）

```
aoem-core       - AOEM 核心
aoem-engine     - AOEM 执行入口
aoem-backend-gpu - GPU 后端
vm-runtime      - SuperVM 运行时
gpu-executor    - GPU 执行器
domain-registry - 域名系统
defi-core       - DeFi 模块
web3-storage    - Web3 存储
network         - 网络模块
consensus       - 共识模块
许可证          - LICENSE 和许可证
文档            - 文档更新
```

---

## 工作流示例

### 每次对话结束后记录（1 条命令）

```bash
python tools/work-logger/bin/changelog.py add \
  --date $(date +%Y-%m-%d) \
  --time $(date +%H:%M) \
  --version 0.5.0 \
  --level L0 \
  --module aoem-core \
  --property 测试 \
  --desc "本次修改的描述" \
  --conclusion "本次结论" \
  --files file1.rs file2.rs
```

### 周报生成（1 条命令）

```bash
python tools/work-logger/bin/changelog.py export \
  --format markdown \
  --output weekly-$(date +%Y%m%d).md
```

### 月度统计（1 条命令）

```bash
python tools/work-logger/bin/changelog.py stats --by-module --by-property
```

### 发布清单（1 条命令）

```bash
python tools/work-logger/bin/changelog.py query --property 生产封盘 --format markdown
```

---

## 文件位置参考

```
tools/work-logger/mylog/
├── changelog.db              # ← SQLite 数据库（核心，勿删）
├── schema.sql                # ← 数据库结构定义
├── init-changelog.py         # ← 初始化脚本
├── README.md                 # ← 完整文档
├── SETUP-COMPLETE.md         # ← 部署说明
├── QUICK-REFERENCE.md        # ← 本文件
├── examples.py               # ← 使用示例
├── quickstart.bat            # ← Windows 快速启动
└── quickstart.ps1            # ← PowerShell 快速启动

tools/work-logger/bin/
└── changelog.py              # ← 主 CLI 工具

SUPERVM-CHANGELOG.md          # ← 导出的表格（自动更新）
```

---

**提示**: 把此文件加入浏览器书签或 IDE 收藏，方便快速查阅。

最后更新: 2026-02-06
