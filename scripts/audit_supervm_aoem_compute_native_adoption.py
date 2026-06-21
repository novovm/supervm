#!/usr/bin/env python3
"""Audit SUPERVM AOEM Compute Native wire_v1 adoption.

This script is audit-only. It scans repository callsites and writes a
deterministic markdown + JSON adoption matrix. It does not execute AOEM, change
runtime behavior, or require AOEM/CUDA availability.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[1]
DOC_MD = ROOT / "docs" / "AOEM-SUPERVM-COMPUTE-NATIVE-WIRE-V1-ADOPTION-MATRIX.md"
DOC_JSON = ROOT / "docs" / "AOEM-SUPERVM-COMPUTE-NATIVE-WIRE-V1-ADOPTION-MATRIX.json"

SCAN_EXTS = {
    ".rs",
    ".c",
    ".cpp",
    ".h",
    ".hpp",
    ".md",
    ".json",
}

IGNORE_PARTS = {
    ".git",
    "target",
    ".venv",
    "node_modules",
}

SELF_GENERATED_FILES = {
    "scripts/audit_supervm_aoem_compute_native_adoption.py",
    "scripts/validate_supervm_compute_native_adoption_matrix.py",
    "docs/AOEM-SUPERVM-COMPUTE-NATIVE-WIRE-V1-ADOPTION-MATRIX.md",
    "docs/AOEM-SUPERVM-COMPUTE-NATIVE-WIRE-V1-ADOPTION-MATRIX.json",
}

SYMBOL_PATTERNS = {
    "wire_v1": re.compile(r"\baoem_execute_ops_wire_v1\b|ops_wire_v1"),
    "state_read_v1": re.compile(r"\baoem_state_read_v1\b|state_read_v1"),
    "execute_ops_v2": re.compile(r"\baoem_execute_ops_v2\b"),
    "execute_primitive_v1": re.compile(r"\baoem_execute_primitive_v1\b"),
    "compute_tensor": re.compile(r"compute\.tensor\.[A-Za-z0-9_]+"),
    "compute_primitive": re.compile(r"compute\.primitive\.[A-Za-z0-9_]+"),
}


@dataclass(frozen=True)
class MatrixRow:
    path_module: str
    callsite_file: str
    current_entrypoint: str
    classification: str
    uses_wire_v1: bool
    uses_state_read_v1: bool
    uses_execute_ops_v2: bool
    uses_execute_primitive_v1: bool
    direct_primitive_callsite: bool
    runtime_canon_bypass_suspect: bool
    migration_required: bool
    migration_priority: str
    keep_as_expert_path_allowed: bool
    notes: str


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def iter_scan_files() -> Iterable[Path]:
    for path in ROOT.rglob("*"):
        if not path.is_file() or path.suffix.lower() not in SCAN_EXTS:
            continue
        if rel(path) in SELF_GENERATED_FILES:
            continue
        parts = set(path.relative_to(ROOT).parts)
        if parts & IGNORE_PARTS:
            continue
        yield path


def classify_path(rel_path: str) -> tuple[str, str]:
    lowered = rel_path.lower()
    if "/archive/" in lowered or lowered.startswith("docs_cn/") and "/archive/" in lowered:
        return "archive_doc", "archived historical reference"
    if lowered.startswith("docs/") or lowered.startswith("docs_cn/") or lowered.endswith("readme.md"):
        return "documentation", "documentation or product guidance"
    if "/examples/" in lowered or "/host-integration/" in lowered:
        return "example", "host integration example or smoke sample"
    if "/tests/" in lowered or "test" in lowered or "smoke" in lowered:
        return "test", "test or smoke path"
    if "bench" in lowered or "perf" in lowered or "baseline" in lowered or "profil" in lowered:
        return "expert_perf", "benchmark/performance/diagnostic path"
    if "crates/aoem-bindings/" in lowered:
        return "binding_library", "shared FFI binding layer"
    if "crates/novovm-node/src/bin/" in lowered:
        return "production_default", "node/default product ingress path"
    if "crates/novovm-exec/src/" in lowered:
        return "production_default", "execution support path"
    if lowered.startswith("aoem/worker-adapter/") or lowered.startswith("aoem/manifest/"):
        return "production_default", "packaged AOEM host artifact path"
    if lowered.startswith("aoem/"):
        return "packaged_artifact", "packaged AOEM artifact or sample"
    if lowered.startswith("crates/"):
        return "production_or_library", "runtime crate path requiring owner review"
    return "other", "repository path requiring owner review"


def entrypoint_for(flags: dict[str, bool]) -> str:
    entries: list[str] = []
    if flags["wire_v1"]:
        entries.append("aoem_execute_ops_wire_v1")
    if flags["execute_ops_v2"]:
        entries.append("aoem_execute_ops_v2")
    if flags["execute_primitive_v1"]:
        entries.append("aoem_execute_primitive_v1")
    if flags["state_read_v1"]:
        entries.append("aoem_state_read_v1")
    if flags["compute_tensor"]:
        entries.append("compute.tensor.*")
    if flags["compute_primitive"]:
        entries.append("compute.primitive.*")
    return ", ".join(entries) if entries else "aoem_reference_only"


def build_row(path: Path, text: str) -> MatrixRow | None:
    flags = {name: bool(pattern.search(text)) for name, pattern in SYMBOL_PATTERNS.items()}
    if not any(flags.values()):
        return None

    rel_path = rel(path)
    classification, class_note = classify_path(rel_path)
    uses_wire = flags["wire_v1"]
    uses_state = flags["state_read_v1"]
    uses_ops_v2 = flags["execute_ops_v2"]
    uses_primitive = flags["execute_primitive_v1"]
    direct_primitive = uses_primitive or (flags["compute_primitive"] and not uses_wire)

    expert_allowed = classification in {
        "expert_perf",
        "example",
        "test",
        "documentation",
        "archive_doc",
        "binding_library",
        "packaged_artifact",
    }

    production_like = classification in {
        "production_default",
        "production_or_library",
    }

    bypass = False
    notes: list[str] = [class_note]
    if direct_primitive and not uses_state and production_like:
        bypass = True
        notes.append("direct primitive path without state_read_v1 in production-like path")
    if uses_ops_v2 and not uses_wire and production_like:
        bypass = True
        notes.append("production-like path still references ops_v2 without wire_v1")
    if flags["compute_tensor"] or flags["compute_primitive"]:
        notes.append("compute semantic token present")

    migration_required = production_like and not uses_wire
    if production_like and uses_wire and not uses_state:
        notes.append("wire_v1 present; state_read_v1 adoption should be verified by path owner")

    priority = "none"
    if migration_required:
        priority = "P0" if classification == "production_default" else "P1"
    elif bypass:
        priority = "review"

    return MatrixRow(
        path_module=classification,
        callsite_file=rel_path,
        current_entrypoint=entrypoint_for(flags),
        classification=classification,
        uses_wire_v1=uses_wire,
        uses_state_read_v1=uses_state,
        uses_execute_ops_v2=uses_ops_v2,
        uses_execute_primitive_v1=uses_primitive,
        direct_primitive_callsite=direct_primitive,
        runtime_canon_bypass_suspect=bypass,
        migration_required=migration_required,
        migration_priority=priority,
        keep_as_expert_path_allowed=expert_allowed,
        notes="; ".join(notes),
    )


def audit() -> dict:
    rows: list[MatrixRow] = []
    for path in sorted(iter_scan_files(), key=lambda p: rel(p).lower()):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        row = build_row(path, text)
        if row is not None:
            rows.append(row)

    production_rows = [
        row
        for row in rows
        if row.classification in {"production_default", "production_or_library"}
    ]
    migration_rows = [row for row in rows if row.migration_required]
    expert_rows = [row for row in rows if row.keep_as_expert_path_allowed]
    bypass_rows = [row for row in rows if row.runtime_canon_bypass_suspect]

    return {
        "matrix_name": "supervm_compute_native_wire_v1_adoption_matrix",
        "matrix_mode": "audit_only",
        "runtime_canon": "wire_v1_to_semantic_dispatch_to_backend_to_aoem_state_to_state_read_v1",
        "aoem_core_changed": False,
        "aoem_ffi_changed": False,
        "supervm_behavior_changed": False,
        "default_path_migrated": False,
        "new_abi_added": False,
        "new_compute_op_added": False,
        "old_entrypoints_deleted": False,
        "wire_v1_callsite_count": sum(row.uses_wire_v1 for row in rows),
        "state_read_v1_callsite_count": sum(row.uses_state_read_v1 for row in rows),
        "execute_ops_v2_callsite_count": sum(row.uses_execute_ops_v2 for row in rows),
        "execute_primitive_v1_callsite_count": sum(
            row.uses_execute_primitive_v1 for row in rows
        ),
        "direct_primitive_callsite_count": sum(row.direct_primitive_callsite for row in rows),
        "runtime_canon_bypass_suspect_count": len(bypass_rows),
        "production_paths_total": len(production_rows),
        "production_paths_wire_v1_ready": sum(row.uses_wire_v1 for row in production_rows),
        "production_paths_requiring_migration": len(migration_rows),
        "expert_paths_allowed_non_default": len(expert_rows),
        "example_paths_count": sum(row.classification == "example" for row in rows),
        "test_paths_count": sum(row.classification == "test" for row in rows),
        "debug_paths_count": sum(
            row.classification in {"expert_perf", "documentation", "archive_doc"}
            for row in rows
        ),
        "migration_required_paths": [asdict(row) for row in migration_rows],
        "expert_keep_paths": [asdict(row) for row in expert_rows],
        "suspect_bypass_paths": [asdict(row) for row in bypass_rows],
        "rows": [asdict(row) for row in rows],
    }


def write_markdown(data: dict) -> None:
    lines = [
        "# AOEM / SuperVM Compute Native wire_v1 Adoption Matrix",
        "",
        "Status: audit-only; no runtime behavior changed.",
        "",
        "## Summary",
        "",
        "| Field | Value |",
        "| --- | ---: |",
    ]
    for key in [
        "wire_v1_callsite_count",
        "state_read_v1_callsite_count",
        "execute_ops_v2_callsite_count",
        "execute_primitive_v1_callsite_count",
        "direct_primitive_callsite_count",
        "runtime_canon_bypass_suspect_count",
        "production_paths_total",
        "production_paths_wire_v1_ready",
        "production_paths_requiring_migration",
        "expert_paths_allowed_non_default",
        "example_paths_count",
        "test_paths_count",
        "debug_paths_count",
    ]:
        lines.append(f"| `{key}` | `{data[key]}` |")

    lines.extend(
        [
            "",
            "## Invariants",
            "",
            "| Invariant | Value |",
            "| --- | --- |",
            f"| `matrix_mode` | `{data['matrix_mode']}` |",
            f"| `runtime_canon` | `{data['runtime_canon']}` |",
            f"| `aoem_core_changed` | `{str(data['aoem_core_changed']).lower()}` |",
            f"| `aoem_ffi_changed` | `{str(data['aoem_ffi_changed']).lower()}` |",
            f"| `supervm_behavior_changed` | `{str(data['supervm_behavior_changed']).lower()}` |",
            f"| `default_path_migrated` | `{str(data['default_path_migrated']).lower()}` |",
            f"| `new_abi_added` | `{str(data['new_abi_added']).lower()}` |",
            f"| `new_compute_op_added` | `{str(data['new_compute_op_added']).lower()}` |",
            f"| `old_entrypoints_deleted` | `{str(data['old_entrypoints_deleted']).lower()}` |",
            "",
            "## Migration Required Paths",
            "",
        ]
    )
    lines.extend(render_rows(data["migration_required_paths"]))
    lines.extend(["", "## Expert / Non-default Keep Paths", ""])
    lines.extend(render_rows(data["expert_keep_paths"]))
    lines.extend(["", "## Runtime Canon Bypass Suspects", ""])
    lines.extend(render_rows(data["suspect_bypass_paths"]))
    lines.extend(["", "## Full Matrix", ""])
    lines.extend(render_rows(data["rows"]))
    lines.append("")

    DOC_MD.write_text("\n".join(lines), encoding="utf-8")


def render_rows(rows: list[dict]) -> list[str]:
    if not rows:
        return ["No rows."]
    lines = [
        "| Path/module | File | Entry | Class | wire_v1 | state_read | ops_v2 | primitive_v1 | migration | priority | expert keep | bypass suspect | Notes |",
        "| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | --- |",
    ]
    for row in rows:
        notes = row["notes"].replace("|", "/")
        rendered_row = dict(row)
        rendered_row["notes"] = notes
        lines.append(
            "| {path_module} | `{callsite_file}` | `{current_entrypoint}` | `{classification}` | "
            "`{uses_wire_v1}` | `{uses_state_read_v1}` | `{uses_execute_ops_v2}` | "
            "`{uses_execute_primitive_v1}` | `{migration_required}` | `{migration_priority}` | "
            "`{keep_as_expert_path_allowed}` | `{runtime_canon_bypass_suspect}` | {notes} |".format(**rendered_row)
        )
    return lines


def main() -> int:
    data = audit()
    DOC_JSON.parent.mkdir(parents=True, exist_ok=True)
    DOC_JSON.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    write_markdown(data)
    print(
        "SUPERVM_AOEM_COMPUTE_NATIVE_WIRE_V1_ADOPTION_MATRIX|"
        f"wire_v1_callsite_count={data['wire_v1_callsite_count']}|"
        f"state_read_v1_callsite_count={data['state_read_v1_callsite_count']}|"
        f"execute_ops_v2_callsite_count={data['execute_ops_v2_callsite_count']}|"
        f"execute_primitive_v1_callsite_count={data['execute_primitive_v1_callsite_count']}|"
        f"runtime_canon_bypass_suspect_count={data['runtime_canon_bypass_suspect_count']}|"
        f"production_paths_total={data['production_paths_total']}|"
        f"production_paths_wire_v1_ready={data['production_paths_wire_v1_ready']}|"
        f"production_paths_requiring_migration={data['production_paths_requiring_migration']}|"
        f"expert_paths_allowed_non_default={data['expert_paths_allowed_non_default']}|"
        "aoem_core_changed=false|aoem_ffi_changed=false|"
        "supervm_behavior_changed=false|new_abi_added=false|new_compute_op_added=false|failures=0"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
