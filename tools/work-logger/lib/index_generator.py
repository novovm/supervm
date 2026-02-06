"""
Docs Index Generator

Auto-generates docs/INDEX.md with directory tree, purpose, and timestamps.
"""

from __future__ import annotations

from datetime import datetime
import json
from pathlib import Path
from typing import List, Tuple
import re


IGNORE_DIRS = {
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".vscode",
    ".idea",
    "cargo-target-supervm",
}
IGNORE_FILES = {
    ".DS_Store",
}
IGNORE_SUFFIXES = {
    ".log",
}
IGNORE_PATH_PREFIXES = {
    "tools/work-logger/data",
    "tools/work-logger/lib/__pycache__",
}

DESCRIPTION_MAP_PATH = Path(__file__).parent.parent / "index-descriptions.json"

TOKEN_MAP = {
    "README": "项目入口",
    "LICENSE": "授权文件",
    "CONTRIBUTING": "贡献指南",
    "ROADMAP": "发展路线图",
    "WORK": "工作",
    "LOGGER": "日志",
    "LOG": "日志",
    "NOTE": "笔记",
    "NOTES": "笔记",
    "WORKNOTE": "工作笔记",
    "SETUP": "安装",
    "COMPLETE": "完成",
    "PYTHON": "Python",
    "EDITION": "版",
    "QUICK": "快速",
    "REFERENCE": "参考",
    "DATABASE": "数据库",
    "SCHEMA": "结构",
    "MIGRATION": "迁移",
    "REPORT": "报告",
    "SUMMARY": "总结",
    "INDEX": "索引",
    "CHANGELOG": "变更日志",
    "DEPLOYMENT": "部署",
    "CHECKLIST": "清单",
    "START": "启动",
    "STOP": "停止",
    "STATUS": "状态",
    "QUERY": "查询",
    "SESSION": "会话",
    "GENERATOR": "生成器",
    "OUTPUT": "输出",
}


def update_docs_index(
    repo_root: Path,
    index_path: Path | None = None,
    scope_root: bool = True,
) -> bool:
    """Generate or update docs/INDEX.md. Returns True if file changed."""
    repo_root = repo_root.resolve()
    if index_path is None:
        index_path = repo_root / "docs" / "INDEX.md"

    root_path = repo_root if scope_root else repo_root / "docs"
    if not root_path.exists() or not root_path.is_dir():
        return False

    index_path.parent.mkdir(parents=True, exist_ok=True)
    description_map = _load_description_map(repo_root)
    content = _render_index(root_path, index_path, repo_root, scope_root, description_map)

    if index_path.exists():
        try:
            existing = index_path.read_text(encoding="utf-8")
            if existing == content:
                return False
        except Exception:
            pass

    index_path.write_text(content, encoding="utf-8")
    return True


def _render_index(
    root_path: Path,
    index_path: Path,
    repo_root: Path,
    scope_root: bool,
    description_map: dict,
) -> str:
    now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    lines: List[str] = []
    lines.append("# Docs Index")
    lines.append("")
    lines.append(f"自动生成时间: {now}")
    lines.append("")
    lines.append("说明: 该文件由 work-logger 自动生成，请勿手动编辑。")
    lines.append(f"范围: {'仓库根目录' if scope_root else 'docs/'}")
    lines.append("")
    lines.append("## 目录结构")
    lines.append("")

    root_display = f"{repo_root.name}/" if scope_root else "docs/"
    root_purpose = "仓库根目录" if scope_root else "文档根目录"
    root_created, root_modified = _get_times(root_path)

    entries = _build_tree_entries(root_path, index_path, repo_root, description_map, prefix="", is_last=True)
    entries.insert(0, (root_display, root_purpose, root_created, root_modified))

    max_name_len = max(len(name) for name, _, _, _ in entries) if entries else 0
    for name, purpose, created_at, modified_at in entries:
        padding = " " * (max_name_len - len(name) + 2)
        lines.append(
            f"{name}{padding}# {purpose} ({created_at}/{modified_at})"
        )
    lines.append("")
    return "\n".join(lines)


def _build_tree_entries(
    root: Path,
    index_path: Path,
    repo_root: Path,
    description_map: dict,
    prefix: str,
    is_last: bool,
) -> List[Tuple[str, str, str, str]]:
    entries_out: List[Tuple[str, str, str, str]] = []
    entries = [e for e in _sorted_entries(root) if not _should_ignore(e, index_path, repo_root)]

    for idx, entry in enumerate(entries):
        last_entry = idx == len(entries) - 1
        connector = "└── " if last_entry else "├── "
        display_name = entry.name + ("/" if entry.is_dir() else "")
        rel_path = _relative_path(entry, repo_root)
        purpose = _infer_purpose(entry, rel_path, description_map)
        created_at, modified_at = _get_times(entry)

        entries_out.append(
            (f"{prefix}{connector}{display_name}", purpose, created_at, modified_at)
        )

        if entry.is_dir():
            child_prefix = prefix + ("    " if last_entry else "│   ")
            child_entries = _build_tree_entries(entry, index_path, repo_root, description_map, child_prefix, last_entry)
            entries_out.extend(child_entries)
    return entries_out


def _relative_path(path: Path, repo_root: Path) -> str:
    rel = path.relative_to(repo_root).as_posix()
    if path.is_dir():
        rel += "/"
    return rel


def _load_description_map(repo_root: Path) -> dict:
    try:
        if DESCRIPTION_MAP_PATH.exists():
            data = json.loads(DESCRIPTION_MAP_PATH.read_text(encoding="utf-8"))
            if isinstance(data, dict):
                return data
    except Exception:
        pass
    return {}


def _should_ignore(path: Path, index_path: Path, repo_root: Path) -> bool:
    if path == index_path:
        return True

    rel = path.relative_to(repo_root).as_posix()
    for prefix in IGNORE_PATH_PREFIXES:
        if rel.startswith(prefix):
            return True

    if path.is_dir() and path.name in IGNORE_DIRS:
        return True
    if path.is_file() and path.name in IGNORE_FILES:
        return True
    if path.is_file() and path.suffix.lower() in IGNORE_SUFFIXES:
        return True

    return False


def _sorted_entries(path: Path) -> List[Path]:
    entries = list(path.iterdir())
    dirs = sorted([p for p in entries if p.is_dir()], key=lambda p: p.name.lower())
    files = sorted([p for p in entries if p.is_file()], key=lambda p: p.name.lower())
    return dirs + files


def _get_times(path: Path) -> Tuple[str, str]:
    stat = path.stat()
    created = datetime.fromtimestamp(stat.st_ctime).strftime("%Y-%m-%d %H:%M")
    modified = datetime.fromtimestamp(stat.st_mtime).strftime("%Y-%m-%d %H:%M")
    return created, modified


def _infer_purpose(path: Path, rel_path: str, description_map: dict) -> str:
    if path.is_dir():
        return description_map.get(rel_path, "目录")

    if rel_path in description_map:
        return description_map[rel_path]

    name_upper = path.name.upper()
    if name_upper == "README.MD" or name_upper == "README":
        return "项目入口"
    if name_upper == "LICENSE" or name_upper == "LICENSE.MD":
        return "授权文件"
    if name_upper == "CONTRIBUTING.MD" or name_upper == "CONTRIBUTING":
        return "贡献指南"
    if name_upper == "ROADMAP.MD" or name_upper == "ROADMAP":
        return "发展路线图"

    if "README" in name_upper:
        return "说明文档"
    if "INDEX" in name_upper:
        return "索引"
    if "ROADMAP" in name_upper:
        return "路线图"
    if "GUIDE" in name_upper or "MANUAL" in name_upper:
        return "指南"
    if "CHECKLIST" in name_upper:
        return "清单"
    if "REPORT" in name_upper:
        return "报告"
    if "SUMMARY" in name_upper:
        return "总结"
    if "PLAN" in name_upper:
        return "计划"
    if "ARCH" in name_upper:
        return "架构"
    if "DESIGN" in name_upper:
        return "设计"
    if "SPEC" in name_upper:
        return "规范"
    if "REFERENCE" in name_upper:
        return "参考"
    if "CHANGELOG" in name_upper:
        return "变更记录"

    content_purpose = _extract_content_purpose(path)
    if content_purpose and not _contains_english(content_purpose):
        return content_purpose

    translated = _translate_filename(path.stem)
    if translated:
        return translated

    # Fallback: prefer short Chinese type labels
    suffix = path.suffix.lower()
    if suffix in {".md", ".markdown"}:
        return "文档"
    if suffix in {".json"}:
        return "数据"
    if suffix in {".toml", ".yaml", ".yml"}:
        return "配置"
    if suffix in {".sql"}:
        return "数据库脚本"
    if suffix in {".png", ".jpg", ".jpeg", ".svg"}:
        return "图片"
    if suffix in {".csv"}:
        return "数据表"
    if suffix in {".txt"}:
        return "文本"
    if suffix in {".ps1", ".sh", ".bat"}:
        return "脚本"
    if suffix in {".py"}:
        return "脚本"
    if suffix in {".rs", ".c", ".cpp", ".h", ".hpp"}:
        return "源码"
    if suffix in {".db", ".sqlite"}:
        return "数据库"

    return "文件"


def _translate_filename(stem: str) -> str:
    if not stem:
        return ""

    tokens = re.split(r"[-_\s]+", stem)
    translated: List[str] = []
    for token in tokens:
        if not token:
            continue
        upper = token.upper()
        if upper in TOKEN_MAP:
            translated.append(TOKEN_MAP[upper])
        elif token.isdigit():
            translated.append(token)

    if not translated:
        return ""
    return "".join(translated)


def _contains_english(text: str) -> bool:
    for ch in text:
        if ("a" <= ch <= "z") or ("A" <= ch <= "Z"):
            return True
    return False


def _extract_content_purpose(path: Path) -> str:
    try:
        suffix = path.suffix.lower()
        if suffix not in {".md", ".markdown", ".txt", ".ps1", ".sh", ".py", ".rs", ".toml", ".yaml", ".yml", ".sql", ".bat"}:
            return ""

        content = path.read_text(encoding="utf-8", errors="ignore")
        raw_lines = content.splitlines()[:200]
        lines = [line.rstrip() for line in raw_lines]
        non_empty = [line.strip() for line in lines if line.strip()]
        if not lines:
            return ""

        if suffix in {".md", ".markdown"}:
            for line in non_empty:
                if line.startswith("#"):
                    return line.lstrip("#").strip()[:80]

        if suffix == ".py":
            return _extract_python_docstring(lines)

        if suffix in {".ps1", ".sh"}:
            return _extract_comment_header(lines)

        if suffix == ".bat":
            return _extract_bat_header(lines)

        if suffix == ".sql":
            return _extract_sql_header(lines)

        first_line = _first_meaningful_line(lines)
        return first_line[:80] if first_line else ""
    except Exception:
        return ""


def _first_meaningful_line(lines: List[str]) -> str:
    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("#!"):
            continue
        if stripped.startswith("# -*-"):
            continue
        if stripped in {"'''", '"""'}:
            continue
        return stripped
    return ""


def _strip_comment_prefix(text: str) -> str:
    stripped = text.strip()
    for prefix in ("#", "//", "--", "REM ", "::"):
        if stripped.startswith(prefix):
            return stripped[len(prefix):].strip()
    return stripped


def _extract_python_docstring(lines: List[str]) -> str:
    # Skip shebang/encoding
    idx = 0
    while idx < len(lines):
        line = lines[idx].strip()
        if not line:
            idx += 1
            continue
        if line.startswith("#!") or line.startswith("# -*-"):
            idx += 1
            continue
        break

    if idx < len(lines) and lines[idx].strip().startswith(('"""', "'''")):
        idx += 1
        while idx < len(lines):
            line = lines[idx].strip()
            if line and not line.startswith(('"""', "'''")):
                return line[:80]
            if line.startswith(('"""', "'''")):
                break
            idx += 1

    return _first_meaningful_line(lines)[:80]


def _extract_comment_header(lines: List[str]) -> str:
    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("#!"):
            continue
        if stripped.startswith("#"):
            return _strip_comment_prefix(stripped)[:80]
        break
    return _first_meaningful_line(lines)[:80]


def _extract_bat_header(lines: List[str]) -> str:
    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.upper().startswith("@ECHO"):
            continue
        if stripped.upper().startswith("REM") or stripped.startswith("::"):
            return _strip_comment_prefix(stripped)[:80]
        break
    return _first_meaningful_line(lines)[:80]


def _extract_sql_header(lines: List[str]) -> str:
    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("--"):
            return _strip_comment_prefix(stripped)[:80]
        break
    return _first_meaningful_line(lines)[:80]


def main() -> int:
    repo_root = Path(__file__).parent.parent.parent.parent
    updated = update_docs_index(repo_root)
    if updated:
        print("✅ docs/INDEX.md updated")
    else:
        print("ℹ️  docs/INDEX.md already up to date or docs/ missing")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
