#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Query Module - Work Sessions Retrieval

Command-line tool for querying work_sessions table.

Usage:
    python query.py --recent 7              # Show last 7 days
    python query.py --module aoem-core     # Filter by module
    python query.py --search "GPU"         # Search work_summary
    python query.py --stats                # Show statistics
    python query.py --export session_id    # Export session details
"""

import sqlite3
import json
import sys
from pathlib import Path
from datetime import datetime
from typing import List, Dict, Any, Optional
from argparse import ArgumentParser, RawDescriptionHelpFormatter

# Auto-detect database location
DB_PATH = Path(__file__).parent.parent / "mylog" / "changelog.db"


class WorkSessionQuery:
    """Query interface for work_sessions"""

    def __init__(self, db_path: Path = DB_PATH):
        self.db_path = db_path

    def get_connection(self) -> sqlite3.Connection:
        """Get database connection"""
        conn = sqlite3.connect(str(self.db_path))
        conn.row_factory = sqlite3.Row
        return conn

    def recent(self, days: int = 7) -> List[Dict[str, Any]]:
        """Get work sessions from last N days"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT 
                    session_id, DATE(start_time) as date, start_time,
                    duration_seconds, work_summary, primary_module,
                    files_changed, lines_added
                FROM work_sessions
                WHERE DATE(start_time) >= DATE('now', ?)
                ORDER BY start_time DESC
                """,
                (f"-{days} days",),
            )

            return [dict(row) for row in cursor.fetchall()]
        finally:
            conn.close()

    def by_module(self, module: str, limit: int = 20) -> List[Dict[str, Any]]:
        """Get work sessions involving a module"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT 
                    session_id, DATE(start_time) as date,
                    duration_seconds, work_summary, primary_module, files_changed
                FROM work_sessions
                WHERE modules_touched LIKE ?
                ORDER BY start_time DESC
                LIMIT ?
                """,
                (f"%{module}%", limit),
            )

            return [dict(row) for row in cursor.fetchall()]
        finally:
            conn.close()

    def search(self, keyword: str, limit: int = 20) -> List[Dict[str, Any]]:
        """Search work_summary by keyword"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT 
                    session_id, DATE(start_time) as date,
                    work_summary, primary_module, files_changed
                FROM work_sessions
                WHERE work_summary LIKE ? OR problems LIKE ? 
                   OR solutions LIKE ? OR next_steps LIKE ?
                ORDER BY start_time DESC
                LIMIT ?
                """,
                (f"%{keyword}%", f"%{keyword}%", f"%{keyword}%", f"%{keyword}%", limit),
            )

            return [dict(row) for row in cursor.fetchall()]
        finally:
            conn.close()

    def get_session(self, session_id: str) -> Optional[Dict[str, Any]]:
        """Get full session details"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT * FROM work_sessions WHERE session_id = ?
                """,
                (session_id,),
            )

            row = cursor.fetchone()
            return dict(row) if row else None
        finally:
            conn.close()

    def stats(self) -> Dict[str, Any]:
        """Get overall statistics"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT 
                    COUNT(*) as total_sessions,
                    SUM(duration_seconds) as total_seconds,
                    SUM(files_changed) as total_files,
                    SUM(lines_added) as total_added,
                    SUM(lines_deleted) as total_deleted,
                    MIN(DATE(start_time)) as first_session,
                    MAX(DATE(start_time)) as last_session
                FROM work_sessions
                """
            )

            row = cursor.fetchone()
            base_stats = dict(row) if row else {}

            # Per-module statistics
            cursor.execute(
                """
                SELECT 
                    primary_module as module,
                    COUNT(*) as count,
                    SUM(lines_added) as lines_added,
                    SUM(lines_deleted) as lines_deleted
                FROM work_sessions
                GROUP BY primary_module
                ORDER BY count DESC
                """
            )

            module_stats = [dict(row) for row in cursor.fetchall()]
            base_stats["by_module"] = module_stats

            return base_stats
        finally:
            conn.close()

    def daily_summary(self, days: int = 30) -> List[Dict[str, Any]]:
        """Get daily work summary"""
        try:
            conn = self.get_connection()
            cursor = conn.cursor()

            cursor.execute(
                """
                SELECT 
                    DATE(start_time) as date,
                    COUNT(*) as sessions,
                    SUM(duration_seconds) / 3600.0 as hours,
                    SUM(files_changed) as files_changed,
                    SUM(lines_added) as lines_added,
                    GROUP_CONCAT(DISTINCT primary_module, ', ') as modules
                FROM work_sessions
                WHERE DATE(start_time) >= DATE('now', ?)
                GROUP BY DATE(start_time)
                ORDER BY date DESC
                """,
                (f"-{days} days",),
            )

            return [dict(row) for row in cursor.fetchall()]
        finally:
            conn.close()


def format_duration(seconds: int) -> str:
    """Format seconds as HH:MM"""
    hours = seconds // 3600
    minutes = (seconds % 3600) // 60
    return f"{hours:02d}:{minutes:02d}"


def print_table(rows: List[Dict[str, Any]], columns: List[str]):
    """Print formatted table"""
    if not rows:
        print("No results found.")
        return

    # Calculate column widths
    widths = {}
    for col in columns:
        widths[col] = len(col)
        for row in rows:
            val = str(row.get(col, ""))
            widths[col] = max(widths[col], len(val))

    # Print header
    header = " | ".join(col.ljust(widths[col]) for col in columns)
    print(header)
    print("-" * len(header))

    # Print rows
    for row in rows:
        values = [str(row.get(col, "")).ljust(widths[col]) for col in columns]
        print(" | ".join(values))


def main():
    parser = ArgumentParser(
        description="Query work_sessions from SuperVM changelog database",
        formatter_class=RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python query.py --recent 7              # Last 7 days
  python query.py --module aoem-core      # Sessions touching aoem-core
  python query.py --search "GPU"          # Search work summaries
  python query.py --stats                 # Show statistics
  python query.py --export session123     # Export session details
  python query.py --daily 30              # Daily summary for 30 days
        """,
    )

    parser.add_argument("--recent", type=int, metavar="DAYS", help="Show last N days")
    parser.add_argument("--module", metavar="NAME", help="Filter by module")
    parser.add_argument("--search", metavar="KEYWORD", help="Search work_summary")
    parser.add_argument("--stats", action="store_true", help="Show overall statistics")
    parser.add_argument("--export", metavar="SESSION_ID", help="Export session details")
    parser.add_argument("--daily", type=int, metavar="DAYS", help="Daily summary")

    args = parser.parse_args()

    if not DB_PATH.exists():
        print(f"❌ Database not found: {DB_PATH}")
        sys.exit(1)

    query = WorkSessionQuery()

    if args.recent:
        print(f"\n📋 Work Sessions (Last {args.recent} Days)\n")
        rows = query.recent(args.recent)
        for row in rows:
            duration = format_duration(row["duration_seconds"])
            print(f"  {row['session_id']}  {row['date']} {duration}")
            print(f"    {row['work_summary']}")
            print(f"    📊 {row['files_changed']} files, +{row['lines_added']} lines")
            print()

    elif args.module:
        print(f"\n🔍 Sessions Touching '{args.module}'\n")
        rows = query.by_module(args.module)
        for row in rows:
            duration = format_duration(row["duration_seconds"])
            print(f"  {row['session_id']}  {row['date']} {duration}")
            print(f"    {row['work_summary']}")
            print()

    elif args.search:
        print(f"\n🔍 Search Results for '{args.search}'\n")
        rows = query.search(args.search)
        for row in rows:
            print(f"  {row['session_id']}  {row['date']}")
            print(f"    {row['work_summary']}")
            print()

    elif args.export:
        session = query.get_session(args.export)
        if session:
            print(f"\n📝 Session {args.export}\n")
            print(f"Time: {session['start_time']} - {session['end_time']}")
            print(f"Duration: {format_duration(session['duration_seconds'])}")
            print(f"Modules: {session['modules_touched']}")
            print(f"\nWork Summary:\n{session['work_summary']}")
            if session["problems"]:
                print(f"\nProblems:\n{session['problems']}")
            if session["solutions"]:
                print(f"\nSolutions:\n{session['solutions']}")
            if session["chat_summary"]:
                print(f"\nChat Summary:\n{session['chat_summary']}")
            if session["next_steps"]:
                print(f"\nNext Steps:\n{session['next_steps']}")
            print(f"\nFiles Changed: {session['files_changed']}")
            print(f"Lines: +{session['lines_added']} -{session['lines_deleted']}")
        else:
            print(f"❌ Session not found: {args.export}")

    elif args.daily:
        print(f"\n📅 Daily Summary (Last {args.daily} Days)\n")
        rows = query.daily_summary(args.daily)
        for row in rows:
            hours = f"{row['hours']:.1f}h"
            print(f"  {row['date']}  {row['sessions']} sessions  {hours}")
            print(f"    Files: {row['files_changed']}, Lines: +{row['lines_added']}")
            print(f"    Modules: {row['modules']}")
            print()

    elif args.stats:
        stats = query.stats()
        print(f"\n📊 Overall Statistics\n")
        print(f"  Total Sessions: {stats.get('total_sessions', 0)}")
        total_sec = stats.get("total_seconds", 0) or 0
        print(f"  Total Time: {total_sec // 3600}h {(total_sec % 3600) // 60}m")
        print(f"  Total Files Changed: {stats.get('total_files', 0)}")
        print(f"  Total Lines: +{stats.get('total_added', 0)} -{stats.get('total_deleted', 0)}")
        print(f"  Period: {stats.get('first_session')} to {stats.get('last_session')}")
        print(f"\n  By Module:")
        for mod in stats.get("by_module", []):
            print(f"    {mod['module']}: {mod['count']} sessions, +{mod['lines_added']} lines")

    else:
        parser.print_help()


if __name__ == "__main__":
    main()
