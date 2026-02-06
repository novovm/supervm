#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
SuperVM Changelog Management CLI

Complete tool for recording and querying SuperVM modifications.

Usage:
    python changelog.py add [options]           # Add new changelog entry
    python changelog.py query [options]         # Query changelog entries
    python changelog.py export [options]        # Export to Markdown/CSV/JSON
    python changelog.py stats [options]         # Show statistics
    python changelog.py list-modules            # List registered modules
    python changelog.py list-properties         # List registered properties

Examples:
    # Add a changelog entry
    python changelog.py add \\
        --date 2026-02-06 \\
        --time 14:30 \\
        --version 0.5.0 \\
        --level L0 \\
        --module aoem-core \\
        --property 测试 \\
        --desc "修复并发控制 bug" \\
        --conclusion "已验证，TPS 提升 5%" \\
        --files aoem/crates/core/aoem-core/src/lib.rs aoem/crates/tests/...

    # Query by module
    python changelog.py query --module aoem-core --since 2026-02-01

    # Query by property
    python changelog.py query --property 生产封盘

    # Export to Markdown
    python changelog.py export --format markdown --output SUPERVM-CHANGELOG.md

    # Show statistics
    python changelog.py stats --by-module --by-property
"""

import sqlite3
import json
import sys
import csv
from pathlib import Path
from datetime import datetime
from typing import List, Optional, Dict, Any
from argparse import ArgumentParser, RawDescriptionHelpFormatter


DB_PATH = Path(__file__).parent.parent / "mylog" / "changelog.db"


class ChangelogDB:
    """SQLite Changelog Database Manager"""

    def __init__(self, db_path: Path = DB_PATH):
        self.db_path = db_path
        if not self.db_path.exists():
            # Auto-initialize database if not exists
            self._init_database()
    
    def _init_database(self):
        """自动初始化数据库"""
        schema_path = self.db_path.parent / "schema.sql"
        if not schema_path.exists():
            raise FileNotFoundError(
                f"Schema file not found: {schema_path}\n"
                f"Cannot auto-initialize database."
            )
        
        conn = sqlite3.connect(str(self.db_path))
        cursor = conn.cursor()
        
        try:
            schema_sql = schema_path.read_text(encoding='utf-8')
            cursor.executescript(schema_sql)
            conn.commit()
            print(f"✅ Database auto-initialized: {self.db_path}")
        except Exception as e:
            print(f"❌ Failed to initialize database: {e}")
            raise
        finally:
            conn.close()

    def get_connection(self) -> sqlite3.Connection:
        conn = sqlite3.connect(str(self.db_path))
        conn.row_factory = sqlite3.Row
        return conn

    def add_entry(
        self,
        date: str,
        time: str,
        version: str,
        level: str,
        module: str,
        property_: str,
        description: str,
        conclusion: str,
        files: List[str],
    ) -> bool:
        """Add a new changelog entry"""
        conn = self.get_connection()
        cursor = conn.cursor()

        try:
            cursor.execute(
                """
                INSERT INTO changelog (date, time, version, architecture_level, module, 
                                      property, description, conclusion, files)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    date,
                    time,
                    version,
                    level,
                    module,
                    property_,
                    description,
                    conclusion,
                    json.dumps(files),
                ),
            )
            conn.commit()
            print(f"✅ Added: {date} {time} | {module} | {property_}")
            return True
        except sqlite3.IntegrityError as e:
            print(f"❌ Error: {e}")
            return False
        finally:
            conn.close()

    def query(
        self,
        module: Optional[str] = None,
        property_: Optional[str] = None,
        level: Optional[str] = None,
        version: Optional[str] = None,
        since: Optional[str] = None,
        until: Optional[str] = None,
        limit: int = 100,
    ) -> List[Dict[str, Any]]:
        """Query changelog entries"""
        conn = self.get_connection()
        cursor = conn.cursor()

        query_parts = ["SELECT * FROM changelog WHERE 1=1"]
        params = []

        if module:
            query_parts.append("AND module = ?")
            params.append(module)
        if property_:
            query_parts.append("AND property = ?")
            params.append(property_)
        if level:
            query_parts.append("AND architecture_level = ?")
            params.append(level)
        if version:
            query_parts.append("AND version = ?")
            params.append(version)
        if since:
            query_parts.append("AND date >= ?")
            params.append(since)
        if until:
            query_parts.append("AND date <= ?")
            params.append(until)

        query_parts.append("ORDER BY date DESC, time DESC LIMIT ?")
        params.append(limit)

        sql = " ".join(query_parts)
        cursor.execute(sql, params)

        results = []
        for row in cursor.fetchall():
            results.append(dict(row))
            results[-1]["files"] = json.loads(results[-1]["files"])

        conn.close()
        return results

    def export_markdown(self, output: Optional[Path] = None) -> str:
        """Export to Markdown table format"""
        conn = self.get_connection()
        cursor = conn.cursor()

        cursor.execute("SELECT * FROM changelog ORDER BY date DESC, time DESC")
        rows = cursor.fetchall()
        conn.close()

        md = """# SuperVM 修改日志

> 自动生成的修改历史记录  
> 用于追踪版本演进、架构改动、模块变更

---

## 日志表

| 日期 | 时间 | 版本 | 架构层级 | 所属模块 | 属性 | 修改/编辑内容简述 | 结论 | 动了哪些文件 |
|------|------|------|--------|--------|------|-----------------|------|-----------|
"""

        for row in rows:
            files = json.loads(row["files"])
            files_str = ", ".join(files) if files else "—"
            if len(files_str) > 50:
                files_str = files_str[:47] + "..."

            md += f"| {row['date']} | {row['time']} | {row['version']} | {row['architecture_level']} | {row['module']} | {row['property']} | {row['description']} | {row['conclusion']} | {files_str} |\n"

        if output:
            output.write_text(md, encoding='utf-8')
            print(f"✅ Exported to Markdown: {output}")

        return md

    def export_csv(self, output: Optional[Path] = None) -> str:
        """Export to CSV format"""
        conn = self.get_connection()
        cursor = conn.cursor()

        cursor.execute("SELECT * FROM changelog ORDER BY date DESC, time DESC")
        rows = cursor.fetchall()
        conn.close()

        import io
        output_buffer = io.StringIO()
        writer = csv.writer(output_buffer)

        # Write header
        if rows:
            writer.writerow(rows[0].keys())
            for row in rows:
                row_data = list(row)
                # Convert files JSON to string
                row_data[-3] = json.loads(row_data[-3])  # files column
                writer.writerow(row_data)

        csv_content = output_buffer.getvalue()

        if output:
            output.write_text(csv_content, encoding='utf-8')
            print(f"✅ Exported to CSV: {output}")

        return csv_content

    def export_json(self, output: Optional[Path] = None) -> str:
        """Export to JSON format"""
        conn = self.get_connection()
        cursor = conn.cursor()

        cursor.execute("SELECT * FROM changelog ORDER BY date DESC, time DESC")
        rows = cursor.fetchall()
        conn.close()

        data = []
        for row in rows:
            row_dict = dict(row)
            row_dict["files"] = json.loads(row_dict["files"])
            data.append(row_dict)

        json_str = json.dumps(data, indent=2, ensure_ascii=False)

        if output:
            output.write_text(json_str, encoding='utf-8')
            print(f"✅ Exported to JSON: {output}")

        return json_str

    def show_stats(self, by_module: bool = False, by_property: bool = False) -> None:
        """Show changelog statistics"""
        conn = self.get_connection()
        cursor = conn.cursor()

        # Total entries
        cursor.execute("SELECT COUNT(*) as count FROM changelog")
        total = cursor.fetchone()["count"]
        print(f"\n📊 Total changelog entries: {total}")

        if by_module:
            print("\n📦 By Module:")
            cursor.execute(
                """
                SELECT module, COUNT(*) as count 
                FROM changelog 
                GROUP BY module 
                ORDER BY count DESC
                """
            )
            for row in cursor.fetchall():
                print(f"   {row['module']:30} {row['count']:3} entries")

        if by_property:
            print("\n🏷️  By Property:")
            cursor.execute(
                """
                SELECT property, COUNT(*) as count 
                FROM changelog 
                GROUP BY property 
                ORDER BY count DESC
                """
            )
            for row in cursor.fetchall():
                print(f"   {row['property']:10} {row['count']:3} entries")

        # Latest entries
        print("\n⏰ Latest 5 entries:")
        cursor.execute(
            "SELECT date, time, module, property FROM changelog ORDER BY date DESC, time DESC LIMIT 5"
        )
        for row in cursor.fetchall():
            print(f"   {row['date']} {row['time']:5} | {row['module']:20} | {row['property']}")

        conn.close()

    def list_modules(self) -> None:
        """List all registered modules"""
        conn = self.get_connection()
        cursor = conn.cursor()

        print("\n📋 Registered Modules:\n")

        cursor.execute("SELECT DISTINCT category FROM module_registry ORDER BY category")
        categories = cursor.fetchall()

        for cat_row in categories:
            category = cat_row["category"]
            print(f"  {category}")
            cursor.execute(
                "SELECT module_name, description FROM module_registry WHERE category = ? ORDER BY module_name",
                (category,),
            )
            for mod_row in cursor.fetchall():
                print(f"    - {mod_row['module_name']:25} {mod_row['description'] or ''}")

        conn.close()

    def list_properties(self) -> None:
        """List all registered properties"""
        conn = self.get_connection()
        cursor = conn.cursor()

        print("\n🏷️  Registered Properties:\n")

        cursor.execute(
            "SELECT property_name, color FROM property_registry ORDER BY priority, property_name"
        )
        for row in cursor.fetchall():
            color = row["color"] or "  "
            print(f"  {color} {row['property_name']}")

        conn.close()


def main():
    parser = ArgumentParser(
        description="SuperVM Changelog Management CLI",
        formatter_class=RawDescriptionHelpFormatter,
        epilog=__doc__,
    )

    subparsers = parser.add_subparsers(dest="command", help="Command to execute")

    # Add command
    add_parser = subparsers.add_parser("add", help="Add new changelog entry")
    add_parser.add_argument("--date", required=True, help="Date (YYYY-MM-DD)")
    add_parser.add_argument("--time", required=True, help="Time (HH:MM)")
    add_parser.add_argument("--version", required=True, help="Version number")
    add_parser.add_argument("--level", required=True, help="Architecture level (L0-L4)")
    add_parser.add_argument("--module", required=True, help="Module name")
    add_parser.add_argument("--property", required=True, help="Property (阶段封盘, 生产封盘, 测试, etc.)")
    add_parser.add_argument("--desc", required=True, help="Description")
    add_parser.add_argument("--conclusion", required=True, help="Conclusion")
    add_parser.add_argument("--files", nargs="*", default=[], help="Modified files")

    # Query command
    query_parser = subparsers.add_parser("query", help="Query changelog entries")
    query_parser.add_argument("--module", help="Filter by module")
    query_parser.add_argument("--property", help="Filter by property")
    query_parser.add_argument("--level", help="Filter by architecture level")
    query_parser.add_argument("--version", help="Filter by version")
    query_parser.add_argument("--since", help="Start date (YYYY-MM-DD)")
    query_parser.add_argument("--until", help="End date (YYYY-MM-DD)")
    query_parser.add_argument("--limit", type=int, default=50, help="Limit results")
    query_parser.add_argument("--format", choices=["table", "json"], default="table")

    # Export command
    export_parser = subparsers.add_parser("export", help="Export changelog")
    export_parser.add_argument("--format", choices=["markdown", "csv", "json"], default="markdown")
    export_parser.add_argument("--output", help="Output file path")

    # Stats command
    stats_parser = subparsers.add_parser("stats", help="Show statistics")
    stats_parser.add_argument("--by-module", action="store_true")
    stats_parser.add_argument("--by-property", action="store_true")

    # List modules/properties
    subparsers.add_parser("list-modules", help="List registered modules")
    subparsers.add_parser("list-properties", help="List registered properties")

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        return

    try:
        db = ChangelogDB()

        if args.command == "add":
            db.add_entry(
                date=args.date,
                time=args.time,
                version=args.version,
                level=args.level,
                module=args.module,
                property_=args.property,
                description=args.desc,
                conclusion=args.conclusion,
                files=args.files,
            )

        elif args.command == "query":
            results = db.query(
                module=args.module,
                property_=args.property,
                level=args.level,
                version=args.version,
                since=args.since,
                until=args.until,
                limit=args.limit,
            )

            if not results:
                print("No results found")
                return

            if args.format == "json":
                print(json.dumps(results, indent=2, ensure_ascii=False))
            else:  # table
                print(f"\n🔍 Found {len(results)} entries:\n")
                print(f"{'Date':10} {'Time':5} {'Version':7} {'Level':4} {'Module':20} {'Property':8} {'Conclusion':15}")
                print("-" * 100)
                for entry in results:
                    files = entry["files"]
                    files_str = ", ".join(files[:2]) if files else "—"
                    print(
                        f"{entry['date']} {entry['time']} {entry['version']:7} {entry['architecture_level']:4} "
                        f"{entry['module']:20} {entry['property']:8} {entry['conclusion'][:15]}"
                    )

        elif args.command == "export":
            output_file = None
            if args.output:
                output_file = Path(args.output)

            if args.format == "markdown":
                db.export_markdown(output_file)
            elif args.format == "csv":
                db.export_csv(output_file)
            elif args.format == "json":
                db.export_json(output_file)

        elif args.command == "stats":
            db.show_stats(by_module=args.by_module, by_property=args.by_property)

        elif args.command == "list-modules":
            db.list_modules()

        elif args.command == "list-properties":
            db.list_properties()

    except FileNotFoundError as e:
        print(f"❌ {e}")
        sys.exit(1)
    except Exception as e:
        print(f"❌ Error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
