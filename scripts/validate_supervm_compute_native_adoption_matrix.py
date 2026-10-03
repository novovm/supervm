#!/usr/bin/env python3
"""Validate SUPERVM AOEM Compute Native wire_v1 adoption matrix."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOC_JSON = ROOT / "docs" / "AOEM-SUPERVM-COMPUTE-NATIVE-WIRE-V1-ADOPTION-MATRIX.json"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def main() -> int:
    data = json.loads(DOC_JSON.read_text(encoding="utf-8"))

    require(data["matrix_name"] == "supervm_compute_native_wire_v1_adoption_matrix", "bad matrix name")
    require(data["matrix_mode"] == "audit_only", "matrix must be audit_only")
    require(data["runtime_canon"] == "wire_v1_to_semantic_dispatch_to_backend_to_aoem_state_to_state_read_v1", "bad runtime canon")
    require(data["aoem_core_changed"] is False, "AOEM core must not change")
    require(data["aoem_ffi_changed"] is False, "AOEM FFI must not change")
    require(data["supervm_behavior_changed"] is False, "SuperVM behavior must not change")
    require(data["default_path_migrated"] is False, "default path must not migrate in this audit")
    require(data["new_abi_added"] is False, "new ABI must not be added")
    require(data["new_compute_op_added"] is False, "new compute op must not be added")
    require(data["old_entrypoints_deleted"] is False, "old entrypoints must not be deleted")

    rows = data["rows"]
    require(rows, "matrix rows must not be empty")
    require(data["wire_v1_callsite_count"] > 0, "wire_v1 callsites must be found")
    require(data["state_read_v1_callsite_count"] > 0, "state_read_v1 callsites must be found")
    require(data["execute_ops_v2_callsite_count"] > 0, "ops_v2 callsites must be found")
    require(data["execute_primitive_v1_callsite_count"] > 0, "primitive_v1 callsites must be found")
    require(data["production_paths_total"] > 0, "production paths must be classified")

    migration_paths = data["migration_required_paths"]
    for row in migration_paths:
        require(row["migration_required"] is True, "migration row must require migration")
        require(row["keep_as_expert_path_allowed"] is False, "migration path must not be expert keep")

    for row in data["expert_keep_paths"]:
        require(row["keep_as_expert_path_allowed"] is True, "expert keep row must be allowed")
        require(row["migration_required"] is False, "expert keep row must not require migration")

    print(
        "SUPERVM_AOEM_COMPUTE_NATIVE_WIRE_V1_ADOPTION_MATRIX_VALIDATE|"
        f"wire_v1_callsite_count={data['wire_v1_callsite_count']}|"
        f"state_read_v1_callsite_count={data['state_read_v1_callsite_count']}|"
        f"execute_ops_v2_callsite_count={data['execute_ops_v2_callsite_count']}|"
        f"execute_primitive_v1_callsite_count={data['execute_primitive_v1_callsite_count']}|"
        f"runtime_canon_bypass_suspect_count={data['runtime_canon_bypass_suspect_count']}|"
        f"production_paths_total={data['production_paths_total']}|"
        f"production_paths_wire_v1_ready={data['production_paths_wire_v1_ready']}|"
        f"production_paths_requiring_migration={data['production_paths_requiring_migration']}|"
        f"expert_paths_allowed_non_default={data['expert_paths_allowed_non_default']}|"
        "default_path_migrated=false|new_abi_added=false|new_compute_op_added=false|failures=0"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
