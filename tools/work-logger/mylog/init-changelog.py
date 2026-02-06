#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Initialize SQLite changelog database for SuperVM

Usage:
    python init-changelog.py                    # Create database with schema
    python init-changelog.py --reset            # Reset database (dangerous!)
"""

import sqlite3
import sys
from pathlib import Path

DB_PATH = Path(__file__).parent / "changelog.db"
SCHEMA_PATH = Path(__file__).parent / "schema.sql"


def init_database(reset: bool = False):
    """初始化数据库"""
    if reset and DB_PATH.exists():
        print(f"⚠️  Removing {DB_PATH}...")
        DB_PATH.unlink()

    conn = sqlite3.connect(str(DB_PATH))
    cursor = conn.cursor()

    # 读取并执行 schema
    if not SCHEMA_PATH.exists():
        print(f"❌ Schema file not found: {SCHEMA_PATH}")
        return False

    schema_sql = SCHEMA_PATH.read_text(encoding='utf-8')
    cursor.executescript(schema_sql)
    conn.commit()

    # 验证表
    cursor.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='changelog'"
    )
    if not cursor.fetchone():
        print("❌ Failed to create tables")
        return False

    conn.close()
    print(f"✅ Database initialized: {DB_PATH}")
    print(f"   Schema: {SCHEMA_PATH}")
    return True


if __name__ == "__main__":
    reset = "--reset" in sys.argv
    if reset:
        print("⚠️  WARNING: --reset will delete all data!")
        confirm = input("确认重置数据库? (y/N): ")
        if confirm.lower() != 'y':
            print("取消重置")
            sys.exit(0)

    success = init_database(reset=reset)
    sys.exit(0 if success else 1)
