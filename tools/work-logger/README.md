# SuperVM Work Logger - Python Edition

**无需 Node.js 的纯 Python 自动工作日志系统**

## 📁 目录结构

```
tools/work-logger/         # 完全自包含
├── bin/                   # 可执行脚本
│   ├── start.ps1          # 启动监听器
│   ├── start-silent.ps1   # 静默启动（VS Code 自动调用）
│   ├── stop.ps1           # 停止并生成笔记
│   └── status.ps1         # 查看运行状态
├── lib/                   # Python 核心模块
│   ├── session_manager.py # 会话管理
│   ├── analyzer.py        # 代码分析（模块推断、Git diff）
│   ├── db_writer.py       # 📊 数据库写入（SQLite）
│   ├── index_generator.py # 📚 docs/INDEX.md 自动生成
│   ├── query.py           # 🔍 数据库查询命令
│   ├── watcher.py         # 文件监听主程序
│   └── install.py         # 安装脚本
├── data/                  # 运行时数据（git ignore）
│   ├── current_session.json
│   ├── watcher.pid
│   └── session_*.json
├── output/                # 工作笔记输出
│   └── WORK-NOTE-*.md
├── .gitignore
└── README.md              # 本文档
```

---

## 🚀 快速开始

### **自动启动（推荐）**
打开 VS Code 工作区 → 自动启动监听器（通过 `.vscode/tasks.json`）

### **手动启动**
```powershell
.\tools\work-logger\bin\start.ps1
```

### **查看状态**
```powershell
.\tools\work-logger\bin\status.ps1
```

### **停止并生成笔记**
```powershell
.\tools\work-logger\bin\stop.ps1
```

会提示输入：
1. 今日主要做了什么？（必填）
2. 遇到了什么问题？（可选）
3. 如何解决的？（可选）
4. 与 Copilot 的关键对话？（可选）
5. 下一步计划？（可选）

然后自动生成完整的工作笔记到 `output/` 目录。

---

## ✨ 特性

✅ **完全自包含** - 所有文件在 `tools/work-logger/` 下  
✅ **自动追踪** - 监听文件创建/修改/删除  
✅ **智能分析** - 模块推断、Git diff、行数统计  
✅ **交互式笔记** - 结合文件变更和人工总结  
✅ **目录索引** - docs/INDEX.md 自动更新（覆盖仓库根目录）  
✅ **后台运行** - 不干扰正常工作  
✅ **会话恢复** - 关闭 VS Code 自动保存，重开继续  

---

## 📊 工作笔记格式

```markdown
# Work Note - Session abc123

**Date**: 2026-02-06 14:00:00  
**Duration**: 2h 15m  

## 📝 Work Summary

**今日工作**: 实现了 XXX 功能

### 🔴 遇到的问题
- 问题描述

### ✅ 解决方案
- 解决方法

### 💬 与 Copilot 的关键对话
- 讨论内容1
- 讨论内容2

### 📋 下一步计划
- 待办事项

## 📊 Statistics
| Metric | Value |
|--------|-------|
| Files Changed | 8 |
| Lines Added | 247 |
| Lines Removed | 85 |

## 📂 Files Changed
- ✅ Created: file1.py (+50 lines)
- ✏️ Modified: file2.py (+10 -5 lines)
```

---

## 🔧 依赖

- **Python**: 3.7+
- **watchdog**: 文件系统监听库（自动安装）
- **Git**: 用于 diff 分析

---

## 📦 迁移/分享

整个 `tools/work-logger/` 目录可以：
- 复制到其他项目
- 制作成 Git submodule
- 打包分享给团队

只需更新 `.vscode/tasks.json` 中的路径即可。

---

**作者**: GitHub Copilot + SuperVM Team  
**版本**: 0.2.0 (Self-Contained Edition)  
**日期**: 2026-02-06

## 特性

✅ **零外部依赖** - 仅需 Python 3.7+ 和 watchdog 库  
✅ **实时监控** - 自动追踪所有文件变更  
✅ **会话管理** - 支持开始/结束会话，自动统计  
✅ **智能分析** - 模块推断、Git diff 解析、行数统计  
✅ **数据库存储** - SQLite 高频记录，支持多维查询  
✅ **灵活查询** - 按日期、模块、关键词查询工作历史  
✅ **Git 集成** - Post-commit hook 自动记录  

---

## 快速开始

### 1. 安装

```powershell
cd tools\work-logger
python lib\install.py
```

这会：
- ✅ 检查 Python 版本（需要 3.7+）
- ✅ 检查 Git
- ✅ 安装 watchdog 库
- ✅ 创建 Git hooks
- ✅ 输出启动指引

### 2. 启动监听

```powershell
# 方式 1: PowerShell 脚本
.\tools\work-logger\bin\start.ps1

# 方式 2: 直接运行
python tools\work-logger\lib\watcher.py .
```

### 3. 开始工作

监听器启动后：

```
🚀 SuperVM Work Logger Started
📂 Watching: D:\WorksArea\SUPERVM
�� Session ID: 3a7f2c91
⏱️  Started at: 2026-02-06 16:30:00

==================================================
Press Ctrl+C to end session and generate report

📝 modified: src/lib.rs (+35 -10)
📝 created: test.rs (+12 -0)
⏱️  18s | 2 files | +47 -10 lines
```

### 4. 结束会话

按 **Ctrl+C** 结束：

```
🛑 Stopping logger...

✅ Session 3a7f2c91 completed
📊 Duration: 18m 23s
📂 Files: 3
📝 Work note: tools\work-logger\output\WORK-NOTE-2026-02-06-3a7f2c91.md
```

---

## 架构

```
tools/work-logger/
├── bin/                    # 启动/停止/查询脚本
├── lib/                    # Python 核心
│   ├── watcher.py          # 主程序（文件监听）
│   ├── session_manager.py  # 会话管理（JSON 存储）
│   ├── analyzer.py         # 代码分析（模块推断、Git diff）
│   ├── db_writer.py        # SQLite 写入
│   ├── query.py            # 查询工具
│   └── install.py          # 安装脚本
├── data/                   # 运行时数据
├── mylog/                  # 数据库与文档
└── README.md               # 本文档

数据存储：
.work-logger/               # 会话数据目录
├── current_session.json    # 当前会话
└── session_*.json          # 历史会话

输出：
docs/worklogs/              # 工作笔记
└── WORK-NOTE-*.md
```

---

## 对比 VS Code Extension 版本

| 特性 | Python 版 | Extension 版 |
|------|----------|--------------|
| 安装要求 | Python 3.7+ | Node.js 18+ |
| 启动方式 | 命令行手动 | VS Code 自动 |
| UI 集成 | 无 | 状态栏、命令面板 |
| 文件监听 | watchdog | VS Code API |
| 会话管理 | JSON 文件 | Workspace State |
| Git 集成 | Hooks | 可选自动 commit |
| 跨平台 | ✅ 完整支持 | ✅ 完整支持 |

**推荐使用场景**：
- **Python 版**：无 Node.js 环境、喜欢命令行、CI/CD 集成
- **Extension 版**：重度 VS Code 用户、需要 UI 集成、自动化程度更高

---

## 配置

会话数据存储在 `.work-logger/current_session.json`：

```json
{
  "session_id": "3a7f2c91",
  "start_time": "2026-02-06T16:30:00",
  "file_changes": {
    "src/lib.rs": {
      "type": "modified",
      "lines_added": 35,
      "lines_removed": 10
    }
  }
}
```

---

## 高级用法

### 后台运行（Linux/macOS）

```bash
nohup python tools/work-logger/lib/watcher.py . > /dev/null 2>&1 &
echo $! > tools/work-logger/data/watcher.pid
```

停止：

```bash
kill $(cat tools/work-logger/data/watcher.pid)
```

### 集成到 CI/CD

```yaml
# .github/workflows/work-logger.yml
- name: Track work session
  run: |
    pip install watchdog
    timeout 300 python tools/work-logger/lib/watcher.py . || true
```

---

## 常见问题

### Q: 如何忽略某些文件？

A: 编辑 `watcher.py` 的 `IGNORE_PATTERNS` 列表：

```python
IGNORE_PATTERNS = [
    '.git',
    'node_modules',
    'target',
    'my-temp-dir',  # 添加自定义规则
]
```

### Q: 监听器占用 CPU 过高？

A: 调整刷新间隔（`watcher.py` 第 139 行）：

```python
time.sleep(5)  # 从 2 秒改为 5 秒
```

### Q: 如何导出所有历史会话？

A: 所有会话存储在数据库 `tools/work-logger/mylog/changelog.db` 的 `work_sessions` 表中。使用 query.ps1 命令：

```powershell
# 导出所有会话
.\tools\work-logger\bin\query.ps1 --recent 365

# 或通过 Python 直接查询
python tools\work-logger\lib\query.py --recent 365
```

---

## 📊 数据库存储

所有工作会话自动记录到 SQLite 数据库：

**数据库位置**: `tools/work-logger/mylog/changelog.db`

**表结构**: `work_sessions` 包含以下信息：
- 会话 ID、开始/结束时间、持续时长
- 5 个问题的答案（工作总结、问题、解决方案、Copilot 讨论、下一步）
- 文件变更统计（文件数、新增行数、删除行数）
- 详细的文件列表（JSON 格式）
- 推断的主模块和涉及模块列表

详见 [DATABASE-SCHEMA.md](DATABASE-SCHEMA.md)

---

## 🔍 查询工作记录

### 查询最近 7 天的工作
```powershell
.\tools\work-logger\bin\query.ps1 --recent 7
```

### 按模块查询
```powershell
.\tools\work-logger\bin\query.ps1 --module aoem-core
```

### 搜索关键词
```powershell
.\tools\work-logger\bin\query.ps1 --search "GPU优化"
```

### 查看总体统计
```powershell
.\tools\work-logger\bin\query.ps1 --stats
```

### 导出会话详情
```powershell
.\tools\work-logger\bin\query.ps1 --export session_id
```

### 日报汇总
```powershell
.\tools\work-logger\bin\query.ps1 --daily 30
```

---

## 💥 工作流程

1. **VS Code 启动** → 自动启动监听器（`.vscode/tasks.json`）
2. **编辑文件** → watcher.py 实时检测变更（2秒去重）
3. **工作结束** → 运行 `stop.ps1`
4. **回答5个问题** → 工作总结、遇到的问题、解决方案、Copilot 讨论、下一步计划
5. **数据保存** → db_writer.py 写入 `work_sessions` 表
6. **查询历史** → 使用 query.ps1 查阅过去的工作记录

---

### Q: 如何导出所有历史会话？

A: 所有会话存储在 `.work-logger/session_*.json`：

```powershell
Get-ChildItem .work-logger\session_*.json | ForEach-Object {
    Get-Content $_ | ConvertFrom-Json
}
```

---

## 依赖

- **Python**: 3.7+
- **watchdog**: 文件系统监听库
- **Git**: 用于 diff 分析

安装 watchdog：

```powershell
pip install watchdog
```

---

## 未来计划

- [ ] Changelog.py 集成（自动调用）
- [ ] Webview UI（Flask 服务器）
- [ ] 性能指标追踪
- [ ] 多仓库支持
- [ ] 导出为 CSV/JSON

---

**作者**: GitHub Copilot + SuperVM Team  
**版本**: 0.1.0 (Python Edition)  
**日期**: 2026-02-06
