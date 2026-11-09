"""Main source sync is lightweight; candidate/PR/scheduled gates stay explicit.

No crate dependency graph is guessed here. Renames include both old and new
paths; missing history and unknown events still select full checks.
"""

import ast
import json
import os
from pathlib import Path
import subprocess
import tomllib
import zipfile


DAILY_SECURITY = "17 3 * * *"
# Exact files only. Do not replace with docs/** or *.md: some documentation is
# compiled with include_str! or consumed by deployment/boundary checks.
PROSE_ONLY = frozenset({
    "AGENTS.md",
    "docs/NOVOVM_DELIVERY_ALIGNMENT.md",
    "docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md",
    "docs/NOVONET_UCA_PQ_IMPLEMENTATION_HANDOFF_2026-10-24.md",
    "docs/NOVONET_UCA_TIERED_SPENDING_AUTHORITY_V1.md",
    "docs/NOVONET_KEY_ONE_MOBILE_HANDOFF_V1.md",
    "docs/NOVOVM_FIRST_RELEASE_SIGNATURE_CONTRACT.md",
    "docs/NOVOVM_CI_CHECK_TIERS.md",
})


def git(*args):
    return subprocess.check_output(["git", *args], stderr=subprocess.PIPE)


def change_range(event_name, event):
    if event_name == "push":
        base, head = event.get("before"), event.get("after")
    elif event_name == "pull_request":
        pr = event["pull_request"]
        base, head = pr["base"]["sha"], pr["head"]["sha"]
    else:
        raise ValueError("event has no supported diff")
    if not base or not head or set(base) == {"0"}:
        raise ValueError("missing base/head history")
    # Verify SHAs before passing them as revisions; never interpret an option.
    for revision in (base, head):
        if len(revision) != 40 or any(c not in "0123456789abcdef" for c in revision):
            raise ValueError("invalid revision")
    if event_name == "pull_request":
        base = git("merge-base", base, head).decode().strip()
    return base, head


def changed_paths(event_name, event):
    base, head = change_range(event_name, event)
    return [p.decode("utf-8") for p in git(
        "diff", "--no-renames", "--name-only", "-z", base, head, "--"
    ).split(b"\0") if p]


def select_scope(event_name, event, paths=None):
    if event_name == "schedule":
        return "security" if event.get("schedule") == DAILY_SECURITY else "full"
    if event_name == "workflow_dispatch":
        profile = event.get("inputs", {}).get("profile")
        return profile if profile in ("security", "recovery") else "full"
    if event_name == "push" and event.get("ref") == "refs/heads/main" and paths:
        return "sync"
    if event_name == "pull_request" and paths and all(
        p in PROSE_ONLY for p in paths
    ):
        return "docs"
    return "full"


def check_sync(event_name, event, paths):
    """No Cargo build, service invocation, extraction or network access."""
    base, head = change_range(event_name, event)
    git("diff", "--check", base, head, "--")
    checked = []
    for name in paths:
        path = Path(name)
        if not path.is_file() or path.is_symlink():
            continue  # Deletions and symlinks are still covered by the Git diff.
        if name.startswith("scripts/") and path.suffix == ".py":
            ast.parse(path.read_bytes(), filename=name)
            checked.append(name)
        elif path.name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "deny.toml"):
            tomllib.loads(path.read_text(encoding="utf-8-sig"))
            checked.append(name)
        elif name.startswith("artifacts/audit/") and path.suffix == ".json":
            json.loads(path.read_text(encoding="utf-8-sig"))
            checked.append(name)
        elif name.startswith("artifacts/audit/") and path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                if archive.testzip() is not None:
                    raise ValueError(f"Corrupt evidence ZIP: {name}")
            checked.append(name)
    # Syntax/CRC checks do not establish the truth of a report or artifact hashes.
    print(json.dumps({"sync_checked": checked, "product_tests_executed": False}))


def outputs_for(scope, tracked):
    if scope not in ("sync", "docs", "security", "recovery", "full"):
        raise ValueError("unknown CI scope")
    if "Cargo.toml" not in tracked:
        raise ValueError("required original product manifest is missing")
    product = [p for p in tracked if not p.startswith(("legacy/", "runtime/"))]
    manifests = sorted(p for p in product if p.endswith("/Cargo.toml"))
    manifest = "Cargo.toml" if "Cargo.toml" in product else next(iter(manifests), "")
    packages = sorted(p for p in product if p.endswith("/package.json"))
    package = "package.json" if "package.json" in product else next(iter(packages), "")
    full = scope == "full"
    return {
        "scope": scope,
        "full": str(full).lower(),
        "has_rust": str(full and bool(manifest)).lower(),
        "rust_manifest": manifest,
        "security": str(scope in ("full", "security") and bool(manifest)).lower(),
        "recovery": str(scope in ("full", "recovery") and bool(manifest)).lower(),
        "has_python": str(full and any(p.endswith(".py") for p in product)).lower(),
        "has_node": str(full and bool(package)).lower(),
        "node_package_json": package,
    }


def validate_results(needs):
    if needs["detect"]["result"] != "success":
        raise ValueError("CI routing did not succeed")
    out = needs["detect"]["outputs"]
    scope = out["scope"]
    if scope not in ("full", "security", "recovery", "docs", "sync"):
        raise ValueError("unknown CI scope")
    required = {
        "full": ("true", "true", "true", "true"),
        "security": ("false", "false", "true", "false"),
        "recovery": ("false", "false", "false", "true"),
        "docs": ("false", "false", "false", "false"),
        "sync": ("false", "false", "false", "false"),
    }[scope]
    if (out["full"], out["has_rust"], out["security"], out["recovery"]) != required:
        raise ValueError("scope contradicts required product/security checks")
    if scope != "full" and (out["has_python"] != "false" or out["has_node"] != "false"):
        raise ValueError("non-full scope unexpectedly selects product checks")
    selected = {
        "lan_script_launch": out["full"],
        "runtime_modules": out["full"],
        "runtime_modules_release": out["full"],
        "native_transport_qualification": out["full"],
        "native_auth_relation": out["full"],
        "rust": out["has_rust"],
        "native_recovery": out["recovery"],
        "rust_security": out["security"],
        "python": out["has_python"],
        "node": out["has_node"],
    }
    for job, enabled in selected.items():
        if enabled not in ("true", "false"):
            raise ValueError(f"invalid selection: {job}")
        expected = "success" if enabled == "true" else "skipped"
        actual = needs.get(job, {}).get("result")
        if actual != expected:
            raise ValueError(f"{job}: expected {expected}, got {actual}")
    return scope


def main():
    event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    paths = None
    event = {}
    try:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
        if event_name in ("push", "pull_request"):
            paths = changed_paths(event_name, event)
    except (KeyError, ValueError, TypeError, AttributeError, OSError, subprocess.CalledProcessError):
        print("Change scope unavailable; selecting full checks.")
        # Malformed dispatch/schedule payload must not accidentally select less.
        event_name, event = "unknown", {}
    scope = select_scope(event_name, event, paths)
    tracked = [p.decode("utf-8") for p in git("ls-files", "-z").split(b"\0") if p]
    outputs = outputs_for(scope, tracked)
    if scope in ("sync", "docs"):
        check_sync(event_name, event, paths)
    if scope == "docs":
        for name in paths:
            path = Path(name)
            if path.exists():  # Deleted prose is also a legitimate docs change.
                content = path.read_text(encoding="utf-8-sig")
                if "\0" in content:
                    raise ValueError("NUL in reviewed prose")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as handle:
        for key, value in outputs.items():
            if "\n" in value or "\r" in value:
                raise ValueError("invalid output value")
            handle.write(f"{key}={value}\n")
    with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as handle:
        handle.write(f"## CI scope: {scope}\n\n")
        handle.write("Source synchronization only; product/platform/security tests NOT EXECUTED. Not release qualification.\n"
                     if scope == "sync" else
                     "Reviewed prose only; product tests NOT EXECUTED. Not release qualification.\n"
                     if scope == "docs" else
                     "Security only; product tests NOT EXECUTED.\n" if scope == "security" else
                     "Current NovoRUDP recovery only; other product/platform/security gates NOT EXECUTED. Not release qualification.\n"
                     if scope == "recovery" else
                     "All required product, current native recovery and platform gates selected. Retired WSS mainchain is not release evidence.\n")
    print(json.dumps(outputs, sort_keys=True))


if __name__ == "__main__":
    import sys
    if sys.argv[1:] == ["--check-results"]:
        scope = validate_results(json.loads(os.environ["CI_NEEDS"]))
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as handle:
            handle.write(f"## Passed selected scope: {scope}\n\n")
            handle.write("Sync/docs/security/recovery-only success is NOT full product or release qualification.\n")
    else:
        main()
