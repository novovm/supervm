# 🚀 Work Logger 快速参考卡

## 启动 & 管理

```powershell
# 启动监听器（自动）
VS Code 启动时自动启动

# 启动监听器（手动）
.\tools\work-logger\bin\start.ps1

# 查看状态
.\tools\work-logger\bin\status.ps1

# 停止并保存工作笔记
.\tools\work-logger\bin\stop.ps1
# 此时会提示回答5个问题：
# 1. 今日主要做了什么？(必填)
# 2. 遇到了什么问题？
# 3. 如何解决的？
# 4. 与Copilot关键对话？
# 5. 下一步计划？
```

---

## 📊 查询历史

```powershell
# 最近7天工作
.\tools\work-logger\bin\query.ps1 --recent 7

# 最近30天工作
.\tools\work-logger\bin\query.ps1 --recent 30

# 按模块查询
.\tools\work-logger\bin\query.ps1 --module aoem-core
.\tools\work-logger\bin\query.ps1 --module gpu-executor
.\tools\work-logger\bin\query.ps1 --module 文档

# 按关键词搜索
.\tools\work-logger\bin\query.ps1 --search "GPU"
.\tools\work-logger\bin\query.ps1 --search "并发"
.\tools\work-logger\bin\query.ps1 --search "bug"

# 查看总体统计
.\tools\work-logger\bin\query.ps1 --stats

# 导出会话详情（显示全部5个问题+文件列表）
.\tools\work-logger\bin\query.ps1 --export session_id

# 日报汇总（30天）
.\tools\work-logger\bin\query.ps1 --daily 30
```

---

## 💾 数据存储位置

| 数据 | 位置 | 说明 |
|------|------|------|
| W数据库 | `tools/work-logger/mylog/changelog.db` | SQLite，包含 work_sessions 表 |
| 运行时 PID | `tools/work-logger/data/watcher.pid` | 当前监听器进程ID |
| 当前会话 | `tools/work-logger/data/current_session.json` | 活跃会话信息 |
| 临时输入 | `tools/work-logger/data/work_note_input.json` | 停止时的5个问题答案 |

---

## 📚 文档位置

| 文档 | 用途 |
|------|------|
| [DATABASE-SCHEMA.md](DATABASE-SCHEMA.md) | 数据表详细说明、查询示例 |
| [README.md](README.md) | 功能说明、快速开始 |
| [MIGRATION-COMPLETE.md](MIGRATION-COMPLETE.md) | 迁移完成报告 |

---

## ⚡ 常见场景

### 场景 1: 今天白天做了些什么？
```powershell
.\tools\work-logger\bin\query.ps1 --recent 1
```

### 场景 2: 上周围绕 GPU 做了什么？
```powershell
.\tools\work-logger\bin\query.ps1 --search "GPU" --recent 7
```

### 场景 3: aoem-core 模块有多少次修改？
```powershell
.\tools\work-logger\bin\query.ps1 --module aoem-core
```

### 场景 4: 本月贡献了多少代码？
```powershell
.\tools\work-logger\bin\query.ps1 --stats
```

### 场景 5: 查看某个会话的完整详情
```powershell
.\tools\work-logger\bin\query.ps1 --export f2c5decd
# 显示：时间、模块、问题和解决方案、改动文件等
```

---

## 🔧 故障排查

**监听器未启动？**
```powershell
# 检查状态
.\tools\work-logger\bin\status.ps1

# 手动启动
.\tools\work-logger\bin\start.ps1
```

**数据库查询失败？**
```powershell
# 确认数据库存在
Test-Path tools/work-logger/mylog/changelog.db  # 应该返回 True

# 重新初始化（危险！）
Remove-Item tools/work-logger/mylog/changelog.db
python tools\work-logger\lib\install.py
```

**无法回答5个问题？**
- 直接按 Enter 跳过可选问题
- 只有"今日主要做了什么"是必填的

---

## 📈 监听器行为

- **启动**: 创建会话，开始监听文件
- **工作中**: 每2秒去重+统计一次文件变更
- **结束**: 刷新最后变更，收集答案，保存到数据库

---

## 💡 最佳实践

1. ✅ **定期查询** - 每周回顾一次工作 (`--recent 7`)
2. ✅ **认真填答** - 5个问题越详细越好
3. ✅ **模块准确** - analyzer 通过文件路径推断，需要合理的目录结构
4. ✅ **备份数据库** - `tools/work-logger/mylog/changelog.db` 定期备份
5. ✅ **清理久远会话** - 运行时数据可定期清理

---

**版本**: 0.3.0 (Database Edition)  
**最后更新**: 2026-02-06
