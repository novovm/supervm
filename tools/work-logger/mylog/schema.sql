-- SQLite Schema for SuperVM Changelog
-- Created: 2026-02-06
-- Purpose: Track all modifications, creations, and document updates

CREATE TABLE IF NOT EXISTS changelog (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,                  -- YYYY-MM-DD
    time TEXT NOT NULL,                  -- HH:MM
    version TEXT NOT NULL,               -- e.g., 0.5.0
    architecture_level TEXT NOT NULL,    -- L0, L1, L2, L3, L4
    module TEXT NOT NULL,                -- e.g., aoem-core, vm-runtime
    property TEXT NOT NULL,              -- 阶段封盘, 生产封盘, 测试, 实验, 验证, 修复
    description TEXT NOT NULL,           -- 修改/编辑内容简述
    conclusion TEXT NOT NULL,            -- 结论/结果
    files TEXT NOT NULL,                 -- JSON array: ["file1.rs", "file2.md", ...]
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(date, time, module)           -- 防止同一时刻对同一模块的重复记录
);

CREATE TABLE IF NOT EXISTS module_registry (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_name TEXT UNIQUE NOT NULL,    -- aoem-core, vm-runtime, etc.
    category TEXT NOT NULL,              -- 并发控制, 执行引擎, GPU加速, 隐私/ZK, 存储, 网络/共识, 许可证, 文档
    description TEXT,                    -- 模块说明
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS property_registry (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    property_name TEXT UNIQUE NOT NULL,  -- 阶段封盘, 生产封盘, 测试, 实验, 验证, 修复
    color TEXT,                          -- 用于 CLI 输出的颜色标记
    priority INTEGER,                    -- 优先级
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 索引优化查询
CREATE INDEX IF NOT EXISTS idx_changelog_date ON changelog(date);
CREATE INDEX IF NOT EXISTS idx_changelog_module ON changelog(module);
CREATE INDEX IF NOT EXISTS idx_changelog_property ON changelog(property);
CREATE INDEX IF NOT EXISTS idx_changelog_level ON changelog(architecture_level);
CREATE INDEX IF NOT EXISTS idx_changelog_version ON changelog(version);

-- 初始化模块注册表
INSERT OR IGNORE INTO module_registry (module_name, category, description) VALUES
    ('aoem-core', '执行引擎', 'AOEM 核心并发控制引擎'),
    ('aoem-engine', '执行引擎', 'AOEM 对外执行入口'),
    ('aoem-backend-gpu', 'GPU加速', 'AOEM GPU 后端'),
    ('aoem-backend-cpu', 'GPU加速', 'AOEM CPU 后端'),
    ('aoem-runtime-wasmtime', '执行引擎', 'WASM 运行时'),
    ('vm-runtime', '并发控制', 'SuperVM 运行时'),
    ('gpu-executor', 'GPU加速', 'GPU 执行器'),
    ('l2-executor', 'GPU加速', 'L2 zkVM 执行器'),
    ('zkvm-executor', 'GPU加速', 'zkVM 执行器'),
    ('domain-registry', '应用', '域名注册系统'),
    ('defi-core', '应用', 'DeFi 核心模块'),
    ('web3-storage', '存储', 'Web3 存储层'),
    ('network', '网络/共识', '网络模块'),
    ('consensus', '网络/共识', '共识模块'),
    ('许可证', '许可证', 'LICENSE 和许可证政策'),
    ('文档', '文档', '项目文档');

-- 初始化属性注册表
INSERT OR IGNORE INTO property_registry (property_name, color, priority) VALUES
    ('阶段封盘', '🔵', 1),
    ('生产封盘', '🔴', 1),
    ('测试', '🟡', 2),
    ('实验', '🟣', 3),
    ('验证', '🟢', 2),
    ('修复', '🔧', 1),
    ('文档', '📚', 2);

-- ==========================================
-- Work Sessions Table (Daily Work Logger)
-- ==========================================
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
    file_details TEXT,              -- JSON: 文件详细信息数组
    
    -- 推断上下文
    primary_module TEXT,
    modules_touched TEXT,           -- JSON: 涉及的所有模块数组
    
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Work Sessions 索引
CREATE INDEX IF NOT EXISTS idx_work_sessions_session_id ON work_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_work_sessions_date ON work_sessions(DATE(start_time));
CREATE INDEX IF NOT EXISTS idx_work_sessions_module ON work_sessions(primary_module);
