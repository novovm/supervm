# Work Logger 数据库设计文档

**最后更新**: 2026-02-06  
**数据库位置**: `tools/work-logger/mylog/changelog.db` (SQLite)  
**功能**: 记录日常工作笔记、文件变更、模块推断

---

## 📊 work_sessions 表完整定义

### 表结构 SQL

```sql
CREATE TABLE IF NOT EXISTS work_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT UNIQUE NOT NULL,
    start_time TIMESTAMP NOT NULL,
    end_time TIMESTAMP,
    duration_seconds INTEGER,
    
    -- 工作笔记（5个问题）
    work_summary TEXT NOT NULL,
    problems TEXT,
    solutions TEXT,
    chat_summary TEXT,
    next_steps TEXT,
    
    -- 文件变更统计
    files_changed INTEGER,
    lines_added INTEGER,
    lines_deleted INTEGER,
    file_details TEXT,              -- JSON: 详细见下表
    
    -- 推断上下文
    primary_module TEXT,
    modules_touched TEXT,           -- JSON: ["aoem-core", "gpu-executor", ...]
    
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 优化查询索引
CREATE INDEX IF NOT EXISTS idx_work_sessions_session_id ON work_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_work_sessions_date ON work_sessions(DATE(start_time));
CREATE INDEX IF NOT EXISTS idx_work_sessions_module ON work_sessions(primary_module);
```

---

## 📋 字段详细说明

### 会话信息组

| 字段 | 类型 | 必填 | 说明 | 示例 |
|------|------|------|------|------|
| **id** | INTEGER | ✅ | 自增主键 | `1, 2, 3...` |
| **session_id** | TEXT | ✅ | 会话唯一标识（8位十六进制） | `f2c5decd` |
| **start_time** | TIMESTAMP | ✅ | 开始时间 | `2026-02-06 14:30:25` |
| **end_time** | TIMESTAMP | ✅ | 结束时间 | `2026-02-06 16:45:10` |
| **duration_seconds** | INTEGER | ✅ | 持续时长（秒） | `8085` |
| **created_at** | TIMESTAMP | ✅ | 记录创建时间（自动） | `2026-02-06 16:45:10` |

---

### 工作笔记组（5个问题）

| 字段 | 类型 | 必填 | 问题 | 说明 |
|------|------|------|------|------|
| **work_summary** | TEXT | ✅ | 今日主要做了什么？ | 工作总结，不能为空 |
| **problems** | TEXT | ⬜ | 遇到了什么问题？ | 可选，详细的问题描述 |
| **solutions** | TEXT | ⬜ | 如何解决的？ | 可选，解决方案或思路 |
| **chat_summary** | TEXT | ⬜ | 与Copilot关键对话？ | 可选，重要的讨论要点 |
| **next_steps** | TEXT | ⬜ | 下一步计划？ | 可选，后续任务或TODO |

**示例值**:
```
work_summary: "重构work-logger从Markdown迁移到SQLite，设计work_sessions表"
problems: "初期目录结构混乱，4个位置分散存放文件"
solutions: "采用方案A完全自包含，所有文件统一到tools/work-logger/"
chat_summary: "讨论3种存储方案，确认SQLite更适合高频查询"
next_steps: "实现db_writer.py，测试端到端流程，添加查询命令"
```

---

### 文件变更统计组

| 字段 | 类型 | 必填 | 说明 | 示例 |
|------|------|------|------|------|
| **files_changed** | INTEGER | ✅ | 变更文件总数 | `12` |
| **lines_added** | INTEGER | ✅ | 新增行数总和 | `234` |
| **lines_deleted** | INTEGER | ✅ | 删除行数总和 | `89` |
| **file_details** | TEXT | ✅ | 文件详情（JSON） | 见下表 |

**file_details JSON 结构**:
```json
[
  {
    "file": "tools/work-logger/lib/watcher.py",
    "module": "work-logger",
    "language": "Python",
    "lines_added": 15,
    "lines_deleted": 3,
    "change_type": "modified"
  },
  {
    "file": "tools/work-logger/mylog/schema.sql",
    "module": "文档",
    "language": "SQL",
    "lines_added": 28,
    "lines_deleted": 0,
    "change_type": "modified"
  },
  {
    "file": "tools/work-logger/lib/db_writer.py",
    "module": "work-logger",
    "language": "Python",
    "lines_added": 150,
    "lines_deleted": 0,
    "change_type": "created"
  }
]
```

**change_type 枚举**: `created`, `modified`, `deleted`

---

### 模块推断组

| 字段 | 类型 | 必填 | 说明 | 示例 |
|------|------|------|------|------|
| **primary_module** | TEXT | ✅ | 主要修改的模块 | `work-logger` |
| **modules_touched** | TEXT | ✅ | 涉及的所有模块（JSON） | `["work-logger", "文档"]` |

**推断逻辑**:
- 根据 file_details 统计各模块文件数和行数
- 行数最多的模块为 primary_module
- 所有出现的模块作为 modules_touched 列表

---

## 🔍 查询示例

### 查询最近7天的工作
```bash
SELECT session_id, start_time, work_summary, primary_module, files_changed 
FROM work_sessions 
WHERE DATE(start_time) >= DATE('now', '-7 days')
ORDER BY start_time DESC;
```

### 按模块查询
```bash
SELECT session_id, work_summary, duration_seconds 
FROM work_sessions 
WHERE modules_touched LIKE '%aoem-core%'
ORDER BY start_time DESC
LIMIT 10;
```

### 统计每日工作量
```bash
SELECT 
    DATE(start_time) as 工作日期,
    COUNT(*) as 会话数,
    SUM(duration_seconds) as 总时长_秒,
    SUM(files_changed) as 文件变更数,
    SUM(lines_added) as 新增代码行
FROM work_sessions
GROUP BY DATE(start_time)
ORDER BY 工作日期 DESC;
```

### 按模块统计贡献
```bash
SELECT 
    primary_module as 模块,
    COUNT(*) as 会话数,
    SUM(lines_added) as 新增行数,
    SUM(lines_deleted) as 删除行数
FROM work_sessions
GROUP BY primary_module
ORDER BY 新增行数 DESC;
```

---

## 📝 与现有 changelog 表的关系

### changelog 表（原系统）
- **用途**: 正式里程碑记录
- **频率**: 低频（每周/月几次）
- **字段**: property（7种预定义类型），version，architecture_level
- **触发**: 手动指定，需审核
- **例如**: "阶段封盘"、"生产封盘"、"验证通过"

### work_sessions 表（新系统）
- **用途**: 日常工作笔记
- **频率**: 高频（每天多次）
- **字段**: work_summary（自由文本），5问体系
- **触发**: 自动记录，无审核
- **例如**: 日常工作总结、解决的问题、下一步计划

### 集成策略
- **默认**: work_sessions 自动记录每个工作会话
- **可选**: 通过 `changelog.py add --from-session <session_id>` 将要点提升到 changelog
- **不重复**: work_sessions 关注过程，changelog 关注结果

---

## 🛠️ 使用流程

1. **工作中**: watcher.py 自动监控文件变更
2. **工作结束**: `stop.ps1` 收集5个问题的答案
3. **保存**: db_writer.py 写入 work_sessions 表
4. **查询**: query.py 提供多种查询命令
5. **提升**: （可选）changelog.py 将关键工作记录到 changelog 表

---

## 📌 备注

- session_id 由 session_manager.py 生成（UUID缩写）
- file_details 中的 module 由 analyzer.py 根据文件路径推断
- 所有 TIMESTAMP 使用 SQLite 的 CURRENT_TIMESTAMP
- 索引已优化查询性能，支持按日期、模块、session_id 快速查询
