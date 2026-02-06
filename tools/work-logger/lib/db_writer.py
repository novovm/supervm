#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Database Writer Module - Work Sessions

Handles persisting work session data to SQLite database.
Integrated with session_manager and analyzer.

Usage:
    from db_writer import WorkSessionWriter
    
    writer = WorkSessionWriter()
    writer.write_session(session_data, work_note_input)
"""

import sqlite3
import json
import sys
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Any, Optional

# Auto-detect database location
DB_PATH = Path(__file__).parent.parent / "mylog" / "changelog.db"


class WorkSessionWriter:
    """SQLite Work Sessions Writer"""

    def __init__(self, db_path: Path = DB_PATH):
        self.db_path = db_path
        self._ensure_db_initialized()

    def _ensure_db_initialized(self):
        """Verify database exists and schema is current"""
        if not self.db_path.parent.exists():
            self.db_path.parent.mkdir(parents=True, exist_ok=True)

        if not self.db_path.exists():
            raise FileNotFoundError(
                f"❌ Database not initialized: {self.db_path}\n"
                f"Run: python {Path(__file__).parent}/install.py"
            )

    def get_connection(self) -> sqlite3.Connection:
        """Get database connection with row factory"""
        conn = sqlite3.connect(str(self.db_path))
        conn.row_factory = sqlite3.Row
        return conn

    def write_session(
        self,
        session_data: Dict[str, Any],
        work_note_input: Dict[str, str],
        file_changes: Dict[str, Any],
        module_inference: Dict[str, Any],
    ) -> bool:
        """
        Write complete work session to database.

        Args:
            session_data: From session_manager (session_id, start_time, end_time, etc.)
            work_note_input: 5 questions from user input
            file_changes: File change details with git diff info
            module_inference: Module inference from analyzer (primary_module, modules_touched)

        Returns:
            True if successful, False otherwise
        """
        try:
            # Build file_details JSON array
            file_details = self._build_file_details(file_changes, module_inference)

            # Calculate statistics
            stats = self._calculate_stats(file_details)

            # Prepare INSERT statement
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                INSERT INTO work_sessions (
                    session_id, start_time, end_time, duration_seconds,
                    work_summary, problems, solutions, chat_summary, next_steps,
                    files_changed, lines_added, lines_deleted, file_details,
                    primary_module, modules_touched
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    session_data.get("session_id"),
                    session_data.get("start_time"),
                    session_data.get("end_time"),
                    session_data.get("duration_seconds"),
                    work_note_input.get("work_summary", "").strip() or None,
                    work_note_input.get("problems", "").strip() or None,
                    work_note_input.get("solutions", "").strip() or None,
                    work_note_input.get("chat_summary", "").strip() or None,
                    work_note_input.get("next_steps", "").strip() or None,
                    stats["files_changed"],
                    stats["lines_added"],
                    stats["lines_deleted"],
                    json.dumps(file_details, ensure_ascii=False, indent=2),
                    module_inference.get("primary_module"),
                    json.dumps(module_inference.get("modules_touched", []), ensure_ascii=False),
                ),
            )

            conn.commit()
            session_id = session_data.get("session_id")
            print(f"✅ Recorded session {session_id} to work_sessions")
            return True

        except sqlite3.IntegrityError as e:
            print(f"❌ Database integrity error: {e}")
            return False
        except Exception as e:
            print(f"❌ Failed to write session: {e}")
            return False
        finally:
            if conn:
                conn.close()

    def _build_file_details(
        self, file_changes: Dict[str, Any], module_inference: Dict[str, Any]
    ) -> List[Dict[str, Any]]:
        """
        Build file_details JSON array from file changes and module inference.

        Format:
        [
            {
                "file": "path/to/file.py",
                "module": "work-logger",
                "language": "Python",
                "lines_added": 15,
                "lines_deleted": 3,
                "change_type": "modified"
            },
            ...
        ]
        """
        details = []
        module_map = module_inference.get("module_map", {})

        for file_path, changes in file_changes.items():
            module = module_map.get(file_path, "unknown")
            language = self._detect_language(file_path)
            change_type = changes.get("type", "modified")

            details.append({
                "file": file_path,
                "module": module,
                "language": language,
                "lines_added": changes.get("lines_added", 0),
                "lines_deleted": changes.get("lines_deleted", 0),
                "change_type": change_type,
            })

        return sorted(details, key=lambda x: x["file"])

    def _calculate_stats(self, file_details: List[Dict[str, Any]]) -> Dict[str, int]:
        """Calculate aggregate statistics from file details"""
        stats = {
            "files_changed": len(file_details),
            "lines_added": sum(f.get("lines_added", 0) for f in file_details),
            "lines_deleted": sum(f.get("lines_deleted", 0) for f in file_details),
        }
        return stats

    def _detect_language(self, file_path: str) -> str:
        """Detect programming language from file extension"""
        ext_map = {
            ".py": "Python",
            ".rs": "Rust",
            ".ts": "TypeScript",
            ".js": "JavaScript",
            ".sql": "SQL",
            ".md": "Markdown",
            ".json": "JSON",
            ".toml": "TOML",
            ".yaml": "YAML",
            ".sh": "Bash",
            ".ps1": "PowerShell",
            ".sol": "Solidity",
            ".go": "Go",
            ".c": "C",
            ".cpp": "C++",
        }
        suffix = Path(file_path).suffix.lower()
        return ext_map.get(suffix, "Unknown")

    def query_recent(self, days: int = 7) -> List[Dict[str, Any]]:
        """Query work sessions from last N days"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT 
                    session_id, start_time, duration_seconds,
                    work_summary, primary_module, files_changed, lines_added
                FROM work_sessions
                WHERE DATE(start_time) >= DATE('now', ?)
                ORDER BY start_time DESC
                """,
                (f"-{days} days",),
            )

            results = [dict(row) for row in cursor.fetchall()]
            return results

        except Exception as e:
            print(f"❌ Query failed: {e}")
            return []
        finally:
            if conn:
                conn.close()

    def query_by_module(self, module: str, limit: int = 20) -> List[Dict[str, Any]]:
        """Query work sessions by module"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT 
                    session_id, start_time, duration_seconds,
                    work_summary, files_changed, lines_added
                FROM work_sessions
                WHERE modules_touched LIKE ?
                ORDER BY start_time DESC
                LIMIT ?
                """,
                (f"%{module}%", limit),
            )

            results = [dict(row) for row in cursor.fetchall()]
            return results

        except Exception as e:
            print(f"❌ Query failed: {e}")
            return []
        finally:
            if conn:
                conn.close()


def main():
    """Test database writer"""
    # Example usage
    writer = WorkSessionWriter()

    sample_session = {
        "session_id": "test1234",
        "start_time": datetime.now().isoformat(),
        "end_time": datetime.now().isoformat(),
        "duration_seconds": 3600,
    }

    sample_input = {
        "work_summary": "测试数据库写入功能",
        "problems": "初期目录结构混乱",
        "solutions": "采用完全自包含方案",
        "chat_summary": "讨论SQLite vs Markdown",
        "next_steps": "实现查询命令",
    }

    sample_files = {
        "tools/work-logger/lib/db_writer.py": {"type": "created", "lines_added": 200, "lines_deleted": 0},
        "tools/work-logger/mylog/schema.sql": {"type": "modified", "lines_added": 30, "lines_deleted": 0},
    }

    sample_modules = {
        "primary_module": "work-logger",
        "modules_touched": ["work-logger", "文档"],
        "module_map": {
            "tools/work-logger/lib/db_writer.py": "work-logger",
            "tools/work-logger/mylog/schema.sql": "文档",
        },
    }

    success = writer.write_session(sample_session, sample_input, sample_files, sample_modules)
    print(f"Write result: {success}")


if __name__ == "__main__":
    main()
