#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Example script showing how to use the SuperVM Changelog system programmatically

Usage:
    python examples.py
"""

import sys
from pathlib import Path

# Add scripts to path
scripts_path = Path(__file__).parent.parent.parent / "scripts"
sys.path.insert(0, str(scripts_path))

from changelog import ChangelogDB


def example_add_entries():
    """Example 1: Add multiple entries"""
    print("=" * 60)
    print("Example 1: Adding entries to the changelog")
    print("=" * 60)

    db = ChangelogDB()

    # Example entry 1
    db.add_entry(
        date="2026-02-06",
        time="14:30",
        version="0.5.0",
        level="L0",
        module="aoem-core",
        property_="测试",
        description="修复并发控制 bug，提升 TPS 5%",
        conclusion="已验证，通过 454 个单元测试",
        files=["aoem/crates/core/aoem-core/src/lib.rs", "aoem/crates/tests/..."],
    )

    # Example entry 2
    db.add_entry(
        date="2026-02-06",
        time="15:00",
        version="0.5.0",
        level="L1",
        module="gpu-executor",
        property_="验证",
        description="GPU MSM 性能基准：512+ 点自动 GPU 路由，<512 点 CPU 处理",
        conclusion="性能基线稳定，无回归；GPU 失败自动降级 CPU",
        files=["src/gpu-executor/src/lib.rs", "src/gpu-executor/src/msm.rs"],
    )

    # Example entry 3
    db.add_entry(
        date="2026-02-06",
        time="16:00",
        version="0.5.0",
        level="L0",
        module="许可证",
        property_="文档",
        description="创建 AOEM 专有化许可证分析文档",
        conclusion="双许可证方案可行，推荐 Cargo.toml 改动方案",
        files=["docs/temp/AOEM-PROPRIETARY-LICENSING-ANALYSIS-2026-02-06.md"],
    )

    print("✅ All entries added\n")


def example_query():
    """Example 2: Query entries with various filters"""
    print("=" * 60)
    print("Example 2: Querying entries")
    print("=" * 60)

    db = ChangelogDB()

    # Query 1: All entries from today
    print("\n📊 All entries from 2026-02-06:")
    results = db.query(since="2026-02-06", until="2026-02-06")
    for entry in results:
        print(f"  {entry['time']} | {entry['module']:20} | {entry['property']:8} | {entry['description'][:40]}")

    # Query 2: L0 layer only
    print("\n📊 L0 layer entries:")
    results = db.query(level="L0")
    for entry in results:
        print(f"  {entry['date']} | {entry['module']:20} | {entry['property']:8}")

    # Query 3: Specific module
    print("\n📊 aoem-core module entries:")
    results = db.query(module="aoem-core")
    for entry in results:
        print(f"  {entry['date']} {entry['time']} | {entry['conclusion'][:50]}")

    print()


def example_stats():
    """Example 3: Show statistics"""
    print("=" * 60)
    print("Example 3: Statistics")
    print("=" * 60 + "\n")

    db = ChangelogDB()
    db.show_stats(by_module=True, by_property=True)


def example_export():
    """Example 4: Export to various formats"""
    print("=" * 60)
    print("Example 4: Exporting to different formats")
    print("=" * 60)

    db = ChangelogDB()

    # Export to Markdown
    print("\n📝 Markdown export (first 500 chars):")
    md = db.export_markdown()
    print(md[:500] + "...\n")

    # Export to JSON
    print("📦 JSON export (sample):")
    json_str = db.export_json()
    print(json_str[:300] + "...\n")

    # Export to CSV
    print("📊 CSV export (first 500 chars):")
    csv_str = db.export_csv()
    print(csv_str[:500] + "...\n")


def example_list_modules():
    """Example 5: List registered modules and properties"""
    print("=" * 60)
    print("Example 5: Registered modules and properties")
    print("=" * 60)

    db = ChangelogDB()

    print("\n")
    db.list_modules()

    print("\n")
    db.list_properties()


def main():
    try:
        print("\n" + "=" * 60)
        print("SuperVM Changelog System - Examples")
        print("=" * 60 + "\n")

        # Run examples
        example_add_entries()
        example_query()
        example_stats()
        example_export()
        example_list_modules()

        print("\n" + "=" * 60)
        print("✅ All examples completed successfully!")
        print("=" * 60 + "\n")

        print("Next steps:")
        print("1. Explore the changelog data:")
        print("   python ../bin/changelog.py query --help")
        print("\n2. Export a report:")
        print("   python ../bin/changelog.py export --format markdown")
        print("\n3. Check statistics:")
        print("   python ../bin/changelog.py stats --by-module")
        print("\nSee README.md for more information.\n")

    except Exception as e:
        print(f"❌ Error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
