import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
from datetime import datetime, timezone


def source_identity(root):
    paths = subprocess.check_output(
        ["git", "ls-files", "-co", "--exclude-standard", "-z"], cwd=root
    ).split(b"\0")
    digest = hashlib.sha256()
    for raw in sorted(set(paths) - {b""}):
        path = root / os.fsdecode(raw)
        digest.update(raw + b"\0")
        if path.is_symlink():
            digest.update(b"link\0" + os.fsencode(os.readlink(path)))
        elif path.is_file():
            digest.update(str(path.stat().st_mode & 0o111).encode() + b"\0")
            with path.open("rb") as source:
                while chunk := source.read(1024 * 1024):
                    digest.update(chunk)
        elif not path.exists():
            digest.update(b"deleted\0")
        else:
            raise RuntimeError(f"Unsupported source entry: {path}")
        digest.update(b"\0")
    return {
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root).decode().strip(),
        "worktree_clean": not subprocess.check_output(["git", "status", "--porcelain"], cwd=root),
        "source_sha256": digest.hexdigest(),
    }


def test_executable(log, target):
    executables = set()
    for line in log.read_text().splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (
            item.get("reason") == "compiler-artifact"
            and item.get("target", {}).get("name") == target
            and item.get("profile", {}).get("test") is True
            and item.get("executable")
        ):
            executables.add(item["executable"])
    if len(executables) != 1:
        raise RuntimeError(f"Expected one freshly verified test binary for {target}")
    return executables.pop()


def isolated(command):
    return [
        "unshare", "--user", "--map-root-user", "--net", "--",
        "sh", "-c", 'ip link set lo up && exec "$@"', "local-readiness", *command,
    ]


def isolated_storage(command, parent_mount_namespace):
    if not re.fullmatch(r"mnt:\[\d+\]", parent_mount_namespace):
        raise RuntimeError("Expected parent mount namespace identity")
    return [
        "unshare", "--user", "--map-root-user", "--net", "--mount", "--propagation", "private", "--",
        "sh", "-c", 'ip link set lo up && exec "$@"', "local-storage-readiness",
        "env", f"NOVOVM_TEST_STORAGE_PARENT_MOUNT_NS={parent_mount_namespace}", *command,
    ]


def rust_test_result(log):
    text = log.read_text(errors="replace")
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", text)
    if len(results) != 1 or int(results[0][0]) == 0 or int(results[0][1]) != 0:
        raise RuntimeError(f"Missing successful, nonempty Rust test result: {log}")
    return dict(zip(("passed", "failed", "ignored"), map(int, results[0])))


def run_step(root, output, name, command, report, timeout=1800, rust_tests=False):
    log = output / f"{name}.log"
    started = time.monotonic()
    step = {"name": name, "command": command, "log": log.name, "accepted": False}
    report["steps"].append(step)
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(("NOVOVM_", "AOEM_", "ETH_")):
            del environment[key]
    environment["TMPDIR"] = str(output / "tmp")
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    with log.open("wb") as stream:
        process = subprocess.Popen(
            command, cwd=root, env=environment, stdout=stream, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        try:
            step["exit_code"] = process.wait(timeout=timeout)
        except BaseException:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise
        finally:
            step["elapsed_seconds"] = round(time.monotonic() - started, 3)
    if step["exit_code"] != 0:
        raise RuntimeError(f"Gate failed: {name}; see {log}")
    if rust_tests:
        step["test_result"] = rust_test_result(log)
    step["accepted"] = True
    print(f"PASS {name} ({step['elapsed_seconds']}s)", flush=True)
    return log


def main():
    parser = argparse.ArgumentParser(description="Linux isolated local acceptance; not mainnet approval")
    parser.add_argument("--output-dir", help="New directory under repository artifacts/")
    args = parser.parse_args()
    root = Path(subprocess.check_output(
        ["git", "-C", str(Path(__file__).resolve().parent), "rev-parse", "--show-toplevel"]
    ).decode().strip()).resolve()
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output = (root / (args.output_dir or f"artifacts/local-readiness-{stamp}")).resolve()
    if not output.is_relative_to((root / "artifacts").resolve()) or output.exists():
        parser.error("Output must be a new directory inside this repository's artifacts/")
    output.mkdir(parents=True)
    (output / "tmp").mkdir()
    report = {
        "schema": "novovm-local-readiness/v1", "generated_at_utc": stamp,
        "accepted": False, "production_ready": False, "multi_machine_tested": False,
        "physical_power_loss_tested": False, "steps": [],
        "remaining_gates": [
            "extended candidate-less failover and partition churn beyond bounded local cases",
            "long-duration soak, AOEM/seal in-flight writes, later ledger checkpoint faults and physical power loss",
            "public RPC gateway",
            "clean release package", "real multi-machine regression", "approved genesis and operations",
        ],
    }
    try:
        if sys.platform != "linux":
            raise RuntimeError("This runner requires Linux network namespaces; do not run on host ports")
        report["source_before"] = source_identity(root)
        run_step(root, output, "namespace", isolated(["true"]), report, 15)
        parent_mount = os.readlink("/proc/self/ns/mnt")
        run_step(root, output, "storage-namespace", isolated_storage(["true"], parent_mount), report, 15)
        run_step(root, output, "runner-tests", [sys.executable, "scripts/tests/test_local_readiness.py"], report)
        run_step(root, output, "fmt", ["cargo", "fmt", "--all", "--check"], report)
        run_step(root, output, "clippy", [
            "cargo", "clippy", "-q", "-p", "novovm-node", "--lib", "--bin", "novovm-node",
            "--test", "native_candidate_node_cli", "--", "-D", "warnings",
        ], report)
        worker_log = run_step(root, output, "build-main-partition-worker", [
            "cargo", "test", "-p", "novovm-node", "--bin", "novovm-node",
            "--no-run", "--message-format=json",
        ], report)
        worker = test_executable(worker_log, "novovm-node")
        report["main_partition_test_worker"] = {
            "path": worker, "sha256": hashlib.sha256(Path(worker).read_bytes()).hexdigest(),
            "instrumentation": "cfg_test_main_entry_authenticated_ingress_drop_only",
            "production_binary": False,
        }
        run_step(root, output, "partition-controller", isolated([
            worker, "--exact",
            "partition_test::controller_rejects_invalid_control_and_filters_only_allowlisted_frames",
            "--test-threads=1",
        ]), report, rust_tests=True)
        for package, selection, target in [
            ("novovm-node", ["--lib"], "novovm_node"),
            ("novovm-network", ["--lib"], "novovm_network"),
            ("novovm-node", ["--test", "native_candidate_node_cli"], "native_candidate_node_cli"),
        ]:
            log = run_step(root, output, f"build-{target}", [
                "cargo", "test", "-p", package, *selection, "--no-run", "--message-format=json",
            ], report)
            executable = test_executable(log, target)
            report.setdefault("test_binaries", {})[target] = {
                "path": executable, "sha256": hashlib.sha256(Path(executable).read_bytes()).hexdigest(),
            }
            run_step(root, output, target, isolated([executable, "--test-threads=1"]), report,
                     3600, rust_tests=True)
            if target == "native_candidate_node_cli":
                run_step(root, output, "continuous-recovery", isolated([
                    executable, "fresh_genesis_main_nodes_continue_three_heights_without_restart",
                    "--ignored", "--test-threads=1",
                ]), report, 1800, rust_tests=True)
                run_step(root, output, "candidate-less-failover", isolated([
                    executable, "fresh_genesis_main_nodes_replace_candidate_less_offline_leader",
                    "--ignored", "--nocapture", "--test-threads=1",
                ]), report, 1800, rust_tests=True)
                run_step(root, output, "prepared-timeout-partition", isolated([
                    "env", f"NOVOVM_TEST_MAIN_PARTITION_BINARY={worker}", executable,
                    "fresh_genesis_main_nodes_heal_prepared_timeout_partition",
                    "--ignored", "--nocapture", "--test-threads=1",
                ]), report, 1800, rust_tests=True)
                run_step(root, output, "transaction-pool-storage-faults", isolated_storage([
                    executable, "fresh_genesis_main_nodes_recover_transaction_pool_storage_faults",
                    "--ignored", "--nocapture", "--test-threads=1",
                ], parent_mount), report, 1800, rust_tests=True)
                run_step(root, output, "database-storage-startup-faults", isolated_storage([
                    executable, "fresh_genesis_main_nodes_recover_storage_startup_faults",
                    "--ignored", "--nocapture", "--test-threads=1",
                ], parent_mount), report, 1800, rust_tests=True)
        report["source_after"] = source_identity(root)
        if report["source_before"] != report["source_after"]:
            raise RuntimeError("Source changed during acceptance; rerun against a stable worktree")
        report["accepted"] = True
    except Exception as error:
        report["error"] = str(error)
    finally:
        try:
            report["source_after"] = source_identity(root)
            if report.get("source_before") != report["source_after"]:
                report["accepted"] = False
                report["source_changed_during_run"] = True
        except Exception as error:
            report["accepted"] = False
            report["source_verification_error"] = str(error)
        (output / "acceptance.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        print(f"Report: {output / 'acceptance.json'}", flush=True)
    return 0 if report["accepted"] else 1


if __name__ == "__main__":
    sys.exit(main())
